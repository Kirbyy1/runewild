pub mod chat;
pub mod crosshair;
pub mod fps;
pub mod hotbar;
pub mod main_menu;
pub mod pause_menu;
pub mod settings_menu;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;

use crate::game::GameState;

use self::{
    chat::{handle_chat_input, setup_chat, update_chat_ui, ChatState},
    crosshair::{setup_crosshair, update_crosshair_visibility},
    fps::{setup_fps_counter, update_fps_counter},
    hotbar::{setup_hotbar, toggle_debug_overlay, update_debug_overlay, update_hotbar},
    main_menu::{despawn_main_menu, main_menu_interactions, spawn_main_menu},
    pause_menu::{despawn_pause_menu, pause_keyboard, pause_menu_interactions, spawn_pause_menu},
    settings_menu::{
        despawn_settings_menu, settings_keyboard, settings_menu_interactions, spawn_settings_menu,
        SettingsOrigin,
    },
};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .init_resource::<hotbar::DebugOverlayState>()
            .init_resource::<ChatState>()
            .init_resource::<SettingsOrigin>()
            .add_systems(
                Startup,
                (setup_crosshair, setup_fps_counter, setup_hotbar, setup_chat),
            )
            // Title screen: spawned at startup and on every re-entry.
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_main_menu)
            .add_systems(
                Update,
                main_menu_interactions.run_if(in_state(GameState::MainMenu)),
            )
            // Pause overlay.
            .add_systems(OnEnter(GameState::Paused), spawn_pause_menu)
            .add_systems(OnExit(GameState::Paused), despawn_pause_menu)
            .add_systems(
                Update,
                (pause_menu_interactions, pause_keyboard).run_if(in_state(GameState::Paused)),
            )
            .add_systems(OnEnter(GameState::Settings), spawn_settings_menu)
            .add_systems(OnExit(GameState::Settings), despawn_settings_menu)
            .add_systems(
                Update,
                (settings_menu_interactions, settings_keyboard)
                    .run_if(in_state(GameState::Settings)),
            )
            // HUD and Chat systems.
            .add_systems(
                Update,
                (
                    update_fps_counter,
                    update_hotbar,
                    update_debug_overlay,
                    update_crosshair_visibility,
                    toggle_debug_overlay.run_if(in_state(GameState::Playing)),
                    handle_chat_input.run_if(in_state(GameState::Playing)),
                    update_chat_ui,
                ),
            );
    }
}
