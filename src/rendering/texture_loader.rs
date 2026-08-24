use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension};
use bevy::render::texture::{
    ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor,
};
use noise::{NoiseFn, OpenSimplex};

use super::texture_atlas::{
    Tile, ATLAS_TILES_PER_COLUMN, ATLAS_TILES_PER_ROW, ATLAS_TILE_RESOLUTION,
};

type Rgb = (u8, u8, u8);

fn tile_seed(index: u32) -> u32 {
    index.wrapping_mul(0x9E3779B1).wrapping_add(0xB5297A4D)
}

fn lerp_color(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t).round() as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t).round() as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t).round() as u8,
    )
}

fn shade(c: Rgb, amount: f32) -> Rgb {
    let f = |v: u8| ((v as f32) * amount).clamp(0.0, 255.0) as u8;
    (f(c.0), f(c.1), f(c.2))
}

fn posterize(c: Rgb) -> Rgb {
    let q = |value: u8| ((value as u16 / 8) * 8 + 4).min(255) as u8;
    (q(c.0), q(c.1), q(c.2))
}

fn paint_tile(
    pixels: &mut [u8],
    atlas_width: u32,
    size: u32,
    col: u32,
    row: u32,
    mut paint: impl FnMut(u32, u32) -> Rgb,
) {
    let origin_x = col * size;
    let origin_y = row * size;
    for y in 0..size {
        for x in 0..size {
            let (r, g, b) = posterize(paint(x / 2 * 2, y / 2 * 2));
            let px = origin_x + x;
            let py = origin_y + y;
            let idx = ((py * atlas_width + px) * 4) as usize;
            pixels[idx] = r;
            pixels[idx + 1] = g;
            pixels[idx + 2] = b;
            pixels[idx + 3] = 255;
        }
    }
}

fn paint_tile_alpha(
    pixels: &mut [u8],
    atlas_width: u32,
    size: u32,
    col: u32,
    row: u32,
    mut alpha: impl FnMut(u32, u32) -> u8,
) {
    for y in 0..size {
        for x in 0..size {
            let px = col * size + x;
            let py = row * size + y;
            pixels[((py * atlas_width + px) * 4 + 3) as usize] = alpha(x, y);
        }
    }
}

fn fbm(noise: &OpenSimplex, x: f64, y: f64, scale: f64) -> f64 {
    let a = noise.get([x / scale, y / scale]);
    let b = noise.get([x / (scale * 0.35) + 91.7, y / (scale * 0.35) + 17.3]) * 0.5;
    (a + b) / 1.5
}

fn grass_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let blade_noise = OpenSimplex::new(seed.wrapping_add(1));
    let base_dark: Rgb = (36, 82, 30);
    let base_light: Rgb = (66, 122, 48);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.28);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let blade = blade_noise.get([x as f64 / 2.4, y as f64 / 9.0]);
        if blade > 0.55 {
            c = shade(c, 1.18);
        } else if blade < -0.6 {
            c = shade(c, 0.85);
        }
        c
    }
}

fn dirt_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let pebble_noise = OpenSimplex::new(seed.wrapping_add(2));
    let base_dark: Rgb = (84, 56, 34);
    let base_light: Rgb = (128, 90, 56);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.3);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let pebble = pebble_noise.get([x as f64 / 5.0, y as f64 / 5.0]);
        if pebble > 0.65 {
            c = shade(c, 0.72);
        }
        c
    }
}

fn grass_side_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut grass = grass_top_tile(size, seed);
    let mut dirt = dirt_tile(size, seed.wrapping_add(3));
    let grass_band = (size as f32 * 0.28) as u32;
    let transition = (size as f32 * 0.10) as u32;
    move |x, y| {
        if y < grass_band {
            grass(x, y)
        } else if y < grass_band + transition {
            let t = (y - grass_band) as f32 / transition.max(1) as f32;
            lerp_color(grass(x, y), dirt(x, y), t)
        } else {
            dirt(x, y)
        }
    }
}

fn stone_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let crack_noise = OpenSimplex::new(seed.wrapping_add(4));
    let base_dark: Rgb = (90, 92, 96);
    let base_light: Rgb = (150, 152, 156);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let crack = crack_noise.get([x as f64 / 6.5, y as f64 / 6.5]).abs();
        if crack < 0.035 {
            c = shade(c, 0.55);
        }
        c
    }
}

