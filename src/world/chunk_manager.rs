//! Chunk column loading with **sparse vertical sections**.
//!
//! A column (`ChunkCoord`) materialises only the 32-block-high sections that
//! actually contain content: the stone band from the configured world bottom up to the
//! surface, ocean/river water, vegetation and player edits. Sections can be
//! negative (below Y = 0) and there is no gameplay height ceiling - the only
//! vertical bounds are the storage envelope in `crate::world`.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::{
    game::WORLD_SEED,
    player::Player,
    settings::WorldSettings,
    world::{
        chunk::{Chunk, WaterShape},
        coordinates::{floor_div, ChunkCoord, SectionCoord, VoxelCoord},
        generation::terrain::TerrainGenerator,
        meshing::greedy::build_chunk_mesh_with_water,
        persistence::SaveGame,
        voxel::BlockType,
        water::WaterSimulation,
        {
            profile_world_enabled, CHUNK_SIZE, SECTION_MAX_Y, SECTION_MIN_Y, VOXEL_SIZE_M,
            WORLD_BOTTOM_VOXEL_Y,
        },
    },
};

const COLUMN_GENERATION_BUDGET: usize = 1;
const CHUNK_MESH_BUDGET: usize = 3;

#[derive(Debug, Default, Clone, Copy, Resource)]
pub struct WorldStats {
    pub loaded_chunks: usize,
    pub generated_chunks: usize,
    pub visible_chunks: usize,
    pub triangles: usize,
    pub seed: u64,
    pub render_distance_m: f32,
}

#[derive(Debug)]
pub struct ChunkEntities {
    pub opaque: Option<Entity>,
    pub transparent: Option<Entity>,
    pub water: Option<Entity>,
    pub triangles: usize,
}

#[derive(Resource)]
pub struct ChunkManager {
    /// Sparse map of loaded sections; key includes the vertical index.
    chunks: HashMap<SectionCoord, Chunk>,
    entities: HashMap<SectionCoord, ChunkEntities>,
    /// Fully generated columns (a column may own zero or more sections).
    columns: HashSet<ChunkCoord>,
    generator: TerrainGenerator,
    generated_chunks: usize,
    /// Authored water edits restored while generating columns. Procedural
    /// water is intentionally excluded because its basin is already stable.
    pending_water_sources: Vec<VoxelCoord>,
}

impl Default for ChunkManager {
    fn default() -> Self {
        Self {
            chunks: HashMap::new(),
            entities: HashMap::new(),
            columns: HashSet::new(),
            generator: TerrainGenerator::new(WORLD_SEED),
            generated_chunks: 0,
            pending_water_sources: Vec::new(),
        }
    }
}

impl ChunkManager {
    #[allow(dead_code)]
    pub fn generator(&self) -> &TerrainGenerator {
        &self.generator
    }

    pub fn column_at(&self, x: i32, z: i32) -> crate::world::generation::terrain::TerrainColumn {
        self.generator.column_at(x, z)
    }

    pub fn block_at(&self, coord: VoxelCoord) -> BlockType {
        let section = coord.section();
        if let Some(chunk) = self.chunks.get(&section) {
            return chunk.get_local(coord.section_local());
        }
        // Generated columns materialise every section from the terrain bottom
        // through their highest content/edit. A missing section in such a
        // column is therefore known air (or the solid void below the bottom),
        // and does not need another procedural noise evaluation.
        if self.columns.contains(&section.column()) {
            return if coord.y < WORLD_BOTTOM_VOXEL_Y {
                BlockType::Stone
            } else {
                BlockType::Air
            };
        }
        self.generator.generated_block(coord)
    }

    pub fn solid_at(&self, coord: VoxelCoord) -> bool {
        self.block_at(coord).is_solid()
    }

    pub(crate) fn water_shape_at(&self, coord: VoxelCoord) -> Option<WaterShape> {
        self.chunks
            .get(&coord.section())
            .and_then(|chunk| chunk.water_shape_local(coord.section_local()))
    }

