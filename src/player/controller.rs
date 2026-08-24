use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use crate::{
    game::GameState,
    player::{Player, PlayerCamera},
    settings::WorldSettings,
    ui::chat::ChatState,
    world::{
        chunk_manager::ChunkManager, coordinates::VoxelCoord, persistence::SaveGame, VOXEL_SIZE_M,
    },
};

const PLAYER_HEIGHT: f32 = 1.82;
const PLAYER_RADIUS: f32 = 0.32;
const EYE_HEIGHT: f32 = 1.62;
const WALK_SPEED: f32 = 5.2;
const SPRINT_SPEED: f32 = 8.4;
const JUMP_SPEED: f32 = 7.2;
const GRAVITY: f32 = 22.0;
const FLIGHT_SPEED: f32 = 12.0;
const DOUBLE_TAP_WINDOW: f32 = 0.3; // seconds

#[derive(Debug, Component)]
pub struct Controller {
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub sensitivity: f32,
    pub flying: bool,
    pub space_pressed: bool,
    pub last_space_press_time: f32,
    pub speed_multiplier: f32,
}

#[derive(Debug, Default, Resource)]
pub struct CursorState {
    grabbed: bool,
    /// One-frame flag: the click that captured (or re-captured) the cursor
    /// must never be treated as a gameplay break/place action.
    suppress_clicks: bool,
}

impl CursorState {
    pub fn is_grabbed(&self) -> bool {
        self.grabbed
    }

    pub fn set_grabbed(&mut self, grabbed: bool) {
        self.grabbed = grabbed;
    }

    pub fn clicks_suppressed(&self) -> bool {
        self.suppress_clicks
    }
}

pub fn setup_player(mut commands: Commands, save: Res<SaveGame>, settings: Res<WorldSettings>) {
    let render_distance_m = settings.render_distance_m();
    commands
        .spawn((
            Player,
            Controller {
                velocity: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                grounded: false,
                sensitivity: 0.0018,
                flying: false,
                space_pressed: false,
                last_space_press_time: -1.0,
                speed_multiplier: 1.0,
            },
            SpatialBundle::from_transform(Transform::from_translation(save.player_position)),
        ))
        .with_children(|parent| {
            parent.spawn((
                PlayerCamera,
                Camera3dBundle {
                    camera: Camera {
                        hdr: true,
                        ..default()
                    },
                    transform: Transform::from_xyz(0.0, EYE_HEIGHT, 0.0),
                    projection: Projection::Perspective(PerspectiveProjection {
                        fov: 75.0_f32.to_radians(),
                        far: render_distance_m * 1.2,
                        ..default()
                    }),
                    tonemapping: Tonemapping::TonyMcMapface,
                    ..default()
                },
                FogSettings {
                    color: Color::srgba(0.48, 0.69, 0.86, 0.72),
                    directional_light_color: Color::srgba(1.0, 0.84, 0.62, 0.42),
                    directional_light_exponent: 24.0,
                    falloff: FogFalloff::Linear {
                        start: render_distance_m * 0.70,
                        end: render_distance_m * 0.98,
                    },
                },
            ));
        });
}

pub fn apply_render_settings(
    settings: Res<WorldSettings>,
    mut cameras: Query<(&mut Projection, &mut FogSettings), With<PlayerCamera>>,
) {
    if !settings.is_changed() {
        return;
    }
    let Ok((mut projection, mut fog)) = cameras.get_single_mut() else {
        return;
    };
    let distance = settings.render_distance_m();
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.far = distance * 1.2;
    }
    fog.falloff = FogFalloff::Linear {
        start: distance * 0.70,
        end: distance * 0.98,
    };
}

