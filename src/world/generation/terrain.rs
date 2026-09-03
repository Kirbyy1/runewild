//! Terrain shaping and world generation.
//!
//! Height pipeline (macro → regional → local):
//! continental shelf spline → rolling hills → foothills + ridged mountain
//! spines with tall peaks → biome shaping (dunes, soft mesa terraces, swamp
//! depressions) → broad river valleys → fine detail. There is **no gameplay
//! height ceiling**: the only vertical bounds are the sparse-section storage
//! envelope defined in `crate::world`.

use crate::world::{
    coordinates::{ChunkCoord, VoxelCoord},
    generation::{
        biome::Biome,
        noise::{ColumnClimate, WorldNoise},
        regional::{RegionalSample, RegionalTerrain},
        trees::{self, GroundKind, TreeKind, TreeVoxel, SLOT_SIZE},
        SEA_LEVEL_METRES,
    },
    voxel::BlockType,
    CHUNK_SIZE, ENABLE_GROUND_DECOR, SECTION_MAX_Y, SECTION_MIN_Y, VOXELS_PER_METER,
    WORLD_BOTTOM_VOXEL_Y, WORLD_BOTTOM_Y,
};

pub const SEA_LEVEL: i32 = SEA_LEVEL_METRES;
/// Above this height trees stop appearing entirely (rock/snow only).
pub const TREELINE: i32 = 70;
/// Minimum regional water coverage for a column to hold generated water.
/// Below this the bed is either uncarved or only barely notched, so a dry
/// bank reads better than a sliver of water.
const WATER_COVERAGE_THRESHOLD: f64 = 0.15;
const MIN_INLAND_WATER_DEPTH_M: f64 = 0.35;
/// A column claiming substantial water coverage must be backed by channel
/// or lake evidence from its own stamp. Stamps always write both together
/// (bilinear sampling dilutes them together), so a wet-looking field with
/// no watermark at all is a blended stale overlap and stays dry rather
/// than growing a free-standing wall of water.
const UNBACKED_COVERAGE: f64 = 0.40;
const UNBACKED_WATERMARK: f64 = 0.05;
#[cfg(test)]
const MODERATE_WATER_SLOPE: f64 = 0.75;

fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerrainRegion {
    Ocean,
    Coast,
    Plains,
    Hills,
    Plateau,
    Valley,
    Mountains,
}

