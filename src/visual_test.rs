//! Development-only deterministic screenshot harness (`--visual-test`).
//!
//! Normal gameplay is untouched unless the flag is passed. The harness:
//! 1. redirects save/settings files (done in `main.rs`),
//! 2. skips the title screen,
//! 3. scans the world generator for representative points of interest
//!    (forest, landmark tree, mountain, lakeshore, meadow),
//! 4. teleports a frozen player through a fixed list of camera poses,
//! 5. waits until every surrounding column is generated *and* meshed,
//! 6. captures `visual_tests/<NN>_<name>.png`,
//! 7. writes per-view frame-time metrics to `visual_tests/perf.txt`.

use std::{collections::HashMap, fs, path::Path};

use bevy::render::view::screenshot::ScreenshotManager;
use bevy::{prelude::*, window::PrimaryWindow};

use crate::{
    game::GameState,
    player::{
        controller::{Controller, EYE_HEIGHT},
        Player, PlayerCamera,
    },
    settings::WorldSettings,
    world::{
        chunk_manager::{ChunkManager, WorldStats},
        coordinates::{ChunkCoord, VoxelCoord},
        generation::{biome::Biome, terrain::TerrainColumn, SEA_LEVEL_METRES},
        voxel::BlockType,
        CHUNK_SIZE,
    },
};

const OUTPUT_DIR: &str = "visual_tests";
/// Frames waited after a view reports ready before the shutter fires.
const SETTLE_FRAMES: u32 = 240;
/// Hard cap per phase so a stalled loader can never hang the run.
const PHASE_TIMEOUT_FRAMES: u32 = 3600;
const SEA_LEVEL_F: f32 = SEA_LEVEL_METRES as f32;

#[derive(Debug, Clone)]
struct ViewSpec {
    name: String,
    pos: Vec3,
    yaw: f32,
    pitch: f32,
}

#[derive(Debug)]
enum Phase {
    /// Waiting for the first drive call so the generator is available.
    ComputeViews,
    /// Teleport and aim this frame.
    Teleport,
    /// Let streaming/shadows/water settle while measuring frame time.
    Settle {
        frames_left: u32,
        seconds: f32,
        frames_measured: u32,
    },
    /// Shutter requested; waiting for the PNG to land on disk.
    Capturing,
}

#[derive(Resource)]
struct VisualTestState {
    views: Vec<ViewSpec>,
    index: usize,
    phase: Phase,
    wait_frames: u32,
    results: Vec<(String, f32, f32)>,
    finished: bool,
}

impl Default for VisualTestState {
    fn default() -> Self {
        Self {
            views: Vec::new(),
            index: 0,
            phase: Phase::ComputeViews,
            wait_frames: 0,
            results: Vec::new(),
            finished: false,
        }
    }
}

pub struct VisualTestPlugin;

impl Plugin for VisualTestPlugin {
    fn build(&self, app: &mut App) {
        fs::create_dir_all(OUTPUT_DIR).ok();
        app.init_resource::<VisualTestState>()
            .add_systems(PreUpdate, (enter_playing_state, hide_hud_overlays))
            .add_systems(PostUpdate, drive_visual_test);
    }
}

fn enter_playing_state(state: Res<State<GameState>>, mut next: ResMut<NextState<GameState>>) {
    if state.get() != &GameState::Playing {
        next.set(GameState::Playing);
    }
}

