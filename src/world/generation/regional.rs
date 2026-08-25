//! Cached macro terrain simulation: thermal erosion, downhill drainage,
//! accumulated flow, lakes, asymmetric banks and floodplain deposition.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::SEA_LEVEL_METRES;

const GRID_M: i32 = 4;
const CORE_CELLS: i32 = 128;
const CORE_M: i32 = GRID_M * CORE_CELLS;
const PADDING: i32 = 24;
const SEAM_BLEND_M: i32 = 48;
const SIDE: i32 = CORE_CELLS + PADDING * 2 + 1;
const CACHE_LIMIT: usize = 32;
/// Thermal erosion tuning: full talus relaxation stairs every smooth flank
/// into concentric benches (the "ziggurat" artifact). Two gentle passes only
/// knock off genuine spikes and keep medium-scale ruggedness intact.
const THERMAL_PASSES: usize = 2;
const THERMAL_TALUS: f32 = 2.75;
const SEA_LEVEL_M: f32 = SEA_LEVEL_METRES as f32;
const RIVER_WATER_CORE: f32 = 0.56;
const LAKE_WATER_CORE: f32 = 0.42;
/// Every cell the hydrology pass carves advertises at least this much
/// coverage. Halo edges otherwise bilinear-fade below the metre-scale wet
/// gate while their beds stay dug out, punching dry pinholes into
/// lakeshores and riverbanks.
const CARVE_COVERAGE_FLOOR: f32 = 0.16;

#[derive(Debug, Clone, Copy, Default)]
pub struct RegionalSample {
    pub height_m: f64,
    /// Local height gradient magnitude in metres per metre, estimated from
    /// the same grid taps as the height bilerp (free).
    pub gradient: f64,
    pub river: f64,
    pub valley: f64,
    pub lake: f64,
    pub flow: f64,
    pub deposition: f64,
    pub moisture: f64,
    pub water_coverage: f64,
    pub water_surface_m: Option<f64>,
    pub water_depth_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RegionCoord {
    x: i32,
    z: i32,
}

struct MacroRegion {
    height: Vec<f32>,
    river: Vec<f32>,
    valley: Vec<f32>,
    lake: Vec<f32>,
    flow: Vec<f32>,
    deposition: Vec<f32>,
    moisture: Vec<f32>,
    water_coverage: Vec<f32>,
    water_surface_weighted: Vec<f32>,
    water_depth: Vec<f32>,
}

#[derive(Default)]
pub struct RegionalTerrain {
    regions: Mutex<HashMap<RegionCoord, Arc<MacroRegion>>>,
}

impl RegionalTerrain {
    pub fn sample<B, R, S>(
        &self,
        x: i32,
        z: i32,
        base_height: B,
        rainfall: R,
        spur: S,
    ) -> RegionalSample
    where
        B: Fn(i32, i32) -> f64,
        R: Fn(i32, i32) -> f64,
        S: Fn(i32, i32) -> f32,
    {
        let coord = RegionCoord {
            x: x.div_euclid(CORE_M),
            z: z.div_euclid(CORE_M),
        };
        let local_x_m = x.rem_euclid(CORE_M);
        let local_z_m = z.rem_euclid(CORE_M);
        let sample_row = |region_z: i32| {
            let center_coord = RegionCoord {
                x: coord.x,
                z: region_z,
            };
            let center = self.sample_region(center_coord, x, z, &base_height, &rainfall, &spur);
            if local_x_m < SEAM_BLEND_M {
                let neighbor = self.sample_region(
                    RegionCoord {
                        x: coord.x - 1,
                        z: region_z,
                    },
                    x,
                    z,
                    &base_height,
                    &rainfall,
                    &spur,
                );
                let t = 0.5 + 0.5 * local_x_m as f64 / SEAM_BLEND_M as f64;
                RegionalSample::blend(neighbor, center, t)
            } else if local_x_m >= CORE_M - SEAM_BLEND_M {
                let neighbor = self.sample_region(
                    RegionCoord {
                        x: coord.x + 1,
                        z: region_z,
                    },
                    x,
                    z,
                    &base_height,
                    &rainfall,
                    &spur,
                );
                let t = 0.5 * (local_x_m - (CORE_M - SEAM_BLEND_M)) as f64 / SEAM_BLEND_M as f64;
                RegionalSample::blend(center, neighbor, t)
            } else {
                center
            }
        };

        let center = sample_row(coord.z);
        if local_z_m < SEAM_BLEND_M {
            let neighbor = sample_row(coord.z - 1);
            let t = 0.5 + 0.5 * local_z_m as f64 / SEAM_BLEND_M as f64;
            RegionalSample::blend(neighbor, center, t)
        } else if local_z_m >= CORE_M - SEAM_BLEND_M {
            let neighbor = sample_row(coord.z + 1);
            let t = 0.5 * (local_z_m - (CORE_M - SEAM_BLEND_M)) as f64 / SEAM_BLEND_M as f64;
            RegionalSample::blend(center, neighbor, t)
        } else {
            center
        }
    }