impl TerrainRegion {
    pub const ALL: [Self; 7] = [
        Self::Ocean,
        Self::Coast,
        Self::Plains,
        Self::Hills,
        Self::Plateau,
        Self::Valley,
        Self::Mountains,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ocean => "ocean",
            Self::Coast => "coast",
            Self::Plains => "plains",
            Self::Hills => "hills",
            Self::Plateau => "plateau",
            Self::Valley => "valley",
            Self::Mountains => "mountains",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainConfig {
    pub plains_relief_m: f64,
    pub hills_relief_m: f64,
    pub plateau_relief_m: f64,
    pub foothill_height_m: f64,
    pub mountain_height_m: f64,
    pub detail_strength: f64,
    pub plain_contour_m: f64,
    pub hill_contour_m: f64,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            // Hytale-style landmass: broad rolling meadows with real
            // vertical presence, chunky hills and commanding ranges.
            plains_relief_m: 1.6,
            hills_relief_m: 9.5,
            plateau_relief_m: 7.5,
            foothill_height_m: 22.0,
            // Compensates the coarser 1 m mountain contour step, which no
            // longer grants peaks their old half-metre snap-ups. Tall enough
            // that flanks read as cliff-and-scare instead of terraced lawn.
            mountain_height_m: 140.0,
            detail_strength: 1.0,
            plain_contour_m: 1.0,
            hill_contour_m: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RegionWeights {
    plains: f64,
    hills: f64,
    plateau: f64,
    valley: f64,
    mountains: f64,
}

impl RegionWeights {
    fn normalized(self) -> Self {
        let sum = self.plains + self.hills + self.plateau + self.valley + self.mountains;
        let scale = 1.0 / sum.max(1.0e-6);
        Self {
            plains: self.plains * scale,
            hills: self.hills * scale,
            plateau: self.plateau * scale,
            valley: self.valley * scale,
            mountains: self.mountains * scale,
        }
    }

    fn dominant(self) -> TerrainRegion {
        let candidates = [
            (self.plains, TerrainRegion::Plains),
            (self.hills, TerrainRegion::Hills),
            (self.plateau, TerrainRegion::Plateau),
            (self.valley, TerrainRegion::Valley),
            (self.mountains, TerrainRegion::Mountains),
        ];
        candidates
            .into_iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, region)| region)
            .unwrap_or(TerrainRegion::Plains)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainColumn {
    /// Rounded one-metre height used by the existing vegetation art system.
    pub height: i32,
    /// Continuous terrain height in metres. Terrain storage quantizes this to
    /// the configured voxel grid.
    pub height_m: f64,
    pub biome: Biome,
    pub region: TerrainRegion,
    pub river_strength: f64,
    pub lake_strength: f64,
    pub flow_strength: f64,
    pub deposition: f64,
    pub process_moisture: f64,
    pub water_level_m: Option<f64>,
    pub ecotone_strength: f64,
    pub slope: f64,
    pub climate: ColumnClimate,
    /// Vegetation density in [0, 1] after biome modulation.
    pub forest_density: f64,
}

pub struct TerrainGenerator {
    seed: u64,
    noise: WorldNoise,
    config: TerrainConfig,
    regional: RegionalTerrain,
}

impl TerrainGenerator {
    pub fn new(seed: u64) -> Self {
        Self::with_config(seed, TerrainConfig::default())
    }

    pub fn with_config(seed: u64, config: TerrainConfig) -> Self {
        Self {
            seed,
            noise: WorldNoise::new(seed),
            config,
            regional: RegionalTerrain::default(),
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Mountain-range mask in [0, 1]; drives elevation, region weights,
    /// biome gates and rock decor. Derived from the anisotropic chain field
    /// so every consumer agrees on where mountains actually are.
    fn mountain_field(&self, x: i32, z: i32, climate: &ColumnClimate) -> f64 {
        self.noise
            .mountain_chain(x, z, climate.continentalness, climate.erosion)
            .mask
    }

    fn region_weights(climate: &ColumnClimate, mountain_mask: f64) -> RegionWeights {
        let mountains = smoothstep(0.20, 0.62, mountain_mask);
        let non_mountain = 1.0 - mountains;
        let valley = non_mountain
            * smoothstep(-0.08, 0.58, climate.erosion)
            * smoothstep(-0.16, 0.54, -climate.elevation)
            * (1.15 + 0.55 * smoothstep(-0.25, 0.45, climate.landform));
        let plateau = non_mountain
            * (1.0 - valley * 0.75)
            * smoothstep(0.05, 0.62, climate.plateau)
            * (1.0 - smoothstep(0.45, 0.88, climate.erosion));
        let plains = non_mountain
            * (1.0 - valley * 0.70)
            * (1.0 - plateau * 0.65)
            * smoothstep(-0.18, 0.58, climate.erosion)
            * (1.0 - smoothstep(0.28, 0.78, climate.landform));
        let hills = non_mountain
            * (1.0 - valley * 0.65)
            * (1.0 - plateau * 0.55)
            * (0.22
                + smoothstep(-0.45, 0.42, -climate.erosion)
                    * smoothstep(-0.55, 0.65, climate.landform));
        RegionWeights {
            plains: plains.max(0.03),
            hills: hills.max(0.03),
            plateau: plateau.max(0.0),
            valley: valley.max(0.0),
            mountains: mountains.max(0.0),
        }
        .normalized()
    }

    fn region_from_climate(&self, x: i32, z: i32, climate: &ColumnClimate) -> TerrainRegion {
        if climate.continentalness < -0.16 {
            TerrainRegion::Ocean
        } else if climate.continentalness < -0.02 {
            TerrainRegion::Coast
        } else {
            let mask = self.mountain_field(x, z, climate);
            Self::region_weights(climate, mask).dominant()
        }
    }

    pub fn mountain_strength_at(&self, x: i32, z: i32) -> f64 {
        let climate = self.noise.climate_at(x, z);
        self.mountain_field(x, z, &climate)
    }

    /// Macro → local terrain height for a world column.
    fn base_landform_height_at(&self, x: i32, z: i32) -> f64 {
        let climate = self.noise.climate_at(x, z);
        let c = climate.continentalness;
        let e = climate.erosion;
        let chain = self.noise.mountain_chain(x, z, c, e);
        let mask = chain.mask;
        let weights = Self::region_weights(&climate, mask);

        // Continental base: oceans, shelves, coastal plains and hinterland.
        // Every submerged tier sits deeper than before so open water reads
        // as genuinely deep sea, while the landward anchor (c >= 0.02) and
        // therefore the coastline itself stay put. The abyssal tier plus
        // seabed relief give far water real basins and ridges.
        // Wider continents: the shelf climbs out of the surf earlier so the
        // world reads as generous landmasses ringed by beaches, not sparse
        // islands in an ocean (Hytale's Orbis is land-forward).
        let continental_base = if c < -0.55 {
            -12.0 + smoothstep(-1.0, -0.55, c) * 17.0
        } else if c < -0.42 {
            5.0
        } else if c < -0.18 {
            5.0 + smoothstep(-0.42, -0.18, c) * 10.0
        } else if c < 0.02 {
            15.0 + smoothstep(-0.18, 0.02, c) * 10.0
        } else if c < 0.50 {
            25.0 + smoothstep(0.02, 0.50, c) * 8.0
        } else {
            33.0 + smoothstep(0.50, 1.0, c) * 9.0
        };

        // Each region owns a distinct height profile. Continuous weights make
        // their borders broad transitions instead of hard biome seams.
        let plains = continental_base + climate.elevation * self.config.plains_relief_m;
        let hills = continental_base + climate.elevation * self.config.hills_relief_m;
        let valley = continental_base - 2.4 - climate.elevation.abs() * 1.3;
        let plateau_level = continental_base + climate.plateau * self.config.plateau_relief_m;
        let plateau = (plateau_level / 2.0).round() * 2.0 + climate.elevation * 0.45;
        // Mountain ranges come from the anisotropic chain field: meandered
        // corridors, ridged spines, massif domes with passes, foothill
        // aprons and flanking moat valleys (see noise::mountain_chain).
        let mountains = continental_base + climate.elevation * 2.2 + chain.height;
        let mut raw = plains * weights.plains
            + hills * weights.hills
            + plateau * weights.plateau
            + valley * weights.valley
            + mountains * weights.mountains;
        // (Mountain spur/gully structure is applied in the regional pass,
        // after hydrology, so gullies never fight carved channels.)

        // Sparse landmarks interrupt the otherwise statistically uniform
        // region profiles. They are broad enough to read as places rather
        // than additional surface noise.
        let inland = smoothstep(-0.02, 0.28, c);
        let isolated_peak = smoothstep(0.54, 0.88, climate.landmark) * (1.0 - mask) * inland * 8.5;
        let basin =
            smoothstep(0.58, 0.90, -climate.landmark) * smoothstep(0.10, 0.65, e) * inland * 4.5;
        let escarpment = smoothstep(0.16, 0.62, climate.plateau.abs())
            * smoothstep(-0.08, 0.22, climate.escarpment)
            * (1.0 - weights.valley)
            * 3.2;
        let mountain_pass = mask
            * (1.0 - smoothstep(0.08, 0.42, climate.peaks.abs()))
            * smoothstep(-0.20, 0.45, climate.erosion)
            * 5.0;
        raw += isolated_peak + escarpment - basin - mountain_pass;

        // Deep seabed relief: broad drowned dunes and low ridges that grow
        // with distance from shore. Shallow water stays clean and readable;
        // only genuinely deep floors receive the swell, so depth-based
        // water colour gets real structure to grade over.
        let abyssal = smoothstep(-0.32, -0.70, c);
        if abyssal > 0.0 {
            raw += abyssal * self.noise.seabed_relief(x, z) * 4.5;
        }

        // Biome-specific shaping is secondary to landform structure.
        let desert_zone =
            climate.temperature > 0.20 && climate.humidity < -0.20 && c > -0.05 && mask < 0.30;
        if desert_zone {
            raw += climate.dunes * 1.8;
        }
        if climate.temperature > 0.24 && climate.humidity < -0.28 && e < -0.05 && c > 0.0 {
            let zone = smoothstep(-0.28, -0.45, climate.humidity);
            let step_h = 5.0 + (trees::hash(self.seed, x / 24, z / 24, 311) % 100) as f64 * 0.03;
            let terraced = (raw / step_h).round() * step_h;
            raw = raw * (1.0 - 0.55 * zone) + terraced * 0.55 * zone;
        }
        if e > 0.30 && climate.humidity > 0.25 && c > -0.05 {
            let swamp = smoothstep(0.25, 0.70, climate.humidity);
            raw = raw * (1.0 - 0.55 * swamp) + (SEA_LEVEL as f64 + 1.0) * 0.55 * swamp;
        }

        // Slope-aware flattening gives valleys floors and mountains readable
        // summits while preserving steep sides between them. The summit
        // blend stays shallow: full 2 m crown shelves read as wedding-cake
        // tiers from any distance.
        let valley_floor = (raw * 2.0).round() * 0.5;
        raw = raw * (1.0 - weights.valley * 0.72) + valley_floor * weights.valley * 0.72;
        let summit = smoothstep(0.76, 0.96, mask);
        let summit_level = (raw / 2.0).round() * 2.0;
        raw = raw * (1.0 - summit * 0.30) + summit_level * summit * 0.30;

        let detail_strength = self.config.detail_strength
            * (weights.plains * 0.10
                + weights.hills * 0.42
                + weights.plateau * 0.14
                + weights.valley * 0.08
                // Mountains need enough medium-frequency relief for gullies
                // and spurs; without it thermal erosion aligns every slope
                // into concentric contour benches.
                + weights.mountains * 2.1);
        raw += climate.detail * detail_strength;

        // Shape and material use the same rocky province field. This creates
        // deliberate outcrops instead of sprinkling unrelated stone patches
        // over otherwise smooth terrain.
        let rocky_province = self.noise.rocky_patch_field(x, z);
        let outcrop = smoothstep(0.62, 0.90, rocky_province)
            * (weights.hills * 1.8 + weights.mountains * 4.2)
            * (1.0 - smoothstep(0.30, 0.82, e));
        raw += outcrop;

        // Plateau provinces receive broad, irregular shelves. The phase is
        // coherent across the region, so ledges read as authored formations
        // rather than identical contour rings around every hill.
        let shelf_strength = weights.plateau
            * smoothstep(0.18, 0.72, climate.escarpment.abs())
            * (0.28 + 0.30 * smoothstep(-0.15, 0.65, climate.geology));
        let shelf_step = 2.0 + smoothstep(-0.35, 0.65, climate.geology) * 2.0;
        let shelf_phase = climate.landform * shelf_step * 0.42;
        let shelved = ((raw + shelf_phase) / shelf_step).round() * shelf_step - shelf_phase;
        raw = raw * (1.0 - shelf_strength) + shelved * shelf_strength;

        // No gameplay ceiling: only the storage sanity envelope applies.
        raw.clamp(
            (WORLD_BOTTOM_Y + 2) as f64,
            (((SECTION_MAX_Y + 1) * CHUNK_SIZE) / VOXELS_PER_METER - 10) as f64,
        )
    }

    fn regional_sample_at(&self, x: i32, z: i32) -> RegionalSample {
        self.regional.sample(
            x,
            z,
            |sample_x, sample_z| self.base_landform_height_at(sample_x, sample_z),
            |sample_x, sample_z| {
                let climate = self.noise.climate_at(sample_x, sample_z);
                (climate.humidity * 0.5 + 0.5).clamp(0.0, 1.0)
            },
            // Mountain spur/gully structure: ridged multi-scale noise gated
            // by the mountain mask, applied by the regional pass after
            // hydrology stamping.
            |sample_x, sample_z| {
                let mask = self.noise.mountain_mask_hint(sample_x, sample_z);
                (mask
                    * (self
                        .noise
                        .mountain_spur_field(sample_x as f64, sample_z as f64)
                        - 0.45)
                    * 12.0) as f32
            },
        )
    }

    fn filtered_water_sample(&self, x: i32, z: i32, mut center: RegionalSample) -> RegionalSample {
        if center.water_coverage < WATER_COVERAGE_THRESHOLD {
            return center;
        }

        let mut surface_sum = 0.0;
        let mut weight_sum = 0.0;
        for dz in -2i32..=2 {
            for dx in -2i32..=2 {
                let sample = if dx == 0 && dz == 0 {
                    center
                } else {
                    self.regional_sample_at(x + dx, z + dz)
                };
                let Some(surface) = sample.water_surface_m else {
                    continue;
                };
                if sample.water_coverage < 0.14 {
                    continue;
                }
                let spatial_weight = f64::from(3 - dx.abs()) * f64::from(3 - dz.abs());
                // Squaring coverage makes robust wet-core samples dominate
                // halo-edge samples, whose surface values degrade toward
                // noise as their weight fades.
                let weight = spatial_weight * sample.water_coverage * sample.water_coverage;
                surface_sum += surface * weight;
                weight_sum += weight;
            }
        }
        if weight_sum > 0.0 {
            center.water_surface_m = Some(surface_sum / weight_sum);
        }
        center
    }

    fn shaped_height_at(&self, x: i32, z: i32) -> f64 {
        let sample = self.regional_sample_at(x, z);
        let climate = self.noise.climate_at(x, z);
        let chain_mask = self.mountain_field(x, z, &climate);
        let weights = Self::region_weights(&climate, chain_mask);

        // Smooth seabeds: contour quantization underwater turns clear
        // shallows into a visible topographic map.
        if sample.height_m < SEA_LEVEL as f64 - 2.0 && sample.river < 0.20 && sample.lake < 0.15 {
            return sample.height_m;
        }

        // Flooded cells always use the fine contour step: their beds were
        // carved below the water plane by the regional pass, and coarse
        // quantization would bench them back up into ankle-deep fringes
        // that reveal the voxel grid through the surface.
        let contour_step = if sample.water_coverage >= WATER_COVERAGE_THRESHOLD
            || sample.river >= 0.20
            || sample.lake >= 0.15
        {
            0.25
        } else if weights.mountains > 0.30 {
            // Mountains skip quantization entirely: any height snapping on a
            // broad cone produces perfectly aligned contour benches (the
            // "ziggurat" artifact). Smooth 12 m jitter shifts whole ledge
            // sections and a little per-column dither roughens the rest, so
            // riser lines stay irregular.
            let jitter = self.noise.mountain_ledge_jitter(x, z);
            let dither = (f64::from(trees::hash01(self.seed, x, z, 613)) - 0.5) * 0.5;
            return sample.height_m + jitter + dither;
        } else if weights.plains + weights.plateau + weights.valley > 0.58 {
            self.config.plain_contour_m
        } else {
            self.config.hill_contour_m
        };
        // A coherent phase offset breaks the perfectly level, repeating
        // staircase contours without introducing per-block speckle. Detail
        // (22 m wavelength) decorrelates neighbouring contour lines, which
        // landform/geology (300 m) could not.
        let phase = (climate.detail * 0.42 + climate.landform * 0.10) * contour_step;
        let quantized = ((sample.height_m + phase) / contour_step).round() * contour_step - phase;

        // Slope gate: on steep gradients the quantizer produces perfectly
        // aligned staircase benches (the "ziggurat" artifact). Blend toward
        // the raw regional height there; flats keep their readable contours.
        let slope_gate = smoothstep(0.30, 0.80, sample.gradient);
        sample.height_m * slope_gate + quantized * (1.0 - slope_gate)
    }

    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        self.shaped_height_at(x, z).round() as i32
    }

    /// Distance to the nearest river centerline in [0, 1].
    #[allow(dead_code)]
    pub fn river_noise(&self, x: i32, z: i32) -> f64 {
        1.0 - self.regional_sample_at(x, z).river
    }

    /// Channel factor in [0, 1]; 1 = river bed.
    pub fn river_strength_at(&self, x: i32, z: i32) -> f64 {
        self.regional_sample_at(x, z).river
    }

    /// Broad valley factor in [0, 1]; shapes the land far beyond the banks.
    fn river_valley_at(&self, x: i32, z: i32) -> f64 {
        self.regional_sample_at(x, z).valley
    }

    /// Selects the biome from the multi-dimensional climate map plus height.
    pub fn biome_at(&self, x: i32, z: i32) -> Biome {
        let climate = self.noise.climate_at(x, z);
        let height = self.height_at(x, z);
        let c = climate.continentalness;
        let t = climate.temperature;
        let h = climate.humidity;
        let e = climate.erosion;

        if height <= SEA_LEVEL - 9 || c < -0.48 {
            return Biome::DeepOcean;
        }
        if height < SEA_LEVEL {
            return Biome::Ocean;
        }

        // Classify beaches from the actual shoreline, not an unrelated
        // continental threshold. This keeps sand and palms wrapped around the
        // coast even when erosion or a landform shifts the local elevation.
        if height <= SEA_LEVEL + 3 && c < 0.72 {
            if t > 0.15 && h > -0.10 {
                return Biome::TropicalBeach;
            }
            return Biome::Beach;
        }

        // Alpine mountains: real elevation, not just a ridge blip.
        let mask = self.mountain_field(x, z, &climate);
        let region = self.region_from_climate(x, z, &climate);
        if height >= 56
            || (mask > 0.76 && height >= 42)
            || (region == TerrainRegion::Mountains && height >= 40)
        {
            return Biome::Mountains;
        }

        // Lowland swamps.
        if e > 0.30 && h > 0.25 && height <= SEA_LEVEL + 3 && c > -0.05 {
            return Biome::Swamp;
        }

        // Temperature ladder.
        if t < -0.35 {
            if h < -0.15 {
                Biome::Tundra
            } else {
                Biome::SnowyForest
            }
        } else if t < -0.10 {
            Biome::Taiga
        } else if t > 0.26 {
            if h < -0.28 {
                if e < -0.12 || climate.peaks > 0.30 {
                    Biome::Badlands
                } else {
                    Biome::Desert
                }
            } else if h < 0.12 {
                Biome::Savanna
            } else {
                Biome::Rainforest
            }
        } else if h < -0.22 {
            Biome::Plains
        } else if h < 0.06 {
            Biome::Meadow
        } else if h < 0.36 {
            if t < 0.06 && climate.peaks > 0.12 {
                Biome::AutumnForest
            } else {
                Biome::Forest
            }
        } else {
            Biome::DenseForest
        }
    }

    fn snowline(&self, x: i32, z: i32, temperature: f64) -> i32 {
        let base = if temperature < -0.25 {
            SNOWLINE_COLD
        } else if temperature < 0.05 {
            SNOWLINE_TEMPERATE
        } else {
            SNOWLINE_WARM
        };
        // Noisy edge so the snow line is not a perfect contour.
        base + ((trees::hash(self.seed, x / 3, z / 3, 331) % 5) as i32 - 2)
    }

    /// Full per-column sample used by generation and decoration passes.
    pub fn column_at(&self, x: i32, z: i32) -> TerrainColumn {
        let climate = self.noise.climate_at(x, z);
        let regional = self.filtered_water_sample(x, z, self.regional_sample_at(x, z));
        let height_m = self.shaped_height_at(x, z);
        let height = height_m.round() as i32;
        let step = 2;
        let neighbors = [
            self.height_at(x - step, z),
            self.height_at(x + step, z),
            self.height_at(x, z - step),
            self.height_at(x, z + step),
        ];
        let slope = neighbors
            .into_iter()
            .map(|n| (n - height).abs() as f64 / step as f64)
            .fold(0.0, f64::max);

        let biome = self.biome_at(x, z);
        let region = self.region_from_climate(x, z, &climate);
        let region_forest_factor = match region {
            TerrainRegion::Ocean => 0.0,
            TerrainRegion::Coast => 0.74,
            TerrainRegion::Plains => 0.85,
            TerrainRegion::Hills => 1.08,
            TerrainRegion::Plateau => 0.86,
            TerrainRegion::Valley => 0.88,
            TerrainRegion::Mountains => 1.08,
        };
        let ecotone_strength = climate_boundary_strength(&climate);
        let forest_density = (self.forest_density_at(x, z, height, slope, biome, &climate)
            * region_forest_factor
            * (0.84 + regional.moisture * 0.32)
            * (1.0 - ecotone_strength * 0.12)
            * (1.0 - regional.river * 0.20))
            .clamp(0.0, 1.0);
        // The sea always wins below sea level: inland river/lake stamps may
        // carry surfaces above or below the ocean surface near coasts, and
        // letting them override here raised floating water blocks and punched
        // notches into the ocean surface. Inland water only applies at or
        // above sea level.
        let water_level_m = if height_m < SEA_LEVEL as f64 {
            Some(SEA_LEVEL as f64)
        } else {
            inland_water_level(height_m, slope, regional)
        };

        TerrainColumn {
            height,
            height_m,
            biome,
            region,
            river_strength: self.river_strength_at(x, z),
            lake_strength: regional.lake,
            flow_strength: regional.flow,
            deposition: regional.deposition,
            process_moisture: regional.moisture,
            water_level_m,
            ecotone_strength,
            slope,
            climate,
            forest_density,
        }
    }

    /// Samples a terrain-storage column from voxel X/Z coordinates while
    /// keeping every noise input in metres.
    pub fn column_at_voxel(&self, voxel_x: i32, voxel_z: i32) -> TerrainColumn {
        let x_m = voxel_x as f64 / VOXELS_PER_METER as f64;
        let z_m = voxel_z as f64 / VOXELS_PER_METER as f64;
        let x0 = x_m.floor() as i32;
        let z0 = z_m.floor() as i32;
        let tx = x_m - x0 as f64;
        let tz = z_m - z0 as f64;

        let mut column = self.column_at(x0, z0);
        let water_level = column.water_level_m;
        let h00 = column.height_m;
        let h10 = self.shaped_height_at(x0 + 1, z0);
        let h01 = self.shaped_height_at(x0, z0 + 1);
        let h11 = self.shaped_height_at(x0 + 1, z0 + 1);
        column.height_m = bilerp(h00, h10, h01, h11, tx, tz);
        column.height = column.height_m.round() as i32;
        column.water_level_m = water_level;
        column
    }

    /// Multi-scale vegetation density in [0, 1]: macro forests, mid groves,
    /// sharp small grove blobs, dampened by clearings, shaped by biome.
    fn forest_density_at(
        &self,
        x: i32,
        z: i32,
        height: i32,
        slope: f64,
        biome: Biome,
        climate: &ColumnClimate,
    ) -> f64 {
        let macro_v = self.noise.macro_vegetation_density(x, z);
        let mid = self.noise.local_vegetation_density(x, z);
        let grove = self.noise.grove_field(x, z);
        let clearing = self.noise.clearing_field(x, z);

        let mut d = macro_v * 0.42 + mid * 0.28 + grove * 0.45 - 0.12;
        d *= 1.0 - 0.55 * smoothstep(0.55, 0.80, clearing);

        // Biome floors/caps create distinct ecosystems from the same field.
        // Lush, overgrown biomes: forests keep a high vegetation floor so
        // canopies close overhead, while open biomes stay genuinely open.
        let (floor, cap) = match biome {
            Biome::Rainforest => (0.62, 1.00),
            Biome::DenseForest => (0.58, 0.98),
            Biome::Forest | Biome::AutumnForest => (0.52, 0.94),
            Biome::Taiga => (0.45, 0.90),
            Biome::SnowyForest => (0.40, 0.85),
            Biome::Swamp => (0.34, 0.72),
            Biome::Meadow => (0.12, 0.46),
            Biome::Plains => (0.05, 0.30),
            Biome::Savanna => (0.08, 0.36),
            Biome::TropicalBeach => (0.25, 0.56),
            Biome::Beach => (0.04, 0.20),
            Biome::Desert => (0.02, 0.20),
            Biome::Badlands => (0.01, 0.14),
            Biome::Mountains => (0.04, 0.40),
            Biome::Tundra => (0.03, 0.24),
            _ => (0.12, 0.60),
        };
        d = floor + cap * d.clamp(0.0, 1.0);

        // Altitude fade towards the treeline; steep slopes thin out.
        d *= 1.0 - smoothstep((TREELINE - 16) as f64, TREELINE as f64, height as f64);
        d *= 1.0 - smoothstep(0.55, 1.30, slope);
        let _ = climate;
        d.clamp(0.0, 1.0)
    }

    /// Generator-side block lookup for arbitrary world coordinates
    /// (including sections that are not currently loaded).
    pub fn generated_block(&self, coord: VoxelCoord) -> BlockType {
        if coord.y < WORLD_BOTTOM_VOXEL_Y {
            // Solid void floor so nothing can fall out of the world.
            return BlockType::Stone;
        }
        let column = self.column_at_voxel(coord.x, coord.z);
        self.generated_block_in_column(coord, column)
    }

    /// Block for one TERRAIN voxel given its pre-sampled column. `coord` is
    /// in voxel space; the column's heights are in metres, so the voxel
    /// Y is converted internally.
    pub fn generated_block_in_column(&self, coord: VoxelCoord, column: TerrainColumn) -> BlockType {
        let vpm = VOXELS_PER_METER;
        let y_m = coord.y as f32 / vpm as f32;
        let surface_top = column.surface_voxel_y();
        let water_top = column.water_top_voxel_y();
        let biome = column.biome;

        if coord.y > surface_top {
            if water_top.is_some_and(|top| coord.y <= top) {
                if biome.is_frozen() && coord.y > water_top.unwrap_or_default() - vpm {
                    BlockType::Ice
                } else {
                    BlockType::Water
                }
            } else {
                BlockType::Air
            }
        } else {
            let depth = surface_top - coord.y;
            // Surface layer: the topmost metre's voxel band.
            if depth < vpm {
                self.surface_block(coord, column, y_m)
            }
            // Subsurface band (3 metres).
            else if depth < 3 * vpm {
                if column.river_strength > 0.48 {
                    BlockType::Gravel
                } else if y_m <= SEA_LEVEL as f32 + 1.0 && is_sandy_shore(biome) {
                    BlockType::Sand
                } else if biome == Biome::Badlands {
                    strata_block(floor_div_i(coord.y, vpm))
                } else if biome == Biome::Desert {
                    BlockType::Sandstone
                } else {
                    biome.subsurface_block()
                }
            }
            // Weathered geological transition below the soil profile.
            else if depth < 8 * vpm {
                self.geology_block(coord, column, depth as f32 / vpm as f32)
            }
            // Deep stone; caves remain gated for the regional terrain pass.
            else {
                if crate::world::ENABLE_CAVES {
                    let density = self.noise.cave_density(
                        floor_div_i(coord.x, vpm),
                        floor_div_i(coord.y, vpm),
                        floor_div_i(coord.z, vpm),
                    );
                    let depth_ratio = (column.height_m - y_m as f64)
                        / (column.height_m - WORLD_BOTTOM_Y as f64).max(1.0);
                    let threshold = 0.46 + depth_ratio * 0.24;
                    if density > threshold {
                        BlockType::Air
                    } else {
                        self.deep_geology_block(coord, column)
                    }
                } else {
                    self.deep_geology_block(coord, column)
                }
            }
        }
    }

    fn geology_block(&self, coord: VoxelCoord, column: TerrainColumn, depth_m: f32) -> BlockType {
        let geology = column.climate.geology;
        if column.biome == Biome::Badlands {
            return strata_block(floor_div_i(coord.y, VOXELS_PER_METER));
        }
        if column.biome == Biome::Desert || geology < -0.68 {
            return BlockType::Sandstone;
        }
        if geology > 0.58 && depth_m < 5.5 {
            return BlockType::Gravel;
        }
        if geology > 0.18
            && column.climate.humidity > 0.28
            && depth_m < 4.5
            && !matches!(column.region, TerrainRegion::Mountains)
        {
            return BlockType::MossStone;
        }
        BlockType::Stone
    }

    fn deep_geology_block(&self, coord: VoxelCoord, column: TerrainColumn) -> BlockType {
        let y_m = floor_div_i(coord.y, VOXELS_PER_METER);
        if column.biome == Biome::Badlands && y_m.rem_euclid(9) <= 1 {
            return strata_block(y_m);
        }
        if column.climate.geology < -0.82 && y_m.rem_euclid(13) <= 1 {
            return BlockType::Sandstone;
        }
        BlockType::Stone
    }

    fn surface_block(&self, coord: VoxelCoord, column: TerrainColumn, y_m: f32) -> BlockType {
        let biome = column.biome;
        let x_m = floor_div_i(coord.x, VOXELS_PER_METER);
        let z_m = floor_div_i(coord.z, VOXELS_PER_METER);
        let snow = self.snowline(x_m, z_m, column.climate.temperature) as f32;

        // Snow caps above the local snowline; bare stone just below it.
        // The transition band and steep faces use hash-dithered snow/stone
        // patches: continuous snow on terraced treads otherwise paints
        // contour rings across the whole mountain face.
        if y_m >= snow {
            // Clustered rock exposure (smooth noise blobs, not per-column
            // speckle): tying stone to steepness painted contour stripes,
            // and pure hash dither read as gray dirt on snow.
            let exposure = self.noise.snow_rock_patch(x_m, z_m);
            let threshold = if y_m < snow + 16.0 { 0.56 } else { 0.64 };
            return if exposure > threshold {
                BlockType::Stone
            } else {
                BlockType::Snow
            };
        }
        let geological_exposure =
            smoothstep(0.35, 0.82, column.climate.geology) * smoothstep(0.45, 1.15, column.slope);
        if column.slope > 1.35
            || (y_m >= snow - 6.0 && column.slope > 0.9)
            || geological_exposure > 0.58
        {
            return if matches!(biome, Biome::Desert | Biome::Badlands) {
                BlockType::Sandstone
            } else if column.climate.humidity > 0.35
                && !matches!(column.region, TerrainRegion::Mountains)
            {
                BlockType::MossStone
            } else if column.climate.geology > 0.72 && column.slope < 1.1 {
                BlockType::Gravel
            } else {
                BlockType::Stone
            };
        }

        if column.river_strength > 0.52 {
            return match biome {
                Biome::Swamp => BlockType::Mud,
                Biome::Desert | Biome::Beach | Biome::TropicalBeach => BlockType::Sand,
                _ => BlockType::Gravel,
            };
        }

        if column.deposition > 0.38 {
            return if matches!(biome, Biome::Desert | Biome::Beach | Biome::TropicalBeach) {
                BlockType::Sand
            } else {
                BlockType::Gravel
            };
        }

        if y_m <= SEA_LEVEL as f32 + 1.0 && is_sandy_shore(biome) {
            let wave_exposure = self.noise.coast_rock(x_m, z_m)
                * smoothstep(
                    0.28,
                    0.92,
                    column.slope + column.climate.escarpment.abs() * 0.35,
                );
            if wave_exposure > 0.52 {
                return if column.slope > 0.82 {
                    BlockType::Stone
                } else {
                    BlockType::Gravel
                };
            }
            return BlockType::Sand;
        }

        if biome == Biome::Badlands {
            return strata_block(floor_div_i(coord.y, VOXELS_PER_METER));
        }

        biome.surface_block()
    }

    /// Evaluates one tree slot on the 7x7 lattice. Pure function of
    /// `(seed, slot)` so results are seamless across chunk borders.
    fn tree_slot_candidate(
        &self,
        cache: &mut ColumnCache,
        sx: i32,
        sz: i32,
    ) -> Option<(VoxelCoord, TreeKind, u32)> {
        let (x, z) = slot_jitter(self.seed, sx, sz);
        let column = cache.column(self, x, z);
        let height = column.height;
        let dry_beach =
            matches!(column.biome, Biome::Beach | Biome::TropicalBeach) && height >= SEA_LEVEL;
        if (!dry_beach && height <= SEA_LEVEL + 1)
            || column.water_level_m.is_some()
            || column.slope > 1.45
            || column.biome.is_aquatic()
        {
            return None;
        }

        // Density gate: the heart of the tiered-forest look.
        let density = column.forest_density;
        let probability = 0.04 + 0.93 * density.powf(1.35);
        if f64::from(trees::hash01(self.seed, sx, sz, 17)) > probability {
            return None;
        }

        let rank = trees::hash(self.seed, sx, sz, 401);

        // Giants are rare landmarks: only in old-growth density and only
        // when this slot wins a 3x3 hash contest.
        let mut kind = self.pick_tree_kind(&column, x, z);
        if matches!(kind, TreeKind::Giant | TreeKind::JungleGiant)
            && (density < 0.72 || !self.slot_is_local_max(sx, sz))
        {
            kind = TreeKind::TallOak;
        }

        // Size-aware exclusion (symmetric): a candidate must yield when a
        // *larger* candidate exists within its own radius, or when an equal
        // radius neighbour wins the rank contest. Losers downgrade, keeping
        // density while enforcing size separation deterministically.
        loop {
            let my_radius = kind.exclusion_slots();
            if !self.spacing_conflict(cache, sx, sz, rank, my_radius) {
                break;
            }
            kind = kind.downgrade()?;
        }
        let resolved_radius = kind.exclusion_slots();
        if resolved_radius > 0 && !self.wins_spacing(cache, sx, sz, rank, resolved_radius) {
            return None;
        }

        // Keep non-giant vegetation out of a giant's trunk zone so emergent
        // crowns read cleanly (small trees may still fringe the crown).
        if !matches!(kind, TreeKind::Giant | TreeKind::JungleGiant)
            && self.too_close_to_giant(cache, sx, sz, x, z)
        {
            return None;
        }
        if !self.tree_site_is_grounded(cache, x, z, kind) {
            return None;
        }

        Some((
            VoxelCoord {
                x,
                y: height + 1,
                z,
            },
            kind,
            rank,
        ))
    }

    fn tree_site_is_grounded(
        &self,
        cache: &mut ColumnCache,
        x: i32,
        z: i32,
        kind: TreeKind,
    ) -> bool {
        let spec = kind.spec(self.seed, x, z);
        let radius = if spec.thickness > 1 { 1 } else { 0 };
        let mut min_height = i32::MAX;
        let mut max_height = i32::MIN;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let column = cache.column(self, x + dx, z + dz);
                if column.biome.is_aquatic()
                    || column.water_level_m.is_some()
                    || column.slope > 0.95
                {
                    return false;
                }
                min_height = min_height.min(column.height);
                max_height = max_height.max(column.height);
            }
        }
        max_height - min_height <= 1
    }

    /// True when `(x, z)` lies within the trunk-clearance radius of a slot
    /// whose resolved winner is a giant.
    fn too_close_to_giant(
        &self,
        cache: &mut ColumnCache,
        sx: i32,
        sz: i32,
        x: i32,
        z: i32,
    ) -> bool {
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nsx = sx + dx;
                let nsz = sz + dz;
                if !self.giant_wins_slot(cache, nsx, nsz) {
                    continue;
                }
                let (gx, gz) = slot_jitter(self.seed, nsx, nsz);
                let dist_sq = ((gx - x).pow(2) + (gz - z).pow(2)) as f64;
                if dist_sq < 25.0 {
                    return true;
                }
            }
        }
        false
    }

