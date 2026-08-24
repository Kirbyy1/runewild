//! Disk persistence for the world: seed, player position and modified blocks.
//!
//! Only deviations from procedural generation ("modified blocks") are stored,
//! keeping the save file small even after long play sessions. The JSON layout
//! matches `save/runewild_world.json`:
//!
//! ```json
//! { "seed": 0, "player_position": [0.0, 0.0, 0.0], "modified_blocks": [] }
//! ```

use std::{collections::BTreeMap, fs, path::PathBuf};

use bevy::{prelude::*, window::WindowCloseRequested};
use serde::{Deserialize, Serialize};

use crate::{
    player::Player,
    world::{coordinates::VoxelCoord, voxel::BlockType, VOXELS_PER_METER},
};

const SAVE_FILE: &str = "save/runewild_world.json";
const AUTOSAVE_INTERVAL_SECS: f32 = 30.0;
const SAVE_FORMAT_VERSION: u32 = 3;
const QUARTER_METRE_FORMAT_VERSION: u32 = 2;
const QUARTER_METRE_VOXELS_PER_METRE: i32 = 4;

/// A single visible voxel deviation from the generated terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedBlock {
    pub coord: VoxelCoord,
    pub block: BlockType,
}

/// Serde mirror of the on-disk document.
#[derive(Debug, Serialize, Deserialize)]
struct SaveData {
    #[serde(default)]
    format_version: u32,
    seed: u64,
    player_position: [f32; 3],
    modified_blocks: Vec<SavedBlock>,
}

/// Resource holding everything that persists between sessions.
#[derive(Debug, Clone, Resource)]
pub struct SaveGame {
    seed: u64,
    /// Last known player position; refreshed every frame and written on save.
    pub player_position: Vec3,
    modified_blocks: Vec<SavedBlock>,
}

impl SaveGame {
    /// Loads `save/runewild_world.json` when present and valid; otherwise a
    /// fresh save for `seed` is created.
    pub fn load_or_default(seed: u64) -> Self {
        match Self::load_from_disk() {
            Some(save) => {
                if save.seed != seed {
                    warn!(
                        "Save seed {} differs from world seed {}; loading it anyway",
                        save.seed, seed
                    );
                }
                info!(
                    "Loaded world save with {} modified block(s)",
                    save.modified_blocks.len()
                );
                save
            }
            None => Self {
                seed,
                player_position: Vec3::ZERO,
                modified_blocks: Vec::new(),
            },
        }
    }

    fn load_from_disk() -> Option<Self> {
        let text = fs::read_to_string(save_file_path()).ok()?;
        let data: SaveData = serde_json::from_str(&text).ok()?;
        let [x, y, z] = data.player_position;
        let modified_blocks = match data.format_version {
            version if version < QUARTER_METRE_FORMAT_VERSION => {
                let old_count = data.modified_blocks.len();
                let migrated = migrate_logical_edits(data.modified_blocks);
                info!(
                    "Migrated {old_count} legacy logical edit(s) into {} terrain block edit(s)",
                    migrated.len()
                );
                migrated
            }
            QUARTER_METRE_FORMAT_VERSION => {
                let old_count = data.modified_blocks.len();
                let migrated = migrate_quarter_metre_edits(data.modified_blocks);
                info!(
                    "Collapsed {old_count} quarter-metre edit(s) into {} one-metre block edit(s)",
                    migrated.len()
                );
                migrated
            }
            _ => data.modified_blocks,
        };
        Some(Self {
            seed: data.seed,
            player_position: Vec3::new(x, y, z),
            modified_blocks,
        })
    }

    /// Records one visible-voxel edit. Edits that restore procedural terrain
    /// are removed so the file only keeps modifications.
    pub fn set_modified_block(
        &mut self,
        coord: VoxelCoord,
        block: BlockType,
        restores_generated: bool,
    ) {
        if restores_generated {
            self.modified_blocks.retain(|saved| saved.coord != coord);
            return;
        }
        if let Some(saved) = self
            .modified_blocks
            .iter_mut()
            .find(|saved| saved.coord == coord)
        {
            saved.block = block;
        } else {
            self.modified_blocks.push(SavedBlock { coord, block });
        }
    }