    fn sample_region<B, R, S>(
        &self,
        coord: RegionCoord,
        x: i32,
        z: i32,
        base_height: &B,
        rainfall: &R,
        spur: &S,
    ) -> RegionalSample
    where
        B: Fn(i32, i32) -> f64,
        R: Fn(i32, i32) -> f64,
        S: Fn(i32, i32) -> f32,
    {
        let cached = self
            .regions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&coord)
            .cloned();
        let region = cached.unwrap_or_else(|| {
            let generated = Arc::new(MacroRegion::generate(coord, base_height, rainfall, spur));
            let mut regions = self
                .regions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if regions.len() >= CACHE_LIMIT {
                if let Some(oldest) = regions.keys().copied().find(|key| *key != coord) {
                    regions.remove(&oldest);
                }
            }
            regions.entry(coord).or_insert(generated).clone()
        });

        let local_x = (x - coord.x * CORE_M) as f32 / GRID_M as f32 + PADDING as f32;
        let local_z = (z - coord.z * CORE_M) as f32 / GRID_M as f32 + PADDING as f32;
        region.sample(local_x, local_z)
    }

    #[cfg(test)]
    fn cached_regions(&self) -> usize {
        self.regions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

impl RegionalSample {
    fn blend(a: Self, b: Self, t: f64) -> Self {
        let scalar = |left: f64, right: f64| left + (right - left) * t;
        let a_water_weight = a.water_coverage * (1.0 - t);
        let b_water_weight = b.water_coverage * t;
        let water_weight = a_water_weight + b_water_weight;
        let water_surface_m = (water_weight > 0.001).then(|| {
            let a_surface = a.water_surface_m.unwrap_or(0.0);
            let b_surface = b.water_surface_m.unwrap_or(0.0);
            (a_surface * a_water_weight + b_surface * b_water_weight) / water_weight
        });
        Self {
            height_m: scalar(a.height_m, b.height_m),
            gradient: scalar(a.gradient, b.gradient),
            river: scalar(a.river, b.river),
            valley: scalar(a.valley, b.valley),
            lake: scalar(a.lake, b.lake),
            flow: scalar(a.flow, b.flow),
            deposition: scalar(a.deposition, b.deposition),
            moisture: scalar(a.moisture, b.moisture),
            water_coverage: scalar(a.water_coverage, b.water_coverage),
            water_surface_m,
            water_depth_m: scalar(a.water_depth_m, b.water_depth_m),
        }
    }
}

impl MacroRegion {
    fn generate<B, R, S>(coord: RegionCoord, base_height: &B, rainfall: &R, spur: &S) -> Self
    where
        B: Fn(i32, i32) -> f64,
        R: Fn(i32, i32) -> f64,
        S: Fn(i32, i32) -> f32,
    {
        let side = SIDE as usize;
        let len = side * side;
        let origin_x = coord.x * CORE_M - PADDING * GRID_M;
        let origin_z = coord.z * CORE_M - PADDING * GRID_M;
        let mut height = vec![0.0; len];
        let mut moisture = vec![0.0; len];
        for gz in 0..side {
            for gx in 0..side {
                let wx = origin_x + gx as i32 * GRID_M;
                let wz = origin_z + gz as i32 * GRID_M;
                let i = index(gx, gz, side);
                height[i] = base_height(wx, wz) as f32;
                moisture[i] = rainfall(wx, wz).clamp(0.0, 1.0) as f32;
            }
        }

        thermal_erode(&mut height, side);
        directional_weathering(&mut height, side, origin_x, origin_z);

        let mut downstream = vec![None; len];
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let i = index(x, z, side);
                let mut best = i;
                let mut best_height = height[i] - 0.015;
                for (dx, dz) in NEIGHBORS {
                    let ni = index((x as i32 + dx) as usize, (z as i32 + dz) as usize, side);
                    if height[ni] < best_height {
                        best = ni;
                        best_height = height[ni];
                    }
                }
                if best != i {
                    downstream[i] = Some(best);
                }
            }
        }

        let mut accumulation: Vec<f32> = moisture.iter().map(|rain| 0.65 + rain * 1.35).collect();
        let mut order: Vec<usize> = (0..len).collect();
        order.sort_unstable_by(|a, b| height[*b].total_cmp(&height[*a]));
        for i in order {
            if let Some(next) = downstream[i] {
                accumulation[next] += accumulation[i];
            }
        }

        let mut carved = height.clone();
        let mut river = vec![0.0f32; len];
        let mut valley = vec![0.0f32; len];
        let mut lake = vec![0.0f32; len];
        let mut flow = vec![0.0f32; len];
        let mut deposition = vec![0.0f32; len];
        let mut water_coverage = vec![0.0f32; len];
        let mut water_surface_sum = vec![0.0f32; len];
        let mut water_surface_weight = vec![0.0f32; len];

        for z in 2..side - 2 {
            for x in 2..side - 2 {
                let i = index(x, z, side);
                let amount = accumulation[i];
                let Some(next) = downstream[i] else {
                    if amount >= 72.0 && height[i] > SEA_LEVEL_M + 1.0 {
                        stamp_lake(
                            x,
                            z,
                            side,
                            amount,
                            height[i],
                            &mut carved,
                            &mut lake,
                            &mut moisture,
                            &mut water_coverage,
                            &mut water_surface_sum,
                            &mut water_surface_weight,
                        );
                    }
                    continue;
                };
                if amount < 34.0 || height[i] <= SEA_LEVEL_M - 1.0 {
                    continue;
                }

                let nx = next % side;
                let nz = next / side;
                let tangent = (nx as f32 - x as f32, nz as f32 - z as f32);
                stamp_channel(
                    coord,
                    x,
                    z,
                    side,
                    tangent,
                    amount,
                    height[i],
                    &mut carved,
                    &mut river,
                    &mut valley,
                    &mut flow,
                    &mut deposition,
                    &mut moisture,
                    &mut water_coverage,
                    &mut water_surface_sum,
                    &mut water_surface_weight,
                );
            }
        }

        let mut water_surface = vec![0.0f32; len];
        for coverage in water_coverage.iter_mut() {
            if *coverage > 0.0 {
                *coverage = (*coverage).max(CARVE_COVERAGE_FLOOR);
            }
        }
        for i in 0..len {
            if water_surface_weight[i] <= f32::EPSILON || water_coverage[i] <= 0.0 {
                continue;
            }
            water_surface[i] = water_surface_sum[i] / water_surface_weight[i];
        }

        relax_water_surfaces(&mut water_surface, &water_coverage, side);

        let mut water_surface_weighted = vec![0.0f32; len];
        let mut water_depth = vec![0.0f32; len];
        for i in 0..len {
            if water_coverage[i] <= 0.0 {
                continue;
            }
            let surface = water_surface[i];
            // Every occupied core is carved beneath the shared surface. This
            // prevents dry checkerboard holes after one-metre quantization,
            // and the 2 m base keeps even halo-floored banks out of the
            // ankle-deep fringe that reveals the voxel contour grid.
            let minimum_depth = 2.05 + water_coverage[i];
            carved[i] = carved[i].min(surface - minimum_depth);
            water_depth[i] = (surface - carved[i]).max(0.0);
            water_surface_weighted[i] = surface * water_coverage[i];
        }

        // Mountain spur/gully structure, applied after hydrology so gullies
        // can never rag a carved channel wall into a dry pinhole. Land only:
        // adding spur to covered cells would lift freshly carved beds back
        // up into ankle-deep water. Any wet-adjacent bed is still re-clamped
        // below the relaxed surface afterwards.
        for gz in 0..side {
            for gx in 0..side {
                let i = index(gx, gz, side);
                if water_coverage[i] <= 0.0 {
                    let wx = origin_x + gx as i32 * GRID_M;
                    let wz = origin_z + gz as i32 * GRID_M;
                    carved[i] += spur(wx, wz);
                }
                if water_coverage[i] > 0.0 {
                    carved[i] = carved[i].min(water_surface[i] - 0.6);
                }
            }
        }

        Self {
            height: carved,
            river,
            valley,
            lake,
            flow,
            deposition,
            moisture,
            water_coverage,
            water_surface_weighted,
            water_depth,
        }
    }