/// Hides every UI node (FPS box, hotbar, crosshair, chat) for clean shots.
fn hide_hud_overlays(
    mut commands: Commands,
    mut overlays: Query<&mut Visibility, With<bevy::ui::Node>>,
    highlights: Query<Entity, With<crate::player::interaction::BlockHighlight>>,
) {
    for mut visibility in &mut overlays {
        *visibility = Visibility::Hidden;
    }
    // The block-highlight cursor has no meaning in a scripted camera run.
    for entity in &highlights {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_visual_test(
    mut state: ResMut<VisualTestState>,
    time: Res<Time>,
    settings: Res<WorldSettings>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut screenshots: ResMut<ScreenshotManager>,
    manager: Option<ResMut<ChunkManager>>,
    mut player: Query<(&mut Transform, &mut Controller), With<Player>>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    stats: Option<Res<WorldStats>>,
    mut exit: EventWriter<AppExit>,
) {
    if state.finished {
        return;
    }

    // Take ownership of the current phase so arm bodies can freely read and
    // write every part of `state` without fighting field borrows.
    let phase = std::mem::replace(&mut state.phase, Phase::Teleport);

    match phase {
        Phase::ComputeViews => {
            let Some(manager) = manager else {
                state.phase = Phase::ComputeViews;
                return;
            };
            state.views = compute_views(&manager);
            info!("visual-test: {} views prepared", state.views.len());
            state.phase = Phase::Teleport;
        }
        Phase::Teleport => {
            let Some(spec) = state.views.get(state.index).cloned() else {
                finish(&mut state, &stats, &mut exit);
                return;
            };
            if let Ok((mut transform, mut controller)) = player.get_single_mut() {
                controller.velocity = Vec3::ZERO;
                controller.grounded = false;
                controller.flying = false;
                controller.yaw = spec.yaw;
                controller.pitch = spec.pitch;
                // The pose is re-asserted every settle frame, overriding any
                // residual gravity integration from `player_controller`.
                transform.translation = spec.pos - Vec3::Y * EYE_HEIGHT;
                transform.rotation = Quat::from_rotation_y(spec.yaw);
            }
            if let Ok(mut camera_transform) = camera.get_single_mut() {
                camera_transform.rotation = Quat::from_rotation_x(spec.pitch);
            }
            state.wait_frames = 0;
            state.phase = Phase::Settle {
                frames_left: SETTLE_FRAMES,
                seconds: 0.0,
                frames_measured: 0,
            };
        }
        Phase::Settle {
            mut frames_left,
            mut seconds,
            mut frames_measured,
        } => {
            // Re-assert the frozen pose every frame: `player_controller`
            // keeps integrating gravity in Update otherwise.
            if let Some(view) = state.views.get(state.index).cloned() {
                if let Ok((mut transform, mut controller)) = player.get_single_mut() {
                    controller.velocity = Vec3::ZERO;
                    transform.translation = view.pos - Vec3::Y * EYE_HEIGHT;
                    transform.rotation = Quat::from_rotation_y(view.yaw);
                }
                if let Ok(mut camera_transform) = camera.get_single_mut() {
                    camera_transform.rotation = Quat::from_rotation_x(view.pitch);
                }
            }

            state.wait_frames += 1;
            seconds += time.delta_seconds();
            frames_measured += 1;
            frames_left -= 1;

            let timed_out = state.wait_frames >= PHASE_TIMEOUT_FRAMES;

            // Once chunks around the view are loaded, un-bury the camera:
            // leaves count as solid, so canopy spawns get pushed out too.
            if let Some(manager) = &manager {
                let buried = state
                    .views
                    .get(state.index)
                    .map(|view| {
                        let eye = VoxelCoord::from_world_pos(view.pos);
                        manager.solid_at(eye) || manager.solid_at(eye.offset(0, 1, 0))
                    })
                    .unwrap_or(false);
                if buried {
                    let index = state.index;
                    let mut still_buried = false;
                    if let Some(view) = state.views.get_mut(index) {
                        let eye = VoxelCoord::from_world_pos(view.pos);
                        view.pos.y = if view.pos.y > eye.y as f32 {
                            eye.y as f32 + 1.0
                        } else {
                            view.pos.y + 1.0
                        };
                        still_buried = view.pos.y <= eye.y as f32 + 40.0;
                    }
                    if still_buried {
                        // Restart the measured window from the new pose.
                        state.phase = Phase::Settle {
                            frames_left: SETTLE_FRAMES,
                            seconds: 0.0,
                            frames_measured: 0,
                        };
                        return;
                    }
                    // else: give up nudging; capture whatever we have.
                }
            }

            let ready = timed_out
                || state
                    .views
                    .get(state.index)
                    .map(|view| {
                        view_ready(&manager, &view.pos, settings.render_distance_sections())
                    })
                    .unwrap_or(false);

            let Some(view) = state.views.get(state.index).cloned() else {
                finish(&mut state, &stats, &mut exit);
                return;
            };

            if !ready && !timed_out {
                // Streaming still in flight: restart the countdown so the
                // measured window only covers steady-state frames.
                state.phase = Phase::Settle {
                    frames_left: SETTLE_FRAMES,
                    seconds: 0.0,
                    frames_measured: 0,
                };
                return;
            }

            if frames_left > 0 && !timed_out {
                // Ready: keep measuring for the full settle window so the
                // FPS figure averages many frames, not just the first one.
                state.phase = Phase::Settle {
                    frames_left,
                    seconds,
                    frames_measured,
                };
                return;
            }

            if timed_out {
                warn!("visual-test: '{}' hit the readiness timeout", view.name);
            }
            let fps = if seconds > 0.0 {
                frames_measured as f32 / seconds
            } else {
                0.0
            };
            state.results.push((view.name.clone(), fps, seconds));

            let Ok(window) = windows.get_single() else {
                error!("visual-test: no primary window");
                finish(&mut state, &stats, &mut exit);
                return;
            };
            let path = output_path(state.index, &view.name);
            match screenshots.save_screenshot_to_disk(window, path) {
                Ok(()) => {
                    state.wait_frames = 0;
                    state.phase = Phase::Capturing;
                }
                Err(_) => {
                    // A request for this window is still pending; retry the
                    // shutter next frame without losing this view's metrics.
                    state.phase = Phase::Settle {
                        frames_left,
                        seconds,
                        frames_measured,
                    };
                }
            }
        }
        Phase::Capturing => {
            // The renderer writes the PNG asynchronously; advance once the
            // file lands on disk (with a generous stall guard).
            let Some(view) = state.views.get(state.index).cloned() else {
                finish(&mut state, &stats, &mut exit);
                return;
            };
            let path = output_path(state.index, &view.name);
            if path.exists() {
                info!("visual-test: saved {}", path.display());
                state.index += 1;
                if state.index >= state.views.len() {
                    finish(&mut state, &stats, &mut exit);
                } else {
                    state.phase = Phase::Teleport;
                }
            } else if state.wait_frames >= PHASE_TIMEOUT_FRAMES {
                error!("visual-test: capture for {} never landed", path.display());
                finish(&mut state, &stats, &mut exit);
            } else {
                state.wait_frames += 1;
                state.phase = Phase::Capturing;
            }
        }
    }
}

fn finish(
    state: &mut VisualTestState,
    stats: &Option<Res<WorldStats>>,
    exit: &mut EventWriter<AppExit>,
) {
    if state.finished {
        return;
    }
    state.finished = true;

    let mut report = String::from("view\tavg_fps\tsettle_seconds\n");
    for (name, fps, seconds) in &state.results {
        report.push_str(&format!("{name}\t{fps:.1}\t{seconds:.2}\n"));
    }
    if let Some(stats) = stats.as_deref() {
        report.push_str(&format!(
            "\ntriangles={}\nloaded_sections={}\ngenerated_columns={}\nrender_distance_m={:.0}\n",
            stats.triangles, stats.loaded_chunks, stats.generated_chunks, stats.render_distance_m
        ));
    }
    let path = Path::new(OUTPUT_DIR).join("perf.txt");
    match fs::write(&path, report) {
        Ok(()) => info!("visual-test: metrics written to {}", path.display()),
        Err(error) => error!("visual-test: failed to write {}: {error}", path.display()),
    }
    exit.send(AppExit::Success);
}

fn output_path(index: usize, name: &str) -> std::path::PathBuf {
    Path::new(OUTPUT_DIR).join(format!("{index:02}_{name}.png"))
}

/// Every column whose mesh could contribute pixels around `pos` must exist
/// and be mesh-clean. Uses one ring inside the load radius so the meshing
/// neighbour gate (including AO diagonals) is already satisfied.
fn view_ready(
    manager: &Option<ResMut<ChunkManager>>,
    pos: &Vec3,
    load_radius_sections: i32,
) -> bool {
    let Some(manager) = manager else {
        return false;
    };
    let center = VoxelCoord::from_world_pos(*pos).chunk();
    let radius = (load_radius_sections - 1).max(1);
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz <= radius * radius {
                let column = ChunkCoord {
                    x: center.x + dx,
                    z: center.z + dz,
                };
                if !manager.column_meshes_ready(column) {
                    return false;
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Deterministic point-of-interest scan
// ---------------------------------------------------------------------------

const SCAN_RADIUS_M: i32 = 1024;
const SCAN_STEP_M: i32 = 24;

fn scanned_columns() -> impl Iterator<Item = (i32, i32)> {
    (-SCAN_RADIUS_M..=SCAN_RADIUS_M)
        .step_by(SCAN_STEP_M as usize)
        .flat_map(move |z| {
            (-SCAN_RADIUS_M..=SCAN_RADIUS_M)
                .step_by(SCAN_STEP_M as usize)
                .map(move |x| (x, z))
        })
}

fn compute_views(manager: &ChunkManager) -> Vec<ViewSpec> {
    let forest_poi = find_forest(manager);
    let meadow_poi = find_meadow(manager);
    let shore_poi = find_shore(manager);
    let peak_poi = find_peak(manager);

    // Anchor the overview over land (the origin is ocean on this seed) and
    // frame the tallest terrain in the region for a composed hero shot.
    let (overview_base, overview_yaw, overview_pitch) = match (&forest_poi, &peak_poi) {
        (Some((fx, fz, _)), Some((px, pz, _))) => {
            let eye = Vec3::new(*fx as f32 + 0.5, 0.0, *fz as f32 + 0.5);
            let (yaw, _) = aim_from_to(eye, Vec3::new(*px as f32, 0.0, *pz as f32));
            (eye, yaw, -0.50)
        }
        _ => (Vec3::new(0.5, 0.0, 0.5), 0.35, -0.62),
    };
    let overview_ground = manager.column_at(
        overview_base.x.floor() as i32,
        overview_base.z.floor() as i32,
    );
    let overview_surface = surface_eye(overview_ground);

    let mut views = Vec::new();

    // 01 - high overview above the origin region.
    views.push(ViewSpec {
        name: "overview".into(),
        pos: overview_surface
            + Vec3::new(0.0, 52.0, 0.0)
            + Vec3::new(-overview_yaw.sin(), 0.0, -overview_yaw.cos()) * -18.0,
        yaw: overview_yaw,
        pitch: overview_pitch,
    });

    // 02 - inside the densest nearby forest.
    if let Some((fx, fz, forest)) = forest_poi {
        let eye = Vec3::new(
            fx as f32 + 0.5,
            forest.height_m as f32 + 2.4,
            fz as f32 + 0.5,
        );
        let target = Vec3::new(fx as f32 + 30.0, eye.y + 1.5, fz as f32 - 18.0);
        let (yaw, pitch) = aim_from_to(eye, target);
        views.push(ViewSpec {
            name: "forest".into(),
            pos: eye,
            yaw,
            pitch,
        });

        // 03 - three-quarter closeup of a tall nearby trunk.
        if let Some(spec) = tree_closeup_view(manager, fx, fz) {
            views.push(spec);
        }
    }

    // 04 - mountains from a sky vantage across the lowland, framing the
    // whole massif silhouette (a floating camera never needs un-burying).
    if let Some((px, pz, peak)) = peak_poi {
        let dir = Vec3::new(-(px as f32), 0.0, -(pz as f32)).normalize_or_zero();
        let eye = Vec3::new(
            px as f32 + dir.x * 150.0,
            peak.height_m as f32 * 0.5 + 52.0,
            pz as f32 + dir.z * 150.0,
        );
        let summit = Vec3::new(px as f32 + 0.5, peak.height_m as f32 - 6.0, pz as f32 + 0.5);
        let (yaw, pitch) = aim_from_to(eye, summit);
        views.push(ViewSpec {
            name: "mountains".into(),
            pos: eye,
            yaw,
            pitch,
        });
    }

    // 05 - macro lens on open grassland materials.
    if let Some((mx, mz, meadow)) = meadow_poi {
        views.push(ViewSpec {
            name: "ground_detail".into(),
            pos: Vec3::new(
                mx as f32 + 0.5,
                meadow.height_m as f32 + 1.75,
                mz as f32 + 0.5,
            ),
            yaw: 0.6,
            pitch: -0.42,
        });
    }

    // 06 - standing on sand looking across open water.
    if let Some(((sx, sz, shore_col), (wx, wz))) = shore_poi {
        let eye = Vec3::new(
            sx as f32 + 0.5,
            shore_col.height_m as f32 + 2.6,
            sz as f32 + 0.5,
        );
        let target = Vec3::new(wx as f32 + 0.5, SEA_LEVEL_F - 3.0, wz as f32 + 0.5);
        let (yaw, pitch) = aim_from_to(eye, target);
        views.push(ViewSpec {
            name: "water".into(),
            pos: eye,
            yaw,
            pitch,
        });
    }

    // 07 - elevated distant view exercising fog and silhouettes.
    let far_yaw = 2.4_f32;
    let dir = Vec3::new(-far_yaw.sin(), 0.0, -far_yaw.cos());
    views.push(ViewSpec {
        name: "distant_view".into(),
        pos: overview_surface + dir * 40.0 + Vec3::new(0.0, 44.0, 0.0),
        yaw: far_yaw,
        pitch: -0.12,
    });

    views
}

fn surface_eye(column: TerrainColumn) -> Vec3 {
    Vec3::new(
        0.5,
        (column.height_m as f32).max(SEA_LEVEL_F).max(2.0) + 2.0,
        0.5,
    )
}

fn aim_from_to(eye: Vec3, target: Vec3) -> (f32, f32) {
    let delta = target - eye;
    let horizontal = delta.xz().length().max(0.001);
    let yaw = (-delta.x).atan2(-delta.z);
    let pitch = (delta.y / horizontal).clamp(-1.35, 1.35).atan();
    (yaw, pitch)
}

fn find_forest(manager: &ChunkManager) -> Option<(i32, i32, TerrainColumn)> {
    let mut best: Option<(i32, i32, TerrainColumn)> = None;
    let mut best_score = 0.0f64;
    for (x, z) in scanned_columns() {
        let column = manager.column_at(x, z);
        if !matches!(column.biome, Biome::Forest | Biome::DenseForest)
            || column.height <= SEA_LEVEL_METRES + 2
            || column.slope >= 0.7
            || column.water_level_m.is_some()
        {
            continue;
        }
        let score = column.forest_density - column.slope * 0.2;
        if score > best_score {
            best_score = score;
            best = Some((x, z, column));
        }
    }
    best
}

fn find_meadow(manager: &ChunkManager) -> Option<(i32, i32, TerrainColumn)> {
    let mut best: Option<(i32, i32, TerrainColumn)> = None;
    let mut best_dist = f64::INFINITY;
    for (x, z) in scanned_columns() {
        let column = manager.column_at(x, z);
        if !matches!(column.biome, Biome::Plains | Biome::Meadow)
            || column.slope >= 0.25
            || column.water_level_m.is_some()
            || column.height <= SEA_LEVEL_METRES + 1
        {
            continue;
        }
        let dist_sq = (x * x + z * z) as f64;
        if dist_sq < best_dist {
            best_dist = dist_sq;
            best = Some((x, z, column));
        }
    }
    best
}

fn find_peak(manager: &ChunkManager) -> Option<(i32, i32, TerrainColumn)> {
    let mut best: Option<(i32, i32, TerrainColumn)> = None;
    let mut best_height = f64::NEG_INFINITY;
    for (x, z) in scanned_columns() {
        let column = manager.column_at(x, z);
        if column.height_m > best_height {
            best_height = column.height_m;
            best = Some((x, z, column));
        }
    }
    best
}

/// Land cell ring search: sandy ground at sea level with genuine ocean a
/// dozen metres away.
type ShoreHit = ((i32, i32, TerrainColumn), (i32, i32));

fn find_shore(manager: &ChunkManager) -> Option<ShoreHit> {
    for ring in [24i32, 48, 96, 160, 240, 320] {
        let step = 8;
        let mut z = -ring;
        while z <= ring {
            let mut x = -ring;
            while x <= ring {
                let edge = x.abs() == ring || z.abs() == ring;
                if edge {
                    let column = manager.column_at(x, z);
                    if column.water_level_m.is_none()
                        && column.height >= SEA_LEVEL_METRES
                        && column.height <= SEA_LEVEL_METRES + 4
                        && column.biome.surface_block() == BlockType::Sand
                    {
                        for (dx, dz) in [(12, 0), (-12, 0), (0, 12), (0, -12)] {
                            let water = manager.column_at(x + dx, z + dz);
                            if water.water_level_m.is_some()
                                && water.height_m < SEA_LEVEL_F as f64
                                && matches!(water.biome, Biome::Ocean | Biome::DeepOcean)
                            {
                                return Some(((x, z, column), (x + dx, z + dz)));
                            }
                        }
                    }
                }
                x += step;
            }
            z += step;
        }
    }
    None
}

/// Finds a tall trunk near the forest POI and frames it from a three-quarter
/// angle so canopy shape and trunk taper are both visible.
fn tree_closeup_view(manager: &ChunkManager, forest_x: i32, forest_z: i32) -> Option<ViewSpec> {
    let center = ChunkCoord {
        x: forest_x.div_euclid(CHUNK_SIZE),
        z: forest_z.div_euclid(CHUNK_SIZE),
    };

    // Lowest wood voxel per (x, z) column == trunk base.
    let mut lowest_wood: HashMap<(i32, i32), i32> = HashMap::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let candidate = ChunkCoord {
                x: center.x + dx,
                z: center.z + dz,
            };
            for voxel in manager.generator().vegetation_for_chunk(candidate) {
                if !matches!(
                    voxel.block,
                    BlockType::Wood | BlockType::BirchWood | BlockType::JungleWood
                ) {
                    continue;
                }
                let key = (voxel.coord.x, voxel.coord.z);
                let entry = lowest_wood.entry(key).or_insert(voxel.coord.y);
                *entry = (*entry).min(voxel.coord.y);
            }
        }
    }

    let mut best: Option<(f64, (i32, i32), i32)> = None;
    for ((x, z), base_y) in lowest_wood {
        let dist_sq = ((x - forest_x).pow(2) + (z - forest_z).pow(2)) as f64;
        let score = -dist_sq + base_y as f64 * 4.0;
        if best
            .map(|(best_score, _, _)| score > best_score)
            .unwrap_or(true)
        {
            best = Some((score, (x, z), base_y));
        }
    }
    let (_, (tx, tz), base_y) = best?;
    let focus_y = (base_y + 8) as f32;
    let eye = Vec3::new(tx as f32 + 11.5, focus_y + 2.0, tz as f32 + 7.5);
    let focus = Vec3::new(tx as f32 + 0.5, focus_y - 2.0, tz as f32 + 0.5);
    let (yaw, pitch) = aim_from_to(eye, focus);
    Some(ViewSpec {
        name: "tree_closeup".into(),
        pos: eye,
        yaw,
        pitch,
    })
}