fn sand_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let grain_noise = OpenSimplex::new(seed.wrapping_add(5));
    let base_dark: Rgb = (196, 168, 110);
    let base_light: Rgb = (232, 206, 148);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.32);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let grain = grain_noise.get([x as f64 / 1.6, y as f64 / 1.6]);
        if grain > 0.7 {
            c = shade(c, 1.12);
        }
        c
    }
}

fn wood_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed.wrapping_add(6));
    let base_dark: Rgb = (86, 58, 34);
    let base_light: Rgb = (156, 112, 68);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let dist = (dx * dx + dy * dy).sqrt();
        let wobble = ring_noise.get([x as f64 / 10.0, y as f64 / 10.0]) as f32 * 2.5;
        let ring = ((dist + wobble) * 0.9).sin() * 0.5 + 0.5;
        lerp_color(base_dark, base_light, ring)
    }
}

fn wood_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let bark_noise = OpenSimplex::new(seed.wrapping_add(7));
    let base_dark: Rgb = (66, 42, 26);
    let base_light: Rgb = (118, 82, 48);
    move |x, y| {
        let groove = ((x as f64 / 4.2).sin() * 0.5 + 0.5) as f32;
        let n = bark_noise.get([x as f64 / 3.0, y as f64 / 14.0]) as f32 * 0.5 + 0.5;
        let t = (groove * 0.65 + n * 0.35).clamp(0.0, 1.0);
        lerp_color(base_dark, base_light, t)
    }
}

fn leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let hole_noise = OpenSimplex::new(seed.wrapping_add(8));
    let base_dark: Rgb = (18, 58, 25);
    let base_light: Rgb = (44, 104, 42);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let hole = hole_noise.get([x as f64 / 4.5, y as f64 / 4.5]);
        if hole > 0.72 {
            c = shade(c, 0.6);
        }
        c
    }
}

fn water_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    water_tile_frame(size, seed, 0.0)
}

fn water_tile_frame(size: u32, seed: u32, phase: f32) -> impl FnMut(u32, u32) -> Rgb {
    let wave_noise = OpenSimplex::new(seed.wrapping_add(9));
    let base_dark: Rgb = (12, 66, 142);
    let base_light: Rgb = (42, 132, 194);
    move |x, y| {
        // Advancing the sample in V makes the same atlas tile ripple across
        // horizontal surfaces and stream downward on vertical waterfall faces.
        let flowing_y = (y as f32 + phase) % size as f32;
        let n = fbm(&wave_noise, x as f64, flowing_y as f64, size as f64 * 0.4);
        let diagonal = ((x as f32 * 0.42 + flowing_y * 0.24).sin() * 0.5 + 0.5) * 0.22;
        let crossing = ((x as f32 * 0.17 - flowing_y * 0.31).sin() * 0.5 + 0.5) * 0.10;
        let mut color = lerp_color(
            base_dark,
            base_light,
            (n as f32 * 0.32 + 0.48 + diagonal + crossing).clamp(0.0, 1.0),
        );
        if (diagonal + crossing) > 0.27 && (x + y + seed).is_multiple_of(7) {
            color = lerp_color(color, (148, 220, 228), 0.28);
        }
        color
    }
}

fn gravel_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let stones: [Rgb; 5] = [
        (82, 82, 78),
        (112, 105, 94),
        (132, 126, 112),
        (91, 102, 104),
        (151, 137, 112),
    ];
    move |x, y| {
        let cell_x = x / 8;
        let cell_y = y / 7;
        let index = ((cell_x * 13 + cell_y * 29 + seed) % stones.len() as u32) as usize;
        let edge = (x % 8 == 0 || y % 7 == 0) as u8;
        let n = noise.get([
            x as f64 / (size as f64 * 0.18),
            y as f64 / (size as f64 * 0.18),
        ]);
        shade(
            stones[index],
            if edge == 1 {
                0.72
            } else {
                0.9 + n as f32 * 0.12
            },
        )
    }
}

fn snow_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let drifts = OpenSimplex::new(seed);
    move |x, y| {
        let n = fbm(&drifts, x as f64, y as f64, size as f64 * 0.34) as f32;
        let sparkle = (x * 17 + y * 31 + seed).is_multiple_of(97) as u8;
        if sparkle == 1 {
            (255, 255, 255)
        } else {
            lerp_color((184, 211, 225), (244, 249, 247), n * 0.5 + 0.55)
        }
    }
}

