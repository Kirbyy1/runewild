use bevy::{math::primitives::Cuboid, prelude::*};

use crate::{
    player::{
        controller::{player_aabb, CursorState},
        Player, PlayerCamera,
    },
    ui::chat::ChatState,
    world::{
        chunk_manager::ChunkManager, coordinates::VoxelCoord, persistence::SaveGame,
        voxel::BlockType, water::WaterSimulation, VOXEL_SIZE_M,
    },
};

const INTERACTION_DISTANCE: f32 = 6.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct RaycastHit {
    pub voxel: VoxelCoord,
    pub previous: VoxelCoord,
    pub normal: IVec3,
    pub distance_m: f32,
}

#[derive(Debug, Default, Resource)]
pub struct TargetedBlock(pub Option<RaycastHit>);

#[derive(Debug, Default, Resource)]
pub struct HotbarSelection {
    pub slot: usize,
}

impl HotbarSelection {
    pub fn selected_block(&self) -> BlockType {
        BlockType::HOTBAR[self.slot]
    }
}

#[derive(Component)]
pub struct BlockHighlight;

#[allow(clippy::too_many_arguments)]
pub fn update_block_highlight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    world: Res<ChunkManager>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    cursor: Res<CursorState>,
    chat_state: Option<Res<ChatState>>,
    mut target: ResMut<TargetedBlock>,
    highlight: Query<Entity, With<BlockHighlight>>,
    mut visibility: Query<&mut Visibility, With<BlockHighlight>>,
) {
    let is_chat_open = chat_state.map(|c| c.is_open).unwrap_or(false);

    let Ok(camera_transform) = camera.get_single() else {
        return;
    };
    let origin = camera_transform.translation();
    let direction = *camera_transform.forward();
    target.0 = raycast_voxels(origin, direction, INTERACTION_DISTANCE, |coord| {
        world.block_at(coord).is_solid()
    });

    let entity = if let Ok(entity) = highlight.get_single() {
        entity
    } else {
        commands
            .spawn((
                BlockHighlight,
                PbrBundle {
                    mesh: meshes.add(Mesh::from(Cuboid::from_size(Vec3::splat(
                        VOXEL_SIZE_M * 1.06,
                    )))),
                    material: materials.add(StandardMaterial {
                        base_color: Color::srgba(1.0, 0.82, 0.32, 0.22),
                        alpha_mode: AlphaMode::Blend,
                        unlit: true,
                        ..default()
                    }),
                    ..default()
                },
            ))
            .id()
    };

    let shown = target
        .0
        .filter(|_| cursor.is_grabbed() && !is_chat_open)
        .map(|hit| hit.voxel.world_pos_m() + Vec3::splat(VOXEL_SIZE_M * 0.5));

    match (shown, visibility.get_mut(entity)) {
        (Some(center), Ok(mut vis)) => {
            commands
                .entity(entity)
                .insert(Transform::from_translation(center));
            *vis = Visibility::Visible;
        }
        (_, Ok(mut vis)) => *vis = Visibility::Hidden,
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
pub fn block_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorState>,
    chat_state: Option<Res<ChatState>>,
    mut selection: ResMut<HotbarSelection>,
    target: Res<TargetedBlock>,
    player: Query<&Transform, With<Player>>,
    mut world: ResMut<ChunkManager>,
    mut water: ResMut<WaterSimulation>,
    mut save: ResMut<SaveGame>,
) {
    let is_chat_open = chat_state.map(|c| c.is_open).unwrap_or(false);
    if is_chat_open {
        return;
    }

    for (slot, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key) {
            selection.slot = slot;
        }
    }

    if !cursor.is_grabbed() || cursor.clicks_suppressed() {
        return;
    }

    let Some(hit) = target.0 else {
        return;
    };
    debug_assert!(hit.distance_m <= INTERACTION_DISTANCE + f32::EPSILON);

    if mouse.just_pressed(MouseButton::Left)
        && world.set_block(hit.voxel, BlockType::Air, &mut save)
    {
        water.remove_source(hit.voxel);
        water.notify_block_changed(hit.voxel);
    }

    if mouse.just_pressed(MouseButton::Right) {
        let Ok(player_transform) = player.get_single() else {
            return;
        };
        let block = selection.selected_block();
        let placement = placement_voxel(hit);
        if block != BlockType::Air
            && !block_intersects_player(placement, player_transform.translation)
            && world.set_block(placement, block, &mut save)
        {
            if block == BlockType::Water {
                water.add_source(placement);
            } else {
                water.remove_source(placement);
                water.notify_block_changed(placement);
            }
        }
    }
}

