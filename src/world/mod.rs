pub mod chunk;
pub mod chunk_manager;
pub mod coordinates;
pub mod generation;
pub mod lighting;
pub mod meshing;
pub mod persistence;
pub mod voxel;
pub mod water;

use bevy::prelude::*;
use std::sync::OnceLock;

use crate::player::controller::setup_player;

use self::chunk_manager::{update_chunk_meshes, update_loaded_chunks, ChunkManager};
use self::generation::diagnostics::export_debug_maps_on_startup;
use self::persistence::{
    apply_loaded_player_position, autosave_world, persist_on_exit, persist_on_window_close,
};
use self::water::{update_water_physics, WaterSimulation};

/// Edge length (in editable terrain blocks) of one chunk section.
pub const CHUNK_SIZE: i32 = 32;
pub const CHUNK_SIZE_USIZE: usize = CHUNK_SIZE as usize;

/// Editable terrain blocks per metre. Keep this at one for Minecraft-scale
/// terrain: one visible/editable voxel is one metre on every axis.
pub const VOXELS_PER_METER: i32 = 1;
/// Physical size of one terrain voxel, in metres.
pub const VOXEL_SIZE_M: f32 = 1.0 / VOXELS_PER_METER as f32;

/// 3D cave carving remains gated until its generation pass is ready for the
/// regional terrain model.
pub const ENABLE_CAVES: bool = false;
/// Rocks, fallen logs and bushes are authored in one-metre art units, which
/// map 1:1 onto the terrain grid at `VOXELS_PER_METER = 1`.
pub const ENABLE_GROUND_DECOR: bool = true;

/// Compile-time default for terrain/meshing profiling. Runtime profiling can
/// be enabled without rebuilding with `PROFILE_WORLD=1`.
pub const PROFILE_WORLD: bool = false;

pub fn profile_world_enabled() -> bool {
    static ENV_ENABLED: OnceLock<bool> = OnceLock::new();
    PROFILE_WORLD
        || *ENV_ENABLED.get_or_init(|| {
            std::env::var("PROFILE_WORLD")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false)
        })
}

/// Lowest terrain height in metres. Not a build limit; it only bounds
/// procedurally materialised terrain memory.
pub const WORLD_BOTTOM_Y: i32 = -24;
pub const WORLD_BOTTOM_VOXEL_Y: i32 = WORLD_BOTTOM_Y * VOXELS_PER_METER;

/// Safety envelope for sparse section storage: section indices outside this
/// range are refused when generating or editing. At the default resolution
/// this is a physical Y envelope of [-64 m, 512 m).
pub const SECTION_MIN_Y: i32 = -8;
pub const SECTION_MAX_Y: i32 = 63;

pub const SECTION_SIZE_M: f32 = CHUNK_SIZE as f32 * VOXEL_SIZE_M;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkManager>()
            .init_resource::<WaterSimulation>()
            .add_systems(
                Startup,
                (
                    apply_loaded_player_position.after(setup_player),
                    export_debug_maps_on_startup,
                ),
            )
            .add_systems(
                Update,
                (
                    update_loaded_chunks,
                    update_chunk_meshes,
                    autosave_world,
                    persist_on_exit,
                    persist_on_window_close,
                ),
            )
            .add_systems(
                Update,
                update_water_physics
                    .after(update_loaded_chunks)
                    .before(update_chunk_meshes),
            );
    }
}