    /// Generates every content-bearing section of one column.
    ///
    /// All loops operate on terrain voxels; generator heights are metres,
    /// converted via `VOXELS_PER_METER`. Only
    /// sections that end up containing voxels are stored.
    fn generate_column(&mut self, column: ChunkCoord, save: &SaveGame) {
        let t0 = std::time::Instant::now();
        let gen = &self.generator;
        let bx = column.x * CHUNK_SIZE;
        let bz = column.z * CHUNK_SIZE;
        // One sampling pass for the whole 32x32 microcolumn footprint.
        let grid = gen.sample_column_grid(column);
        let col_at = |lx: usize, lz: usize| &grid[lz * CHUNK_SIZE as usize + lx];

        // 1. Highest content voxel in the column.
        let mut min_bottom = WORLD_BOTTOM_VOXEL_Y;
        let mut max_top = WORLD_BOTTOM_VOXEL_Y;
        for lz in 0..CHUNK_SIZE as usize {
            for lx in 0..CHUNK_SIZE as usize {
                let col = col_at(lx, lz);
                let surface_top = col.surface_voxel_y();
                let top = col
                    .water_top_voxel_y()
                    .unwrap_or(surface_top)
                    .max(surface_top);
                max_top = max_top.max(top);
            }
        }

        // 2. Vegetation and player edits can reach higher.
        let vegetation = gen.vegetation_for_chunk(column);
        for v in &vegetation {
            max_top = max_top.max(v.coord.y);
        }
        for saved in save.blocks_in_column(column) {
            min_bottom = min_bottom.min(saved.coord.y);
            max_top = max_top.max(saved.coord.y);
        }

        // 3. Materialise only sections that carry content (sparse storage).
        let sy0 = floor_div(min_bottom, CHUNK_SIZE).max(SECTION_MIN_Y);
        let sy1 = floor_div(max_top, CHUNK_SIZE).clamp(SECTION_MIN_Y, SECTION_MAX_Y);

        let mut fresh: Vec<Chunk> = (sy0..=sy1).map(|sy| Chunk::new(column, sy)).collect();
        let base_index = |sy: i32| (sy - sy0) as usize;
        let slice_top = sy1 * CHUNK_SIZE + CHUNK_SIZE - 1;

        // Terrain fill (voxel space).
        for lz in 0..CHUNK_SIZE as usize {
            for lx in 0..CHUNK_SIZE as usize {
                let wx = bx + lx as i32;
                let wz = bz + lz as i32;
                let col = col_at(lx, lz);
                let solid_top = col.surface_voxel_y().min(slice_top);
                for wy in WORLD_BOTTOM_VOXEL_Y..=solid_top {
                    let block = gen.generated_block_in_column(
                        VoxelCoord {
                            x: wx,
                            y: wy,
                            z: wz,
                        },
                        *col,
                    );
                    if block == BlockType::Air {
                        continue; // caves (when enabled)
                    }
                    let chunk = &mut fresh[base_index(floor_div(wy, CHUNK_SIZE))];
                    chunk.set_local(
                        VoxelCoord {
                            x: wx,
                            y: wy,
                            z: wz,
                        }
                        .section_local(),
                        block,
                    );
                }
                if let Some(water_top) = col
                    .water_top_voxel_y()
                    .filter(|top| *top > col.surface_voxel_y())
                {
                    for wy in (col.surface_voxel_y() + 1)..=water_top.min(slice_top) {
                        let frozen = col.biome.is_frozen() && wy == water_top;
                        let chunk = &mut fresh[base_index(floor_div(wy, CHUNK_SIZE))];
                        chunk.set_local(
                            VoxelCoord {
                                x: wx,
                                y: wy,
                                z: wz,
                            }
                            .section_local(),
                            if frozen {
                                BlockType::Ice
                            } else {
                                BlockType::Water
                            },
                        );
                    }
                }
            }
        }

        // Vegetation overlay (never overwrites terrain trunks with leaves).
        for v in vegetation {
            let sy = floor_div(v.coord.y, CHUNK_SIZE);
            if !(sy0..=sy1).contains(&sy) {
                continue;
            }
            let chunk = &mut fresh[base_index(sy)];
            let existing = chunk.get_local(v.coord.section_local());
            let replaceable = matches!(
                existing,
                BlockType::Air
                    | BlockType::Leaves
                    | BlockType::PineLeaves
                    | BlockType::JungleLeaves
                    | BlockType::AutumnLeaves
                    | BlockType::PalmLeaves
                    | BlockType::Snow
                    | BlockType::Water
            );
            if replaceable {
                chunk.set_local(v.coord.section_local(), v.block);
            }
        }

        // Player edits are authoritative and address one visible voxel each.
        for saved in save.blocks_in_column(column) {
            let p = saved.coord;
            let sy = floor_div(p.y, CHUNK_SIZE);
            if !(SECTION_MIN_Y..=SECTION_MAX_Y).contains(&sy) {
                continue;
            }
            while fresh.last().is_none_or(|c| c.section_y < sy) {
                let next = fresh.last().map(|c| c.section_y + 1).unwrap_or(sy);
                fresh.push(Chunk::new(column, next));
            }
            if let Some(chunk) = fresh.iter_mut().find(|c| c.section_y == sy) {
                chunk.set_local(p.section_local(), saved.block);
                if saved.block == BlockType::Water {
                    self.pending_water_sources.push(p);
                }
            }
        }

        let new_keys: Vec<_> = fresh.iter().map(Chunk::key).collect();
        let section_count = new_keys.len();
        for chunk in fresh {
            self.chunks.insert(chunk.key(), chunk);
        }
        for key in new_keys {
            for adjacent in section_neighbors(key) {
                if let Some(chunk) = self.chunks.get_mut(&adjacent) {
                    chunk.dirty = true;
                }
            }
        }
        self.columns.insert(column);
        self.generated_chunks += 1;

        if profile_world_enabled() {
            info!(
                "column {:?} generated in {:.2} ms ({} sections)",
                column,
                t0.elapsed().as_secs_f64() * 1000.0,
                section_count,
            );
        }
    }

