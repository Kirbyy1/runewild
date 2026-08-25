use crate::world::voxel::{BlockType, FaceDirection};
use bevy::prelude::*;

/// Number of tiles per row/column in the generated texture atlas.
pub const ATLAS_TILES_PER_ROW: u32 = 6;
pub const ATLAS_TILES_PER_COLUMN: u32 = 6;
/// Resolution of one generated tile. The painter works in 2x2 texel cells,
/// yielding a Minecraft-like 16x16 visual grid without excessive distant
/// shimmer.
pub const ATLAS_TILE_RESOLUTION: u32 = 32;

/// Named tile slots within the atlas grid, laid out left-to-right, top-to-bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    GrassTop,
    GrassSide,
    Dirt,
    Stone,
    Sand,
    Gravel,

    WoodTop,
    WoodSide,
    Leaves,
    BirchSide,
    PineLeaves,
    Water,

    SnowTop,
    SnowSide,
    MossStone,
    JungleWoodTop,
    JungleWoodSide,
    JungleLeaves,

    AutumnLeaves,
    PalmLeaves,
    SandstoneTop,
    SandstoneSide,
    Terracotta,
    Mud,

    Ice,
    CactusTop,
    CactusSide,

    GroundGrassTuft,
    GroundFlowersYellow,
    GroundFlowersWhite,
    GroundFlowersRed,
    GroundFern,
    GroundMushroom,
}

impl Tile {
    /// (column, row) position of this tile within the atlas grid.
    pub const fn coords(self) -> (u32, u32) {
        match self {
            Tile::GrassTop => (0, 0),
            Tile::GrassSide => (1, 0),
            Tile::Dirt => (2, 0),
            Tile::Stone => (3, 0),
            Tile::Sand => (4, 0),
            Tile::Gravel => (5, 0),

            Tile::WoodTop => (0, 1),
            Tile::WoodSide => (1, 1),
            Tile::Leaves => (2, 1),
            Tile::BirchSide => (3, 1),
            Tile::PineLeaves => (4, 1),
            Tile::Water => (5, 1),

            Tile::SnowTop => (0, 2),
            Tile::SnowSide => (1, 2),
            Tile::MossStone => (2, 2),
            Tile::JungleWoodTop => (3, 2),
            Tile::JungleWoodSide => (4, 2),
            Tile::JungleLeaves => (5, 2),

            Tile::AutumnLeaves => (0, 3),
            Tile::PalmLeaves => (1, 3),
            Tile::SandstoneTop => (2, 3),
            Tile::SandstoneSide => (3, 3),
            Tile::Terracotta => (4, 3),
            Tile::Mud => (5, 3),

            Tile::Ice => (0, 4),
            Tile::CactusTop => (1, 4),
            Tile::CactusSide => (2, 4),

            Tile::GroundGrassTuft => (0, 5),
            Tile::GroundFlowersYellow => (1, 5),
            Tile::GroundFlowersWhite => (2, 5),
            Tile::GroundFlowersRed => (3, 5),
            Tile::GroundFern => (4, 5),
            Tile::GroundMushroom => (5, 5),
        }
    }
}

/// Maps a block type + face direction to its atlas tile coordinates (column, row).
pub fn get_texture_coords(block: BlockType, face: Option<FaceDirection>) -> (u32, u32) {
    let tile = match block {
        BlockType::Air => Tile::Dirt,

        BlockType::Grass => match face {
            Some(FaceDirection::Top) => Tile::GrassTop,
            Some(FaceDirection::Bottom) => Tile::Dirt,
            _ => Tile::GrassSide,
        },

        BlockType::Dirt => Tile::Dirt,
        BlockType::Stone => Tile::Stone,
        BlockType::Sand => Tile::Sand,
        BlockType::Gravel => Tile::Gravel,

        BlockType::Wood => match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => Tile::WoodTop,
            _ => Tile::WoodSide,
        },

        BlockType::BirchWood => match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => Tile::WoodTop,
            _ => Tile::BirchSide,
        },

        BlockType::JungleWood => match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => Tile::JungleWoodTop,
            _ => Tile::JungleWoodSide,
        },

        BlockType::Leaves => Tile::Leaves,
        BlockType::PineLeaves => Tile::PineLeaves,
        BlockType::JungleLeaves => Tile::JungleLeaves,
        BlockType::AutumnLeaves => Tile::AutumnLeaves,
        BlockType::PalmLeaves => Tile::PalmLeaves,

        BlockType::Water => Tile::Water,
        BlockType::Ice => Tile::Ice,

        BlockType::Snow => match face {
            Some(FaceDirection::Top) => Tile::SnowTop,
            Some(FaceDirection::Bottom) => Tile::Stone,
            _ => Tile::SnowSide,
        },
        BlockType::MossStone => Tile::MossStone,

        BlockType::Sandstone => match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => Tile::SandstoneTop,
            _ => Tile::SandstoneSide,
        },

        BlockType::Terracotta => Tile::Terracotta,
        BlockType::Mud => Tile::Mud,

        BlockType::Cactus => match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => Tile::CactusTop,
            _ => Tile::CactusSide,
        },

        BlockType::GrassTuft => Tile::GroundGrassTuft,
        BlockType::FlowersYellow => Tile::GroundFlowersYellow,
        BlockType::FlowersWhite => Tile::GroundFlowersWhite,
        BlockType::FlowersRed => Tile::GroundFlowersRed,
        BlockType::Fern => Tile::GroundFern,
        BlockType::Mushroom => Tile::GroundMushroom,
    };

    tile.coords()
}

/// Texture atlas configuration
#[derive(Resource, Debug, Clone, Default)]
pub struct TextureAtlasConfig {
    pub texture_handle: Option<Handle<Image>>,
    pub water_texture_frames: Vec<Handle<Image>>,
    pub opaque_material: Option<Handle<StandardMaterial>>,
    pub foliage_material: Option<Handle<StandardMaterial>>,
    pub water_material: Option<Handle<StandardMaterial>>,
}
