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
    let q = |value: u8| ((value as u16 / 2) * 2 + 1).min(255) as u8;
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
            let (r, g, b) = posterize(paint(x, y));
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
    let flower_noise = OpenSimplex::new(seed.wrapping_add(107));
    // Desaturated a touch: full-saturation lawns read as toy plastic under
    // bright sun; the eye expects more yellow in the midtones.
    let base_dark: Rgb = (66, 134, 50);
    let base_mid: Rgb = (92, 164, 62);
    let base_light: Rgb = (126, 196, 88);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.26);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        let blade = blade_noise.get([x as f64 / 2.2, y as f64 / 7.5]);
        if blade > 0.48 {
            c = shade(c, 1.08);
        } else if blade < -0.52 {
            c = shade(c, 0.88);
        }
        // Rare tiny meadow flower flecks
        let flower = flower_noise.get([x as f64 / 1.5, y as f64 / 1.5]);
        if flower > 0.90 {
            c = (248, 236, 140);
        } else if flower < -0.92 {
            c = (250, 252, 255);
        }
        c
    }
}

fn dirt_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let pebble_noise = OpenSimplex::new(seed.wrapping_add(2));
    let crumb_noise = OpenSimplex::new(seed.wrapping_add(77));
    let base_dark: Rgb = (102, 68, 42);
    let base_mid: Rgb = (132, 92, 58);
    let base_light: Rgb = (156, 114, 76);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.28);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        let pebble = pebble_noise.get([x as f64 / 4.5, y as f64 / 4.5]);
        if pebble > 0.60 {
            c = (168, 138, 102); // Rounded gravel pebble
        } else if pebble < -0.62 {
            c = shade(c, 0.72); // Soil pocket shadow
        }
        let crumb = crumb_noise.get([x as f64 / 1.8, y as f64 / 1.8]);
        if crumb > 0.55 {
            c = shade(c, 1.08);
        }
        c
    }
}

fn grass_side_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut grass = grass_top_tile(size, seed);
    let mut dirt = dirt_tile(size, seed.wrapping_add(3));
    let fringe_noise = OpenSimplex::new(seed.wrapping_add(101));
    let base_band = (size as f32 * 0.34) as i32;
    move |x, y| {
        let drop = (fringe_noise.get([x as f64 / 3.0, 0.0]) * 4.2).round() as i32;
        let blade_tip = if (x % 3 == 0) && drop > 0 { 2 } else { 0 };
        let grass_edge = base_band + drop + blade_tip;
        if (y as i32) < grass_edge {
            let mut g = grass(x, y);
            if (y as i32) == grass_edge - 1 {
                g = shade(g, 0.88); // Shadow on underside of grass blades
            }
            g
        } else if (y as i32) < grass_edge + 2 {
            let t = ((y as i32) - grass_edge) as f32 / 2.0;
            lerp_color(grass(x, y), dirt(x, y), t)
        } else {
            dirt(x, y)
        }
    }
}

fn stone_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let crack_noise = OpenSimplex::new(seed.wrapping_add(4));
    let speckle_noise = OpenSimplex::new(seed.wrapping_add(91));
    let base_dark: Rgb = (128, 134, 142);
    let base_mid: Rgb = (154, 160, 168);
    let base_light: Rgb = (182, 188, 196);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        let crack = crack_noise.get([x as f64 / 6.0, y as f64 / 6.0]).abs();
        // Softer cracks: near-black wormy lines on shadowed faces turned
        // every cliff riser into high-contrast stripes.
        if crack < 0.026 {
            c = shade(c, 0.74);
        } else if crack < 0.055 {
            c = shade(c, 0.88);
        }
        let speckle = speckle_noise.get([x as f64 / 2.0, y as f64 / 2.0]);
        if speckle > 0.65 {
            c = (198, 204, 214);
        }
        c
    }
}