    /// Sets one visible terrain voxel and invalidates its section plus any
    /// loaded section that shares the changed boundary face.
    pub fn set_block(&mut self, coord: VoxelCoord, block: BlockType, save: &mut SaveGame) -> bool {
        if self.write_block(coord, block) {
            self.mark_voxel_and_neighbors_dirty(coord);
            let restores_generated = self.generator.generated_block(coord) == block;
            save.set_modified_block(coord, block, restores_generated);
            true
        } else {
            false
        }
    }

    fn write_block(&mut self, coord: VoxelCoord, block: BlockType) -> bool {
        let section = coord.section();
        if !(SECTION_MIN_Y..=SECTION_MAX_Y).contains(&section.y) {
            return false;
        }
        if self.block_at(coord) == block {
            return false;
        }
        if !self.chunks.contains_key(&section) {
            let column = section.column();
            if !self.columns.contains(&column) {
                return false;
            }
            self.chunks.insert(section, Chunk::new(column, section.y));
        }
        let Some(chunk) = self.chunks.get_mut(&section) else {
            return false;
        };
        if !chunk.set_world(coord, block) {
            return false;
        }
        true
    }

    pub(crate) fn is_voxel_loaded(&self, coord: VoxelCoord) -> bool {
        self.columns.contains(&coord.chunk())
            && (SECTION_MIN_Y..=SECTION_MAX_Y).contains(&coord.section().y)
    }

    pub(crate) fn set_simulated_block(&mut self, coord: VoxelCoord, block: BlockType) -> bool {
        if !self.is_voxel_loaded(coord) || !self.write_block(coord, block) {
            return false;
        }
        self.mark_voxel_and_neighbors_dirty(coord);
        true
    }

    pub(crate) fn set_simulated_water_shape(
        &mut self,
        coord: VoxelCoord,
        shape: WaterShape,
    ) -> bool {
        self.chunks
            .get_mut(&coord.section())
            .is_some_and(|chunk| chunk.set_water_shape_local(coord.section_local(), shape))
    }

    fn take_pending_water_sources(&mut self) -> Vec<VoxelCoord> {
        std::mem::take(&mut self.pending_water_sources)
    }

    fn mark_voxel_and_neighbors_dirty(&mut self, coord: VoxelCoord) {
        let affected: HashSet<_> = voxel_and_face_neighbors(coord)
            .into_iter()
            .map(VoxelCoord::section)
            .collect();
        for section in affected {
            if let Some(chunk) = self.chunks.get_mut(&section) {
                chunk.dirty = true;
            }
        }
    }
}