#[allow(clippy::too_many_arguments)]
pub fn player_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cursor: ResMut<CursorState>,
    mut next_state: ResMut<NextState<GameState>>,
    world: Res<ChunkManager>,
    chat_state: Option<Res<ChatState>>,
    mut player: Query<(&mut Transform, &mut Controller), With<Player>>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    let is_chat_open = chat_state.map(|c| c.is_open).unwrap_or(false);

    if let Ok(mut window) = windows.get_single_mut() {
        if is_chat_open {
            if cursor.grabbed {
                cursor.grabbed = false;
                set_window_cursor(&mut window, false);
            }
        } else {
            if mouse_buttons.just_pressed(MouseButton::Left) {
                cursor.grabbed = true;
                set_window_cursor(&mut window, true);
            }
            if keys.just_pressed(KeyCode::Escape) && cursor.grabbed {
                next_state.set(GameState::Paused);
            }
        }
    }

    let Ok((mut transform, mut controller)) = player.get_single_mut() else {
        return;
    };

    if cursor.grabbed && !is_chat_open {
        let delta = mouse_motion
            .read()
            .fold(Vec2::ZERO, |acc, event| acc + event.delta);
        controller.yaw -= delta.x * controller.sensitivity;
        controller.pitch = (controller.pitch - delta.y * controller.sensitivity).clamp(-1.52, 1.52);
    } else {
        mouse_motion.clear();
    }

    transform.rotation = Quat::from_rotation_y(controller.yaw);
    if let Ok(mut camera_transform) = camera.get_single_mut() {
        camera_transform.rotation = Quat::from_rotation_x(controller.pitch);
    }

    if is_chat_open {
        // While typing in chat, do not move player with WASD
        return;
    }

    let forward = *transform.forward();
    let right = *transform.right();
    let up = Vec3::Y;
    let mut wish = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish -= right;
    }

    // Double-spacebar detection for flight toggle
    let space_currently_pressed = keys.pressed(KeyCode::Space);
    let current_time = time.elapsed_seconds();

    if space_currently_pressed && !controller.space_pressed {
        // Just pressed space
        if current_time - controller.last_space_press_time < DOUBLE_TAP_WINDOW {
            // Double tap detected
            controller.flying = !controller.flying;
        }
        controller.last_space_press_time = current_time;
    }
    controller.space_pressed = space_currently_pressed;

    let speed_mult = controller.speed_multiplier.max(0.05);

    if controller.flying {
        // Flight mode movement
        if keys.pressed(KeyCode::Space) {
            wish += up;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            wish -= up;
        }

        wish.y = 0.0; // Reset Y for horizontal wishes
        if wish.length_squared() > 0.0 {
            wish = wish.normalize();
        }

        let speed = FLIGHT_SPEED * speed_mult;
        controller.velocity = wish * speed;
        if keys.pressed(KeyCode::Space) {
            controller.velocity.y = speed;
        } else if keys.pressed(KeyCode::ShiftLeft) {
            controller.velocity.y = -speed;
        }
    } else {
        // Normal walking/jumping mode
        wish.y = 0.0;
        if wish.length_squared() > 0.0 {
            wish = wish.normalize();
        }

        let base_speed = if keys.pressed(KeyCode::ShiftLeft) {
            SPRINT_SPEED
        } else {
            WALK_SPEED
        };
        let speed = base_speed * speed_mult;
        controller.velocity.x = wish.x * speed;
        controller.velocity.z = wish.z * speed;
        controller.velocity.y -= GRAVITY * time.delta_seconds();

        if controller.grounded && keys.just_pressed(KeyCode::Space) {
            controller.velocity.y = JUMP_SPEED;
            controller.grounded = false;
        }
    }

    let dt = time.delta_seconds();
    let dx = controller.velocity.x * dt;
    let dz = controller.velocity.z * dt;
    let dy = controller.velocity.y * dt;

    if !controller.flying {
        move_axis(
            &world,
            &mut transform.translation,
            &mut controller,
            Vec3::X,
            dx,
        );
        move_axis(
            &world,
            &mut transform.translation,
            &mut controller,
            Vec3::Z,
            dz,
        );
        move_axis(
            &world,
            &mut transform.translation,
            &mut controller,
            Vec3::Y,
            dy,
        );
    } else {
        // In flight mode, no collision detection needed
        transform.translation += controller.velocity * dt;
    }
}