    /// Iterates the saved edits belonging to one chunk column.
    pub fn blocks_in_column(
        &self,
        column: crate::world::coordinates::ChunkCoord,
    ) -> impl Iterator<Item = &SavedBlock> + '_ {
        self.modified_blocks
            .iter()
            .filter(move |saved| saved.coord.chunk() == column)
    }

    /// Serializes the save to disk, creating the folder when missing.
    pub fn write_to_disk(&self) {
        let data = SaveData {
            format_version: SAVE_FORMAT_VERSION,
            seed: self.seed,
            player_position: self.player_position.to_array(),
            modified_blocks: self.modified_blocks.clone(),
        };
        let json = match serde_json::to_string_pretty(&data) {
            Ok(json) => json,
            Err(err) => {
                error!("Failed to serialize world save: {err}");
                return;
            }
        };
        let path = save_file_path();
        let result = path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .and_then(|_| fs::write(&path, json));
        if let Err(err) = result {
            error!("Failed to write world save: {err}");
        }
    }
}

fn save_file_path() -> PathBuf {
    std::env::var_os("RUNEWILD_SAVE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SAVE_FILE))
}

/// Saves written before version 2 used one-metre logical coordinates. Expand
/// those edits once on load so old worlds keep the same physical changes.
fn migrate_logical_edits(blocks: Vec<SavedBlock>) -> Vec<SavedBlock> {
    let mut migrated = Vec::with_capacity(blocks.len() * VOXELS_PER_METER.pow(3) as usize);
    for saved in blocks {
        let origin = VoxelCoord::new(
            saved.coord.x * VOXELS_PER_METER,
            saved.coord.y * VOXELS_PER_METER,
            saved.coord.z * VOXELS_PER_METER,
        );
        for dy in 0..VOXELS_PER_METER {
            for dz in 0..VOXELS_PER_METER {
                for dx in 0..VOXELS_PER_METER {
                    migrated.push(SavedBlock {
                        coord: origin.offset(dx, dy, dz),
                        block: saved.block,
                    });
                }
            }
        }
    }
    migrated
}

/// Saves from format 2 addressed quarter-metre terrain voxels. Collapse each
/// edit into the containing one-metre block. Later entries deliberately take
/// precedence when several old voxels occupy the same new block.
fn migrate_quarter_metre_edits(blocks: Vec<SavedBlock>) -> Vec<SavedBlock> {
    let mut collapsed = BTreeMap::new();
    for saved in blocks {
        let coord = VoxelCoord::new(
            saved.coord.x.div_euclid(QUARTER_METRE_VOXELS_PER_METRE),
            saved.coord.y.div_euclid(QUARTER_METRE_VOXELS_PER_METRE),
            saved.coord.z.div_euclid(QUARTER_METRE_VOXELS_PER_METRE),
        );
        collapsed.insert((coord.x, coord.y, coord.z), saved.block);
    }
    collapsed
        .into_iter()
        .map(|((x, y, z), block)| SavedBlock {
            coord: VoxelCoord::new(x, y, z),
            block,
        })
        .collect()
}

/// Moves the player onto the persisted position once it has been spawned.
pub fn apply_loaded_player_position(
    mut player: Query<&mut Transform, With<Player>>,
    save: Res<SaveGame>,
) {
    if let Ok(mut transform) = player.get_single_mut() {
        transform.translation = save.player_position;
    }
}

/// Keeps `player_position` fresh and periodically writes the save to disk.
pub(crate) fn autosave_world(
    time: Res<Time>,
    mut state: Local<AutosaveState>,
    mut save: ResMut<SaveGame>,
    player: Query<&Transform, With<Player>>,
) {
    if let Ok(transform) = player.get_single() {
        save.player_position = transform.translation;
    }
    state.timer.tick(time.delta());
    if state.timer.just_finished() {
        save.write_to_disk();
        debug!("World autosaved");
    }
}

/// Final write when the app exits through [`AppExit`] (e.g. menu quit).
pub fn persist_on_exit(mut exit: EventReader<AppExit>, save: ResMut<SaveGame>) {
    if !exit.is_empty() {
        exit.clear();
        save.write_to_disk();
        info!("World saved on exit");
    }
}

/// Safety net for closing the window directly via its close button.
pub fn persist_on_window_close(
    mut closings: EventReader<WindowCloseRequested>,
    save: ResMut<SaveGame>,
) {
    if !closings.is_empty() {
        closings.clear();
        save.write_to_disk();
        info!("World saved on window close");
    }
}

pub(crate) struct AutosaveState {
    timer: Timer,
}

impl Default for AutosaveState {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(AUTOSAVE_INTERVAL_SECS, TimerMode::Repeating),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::WORLD_SEED;

    const SAMPLE: &str = r#"{
        "seed": 5932734143854627908,
        "player_position": [-57.155186, 23.984503, 829.32874],
        "modified_blocks": [
            {"coord": {"x": 171, "y": 23, "z": 30}, "block": "Air"},
            {"coord": {"x": -115, "y": 16, "z": 118}, "block": "Grass"}
        ]
    }"#;

    #[test]
    fn deserializes_on_disk_schema() {
        let data: SaveData = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(data.format_version, 0);
        assert_eq!(data.seed, 5932734143854627908);
        assert_eq!(data.player_position, [-57.155186, 23.984503, 829.32874]);
        assert_eq!(data.modified_blocks.len(), 2);
        assert_eq!(data.modified_blocks[0].coord.x, 171);
        assert_eq!(data.modified_blocks[0].block, BlockType::Air);
        assert_eq!(data.modified_blocks[1].block, BlockType::Grass);

        let serialized = serde_json::to_string(&data).unwrap();
        for key in [
            "\"seed\"",
            "\"player_position\"",
            "\"modified_blocks\"",
            "\"coord\"",
            "\"block\"",
        ] {
            assert!(serialized.contains(key), "missing key {key}");
        }
    }

    #[test]
    fn set_modified_block_upserts_and_restores() {
        let coord = VoxelCoord::new(5, 17, -9);
        let mut save = SaveGame {
            seed: WORLD_SEED,
            player_position: Vec3::ZERO,
            modified_blocks: Vec::new(),
        };

        save.set_modified_block(coord, BlockType::Stone, false);
        save.set_modified_block(coord, BlockType::Dirt, false);
        assert_eq!(save.modified_blocks.len(), 1);
        assert_eq!(save.modified_blocks[0].block, BlockType::Dirt);

        // Restoring the generated block removes the entry entirely.
        save.set_modified_block(coord, BlockType::Grass, true);
        assert!(save.modified_blocks.is_empty());
    }

    #[test]
    fn blocks_in_column_filters_by_column() {
        let inside = VoxelCoord::new(1, 20, 1);
        let outside = VoxelCoord::new(32, 20, 32);
        let above_limit = VoxelCoord::new(2, 384, 2);

        let mut save = SaveGame {
            seed: 7,
            player_position: Vec3::ZERO,
            modified_blocks: Vec::new(),
        };
        save.set_modified_block(inside, BlockType::Stone, false);
        save.set_modified_block(outside, BlockType::Stone, false);
        save.set_modified_block(above_limit, BlockType::Stone, false);

        let column = crate::world::coordinates::ChunkCoord { x: 0, z: 0 };
        let found: Vec<_> = save.blocks_in_column(column).collect();
        assert_eq!(
            found.len(),
            2,
            "column (0,0) holds `inside` and `above_limit`"
        );
        // Edits far above the old height ceiling persist fine.
        assert!(found.iter().any(|s| s.coord.y == 384));
    }

    #[test]
    fn voxel_save_coordinates_map_to_negative_columns() {
        let mut save = SaveGame {
            seed: 7,
            player_position: Vec3::ZERO,
            modified_blocks: Vec::new(),
        };
        let in_column = VoxelCoord::new(-1, 4, -1);
        let next_column = VoxelCoord::new(-33, 4, -33);
        save.set_modified_block(in_column, BlockType::Air, false);
        save.set_modified_block(next_column, BlockType::Air, false);

        let found: Vec<_> = save
            .blocks_in_column(crate::world::coordinates::ChunkCoord { x: -1, z: -1 })
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].coord, in_column);
    }

    #[test]
    fn legacy_logical_edit_maps_to_exact_one_metre_block() {
        let migrated = migrate_logical_edits(vec![SavedBlock {
            coord: VoxelCoord::new(-1, 2, 3),
            block: BlockType::Stone,
        }]);
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].coord, VoxelCoord::new(-1, 2, 3));
    }

    #[test]
    fn quarter_metre_edits_collapse_with_negative_floor_semantics() {
        let migrated = migrate_quarter_metre_edits(vec![
            SavedBlock {
                coord: VoxelCoord::new(3, 3, 3),
                block: BlockType::Stone,
            },
            SavedBlock {
                coord: VoxelCoord::new(-1, -4, -5),
                block: BlockType::Dirt,
            },
            SavedBlock {
                coord: VoxelCoord::new(1, 2, 3),
                block: BlockType::Air,
            },
        ]);

        assert_eq!(migrated.len(), 2);
        assert_eq!(migrated[0].coord, VoxelCoord::new(-1, -1, -2));
        assert_eq!(migrated[0].block, BlockType::Dirt);
        assert_eq!(migrated[1].coord, VoxelCoord::new(0, 0, 0));
        assert_eq!(migrated[1].block, BlockType::Air);
    }
}