fn snow_side_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut snow = snow_tile(size, seed);
    let mut stone = stone_tile(size, seed.wrapping_add(19));
    let cap = size / 4;
    move |x, y| {
        let drip = ((x * 11 + seed) % 9).min(3);
        if y < cap + drip {
            snow(x, y)
        } else {
            stone(x, y)
        }
    }
}

fn moss_stone_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut stone = stone_tile(size, seed);
    let moss = OpenSimplex::new(seed.wrapping_add(23));
    move |x, y| {
        let rock = stone(x, y);
        let patch = moss.get([x as f64 / 13.0, y as f64 / 10.0]);
        if patch + (1.0 - y as f64 / size as f64) * 0.35 > 0.32 {
            lerp_color(rock, (74, 118, 58), 0.58)
        } else {
            rock
        }
    }
}

fn birch_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let grain = OpenSimplex::new(seed);
    move |x, y| {
        let n = grain.get([x as f64 / 9.0, y as f64 / 18.0]) as f32;
        let scar = (y % 13 <= 1 && (x + y + seed) % 11 < 5) || (x % 31 == 0);
        if scar {
            (58, 55, 49)
        } else {
            lerp_color((166, 164, 145), (226, 220, 190), n * 0.5 + 0.55)
        }
    }
}

fn pine_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let needles = OpenSimplex::new(seed);
    move |x, y| {
        let n = fbm(&needles, x as f64, y as f64, size as f64 * 0.2);
        lerp_color((18, 56, 40), (44, 102, 62), n as f32 * 0.5 + 0.5)
    }
}

fn jungle_wood_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (94, 62, 36);
    let base_light: Rgb = (142, 98, 56);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let dist = (dx * dx + dy * dy).sqrt();
        let wobble = ring_noise.get([x as f64 / 8.0, y as f64 / 8.0]) as f32 * 2.0;
        let ring = ((dist + wobble) * 1.1).sin() * 0.5 + 0.5;
        lerp_color(base_dark, base_light, ring)
    }
}

fn jungle_wood_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let bark_noise = OpenSimplex::new(seed);
    let vine_noise = OpenSimplex::new(seed.wrapping_add(31));
    let base_dark: Rgb = (82, 52, 28);
    let base_light: Rgb = (124, 82, 46);
    move |x, y| {
        let groove = ((x as f64 / 5.0).sin() * 0.5 + 0.5) as f32;
        let n = bark_noise.get([x as f64 / 4.0, y as f64 / 16.0]) as f32 * 0.5 + 0.5;
        let mut c = lerp_color(base_dark, base_light, groove * 0.6 + n * 0.4);
        let vine = vine_noise.get([x as f64 / 7.0, y as f64 / 12.0]);
        if vine > 0.45 {
            c = lerp_color(c, (42, 108, 38), 0.65);
        }
        c
    }
}

fn jungle_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (14, 62, 25);
    let base_light: Rgb = (34, 108, 40);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.25);
        lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5)
    }
}

fn autumn_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let color_noise = OpenSimplex::new(seed.wrapping_add(53));
    let amber: Rgb = (218, 126, 28);
    let crimson: Rgb = (194, 52, 24);
    let gold: Rgb = (235, 178, 36);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let cn = color_noise.get([x as f64 / 12.0, y as f64 / 12.0]);
        let base = if cn > 0.15 {
            gold
        } else if cn < -0.15 {
            crimson
        } else {
            amber
        };
        let variation = n as f32 * 0.2 + 0.9;
        shade(base, variation)
    }
}

fn palm_leaves_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let frond_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (24, 72, 32);
    let base_light: Rgb = (54, 118, 48);
    move |x, y| {
        let frond = ((x as f64 * 1.5 + y as f64 * 0.5).sin() * 0.5 + 0.5) as f32;
        let n = frond_noise.get([x as f64 / 8.0, y as f64 / 8.0]) as f32 * 0.5 + 0.5;
        lerp_color(base_dark, base_light, frond * 0.6 + n * 0.4)
    }
}

fn sandstone_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (198, 164, 108);
    let base_light: Rgb = (226, 194, 138);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.35);
        lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5)
    }
}

fn sandstone_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let strata_noise = OpenSimplex::new(seed);
    let layer1: Rgb = (212, 178, 122);
    let layer2: Rgb = (186, 148, 96);
    let layer3: Rgb = (228, 198, 142);
    move |x, y| {
        let wave = (strata_noise.get([x as f64 / 14.0, 0.0]) * 3.0) as i32;
        let band = ((y as i32 + wave) / 6) % 3;
        let color = match band.abs() {
            0 => layer1,
            1 => layer2,
            _ => layer3,
        };
        let grain = (strata_noise.get([x as f64 / 3.0, y as f64 / 3.0]) * 0.08) as f32 + 0.96;
        shade(color, grain)
    }
}