    fn sample(&self, x: f32, z: f32) -> RegionalSample {
        let side = SIDE as usize;
        let water_coverage = bilerp_grid(&self.water_coverage, side, x, z);
        let water_surface_m = (water_coverage > 0.01).then(|| {
            bilerp_grid(&self.water_surface_weighted, side, x, z) as f64 / water_coverage as f64
        });
        RegionalSample {
            height_m: bilerp_grid(&self.height, side, x, z) as f64,
            gradient: height_gradient(&self.height, side, x, z) as f64,
            river: bilerp_grid(&self.river, side, x, z) as f64,
            valley: bilerp_grid(&self.valley, side, x, z) as f64,
            lake: bilerp_grid(&self.lake, side, x, z) as f64,
            flow: bilerp_grid(&self.flow, side, x, z) as f64,
            deposition: bilerp_grid(&self.deposition, side, x, z) as f64,
            moisture: bilerp_grid(&self.moisture, side, x, z) as f64,
            water_coverage: water_coverage as f64,
            water_surface_m,
            water_depth_m: bilerp_grid(&self.water_depth, side, x, z) as f64,
        }
    }
}

const NEIGHBORS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

fn thermal_erode(height: &mut [f32], side: usize) {
    let mut delta = vec![0.0f32; height.len()];
    for _ in 0..THERMAL_PASSES {
        delta.fill(0.0);
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let i = index(x, z, side);
                let mut lowest = i;
                let mut drop = 0.0f32;
                for (dx, dz) in NEIGHBORS {
                    let ni = index((x as i32 + dx) as usize, (z as i32 + dz) as usize, side);
                    let candidate = height[i] - height[ni];
                    if candidate > drop {
                        drop = candidate;
                        lowest = ni;
                    }
                }
                let talus = THERMAL_TALUS;
                if lowest != i && drop > talus {
                    let moved = (drop - talus) * 0.17;
                    delta[i] -= moved;
                    delta[lowest] += moved;
                }
            }
        }
        for (height, delta) in height.iter_mut().zip(&delta) {
            *height += delta;
        }
    }
}

