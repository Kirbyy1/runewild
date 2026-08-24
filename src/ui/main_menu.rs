//! Title screen shown at startup and whenever the player returns from pause.

use bevy::prelude::*;

use crate::{game::GameState, ui::settings_menu::SettingsOrigin};

#[derive(Component)]
pub struct MainMenuRoot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Play,
    Settings,
    Quit,
}

#[derive(Component)]
pub struct MenuButton {
    action: MenuAction,
}

fn button_normal() -> Color {
    Color::srgba(0.16, 0.12, 0.07, 0.92)
}

fn button_hovered() -> Color {
    Color::srgba(0.32, 0.24, 0.12, 0.96)
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

/// Spawns the title screen. Runs at startup (initial state is `MainMenu`) and
/// again whenever the state re-enters `MainMenu`.
pub fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuRoot,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                background_color: Color::srgba(0.03, 0.05, 0.03, 0.72).into(),
                ..default()
            },
        ))
        .with_children(|menu| {
            menu.spawn(TextBundle::from_section(
                "RUNEWILD",
                TextStyle {
                    font_size: 84.0,
                    color: text_primary(),
                    ..default()
                },
            ));
            menu.spawn(TextBundle::from_section(
                "A first-person fantasy voxel sandbox",
                TextStyle {
                    font_size: 20.0,
                    color: text_muted(0.9),
                    ..default()
                },
            ));

            spacer(menu, 30.0);
            menu_button(menu, MenuAction::Play, "Play");
            menu_button(menu, MenuAction::Settings, "Settings");
            menu_button(menu, MenuAction::Quit, "Quit");

            spacer(menu, 22.0);
            menu.spawn(TextBundle::from_section(
                "Left-click in game to capture the mouse  ·  Esc opens the pause menu",
                TextStyle {
                    font_size: 14.0,
                    color: text_muted(0.65),
                    ..default()
                },
            ));
        });
}

fn spacer(parent: &mut ChildBuilder, height: f32) {
    parent.spawn(NodeBundle {
        style: Style {
            height: Val::Px(height),
            ..default()
        },
        ..default()
    });
}

fn menu_button(parent: &mut ChildBuilder, action: MenuAction, label: &str) {
    parent
        .spawn((
            MenuButton { action },
            ButtonBundle {
                style: Style {
                    width: Val::Px(250.0),
                    height: Val::Px(52.0),
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
                    font_size: 24.0,
                    color: text_primary(),
                    ..default()
                },
            ));
        });
}

pub fn main_menu_interactions(
    mut next_state: ResMut<NextState<GameState>>,
    mut settings_origin: ResMut<SettingsOrigin>,
    mut exit: EventWriter<AppExit>,
    mut buttons: Query<
        (
            &Interaction,
            &MenuButton,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        Changed<Interaction>,
    >,
) {
    for (interaction, button, mut background, mut border) in &mut buttons {
        let hovered = matches!(interaction, Interaction::Hovered | Interaction::Pressed);
        *background = if hovered {
            button_hovered()
        } else {
            button_normal()
        }
        .into();
        *border = if hovered {
            Color::srgba(1.0, 0.86, 0.38, 0.85)
        } else {
            border_gold()
        }
        .into();

        if *interaction == Interaction::Pressed {
            match button.action {
                MenuAction::Play => next_state.set(GameState::Playing),
                MenuAction::Settings => {
                    *settings_origin = SettingsOrigin::MainMenu;
                    next_state.set(GameState::Settings);
                }
                MenuAction::Quit => {
                    exit.send(AppExit::Success);
                }
            }
        }
    }
}

pub fn despawn_main_menu(mut commands: Commands, root: Query<Entity, With<MainMenuRoot>>) {
    for entity in root.iter() {
        commands.entity(entity).despawn_recursive();
    }
}