fn sand_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let ripple_noise = OpenSimplex::new(seed.wrapping_add(5));
    let grain_noise = OpenSimplex::new(seed.wrapping_add(19));
    let base_dark: Rgb = (218, 186, 126);
    let base_mid: Rgb = (238, 208, 148);
    let base_light: Rgb = (252, 226, 172);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.32);
        let ripple = ((x as f32 * 0.35 + (y as f32 * 0.15)).sin() * 0.5 + 0.5) * 0.22;
        let t = (n as f32 * 0.4 + 0.4 + ripple).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        let ripple_wave = ripple_noise.get([x as f64 / 7.0, y as f64 / 7.0]);
        if ripple_wave > 0.55 {
            c = shade(c, 1.07);
        }
        let grain = grain_noise.get([x as f64 / 1.5, y as f64 / 1.5]);
        if grain > 0.72 {
            c = (255, 244, 210);
        }
        c
    }
}

fn wood_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed.wrapping_add(6));
    let bark_outer: Rgb = (82, 54, 32);
    let base_dark: Rgb = (142, 98, 60);
    let base_mid: Rgb = (178, 132, 84);
    let base_light: Rgb = (204, 156, 104);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > center - 2.5 {
            return bark_outer;
        }
        let wobble = ring_noise.get([x as f64 / 8.0, y as f64 / 8.0]) as f32 * 2.2;
        let ring = ((dist + wobble) * 0.85).sin() * 0.5 + 0.5;
        let mut c = if ring < 0.5 {
            lerp_color(base_dark, base_mid, ring * 2.0)
        } else {
            lerp_color(base_mid, base_light, (ring - 0.5) * 2.0)
        };
        if dist < 3.0 {
            c = shade(c, 0.85); // Core heartwood
        }
        c
    }
}

fn wood_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let bark_noise = OpenSimplex::new(seed.wrapping_add(7));
    let furrow_noise = OpenSimplex::new(seed.wrapping_add(27));
    let base_dark: Rgb = (72, 46, 28);
    let base_mid: Rgb = (108, 74, 46);
    let base_light: Rgb = (142, 100, 64);
    move |x, y| {
        let groove = ((x as f64 / 3.8).sin() * 0.5 + 0.5) as f32;
        let n = bark_noise.get([x as f64 / 2.8, y as f64 / 12.0]) as f32 * 0.5 + 0.5;
        let furrow = furrow_noise.get([x as f64 / 5.0, y as f64 / 16.0]);
        let t = (groove * 0.60 + n * 0.40).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        if furrow > 0.52 {
            c = shade(c, 1.12);
        } else if furrow < -0.52 {
            c = shade(c, 0.80);
        }
        c
    }
}

fn leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let clump_noise = OpenSimplex::new(seed.wrapping_add(8));
    let leaf_light: Rgb = (88, 192, 58);
    let leaf_mid: Rgb = (54, 150, 42);
    let leaf_dark: Rgb = (34, 102, 30);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(leaf_dark, leaf_mid, t * 2.0)
        } else {
            lerp_color(leaf_mid, leaf_light, (t - 0.5) * 2.0)
        };
        let clump = clump_noise.get([x as f64 / 3.8, y as f64 / 3.8]);
        if clump > 0.48 {
            c = shade(c, 1.18);
        } else if clump < -0.52 {
            c = shade(c, 0.78);
        }
        c
    }
}

fn water_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    water_tile_frame(size, seed, 0.0)
}

