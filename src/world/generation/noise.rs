use noise::{NoiseFn, OpenSimplex};

/// Pre-computed climate and terrain data for a single (x, z) column,
/// sampled once and reused across all y-levels to avoid redundant noise calls.
#[derive(Debug, Clone, Copy)]
pub struct ColumnClimate {
    /// Continental-scale noise (ocean vs. shelf vs. inland vs. highlands). Range ~[-1, 1].
    pub continentalness: f64,
    /// Elevation noise (medium-scale hills, plateaus, valleys). Range ~[-1, 1].
    pub elevation: f64,
    /// Ridged mountain spine noise for continuous mountain ranges. Range ~[0, 1].
    pub ridges: f64,
    /// Secondary peak/weirdness noise for variation. Range ~[-1, 1].
    pub peaks: f64,
    /// Temperature climate map (-1.0 freezing to +1.0 scorching). Range ~[-1, 1].
    pub temperature: f64,
    /// Humidity / rainfall climate map (-1.0 arid to +1.0 lush rainforest). Range ~[-1, 1].
    pub humidity: f64,
    /// Erosion noise (high = flat valleys/plains/swamps, low = rugged peaks/gorges). Range ~[-1, 1].
    pub erosion: f64,
    /// Fine-scale detail noise for organic micro-relief. Range ~[-1, 1].
    pub detail: f64,
    /// Desert dune ripple noise. Range ~[-1, 1].
    pub dunes: f64,
    /// Large coherent landform zoning field. Range ~[-1, 1].
    pub landform: f64,
    /// Broad plateau/shelf field. Range ~[-1, 1].
    pub plateau: f64,
    /// Coherent geological province field. Range ~[-1, 1].
    pub geology: f64,
    /// Sparse landmark province used for isolated peaks and basins.
    pub landmark: f64,
    /// Broad directional break used for escarpments and passes.
    pub escarpment: f64,
}

pub struct WorldNoise {
    seed: u64,
    continental: OpenSimplex,
    elevation: OpenSimplex,
    mountains: OpenSimplex,
    detail: OpenSimplex,
    cave_large: OpenSimplex,
    cave_small: OpenSimplex,
    temperature: OpenSimplex,
    humidity: OpenSimplex,
    erosion: OpenSimplex,
    ridges: OpenSimplex,
    peaks: OpenSimplex,
    warp_x: OpenSimplex,
    warp_z: OpenSimplex,
    veg_macro: OpenSimplex,
    veg_local: OpenSimplex,
    clearing_macro: OpenSimplex,
    clearing_small: OpenSimplex,
    foliage_blend: OpenSimplex,
    grove: OpenSimplex,
    rocky_patch: OpenSimplex,
    bush_patch: OpenSimplex,
    coast_rock: OpenSimplex,
    landform: OpenSimplex,
    plateau: OpenSimplex,
    geology: OpenSimplex,
    landmark: OpenSimplex,
    escarpment: OpenSimplex,
    seabed_broad: OpenSimplex,
    seabed_medium: OpenSimplex,
    chain_axis: OpenSimplex,
    chain_meander: OpenSimplex,
    chain_crest: OpenSimplex,
    chain_crest2: OpenSimplex,
    chain_crest3: OpenSimplex,
    chain_band: OpenSimplex,
    chain_hills: OpenSimplex,
}

