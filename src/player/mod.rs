pub mod controller;
pub mod interaction;

use bevy::prelude::*;

use crate::game::GameState;

use self::{
    controller::{
        apply_render_settings, clear_click_suppression, on_enter_playing, player_controller,
        release_cursor, setup_player, CursorState,
    },
    interaction::{block_interaction, update_block_highlight, HotbarSelection, TargetedBlock},
};

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct PlayerCamera;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorState>()
            .init_resource::<HotbarSelection>()
            .init_resource::<TargetedBlock>()
            .add_systems(Startup, setup_player)
            .add_systems(OnEnter(GameState::Playing), on_enter_playing)
            .add_systems(OnEnter(GameState::Paused), release_cursor)
            .add_systems(OnEnter(GameState::Settings), release_cursor)
            .add_systems(OnEnter(GameState::MainMenu), release_cursor)
            .add_systems(PostUpdate, clear_click_suppression)
            .add_systems(Update, apply_render_settings)
            .add_systems(
                Update,
                (player_controller, update_block_highlight, block_interaction)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