    /// Full deterministic evaluation of whether a slot resolves to a giant.
    fn giant_wins_slot(&self, cache: &mut ColumnCache, sx: i32, sz: i32) -> bool {
        let (x, z) = slot_jitter(self.seed, sx, sz);
        let column = cache.column(self, x, z);
        if column.forest_density < 0.72
            || column.height <= SEA_LEVEL + 1
            || column.river_strength > 0.28
        {
            return false;
        }
        if !self.slot_is_local_max(sx, sz) {
            return false;
        }
        if !self.tree_site_is_grounded(cache, x, z, TreeKind::Giant) {
            return false;
        }
        let rank = trees::hash(self.seed, sx, sz, 401);
        !self.spacing_conflict(cache, sx, sz, rank, 2)
    }

    fn pick_tree_kind(&self, column: &TerrainColumn, x: i32, z: i32) -> TreeKind {
        use TreeKind::*;
        let blend = self.noise.foliage_blend(x, z);
        let dense = column.forest_density;
        if column.ecotone_strength > 0.48
            && trees::hash01(self.seed, x, z, 887) < column.ecotone_strength as f32 * 0.38
        {
            return if column.climate.temperature < -0.08 {
                if blend > 0.0 {
                    Birch
                } else {
                    PineSmall
                }
            } else if column.climate.temperature > 0.22 && column.climate.humidity < 0.12 {
                if blend > 0.0 {
                    Acacia
                } else {
                    YoungOak
                }
            } else if column.process_moisture > 0.68 {
                SwampWillow
            } else if blend > 0.25 {
                Birch
            } else {
                YoungOak
            };
        }
        match column.biome {
            Biome::Forest => {
                if dense > 0.90 && blend > 0.65 {
                    Giant // rare old-growth emergent
                } else if dense > 0.75 && blend > 0.25 {
                    TallOak
                } else if blend < -0.45 {
                    Birch
                } else if dense > 0.62 && blend > 0.55 {
                    WideOak
                } else {
                    Oak
                }
            }
            Biome::AutumnForest => {
                if dense > 0.78 && blend > 0.30 {
                    WideOak
                } else {
                    AutumnOak
                }
            }
            Biome::DenseForest => {
                if dense > 0.88 && blend > 0.55 {
                    Giant
                } else if dense > 0.80 && blend > 0.20 {
                    TallOak
                } else if blend < -0.50 {
                    Birch
                } else {
                    Oak
                }
            }
            Biome::Rainforest => {
                if dense > 0.85 {
                    JungleGiant
                } else if blend > 0.10 {
                    JungleMedium
                } else if blend < -0.28 {
                    Palm
                } else {
                    Oak
                }
            }
            Biome::Taiga => {
                if blend > 0.42 {
                    PineTall
                } else if blend > -0.15 {
                    PineMedium
                } else if blend < -0.60 {
                    Birch
                } else {
                    PineSmall
                }
            }
            Biome::SnowyForest => {
                if blend > 0.25 {
                    PineSnowy
                } else {
                    PineMedium
                }
            }
            Biome::Savanna => {
                if blend > -0.30 {
                    Acacia
                } else {
                    Oak
                }
            }
            Biome::Swamp => SwampWillow,
            Biome::TropicalBeach => Palm,
            Biome::Beach => {
                if column.climate.temperature < -0.08 {
                    PineSmall
                } else {
                    YoungOak
                }
            }
            Biome::Plains => {
                if blend > 0.30 {
                    Birch
                } else if blend < -0.40 && dense > 0.18 {
                    Oak
                } else {
                    YoungOak
                }
            }
            Biome::Meadow => {
                if blend < -0.30 && dense > 0.30 {
                    WideOak
                } else if dense > 0.22 {
                    Oak
                } else {
                    YoungOak
                }
            }
            Biome::Mountains | Biome::Tundra => {
                if blend > 0.0 {
                    PineSmall
                } else {
                    PineMedium
                }
            }
            _ => Oak,
        }
    }

