use bevy::prelude::*;

use crate::game::GameState;

#[derive(Component)]
pub struct Crosshair;

pub fn setup_crosshair(mut commands: Commands) {
    commands
        .spawn((
            Crosshair,
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    margin: UiRect {
                        left: Val::Px(-9.0),
                        top: Val::Px(-9.0),
                        ..default()
                    },
                    ..default()
                },
                background_color: Color::NONE.into(),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Crosshair,
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left: Val::Px(8.0),
                        top: Val::Px(2.0),
                        width: Val::Px(2.0),
                        height: Val::Px(14.0),
                        ..default()
                    },
                    background_color: Color::srgba(1.0, 0.94, 0.78, 0.86).into(),
                    ..default()
                },
            ));
            parent.spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(2.0),
                    top: Val::Px(8.0),
                    width: Val::Px(14.0),
                    height: Val::Px(2.0),
                    ..default()
                },
                background_color: Color::srgba(1.0, 0.94, 0.78, 0.86).into(),
                ..default()
            });
        });
}

/// Hides the crosshair on menus and pause screens, shows it during play.
pub fn update_crosshair_visibility(
    state: Res<State<GameState>>,
    mut crosshairs: Query<&mut Style, With<Crosshair>>,
) {
    if !state.is_changed() {
        return;
    }
    let visible = state.get() == &GameState::Playing;
    for mut style in &mut crosshairs {
        style.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}
