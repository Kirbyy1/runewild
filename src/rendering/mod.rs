pub mod atmosphere;
pub mod materials;
pub mod texture_atlas;
pub mod texture_loader;

use bevy::prelude::*;

use self::atmosphere::{drift_clouds, setup_atmosphere, setup_clouds};
use self::texture_atlas::TextureAtlasConfig;
use self::texture_loader::{animate_water_texture, load_or_create_atlas};

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextureAtlasConfig>()
            .add_systems(
                Startup,
                (setup_atmosphere, setup_clouds, load_or_create_atlas),
            )
            .add_systems(Update, (animate_water_texture, drift_clouds));
    }
}
