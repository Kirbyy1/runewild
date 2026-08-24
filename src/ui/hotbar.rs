use bevy::prelude::*;

use crate::{
    player::{interaction::HotbarSelection, Player},
    world::{
        chunk_manager::{ChunkManager, WorldStats},
        coordinates::VoxelCoord,
        voxel::BlockType,
    },
};

#[derive(Component)]
pub struct HotbarSlot {
    index: usize,
}

#[derive(Component)]
pub struct DebugOverlay;

#[derive(Debug, Default, Resource)]
pub struct DebugOverlayState {
    visible: bool,
}

pub fn setup_hotbar(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                bottom: Val::Px(24.0),
                left: Val::Percent(50.0),
                width: Val::Px(520.0),
                height: Val::Px(58.0),
                margin: UiRect::left(Val::Px(-260.0)),
                display: Display::Flex,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
            background_color: Color::srgba(0.12, 0.08, 0.05, 0.50).into(),
            ..default()
        })
        .with_children(|parent| {
            for index in 0..9 {
                parent
                    .spawn((
                        HotbarSlot { index },
                        NodeBundle {
                            style: Style {
                                width: Val::Px(48.0),
                                height: Val::Px(48.0),
                                border: UiRect::all(Val::Px(2.0)),
                                display: Display::Flex,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            border_color: Color::srgba(0.95, 0.76, 0.42, 0.45).into(),
                            background_color: BlockType::HOTBAR[index].color(0.0, None).into(),
                            ..default()
                        },
                    ))
                    .with_children(|slot| {
                        slot.spawn(TextBundle::from_section(
                            (index + 1).to_string(),
                            TextStyle {
                                font_size: 13.0,
                                color: Color::srgb(0.08, 0.06, 0.04),
                                ..default()
                            },
                        ));
                    });
            }
        });

    commands.spawn((
        DebugOverlay,
        TextBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(44.0),
                display: Display::None,
                ..default()
            },
            text: Text::from_section(
                "",
                TextStyle {
                    font_size: 14.0,
                    color: Color::srgb(0.98, 0.92, 0.78),
                    ..default()
                },
            ),
            ..default()
        },
    ));
}

pub fn update_hotbar(
    selection: Res<HotbarSelection>,
    mut slots: Query<(&HotbarSlot, &mut BorderColor, &mut BackgroundColor)>,
) {
    if !selection.is_changed() {
        return;
    }
    for (slot, mut border, mut background) in &mut slots {
        let selected = slot.index == selection.slot;
        *border = if selected {
            Color::srgb(1.0, 0.86, 0.38).into()
        } else {
            Color::srgba(0.95, 0.76, 0.42, 0.45).into()
        };
        *background = BlockType::HOTBAR[slot.index]
            .color(if selected { 0.06 } else { 0.0 }, None)
            .into();
    }
}

pub fn toggle_debug_overlay(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<DebugOverlayState>) {
    if keys.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
    }
}

pub fn update_debug_overlay(
    state: Res<DebugOverlayState>,
    stats: Option<Res<WorldStats>>,
    world: Res<ChunkManager>,
    player: Query<&Transform, With<Player>>,
    mut overlay: Query<(&mut Style, &mut Text), With<DebugOverlay>>,
) {
    let Ok((mut style, mut text)) = overlay.get_single_mut() else {
        return;
    };
    style.display = if state.visible {
        Display::Flex
    } else {
        Display::None
    };
    if !state.visible {
        return;
    }
    let position = player
        .get_single()
        .map(|transform| transform.translation)
        .unwrap_or(Vec3::ZERO);
    let chunk = VoxelCoord::from_world_pos(position).chunk();
    let stats = stats.as_deref().copied().unwrap_or_default();
    let column = world.column_at(position.x.floor() as i32, position.z.floor() as i32);

    text.sections[0].value = format!(
        "Runewild F3\n\
         Pos: {:.1}, {:.1}, {:.1}\n\
         Chunk: [{}, {}]\n\
         Biome: {}\n\
         Continentalness: {:.2} | Erosion: {:.2}\n\
         Temp: {:.2} | Humidity: {:.2} | Ridges: {:.2}\n\
         Terrain Height: {} | Slope: {:.2}\n\
         River: {:.2} | Lake: {:.2} | Flow: {:.2}\n\
         Moisture: {:.2} | Ecotone: {:.2}\n\
         Chunks (Loaded: {}, Generated: {}, Visible: {})\n\
         Triangles: {} | Render: {:.0} m | Seed: {:#x}",
        position.x,
        position.y,
        position.z,
        chunk.x,
        chunk.z,
        column.biome.name(),
        column.climate.continentalness,
        column.climate.erosion,
        column.climate.temperature,
        column.climate.humidity,
        column.climate.ridges,
        column.height,
        column.slope,
        column.river_strength,
        column.lake_strength,
        column.flow_strength,
        column.process_moisture,
        column.ecotone_strength,
        stats.loaded_chunks,
        stats.generated_chunks,
        stats.visible_chunks,
        stats.triangles,
        stats.render_distance_m,
        stats.seed
    );
}