    /// Cheap rank-only probe used by spacing checks.
    fn raw_slot_rank(
        &self,
        cache: &mut ColumnCache,
        sx: i32,
        sz: i32,
    ) -> Option<(VoxelCoord, TreeKind, u32)> {
        let (x, z) = slot_jitter(self.seed, sx, sz);
        let column = cache.column(self, x, z);
        let probability = 0.04 + 0.93 * column.forest_density.powf(1.35);
        if f64::from(trees::hash01(self.seed, sx, sz, 17)) > probability {
            return None;
        }
        if column.height <= SEA_LEVEL + 1 || column.river_strength > 0.28 {
            return None;
        }
        let mut kind = self.pick_tree_kind(&column, x, z);
        if matches!(kind, TreeKind::Giant | TreeKind::JungleGiant)
            && (column.forest_density < 0.72 || !self.slot_is_local_max(sx, sz))
        {
            kind = TreeKind::TallOak;
        }
        if !self.tree_site_is_grounded(cache, x, z, kind) {
            return None;
        }
        Some((
            VoxelCoord {
                x,
                y: column.height + 1,
                z,
            },
            kind,
            trees::hash(self.seed, sx, sz, 401),
        ))
    }

    /// Symmetric spacing rule: conflict when a neighbour within `radius`
    /// slots is *larger* (bigger exclusion always wins) or equally large
    /// with a better rank.
    fn spacing_conflict(
        &self,
        cache: &mut ColumnCache,
        sx: i32,
        sz: i32,
        rank: u32,
        radius: i32,
    ) -> bool {
        if radius == 0 {
            return false;
        }
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx == 0 && dz == 0 {
                    continue;
                }
                if let Some((_, other_kind, other_rank)) =
                    self.raw_slot_rank(cache, sx + dx, sz + dz)
                {
                    let other_radius = other_kind.exclusion_slots();
                    if other_radius > radius || (other_radius == radius && other_rank > rank) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// True when this slot's rank beats every candidate within `radius`
    /// slots (Chebyshev distance).
    fn wins_spacing(
        &self,
        cache: &mut ColumnCache,
        sx: i32,
        sz: i32,
        rank: u32,
        radius: i32,
    ) -> bool {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx == 0 && dz == 0 {
                    continue;
                }
                if let Some((_, _, other)) = self.raw_slot_rank(cache, sx + dx, sz + dz) {
                    if other > rank {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Giants additionally require winning a plain hash contest with their
    /// eight neighbouring slots.
    fn slot_is_local_max(&self, sx: i32, sz: i32) -> bool {
        let mine = trees::hash(self.seed, sx, sz, 517);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if (dx != 0 || dz != 0) && trees::hash(self.seed, sx + dx, sz + dz, 517) > mine {
                    return false;
                }
            }
        }
        true
    }

    /// Evaluates one decoration slot on the 4x4 lattice with *contextual*
    /// placement: rocks cluster near cliffs/mountains/rivers/coasts and in
    /// dedicated rocky patches; logs only appear inside forests.
    fn ground_slot_candidate(
        &self,
        cache: &mut ColumnCache,
        gx: i32,
        gz: i32,
        accepted_trees: &[(VoxelCoord, TreeKind)],
    ) -> Option<(VoxelCoord, GroundKind, BlockType)> {
        let jx = (trees::hash(self.seed, gx, gz, 71) % DECOR as u32) as i32;
        let jz = (trees::hash(self.seed, gx, gz, 73) % DECOR as u32) as i32;
        let x = gx * DECOR + jx;
        let z = gz * DECOR + jz;

        let column = cache.column(self, x, z);
        let height = column.height;
        if height <= SEA_LEVEL
            || column.river_strength > 0.45
            || column.biome.is_aquatic()
            || column.slope > 1.9
        {
            return None;
        }

        // Keep clear of trunks (footprint-aware, not blanket radius): dense
        // forests previously suppressed nearly all ground decor because
        // every floor cell sat within 3 blocks of some trunk.
        for (pos, kind) in accepted_trees {
            let clearance = match kind.thickness() {
                2 => 2,
                3 => 3,
                _ => 1,
            };
            if (pos.x - x).abs() <= clearance && (pos.z - z).abs() <= clearance {
                return None;
            }
        }

        let climate = &column.climate;
        let mask = self.mountain_field(x, z, climate);
        let rocky_patch = self.noise.rocky_patch_field(x, z);
        let valley = self.river_valley_at(x, z);
        let coastal = self.coast_rock_factor(x, z);

        let rock_p = 0.004
            + 0.65 * smoothstep(0.95, 1.9, column.slope)
            + 0.55 * mask
            + 0.42 * smoothstep(0.68, 0.90, rocky_patch)
            + 0.35 * valley * smoothstep(0.08, 0.30, column.river_strength)
            + 0.40 * coastal;

        let forest = column.forest_density;
        let log_p = 0.005 + 0.16 * smoothstep(0.42, 0.85, forest);

        let bush_p = if forest > 0.35 {
            0.14 + 0.38 * smoothstep(0.60, 0.90, self.noise.bush_patch_field(x, z))
        } else {
            0.03
        };

        let roll = f64::from(trees::hash01(self.seed, gx, gz, 89));
        let pick = rock_p + log_p + bush_p;
        if roll > pick {
            return None;
        }

        let kind_roll = trees::hash(self.seed, gx, gz, 93);
        let bush_leaf = bush_leaf_for(column.biome);
        let kind = if roll < rock_p {
            // Size ladder: boulders belong to mountains/rocky hearts.
            match kind_roll % 100 {
                0..=2 if mask > 0.65 => GroundKind::MegaBoulder,
                0..=17 if rocky_patch > 0.66 || mask > 0.4 => GroundKind::Boulder,
                0..=52 => GroundKind::Rock,
                _ => GroundKind::Pebble,
            }
        } else if roll < rock_p + log_p {
            GroundKind::FallenLog
        } else {
            GroundKind::Bush
        };

        Some((
            VoxelCoord {
                x,
                y: height + 1,
                z,
            },
            kind,
            bush_leaf,
        ))
    }

    /// Rocky-shore factor in [0, 1] for offshore rocks and stony beaches.
    fn coast_rock_factor(&self, x: i32, z: i32) -> f64 {
        let climate = self.noise.climate_at(x, z);
        let c = climate.continentalness;
        if !(-0.20..0.06).contains(&c) {
            return 0.0;
        }
        smoothstep(0.25, 0.75, self.noise.coast_rock(x, z))
    }

    /// Generates all vegetation and decorations for one chunk column.
    ///
    /// Tree/decoration builders work in one-metre "art" units; the results
    /// are expanded onto the `VOXELS_PER_METER` grid so vegetation keeps
    /// its authored shape at the finer terrain resolution. The returned list
    /// spans the whole column (trees cross section borders); vertical
    /// placement is clamped to the storage envelope.
    pub fn vegetation_for_chunk(&self, chunk_coord: ChunkCoord) -> Vec<TreeVoxel> {
        let mut cache = ColumnCache::default();
        let bx = floor_div_i(chunk_coord.x * CHUNK_SIZE, VOXELS_PER_METER);
        let bz = floor_div_i(chunk_coord.z * CHUNK_SIZE, VOXELS_PER_METER);
        let column_size_m = CHUNK_SIZE / VOXELS_PER_METER;

        // Pass 1: tree slots (with margin so canopies reach across borders).
        let pad = 12;
        let sx_min = floor_div_i(bx - pad - SLOT_SIZE, SLOT_SIZE);
        let sx_max = floor_div_i(bx + column_size_m + pad, SLOT_SIZE);
        let sz_min = floor_div_i(bz - pad - SLOT_SIZE, SLOT_SIZE);
        let sz_max = floor_div_i(bz + column_size_m + pad, SLOT_SIZE);

        let mut accepted: Vec<(VoxelCoord, TreeKind)> = Vec::new();
        for sx in sx_min..=sx_max {
            for sz in sz_min..=sz_max {
                if let Some((pos, kind, _)) = self.tree_slot_candidate(&mut cache, sx, sz) {
                    accepted.push((pos, kind));
                }
            }
        }

        let vpm = VOXELS_PER_METER;
        let vertical_lo = SECTION_MIN_Y * CHUNK_SIZE;
        let vertical_hi = (SECTION_MAX_Y + 1) * CHUNK_SIZE;

        let mut out: Vec<TreeVoxel> = Vec::new();

        for (pos, kind) in &accepted {
            let spec = kind.spec(self.seed, pos.x, pos.z);
            let surface_offset = self
                .column_at_voxel(pos.x * vpm, pos.z * vpm)
                .surface_voxel_y()
                + 1
                - pos.y * vpm;
            for v in trees::build_tree(self.seed, *pos, &spec) {
                expand_art_voxel(&v, vpm, surface_offset, vertical_lo, vertical_hi, &mut out);
            }
        }

        if ENABLE_GROUND_DECOR {
            let gx_min = floor_div_i(bx - 8 - DECOR, DECOR);
            let gx_max = floor_div_i(bx + column_size_m + 8, DECOR);
            let gz_min = floor_div_i(bz - 8 - DECOR, DECOR);
            let gz_max = floor_div_i(bz + column_size_m + 8, DECOR);
            for gx in gx_min..=gx_max {
                for gz in gz_min..=gz_max {
                    if let Some((pos, kind, leaf)) =
                        self.ground_slot_candidate(&mut cache, gx, gz, &accepted)
                    {
                        let surface_offset = self
                            .column_at_voxel(pos.x * vpm, pos.z * vpm)
                            .surface_voxel_y()
                            + 1
                            - pos.y * vpm;
                        for v in trees::build_ground(self.seed, pos, kind, leaf) {
                            expand_art_voxel(
                                &v,
                                vpm,
                                surface_offset,
                                vertical_lo,
                                vertical_hi,
                                &mut out,
                            );
                        }
                    }
                }
            }
        }

        // Ground plants last so they never overwrite decor geometry.
        for v in self.plants_for_chunk(&mut cache, chunk_coord, &accepted) {
            out.push(v);
        }

        // Margin candidates are needed for seamless canopies, but this call
        // owns only one storage column. Keep the generated overlap in the
        // section that actually contains it instead of modulo-wrapping it.
        out.retain(|voxel| voxel.coord.chunk() == chunk_coord);
        out
    }

    /// Ground plant pass: grass tufts, flowers, ferns and mushrooms placed
    /// in coherent clumps on a fine 2 m lattice. Pure `(seed, slot)` like
    /// every other decoration stage, so chunk borders stay seamless.
    fn plants_for_chunk(
        &self,
        cache: &mut ColumnCache,
        chunk_coord: ChunkCoord,
        accepted_trees: &[(VoxelCoord, TreeKind)],
    ) -> Vec<TreeVoxel> {
        const TUFT_LATTICE: i32 = 2;

        let bx = floor_div_i(chunk_coord.x * CHUNK_SIZE, VOXELS_PER_METER);
        let bz = floor_div_i(chunk_coord.z * CHUNK_SIZE, VOXELS_PER_METER);
        let column_size_m = CHUNK_SIZE / VOXELS_PER_METER;
        // Plants are single voxels and cannot straddle borders, so unlike
        // trees/decor this lattice needs no evaluation pad.
        let gx_min = floor_div_i(bx + TUFT_LATTICE - 1, TUFT_LATTICE);
        let gx_max = floor_div_i(bx + column_size_m - 1, TUFT_LATTICE);
        let gz_min = floor_div_i(bz + TUFT_LATTICE - 1, TUFT_LATTICE);
        let gz_max = floor_div_i(bz + column_size_m - 1, TUFT_LATTICE);

        let vertical_lo = SECTION_MIN_Y * CHUNK_SIZE;
        let vertical_hi = (SECTION_MAX_Y + 1) * CHUNK_SIZE;
        let vpm = VOXELS_PER_METER;

        let mut out = Vec::new();
        for gx in gx_min..=gx_max {
            for gz in gz_min..=gz_max {
                let jx = (trees::hash(self.seed, gx, gz, 401) % TUFT_LATTICE as u32) as i32;
                let jz = (trees::hash(self.seed, gx, gz, 409) % TUFT_LATTICE as u32) as i32;
                let x = gx * TUFT_LATTICE + jx;
                let z = gz * TUFT_LATTICE + jz;

                // Clump gate: outside meadow patches the cost is one noise
                // lookup per 4 m².
                let clump = self.noise.meadow_patch_field(x, z);
                if clump < 0.14 {
                    continue;
                }

                let column = cache.column(self, x, z);
                if column.height <= SEA_LEVEL + 1
                    || column.water_level_m.is_some()
                    || column.slope > 0.8
                    || column.river_strength > 0.45
                    || column.biome.is_aquatic()
                    || matches!(column.biome, Biome::Desert | Biome::Badlands)
                {
                    continue;
                }

                // Yield to trunks: keep the immediate footprint clear.
                if accepted_trees
                    .iter()
                    .any(|(pos, _)| (pos.x - x).abs() <= 1 && (pos.z - z).abs() <= 1)
                {
                    continue;
                }

                // Probabilities inside a clump, shaped by biome character.
                let density = column.forest_density;
                let clearing = 1.0 - smoothstep(0.45, 0.8, density);
                // Overgrown Hytale ground cover: dense tufts and blooming
                // meadows inside every clump patch.
                let tuft_p = (0.32 + 0.68 * clump) * tuft_biome_factor(column.biome);
                let flower_p = (0.16 + 0.44 * clump) * clearing * flower_biome_factor(column.biome);
                let fern_p = 0.30 * smoothstep(0.5, 0.8, density) * fern_biome_factor(column.biome);
                let mushroom_p = 0.12 * smoothstep(0.55, 0.8, density);

                let roll = f64::from(trees::hash01(self.seed, x, z, 419));
                let pick = tuft_p + flower_p + fern_p + mushroom_p;
                if roll > pick {
                    continue;
                }

                let block = if roll < tuft_p {
                    BlockType::GrassTuft
                } else if roll < tuft_p + flower_p {
                    match trees::hash(self.seed, x, z, 431) % 3 {
                        0 => BlockType::FlowersYellow,
                        1 => BlockType::FlowersWhite,
                        _ => BlockType::FlowersRed,
                    }
                } else if roll < tuft_p + flower_p + fern_p {
                    if column.climate.humidity > 0.45 {
                        BlockType::Fern
                    } else {
                        BlockType::GrassTuft
                    }
                } else {
                    BlockType::Mushroom
                };

                let pos = VoxelCoord {
                    x,
                    y: column.height + 1,
                    z,
                };
                if pos.y < vertical_lo || pos.y >= vertical_hi {
                    continue;
                }
                // The cached column already knows the surface height; no
                // fresh pipeline sample needed for a single-voxel plant.
                let surface_offset = column.surface_voxel_y() + 1 - pos.y * vpm;
                expand_art_voxel(
                    &TreeVoxel { coord: pos, block },
                    vpm,
                    surface_offset,
                    vertical_lo,
                    vertical_hi,
                    &mut out,
                );
            }
        }
        out
    }

    /// Samples every microcolumn of a chunk column once; used by the fill
    /// loop so all vertical sections of a column share one sampling pass.
    pub fn sample_column_grid(&self, column: ChunkCoord) -> Box<[TerrainColumn]> {
        let bx = column.x * CHUNK_SIZE;
        let bz = column.z * CHUNK_SIZE;
        let mut cache = std::collections::HashMap::new();
        let mut grid = Vec::with_capacity((CHUNK_SIZE * CHUNK_SIZE) as usize);
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let x_m = (bx + lx) as f64 / VOXELS_PER_METER as f64;
                let z_m = (bz + lz) as f64 / VOXELS_PER_METER as f64;
                let x0 = x_m.floor() as i32;
                let z0 = z_m.floor() as i32;
                let tx = x_m - x0 as f64;
                let tz = z_m - z0 as f64;

                let mut get = |x, z| *cache.entry((x, z)).or_insert_with(|| self.column_at(x, z));
                let mut sample = get(x0, z0);
                let h10 = get(x0 + 1, z0).height_m;
                let h01 = get(x0, z0 + 1).height_m;
                let h11 = get(x0 + 1, z0 + 1).height_m;
                sample.height_m = bilerp(sample.height_m, h10, h01, h11, tx, tz);
                sample.height = sample.height_m.round() as i32;
                grid.push(sample);
            }
        }
        grid.into_boxed_slice()
    }
}

impl TerrainColumn {
    /// Inclusive Y index of the top solid voxel. The `+ VPM - 1`
    /// preserves the original inclusive one-metre top-block semantics.
    pub fn surface_voxel_y(self) -> i32 {
        (self.height_m * VOXELS_PER_METER as f64).floor() as i32 + VOXELS_PER_METER - 1
    }