fn voxel_and_face_neighbors(coord: VoxelCoord) -> [VoxelCoord; 7] {
    [
        coord,
        coord.offset(1, 0, 0),
        coord.offset(-1, 0, 0),
        coord.offset(0, 1, 0),
        coord.offset(0, -1, 0),
        coord.offset(0, 0, 1),
        coord.offset(0, 0, -1),
    ]
}

fn section_neighbors(section: SectionCoord) -> [SectionCoord; 6] {
    [
        SectionCoord {
            x: section.x + 1,
            ..section
        },
        SectionCoord {
            x: section.x - 1,
            ..section
        },
        SectionCoord {
            y: section.y + 1,
            ..section
        },
        SectionCoord {
            y: section.y - 1,
            ..section
        },
        SectionCoord {
            z: section.z + 1,
            ..section
        },
        SectionCoord {
            z: section.z - 1,
            ..section
        },
    ]
}

pub fn update_loaded_chunks(
    mut commands: Commands,
    mut manager: ResMut<ChunkManager>,
    mut water: ResMut<WaterSimulation>,
    save: Res<SaveGame>,
    settings: Res<WorldSettings>,
    player: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = player.get_single() else {
        return;
    };
    let player_chunk = VoxelCoord::from_world_pos(player_transform.translation).chunk();
    let mut wanted = HashSet::new();

    let load_radius = settings.render_distance_sections();
    for dz in -load_radius..=load_radius {
        for dx in -load_radius..=load_radius {
            if dx * dx + dz * dz <= load_radius * load_radius {
                wanted.insert(ChunkCoord {
                    x: player_chunk.x + dx,
                    z: player_chunk.z + dz,
                });
            }
        }
    }

    let mut missing: Vec<_> = wanted
        .iter()
        .copied()
        .filter(|coord| !manager.columns.contains(coord))
        .collect();
    missing.sort_unstable_by_key(|coord| {
        let dx = coord.x - player_chunk.x;
        let dz = coord.z - player_chunk.z;
        dx * dx + dz * dz
    });

    for coord in missing.into_iter().take(COLUMN_GENERATION_BUDGET) {
        manager.generate_column(coord, &save);
        water.add_sources(manager.take_pending_water_sources());
        water.notify_column_loaded(coord);
    }

    // Unload whole columns outside the radius.
    let unload: Vec<_> = manager
        .columns
        .iter()
        .copied()
        .filter(|coord| {
            let dx = coord.x - player_chunk.x;
            let dz = coord.z - player_chunk.z;
            let unload_radius = settings.unload_distance_sections();
            dx * dx + dz * dz > unload_radius * unload_radius
        })
        .collect();

    for column in unload {
        water.remove_column(column);
        manager.columns.remove(&column);
        let stale: Vec<_> = manager
            .chunks
            .keys()
            .copied()
            .filter(|sec| sec.column() == column)
            .collect();
        for key in stale {
            manager.chunks.remove(&key);
            if let Some(entities) = manager.entities.remove(&key) {
                for entity in [entities.opaque, entities.transparent, entities.water]
                    .into_iter()
                    .flatten()
                {
                    commands.entity(entity).despawn_recursive();
                }
            }
        }
    }
}

