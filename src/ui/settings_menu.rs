//! Settings screen shared by the title and pause menus.

use bevy::prelude::*;

use crate::{game::GameState, settings::WorldSettings};

#[derive(Debug, Clone, Copy, Default, Resource)]
pub enum SettingsOrigin {
    #[default]
    MainMenu,
    Paused,
}

impl SettingsOrigin {
    pub fn return_state(self) -> GameState {
        match self {
            Self::MainMenu => GameState::MainMenu,
            Self::Paused => GameState::Paused,
        }
    }
}

#[derive(Component)]
pub struct SettingsMenuRoot;

#[derive(Component)]
pub struct RenderDistanceValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsAction {
    DecreaseRenderDistance,
    IncreaseRenderDistance,
    Back,
}

#[derive(Component)]
pub struct SettingsButton {
    action: SettingsAction,
}

fn button_normal() -> Color {
    Color::srgba(0.19, 0.15, 0.09, 0.95)
}

fn button_hovered() -> Color {
    Color::srgba(0.33, 0.25, 0.13, 0.98)
}

fn text_primary() -> Color {
    Color::srgb(0.96, 0.90, 0.74)
}

fn text_muted() -> Color {
    Color::srgba(0.82, 0.79, 0.68, 0.78)
}

fn border_gold() -> Color {
    Color::srgba(0.95, 0.76, 0.42, 0.55)
}

pub fn spawn_settings_menu(mut commands: Commands, settings: Res<WorldSettings>) {
    commands
        .spawn((
            SettingsMenuRoot,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                background_color: Color::srgba(0.02, 0.03, 0.02, 0.72).into(),
                ..default()
            },
        ))
        .with_children(|overlay| {
            overlay
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Px(460.0),
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(18.0),
                        padding: UiRect::all(Val::Px(30.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    border_color: border_gold().into(),
                    background_color: Color::srgba(0.10, 0.08, 0.05, 0.97).into(),
                    ..default()
                })
                .with_children(|panel| {
                    panel.spawn(TextBundle::from_section(
                        "SETTINGS",
                        TextStyle {
                            font_size: 42.0,
                            color: text_primary(),
                            ..default()
                        },
                    ));

                    panel
                        .spawn(NodeBundle {
                            style: Style {
                                width: Val::Percent(100.0),
                                display: Display::Flex,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                column_gap: Val::Px(16.0),
                                padding: UiRect::vertical(Val::Px(12.0)),
                                ..default()
                            },
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn(TextBundle::from_section(
                                "Render distance",
                                TextStyle {
                                    font_size: 20.0,
                                    color: text_primary(),
                                    ..default()
                                },
                            ));
                            row.spawn(NodeBundle {
                                style: Style {
                                    display: Display::Flex,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    ..default()
                                },
                                ..default()
                            })
                            .with_children(|stepper| {
                                stepper_button(
                                    stepper,
                                    SettingsAction::DecreaseRenderDistance,
                                    "-",
                                );
                                stepper.spawn((
                                    RenderDistanceValue,
                                    TextBundle::from_section(
                                        distance_label(&settings),
                                        TextStyle {
                                            font_size: 18.0,
                                            color: text_muted(),
                                            ..default()
                                        },
                                    )
                                    .with_style(Style {
                                        width: Val::Px(108.0),
                                        ..default()
                                    })
                                    .with_text_justify(JustifyText::Center),
                                ));
                                stepper_button(
                                    stepper,
                                    SettingsAction::IncreaseRenderDistance,
                                    "+",
                                );
                            });
                        });

                    menu_button(panel, SettingsAction::Back, "Back");
                });
        });
}

fn distance_label(settings: &WorldSettings) -> String {
    format!("{:.0} m", settings.render_distance_m())
}

fn stepper_button(parent: &mut ChildBuilder, action: SettingsAction, label: &str) {
    button(parent, action, label, 44.0, 44.0, 25.0);
}

fn menu_button(parent: &mut ChildBuilder, action: SettingsAction, label: &str) {
    button(parent, action, label, 260.0, 46.0, 21.0);
}

fn button(
    parent: &mut ChildBuilder,
    action: SettingsAction,
    label: &str,
    width: f32,
    height: f32,
    font_size: f32,
) {
    parent
        .spawn((
            SettingsButton { action },
            ButtonBundle {
                style: Style {
                    width: Val::Px(width),
                    height: Val::Px(height),
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                border_color: border_gold().into(),
                background_color: button_normal().into(),
                ..default()
            },
        ))
        .with_children(|button| {
            button.spawn(TextBundle::from_section(
                label,
                TextStyle {
                    font_size,
                    color: text_primary(),
                    ..default()
                },
            ));
        });
}

pub fn settings_menu_interactions(
    mut next_state: ResMut<NextState<GameState>>,
    origin: Res<SettingsOrigin>,
    mut settings: ResMut<WorldSettings>,
    mut values: Query<&mut Text, With<RenderDistanceValue>>,
    mut buttons: Query<(&Interaction, &SettingsButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, button, mut background) in &mut buttons {
        *background = if *interaction == Interaction::None {
            button_normal()
        } else {
            button_hovered()
        }
        .into();

        if *interaction != Interaction::Pressed {
            continue;
        }
        let changed = match button.action {
            SettingsAction::DecreaseRenderDistance => settings.change_render_distance(-1),
            SettingsAction::IncreaseRenderDistance => settings.change_render_distance(1),
            SettingsAction::Back => {
                next_state.set(origin.return_state());
                false
            }
        };
        if changed {
            settings.write_to_disk();
            if let Ok(mut value) = values.get_single_mut() {
                value.sections[0].value = distance_label(&settings);
            }
        }
    }
}

pub fn settings_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    origin: Res<SettingsOrigin>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_state.set(origin.return_state());
    }
}

pub fn despawn_settings_menu(mut commands: Commands, root: Query<Entity, With<SettingsMenuRoot>>) {
    for entity in root.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{MAX_RENDER_DISTANCE_SECTIONS, MIN_RENDER_DISTANCE_SECTIONS};

    #[test]
    fn settings_return_to_the_menu_that_opened_them() {
        assert_eq!(SettingsOrigin::MainMenu.return_state(), GameState::MainMenu);
        assert_eq!(SettingsOrigin::Paused.return_state(), GameState::Paused);
    }

    #[test]
    fn render_distance_label_is_in_metres() {
        let settings = WorldSettings::default();
        assert_eq!(distance_label(&settings), "160 m");
        assert!(settings.render_distance_sections() >= MIN_RENDER_DISTANCE_SECTIONS);
        assert!(settings.render_distance_sections() <= MAX_RENDER_DISTANCE_SECTIONS);
    }
}
