//! Pause overlay shown whenever gameplay is suspended with `Esc`.

use bevy::prelude::*;

use crate::{game::GameState, ui::settings_menu::SettingsOrigin, world::persistence::SaveGame};

#[derive(Component)]
pub struct PauseMenuRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseAction {
    Resume,
    Settings,
    SaveAndTitle,
    Quit,
}

#[derive(Component)]
pub struct PauseButton {
    action: PauseAction,
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

fn text_muted(alpha: f32) -> Color {
    Color::srgba(0.82, 0.79, 0.68, alpha)
}

fn border_gold() -> Color {
    Color::srgba(0.95, 0.76, 0.42, 0.55)
}

pub fn spawn_pause_menu(mut commands: Commands) {
    commands
        .spawn((
            PauseMenuRoot,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    display: Display::Flex,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                background_color: Color::srgba(0.02, 0.03, 0.02, 0.62).into(),
                ..default()
            },
        ))
        .with_children(|overlay| {
            overlay
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Px(400.0),
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(10.0),
                        padding: UiRect::all(Val::Px(28.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    border_color: border_gold().into(),
                    background_color: Color::srgba(0.10, 0.08, 0.05, 0.97).into(),
                    ..default()
                })
                .with_children(|panel| {
                    panel.spawn(TextBundle::from_section(
                        "PAUSED",
                        TextStyle {
                            font_size: 44.0,
                            color: text_primary(),
                            ..default()
                        },
                    ));
                    panel.spawn(TextBundle::from_section(
                        "The world holds its breath...",
                        TextStyle {
                            font_size: 16.0,
                            color: text_muted(0.85),
                            ..default()
                        },
                    ));

                    spacer(panel, 14.0);
                    pause_button(panel, PauseAction::Resume, "Resume");
                    pause_button(panel, PauseAction::Settings, "Settings");
                    pause_button(panel, PauseAction::SaveAndTitle, "Save & Main Menu");
                    pause_button(panel, PauseAction::Quit, "Quit Game");

                    spacer(panel, 10.0);
                    panel.spawn(TextBundle::from_section(
                        "Esc resumes  ·  autosaves every 30 s",
                        TextStyle {
                            font_size: 13.0,
                            color: text_muted(0.65),
                            ..default()
                        },
                    ));
                });
        });
}

// --- interaction and helpers below ---

fn spacer(parent: &mut ChildBuilder, height: f32) {
    parent.spawn(NodeBundle {
        style: Style {
            height: Val::Px(height),
            ..default()
        },
        ..default()
    });
}

fn pause_button(parent: &mut ChildBuilder, action: PauseAction, label: &str) {
    parent
        .spawn((
            PauseButton { action },
            ButtonBundle {
                style: Style {
                    width: Val::Px(300.0),
                    height: Val::Px(46.0),
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
                    font_size: 21.0,
                    color: text_primary(),
                    ..default()
                },
            ));
        });
}

pub fn pause_menu_interactions(
    mut next_state: ResMut<NextState<GameState>>,
    mut settings_origin: ResMut<SettingsOrigin>,
    mut exit: EventWriter<AppExit>,
    save: ResMut<SaveGame>,
    mut buttons: Query<(&Interaction, &PauseButton, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, button, mut background) in &mut buttons {
        *background = if *interaction == Interaction::None {
            button_normal()
        } else {
            button_hovered()
        }
        .into();

        if *interaction == Interaction::Pressed {
            match button.action {
                PauseAction::Resume => next_state.set(GameState::Playing),
                PauseAction::Settings => {
                    *settings_origin = SettingsOrigin::Paused;
                    next_state.set(GameState::Settings);
                }
                PauseAction::SaveAndTitle => {
                    save.write_to_disk();
                    info!("World saved; returning to main menu");
                    next_state.set(GameState::MainMenu);
                }
                PauseAction::Quit => {
                    save.write_to_disk();
                    exit.send(AppExit::Success);
                }
            }
        }
    }
}

/// Esc while paused resumes play without touching the mouse.
pub fn pause_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_state.set(GameState::Playing);
    }
}

pub fn despawn_pause_menu(mut commands: Commands, root: Query<Entity, With<PauseMenuRoot>>) {
    for entity in root.iter() {
        commands.entity(entity).despawn_recursive();
    }
}