fn placement_voxel(hit: RaycastHit) -> VoxelCoord {
    let face_adjacent = hit.voxel.offset(hit.normal.x, hit.normal.y, hit.normal.z);
    // Exact edge/corner crossings can advance more than one DDA axis at once.
    // A single-axis normal is ambiguous there, while `previous` remains the
    // actual empty cell traversed immediately before the hit.
    if hit.normal != IVec3::ZERO && face_adjacent == hit.previous {
        face_adjacent
    } else {
        hit.previous
    }
}

fn block_intersects_player(block: VoxelCoord, player_position: Vec3) -> bool {
    let (min, max) = player_aabb(player_position);
    let bmin = block.world_pos_m();
    let bmax = bmin + Vec3::splat(VOXEL_SIZE_M);
    min.x < bmax.x
        && max.x > bmin.x
        && min.y < bmax.y
        && max.y > bmin.y
        && min.z < bmax.z
        && max.z > bmin.z
}

pub fn raycast_voxels<F>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    mut solid: F,
) -> Option<RaycastHit>
where
    F: FnMut(VoxelCoord) -> bool,
{
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }

    if max_distance < 0.0 {
        return None;
    }

    let (mut voxel, mut normal) = initial_voxel(origin, direction);
    let mut previous = voxel;
    let step = IVec3::new(
        direction.x.signum() as i32,
        direction.y.signum() as i32,
        direction.z.signum() as i32,
    );
    let mut t_max = Vec3::new(
        boundary_distance(origin.x, direction.x, voxel.x),
        boundary_distance(origin.y, direction.y, voxel.y),
        boundary_distance(origin.z, direction.z, voxel.z),
    );
    let t_delta = Vec3::new(
        if direction.x == 0.0 {
            f32::INFINITY
        } else {
            VOXEL_SIZE_M / direction.x.abs()
        },
        if direction.y == 0.0 {
            f32::INFINITY
        } else {
            VOXEL_SIZE_M / direction.y.abs()
        },
        if direction.z == 0.0 {
            f32::INFINITY
        } else {
            VOXEL_SIZE_M / direction.z.abs()
        },
    );
    let mut distance = 0.0;

    loop {
        if solid(voxel) {
            return Some(RaycastHit {
                voxel,
                previous,
                normal,
                distance_m: distance,
            });
        }

        let next_distance = t_max.x.min(t_max.y).min(t_max.z);
        if !next_distance.is_finite() || next_distance > max_distance {
            return None;
        }

        previous = voxel;
        normal = IVec3::ZERO;
        let epsilon = 1.0e-6;
        if (t_max.x - next_distance).abs() <= epsilon {
            voxel.x += step.x;
            t_max.x += t_delta.x;
            normal = IVec3::new(-step.x, 0, 0);
        }
        if (t_max.y - next_distance).abs() <= epsilon {
            voxel.y += step.y;
            t_max.y += t_delta.y;
            if normal == IVec3::ZERO {
                normal = IVec3::new(0, -step.y, 0);
            }
        }
        if (t_max.z - next_distance).abs() <= epsilon {
            voxel.z += step.z;
            t_max.z += t_delta.z;
            if normal == IVec3::ZERO {
                normal = IVec3::new(0, 0, -step.z);
            }
        }
        distance = next_distance;
    }
}