fn terracotta_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let clay_noise = OpenSimplex::new(seed);
    let bands: [Rgb; 4] = [(186, 96, 62), (158, 76, 48), (204, 118, 78), (172, 84, 54)];
    move |x, y| {
        let wave = (clay_noise.get([x as f64 / 18.0, 0.0]) * 4.0) as i32;
        let band_idx = (((y as i32 + wave) / 8).rem_euclid(bands.len() as i32)) as usize;
        let base = bands[band_idx];
        let detail = clay_noise.get([x as f64 / 5.0, y as f64 / 5.0]) as f32 * 0.1 + 0.95;
        shade(base, detail)
    }
}

fn mud_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let puddle_noise = OpenSimplex::new(seed.wrapping_add(71));
    let base_dark: Rgb = (68, 48, 32);
    let base_light: Rgb = (102, 74, 48);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.28);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let puddle = puddle_noise.get([x as f64 / 6.0, y as f64 / 6.0]);
        if puddle > 0.4 {
            c = shade(c, 0.78);
        }
        c
    }
}

fn ice_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let crystal_noise = OpenSimplex::new(seed);
    let crack_noise = OpenSimplex::new(seed.wrapping_add(89));
    let base_dark: Rgb = (156, 198, 235);
    let base_light: Rgb = (212, 236, 252);
    move |x, y| {
        let n = fbm(&crystal_noise, x as f64, y as f64, size as f64 * 0.3);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let crack = crack_noise.get([x as f64 / 7.0, y as f64 / 7.0]).abs();
        if crack < 0.04 {
            c = (240, 250, 255);
        }
        c
    }
}

fn cactus_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (54, 126, 48);
    let base_light: Rgb = (88, 168, 68);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let angle = dy.atan2(dx);
        let rib = (angle * 6.0).cos() * 0.5 + 0.5;
        let n = ring_noise.get([x as f64 / 5.0, y as f64 / 5.0]) as f32 * 0.5 + 0.5;
        lerp_color(base_dark, base_light, rib * 0.6 + n * 0.4)
    }
}

fn cactus_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let rib_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (48, 118, 42);
    let base_light: Rgb = (82, 162, 64);
    move |x, y| {
        let rib = ((x as f64 / 5.3).sin() * 0.5 + 0.5) as f32;
        let n = rib_noise.get([x as f64 / 4.0, y as f64 / 12.0]) as f32 * 0.5 + 0.5;
        let mut c = lerp_color(base_dark, base_light, rib * 0.7 + n * 0.3);
        let spine = (x % 11 == 0) && (y % 9 == 0);
        if spine {
            c = (220, 215, 180);
        }
        c
    }
}