fn water_tile_frame(size: u32, seed: u32, phase: f32) -> impl FnMut(u32, u32) -> Rgb {
    let wave_noise = OpenSimplex::new(seed.wrapping_add(9));
    // Deeper, less sky-coloured palette: water should read as blue even
    // where it reflects a bright horizon.
    let base_deep: Rgb = (14, 84, 148);
    let base_mid: Rgb = (28, 124, 192);
    let base_light: Rgb = (54, 164, 220);
    move |x, y| {
        let flowing_y = (y as f32 + phase) % size as f32;
        let n = fbm(
            &wave_noise,
            x as f64 * 0.8,
            flowing_y as f64 * 0.8,
            size as f64 * 0.35,
        );
        // Gentle swells: high-contrast sine lattices shimmer into moiré
        // once the 32 px tile minifies across an ocean of blocks.
        let wave1 = ((x as f32 * 0.38 + flowing_y * 0.28).sin() * 0.5 + 0.5) * 0.16;
        let wave2 = ((x as f32 * 0.22 - flowing_y * 0.36 + 1.5).cos() * 0.5 + 0.5) * 0.13;
        let t = (n as f32 * 0.35 + 0.45 + wave1 + wave2).clamp(0.0, 1.0);
        let mut color = if t < 0.5 {
            lerp_color(base_deep, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        if wave1 + wave2 > 0.22 && n > 0.25 {
            color = lerp_color(color, (196, 242, 255), 0.22);
        }
        color
    }
}

fn gravel_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let stones: [Rgb; 5] = [
        (104, 102, 98),
        (134, 126, 114),
        (158, 150, 136),
        (116, 126, 128),
        (174, 162, 138),
    ];
    move |x, y| {
        let cell_x = x / 7;
        let cell_y = y / 6;
        let index = ((cell_x * 13 + cell_y * 29 + seed) % stones.len() as u32) as usize;
        let edge = (x % 7 == 0 || y % 6 == 0) as u8;
        let n = noise.get([
            x as f64 / (size as f64 * 0.18),
            y as f64 / (size as f64 * 0.18),
        ]);
        shade(
            stones[index],
            if edge == 1 {
                0.68
            } else {
                0.92 + n as f32 * 0.14
            },
        )
    }
}

fn snow_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let drifts = OpenSimplex::new(seed);
    let sparkle_noise = OpenSimplex::new(seed.wrapping_add(41));
    let base_blue: Rgb = (212, 232, 248);
    let base_white: Rgb = (252, 254, 255);
    move |x, y| {
        let n = fbm(&drifts, x as f64, y as f64, size as f64 * 0.32) as f32;
        let mut c = lerp_color(base_blue, base_white, n * 0.5 + 0.6);
        let sparkle = sparkle_noise.get([x as f64 / 3.0, y as f64 / 3.0]);
        if sparkle > 0.86 {
            c = (255, 255, 255);
        }
        c
    }
}

fn snow_side_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut snow = snow_tile(size, seed);
    let mut stone = stone_tile(size, seed.wrapping_add(19));
    // Thin stone fringe only: a full stone lower third turned every snowed
    // riser into a dark stripe and striped whole mountain faces.
    let cap = size * 7 / 8;
    let drip_noise = OpenSimplex::new(seed.wrapping_add(33));
    move |x, y| {
        let drip = (drip_noise.get([x as f64 / 3.0, 0.0]) * 3.5).round() as i32;
        if (y as i32) < cap as i32 + drip {
            snow(x, y)
        } else {
            stone(x, y)
        }
    }
}

fn moss_stone_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let mut stone = stone_tile(size, seed);
    let moss = OpenSimplex::new(seed.wrapping_add(23));
    let moss_light: Rgb = (104, 186, 68);
    let moss_dark: Rgb = (58, 124, 44);
    move |x, y| {
        let rock = stone(x, y);
        let patch = moss.get([x as f64 / 10.0, y as f64 / 8.0]);
        if patch + (1.0 - y as f64 / size as f64) * 0.38 > 0.28 {
            let t = (patch as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
            let moss_col = lerp_color(moss_dark, moss_light, t);
            lerp_color(rock, moss_col, 0.78)
        } else {
            rock
        }
    }
}

fn birch_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let grain = OpenSimplex::new(seed);
    let bark_base: Rgb = (238, 236, 226);
    let bark_light: Rgb = (252, 250, 244);
    let lenticel_color: Rgb = (42, 38, 34);
    let lenticel_edge: Rgb = (92, 84, 76);
    move |x, y| {
        let n = grain.get([x as f64 / 8.0, y as f64 / 16.0]) as f32;
        let mut c = lerp_color(bark_base, bark_light, n * 0.5 + 0.5);
        let notch_y = y % 10;
        let notch_x = (x + (y / 10) * 11) % 13;
        if notch_y == 0 && notch_x < 5 {
            c = lenticel_color;
        } else if (notch_y == 1 || notch_y == 9) && notch_x < 4 {
            c = lenticel_edge;
        }
        c
    }
}

fn pine_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let needles = OpenSimplex::new(seed);
    let base_dark: Rgb = (28, 92, 60);
    let base_mid: Rgb = (46, 136, 88);
    let base_light: Rgb = (72, 178, 116);
    move |x, y| {
        let n = fbm(&needles, x as f64, y as f64, size as f64 * 0.20);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        }
    }
}

