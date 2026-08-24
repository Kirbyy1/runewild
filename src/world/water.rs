//! Lightweight cellular water flow for loaded terrain.
//!
//! Player-placed water blocks are stable sources. Generated rivers, lakes and
//! oceans already occupy terrain-carved basins and remain static. Flowing cells
//! fall vertically first, then spread a bounded number of blocks across
//! supported terrain. Runtime flow is deliberately transient: only authored
//! player edits belong in the world save.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use crate::world::{
    chunk::WaterShape,
    chunk_manager::ChunkManager,
    coordinates::{ChunkCoord, VoxelCoord},
    voxel::BlockType,
};

const FLOW_INTERVAL_SECONDS: f32 = 0.10;
const FLOW_UPDATES_PER_TICK: usize = 128;
const MAX_HORIZONTAL_LEVEL: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlowCell {
    level: u8,
    source: bool,
    falling: bool,
}

impl FlowCell {
    const SOURCE: Self = Self {
        level: 0,
        source: true,
        falling: false,
    };

    fn visual(self) -> WaterShape {
        WaterShape {
            level: self.level,
            falling: self.falling,
        }
    }
}

#[derive(Resource)]
pub struct WaterSimulation {
    timer: Timer,
    cells: HashMap<VoxelCoord, FlowCell>,
    pending: VecDeque<VoxelCoord>,
    queued: HashSet<VoxelCoord>,
}

impl Default for WaterSimulation {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(FLOW_INTERVAL_SECONDS, TimerMode::Repeating),
            cells: HashMap::new(),
            pending: VecDeque::new(),
            queued: HashSet::new(),
        }
    }
}

impl WaterSimulation {
    pub fn add_source(&mut self, coord: VoxelCoord) {
        self.cells.insert(coord, FlowCell::SOURCE);
        self.wake(coord);
    }

    pub fn add_sources(&mut self, sources: impl IntoIterator<Item = VoxelCoord>) {
        for source in sources {
            self.add_source(source);
        }
    }

    pub fn remove_source(&mut self, coord: VoxelCoord) {
        if let Some(cell) = self.cells.get_mut(&coord) {
            cell.source = false;
        }
        self.wake_with_neighbors(coord);
    }

    pub fn notify_block_changed(&mut self, coord: VoxelCoord) {
        self.wake_with_neighbors(coord);
    }

    pub fn remove_column(&mut self, column: ChunkCoord) {
        self.cells.retain(|coord, _| coord.chunk() != column);
        self.pending.retain(|coord| coord.chunk() != column);
        self.queued.retain(|coord| coord.chunk() != column);
    }

    pub fn notify_column_loaded(&mut self, column: ChunkCoord) {
        let boundary_cells: Vec<_> = self
            .cells
            .keys()
            .copied()
            .filter(|coord| {
                horizontal_neighbors(*coord)
                    .into_iter()
                    .any(|neighbor| neighbor.chunk() == column)
            })
            .collect();
        for coord in boundary_cells {
            self.wake(coord);
        }
    }

    fn wake(&mut self, coord: VoxelCoord) {
        if self.queued.insert(coord) {
            self.pending.push_back(coord);
        }
    }

    fn wake_with_neighbors(&mut self, coord: VoxelCoord) {
        self.wake(coord);
        for neighbor in flow_neighbors(coord) {
            self.wake(neighbor);
        }
    }

    fn insert_flow(&mut self, world: &mut ChunkManager, coord: VoxelCoord, cell: FlowCell) {
        if !world.is_voxel_loaded(coord) {
            return;
        }

        match world.block_at(coord) {
            BlockType::Air => {
                if world.set_simulated_block(coord, BlockType::Water) {
                    self.cells.insert(coord, cell);
                    world.set_simulated_water_shape(coord, cell.visual());
                    self.wake(coord);
                }
            }
            BlockType::Water => {
                if let Some(existing) = self.cells.get(&coord) {
                    if !existing.source && cell.level < existing.level {
                        self.cells.insert(coord, cell);
                        world.set_simulated_water_shape(coord, cell.visual());
                        self.wake(coord);
                    }
                }
            }
            _ => {}
        }
    }