pub fn update_chunk_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut manager: ResMut<ChunkManager>,
    atlas_config: Res<crate::rendering::texture_atlas::TextureAtlasConfig>,
    settings: Res<WorldSettings>,
    player: Query<&Transform, With<Player>>,
    mut stats: Local<WorldStats>,
) {
    let (Some(opaque_material), Some(foliage_material), Some(water_material)) = (
        atlas_config.opaque_material.as_ref(),
        atlas_config.foliage_material.as_ref(),
        atlas_config.water_material.as_ref(),
    ) else {
        return;
    };

    let player_section = player
        .get_single()
        .map(|t| VoxelCoord::from_world_pos(t.translation).section())
        .unwrap_or_default();
    let mut dirty: Vec<_> = manager
        .chunks
        .iter()
        .filter_map(|(key, chunk)| {
            (chunk.dirty
                && column_ready_for_meshing(
                    &manager.columns,
                    player_section.column(),
                    key.column(),
                    settings.render_distance_sections(),
                ))
            .then_some(*key)
        })
        .collect();
    dirty.sort_unstable_by_key(|key| {
        let dx = key.x - player_section.x;
        let dz = key.z - player_section.z;
        let dy = (key.y - player_section.y) * 2;
        dx * dx + dz * dz + dy * dy
    });
    dirty.truncate(CHUNK_MESH_BUDGET);

    for key in dirty {
        let Some(chunk) = manager.chunks.get(&key).cloned() else {
            continue;
        };
        let t_mesh = std::time::Instant::now();
        let mut shell_cache = HashMap::new();
        let data = build_chunk_mesh_with_water(
            &chunk,
            |voxel| {
                *shell_cache
                    .entry(voxel)
                    .or_insert_with(|| manager.block_at(voxel))
            },
            |voxel| manager.water_shape_at(voxel),
        );
        let vertices =
            data.positions.len() + data.transparent_positions.len() + data.water_positions.len();
        let (opaque_mesh, transparent_mesh, water_mesh, triangles) = data.into_meshes();
        if profile_world_enabled() && triangles > 0 {
            info!(
                "section {:?} meshed in {:.2} ms ({} vertices, {} tris)",
                key,
                t_mesh.elapsed().as_secs_f64() * 1000.0,
                vertices,
                triangles
            );
        }

        if let Some(old) = manager.entities.remove(&key) {
            for entity in [old.opaque, old.transparent, old.water]
                .into_iter()
                .flatten()
            {
                commands.entity(entity).despawn_recursive();
            }
        }

        // Mesh vertices are in voxel units; scale converts to metres.
        let voxel_scale = Transform::from_scale(Vec3::splat(VOXEL_SIZE_M));

        let opaque = opaque_mesh.map(|mesh| {
            commands
                .spawn(PbrBundle {
                    mesh: meshes.add(mesh),
                    material: opaque_material.clone(),
                    transform: voxel_scale,
                    ..default()
                })
                .id()
        });

        let transparent = transparent_mesh.map(|mesh| {
            commands
                .spawn(PbrBundle {
                    mesh: meshes.add(mesh),
                    material: foliage_material.clone(),
                    transform: voxel_scale,
                    ..default()
                })
                .id()
        });

        let water = water_mesh.map(|mesh| {
            commands
                .spawn(PbrBundle {
                    mesh: meshes.add(mesh),
                    material: water_material.clone(),
                    transform: voxel_scale,
                    ..default()
                })
                .id()
        });

        manager.entities.insert(
            key,
            ChunkEntities {
                opaque,
                transparent,
                water,
                triangles,
            },
        );
        if let Some(chunk) = manager.chunks.get_mut(&key) {
            chunk.dirty = false;
        }
    }

    *stats = WorldStats {
        loaded_chunks: manager.chunks.len(),
        generated_chunks: manager.generated_chunks,
        visible_chunks: manager.entities.len(),
        triangles: manager
            .entities
            .values()
            .map(|entity| entity.triangles)
            .sum(),
        seed: manager.generator.seed(),
        render_distance_m: settings.render_distance_m(),
    };
    commands.insert_resource(*stats);
}