/// Creates the full block texture atlas: one procedurally generated, richly detailed
/// tile per block face type, arranged in a grid described by `texture_atlas`.
pub fn create_block_texture_atlas() -> (u32, u32, Vec<u8>) {
    let tile_size = ATLAS_TILE_RESOLUTION;
    let tiles_per_row = ATLAS_TILES_PER_ROW;
    let tiles_per_col = ATLAS_TILES_PER_COLUMN;

    let width = tile_size * tiles_per_row;
    let height = tile_size * tiles_per_col;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    let tiles: [(Tile, u32); 27] = [
        (Tile::GrassTop, 0),
        (Tile::GrassSide, 1),
        (Tile::Dirt, 2),
        (Tile::Stone, 3),
        (Tile::Sand, 4),
        (Tile::Gravel, 5),
        (Tile::WoodTop, 6),
        (Tile::WoodSide, 7),
        (Tile::Leaves, 8),
        (Tile::BirchSide, 9),
        (Tile::PineLeaves, 10),
        (Tile::Water, 11),
        (Tile::SnowTop, 12),
        (Tile::SnowSide, 13),
        (Tile::MossStone, 14),
        (Tile::JungleWoodTop, 15),
        (Tile::JungleWoodSide, 16),
        (Tile::JungleLeaves, 17),
        (Tile::AutumnLeaves, 18),
        (Tile::PalmLeaves, 19),
        (Tile::SandstoneTop, 20),
        (Tile::SandstoneSide, 21),
        (Tile::Terracotta, 22),
        (Tile::Mud, 23),
        (Tile::Ice, 24),
        (Tile::CactusTop, 25),
        (Tile::CactusSide, 26),
    ];

    for (tile, kind) in tiles {
        let (col, row) = tile.coords();
        let seed = tile_seed(kind);
        match tile {
            Tile::GrassTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                grass_top_tile(tile_size, seed),
            ),
            Tile::GrassSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                grass_side_tile(tile_size, seed),
            ),
            Tile::Dirt => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                dirt_tile(tile_size, seed),
            ),
            Tile::Stone => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                stone_tile(tile_size, seed),
            ),
            Tile::Sand => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                sand_tile(tile_size, seed),
            ),
            Tile::Gravel => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                gravel_tile(tile_size, seed),
            ),
            Tile::WoodTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                wood_top_tile(tile_size, seed),
            ),
            Tile::WoodSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                wood_side_tile(tile_size, seed),
            ),
            Tile::Leaves => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                leaves_tile(tile_size, seed),
            ),
            Tile::BirchSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                birch_side_tile(tile_size, seed),
            ),
            Tile::PineLeaves => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                pine_leaves_tile(tile_size, seed),
            ),
            Tile::Water => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                water_tile(tile_size, seed),
            ),
            Tile::SnowTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                snow_tile(tile_size, seed),
            ),
            Tile::SnowSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                snow_side_tile(tile_size, seed),
            ),
            Tile::MossStone => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                moss_stone_tile(tile_size, seed),
            ),
            Tile::JungleWoodTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                jungle_wood_top_tile(tile_size, seed),
            ),
            Tile::JungleWoodSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                jungle_wood_side_tile(tile_size, seed),
            ),
            Tile::JungleLeaves => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                jungle_leaves_tile(tile_size, seed),
            ),
            Tile::AutumnLeaves => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                autumn_leaves_tile(tile_size, seed),
            ),
            Tile::PalmLeaves => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                palm_leaves_tile(tile_size, seed),
            ),
            Tile::SandstoneTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                sandstone_top_tile(tile_size, seed),
            ),
            Tile::SandstoneSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                sandstone_side_tile(tile_size, seed),
            ),
            Tile::Terracotta => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                terracotta_tile(tile_size, seed),
            ),
            Tile::Mud => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                mud_tile(tile_size, seed),
            ),
            Tile::Ice => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                ice_tile(tile_size, seed),
            ),
            Tile::CactusTop => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                cactus_top_tile(tile_size, seed),
            ),
            Tile::CactusSide => paint_tile(
                &mut pixels,
                width,
                tile_size,
                col,
                row,
                cactus_side_tile(tile_size, seed),
            ),
        }
    }

    // Small irregular cutouts keep foliage airy without the large checkerboard
    // holes that previously dominated whole tree canopies.
    let leaf_tiles = [
        (Tile::Leaves, 8_u32),
        (Tile::PineLeaves, 10_u32),
        (Tile::JungleLeaves, 17_u32),
        (Tile::AutumnLeaves, 18_u32),
        (Tile::PalmLeaves, 19_u32),
    ];
    for (tile, salt) in leaf_tiles {
        let (col, row) = tile.coords();
        paint_tile_alpha(&mut pixels, width, tile_size, col, row, |x, y| {
            let cell = (x / 2) * 37 + (y / 2) * 73 + tile_seed(salt);
            if cell.wrapping_mul(0x9E37_79B1).is_multiple_of(29) {
                0
            } else {
                255
            }
        });
    }

    // Water & Ice alpha translucency
    let (water_col, water_row) = Tile::Water.coords();
    paint_tile_alpha(
        &mut pixels,
        width,
        tile_size,
        water_col,
        water_row,
        |_x, _y| 200,
    );

    let (ice_col, ice_row) = Tile::Ice.coords();
    paint_tile_alpha(&mut pixels, width, tile_size, ice_col, ice_row, |_x, _y| {
        215
    });

    (width, height, pixels)
}

#[derive(Default)]
pub struct WaterTextureAnimation {
    elapsed: f32,
    frame: usize,
}