    fn has_inflow(&self, world: &ChunkManager, coord: VoxelCoord, cell: FlowCell) -> bool {
        let above = coord.offset(0, 1, 0);
        if world.block_at(above) == BlockType::Water && self.cells.contains_key(&above) {
            return true;
        }

        horizontal_neighbors(coord).into_iter().any(|neighbor| {
            self.cells.get(&neighbor).is_some_and(|parent| {
                world.block_at(neighbor) == BlockType::Water
                    && (parent.source || parent.level < cell.level)
            })
        })
    }

    fn process_cell(&mut self, world: &mut ChunkManager, coord: VoxelCoord) {
        let Some(cell) = self.cells.get(&coord).copied() else {
            return;
        };
        if !world.is_voxel_loaded(coord) {
            return;
        }
        if world.block_at(coord) != BlockType::Water {
            self.cells.remove(&coord);
            self.wake_with_neighbors(coord);
            return;
        }
        world.set_simulated_water_shape(coord, cell.visual());

        if !cell.source && !self.has_inflow(world, coord, cell) {
            self.cells.remove(&coord);
            if world.set_simulated_block(coord, BlockType::Air) {
                self.wake_with_neighbors(coord);
            }
            return;
        }

        let below = coord.offset(0, -1, 0);
        let below_block = world.block_at(below);
        if world.is_voxel_loaded(below) && below_block == BlockType::Air {
            self.insert_flow(
                world,
                below,
                FlowCell {
                    level: cell.level,
                    source: false,
                    falling: true,
                },
            );
            return;
        }

        // A vertical falling strand must not fan out from every level after
        // the strand below it fills. Only its bottom cell may become a
        // horizontal flow when it actually lands on solid terrain.
        if cell.falling {
            if below_block == BlockType::Water {
                return;
            }
            if let Some(landed) = self.cells.get_mut(&coord) {
                landed.falling = false;
            }
            world.set_simulated_water_shape(
                coord,
                WaterShape {
                    level: cell.level,
                    falling: false,
                },
            );
        }

        if cell.level >= MAX_HORIZONTAL_LEVEL {
            return;
        }
        let next_level = cell.level + 1;
        for target in rotated_horizontal_neighbors(coord) {
            self.insert_flow(
                world,
                target,
                FlowCell {
                    level: next_level,
                    source: false,
                    falling: false,
                },
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn step_cells(&mut self, world: &mut ChunkManager, budget: usize) {
        self.run_budget(world, budget);
    }

    fn run_budget(&mut self, world: &mut ChunkManager, budget: usize) {
        for _ in 0..budget {
            let Some(coord) = self.pending.pop_front() else {
                break;
            };
            self.queued.remove(&coord);
            self.process_cell(world, coord);
        }
    }
}

pub fn update_water_physics(
    time: Res<Time>,
    mut water: ResMut<WaterSimulation>,
    mut world: ResMut<ChunkManager>,
) {
    water.timer.tick(time.delta());
    if water.timer.just_finished() {
        water.run_budget(&mut world, FLOW_UPDATES_PER_TICK);
    }
}

fn horizontal_neighbors(coord: VoxelCoord) -> [VoxelCoord; 4] {
    [
        coord.offset(1, 0, 0),
        coord.offset(-1, 0, 0),
        coord.offset(0, 0, 1),
        coord.offset(0, 0, -1),
    ]
}

fn rotated_horizontal_neighbors(coord: VoxelCoord) -> [VoxelCoord; 4] {
    let neighbors = horizontal_neighbors(coord);
    let rotation = (coord.x.wrapping_mul(31) ^ coord.y.wrapping_mul(17) ^ coord.z.wrapping_mul(13))
        .unsigned_abs() as usize
        % neighbors.len();
    std::array::from_fn(|index| neighbors[(index + rotation) % neighbors.len()])
}

fn flow_neighbors(coord: VoxelCoord) -> [VoxelCoord; 6] {
    [
        coord.offset(0, 1, 0),
        coord.offset(0, -1, 0),
        coord.offset(1, 0, 0),
        coord.offset(-1, 0, 0),
        coord.offset(0, 0, 1),
        coord.offset(0, 0, -1),
    ]
}