fn directional_weathering(height: &mut [f32], side: usize, origin_x: i32, origin_z: i32) {
    let source = height.to_vec();
    for z in 1..side - 1 {
        for x in 1..side - 1 {
            let i = index(x, z, side);
            let world_x = origin_x + x as i32 * GRID_M;
            let world_z = origin_z + z as i32 * GRID_M;
            let angle =
                0.7 + (world_x as f32 / 900.0).sin() * 0.8 + (world_z as f32 / 1100.0).cos() * 0.6;
            let wind = (angle.cos(), angle.sin());
            let gx = (source[index(x + 1, z, side)] - source[index(x - 1, z, side)]) * 0.5;
            let gz = (source[index(x, z + 1, side)] - source[index(x, z - 1, side)]) * 0.5;
            let slope = (gx * gx + gz * gz).sqrt();
            if slope > 0.7 {
                let aspect = (gx * wind.0 + gz * wind.1) / slope.max(0.001);
                height[i] -= aspect.max(0.0) * (slope - 0.7).min(3.0) * 0.16;
            }
        }
    }
}

/// Smooths connected water stamps into coherent surfaces. A limited grade
/// still permits rivers to descend, but removes multi-block steps caused by
/// overlapping channel and lake stamps choosing independent elevations.
fn relax_water_surfaces(surface: &mut [f32], coverage: &[f32], side: usize) {
    const MAX_STEP_PER_CELL: f32 = 0.72;
    for _ in 0..12 {
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let i = index(x, z, side);
                if coverage[i] <= 0.0 {
                    continue;
                }
                for (dx, dz) in [(1i32, 0i32), (0, 1)] {
                    let neighbor = index((x as i32 + dx) as usize, (z as i32 + dz) as usize, side);
                    if coverage[neighbor] <= 0.0 {
                        continue;
                    }
                    let difference = surface[i] - surface[neighbor];
                    if difference.abs() <= MAX_STEP_PER_CELL {
                        continue;
                    }
                    let excess = (difference.abs() - MAX_STEP_PER_CELL) * 0.5;
                    let direction = difference.signum();
                    surface[i] -= excess * direction;
                    surface[neighbor] += excess * direction;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stamp_channel(
    region: RegionCoord,
    x: usize,
    z: usize,
    side: usize,
    tangent: (f32, f32),
    amount: f32,
    center_height: f32,
    carved: &mut [f32],
    river: &mut [f32],
    valley: &mut [f32],
    flow: &mut [f32],
    deposition: &mut [f32],
    moisture: &mut [f32],
    water_coverage: &mut [f32],
    water_surface_sum: &mut [f32],
    water_surface_weight: &mut [f32],
) {
    let discharge = (amount / 34.0).ln_1p();
    let width = (0.70 + discharge * 0.62).clamp(0.8, 3.2);
    let depth = (0.65 + discharge * 0.72).clamp(0.8, 4.8);
    let global_x = region.x * CORE_CELLS + x as i32 - PADDING;
    let global_z = region.z * CORE_CELLS + z as i32 - PADDING;
    let bend = ((global_x as f32 * 0.091 + global_z as f32 * 0.057).sin() * 0.68
        + (global_x as f32 * 0.037 - global_z as f32 * 0.073).sin() * 0.32)
        .clamp(-1.0, 1.0);
    let tangent_length = (tangent.0 * tangent.0 + tangent.1 * tangent.1)
        .sqrt()
        .max(0.001);
    let center_offset = (
        -tangent.1 / tangent_length * bend * 0.48,
        tangent.0 / tangent_length * bend * 0.48,
    );
    let radius = (width * 3.4).ceil() as i32;
    let plane = center_height - depth * 0.46;
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let tx = x as i32 + dx;
            let tz = z as i32 + dz;
            if tx < 1 || tz < 1 || tx >= side as i32 - 1 || tz >= side as i32 - 1 {
                continue;
            }
            let relative_x = dx as f32 - center_offset.0;
            let relative_z = dz as f32 - center_offset.1;
            let distance = (relative_x * relative_x + relative_z * relative_z).sqrt();
            let signed_side = relative_x * tangent.1 - relative_z * tangent.0;
            let side_scale = 1.0 + 0.32 * bend * signed_side.signum();
            let bank_width = (width * side_scale).max(0.55);
            let channel = 1.0 - smoothstep(bank_width * 0.34, bank_width, distance);
            let floodplain = 1.0 - smoothstep(bank_width * 1.2, width * 3.4, distance);
            if channel <= 0.0 && floodplain <= 0.0 {
                continue;
            }
            let i = index(tx as usize, tz as usize, side);
            let profile = (distance / bank_width).clamp(0.0, 1.0).powf(1.65);
            let target = center_height - depth + profile * depth * 0.78;
            carved[i] = carved[i].min(target).min(carved[i] - floodplain * 0.16);
            river[i] = river[i].max(channel);
            valley[i] = valley[i].max(floodplain);
            flow[i] = flow[i].max((amount / 260.0).clamp(0.0, 1.0) * channel);
            let inner_bank = (-signed_side.signum() * bend).max(0.0);
            deposition[i] = deposition[i].max(inner_bank * floodplain * (1.0 - channel * 0.6));
            moisture[i] = moisture[i].max(channel.max(floodplain * 0.72));
            let wet = smoothstep(RIVER_WATER_CORE - 0.14, RIVER_WATER_CORE + 0.08, channel);
            // The carve footprint is deliberately wider than the wet core;
            // carrying a faint coverage halo across the whole carved channel
            // keeps the flooded waterline glued to the banks instead of
            // leaving dry slits where bilinear heights dip below the plane.
            let halo = wet.max(floodplain * 0.30);
            if halo > 0.0 {
                water_coverage[i] = water_coverage[i].max(halo);
                water_surface_sum[i] += plane * halo;
                water_surface_weight[i] += halo;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stamp_lake(
    x: usize,
    z: usize,
    side: usize,
    amount: f32,
    sink_height: f32,
    carved: &mut [f32],
    lake: &mut [f32],
    moisture: &mut [f32],
    water_coverage: &mut [f32],
    water_surface_sum: &mut [f32],
    water_surface_weight: &mut [f32],
) {
    let radius = (1.0 + (amount / 72.0).ln_1p() * 1.5).clamp(1.4, 4.5);
    let cells = radius.ceil() as i32;
    for dz in -cells..=cells {
        for dx in -cells..=cells {
            let distance = ((dx * dx + dz * dz) as f32).sqrt();
            let strength = 1.0 - smoothstep(radius * 0.65, radius, distance);
            if strength <= 0.0 {
                continue;
            }
            let tx = x as i32 + dx;
            let tz = z as i32 + dz;
            if tx < 0 || tz < 0 || tx >= side as i32 || tz >= side as i32 {
                continue;
            }
            let i = index(tx as usize, tz as usize, side);
            carved[i] = carved[i].min(sink_height - 0.55 * strength);
            lake[i] = lake[i].max(strength);
            moisture[i] = moisture[i].max(strength);
            // Same carve-wider-than-water principle as channels: the halo
            // keeps shorelines flooded up to the true bank.
            let halo = smoothstep(LAKE_WATER_CORE - 0.12, LAKE_WATER_CORE + 0.08, strength)
                .max(strength * 0.30);
            if halo > 0.0 {
                let water_surface = sink_height + 0.10;
                water_coverage[i] = water_coverage[i].max(halo);
                water_surface_sum[i] += water_surface * halo;
                water_surface_weight[i] += halo;
            }
        }
    }
}

fn bilerp_grid(values: &[f32], side: usize, x: f32, z: f32) -> f32 {
    let (a, b, c, d, tx, tz) = grid_taps(values, side, x, z);
    let near = a + (b - a) * tx;
    let far = c + (d - c) * tx;
    near + (far - near) * tz
}

/// Magnitude of the local height gradient, from the same four taps the
/// height bilerp uses. Units: metres per metre.
fn height_gradient(values: &[f32], side: usize, x: f32, z: f32) -> f32 {
    let (a, b, c, _d, _tx, _tz) = grid_taps(values, side, x, z);
    let gx = (b - a) / GRID_M as f32;
    let gz = (c - a) / GRID_M as f32;
    (gx * gx + gz * gz).sqrt()
}

fn grid_taps(values: &[f32], side: usize, x: f32, z: f32) -> (f32, f32, f32, f32, f32, f32) {
    let x0 = x.floor().clamp(0.0, (side - 2) as f32) as usize;
    let z0 = z.floor().clamp(0.0, (side - 2) as f32) as usize;
    let tx = x - x0 as f32;
    let tz = z - z0 as f32;
    let a = values[index(x0, z0, side)];
    let b = values[index(x0 + 1, z0, side)];
    let c = values[index(x0, z0 + 1, side)];
    let d = values[index(x0 + 1, z0 + 1, side)];
    (a, b, c, d, tx, tz)
}

fn index(x: usize, z: usize, side: usize) -> usize {
    x + z * side
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(x: i32, z: i32) -> f64 {
        34.0 - x as f64 * 0.012 + (z as f64 / 45.0).sin() * 4.0
    }

    #[test]
    fn samples_are_deterministic_and_cached() {
        let terrain = RegionalTerrain::default();
        let first = terrain.sample(40, -19, base, |_, _| 0.7, |_, _| 0.0);
        let cached_after_first = terrain.cached_regions();
        let second = terrain.sample(40, -19, base, |_, _| 0.7, |_, _| 0.0);
        assert_eq!(first.height_m, second.height_m);
        assert_eq!(first.river, second.river);
        assert!((1..=4).contains(&cached_after_first));
        assert_eq!(terrain.cached_regions(), cached_after_first);
    }

    #[test]
    fn region_boundaries_do_not_crack() {
        let terrain = RegionalTerrain::default();
        for z in (-128..128).step_by(16) {
            let left = terrain.sample(CORE_M - 1, z, base, |_, _| 0.65, |_, _| 0.0);
            let right = terrain.sample(CORE_M, z, base, |_, _| 0.65, |_, _| 0.0);
            assert!(
                (left.height_m - right.height_m).abs() < 2.5,
                "regional seam at z={z}: {} vs {}",
                left.height_m,
                right.height_m
            );
        }
    }

    #[test]
    fn drainage_creates_variable_flow_and_asymmetric_deposition() {
        let terrain = RegionalTerrain::default();
        let mut wet = 0usize;
        let mut deposits = 0usize;
        let mut flow_values = std::collections::HashSet::new();
        for z in (-256..256).step_by(4) {
            for x in (-256..256).step_by(4) {
                let sample = terrain.sample(x, z, base, |_, _| 0.8, |_, _| 0.0);
                wet += usize::from(sample.river > 0.2 || sample.lake > 0.2);
                deposits += usize::from(sample.deposition > 0.2);
                if sample.flow > 0.0 {
                    flow_values.insert((sample.flow * 100.0) as i32);
                }
            }
        }
        assert!(wet > 20, "drainage produced no channels or lakes");
        assert!(deposits > 5, "river banks produced no point-bar deposition");
        assert!(
            flow_values.len() > 3,
            "all channels have the same discharge"
        );
    }
}