/// Advances the water tile at 8 frames per second. Updating one tile in this
/// compact atlas is cheap, while the shared animation makes lakes ripple and
/// waterfall faces visibly stream instead of reading as static blue blocks.
pub fn animate_water_texture(
    time: Res<Time>,
    atlas_config: Res<super::texture_atlas::TextureAtlasConfig>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut animation: Local<WaterTextureAnimation>,
) {
    const FRAME_SECONDS: f32 = 0.125;
    animation.elapsed += time.delta_seconds();
    if animation.elapsed < FRAME_SECONDS {
        return;
    }
    animation.elapsed %= FRAME_SECONDS;
    if atlas_config.water_texture_frames.len() < 2 {
        return;
    }
    animation.frame = (animation.frame + 1) % atlas_config.water_texture_frames.len();
    let Some(material_handle) = atlas_config.water_material.as_ref() else {
        return;
    };
    let Some(material) = materials.get_mut(material_handle) else {
        return;
    };
    material.base_color_texture = Some(atlas_config.water_texture_frames[animation.frame].clone());
}

fn paint_water_frame(pixels: &mut [u8], phase: f32) {
    let (col, row) = Tile::Water.coords();
    let atlas_width = ATLAS_TILE_RESOLUTION * ATLAS_TILES_PER_ROW;
    paint_tile(
        pixels,
        atlas_width,
        ATLAS_TILE_RESOLUTION,
        col,
        row,
        water_tile_frame(ATLAS_TILE_RESOLUTION, tile_seed(11), phase),
    );
    paint_tile_alpha(
        pixels,
        atlas_width,
        ATLAS_TILE_RESOLUTION,
        col,
        row,
        |_x, _y| 200,
    );
}

fn atlas_image(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        min_filter: ImageFilterMode::Linear,
        ..ImageSamplerDescriptor::nearest()
    });
    image
}

/// Load or create the block texture atlas and register it as a resource.
pub fn load_or_create_atlas(
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut atlas_config: ResMut<super::texture_atlas::TextureAtlasConfig>,
) {
    let (width, height, pixels) = create_block_texture_atlas();
    let texture_handle = images.add(atlas_image(width, height, pixels.clone()));
    // Texture-handle swaps are reliably propagated by Bevy's material asset
    // pipeline. Sixteen compact full-atlas frames cost about 2.3 MiB and avoid
    // a custom water shader during this voxel-material phase.
    let mut water_frames = Vec::with_capacity(16);
    water_frames.push(texture_handle.clone());
    for frame in 1..16 {
        let mut frame_pixels = pixels.clone();
        paint_water_frame(&mut frame_pixels, frame as f32 * 1.5);
        water_frames.push(images.add(atlas_image(width, height, frame_pixels)));
    }
    atlas_config.texture_handle = Some(texture_handle.clone());
    atlas_config.water_texture_frames = water_frames.clone();
    atlas_config.opaque_material = Some(materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle.clone()),
        perceptual_roughness: 0.84,
        reflectance: 0.14,
        ..default()
    }));
    atlas_config.foliage_material = Some(materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle.clone()),
        alpha_mode: AlphaMode::AlphaToCoverage,
        perceptual_roughness: 0.88,
        reflectance: 0.10,
        ..default()
    }));
    atlas_config.water_material = Some(materials.add(StandardMaterial {
        base_color_texture: Some(water_frames[0].clone()),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.28,
        reflectance: 0.30,
        ..default()
    }));

    info!(
        "Texture atlas ready: {}x{} pixels ({}x{} tiles of {}px)",
        width, height, ATLAS_TILES_PER_ROW, ATLAS_TILES_PER_COLUMN, ATLAS_TILE_RESOLUTION
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_has_expected_size_and_alpha_layers() {
        let (width, height, pixels) = create_block_texture_atlas();
        assert_eq!(width, ATLAS_TILES_PER_ROW * ATLAS_TILE_RESOLUTION);
        assert_eq!(height, ATLAS_TILES_PER_COLUMN * ATLAS_TILE_RESOLUTION);
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 200));
    }

    #[test]
    fn water_animation_changes_the_tile_without_changing_its_palette_bounds() {
        let mut first = water_tile_frame(ATLAS_TILE_RESOLUTION, tile_seed(11), 0.0);
        let mut later = water_tile_frame(ATLAS_TILE_RESOLUTION, tile_seed(11), 9.0);
        let samples = [(0, 0), (7, 13), (16, 16), (29, 5)];
        assert!(samples.iter().any(|&(x, y)| first(x, y) != later(x, y)));
        for &(x, y) in &samples {
            let color = later(x, y);
            assert!(color.2 > color.0 && color.2 > color.1);
        }
    }
}
