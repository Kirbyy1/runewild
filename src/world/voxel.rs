use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockType {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
    Wood,
    Leaves,
    Water,
    Gravel,
    Snow,
    MossStone,
    BirchWood,
    PineLeaves,
    // New blocks for rich biomes
    JungleWood,
    JungleLeaves,
    AutumnLeaves,
    PalmLeaves,
    Sandstone,
    Terracotta,
    Mud,
    Ice,
    Cactus,
    // Ground plants: rendered as crossed cutout quads, never solid.
    GrassTuft,
    FlowersYellow,
    FlowersWhite,
    FlowersRed,
    Fern,
    Mushroom,
}

impl BlockType {
    pub fn is_plant(self) -> bool {
        matches!(
            self,
            BlockType::GrassTuft
                | BlockType::FlowersYellow
                | BlockType::FlowersWhite
                | BlockType::FlowersRed
                | BlockType::Fern
                | BlockType::Mushroom
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDirection {
    Top,
    Bottom,
    North,
    South,
    East,
    West,
}

impl BlockType {
    pub const HOTBAR: [BlockType; 9] = [
        BlockType::Grass,
        BlockType::Dirt,
        BlockType::Stone,
        BlockType::Sand,
        BlockType::Wood,
        BlockType::Leaves,
        BlockType::Water,
        BlockType::Gravel,
        BlockType::Snow,
    ];

    pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water) && !self.is_plant()
    }

    pub fn is_transparent(self) -> bool {
        matches!(
            self,
            BlockType::Air
                | BlockType::Leaves
                | BlockType::PineLeaves
                | BlockType::JungleLeaves
                | BlockType::AutumnLeaves
                | BlockType::PalmLeaves
                | BlockType::Cactus
                | BlockType::Water
                | BlockType::Ice
        ) || self.is_plant()
    }

    pub fn is_translucent(self) -> bool {
        matches!(self, BlockType::Water | BlockType::Ice)
    }

    /// Get block color with deterministic face-based variation
    pub fn color(self, variation: f32, face: Option<FaceDirection>) -> Color {
        let v = variation.clamp(-0.12, 0.12);
        match self {
            BlockType::Air => Color::NONE,
            BlockType::Grass => Self::grass_color(v, face),
            BlockType::Dirt => Self::dirt_color(v),
            BlockType::Stone => Self::stone_color(v),
            BlockType::Sand => Self::sand_color(v),
            BlockType::Wood => Self::wood_color(v, face),
            BlockType::Leaves => Self::leaves_color(v),
            BlockType::Water => Self::water_color(v),
            BlockType::Gravel => Color::srgb(0.48 + v, 0.46 + v, 0.42 + v),
            BlockType::Snow => Color::srgb(0.93 + v * 0.2, 0.97 + v * 0.15, 1.0),
            BlockType::MossStone => Color::srgb(0.44 + v, 0.56 + v, 0.38 + v),
            BlockType::BirchWood => match face {
                Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => {
                    Color::srgb(0.72 + v, 0.59 + v, 0.38 + v)
                }
                _ => Color::srgb(0.84 + v, 0.82 + v, 0.68 + v),
            },
            BlockType::PineLeaves => Color::srgba(
                (0.16 + v).clamp(0.0, 1.0),
                (0.48 + v).clamp(0.0, 1.0),
                (0.31 + v).clamp(0.0, 1.0),
                0.96,
            ),
            BlockType::JungleWood => match face {
                Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => {
                    Color::srgb(0.55 + v, 0.38 + v, 0.22 + v)
                }
                _ => Color::srgb(0.48 + v, 0.32 + v, 0.18 + v),
            },
            BlockType::JungleLeaves => Color::srgba(
                (0.12 + v * 0.5).clamp(0.0, 1.0),
                (0.72 + v * 0.6).clamp(0.0, 1.0),
                (0.20 + v * 0.4).clamp(0.0, 1.0),
                0.95,
            ),
            BlockType::AutumnLeaves => {
                // Warm amber, gold and crimson tones
                let shift = (v * 3.0).sin() * 0.15;
                Color::srgba(
                    (0.88 + v + shift).clamp(0.0, 1.0),
                    (0.46 + v * 0.8 - shift * 0.5).clamp(0.0, 1.0),
                    (0.14 + v * 0.3).clamp(0.0, 1.0),
                    0.95,
                )
            }
            BlockType::PalmLeaves => Color::srgba(
                (0.24 + v * 0.4).clamp(0.0, 1.0),
                (0.78 + v * 0.5).clamp(0.0, 1.0),
                (0.28 + v * 0.3).clamp(0.0, 1.0),
                0.94,
            ),
            BlockType::Sandstone => {
                let r = 0.86 + v * 0.25;
                let g = 0.72 + v * 0.22;
                let b = 0.46 + v * 0.18;
                Color::srgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
            }
            BlockType::Terracotta => {
                let r = 0.78 + v * 0.3;
                let g = 0.42 + v * 0.2;
                let b = 0.28 + v * 0.15;
                Color::srgb(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
            }
            BlockType::Mud => Color::srgb(
                (0.36 + v * 0.3).clamp(0.0, 1.0),
                (0.26 + v * 0.25).clamp(0.0, 1.0),
                (0.18 + v * 0.2).clamp(0.0, 1.0),
            ),
            BlockType::Ice => Color::srgba(
                (0.68 + v * 0.2).clamp(0.0, 1.0),
                (0.84 + v * 0.15).clamp(0.0, 1.0),
                (0.96 + v * 0.1).clamp(0.0, 1.0),
                0.82,
            ),
            BlockType::Cactus => Color::srgba(
                (0.28 + v * 0.3).clamp(0.0, 1.0),
                (0.62 + v * 0.4).clamp(0.0, 1.0),
                (0.22 + v * 0.2).clamp(0.0, 1.0),
                0.98,
            ),
            // Ground plants render from atlas tiles as crossed cutout quads;
            // these colors only back the rare non-textured fallback path.
            BlockType::GrassTuft => Color::srgba(0.36, 0.62, 0.24, 1.0),
            BlockType::FlowersYellow => Color::srgba(0.36, 0.62, 0.24, 1.0),
            BlockType::FlowersWhite => Color::srgba(0.36, 0.62, 0.24, 1.0),
            BlockType::FlowersRed => Color::srgba(0.36, 0.62, 0.24, 1.0),
            BlockType::Fern => Color::srgba(0.24, 0.52, 0.22, 1.0),
            BlockType::Mushroom => Color::srgba(0.82, 0.76, 0.66, 1.0),
        }
    }

    fn grass_color(v: f32, face: Option<FaceDirection>) -> Color {
        match face {
            Some(FaceDirection::Top) => {
                let base_r = 0.38 + v * 0.6;
                let base_g = 0.80 + v * 0.7;
                let base_b = 0.28 + v * 0.45;
                let blade_detail = (v * 2.0).sin().abs() * 0.15;
                Color::srgb(
                    (base_r + blade_detail * 0.07).clamp(0.0, 1.0),
                    (base_g + blade_detail * 0.10).clamp(0.0, 1.0),
                    (base_b + blade_detail * 0.05).clamp(0.0, 1.0),
                )
            }
            Some(FaceDirection::Bottom) => {
                let base_r = 0.35 + v * 0.25;
                let base_g = 0.48 + v * 0.25;
                let base_b = 0.22 + v * 0.25;
                Color::srgb(
                    base_r.clamp(0.0, 1.0),
                    base_g.clamp(0.0, 1.0),
                    base_b.clamp(0.0, 1.0),
                )
            }
            _ => {
                let base_r = 0.40 + v * 0.45;
                let base_g = 0.70 + v * 0.55;
                let base_b = 0.28 + v * 0.40;
                let stripe = ((v * 3.5).sin() * 0.5 + 0.5) * 0.18;
                Color::srgb(
                    (base_r + stripe * 0.06).clamp(0.0, 1.0),
                    (base_g + stripe * 0.07).clamp(0.0, 1.0),
                    (base_b + stripe * 0.05).clamp(0.0, 1.0),
                )
            }
        }
    }

    fn dirt_color(v: f32) -> Color {
        let base_r = 0.54 + v * 0.45;
        let base_g = 0.34 + v * 0.40;
        let base_b = 0.20 + v * 0.35;

        let stone_pattern = ((v * 2.5).sin() * 0.5 + 0.5) * 0.35;
        let stone_r = stone_pattern * 0.16;
        let stone_g = stone_pattern * 0.13;
        let stone_b = stone_pattern * 0.11;

        Color::srgb(
            (base_r + stone_r).clamp(0.0, 1.0),
            (base_g + stone_g).clamp(0.0, 1.0),
            (base_b + stone_b).clamp(0.0, 1.0),
        )
    }

    fn stone_color(v: f32) -> Color {
        let base_gray = 0.54 + v * 0.35;
        let rock_pattern = (v * 1.8).sin() * 0.5 + 0.5;
        let color_shift = rock_pattern * 0.12;

        let r_cool = (base_gray - color_shift * 0.3).clamp(0.0, 1.0);
        let g_cool = (base_gray - color_shift * 0.15).clamp(0.0, 1.0);
        let b_cool = (base_gray + color_shift * 0.4).clamp(0.0, 1.0);

        let r_warm = (base_gray + color_shift * 0.3).clamp(0.0, 1.0);
        let g_warm = (base_gray + color_shift * 0.1).clamp(0.0, 1.0);
        let b_warm = (base_gray - color_shift * 0.2).clamp(0.0, 1.0);

        let blend = (v.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        Color::srgb(
            r_cool * (1.0 - blend) + r_warm * blend,
            g_cool * (1.0 - blend) + g_warm * blend,
            b_cool * (1.0 - blend) + b_warm * blend,
        )
    }

    fn sand_color(v: f32) -> Color {
        let base_r = 0.92 + v * 0.35;
        let base_g = 0.80 + v * 0.30;
        let base_b = 0.50 + v * 0.25;
        let grain = ((v * 3.2).sin() * 0.5 + 0.5) * 0.10;

        Color::srgb(
            (base_r + grain * 0.05).clamp(0.0, 1.0),
            (base_g + grain * 0.04).clamp(0.0, 1.0),
            (base_b + grain * 0.03).clamp(0.0, 1.0),
        )
    }

    fn wood_color(v: f32, face: Option<FaceDirection>) -> Color {
        match face {
            Some(FaceDirection::Top) | Some(FaceDirection::Bottom) => {
                let base_r = 0.68 + v * 0.55;
                let base_g = 0.45 + v * 0.45;
                let base_b = 0.26 + v * 0.35;
                let ring_pattern = ((v * 4.0).sin().abs() * 0.5 + 0.5) * 0.30;

                Color::srgb(
                    (base_r + ring_pattern * 0.14).clamp(0.0, 1.0),
                    (base_g + ring_pattern * 0.10).clamp(0.0, 1.0),
                    (base_b + ring_pattern * 0.05).clamp(0.0, 1.0),
                )
            }
            _ => {
                let base_r = 0.60 + v * 0.45;
                let base_g = 0.40 + v * 0.35;
                let base_b = 0.22 + v * 0.30;
                let groove = ((v * 5.0).sin() * 0.5 + 0.5) * 0.25;
                let shadow = ((v * 2.8).cos().abs() - 0.5) * 0.18;

                Color::srgb(
                    (base_r + groove * 0.10 + shadow * 0.05).clamp(0.0, 1.0),
                    (base_g + groove * 0.07 + shadow * 0.04).clamp(0.0, 1.0),
                    (base_b + groove * 0.05 + shadow * 0.03).clamp(0.0, 1.0),
                )
            }
        }
    }

    fn leaves_color(v: f32) -> Color {
        let base_r = 0.22 + v * 0.45;
        let base_g = 0.76 + v * 0.55;
        let base_b = 0.26 + v * 0.40;

        let vein_major = ((v * 2.5).sin().abs() - 0.3).abs() * 0.5;
        let vein_minor = (v * 5.8).sin().abs() * 0.35;
        let veining = vein_major + vein_minor * 0.5;
        let shadow = ((v * 1.6).sin() * 0.5 + 0.5) * 0.18;

        Color::srgba(
            (base_r - shadow * 0.12 - veining * 0.06).clamp(0.0, 1.0),
            (base_g - veining * 0.10).clamp(0.0, 1.0),
            (base_b - shadow * 0.10 - veining * 0.08).clamp(0.0, 1.0),
            0.93,
        )
    }

    fn water_color(v: f32) -> Color {
        let base_r = 0.26 + v * 0.28;
        let base_g = 0.62 + v * 0.35;
        let base_b = 0.94 + v * 0.15;

        let wave_major = ((v * 3.5).sin() * 0.5 + 0.5) * 0.18;
        let wave_minor = (v * 6.2).sin().abs() * 0.10;

        Color::srgba(
            (base_r + wave_major * 0.10).clamp(0.0, 1.0),
            (base_g + wave_major * 0.12 + wave_minor * 0.05).clamp(0.0, 1.0),
            (base_b - wave_minor * 0.06).clamp(0.0, 1.0),
            0.78,
        )
    }
}
