use bevy::prelude::*;

use crate::{
    player::PlayerPlugin,
    rendering::RenderingPlugin,
    settings::WorldSettings,
    ui::UiPlugin,
    visual_test::VisualTestPlugin,
    world::{persistence::SaveGame, WorldPlugin},
};

pub const WORLD_SEED: u64 = 0x5255_4e45_5749_4c44;

/// Top-level application flow: title screen, live gameplay, pause overlay.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States)]
pub enum GameState {
    #[default]
    MainMenu,
    Playing,
    Paused,
    Settings,
}

pub fn run(visual_test: bool) {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.33, 0.60, 0.91)))
        .insert_resource(Msaa::Sample4)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Runewild".to_string(),
                resolution: (1280.0_f32, 720.0_f32).into(),
                // The screenshot harness measures real frame cost, so it
                // must not be throttled to the display refresh rate.
                present_mode: if visual_test {
                    bevy::window::PresentMode::AutoNoVsync
                } else {
                    bevy::window::PresentMode::AutoVsync
                },
                ..default()
            }),
            ..default()
        }))
        .insert_resource(SaveGame::load_or_default(WORLD_SEED))
        .insert_resource(WorldSettings::load_or_default())
        .init_state::<GameState>()
        .add_plugins((RenderingPlugin, WorldPlugin, PlayerPlugin, UiPlugin));
    if visual_test {
        app.add_plugins(VisualTestPlugin);
    }
    app.run();
}