    pub fn water_top_voxel_y(self) -> Option<i32> {
        self.water_level_m.map(|level| {
            if (level - SEA_LEVEL as f64).abs() < f64::EPSILON {
                sea_top_voxel_y()
            } else {
                (level * VOXELS_PER_METER as f64).floor() as i32 + VOXELS_PER_METER - 1
            }
        })
    }
}

pub const fn sea_top_voxel_y() -> i32 {
    (SEA_LEVEL + 1) * VOXELS_PER_METER - 1
}

fn bilerp(h00: f64, h10: f64, h01: f64, h11: f64, tx: f64, tz: f64) -> f64 {
    let near = h00 + (h10 - h00) * tx;
    let far = h01 + (h11 - h01) * tx;
    near + (far - near) * tz
}

fn climate_boundary_strength(climate: &ColumnClimate) -> f64 {
    let temperature_distance = [-0.35, -0.10, 0.26]
        .into_iter()
        .map(|edge| (climate.temperature - edge).abs())
        .fold(f64::INFINITY, f64::min);
    let humidity_distance = [-0.28, -0.22, 0.06, 0.12, 0.36]
        .into_iter()
        .map(|edge| (climate.humidity - edge).abs())
        .fold(f64::INFINITY, f64::min);
    1.0 - smoothstep(0.015, 0.12, temperature_distance.min(humidity_distance))
}

fn inland_water_level(height_m: f64, _slope: f64, regional: RegionalSample) -> Option<f64> {
    // Watermark evidence: the wet core itself or the faint coverage halo
    // that rides the whole carved channel/lake footprint. The regional pass
    // floors coverage on every carved cell, so anything the sim dug out is
    // guaranteed to clear this gate.
    let influenced = regional.water_coverage >= WATER_COVERAGE_THRESHOLD
        || regional.river >= 0.10
        || regional.lake >= 0.10;
    if !influenced || regional.water_depth_m < MIN_INLAND_WATER_DEPTH_M {
        return None;
    }

    let surface = regional.water_surface_m?;
    let bed_top = height_m.floor();
    // Water is a flat plane: every qualified column fills to the same stamp
    // surface, so lakes and slow rivers read as one clean voxel level that
    // intersects the banks. The bed was carved beneath that surface by the
    // same stamp, so the plane is always terrain-supported.
    let water_top = surface.floor();
    let gap = water_top - bed_top;
    if gap < 1.0 {
        return None;
    }
    let watermark = regional.river.max(regional.lake);
    if regional.water_coverage >= UNBACKED_COVERAGE && watermark < UNBACKED_WATERMARK {
        return None;
    }
    Some(water_top + 0.01)
}

/// Expands one metre-scale art voxel into a VPM³ cube of terrain voxels.
fn expand_art_voxel(
    v: &TreeVoxel,
    vpm: i32,
    vertical_offset: i32,
    vertical_lo: i32,
    vertical_hi: i32,
    out: &mut Vec<TreeVoxel>,
) {
    let ox = v.coord.x * vpm;
    let oy = v.coord.y * vpm + vertical_offset;
    let oz = v.coord.z * vpm;
    for dy in 0..vpm {
        for dz in 0..vpm {
            for dx in 0..vpm {
                let y = oy + dy;
                if y < vertical_lo || y >= vertical_hi {
                    continue;
                }
                out.push(TreeVoxel {
                    coord: VoxelCoord {
                        x: ox + dx,
                        y,
                        z: oz + dz,
                    },
                    block: v.block,
                });
            }
        }
    }
}

/// Finer lattice for ground decoration slots.
const DECOR: i32 = 4;
const SNOWLINE_WARM: i32 = 88;
const SNOWLINE_TEMPERATE: i32 = 76;
const SNOWLINE_COLD: i32 = 62;

/// Per-call cache of sampled terrain columns; generation touches each column
/// many times (terrain fill, slope, candidates), so this is a large win.
#[derive(Default)]
struct ColumnCache {
    map: std::collections::HashMap<(i32, i32), TerrainColumn>,
}

impl ColumnCache {
    fn column(&mut self, gen: &TerrainGenerator, x: i32, z: i32) -> TerrainColumn {
        *self
            .map
            .entry((x, z))
            .or_insert_with(|| gen.column_at(x, z))
    }
}

fn slot_jitter(seed: u64, sx: i32, sz: i32) -> (i32, i32) {
    // Jitter stays inside [1, SLOT_SIZE-2] so two trees in neighbouring
    // slots are always at least ~3 blocks apart on each axis.
    let jx = 1 + (trees::hash(seed, sx, sz, 11) % (SLOT_SIZE - 2) as u32) as i32;
    let jz = 1 + (trees::hash(seed, sx, sz, 13) % (SLOT_SIZE - 2) as u32) as i32;
    (sx * SLOT_SIZE + jx, sz * SLOT_SIZE + jz)
}

fn floor_div_i(a: i32, b: i32) -> i32 {
    let mut q = a / b;
    let r = a % b;
    if r != 0 && ((r > 0) != (b > 0)) {
        q -= 1;
    }
    q
}

fn is_sandy_shore(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::Beach | Biome::TropicalBeach | Biome::Ocean | Biome::DeepOcean
    )
}

