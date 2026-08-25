//! Mob (mobile entity) system: passive animals and wandering creatures.
//!
//! Currently implements sheep that spawn on grassy terrain near the player
//! and wander procedurally, staying on the surface and avoiding cliffs
//! and water.

pub mod sheep;

use bevy::prelude::*;

use crate::game::GameState;

/// Plugin that registers all mob-related systems.
pub struct MobPlugin;

impl Plugin for MobPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), sheep::spawn_sheep)
            .add_systems(
                Update,
                (sheep::update_sheep, sheep::despawn_distant_sheep)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}