impl WorldNoise {
    pub fn new(seed: u64) -> Self {
        let s = seed as u32;
        Self {
            seed,
            continental: OpenSimplex::new(s),
            elevation: OpenSimplex::new(s.wrapping_add(11)),
            mountains: OpenSimplex::new(s.wrapping_add(29)),
            detail: OpenSimplex::new(s.wrapping_add(47)),
            cave_large: OpenSimplex::new(s.wrapping_add(83)),
            cave_small: OpenSimplex::new(s.wrapping_add(97)),
            temperature: OpenSimplex::new(s.wrapping_add(109)),
            humidity: OpenSimplex::new(s.wrapping_add(127)),
            erosion: OpenSimplex::new(s.wrapping_add(137)),
            ridges: OpenSimplex::new(s.wrapping_add(149)),
            peaks: OpenSimplex::new(s.wrapping_add(163)),
            warp_x: OpenSimplex::new(s.wrapping_add(179)),
            warp_z: OpenSimplex::new(s.wrapping_add(191)),
            veg_macro: OpenSimplex::new(s.wrapping_add(251)),
            veg_local: OpenSimplex::new(s.wrapping_add(269)),
            clearing_macro: OpenSimplex::new(s.wrapping_add(281)),
            clearing_small: OpenSimplex::new(s.wrapping_add(293)),
            foliage_blend: OpenSimplex::new(s.wrapping_add(307)),
            grove: OpenSimplex::new(s.wrapping_add(331)),
            rocky_patch: OpenSimplex::new(s.wrapping_add(347)),
            bush_patch: OpenSimplex::new(s.wrapping_add(359)),
            coast_rock: OpenSimplex::new(s.wrapping_add(373)),
            landform: OpenSimplex::new(s.wrapping_add(389)),
            plateau: OpenSimplex::new(s.wrapping_add(401)),
            geology: OpenSimplex::new(s.wrapping_add(419)),
            landmark: OpenSimplex::new(s.wrapping_add(433)),
            escarpment: OpenSimplex::new(s.wrapping_add(449)),
            seabed_broad: OpenSimplex::new(s.wrapping_add(467)),
            seabed_medium: OpenSimplex::new(s.wrapping_add(487)),
            chain_axis: OpenSimplex::new(s.wrapping_add(503)),
            chain_meander: OpenSimplex::new(s.wrapping_add(521)),
            chain_crest: OpenSimplex::new(s.wrapping_add(547)),
            chain_crest2: OpenSimplex::new(s.wrapping_add(569)),
            chain_crest3: OpenSimplex::new(s.wrapping_add(631)),
            chain_band: OpenSimplex::new(s.wrapping_add(587)),
            chain_hills: OpenSimplex::new(s.wrapping_add(607)),
        }
    }