fn initial_voxel(origin: Vec3, direction: Vec3) -> (VoxelCoord, IVec3) {
    let mut voxel = VoxelCoord::from_world_pos(origin);
    let mut normal = IVec3::ZERO;
    for (axis, value, dir) in [
        (0, origin.x, direction.x),
        (1, origin.y, direction.y),
        (2, origin.z, direction.z),
    ] {
        let grid = value / VOXEL_SIZE_M;
        if dir < 0.0 && (grid - grid.round()).abs() <= 1.0e-6 {
            match axis {
                0 => {
                    voxel.x -= 1;
                    normal = IVec3::X;
                }
                1 => {
                    voxel.y -= 1;
                    if normal == IVec3::ZERO {
                        normal = IVec3::Y;
                    }
                }
                _ => {
                    voxel.z -= 1;
                    if normal == IVec3::ZERO {
                        normal = IVec3::Z;
                    }
                }
            }
        }
    }
    (voxel, normal)
}

fn boundary_distance(origin: f32, direction: f32, voxel: i32) -> f32 {
    if direction == 0.0 {
        return f32::INFINITY;
    }
    let boundary = if direction > 0.0 {
        (voxel + 1) as f32 * VOXEL_SIZE_M
    } else {
        voxel as f32 * VOXEL_SIZE_M
    };
    ((boundary - origin) / direction).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raycast_traverses_one_metre_voxels_in_metres() {
        let hit = raycast_voxels(Vec3::new(0.5, 1.5, 0.5), Vec3::X, 6.0, |coord| {
            coord == VoxelCoord { x: 5, y: 1, z: 0 }
        })
        .unwrap();
        assert_eq!(hit.voxel, VoxelCoord { x: 5, y: 1, z: 0 });
        assert_eq!(hit.previous, VoxelCoord { x: 4, y: 1, z: 0 });
        assert_eq!(hit.normal, IVec3::new(-1, 0, 0));
        assert!((hit.distance_m - 4.5).abs() < 1.0e-5);
    }

    #[test]
    fn raycast_respects_max_distance() {
        let hit = raycast_voxels(Vec3::new(0.5, 0.5, 0.5), Vec3::X, 6.0, |coord| {
            coord == VoxelCoord { x: 7, y: 0, z: 0 }
        });
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_handles_negative_coordinates_and_normal() {
        let hit = raycast_voxels(Vec3::new(-0.1, -0.1, -0.1), Vec3::NEG_X, 2.0, |coord| {
            coord
                == VoxelCoord {
                    x: -2,
                    y: -1,
                    z: -1,
                }
        })
        .unwrap();
        assert_eq!(
            hit.voxel,
            VoxelCoord {
                x: -2,
                y: -1,
                z: -1
            }
        );
        assert_eq!(
            hit.previous,
            VoxelCoord {
                x: -1,
                y: -1,
                z: -1
            }
        );
        assert_eq!(hit.normal, IVec3::X);
        assert!((hit.distance_m - 0.9).abs() < 1.0e-5);
    }

    #[test]
    fn placement_aabb_uses_one_visible_voxel() {
        let block = VoxelCoord::new(-1, 0, 0);
        assert!(block_intersects_player(block, Vec3::new(-0.2, 0.0, 0.1)));
        assert!(!block_intersects_player(block, Vec3::new(0.5, 0.0, 0.1)));
    }

    #[test]
    fn placement_uses_previous_voxel_when_hit_normal_is_ambiguous() {
        let hit = RaycastHit {
            voxel: VoxelCoord::new(0, 6, 0),
            previous: VoxelCoord::new(0, 6, -1),
            normal: IVec3::ZERO,
            distance_m: 1.0,
        };
        assert_eq!(placement_voxel(hit), hit.previous);
    }
}