fn strata_block(y: i32) -> BlockType {
    if y % 4 == 0 || y % 7 == 0 {
        BlockType::Sandstone
    } else {
        BlockType::Terracotta
    }
}

fn bush_leaf_for(biome: Biome) -> BlockType {
    match biome {
        Biome::Rainforest => BlockType::JungleLeaves,
        Biome::AutumnForest => BlockType::AutumnLeaves,
        Biome::Taiga | Biome::SnowyForest | Biome::Tundra => BlockType::PineLeaves,
        _ => BlockType::Leaves,
    }
}

/// Biome character multipliers for the ground-plant pass: meadows bloom,
/// forests grow ferns and mushrooms, dry lands stay sparse.
fn tuft_biome_factor(biome: Biome) -> f64 {
    match biome {
        Biome::Meadow => 1.7,
        Biome::Plains => 1.1,
        Biome::Savanna => 0.5,
        Biome::Forest | Biome::DenseForest | Biome::AutumnForest => 0.95,
        Biome::Rainforest => 0.75,
        Biome::Taiga | Biome::SnowyForest => 0.4,
        Biome::Tundra => 0.25,
        Biome::Swamp => 0.75,
        Biome::Beach | Biome::TropicalBeach => 0.15,
        Biome::Mountains => 0.55,
        _ => 0.6,
    }
}

fn flower_biome_factor(biome: Biome) -> f64 {
    match biome {
        Biome::Meadow => 1.6,
        Biome::Plains => 0.9,
        Biome::Forest | Biome::AutumnForest => 0.7,
        Biome::DenseForest => 0.4,
        Biome::Savanna => 0.5,
        Biome::Taiga | Biome::SnowyForest | Biome::Tundra => 0.15,
        Biome::Swamp => 0.3,
        _ => 0.25,
    }
}