fn column_ready_for_meshing(
    loaded: &HashSet<ChunkCoord>,
    player: ChunkCoord,
    column: ChunkCoord,
    load_radius: i32,
) -> bool {
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neighbor = ChunkCoord {
            x: column.x + dx,
            z: column.z + dz,
        };
        let px = neighbor.x - player.x;
        let pz = neighbor.z - player.z;
        let neighbor_is_wanted = px * px + pz * pz <= load_radius * load_radius;
        if neighbor_is_wanted && !loaded.contains(&neighbor) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DEFAULT_RENDER_DISTANCE_SECTIONS;
    use crate::world::{generation::terrain::SEA_LEVEL, water::WaterSimulation, VOXELS_PER_METER};

    fn water_test_world() -> ChunkManager {
        let mut manager = ChunkManager::default();
        let column = ChunkCoord { x: 0, z: 0 };
        let section = SectionCoord { x: 0, y: 0, z: 0 };
        manager.columns.insert(column);
        manager.chunks.insert(section, Chunk::new(column, 0));
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert!(manager.write_block(VoxelCoord::new(x, 0, z), BlockType::Stone));
            }
        }
        manager
    }

    #[test]
    fn section_keys_are_stable_and_negative() {
        let voxel = VoxelCoord {
            x: -40,
            y: -50,
            z: 70,
        };
        let sec = voxel.section();
        assert_eq!(sec, SectionCoord { x: -2, y: -2, z: 2 });
        assert_eq!(sec.column(), ChunkCoord { x: -2, z: 2 });
    }

    #[test]
    fn simulated_water_falls_before_spreading_and_has_bounded_reach() {
        let mut manager = water_test_world();
        let mut water = WaterSimulation::default();
        let source = VoxelCoord::new(16, 5, 16);
        assert!(manager.set_simulated_block(source, BlockType::Water));
        water.add_source(source);
        water.step_cells(&mut manager, 20_000);

        for y in 1..=5 {
            assert_eq!(
                manager.block_at(VoxelCoord::new(16, y, 16)),
                BlockType::Water,
                "falling water disconnected at y={y}"
            );
        }
        for y in 2..5 {
            assert_eq!(
                manager.block_at(VoxelCoord::new(17, y, 16)),
                BlockType::Air,
                "falling water spread sideways before reaching terrain at y={y}"
            );
        }
        assert_eq!(
            manager.block_at(VoxelCoord::new(22, 1, 16)),
            BlockType::Water
        );
        assert_eq!(
            manager.block_at(VoxelCoord::new(23, 1, 16)),
            BlockType::Air,
            "horizontal water exceeded its six-block flow range"
        );
    }

    #[test]
    fn simulated_water_drains_after_its_source_is_removed() {
        let mut manager = water_test_world();
        let mut water = WaterSimulation::default();
        let source = VoxelCoord::new(16, 1, 16);
        assert!(manager.set_simulated_block(source, BlockType::Water));
        water.add_source(source);
        water.step_cells(&mut manager, 20_000);
        assert_eq!(manager.block_at(source.offset(4, 0, 0)), BlockType::Water);

        assert!(manager.set_simulated_block(source, BlockType::Air));
        water.remove_source(source);
        for _ in 0..8 {
            water.step_cells(&mut manager, 20_000);
        }

        assert_eq!(manager.block_at(source.offset(1, 0, 0)), BlockType::Air);
        assert_eq!(manager.block_at(source.offset(4, 0, 0)), BlockType::Air);
    }

    #[test]
    fn simulated_flow_never_drains_preexisting_procedural_water() {
        let mut manager = water_test_world();
        let mut water = WaterSimulation::default();
        let source = VoxelCoord::new(16, 1, 16);
        let static_water = source.offset(1, 0, 0);
        assert!(manager.set_simulated_block(source, BlockType::Water));
        assert!(manager.set_simulated_block(static_water, BlockType::Water));
        water.add_source(source);
        water.step_cells(&mut manager, 20_000);

        assert!(manager.set_simulated_block(source, BlockType::Air));
        water.remove_source(source);
        for _ in 0..8 {
            water.step_cells(&mut manager, 20_000);
        }

        assert_eq!(manager.block_at(static_water), BlockType::Water);
    }

    #[test]
    fn simulated_water_resumes_across_a_newly_loaded_column_boundary() {
        let mut manager = water_test_world();
        let mut water = WaterSimulation::default();
        let source = VoxelCoord::new(CHUNK_SIZE - 1, 1, 16);
        assert!(manager.set_simulated_block(source, BlockType::Water));
        water.add_source(source);
        water.step_cells(&mut manager, 20_000);

        let right = ChunkCoord { x: 1, z: 0 };
        manager.columns.insert(right);
        manager
            .chunks
            .insert(SectionCoord { x: 1, y: 0, z: 0 }, Chunk::new(right, 0));
        for z in 0..CHUNK_SIZE {
            for x in CHUNK_SIZE..CHUNK_SIZE * 2 {
                assert!(manager.write_block(VoxelCoord::new(x, 0, z), BlockType::Stone));
            }
        }
        water.notify_column_loaded(right);
        water.step_cells(&mut manager, 20_000);

        assert_eq!(
            manager.block_at(VoxelCoord::new(CHUNK_SIZE, 1, 16)),
            BlockType::Water
        );
    }

    #[test]
    fn building_and_collision_work_above_the_old_height_limit() {
        let mut manager = ChunkManager::default();
        let save = SaveGame::load_or_default(WORLD_SEED);
        let column = ChunkCoord { x: 0, z: 0 };

        manager.generate_column(column, &save);
        assert!(
            !manager.chunks.is_empty(),
            "column must materialise at least one section"
        );

        // Pick a spot well above the old single-section ceiling (y < 32).
        let surface = manager.column_at(5, 5).height;
        let y = surface.max(SEA_LEVEL) + 45;
        assert!(y > 47, "test expects to build far above y=32");
        let voxel = VoxelCoord::new(
            5 * VOXELS_PER_METER,
            y * VOXELS_PER_METER,
            5 * VOXELS_PER_METER,
        );

        let mut save = save;
        assert!(
            manager.set_block(voxel, BlockType::Stone, &mut save),
            "placement above the old ceiling failed"
        );
        assert_eq!(manager.block_at(voxel), BlockType::Stone);
        assert!(manager.solid_at(voxel), "collision lookup failed");
        assert_ne!(
            manager.block_at(voxel.offset(1, 0, 0)),
            BlockType::Stone,
            "a single placement must not fill neighboring voxels"
        );
        assert!(
            save.blocks_in_column(column).any(|s| s.coord == voxel),
            "edit above the old ceiling was not recorded"
        );

        // Deep underground stays solid as well.
        assert!(manager.solid_at(VoxelCoord {
            x: 5,
            y: WORLD_BOTTOM_VOXEL_Y - 10,
            z: 5
        }));
    }

    #[test]
    fn voxel_edge_edit_dirties_loaded_neighbor_once() {
        let mut manager = ChunkManager::default();
        let mut save = SaveGame::load_or_default(WORLD_SEED);
        let left = ChunkCoord { x: 0, z: 0 };
        let right = ChunkCoord { x: 1, z: 0 };
        manager.generate_column(left, &save);
        manager.generate_column(right, &save);
        for chunk in manager.chunks.values_mut() {
            chunk.dirty = false;
        }

        let edge_x = CHUNK_SIZE - 1;
        let surface_y = manager
            .generator()
            .column_at_voxel(edge_x, 0)
            .surface_voxel_y();
        let edit = VoxelCoord::new(edge_x, surface_y, 0);
        assert!(manager.set_block(edit, BlockType::Air, &mut save));
        let section_y = floor_div(edit.y, CHUNK_SIZE);
        assert!(manager
            .chunks
            .get(&SectionCoord {
                x: 0,
                y: section_y,
                z: 0
            })
            .is_some_and(|chunk| chunk.dirty));
        assert!(manager
            .chunks
            .get(&SectionCoord {
                x: 1,
                y: section_y,
                z: 0
            })
            .is_some_and(|chunk| chunk.dirty));
    }

    #[test]
    fn meshing_waits_for_wanted_neighbors_but_allows_load_frontier() {
        let player = ChunkCoord { x: 0, z: 0 };
        let center = ChunkCoord { x: 0, z: 0 };
        let radius = DEFAULT_RENDER_DISTANCE_SECTIONS;
        let mut loaded = HashSet::from([center]);
        assert!(!column_ready_for_meshing(&loaded, player, center, radius));
        for neighbor in [
            ChunkCoord { x: 1, z: 0 },
            ChunkCoord { x: -1, z: 0 },
            ChunkCoord { x: 0, z: 1 },
            ChunkCoord { x: 0, z: -1 },
        ] {
            loaded.insert(neighbor);
        }
        assert!(column_ready_for_meshing(&loaded, player, center, radius));

        let frontier = ChunkCoord { x: radius, z: 0 };
        loaded.insert(frontier);
        loaded.insert(ChunkCoord {
            x: radius - 1,
            z: 0,
        });
        loaded.insert(ChunkCoord { x: radius, z: 1 });
        loaded.insert(ChunkCoord { x: radius, z: -1 });
        assert!(column_ready_for_meshing(&loaded, player, frontier, radius));
    }
}