    /// Deterministic hash for cellular-noise feature points (splitmix-style).
    fn massif_hash(&self, cell_x: i64, cell_z: i64) -> u64 {
        let mut h = (cell_x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (cell_z as u64)
                .rotate_left(32)
                .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ self.seed.wrapping_mul(0x1000_0000_0193);
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^ (h >> 31)
    }

    /// Cellular F1 distance in cell units for the massif domes, plus a
    /// stable per-massif random value in [0, 1), so every dome in a range
    /// carries its own height character (sky-piercing summits next to
    /// modest shoulders) instead of all massifs sharing one silhouette.
    /// Hand-rolled because the `noise` crate's `Worley` holds an `Rc` and
    /// is neither `Send` nor `Sync`, which `WorldNoise` (used from parallel
    /// systems) requires. Same 550 m cell scale and F1 semantics as before.
    fn massif_cell(&self, x: f64, z: f64) -> (f64, f64) {
        const CELL: f64 = 550.0;
        let cell_x = (x / CELL).floor() as i64;
        let cell_z = (z / CELL).floor() as i64;
        let mut best = f64::INFINITY;
        let mut best_hash = 0u64;
        for dz in -1..=1i64 {
            for dx in -1..=1i64 {
                let (nx, nz) = (cell_x + dx, cell_z + dz);
                let h = self.massif_hash(nx, nz);
                let px = (nx as f64 + (h & 0xFFFF) as f64 / 65536.0) * CELL;
                let pz = (nz as f64 + ((h >> 16) & 0xFFFF) as f64 / 65536.0) * CELL;
                let dist = ((px - x) * (px - x) + (pz - z) * (pz - z)).sqrt();
                if dist < best {
                    best = dist;
                    best_hash = h;
                }
            }
        }
        ((best / CELL), (best_hash >> 32) as f64 / u32::MAX as f64)
    }

    /// Anisotropic mountain-range structure. One orientation per ~2000 m
    /// province, a meandered ~1 km corridor, a three-octave ridged crest
    /// sharpened to a blade along the spine, Worley massif domes whose
    /// individual heights vary dome-to-dome with deep saddle passes between
    /// them, a foothill apron and a flanking moat valley. Deterministic in
    /// world coordinates only.
    pub fn mountain_chain(
        &self,
        x: i32,
        z: i32,
        continentalness: f64,
        erosion: f64,
    ) -> MountainChain {
        // Ranges rise across most coherent inland provinces; only strongly
        // eroded flatlands suppress them entirely.
        let province = smoothstep_range(-0.12, 0.28, continentalness)
            * (1.0 - smoothstep_range(0.18, 0.72, erosion));
        if province <= 0.0 {
            return MountainChain {
                mask: 0.0,
                height: 0.0,
            };
        }

        let xf = x as f64;
        let zf = z as f64;
        let axis = self.chain_axis.get([xf / 2000.0, zf / 2000.0]) * std::f64::consts::FRAC_PI_2;
        let (cos_a, sin_a) = (axis.cos(), axis.sin());
        let u = xf * cos_a + zf * sin_a;
        let v = -xf * sin_a + zf * cos_a;
        let meander = self.chain_meander.get([u / 420.0, v / 420.0]) * 200.0;
        let v = v + meander;

        let across = v / 520.0;
        let corridor = (-across * across).exp();

        // Ridged crest along the spine: three octaves from the range-scale
        // ridgeline down to crag, sharpened so ridgelines stay narrow while
        // flanks fall away smoothly.
        let c1 = 1.0 - self.chain_crest.get([xf / 420.0, zf / 420.0]).abs();
        let c2 = 1.0
            - self
                .chain_crest2
                .get([xf / 170.0 + 31.7, zf / 170.0 + 11.3])
                .abs();
        let c3 = 1.0
            - self
                .chain_crest3
                .get([xf / 70.0 + 7.7, zf / 70.0 + 91.3])
                .abs();
        let crest = (c1 * 0.50 + c2 * 0.30 + c3 * 0.20)
            .clamp(0.0, 1.0)
            .powf(2.4);

        // Massif domes along the corridor (cellular F1 falloff). Each dome
        // rolls its own height budget so a range reads as a train of distinct
        // peaks and shoulders; saddle passes open where a slow field crosses
        // zero.
        let (f1, massif_roll) = self.massif_cell(xf, zf);
        let massif = smoothstep_range(0.85, 0.25, f1);
        let peak_amp = 0.55 + 0.65 * massif_roll;
        let pass = 1.0
            - smoothstep_range(
                0.0,
                0.30,
                self.chain_hills.get([xf / 550.0, zf / 550.0]).abs(),
            );

        // Some sections of a range are higher than others.
        let band = self.chain_band.get([u / 900.0, v / 900.0]);
        let ridge_top = 90.0 + 40.0 * band;
        let tiers = 1.0
            + 0.35 * smoothstep_range(0.45, 0.72, crest * massif)
            + 0.25 * smoothstep_range(0.72, 0.90, crest * massif);
        let spine = massif * peak_amp * tiers * crest.powf(0.8) * ridge_top * (0.35 + 0.65 * pass);

        // Broad foot bulge inside the corridor, a rolling foothill apron
        // outside it, and a shallow moat valley beyond the apron.
        let base_bulge = 32.0 * corridor;
        let apron_dist = (v.abs() - 600.0) / 340.0;
        let apron = (-apron_dist * apron_dist).exp()
            * 22.0
            * (0.5 + 0.5 * (self.chain_hills.get([xf / 170.0, zf / 170.0]) * 0.5 + 0.5));
        let moat = smoothstep_range(0.95, 1.7, v.abs() / 450.0) * 16.0;

        let height = province * (corridor * spine + base_bulge + apron - moat);
        let mask = (province * corridor * massif).clamp(0.0, 1.0);
        MountainChain { mask, height }
    }

    /// Continentalness field shared by `climate_at` and the cheap
    /// `mountain_mask_hint`, so the two can never drift apart.
    fn continentalness_at(&self, warped_x: f64, warped_z: f64) -> f64 {
        let (coast_x, coast_z) = (
            warped_x + self.warp_x.get([warped_x / 90.0 + 3.7, warped_z / 90.0]) * 25.0,
            warped_z + self.warp_z.get([warped_x / 90.0, warped_z / 90.0 + 9.2]) * 25.0,
        );
        let c_macro = self.continental.get([coast_x / 3200.0, coast_z / 3200.0]);
        let c_medium = self
            .continental
            .get([coast_x / 1300.0 + 52.3, coast_z / 1300.0 + 81.7])
            * 0.25;
        // Small landward bias: with kilometre-scale continents any given
        // window is otherwise likelier to sit in open water, which starves
        // the spawn region of usable land.
        ((c_macro * 0.8 + c_medium) + 0.06).clamp(-1.0, 1.0)
    }

    /// Shared macro domain warp in metres. Landforms, climate and geology all
    /// use these coordinates so their borders bend together without shrinking
    /// feature scale when terrain resolution changes.
    pub fn terrain_warp_at(&self, x: f64, z: f64) -> (f64, f64) {
        let warp_scale = 320.0;
        let wx = self.warp_x.get([x / warp_scale, z / warp_scale]) * 55.0;
        let wz = self
            .warp_z
            .get([x / warp_scale + 17.1, z / warp_scale + 31.9])
            * 55.0;
        (x + wx, z + wz)
    }

    /// Samples all 2D climate and terrain layers at a world (x, z) coordinate.
    /// Uses domain warping for fluid, organic land boundaries and natural biome shapes.
    pub fn climate_at(&self, x: i32, z: i32) -> ColumnClimate {
        let xf = x as f64;
        let zf = z as f64;

        // Macro-scale domain warp (creates winding coastlines and natural biome curves)
        let (warped_x, warped_z) = self.terrain_warp_at(xf, zf);

        // Continentalness: large scale landmass distribution (continents,
        // oceans, shelves). The primary octave sets the continent scale
        // (multi-kilometre landmasses); the medium octave adds inland seas
        // and bays. A dedicated coast warp keeps shorelines winding at the
        // kilometre scale without disturbing landform/geology alignment.
        let continentalness = self.continentalness_at(warped_x, warped_z);

        // Temperature: smooth thermal bands with local variation and continent influence
        let temp_base = self.temperature.get([warped_x / 460.0, warped_z / 460.0]);
        let temp_detail = self
            .temperature
            .get([warped_x / 140.0 + 19.4, warped_z / 140.0 + 44.1])
            * 0.18;
        let temperature = (temp_base + temp_detail).clamp(-1.0, 1.0);

        // Humidity: moisture distribution with natural gradients
        let hum_base = self.humidity.get([warped_x / 420.0, warped_z / 420.0]);
        let hum_detail = self
            .humidity
            .get([warped_x / 120.0 + 63.8, warped_z / 120.0 + 91.2])
            * 0.20;
        let humidity = (hum_base + hum_detail).clamp(-1.0, 1.0);

        // Erosion: determines terrain sharpness, valleys vs plains
        let erosion = self.erosion.get([warped_x / 260.0, warped_z / 260.0]);

        // Mountain Ridges: ridged multifractal for long sharp mountain crests
        let r1 = self.ridges.get([warped_x / 220.0, warped_z / 220.0]);
        let r2 = self
            .mountains
            .get([warped_x / 110.0 + 33.1, warped_z / 110.0 + 77.4])
            * 0.5;
        let raw_ridge = (1.0 - (r1.abs() * 0.7 + r2.abs() * 0.3) * 2.0).clamp(-1.0, 1.0);
        let ridges = (raw_ridge * 0.5 + 0.5).clamp(0.0, 1.0);

        // Elevation / regional rolling hills
        let e1 = self.elevation.get([warped_x / 110.0, warped_z / 110.0]);
        let e2 = self
            .elevation
            .get([warped_x / 48.0 + 12.5, warped_z / 48.0 + 88.2])
            * 0.35;
        let elevation = e1 + e2;

        // Secondary peaks & weirdness
        let peaks = self.peaks.get([warped_x / 160.0, warped_z / 160.0]);

        // Local fine detail
        let detail = self.detail.get([xf / 22.0, zf / 22.0]);

        // Desert dunes: directional ripple wave pattern
        let dune_angle: f64 = 0.65; // ~37 degrees wind direction
        let u = xf * dune_angle.cos() - zf * dune_angle.sin();
        let v = xf * dune_angle.sin() + zf * dune_angle.cos();
        let dune_warp = self.detail.get([u / 45.0, v / 45.0]) * 6.0;
        let dune_wave = (((u + dune_warp) / 16.0).sin() * 0.5 + 0.5).powf(1.6);
        let dunes = dune_wave * 2.0 - 1.0;

        // Very broad landform and geology provinces. These are intentionally
        // lower frequency than elevation noise so regions remain readable.
        let landform = self
            .landform
            .get([warped_x / 310.0 + 7.3, warped_z / 310.0 + 19.7]);
        let plateau = self
            .plateau
            .get([warped_x / 230.0 + 43.1, warped_z / 230.0 + 11.9]);
        let geology_macro = self
            .geology
            .get([warped_x / 280.0 + 23.4, warped_z / 280.0 + 67.2]);
        let geology_detail = self
            .geology
            .get([warped_x / 95.0 + 81.7, warped_z / 95.0 + 31.3])
            * 0.22;
        let geology = (geology_macro + geology_detail).clamp(-1.0, 1.0);
        let landmark = self
            .landmark
            .get([warped_x / 190.0 + 14.7, warped_z / 190.0 + 73.2]);
        let escarpment = self
            .escarpment
            .get([warped_x / 340.0 + 91.3, warped_z / 340.0 + 27.4]);

        ColumnClimate {
            continentalness,
            temperature,
            humidity,
            erosion,
            ridges,
            peaks,
            elevation,
            detail,
            dunes,
            landform,
            plateau,
            geology,
            landmark,
            escarpment,
        }
    }

    /// Macro vegetation density (0.0 to 1.0). Controls large-scale forest density vs sparse woodland.
    pub fn macro_vegetation_density(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.veg_macro.get([xf / 190.0, zf / 190.0]);
        (n * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    /// Local vegetation density (0.0 to 1.0). Controls micro-clustering into groves and natural gaps.
    pub fn local_vegetation_density(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n1 = self.veg_local.get([xf / 48.0, zf / 48.0]);
        let n2 = self.veg_local.get([xf / 20.0 + 37.1, zf / 20.0 + 81.3]) * 0.3;
        ((n1 + n2) * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    /// Multi-scale clearing factor (0.0 to 1.0). High values indicate clearings, gaps or open meadows.
    pub fn clearing_field(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        // Large meadow scale
        let m = self.clearing_macro.get([xf / 115.0, zf / 115.0]);
        // Medium clearing scale
        let c = self
            .clearing_small
            .get([xf / 42.0 + 13.7, zf / 42.0 + 53.2]);
        // Small opening gaps
        let g = self.detail.get([xf / 18.0 + 77.1, zf / 18.0 + 29.4]) * 0.25;

        let combined = m * 0.55 + c * 0.35 + g;
        (combined * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    /// Local foliage blending noise (-1.0 to 1.0) for species mixtures and autumn leaf tones.
    pub fn foliage_blend(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        self.foliage_blend.get([xf / 36.0, zf / 36.0])
    }

    /// Combined 3D cave density for winding tunnels and deep caverns.
    /// The old vertical "fissure" term (which sliced narrow artificial
    /// trenches through the landscape) has been removed on purpose.
    pub fn cave_density(&self, x: i32, y: i32, z: i32) -> f64 {
        let xf = x as f64;
        let yf = y as f64;
        let zf = z as f64;

        // Domain-warped 3D tunnel coordinates
        let tw_x = self.warp_x.get([xf / 40.0, yf / 40.0, zf / 40.0]) * 8.0;
        let tw_z = self
            .warp_z
            .get([xf / 40.0 + 11.2, yf / 40.0, zf / 40.0 + 29.5])
            * 8.0;
        let tx = (xf + tw_x) / 28.0;
        let ty = yf / 18.0;
        let tz = (zf + tw_z) / 28.0;

        // Winding tubular tunnels.
        let tunnel_a = self.cave_large.get([tx, ty, tz]);
        let tunnel_b = self.cave_small.get([tx + 31.7, ty + 18.3, tz + 73.1]);
        let tunnel = 1.0 - (tunnel_a * tunnel_a + tunnel_b * tunnel_b).sqrt() * 2.5;

        // Large caverns in deep stone only (never near the surface).
        let cavern = self.cave_large.get([xf / 26.0, yf / 20.0, zf / 26.0]);

        tunnel.max(cavern * 0.55)
    }

    /// Sharp small grove blobs (~15-40 block radius). Drives groves and
    /// forest clusters on top of the macro density field.
    pub fn grove_field(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.grove.get([xf / 26.0, zf / 26.0]);
        let detail = self.grove.get([xf / 11.0 + 7.7, zf / 11.0 + 3.9]) * 0.30;
        smoothstep01((n * 0.85 + detail) * 0.5 + 0.5)
    }

    /// Dedicated rocky patches (~60-120 blocks across).
    pub fn rocky_patch_field(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.rocky_patch.get([xf / 78.0, zf / 78.0]);
        let n2 = self.rocky_patch.get([xf / 31.0 + 17.3, zf / 31.0 + 41.1]) * 0.35;
        smoothstep01((n * 0.75 + n2) * 0.5 + 0.5)
    }

    /// Bush/flower patch zones.
    pub fn bush_patch_field(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.bush_patch.get([xf / 34.0, zf / 34.0]);
        smoothstep01(n * 0.5 + 0.5)
    }

    /// Coherent meadow-plant clump mask (0..1). Ground vegetation concentrates
    /// where this field is high instead of sprinkling uniformly.
    pub fn meadow_patch_field(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.bush_patch.get([xf / 52.0 + 71.3, zf / 52.0 + 13.9]);
        let detail = self.bush_patch.get([xf / 17.0 + 5.1, zf / 17.0 + 91.7]) * 0.30;
        smoothstep01((n * 0.85 + detail) * 0.5 + 0.5)
    }

    /// Ridged spur/gully field for mountain flanks. ~[0, 1] ridges with
    /// secondary variation; the mid frequency exists to break
    /// contour-parallel stair lines into merging/splitting risers. Kept at
    /// wavelengths the 4 m regional grid can represent without aliasing.
    pub fn mountain_spur_field(&self, x: f64, z: f64) -> f64 {
        let ridges = 1.0 - (self.ridges.get([x / 46.0, z / 46.0]).abs() * 1.7).min(1.0);
        let mid = self.mountains.get([x / 13.0, z / 13.0]) * 0.42;
        ridges + mid
    }

    /// Smooth medium-scale jitter (~12 m wavelength) for mountain voxel
    /// steps: shifts whole ledge sections so stair lines stay irregular.
    pub fn mountain_ledge_jitter(&self, x: i32, z: i32) -> f64 {
        self.mountains
            .get([x as f64 / 12.0 + 3.7, z as f64 / 12.0 + 91.2])
            * 1.6
    }

    /// Broad swells for the deep ocean floor in [-1, 1]: a ~110 m landform
    /// wave plus a ~34 m secondary, so abyssal seabeds read as drowned
    /// terrain with basins and ridges instead of a poured bowl.
    pub fn seabed_relief(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let broad = self.seabed_broad.get([xf / 110.0, zf / 110.0]);
        let medium = self.seabed_medium.get([xf / 34.0 + 41.7, zf / 34.0 + 13.9]) * 0.38;
        (broad + medium).clamp(-1.0, 1.0)
    }

    /// Clustered rock-exposure patches inside the snow zone (0..1). Used
    /// instead of per-column hash so exposed stone forms blobs, not speckle.
    pub fn snow_rock_patch(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.rocky_patch.get([xf / 9.0 + 41.3, zf / 9.0 + 7.7]);
        smoothstep01(n * 0.5 + 0.5)
    }

    /// Standalone mountain-mask estimate matching the mask used by the
    /// height pipeline (`mountain_chain`), for callers that only need the
    /// mask and cannot afford a full climate sample.
    pub fn mountain_mask_hint(&self, x: i32, z: i32) -> f64 {
        let (warped_x, warped_z) = self.terrain_warp_at(x as f64, z as f64);
        let continentalness = self.continentalness_at(warped_x, warped_z);
        let erosion = self.erosion.get([warped_x / 260.0, warped_z / 260.0]);
        self.mountain_chain(x, z, continentalness, erosion).mask
    }

    /// Rocky coastline variation.
    pub fn coast_rock(&self, x: i32, z: i32) -> f64 {
        let xf = x as f64;
        let zf = z as f64;
        let n = self.coast_rock.get([xf / 46.0, zf / 46.0]);
        smoothstep01(n * 0.5 + 0.5)
    }
}

/// Anisotropic mountain-range sample: `mask` drives biome/decor/snow gates,
/// `height` is the metre contribution to the landform stack.
#[derive(Debug, Clone, Copy)]
pub struct MountainChain {
    pub mask: f64,
    pub height: f64,
}

/// Cheap clamp+shape used by the fields above.
fn smoothstep01(v: f64) -> f64 {
    let t = v.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep_range(edge0: f64, edge1: f64, value: f64) -> f64 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_warp_is_deterministic_bounded_and_handles_negative_coordinates() {
        let noise = WorldNoise::new(42);
        for (x, z) in [
            (0.0, 0.0),
            (125.5, -481.25),
            (-0.25, -0.25),
            (-900.0, 731.0),
        ] {
            let first = noise.terrain_warp_at(x, z);
            let second = noise.terrain_warp_at(x, z);
            assert_eq!(first, second);
            assert!((first.0 - x).abs() <= 55.0);
            assert!((first.1 - z).abs() <= 55.0);
        }
    }

    #[test]
    fn macro_fields_change_smoothly_without_becoming_voxel_frequency_noise() {
        let noise = WorldNoise::new(1337);
        for (x, z) in [(-700, -300), (-1, -1), (0, 0), (413, 829)] {
            let a = noise.climate_at(x, z);
            let b = noise.climate_at(x + 1, z);
            assert!((a.landform - b.landform).abs() < 0.08);
            assert!((a.plateau - b.plateau).abs() < 0.08);
            assert!((a.geology - b.geology).abs() < 0.10);
        }
    }
}