fn jungle_wood_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (108, 68, 38);
    let base_light: Rgb = (168, 116, 68);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let dist = (dx * dx + dy * dy).sqrt();
        let wobble = ring_noise.get([x as f64 / 7.0, y as f64 / 7.0]) as f32 * 2.0;
        let ring = ((dist + wobble) * 1.1).sin() * 0.5 + 0.5;
        lerp_color(base_dark, base_light, ring)
    }
}

fn jungle_wood_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let bark_noise = OpenSimplex::new(seed);
    let vine_noise = OpenSimplex::new(seed.wrapping_add(31));
    let base_dark: Rgb = (98, 60, 34);
    let base_light: Rgb = (144, 96, 56);
    move |x, y| {
        let groove = ((x as f64 / 4.8).sin() * 0.5 + 0.5) as f32;
        let n = bark_noise.get([x as f64 / 3.8, y as f64 / 14.0]) as f32 * 0.5 + 0.5;
        let mut c = lerp_color(base_dark, base_light, groove * 0.6 + n * 0.4);
        let vine = vine_noise.get([x as f64 / 6.0, y as f64 / 10.0]);
        if vine > 0.42 {
            c = lerp_color(c, (54, 142, 48), 0.72);
        }
        c
    }
}

fn jungle_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (32, 128, 46);
    let base_mid: Rgb = (58, 178, 64);
    let base_light: Rgb = (88, 218, 82);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.24);
        let t = (n as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        }
    }
}

fn autumn_leaves_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let color_noise = OpenSimplex::new(seed.wrapping_add(53));
    let amber: Rgb = (236, 138, 32);
    let crimson: Rgb = (214, 52, 24);
    let gold: Rgb = (252, 196, 44);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.22);
        let cn = color_noise.get([x as f64 / 10.0, y as f64 / 10.0]);
        let base = if cn > 0.18 {
            gold
        } else if cn < -0.18 {
            crimson
        } else {
            amber
        };
        let variation = n as f32 * 0.25 + 0.88;
        shade(base, variation)
    }
}

fn palm_leaves_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let frond_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (38, 114, 46);
    let base_light: Rgb = (78, 176, 68);
    move |x, y| {
        let frond = ((x as f64 * 1.6 + y as f64 * 0.6).sin() * 0.5 + 0.5) as f32;
        let n = frond_noise.get([x as f64 / 7.0, y as f64 / 7.0]) as f32 * 0.5 + 0.5;
        lerp_color(base_dark, base_light, frond * 0.65 + n * 0.35)
    }
}

fn sandstone_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (218, 184, 126);
    let base_light: Rgb = (244, 216, 160);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.35);
        lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5)
    }
}

fn sandstone_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let strata_noise = OpenSimplex::new(seed);
    let layer1: Rgb = (230, 198, 142);
    let layer2: Rgb = (204, 168, 114);
    let layer3: Rgb = (242, 218, 164);
    move |x, y| {
        let wave = (strata_noise.get([x as f64 / 12.0, 0.0]) * 3.0) as i32;
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
    let bands: [Rgb; 4] = [
        (204, 114, 76),
        (174, 90, 58),
        (224, 134, 92),
        (190, 102, 66),
    ];
    move |x, y| {
        let wave = (clay_noise.get([x as f64 / 16.0, 0.0]) * 3.5) as i32;
        let band_idx = (((y as i32 + wave) / 7).rem_euclid(bands.len() as i32)) as usize;
        let base = bands[band_idx];
        let detail = clay_noise.get([x as f64 / 4.5, y as f64 / 4.5]) as f32 * 0.1 + 0.95;
        shade(base, detail)
    }
}

fn mud_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let noise = OpenSimplex::new(seed);
    let puddle_noise = OpenSimplex::new(seed.wrapping_add(71));
    let base_dark: Rgb = (82, 56, 38);
    let base_light: Rgb = (122, 88, 60);
    move |x, y| {
        let n = fbm(&noise, x as f64, y as f64, size as f64 * 0.28);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let puddle = puddle_noise.get([x as f64 / 5.5, y as f64 / 5.5]);
        if puddle > 0.38 {
            c = shade(c, 0.74);
        }
        c
    }
}