fn fern_biome_factor(biome: Biome) -> f64 {
    match biome {
        Biome::Rainforest => 1.8,
        Biome::DenseForest => 1.4,
        Biome::Forest | Biome::AutumnForest => 1.0,
        Biome::Swamp => 1.2,
        Biome::Taiga | Biome::SnowyForest => 0.5,
        _ => 0.3,
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;
    use crate::world::generation::trees::TreeKind;

    const SEEDS: [u64; 4] = [7, 42, 1337, 987_654_321];

    fn sampled_columns(seed: u64) -> Vec<TerrainColumn> {
        let gen = TerrainGenerator::new(seed);
        let mut out = Vec::new();
        for z in (-160..160).step_by(8) {
            for x in (-160..160).step_by(8) {
                out.push(gen.column_at(x, z));
            }
        }
        out
    }

    #[test]
    fn terrain_profiles_are_configurable_and_deterministic() {
        let baseline = TerrainGenerator::new(42);
        let config = TerrainConfig {
            hills_relief_m: 0.0,
            mountain_height_m: 25.0,
            ..TerrainConfig::default()
        };
        let configured = TerrainGenerator::with_config(42, config);
        let configured_again = TerrainGenerator::with_config(42, config);
        let mut changed = 0usize;

        for z in (-256..256).step_by(16) {
            for x in (-256..256).step_by(16) {
                let height = configured.height_at(x, z);
                assert_eq!(height, configured_again.height_at(x, z));
                changed += usize::from(height != baseline.height_at(x, z));
            }
        }
        assert!(
            changed > 50,
            "terrain configuration had no meaningful effect"
        );
    }

    #[test]
    fn terrain_regions_are_broad_coherent_and_diverse_across_seeds() {
        let mut combined_regions = std::collections::HashSet::new();
        for seed in SEEDS {
            let gen = TerrainGenerator::new(seed);
            let mut region_counts = std::collections::HashMap::new();
            let mut matching_neighbors = 0usize;
            let mut pairs = 0usize;
            // Continent-scale landmasses push ocean kilometres from the
            // origin, so the window has to span a full province for every
            // configured region to appear.
            for z in (-1024..1024).step_by(16) {
                for x in (-1024..1024).step_by(16) {
                    let region = gen.column_at(x, z).region;
                    combined_regions.insert(region);
                    *region_counts.entry(region).or_insert(0usize) += 1;
                    matching_neighbors += usize::from(gen.column_at(x + 16, z).region == region);
                    matching_neighbors += usize::from(gen.column_at(x, z + 16).region == region);
                    pairs += 2;
                }
            }
            assert!(
                region_counts.len() >= 5,
                "seed {seed}: only {} terrain regions",
                region_counts.len()
            );
            let coherence = matching_neighbors as f64 / pairs as f64;
            assert!(
                coherence > 0.72,
                "seed {seed}: fragmented terrain regions ({coherence:.2})"
            );
        }
        assert_eq!(
            combined_regions.len(),
            TerrainRegion::ALL.len(),
            "one or more configured terrain regions never became dominant"
        );
    }

    #[test]
    fn dry_low_coasts_resolve_to_beaches_at_the_actual_shoreline() {
        for seed in SEEDS {
            let gen = TerrainGenerator::new(seed);
            let mut shoreline = 0usize;
            let mut beaches = 0usize;
            for z in (-512..512).step_by(8) {
                for x in (-512..512).step_by(8) {
                    let column = gen.column_at(x, z);
                    if (SEA_LEVEL..=SEA_LEVEL + 3).contains(&column.height)
                        && (-0.48..0.72).contains(&column.climate.continentalness)
                    {
                        shoreline += 1;
                        beaches += usize::from(matches!(
                            column.biome,
                            Biome::Beach | Biome::TropicalBeach
                        ));
                    }
                }
            }
            assert!(shoreline > 20, "seed {seed}: no sampled dry shoreline");
            assert_eq!(
                beaches, shoreline,
                "seed {seed}: actual shoreline contains non-beach columns"
            );
        }
    }

    #[test]
    fn geological_provinces_create_coherent_non_stone_subsurface_materials() {
        let gen = TerrainGenerator::new(1337);
        let mut non_stone = 0usize;
        let mut samples = 0usize;
        let mut neighboring_geology_delta = 0.0;
        for z in (-1280..1280).step_by(16) {
            for x in (-1280..1280).step_by(16) {
                let column = gen.column_at(x, z);
                if column.height <= SEA_LEVEL + 2 {
                    continue;
                }
                let coord = VoxelCoord::new(
                    x * VOXELS_PER_METER,
                    column.surface_voxel_y() - 4 * VOXELS_PER_METER,
                    z * VOXELS_PER_METER,
                );
                non_stone += usize::from(gen.generated_block(coord) != BlockType::Stone);
                neighboring_geology_delta +=
                    (column.climate.geology - gen.column_at(x + 8, z).climate.geology).abs();
                samples += 1;
            }
        }
        assert!(samples > 1_000);
        assert!(non_stone > 25, "geology never appeared below the surface");
        assert!(
            neighboring_geology_delta / (samples as f64) < 0.16,
            "geological provinces change too abruptly"
        );
    }

    #[test]
    fn voxel_columns_sample_the_same_world_metre_coordinates() {
        let gen = TerrainGenerator::new(42);
        for (x_m, z_m) in [(0, 0), (17, -9), (-33, 41)] {
            let logical = gen.column_at(x_m, z_m);
            let voxel = gen.column_at_voxel(x_m * VOXELS_PER_METER, z_m * VOXELS_PER_METER);
            assert!((logical.height_m - voxel.height_m).abs() < 1.0e-10);
            assert_eq!(logical.biome, voxel.biome);
        }
    }

    #[test]
    fn terrain_voxels_are_minecraft_scale() {
        assert_eq!(VOXELS_PER_METER, 1);
        assert!((crate::world::VOXEL_SIZE_M - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            VoxelCoord::new(-1, 3, 7).world_pos_m(),
            bevy::prelude::Vec3::new(-1.0, 3.0, 7.0)
        );
    }

    #[test]
    fn non_rugged_land_has_more_flats_than_transitions() {
        let gen = TerrainGenerator::new(42);
        let mut flat_pairs = 0usize;
        let mut pairs = 0usize;
        // Ranges now sprawl across several hundred metres, so the window
        // spans multiple provinces and merely needs its lowland share to be
        // predominantly flat rather than assuming the origin cell is plains.
        for z in (-384..384).step_by(8) {
            for x in -384..383 {
                let left = gen.column_at(x, z);
                let right = gen.column_at(x + 1, z);
                let lowland = left.height > SEA_LEVEL + 1
                    && right.height > SEA_LEVEL + 1
                    && gen.mountain_field(x, z, &left.climate) < 0.30
                    && gen.mountain_field(x + 1, z, &right.climate) < 0.30
                    && left.river_strength < 0.20
                    && right.river_strength < 0.20;
                if !lowland {
                    continue;
                }
                pairs += 1;
                if left.surface_voxel_y() == right.surface_voxel_y() {
                    flat_pairs += 1;
                }
            }
        }
        assert!(
            pairs > 1_000,
            "terrain sample did not contain enough lowland"
        );
        assert!(
            flat_pairs * 2 > pairs,
            "lowland still contains more slopes than flats: {flat_pairs}/{pairs}"
        );
    }

    #[test]
    fn generated_surface_matches_inclusive_voxel_height() {
        let gen = TerrainGenerator::new(1337);
        let column = gen.column_at_voxel(-17, 29);
        let top = VoxelCoord {
            x: -17,
            y: column.surface_voxel_y(),
            z: 29,
        };
        assert!(gen.generated_block_in_column(top, column).is_solid());
        let above = top.offset(0, 1, 0);
        assert_eq!(
            gen.generated_block(above),
            gen.generated_block_in_column(above, column)
        );
    }

    #[test]
    fn tree_art_voxel_maps_to_one_world_block() {
        let art = TreeVoxel {
            coord: VoxelCoord { x: -1, y: 2, z: 3 },
            block: BlockType::Wood,
        };
        let mut out = Vec::new();
        expand_art_voxel(&art, VOXELS_PER_METER, 2, -100, 100, &mut out);
        assert_eq!(out.len(), VOXELS_PER_METER.pow(3) as usize);
        assert!(out.iter().all(|voxel| voxel.block == BlockType::Wood));
        assert_eq!(out.iter().map(|v| v.coord.x).min(), Some(-1));
        assert_eq!(out.iter().map(|v| v.coord.x).max(), Some(-1));
        assert_eq!(out.iter().map(|v| v.coord.y).min(), Some(4));
        assert_eq!(out.iter().map(|v| v.coord.y).max(), Some(4));
    }

    #[test]
    fn terrain_has_real_vertical_range_across_seeds() {
        for seed in SEEDS {
            let gen = TerrainGenerator::new(seed);
            // Continent-scale landforms no longer guarantee mountains inside
            // any fixed window around the origin, so anchor the sampling on
            // the strongest mountain signal within a few kilometres (the
            // mask field is cheap: climate + chain, no regional sim).
            let mut anchor = (0i32, 0i32, 0.0f64);
            for z in (-3200..3200).step_by(64) {
                for x in (-3200..3200).step_by(64) {
                    let mask = gen.mountain_strength_at(x, z);
                    if mask > anchor.2 {
                        anchor = (x, z, mask);
                    }
                }
            }
            assert!(
                anchor.2 >= 0.5,
                "seed {seed}: no mountain province within ±3.2 km (best mask {})",
                anchor.2
            );

            let (ax, az) = (anchor.0, anchor.1);
            let mut max = i32::MIN;
            let mut min = i32::MAX;
            let mut mountains = false;
            let mut heights = Vec::new();
            for z in (az - 192..az + 192).step_by(8) {
                for x in (ax - 192..ax + 192).step_by(8) {
                    let column = gen.column_at(x, z);
                    max = max.max(column.height);
                    min = min.min(column.height);
                    mountains |= column.biome == Biome::Mountains;
                    heights.push(column.height);
                }
            }

            assert!(max >= 52, "seed {seed}: no serious elevation (max {max})");
            assert!(
                max - min >= 20,
                "seed {seed}: vertical spread only {}",
                max - min
            );
            assert!(heights.iter().all(|h| *h >= WORLD_BOTTOM_Y + 2));
            assert!(
                heights.iter().all(|h| *h < SECTION_MAX_Y * CHUNK_SIZE),
                "seed {seed}: height exceeds storage envelope"
            );
            assert!(
                mountains,
                "seed {seed}: no mountain biome found near the anchor"
            );
        }
    }

    #[test]
    fn rivers_exist_and_reach_below_sea_level() {
        for seed in SEEDS {
            let gen = TerrainGenerator::new(seed);
            let mut found = false;
            'outer: for z in (-960..960).step_by(12) {
                for x in (-960..960).step_by(12) {
                    if gen.river_strength_at(x, z) > 0.6 && gen.height_at(x, z) <= SEA_LEVEL + 1 {
                        found = true;
                        break 'outer;
                    }
                }
            }
            assert!(found, "seed {seed}: no carved river channel found");
        }
    }

    #[test]
    fn inland_lakes_have_water_above_their_bed() {
        let gen = TerrainGenerator::new(42);
        let mut found = None;
        'scan: for z in (-512..512).step_by(4) {
            for x in (-512..512).step_by(4) {
                let column = gen.column_at(x, z);
                if column.lake_strength > 0.25
                    && column
                        .water_level_m
                        .is_some_and(|water| water > column.height_m)
                {
                    found = Some((x, z, column));
                    break 'scan;
                }
            }
        }
        let (x, z, column) = found.expect("regional drainage produced no inland lake");
        let water = VoxelCoord::new(
            x * VOXELS_PER_METER,
            column.surface_voxel_y() + 1,
            z * VOXELS_PER_METER,
        );
        assert!(matches!(
            gen.generated_block(water),
            BlockType::Water | BlockType::Ice
        ));
    }

    #[test]
    fn floodplains_and_shallow_edges_do_not_create_water_blocks() {
        let floodplain = RegionalSample {
            water_coverage: WATER_COVERAGE_THRESHOLD - 0.01,
            water_surface_m: Some(34.0),
            water_depth_m: 2.0,
            ..RegionalSample::default()
        };
        let shallow_core = RegionalSample {
            water_coverage: 1.0,
            water_surface_m: Some(34.0),
            water_depth_m: MIN_INLAND_WATER_DEPTH_M - 0.01,
            ..RegionalSample::default()
        };
        let channel_core = RegionalSample {
            water_coverage: WATER_COVERAGE_THRESHOLD,
            water_surface_m: Some(33.4),
            water_depth_m: MIN_INLAND_WATER_DEPTH_M,
            ..RegionalSample::default()
        };
        let misplaced_high_surface = RegionalSample {
            water_coverage: 1.0,
            water_surface_m: Some(80.0),
            water_depth_m: 48.0,
            ..RegionalSample::default()
        };

        assert_eq!(inland_water_level(31.4, 0.0, floodplain), None);
        assert_eq!(inland_water_level(31.4, 0.0, shallow_core), None);
        assert_eq!(inland_water_level(31.4, 0.0, channel_core), Some(33.01));
        assert_eq!(
            inland_water_level(31.4, 0.0, misplaced_high_surface),
            None,
            "an overlapping high water stamp must not create a vertical curtain"
        );
        assert_eq!(
            inland_water_level(31.4, 2.0, misplaced_high_surface),
            None,
            "steep water must remain a single terrain-supported layer"
        );
        assert_eq!(
            inland_water_level(31.4, 0.9, misplaced_high_surface),
            None,
            "moderate slopes reject stamps far above the terrain they claim"
        );
    }

    #[test]
    fn generated_inland_water_is_connected_to_an_adjacent_water_column() {
        let gen = TerrainGenerator::new(42);
        let mut water_columns = 0usize;
        let mut connected_columns = 0usize;
        for z in (-256..256).step_by(2) {
            for x in (-256..256).step_by(2) {
                let column = gen.column_at(x, z);
                if column.height_m < SEA_LEVEL as f64 || column.water_level_m.is_none() {
                    continue;
                }
                water_columns += 1;
                let connected = [
                    (-1, -1),
                    (0, -1),
                    (1, -1),
                    (-1, 0),
                    (1, 0),
                    (-1, 1),
                    (0, 1),
                    (1, 1),
                ]
                .into_iter()
                .any(|(dx, dz)| {
                    let neighbor = gen.column_at(x + dx, z + dz);
                    neighbor.height_m >= SEA_LEVEL as f64 && neighbor.water_level_m.is_some()
                });
                connected_columns += usize::from(connected);
            }
        }

        assert!(water_columns > 10, "no inland waterways were generated");
        assert!(
            connected_columns * 100 >= water_columns * 95,
            "isolated inland water columns: {connected_columns}/{water_columns} connected"
        );
    }

    #[test]
    fn inland_water_has_no_enclosed_dry_holes_or_surface_spikes() {
        let gen = TerrainGenerator::new(42);
        let min = -192;
        let max = 192;
        let side = (max - min) as usize;
        let mut water = vec![None; side * side];
        let mut beds = vec![0; side * side];
        let mut slopes = vec![0.0; side * side];
        for z in min..max {
            for x in min..max {
                let column = gen.column_at(x, z);
                let index = ((z - min) as usize) * side + (x - min) as usize;
                beds[index] = column.surface_voxel_y();
                slopes[index] = column.slope;
                // Record every water column, including riverbed cells that
                // dip below sea level near coasts: they are water-filled in
                // game and must not count as dry holes.
                water[index] = column.water_top_voxel_y();
            }
        }

        let mut enclosed_holes = 0usize;
        let mut surface_spikes = 0usize;
        let mut shallow_columns = 0usize;
        let mut overdeep_columns = 0usize;
        let mut overdeep_sloped_columns = 0usize;
        let mut wet_columns = 0usize;
        let mut spike_examples = Vec::new();
        let mut wet_pairs = 0usize;
        let mut hole_examples = Vec::new();
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let index = z * side + x;
                if let Some(top) = water[index] {
                    let column = gen.column_at(x as i32 + min, z as i32 + min);
                    // Open ocean follows its own rules: the depth/curtain
                    // checks below exist for terrain-carved inland water.
                    if column.height_m < SEA_LEVEL as f64 {
                        continue;
                    }
                    wet_columns += 1;
                    let depth = top - column.surface_voxel_y();
                    shallow_columns +=
                        usize::from(column.slope <= MODERATE_WATER_SLOPE && depth < 2);
                    // Natural planar fills reach deep gorge pools (~50 m);
                    // anything past that magnitude is treated as a runaway
                    // curtain.
                    overdeep_columns += usize::from(f64::from(depth) > 64.0);
                    let slope_depth_limit = 64.0 as i32 * VOXELS_PER_METER;
                    overdeep_sloped_columns += usize::from(depth > slope_depth_limit);
                    for neighbor_index in [index + 1, index + side] {
                        let Some(other_top) = water[neighbor_index] else {
                            continue;
                        };
                        if slopes[index] > MODERATE_WATER_SLOPE
                            || slopes[neighbor_index] > MODERATE_WATER_SLOPE
                            || (beds[index] - beds[neighbor_index]).abs() > 1
                        {
                            continue;
                        }
                        // Estuaries are exempt: a planar river arriving above
                        // sea level legitimately drops into the ocean plane.
                        let sea_top = sea_top_voxel_y();
                        if top == sea_top || other_top == sea_top {
                            continue;
                        }
                        wet_pairs += 1;
                        if (top - other_top).abs() > 1 {
                            surface_spikes += 1;
                            if spike_examples.len() < 8 {
                                spike_examples.push((
                                    x as i32 + min,
                                    z as i32 + min,
                                    top,
                                    other_top,
                                ));
                            }
                        }
                    }
                } else {
                    let wet_neighbors = [
                        index - side - 1,
                        index - side,
                        index - side + 1,
                        index - 1,
                        index + 1,
                        index + side - 1,
                        index + side,
                        index + side + 1,
                    ]
                    .into_iter()
                    .filter(|neighbor| water[*neighbor].is_some())
                    .count();
                    // A real pinhole sits *under* water: below sea level, or
                    // below an adjacent water surface. Dry sand at the
                    // waterline (height == sea level, zero coverage) is just
                    // a beach, not a hole.
                    let is_hole = wet_neighbors >= 7 && {
                        let column = gen.column_at(x as i32 + min, z as i32 + min);
                        let below_sea = (column.height_m + 0.01) < SEA_LEVEL as f64;
                        let submerged = [
                            index - side - 1,
                            index - side,
                            index - side + 1,
                            index - 1,
                            index + 1,
                            index + side - 1,
                            index + side,
                            index + side + 1,
                        ]
                        .into_iter()
                        .filter_map(|neighbor| water[neighbor])
                        .any(|top| f64::from(top) > column.height_m + 1.0);
                        below_sea || submerged
                    };
                    enclosed_holes += usize::from(is_hole);
                    if is_hole && hole_examples.len() < 8 {
                        hole_examples.push((x as i32 + min, z as i32 + min));
                    }
                }
            }
        }

        assert!(
            wet_pairs > 100,
            "sample did not contain enough inland water"
        );
        assert_eq!(
            enclosed_holes, 0,
            "inland water contains dry pinholes at {hole_examples:?}"
        );
        assert_eq!(
            overdeep_columns, 0,
            "inland water contains unsupported vertical curtains"
        );
        assert_eq!(
            overdeep_sloped_columns, 0,
            "waterfall depth is not attached tightly enough to its supporting slope"
        );
        assert!(
            surface_spikes * 100 <= wet_pairs,
            "too many abrupt water steps ({surface_spikes}/{wet_pairs} pairs): {spike_examples:?}"
        );
        assert!(
            // A thin one-block water fringe hugging the banks is intentional
            // shore glue now that planes run flat into the terrain; only a
            // runaway share of ankle-deep columns would reveal contours.
            shallow_columns * 100 <= wet_columns * 5,
            "too many contour-revealing shallow columns: {shallow_columns}/{wet_columns}"
        );
    }

    #[test]
    fn channels_have_downhill_exits_and_variable_discharge() {
        let gen = TerrainGenerator::new(1337);
        let mut channels = 0usize;
        let mut downhill = 0usize;
        let mut discharges = std::collections::HashSet::new();
        for z in (-960..960).step_by(10) {
            for x in (-960..960).step_by(10) {
                let column = gen.column_at(x, z);
                if column.river_strength < 0.45 {
                    continue;
                }
                channels += 1;
                discharges.insert((column.flow_strength * 100.0).round() as i32);
                let has_downhill = [(4, 0), (-4, 0), (0, 4), (0, -4)]
                    .into_iter()
                    .any(|(dx, dz)| gen.column_at(x + dx, z + dz).height_m <= column.height_m);
                downhill += usize::from(has_downhill);
            }
        }
        assert!(channels > 20, "no channel network found");
        assert!(downhill * 5 >= channels * 4, "too many uphill river cells");
        assert!(discharges.len() > 3, "river discharge never changes");
    }

    #[test]
    fn major_landforms_have_asymmetric_shoulders() {
        let gen = TerrainGenerator::new(7);
        let mut asymmetric = 0usize;
        for z in (-1152..1152).step_by(24) {
            for x in (-1152..1152).step_by(24) {
                let center = gen.height_at(x, z);
                let drops = [
                    center - gen.height_at(x + 16, z),
                    center - gen.height_at(x - 16, z),
                    center - gen.height_at(x, z + 16),
                    center - gen.height_at(x, z - 16),
                ];
                if center > 38
                    && drops.iter().copied().max().unwrap() - drops.iter().copied().min().unwrap()
                        >= 4
                {
                    asymmetric += 1;
                }
            }
        }
        assert!(
            asymmetric > 25,
            "landforms still have overly symmetric shoulders"
        );
    }

    #[test]
    fn forests_show_multiple_density_tiers() {
        let mut saw_sparse = false;
        let mut saw_dense = false;
        for seed in SEEDS {
            let mut forest_mean = 0.0;
            let mut forest_n = 0;
            for col in sampled_columns(seed) {
                if col.height <= SEA_LEVEL + 1 || col.slope > 1.2 {
                    continue;
                }
                if matches!(
                    col.biome,
                    Biome::Forest | Biome::DenseForest | Biome::Rainforest
                ) {
                    forest_mean += col.forest_density;
                    forest_n += 1;
                }
                if col.forest_density < 0.18 {
                    saw_sparse = true;
                }
                if col.forest_density > 0.72 {
                    saw_dense = true;
                }
            }
            if forest_n > 20 {
                let mean = forest_mean / forest_n as f64;
                assert!(mean > 0.40, "seed {seed}: forests too sparse ({mean:.2})");
            }
        }
        assert!(saw_sparse, "no open/clearing areas anywhere");
        assert!(saw_dense, "no dense forest pockets anywhere");
    }

    #[test]
    fn tree_proportions_canopy_scales_with_trunk() {
        for seed in SEEDS {
            for i in 0..24u32 {
                let x = (i as i32) * 37 + 5;
                let z = (i as i32) * -53 + 11;
                for kind in [
                    TreeKind::YoungOak,
                    TreeKind::Oak,
                    TreeKind::TallOak,
                    TreeKind::WideOak,
                    TreeKind::Birch,
                    TreeKind::Giant,
                    TreeKind::JungleGiant,
                    TreeKind::PineSmall,
                    TreeKind::PineTall,
                    TreeKind::Palm,
                ] {
                    let spec = kind.spec(seed, x, z);
                    if is_dome_kind(kind) {
                        assert!(
                            spec.crown_radius * 2 >= spec.trunk_h,
                            "{kind:?}: r={} vs trunk={} (pole tree!)",
                            spec.crown_radius,
                            spec.trunk_h
                        );
                    }
                    if is_conifer(kind) {
                        assert!(
                            spec.crown_height + 2 >= spec.trunk_h,
                            "{kind:?}: cone h={} on trunk={}",
                            spec.crown_height,
                            spec.trunk_h
                        );
                    }
                }
            }
        }
    }

    fn is_dome_kind(kind: TreeKind) -> bool {
        matches!(
            kind,
            TreeKind::YoungOak
                | TreeKind::Oak
                | TreeKind::TallOak
                | TreeKind::WideOak
                | TreeKind::Giant
                | TreeKind::JungleGiant
                | TreeKind::JungleMedium
                | TreeKind::SwampWillow
                | TreeKind::AutumnOak
        )
    }

    fn is_conifer(kind: TreeKind) -> bool {
        matches!(
            kind,
            TreeKind::PineSmall | TreeKind::PineMedium | TreeKind::PineTall | TreeKind::PineSnowy
        )
    }

    #[test]
    fn accepted_trees_keep_minimum_gaps() {
        let gen = TerrainGenerator::new(42);
        let mut cache = ColumnCache::default();
        let mut trunks: Vec<(VoxelCoord, TreeKind, i32, i32)> = Vec::new();
        for sx in -30..30 {
            for sz in -30..30 {
                if let Some((pos, kind, _)) = gen.tree_slot_candidate(&mut cache, sx, sz) {
                    trunks.push((pos, kind, sx, sz));
                }
            }
        }

        for a in 0..trunks.len() {
            for b in (a + 1)..trunks.len() {
                let (pa, ka, asx, asz) = &trunks[a];
                let (pb, kb, bsx, bsz) = &trunks[b];
                let dist = (((pa.x - pb.x).pow(2) + (pa.z - pb.z).pow(2)) as f64).sqrt();
                let required = required_gap(*ka, *kb);
                assert!(
                    dist >= required,
                    "trees {ka:?} at {pa:?} slot ({asx},{asz}) and {kb:?} at {pb:?} slot ({bsx},{bsz}) are {dist} apart (< {required})"
                );
            }
        }
        assert!(trunks.len() > 20, "sample region produced almost no trees");
    }

    /// Minimum gaps by pair class. Giants deliberately keep some smaller
    /// vegetation around them (emergent look) - smalls may sit at the crown
    /// fringe, just not inside the trunk zone.
    fn required_gap(a: TreeKind, b: TreeKind) -> f64 {
        use TreeKind::*;
        let big = |k: TreeKind| matches!(k, Giant | JungleGiant);
        if big(a) && big(b) {
            15.0
        } else if big(a) || big(b) {
            5.0
        } else if a.exclusion_slots() > 0 && b.exclusion_slots() > 0 {
            8.0
        } else {
            3.0
        }
    }

    #[test]
    fn rocks_are_contextual_not_sprinkled() {
        let gen = TerrainGenerator::new(1337);
        let mut cache = ColumnCache::default();
        let empty_trees: Vec<(VoxelCoord, TreeKind)> = Vec::new();

        let mut rocky_cells: f64 = 0.0;
        let mut rocky_hits: f64 = 0.0;
        let mut plain_cells: f64 = 0.0;
        let mut plain_hits: f64 = 0.0;

        for x in (-300i32..300).step_by(4) {
            for z in (-300i32..300).step_by(4) {
                // Evaluate the slot's *actual* jittered position, not the
                // grid point - otherwise classification is meaningless.
                let gx = x.div_euclid(DECOR);
                let gz = z.div_euclid(DECOR);
                let (px, pz) = (
                    gx * DECOR + (trees::hash(gen.seed, gx, gz, 71) % DECOR as u32) as i32,
                    gz * DECOR + (trees::hash(gen.seed, gx, gz, 73) % DECOR as u32) as i32,
                );
                let col = cache.column(&gen, px, pz);
                if col.height <= SEA_LEVEL + 1 || col.river_strength > 0.45 {
                    continue;
                }
                let hit = gen
                    .ground_slot_candidate(&mut cache, gx, gz, &empty_trees)
                    .map(|(_, k, _)| {
                        matches!(
                            k,
                            GroundKind::Pebble
                                | GroundKind::Rock
                                | GroundKind::Boulder
                                | GroundKind::MegaBoulder
                        )
                    })
                    .unwrap_or(false);

                if col.slope > 1.05 || gen.mountain_field(px, pz, &col.climate) > 0.45 {
                    rocky_cells += 1.0;
                    rocky_hits += hit as i32 as f64;
                } else if col.slope < 0.35
                    && col.forest_density < 0.3
                    // Foothill skirts, river shoulders, rocky patches and
                    // rocky coasts are *meant* to carry rocks - exclude all
                    // of them from the "open plains" control bucket.
                    && gen.mountain_field(px, pz, &col.climate) < 0.15
                    && gen.river_valley_at(x, z) < 0.20
                    && gen.noise.rocky_patch_field(x, z) < 0.45
                    && gen.coast_rock_factor(x, z) < 0.02
                {
                    plain_cells += 1.0;
                    plain_hits += hit as i32 as f64;
                }
            }
        }

        let rocky_rate = rocky_hits / rocky_cells.max(1.0);
        let plain_rate = plain_hits / plain_cells.max(1.0);
        assert!(
            rocky_rate > plain_rate * 2.0 + 0.005,
            "rocks not contextual: rocky {rocky_rate:.3} vs plains {plain_rate:.3}"
        );
    }
}