fn move_axis(
    world: &ChunkManager,
    position: &mut Vec3,
    controller: &mut Controller,
    axis: Vec3,
    amount: f32,
) {
    if amount == 0.0 {
        return;
    }
    let steps = (amount.abs() / (VOXEL_SIZE_M * 0.5)).ceil().max(1.0) as usize;
    let step = amount / steps as f32;
    for _ in 0..steps {
        let old = *position;
        *position += axis * step;
        if overlaps_world(world, *position) {
            *position = old;
            if axis.y != 0.0 {
                controller.grounded = amount < 0.0;
                controller.velocity.y = 0.0;
            }
            return;
        }
    }
    if axis.y != 0.0 {
        controller.grounded = false;
    }
}

pub fn player_aabb(position: Vec3) -> (Vec3, Vec3) {
    (
        Vec3::new(
            position.x - PLAYER_RADIUS,
            position.y,
            position.z - PLAYER_RADIUS,
        ),
        Vec3::new(
            position.x + PLAYER_RADIUS,
            position.y + PLAYER_HEIGHT,
            position.z + PLAYER_RADIUS,
        ),
    )
}

fn overlaps_world(world: &ChunkManager, position: Vec3) -> bool {
    let (min, max) = player_aabb(position);
    let (voxel_min, voxel_max) = voxel_bounds_for_aabb(min, max);
    for y in voxel_min.y..=voxel_max.y {
        for z in voxel_min.z..=voxel_max.z {
            for x in voxel_min.x..=voxel_max.x {
                if world.solid_at(VoxelCoord { x, y, z }) {
                    return true;
                }
            }
        }
    }
    false
}

fn voxel_bounds_for_aabb(min: Vec3, max: Vec3) -> (VoxelCoord, VoxelCoord) {
    let min = VoxelCoord::from_world_pos(min);
    // AABBs are half-open at their maximum edge. `ceil - 1` avoids treating
    // a voxel touched only at the boundary as overlapping, including at
    // exact negative-grid boundaries.
    let max = VoxelCoord {
        x: (max.x / VOXEL_SIZE_M).ceil() as i32 - 1,
        y: (max.y / VOXEL_SIZE_M).ceil() as i32 - 1,
        z: (max.z / VOXEL_SIZE_M).ceil() as i32 - 1,
    };
    (min, max)
}

pub fn set_window_cursor(window: &mut Window, locked: bool) {
    window.cursor.grab_mode = if locked {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    window.cursor.visible = !locked;
}

/// Entering gameplay captures and hides the mouse, and suppresses the click
/// that triggered the transition (Play/Resume button presses).
pub fn on_enter_playing(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cursor: ResMut<CursorState>,
) {
    cursor.grabbed = true;
    cursor.suppress_clicks = true;
    if let Ok(mut window) = windows.get_single_mut() {
        set_window_cursor(&mut window, true);
    }
}

/// Pausing or returning to the title screen frees the mouse.
pub fn release_cursor(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut cursor: ResMut<CursorState>,
) {
    cursor.grabbed = false;
    if let Ok(mut window) = windows.get_single_mut() {
        set_window_cursor(&mut window, false);
    }
}

/// Runs in `PostUpdate`: re-enables gameplay clicks after the capture frame.
pub fn clear_click_suppression(mut cursor: ResMut<CursorState>) {
    cursor.suppress_clicks = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_bounds_use_one_metre_voxels_and_half_open_maximum() {
        let (min, max) =
            voxel_bounds_for_aabb(Vec3::new(-0.25, 0.0, 0.25), Vec3::new(0.25, 1.0, 0.50));
        assert_eq!(min, VoxelCoord { x: -1, y: 0, z: 0 });
        assert_eq!(max, VoxelCoord { x: 0, y: 0, z: 0 });
    }

    #[test]
    fn player_dimensions_remain_expressed_in_metres() {
        let (min, max) = player_aabb(Vec3::new(2.0, 3.0, -4.0));
        assert!((max.y - min.y - PLAYER_HEIGHT).abs() < 1.0e-6);
        assert!((max.x - min.x - PLAYER_RADIUS * 2.0).abs() < 1.0e-6);
    }
}
