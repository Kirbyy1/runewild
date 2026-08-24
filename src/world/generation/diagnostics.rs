use std::{collections::HashMap, fs, io, path::Path};

use bevy::prelude::*;

use crate::world::chunk_manager::ChunkManager;

use super::{
    biome::Biome,
    terrain::{TerrainGenerator, TerrainRegion, SEA_LEVEL},
};

#[derive(Debug, Clone)]
pub struct TerrainDiagnostics {
    pub samples: usize,
    pub min_height_m: f64,
    pub max_height_m: f64,
    pub flat_neighbor_ratio: f64,
    pub steep_column_ratio: f64,
    pub river_column_ratio: f64,
    pub lake_column_ratio: f64,
    pub inland_water_column_ratio: f64,
    pub deposition_column_ratio: f64,
    pub region_counts: HashMap<TerrainRegion, usize>,
    pub biome_counts: HashMap<Biome, usize>,
}

impl TerrainDiagnostics {
    pub fn sample(generator: &TerrainGenerator, center: IVec2, radius_m: i32, step_m: i32) -> Self {
        let step = step_m.max(1);
        let mut min_height_m = f64::INFINITY;
        let mut max_height_m = f64::NEG_INFINITY;
        let mut flat_neighbors = 0usize;
        let mut neighbor_pairs = 0usize;
        let mut steep_columns = 0usize;
        let mut river_columns = 0usize;
        let mut lake_columns = 0usize;
        let mut inland_water_columns = 0usize;
        let mut deposition_columns = 0usize;
        let mut samples = 0usize;
        let mut region_counts = HashMap::new();
        let mut biome_counts = HashMap::new();

        for z in (center.y - radius_m..=center.y + radius_m).step_by(step as usize) {
            for x in (center.x - radius_m..=center.x + radius_m).step_by(step as usize) {
                let column = generator.column_at(x, z);
                min_height_m = min_height_m.min(column.height_m);
                max_height_m = max_height_m.max(column.height_m);
                steep_columns += usize::from(column.slope > 1.0);
                river_columns += usize::from(column.river_strength > 0.20);
                lake_columns += usize::from(column.lake_strength > 0.20);
                inland_water_columns += usize::from(
                    column.height_m >= SEA_LEVEL as f64 && column.water_level_m.is_some(),
                );
                deposition_columns += usize::from(column.deposition > 0.20);
                samples += 1;
                *region_counts.entry(column.region).or_insert(0) += 1;
                *biome_counts.entry(column.biome).or_insert(0) += 1;

                let right = generator.column_at(x + step, z);
                let down = generator.column_at(x, z + step);
                for neighbor in [right, down] {
                    neighbor_pairs += 1;
                    let a = column.surface_voxel_y();
                    let b = neighbor.surface_voxel_y();
                    flat_neighbors += usize::from(a == b);
                }
            }
        }

        Self {
            samples,
            min_height_m,
            max_height_m,
            flat_neighbor_ratio: flat_neighbors as f64 / neighbor_pairs.max(1) as f64,
            steep_column_ratio: steep_columns as f64 / samples.max(1) as f64,
            river_column_ratio: river_columns as f64 / samples.max(1) as f64,
            lake_column_ratio: lake_columns as f64 / samples.max(1) as f64,
            inland_water_column_ratio: inland_water_columns as f64 / samples.max(1) as f64,
            deposition_column_ratio: deposition_columns as f64 / samples.max(1) as f64,
            region_counts,
            biome_counts,
        }
    }
}

pub fn export_debug_maps_on_startup(manager: Res<ChunkManager>) {
    let enabled = std::env::var("WORLD_DEBUG_MAP")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let output = Path::new("debug/worldgen");
    match write_debug_maps(manager.generator(), output, IVec2::ZERO, 1024, 256) {
        Ok(diagnostics) => {
            info!(
                "worldgen debug maps written to {} ({} samples, height {:.1}..{:.1} m, flat {:.1}%, steep {:.1}%, river {:.1}%, lake {:.1}%, inland water {:.1}%)",
                output.display(),
                diagnostics.samples,
                diagnostics.min_height_m,
                diagnostics.max_height_m,
                diagnostics.flat_neighbor_ratio * 100.0,
                diagnostics.steep_column_ratio * 100.0,
                diagnostics.river_column_ratio * 100.0,
                diagnostics.lake_column_ratio * 100.0,
                diagnostics.inland_water_column_ratio * 100.0,
            );
        }
        Err(error) => error!("failed to write worldgen debug maps: {error}"),
    }
}