fn ice_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let crystal_noise = OpenSimplex::new(seed);
    let crack_noise = OpenSimplex::new(seed.wrapping_add(89));
    let base_dark: Rgb = (174, 216, 248);
    let base_light: Rgb = (230, 248, 255);
    move |x, y| {
        let n = fbm(&crystal_noise, x as f64, y as f64, size as f64 * 0.3);
        let mut c = lerp_color(base_dark, base_light, n as f32 * 0.5 + 0.5);
        let crack = crack_noise.get([x as f64 / 6.0, y as f64 / 6.0]).abs();
        if crack < 0.042 {
            c = (248, 254, 255);
        }
        c
    }
}

fn cactus_top_tile(size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let ring_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (68, 146, 58);
    let base_light: Rgb = (106, 192, 84);
    let center = size as f32 / 2.0;
    move |x, y| {
        let dx = x as f32 - center;
        let dy = y as f32 - center;
        let angle = dy.atan2(dx);
        let rib = (angle * 6.0).cos() * 0.5 + 0.5;
        let n = ring_noise.get([x as f64 / 4.5, y as f64 / 4.5]) as f32 * 0.5 + 0.5;
        lerp_color(base_dark, base_light, rib * 0.6 + n * 0.4)
    }
}

fn cactus_side_tile(_size: u32, seed: u32) -> impl FnMut(u32, u32) -> Rgb {
    let rib_noise = OpenSimplex::new(seed);
    let base_dark: Rgb = (58, 136, 52);
    let base_mid: Rgb = (82, 168, 68);
    let base_light: Rgb = (112, 198, 92);
    move |x, y| {
        let rib = ((x as f32 / 4.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let n = rib_noise.get([x as f64 / 3.0, y as f64 / 8.0]) as f32 * 0.5 + 0.5;
        let t = (rib * 0.65 + n * 0.35).clamp(0.0, 1.0);
        let mut c = if t < 0.5 {
            lerp_color(base_dark, base_mid, t * 2.0)
        } else {
            lerp_color(base_mid, base_light, (t - 0.5) * 2.0)
        };
        // Spines at regular rib intervals
        if (x % 4 == 2) && (y % 6 == 3) {
            c = (242, 244, 220);
        }
        c
    }
}

// ---------------------------------------------------------------------------
// Ground plant sprites (crossed cutout quads)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum PlantSprite {
    GrassTuft,
    Flowers(u8), // 0 yellow, 1 white, 2 red
    Fern,
    Mushroom,
}

/// One pixel of a plant sprite: colour + alpha. Deterministic in
/// `(seed, x, y)` so the colour and alpha passes always agree.
fn plant_pixel(x: u32, y: u32, size: u32, seed: u32, sprite: PlantSprite) -> (Rgb, u8) {
    let hash = |salt: u32| -> u32 {
        (seed
            ^ (x.wrapping_mul(0x9E37_79B1))
            ^ (y.wrapping_mul(0x85EB_CA77))
            ^ salt.wrapping_mul(0xC2B2_AE35))
        .wrapping_mul(0x2722_0A95)
    };
    let fx = x as f32 + 0.5;
    let fy = y as f32 + 0.5;
    let s = size as f32;
    let from_bottom = s - fy;

    match sprite {
        PlantSprite::GrassTuft => {
            // Few, wide, two-toned blades: 1-2 px slivers turn into dithered
            // static when the 32 px sprite minifies at distance.
            let blade_count = 7;
            for blade in 0..blade_count {
                let root = 2.0 + blade as f32 * ((s - 4.0) / (blade_count - 1) as f32);
                let lean = ((hash(blade as u32 + 11) % 7) as f32 - 3.0) * 0.5;
                let height = s * (0.42 + (hash(blade as u32 + 23) % 100) as f32 / 200.0);
                let tip_shift = lean * (from_bottom / height).clamp(0.0, 1.0);
                let width = 2.7 - 1.2 * (from_bottom / height).clamp(0.0, 1.0);
                if from_bottom <= height && (fx - (root + tip_shift)).abs() <= width {
                    let t = (from_bottom / height).clamp(0.0, 1.0);
                    // Deliberately darker + richer than the lawn so tufts
                    // read as foliage accents, never as pale haze.
                    let base: Rgb = (44, 96, 38);
                    let tip: Rgb = (108, 178, 72);
                    let mut c = lerp_color(base, tip, t);
                    if hash(blade as u32 + 37) % 4 == 0 {
                        c = shade(c, 0.86);
                    }
                    return (c, 255);
                }
            }
            ((0, 0, 0), 0)
        }
        PlantSprite::Flowers(variant) => {
            let stem_x = s * 0.5 + ((hash(5) % 5) as f32 - 2.0);
            let stem_h = s * (0.42 + (hash(7) % 100) as f32 / 400.0);
            // Stem
            if fx >= stem_x - 1.2 && fx <= stem_x + 1.2 && from_bottom <= stem_h {
                return ((74, 138, 54), 255);
            }
            // Head: small petal blob kept low on the stem - a large head on
            // both crossed quads projects as a flat yellow diamond from
            // above.
            let head_cy = s - stem_h - 1.5;
            let head_dx = fx - stem_x;
            let head_dy = fy - head_cy;
            let head_r = s * 0.13;
            if (head_dx * head_dx + head_dy * head_dy * 1.6).sqrt() <= head_r {
                let petal: Rgb = match variant {
                    0 => (244, 208, 66),
                    1 => (246, 248, 250),
                    _ => (214, 84, 74),
                };
                let center: Rgb = match variant {
                    0 => (214, 138, 44),
                    _ => (240, 196, 84),
                };
                if head_dx.abs() < head_r * 0.42 && head_dy.abs() < head_r * 0.42 {
                    return (center, 255);
                }
                if hash(13) % 6 == 0 {
                    return (shade(petal, 0.88), 255);
                }
                return (petal, 255);
            }
            // A couple of ground leaves
            if from_bottom < 5.0 && (fx - stem_x).abs() > 1.2 && (fx - stem_x).abs() < 5.0 {
                let leaf = hash((fx as u32) % 3 + 29) % 4;
                if leaf == 0 {
                    return ((86, 152, 60), 255);
                }
            }
            ((0, 0, 0), 0)
        }
        PlantSprite::Fern => {
            let fronds = 5;
            for frond in 0..fronds {
                let dir = if frond % 2 == 0 { 1.0 } else { -1.0 };
                let spread = (frond as f32 - 2.0).abs() * 0.35 + 0.4;
                let length = s * (0.55 + (hash(frond as u32 + 3) % 100) as f32 / 300.0);
                for step in 0..(length as i32) {
                    let t = step as f32;
                    let px = s * 0.5 + dir * t * spread;
                    let py = s - 1.0 - t * 0.85 + (t * t) * 0.012;
                    let dx = (fx - px).abs();
                    let dy = (fy - py).abs();
                    // Leaflets alternate on both sides of the rib.
                    let leaflet = (dx < 2.6 && dy < 1.2) || (dx < 1.2 && dy < 2.4);
                    if leaflet {
                        let base: Rgb = (52, 110, 44);
                        let tip: Rgb = (104, 172, 76);
                        let c = lerp_color(base, tip, (t / length).clamp(0.0, 1.0));
                        return (c, 255);
                    }
                }
            }
            ((0, 0, 0), 0)
        }
        PlantSprite::Mushroom => {
            let stem_cx = s * 0.5;
            let stem_w = 3.2;
            let stem_top = s * 0.42;
            if (fx - stem_cx).abs() <= stem_w && from_bottom <= stem_top {
                return ((226, 212, 186), 255);
            }
            // Cap: half ellipse
            let cap_cy = stem_top + 2.0;
            let cap_rx = s * 0.34;
            let cap_ry = s * 0.20;
            let nx = (fx - stem_cx) / cap_rx;
            let ny = (fy - cap_cy) / cap_ry;
            if ny <= 0.4 && nx * nx + ny * ny <= 1.0 {
                let c: Rgb = (190, 62, 50);
                if hash((x / 3) * 7 + (y / 3)) % 7 == 0 {
                    return ((242, 236, 226), 255);
                }
                if ny > 0.1 {
                    return (shade(c, 0.82), 255);
                }
                return (c, 255);
            }
            ((0, 0, 0), 0)
        }
    }
}

fn paint_plant_sprite(
    pixels: &mut [u8],
    width: u32,
    size: u32,
    tile: Tile,
    seed: u32,
    sprite: PlantSprite,
) {
    let (col, row) = tile.coords();
    paint_tile(pixels, width, size, col, row, |x, y| {
        plant_pixel(x, y, size, seed, sprite).0
    });
    paint_tile_alpha(pixels, width, size, col, row, |x, y| {
        plant_pixel(x, y, size, seed, sprite).1
    });
}

/// Generates the raw pixel buffer for the full block texture atlas.
pub fn create_block_texture_atlas() -> (u32, u32, Vec<u8>) {
    let tile_size = ATLAS_TILE_RESOLUTION;
    let width = ATLAS_TILES_PER_ROW * tile_size;
    let height = ATLAS_TILES_PER_COLUMN * tile_size;
    let mut pixels = vec![255_u8; (width * height * 4) as usize];

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
            // Ground plant sprites are painted after this loop (they need a
            // colour and an alpha pass).
            Tile::GroundGrassTuft
            | Tile::GroundFlowersYellow
            | Tile::GroundFlowersWhite
            | Tile::GroundFlowersRed
            | Tile::GroundFern
            | Tile::GroundMushroom => {}
        }
    }

    // Ground plant sprites (row 5): colour + cutout alpha, deterministic
    // per tile so the two passes always agree.
    let plant_seed = tile_seed(90);
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundGrassTuft,
        plant_seed,
        PlantSprite::GrassTuft,
    );
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundFlowersYellow,
        plant_seed + 1,
        PlantSprite::Flowers(0),
    );
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundFlowersWhite,
        plant_seed + 2,
        PlantSprite::Flowers(1),
    );
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundFlowersRed,
        plant_seed + 3,
        PlantSprite::Flowers(2),
    );
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundFern,
        plant_seed + 4,
        PlantSprite::Fern,
    );
    paint_plant_sprite(
        &mut pixels,
        width,
        tile_size,
        Tile::GroundMushroom,
        plant_seed + 5,
        PlantSprite::Mushroom,
    );

    // Stylized foliage cutout holes for airy leaves
    let leaf_tiles = [
        (Tile::Leaves, 8_u32),
        (Tile::PineLeaves, 10_u32),
        (Tile::JungleLeaves, 17_u32),
        (Tile::AutumnLeaves, 18_u32),
        (Tile::PalmLeaves, 19_u32),
    ];
    for (tile, salt) in leaf_tiles {
        let (col, row) = tile.coords();
        // Organic gap clusters instead of uniform pinpricks: a low-frequency
        // noise field decides where the canopy shows through, so holes read
        // as sky between leaf clumps rather than moth damage.
        let hole_noise = OpenSimplex::new(tile_seed(salt).wrapping_add(4_231));
        paint_tile_alpha(&mut pixels, width, tile_size, col, row, |x, y| {
            let gap = fbm(&hole_noise, x as f64, y as f64, tile_size as f64 * 0.16);
            if gap > 0.58 {
                0
            } else {
                255
            }
        });
    }

    // Water alpha translucency: 215 (keeps shallows see-through while the
    // deeper palette still reads as blue).
    let (water_col, water_row) = Tile::Water.coords();
    paint_tile_alpha(
        &mut pixels,
        width,
        tile_size,
        water_col,
        water_row,
        |_x, _y| 215,
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

/// Advances the water tile at 8 frames per second.
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
        |_x, _y| 215,
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
        perceptual_roughness: 0.82,
        reflectance: 0.16,
        ..default()
    }));
    atlas_config.foliage_material = Some(materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle.clone()),
        alpha_mode: AlphaMode::AlphaToCoverage,
        perceptual_roughness: 0.84,
        reflectance: 0.12,
        cull_mode: None,
        ..default()
    }));
    atlas_config.water_material = Some(materials.add(StandardMaterial {
        base_color_texture: Some(water_frames[0].clone()),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.10,
        reflectance: 0.55,
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
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 215));
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
