use crate::world::voxel::BlockType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Biome {
    DeepOcean,
    Ocean,
    Beach,
    TropicalBeach,
    Plains,
    Meadow,
    Forest,
    DenseForest,
    AutumnForest,
    Rainforest,
    Savanna,
    Desert,
    Badlands,
    Swamp,
    Taiga,
    SnowyForest,
    Tundra,
    Mountains,
}

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Biome::DeepOcean => "Deep Ocean",
            Biome::Ocean => "Ocean",
            Biome::Beach => "Beach",
            Biome::TropicalBeach => "Tropical Beach",
            Biome::Plains => "Plains",
            Biome::Meadow => "Meadow",
            Biome::Forest => "Forest",
            Biome::DenseForest => "Dense Forest",
            Biome::AutumnForest => "Autumn Forest",
            Biome::Rainforest => "Tropical Rainforest",
            Biome::Savanna => "Savanna",
            Biome::Desert => "Desert",
            Biome::Badlands => "Badlands",
            Biome::Swamp => "Swamp",
            Biome::Taiga => "Taiga",
            Biome::SnowyForest => "Snowy Forest",
            Biome::Tundra => "Tundra",
            Biome::Mountains => "Alpine Mountains",
        }
    }

    pub fn surface_block(self) -> BlockType {
        match self {
            Biome::DeepOcean | Biome::Ocean => BlockType::Gravel,
            Biome::Beach | Biome::TropicalBeach => BlockType::Sand,
            Biome::Desert => BlockType::Sand,
            Biome::Badlands => BlockType::Terracotta,
            Biome::Swamp => BlockType::Mud,
            Biome::SnowyForest | Biome::Tundra => BlockType::Snow,
            Biome::Mountains => BlockType::Stone,
            _ => BlockType::Grass,
        }
    }

    pub fn subsurface_block(self) -> BlockType {
        match self {
            Biome::DeepOcean | Biome::Ocean => BlockType::Gravel,
            Biome::Beach | Biome::TropicalBeach => BlockType::Sand,
            Biome::Desert => BlockType::Sandstone,
            Biome::Badlands => BlockType::Terracotta,
            Biome::Swamp => BlockType::Mud,
            Biome::SnowyForest => BlockType::Dirt,
            Biome::Tundra | Biome::Mountains => BlockType::Stone,
            _ => BlockType::Dirt,
        }
    }

    pub fn is_aquatic(self) -> bool {
        matches!(self, Biome::DeepOcean | Biome::Ocean)
    }

    pub fn is_frozen(self) -> bool {
        matches!(self, Biome::SnowyForest | Biome::Tundra)
    }
}