pub fn write_debug_maps(
    generator: &TerrainGenerator,
    output: &Path,
    center: IVec2,
    span_m: i32,
    pixels: usize,
) -> io::Result<TerrainDiagnostics> {
    fs::create_dir_all(output)?;
    let pixels = pixels.max(2);
    let step = (span_m / pixels as i32).max(1);
    let half = step * pixels as i32 / 2;
    let mut columns = Vec::with_capacity(pixels * pixels);
    for pz in 0..pixels {
        for px in 0..pixels {
            let x = center.x - half + px as i32 * step;
            let z = center.y - half + pz as i32 * step;
            columns.push((
                generator.column_at(x, z),
                generator.mountain_strength_at(x, z),
            ));
        }
    }

    let min_height = columns
        .iter()
        .map(|(column, _)| column.height_m)
        .fold(f64::INFINITY, f64::min);
    let max_height = columns
        .iter()
        .map(|(column, _)| column.height_m)
        .fold(f64::NEG_INFINITY, f64::max);
    let height_range = (max_height - min_height).max(1.0);

    let mut height = Vec::with_capacity(columns.len());
    let mut slope = Vec::with_capacity(columns.len());
    let mut biome = Vec::with_capacity(columns.len());
    let mut river = Vec::with_capacity(columns.len());
    let mut mountain = Vec::with_capacity(columns.len());
    let mut region = Vec::with_capacity(columns.len());
    let mut geology = Vec::with_capacity(columns.len());
    let mut flow = Vec::with_capacity(columns.len());
    let mut lakes = Vec::with_capacity(columns.len());
    let mut deposition = Vec::with_capacity(columns.len());
    let mut water = Vec::with_capacity(columns.len());
    for (column, mountain_strength) in &columns {
        let h = ((column.height_m - min_height) / height_range).clamp(0.0, 1.0);
        height.push(gray(h));
        slope.push(heat((column.slope / 2.0).clamp(0.0, 1.0)));
        biome.push(biome_color(column.biome));
        river.push([
            20,
            (70.0 + column.river_strength * 100.0) as u8,
            (130.0 + column.river_strength * 125.0) as u8,
        ]);
        mountain.push(gray(mountain_strength.clamp(0.0, 1.0)));
        region.push(region_color(column.region));
        let g = (column.climate.geology * 0.5 + 0.5).clamp(0.0, 1.0);
        geology.push([
            (180.0 * g) as u8,
            (130.0 * (1.0 - (g - 0.5).abs() * 2.0)) as u8,
            (180.0 * (1.0 - g)) as u8,
        ]);
        flow.push(heat(column.flow_strength.clamp(0.0, 1.0)));
        lakes.push([20, (90.0 + column.lake_strength * 90.0) as u8, 210]);
        deposition.push([
            (175.0 + column.deposition * 70.0) as u8,
            (110.0 + column.deposition * 90.0) as u8,
            45,
        ]);
        water.push(if column.water_level_m.is_some() {
            if column.height_m >= SEA_LEVEL as f64 {
                [35, 175, 235]
            } else {
                [20, 70, 145]
            }
        } else {
            [8, 8, 10]
        });
    }

    for (name, data) in [
        ("height.ppm", height),
        ("slope.ppm", slope),
        ("biome.ppm", biome),
        ("river.ppm", river),
        ("mountain.ppm", mountain),
        ("region.ppm", region),
        ("geology.ppm", geology),
        ("flow.ppm", flow),
        ("lakes.ppm", lakes),
        ("deposition.ppm", deposition),
        ("water.ppm", water),
    ] {
        write_ppm(&output.join(name), pixels, pixels, &data)?;
    }

    let diagnostics = TerrainDiagnostics::sample(generator, center, half, step);
    let mut summary = format!(
        "samples={}\nheight_m={:.2}..{:.2}\nflat_neighbor_ratio={:.4}\nsteep_column_ratio={:.4}\nriver_column_ratio={:.4}\nlake_column_ratio={:.4}\ninland_water_column_ratio={:.4}\ndeposition_column_ratio={:.4}\n",
        diagnostics.samples,
        diagnostics.min_height_m,
        diagnostics.max_height_m,
        diagnostics.flat_neighbor_ratio,
        diagnostics.steep_column_ratio,
        diagnostics.river_column_ratio,
        diagnostics.lake_column_ratio,
        diagnostics.inland_water_column_ratio,
        diagnostics.deposition_column_ratio,
    );
    for region in TerrainRegion::ALL {
        let count = diagnostics.region_counts.get(&region).copied().unwrap_or(0);
        summary.push_str(&format!("region.{}={}\n", region.name(), count));
    }
    let mut biomes: Vec<_> = diagnostics.biome_counts.iter().collect();
    biomes.sort_by_key(|(biome, _)| format!("{biome:?}"));
    for (biome, count) in biomes {
        summary.push_str(&format!("biome.{biome:?}={count}\n"));
    }
    fs::write(output.join("summary.txt"), summary)?;
    Ok(diagnostics)
}

fn write_ppm(path: &Path, width: usize, height: usize, pixels: &[[u8; 3]]) -> io::Result<()> {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    bytes.reserve(pixels.len() * 3);
    for pixel in pixels {
        bytes.extend_from_slice(pixel);
    }
    fs::write(path, bytes)
}

fn gray(value: f64) -> [u8; 3] {
    let value = (value * 255.0) as u8;
    [value, value, value]
}

fn heat(value: f64) -> [u8; 3] {
    [(value * 255.0) as u8, ((1.0 - value) * 210.0) as u8, 35]
}

fn region_color(region: TerrainRegion) -> [u8; 3] {
    match region {
        TerrainRegion::Ocean => [28, 84, 150],
        TerrainRegion::Coast => [220, 196, 118],
        TerrainRegion::Plains => [113, 177, 82],
        TerrainRegion::Hills => [91, 137, 67],
        TerrainRegion::Plateau => [180, 143, 82],
        TerrainRegion::Valley => [69, 151, 118],
        TerrainRegion::Mountains => [124, 126, 132],
    }
}

fn biome_color(biome: Biome) -> [u8; 3] {
    match biome {
        Biome::DeepOcean => [15, 48, 105],
        Biome::Ocean => [30, 94, 164],
        Biome::Beach | Biome::TropicalBeach => [224, 207, 132],
        Biome::Plains | Biome::Meadow => [132, 190, 82],
        Biome::Forest | Biome::DenseForest => [35, 112, 58],
        Biome::AutumnForest => [181, 105, 48],
        Biome::Rainforest => [19, 132, 77],
        Biome::Savanna => [183, 169, 73],
        Biome::Desert | Biome::Badlands => [211, 159, 89],
        Biome::Swamp => [65, 104, 74],
        Biome::Taiga | Biome::SnowyForest => [81, 128, 116],
        Biome::Tundra => [190, 205, 198],
        Biome::Mountains => [128, 130, 135],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::VOXELS_PER_METER;

    #[test]
    fn diagnostics_count_every_sample_and_region() {
        let generator = TerrainGenerator::new(42);
        let diagnostics = TerrainDiagnostics::sample(&generator, IVec2::ZERO, 128, 8);
        assert_eq!(
            diagnostics.region_counts.values().sum::<usize>(),
            diagnostics.samples
        );
        assert_eq!(
            diagnostics.biome_counts.values().sum::<usize>(),
            diagnostics.samples
        );
        assert!(diagnostics.max_height_m > diagnostics.min_height_m);
        assert!((0.0..=1.0).contains(&diagnostics.flat_neighbor_ratio));
        assert!((0.0..=1.0).contains(&diagnostics.steep_column_ratio));
        assert!((0.0..=1.0).contains(&diagnostics.river_column_ratio));
        assert!((0.0..=1.0).contains(&diagnostics.lake_column_ratio));
        assert!((0.0..=1.0).contains(&diagnostics.inland_water_column_ratio));
        assert!((0.0..=1.0).contains(&diagnostics.deposition_column_ratio));
    }

    #[test]
    fn ppm_export_writes_all_debug_layers() {
        let generator = TerrainGenerator::new(7);
        let output = std::env::temp_dir().join(format!(
            "runewild-worldgen-{}-{}",
            std::process::id(),
            VOXELS_PER_METER
        ));
        if output.exists() {
            fs::remove_dir_all(&output).unwrap();
        }
        write_debug_maps(&generator, &output, IVec2::ZERO, 64, 16).unwrap();
        for name in [
            "height.ppm",
            "slope.ppm",
            "biome.ppm",
            "river.ppm",
            "mountain.ppm",
            "region.ppm",
            "geology.ppm",
            "flow.ppm",
            "lakes.ppm",
            "deposition.ppm",
            "water.ppm",
            "summary.txt",
        ] {
            assert!(output.join(name).is_file(), "missing {name}");
        }
        fs::remove_dir_all(output).unwrap();
    }
}
