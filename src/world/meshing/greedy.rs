use bevy::{
    prelude::*,
    render::{mesh::Indices, render_asset::RenderAssetUsages, render_resource::PrimitiveTopology},
};

use crate::world::{
    chunk::{Chunk, WaterShape},
    coordinates::VoxelCoord,
    voxel::{BlockType, FaceDirection},
    CHUNK_SIZE, VOXELS_PER_METER,
};

#[cfg(test)]
use crate::world::coordinates::LocalVoxelCoord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FaceKey {
    block: BlockType,
}
#[derive(Debug, Clone, Default)]
pub struct ChunkMeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub transparent_positions: Vec<[f32; 3]>,
    pub transparent_normals: Vec<[f32; 3]>,
    pub transparent_colors: Vec<[f32; 4]>,
    pub transparent_uvs: Vec<[f32; 2]>,
    pub transparent_indices: Vec<u32>,
    pub water_positions: Vec<[f32; 3]>,
    pub water_normals: Vec<[f32; 3]>,
    pub water_colors: Vec<[f32; 4]>,
    pub water_uvs: Vec<[f32; 2]>,
    pub water_indices: Vec<u32>,
    pub triangles: usize,
}

impl ChunkMeshData {
    pub fn into_meshes(self) -> (Option<Mesh>, Option<Mesh>, Option<Mesh>, usize) {
        let opaque = make_mesh(
            self.positions,
            self.normals,
            self.colors,
            self.uvs,
            self.indices,
        );
        let transparent = make_mesh(
            self.transparent_positions,
            self.transparent_normals,
            self.transparent_colors,
            self.transparent_uvs,
            self.transparent_indices,
        );
        let water = make_mesh(
            self.water_positions,
            self.water_normals,
            self.water_colors,
            self.water_uvs,
            self.water_indices,
        );
        (opaque, transparent, water, self.triangles)
    }
}

/// Greedy meshing: sweeps the three axes, builds per-slice visibility masks
/// and merges coplanar same-material faces into maximal rectangles.
/// Positions stay in VOXEL units; the section entity scales them to metres.
#[cfg(test)]
pub fn build_chunk_mesh<F>(chunk: &Chunk, neighbor: F) -> ChunkMeshData
where
    F: FnMut(VoxelCoord) -> BlockType,
{
    build_chunk_mesh_with_water(chunk, neighbor, |_| None)
}

pub fn build_chunk_mesh_with_water<F, G>(
    chunk: &Chunk,
    mut neighbor: F,
    mut water_neighbor: G,
) -> ChunkMeshData
where
    F: FnMut(VoxelCoord) -> BlockType,
    G: FnMut(VoxelCoord) -> Option<WaterShape>,
{
    let mut data = ChunkMeshData::default();
    let n = CHUNK_SIZE;
    let origin = |pos: [i32; 3]| VoxelCoord {
        x: chunk.coord.x * n + pos[0],
        y: chunk.section_y * n + pos[1],
        z: chunk.coord.z * n + pos[2],
    };
    // Sample any voxel around this section (negative/overflow included).
    let mut block_at = |coord: VoxelCoord| -> BlockType {
        if chunk.contains(coord) {
            chunk.get_local(coord.section_local())
        } else {
            neighbor(coord)
        }
    };
    let mut water_at = |coord: VoxelCoord| -> Option<WaterShape> {
        if chunk.contains(coord) {
            chunk.water_shape_local(coord.section_local())
        } else {
            water_neighbor(coord)
        }
    };

    let side_len = n as usize;
    // The atlas assigns one tile to one Minecraft-scale block face. Merging
    // those faces would stretch and re-scale the tile whenever an adjacent
    // block is edited. Fine voxel modes may still use greedy rectangles for
    // performance, but one-metre blocks retain stable per-face UVs.
    let merge_limit = if VOXELS_PER_METER == 1 { 1 } else { side_len };
    let mut mask = vec![None; side_len * side_len];

    for axis in 0..3usize {
        let (ua, va) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in 0..2usize {
            let dir = if side == 1 { 1 } else { -1 };
            // FACES stores positive then negative for each axis; sweep side
            // zero examines the negative neighbour.
            let face = FACES[axis * 2 + (1 - side)];

            for layer in 0..n {
                for vu in 0..n as usize {
                    for vv in 0..n as usize {
                        let mut pos = [0i32; 3];
                        pos[axis] = layer;
                        pos[ua] = vu as i32;
                        pos[va] = vv as i32;
                        let block = block_at(origin(pos));
                        let mut npos = pos;
                        npos[axis] += dir;
                        let index = vu + vv * side_len;
                        // Plants emit once per cell (their crossed quads are
                        // face-independent), pinned to the top-face pass.
                        let plant_pass = block.is_plant() && axis == 1 && side == 1;
                        mask[index] = (plant_pass
                            || (block != BlockType::Air
                                && !block.is_plant()
                                && should_emit_face(block, block_at(origin(npos)))))
                        .then_some(FaceKey { block });
                    }
                }

                for vu in 0..n as usize {
                    for vv in 0..n as usize {
                        let mask_index = vu + vv * side_len;
                        let Some(key) = mask[mask_index] else {
                            continue;
                        };
                        let mut w = 1usize;
                        while w < merge_limit
                            && vu + w < side_len
                            && mask[vu + w + vv * side_len] == Some(key)
                        {
                            w += 1;
                        }
                        let mut h = 1usize;
                        'grow: while h < merge_limit && vv + h < side_len {
                            for k in 0..w {
                                if mask[vu + k + (vv + h) * side_len] != Some(key) {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }
                        for du in 0..w {
                            for dv in 0..h {
                                mask[vu + du + (vv + dv) * side_len] = None;
                            }
                        }

                        let mut base = [0i32; 3];
                        base[axis] = layer;
                        base[ua] = vu as i32;
                        base[va] = vv as i32;
                        let mut extents = [1i32; 3];
                        extents[ua] = w as i32;
                        extents[va] = h as i32;

                        emit_rect(
                            &mut data,
                            &origin,
                            base,
                            extents,
                            &face,
                            ua,
                            va,
                            key,
                            &mut block_at,
                            &mut water_at,
                        );
                    }
                }
            }
        }
    }

    data
}

/// Emits one merged quad with edit-stable block coloring.
#[allow(clippy::too_many_arguments)]
fn emit_rect<F, G>(
    data: &mut ChunkMeshData,
    origin: &dyn Fn([i32; 3]) -> VoxelCoord,
    base: [i32; 3],
    extents: [i32; 3],
    face: &Face,
    ua: usize,
    va: usize,
    key: FaceKey,
    block_at: &mut F,
    water_at: &mut G,
) where
    F: FnMut(VoxelCoord) -> BlockType,
    G: FnMut(VoxelCoord) -> Option<WaterShape>,
{
    use crate::rendering::texture_atlas::{
        get_texture_coords, ATLAS_TILES_PER_COLUMN, ATLAS_TILES_PER_ROW, ATLAS_TILE_RESOLUTION,
    };

    let block = key.block;
    let normal = face.normal;
    let light = face.light;

    let world_base = origin(base);

    // Ground plants render as two crossed cutout quads instead of a cube.
    if block.is_plant() {
        emit_plant_cross(data, world_base, block);
        return;
    }

    let face_direction = match normal {
        [1.0, 0.0, 0.0] => Some(FaceDirection::East),
        [-1.0, 0.0, 0.0] => Some(FaceDirection::West),
        [0.0, 1.0, 0.0] => Some(FaceDirection::Top),
        [0.0, -1.0, 0.0] => Some(FaceDirection::Bottom),
        [0.0, 0.0, 1.0] => Some(FaceDirection::North),
        _ => Some(FaceDirection::South),
    };

    // The atlas already owns the material palette. A second saturated block
    // tint crushed shadows and made grass/foliage read as neon; vertex color
    // now carries directional lighting only, plus a tiny deterministic
    // per-block variation on natural materials so large surfaces of one
    // block type stop reading as wallpaper. Leaves get an additional coarse
    // cluster tint (3x3x3 cells) so canopies read as two-tone foliage
    // masses instead of single-colour mega-cubes from outside.
    let variation = match block {
        BlockType::Grass
        | BlockType::Dirt
        | BlockType::Leaves
        | BlockType::PineLeaves
        | BlockType::JungleLeaves
        | BlockType::AutumnLeaves
        | BlockType::PalmLeaves => {
            let h = (world_base.x as u32).wrapping_mul(0x9E37_79B1)
                ^ (world_base.y as u32).wrapping_mul(0x85EB_CA77)
                ^ (world_base.z as u32).wrapping_mul(0xC2B2_AE3D);
            let fine = 0.965 + 0.07 * ((h >> 11) & 0x3FF) as f32 / 1024.0;
            if block == BlockType::Grass || block == BlockType::Dirt {
                fine
            } else {
                let cluster_h = (world_base.x as u32 / 3).wrapping_mul(0x2722_0A95)
                    ^ (world_base.y as u32 / 3)
                        .wrapping_mul(0x85EB_CA77)
                        .rotate_left(7)
                    ^ (world_base.z as u32 / 3)
                        .wrapping_mul(0x9E37_79B1)
                        .rotate_left(17);
                let cluster = 0.90 + 0.16 * ((cluster_h >> 9) & 0x1FF) as f32 / 512.0;
                fine * cluster
            }
        }
        _ => 1.0,
    };
    let color = Color::WHITE.to_srgba();
    let (cr, cg, cb) = (
        color.red * variation,
        color.green * variation,
        color.blue * variation,
    );
    let (ac, ar) = get_texture_coords(block, face_direction);
    let tile_u = 1.0 / ATLAS_TILES_PER_ROW as f32;
    let tile_v = 1.0 / ATLAS_TILES_PER_COLUMN as f32;
    let inset_u = 0.5 / (ATLAS_TILES_PER_ROW * ATLAS_TILE_RESOLUTION) as f32;
    let inset_v = 0.5 / (ATLAS_TILES_PER_COLUMN * ATLAS_TILE_RESOLUTION) as f32;
    let min_u = ac as f32 * tile_u + inset_u;
    let min_v = ar as f32 * tile_v + inset_v;
    let max_u = (ac + 1) as f32 * tile_u - inset_u;
    let max_v = (ar + 1) as f32 * tile_v - inset_v;

    let (positions, normals, colors, uvs, indices) = if block.is_translucent() {
        (
            &mut data.water_positions,
            &mut data.water_normals,
            &mut data.water_colors,
            &mut data.water_uvs,
            &mut data.water_indices,
        )
    } else if block.is_transparent() {
        (
            &mut data.transparent_positions,
            &mut data.transparent_normals,
            &mut data.transparent_colors,
            &mut data.transparent_uvs,
            &mut data.transparent_indices,
        )
    } else {
        (
            &mut data.positions,
            &mut data.normals,
            &mut data.colors,
            &mut data.uvs,
            &mut data.indices,
        )
    };

    let start = positions.len() as u32;
    let uv_corners = [
        [min_u, min_v],
        [max_u, min_v],
        [max_u, max_v],
        [min_u, max_v],
    ];
    // Per-vertex ambient occlusion (0fps smooth lighting): sample the three
    // voxel neighbours that touch each corner inside the shell one layer in
    // front of the face. Water stays untouched so basins keep a clean read.
    let ao = if block.is_translucent() || extents[ua] != 1 || extents[va] != 1 {
        [1.0; 4]
    } else {
        corner_ao(world_base, normal, ua, va, &face.vertices, block_at)
    };
    // Anisotropy fix: pick the triangle diagonal that connects the two
    // darkest corners so interpolation stays symmetric across the quad.
    let flip = ao[0] + ao[2] > ao[1] + ao[3];
    // Depth gradient + foam on water tops; sides get a damped tint only so
    // shoreline blocks stay translucent rather than slab-like. Sand touching
    // water darkens into a wet band at the waterline.
    let wet_sand = block == BlockType::Sand
        && [
            world_base.offset(0, 1, 0),
            world_base.offset(0, -1, 0),
            world_base.offset(1, 0, 0),
            world_base.offset(-1, 0, 0),
            world_base.offset(0, 0, 1),
            world_base.offset(0, 0, -1),
        ]
        .iter()
        .any(|n| block_at(*n) == BlockType::Water);
    let (tr, tg, tb, ta) = if block == BlockType::Water && normal[1] > -0.5 {
        water_face_tint(world_base, normal[1] > 0.5, block_at)
    } else if wet_sand {
        (0.74, 0.77, 0.84, 1.0)
    } else {
        (1.0, 1.0, 1.0, 1.0)
    };
    // Mountain materials zebra-stripe on terraced slopes: bright tops next
    // to dark, AO-creased risers align into contour bands. Flatten the
    // tread/riser luminance gap for vertical faces of the rocky palette.
    let rocky_face = normal[1] == 0.0
        && matches!(
            block,
            BlockType::Stone
                | BlockType::Snow
                | BlockType::Gravel
                | BlockType::Sandstone
                | BlockType::Terracotta
                | BlockType::MossStone
        );
    let face_light = if rocky_face { light.max(0.94) } else { light };
    for (corner, vertex) in face.vertices.iter().enumerate() {
        // Position: scale the two tangent components by rect extents.
        let mut p = [
            world_base.x as f32 + vertex[0],
            world_base.y as f32 + vertex[1],
            world_base.z as f32 + vertex[2],
        ];
        if normal[ua] == 0.0 {
            p[ua] = world_base_axis(world_base, ua) + vertex[ua] * extents[ua] as f32;
        }
        if normal[va] == 0.0 {
            p[va] = world_base_axis(world_base, va) + vertex[va] * extents[va] as f32;
        }
        // Minecraft-style water uses a lowered source surface and slopes
        // toward higher flow levels. Falling strands stay full-height so
        // vertically adjacent cells form one continuous waterfall.
        if block == BlockType::Water && vertex[1] > 0.5 && normal[1] >= 0.0 {
            p[1] = world_base.y as f32
                + water_corner_height(world_base, vertex[0], vertex[2], block_at, water_at);
        }

        positions.push([p[0], p[1], p[2]]);
        normals.push(normal);
        let corner_ao = if rocky_face {
            ao[corner].max(0.92)
        } else {
            ao[corner]
        };
        colors.push([
            cr * face_light * corner_ao * tr,
            cg * face_light * corner_ao * tg,
            cb * face_light * corner_ao * tb,
            color.alpha * ta,
        ]);
        uvs.push(uv_corners[corner]);
    }
    indices.extend(if flip {
        [start, start + 1, start + 3, start + 1, start + 2, start + 3]
    } else {
        [start, start + 1, start + 2, start, start + 2, start + 3]
    });
    data.triangles += 2;
}

/// Brightness for the four occlusion levels reported by [`vertex_ao_level`].
const AO_LEVELS: [f32; 4] = [0.60, 0.78, 0.90, 1.0];

/// The four in-plane neighbours of a voxel.
fn horizontal_neighbors(coord: VoxelCoord) -> [VoxelCoord; 4] {
    [
        coord.offset(1, 0, 0),
        coord.offset(-1, 0, 0),
        coord.offset(0, 0, 1),
        coord.offset(0, 0, -1),
    ]
}

/// Per-face water tint: shallows read bright and clear over the sandy
/// floor, deep basins grade into dark rich blue, and top faces touching
/// the shore get a slim foam rim. Alpha rises with depth so shallows stay
/// see-through while deep water turns properly opaque. Vertex-colour only -
/// no extra geometry or draw cost. Values stay close to 1.0: HDR-strength
/// tints turned shoreline blocks into opaque pale slabs that read as ice.
#[allow(clippy::type_complexity)]
fn water_face_tint<F>(coord: VoxelCoord, is_top: bool, block_at: &mut F) -> (f32, f32, f32, f32)
where
    F: FnMut(VoxelCoord) -> BlockType,
{
    // Depth to the floor (capped): 0 = ankle-deep shelf, 12 = deep sea.
    let mut depth = 0usize;
    while depth < 12 && block_at(coord.offset(0, -(depth as i32 + 1), 0)) == BlockType::Water {
        depth += 1;
    }
    let shallowness = 1.0 - depth as f32 / 12.0;

    // Foam only on top faces: land rim on the shallow shelf (so rivers
    // don't turn into white ribbons) plus spill edges. Side faces stay
    // untinted by foam - foamed sides rendered as solid glowing slabs.
    let mut foam_strength = 0.0f32;
    if is_top {
        let touches_land = horizontal_neighbors(coord)
            .iter()
            .any(|n| !matches!(block_at(*n), BlockType::Water | BlockType::Air));
        let spill_edge = horizontal_neighbors(coord)
            .iter()
            .any(|n| block_at(*n) == BlockType::Air);
        if touches_land && depth == 0 {
            foam_strength = 0.40;
        } else if touches_land && depth <= 1 {
            foam_strength = 0.22;
        } else if spill_edge {
            foam_strength = 0.50;
        }
    }

    let deep = (0.72, 0.85, 1.04);
    let shallow = (1.26, 1.16, 1.02);
    let t = shallowness * shallowness;
    let mut r = deep.0 + (shallow.0 - deep.0) * t;
    let mut g = deep.1 + (shallow.1 - deep.1) * t;
    let mut b = deep.2 + (shallow.2 - deep.2) * t;
    // Clear shallows, richer depths: the texture's own translucency scales
    // with the vertex alpha, so shallow floors show through cleanly while
    // deep water keeps its colour instead of washing out.
    let mut alpha = 0.78 + (1.0 - shallowness) * 0.30;
    // Side faces get half the gradient so shoreline water blocks don't
    // render as bright plates against the sand.
    if !is_top {
        r = 1.0 + (r - 1.0) * 0.5;
        g = 1.0 + (g - 1.0) * 0.5;
        b = 1.0 + (b - 1.0) * 0.5;
        alpha = 1.0 + (alpha - 1.0) * 0.5;
    }
    if foam_strength > 0.0 {
        const FOAM: (f32, f32, f32) = (2.3, 2.35, 2.3);
        r += (FOAM.0 - r) * foam_strength;
        g += (FOAM.1 - g) * foam_strength;
        b += (FOAM.2 - b) * foam_strength;
        alpha = (alpha + 0.25).min(1.2);
    }
    (r, g, b, alpha)
}

/// Only fully opaque voxels darken their neighbourhood; water and air keep
/// shorelines and cave mouths bright.
fn occludes(block: BlockType) -> bool {
    block.is_solid()
}

fn corner_ao<F>(
    base: VoxelCoord,
    normal: [f32; 3],
    ua: usize,
    va: usize,
    vertices: &[[f32; 3]; 4],
    block_at: &mut F,
) -> [f32; 4]
where
    F: FnMut(VoxelCoord) -> BlockType,
{
    let front = base.offset(normal[0] as i32, normal[1] as i32, normal[2] as i32);
    let (ux, uy, uz) = match ua {
        0 => (1, 0, 0),
        1 => (0, 1, 0),
        _ => (0, 0, 1),
    };
    let (vx, vy, vz) = match va {
        0 => (1, 0, 0),
        1 => (0, 1, 0),
        _ => (0, 0, 1),
    };
    let mut levels = [0.0f32; 4];
    for (corner, vertex) in vertices.iter().enumerate() {
        let du = if vertex[ua] < 0.5 { -1 } else { 1 };
        let dv = if vertex[va] < 0.5 { -1 } else { 1 };
        let side1 = u8::from(occludes(block_at(front.offset(du * ux, du * uy, du * uz))));
        let side2 = u8::from(occludes(block_at(front.offset(dv * vx, dv * vy, dv * vz))));
        let diagonal = u8::from(occludes(block_at(front.offset(
            du * ux + dv * vx,
            du * uy + dv * vy,
            du * uz + dv * vz,
        ))));
        let level = if side1 == 1 && side2 == 1 {
            0
        } else {
            3 - (side1 + side2 + diagonal)
        };
        levels[corner] = AO_LEVELS[level as usize];
    }
    levels
}

fn water_corner_height<F, G>(
    coord: VoxelCoord,
    corner_x: f32,
    corner_z: f32,
    block_at: &mut F,
    water_at: &mut G,
) -> f32
where
    F: FnMut(VoxelCoord) -> BlockType,
    G: FnMut(VoxelCoord) -> Option<WaterShape>,
{
    if water_at(coord).is_some_and(|shape| shape.falling) {
        return 1.0;
    }

    let x_offsets = if corner_x < 0.5 { [-1, 0] } else { [0, 1] };
    let z_offsets = if corner_z < 0.5 { [-1, 0] } else { [0, 1] };
    let mut height_sum = 0.0;
    let mut weight_sum = 0.0;

    for dx in x_offsets {
        for dz in z_offsets {
            let sample = coord.offset(dx, 0, dz);
            let sampled_block = block_at(sample);
            if sampled_block != BlockType::Water {
                // Air at the same elevation is an open spill edge. Including
                // it as zero-height water tapers shorelines and flow fronts;
                // solid banks stay ignored and retain a level waterline.
                if sampled_block == BlockType::Air {
                    weight_sum += 1.0;
                }
                continue;
            }
            let height = if block_at(sample.offset(0, 1, 0)) == BlockType::Water
                || water_at(sample).is_some_and(|shape| shape.falling)
            {
                1.0
            } else {
                let level = water_at(sample).map_or(0, |shape| shape.level.min(7));
                f32::from(8 - level) / 9.0
            };
            let weight = if height >= 8.0 / 9.0 { 10.0 } else { 1.0 };
            height_sum += height * weight;
            weight_sum += weight;
        }
    }

    if weight_sum > 0.0 {
        height_sum / weight_sum
    } else {
        8.0 / 9.0
    }
}

fn world_base_axis(base: VoxelCoord, axis: usize) -> f32 {
    match axis {
        0 => base.x as f32,
        1 => base.y as f32,
        _ => base.z as f32,
    }
}

/// Emits the two crossed cutout quads for one ground-plant cell. Both quads
/// use the up normal so plants light like the lawn they sit on; the foliage
/// material is double-sided, so no extra reversed copies are needed.
fn emit_plant_cross(data: &mut ChunkMeshData, base: VoxelCoord, block: BlockType) {
    use crate::rendering::texture_atlas::{
        get_texture_coords, ATLAS_TILES_PER_COLUMN, ATLAS_TILES_PER_ROW, ATLAS_TILE_RESOLUTION,
    };

    let (ac, ar) = get_texture_coords(block, Some(FaceDirection::Top));
    let tile_u = 1.0 / ATLAS_TILES_PER_ROW as f32;
    let tile_v = 1.0 / ATLAS_TILES_PER_COLUMN as f32;
    let inset_u = 0.5 / (ATLAS_TILES_PER_ROW * ATLAS_TILE_RESOLUTION) as f32;
    let inset_v = 0.5 / (ATLAS_TILES_PER_COLUMN * ATLAS_TILE_RESOLUTION) as f32;
    let min_u = ac as f32 * tile_u + inset_u;
    let min_v = ar as f32 * tile_v + inset_v;
    let max_u = (ac + 1) as f32 * tile_u - inset_u;
    let max_v = (ar + 1) as f32 * tile_v - inset_v;
    let uv_corners = [
        [min_u, min_v],
        [max_u, min_v],
        [max_u, max_v],
        [min_u, max_v],
    ];

    // Full-cell height would turn tufts into green walls; each species gets
    // its own natural silhouette height.
    let height = match block {
        BlockType::GrassTuft => 0.45,
        BlockType::FlowersYellow | BlockType::FlowersWhite | BlockType::FlowersRed => 0.55,
        BlockType::Fern => 0.70,
        BlockType::Mushroom => 0.38,
        _ => 0.5,
    };

    // Two diagonal quads across the cell, slightly inset so dense clumps
    // keep readable gaps between neighbouring sprites.
    let inset = 0.10;
    let lo = inset;
    let hi = 1.0 - inset;
    let quads = [
        [
            [lo, 0.0, lo],
            [hi, 0.0, hi],
            [hi, height, hi],
            [lo, height, lo],
        ],
        [
            [hi, 0.0, lo],
            [lo, 0.0, hi],
            [lo, height, hi],
            [hi, height, lo],
        ],
    ];
    let normal = [0.0, 1.0, 0.0];
    let light = 1.02;

    for quad in quads {
        let start = data.transparent_positions.len() as u32;
        for (corner, vertex) in quad.iter().enumerate() {
            data.transparent_positions.push([
                base.x as f32 + vertex[0],
                base.y as f32 + vertex[1],
                base.z as f32 + vertex[2],
            ]);
            data.transparent_normals.push(normal);
            data.transparent_colors.push([light, light, light, 1.0]);
            data.transparent_uvs.push(uv_corners[corner]);
        }
        data.transparent_indices
            .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
        data.triangles += 2;
    }
}

fn should_emit_face(block: BlockType, neighbor: BlockType) -> bool {
    if neighbor == BlockType::Air {
        return true;
    }
    neighbor.is_transparent() && neighbor != block
}

fn make_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Option<Mesh> {
    if positions.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[derive(Debug, Clone, Copy)]
struct Face {
    normal: [f32; 3],
    vertices: [[f32; 3]; 4],
    light: f32,
}

const FACES: [Face; 6] = [
    Face {
        normal: [1.0, 0.0, 0.0],
        vertices: [
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
        light: 0.84,
    },
    Face {
        normal: [-1.0, 0.0, 0.0],
        vertices: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        light: 0.80,
    },
    Face {
        normal: [0.0, 1.0, 0.0],
        vertices: [
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        light: 1.00,
    },
    Face {
        normal: [0.0, -1.0, 0.0],
        vertices: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        light: 0.64,
    },
    Face {
        normal: [0.0, 0.0, 1.0],
        vertices: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        light: 0.90,
    },
    Face {
        normal: [0.0, 0.0, -1.0],
        vertices: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        light: 0.76,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::Chunk, coordinates::ChunkCoord};

    fn block_or_air(chunk: &Chunk, coord: VoxelCoord) -> BlockType {
        chunk.get_world(coord).unwrap_or(BlockType::Air)
    }

    #[test]
    fn single_voxel_emits_six_outward_faces() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        chunk.set_local(LocalVoxelCoord { x: 1, y: 1, z: 1 }, BlockType::Stone);

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        assert_eq!(data.triangles, 12);
        assert_eq!(data.positions.len(), 24);
        for expected in FACES.map(|face| face.normal) {
            assert_eq!(
                data.normals
                    .iter()
                    .filter(|normal| **normal == expected)
                    .count(),
                4
            );
        }
    }

    #[test]
    fn adjacent_solid_blocks_do_not_generate_internal_faces() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for y in 0..CHUNK_SIZE as usize {
            for z in 0..CHUNK_SIZE as usize {
                for x in 0..CHUNK_SIZE as usize {
                    chunk.set_local(LocalVoxelCoord { x, y, z }, BlockType::Air);
                }
            }
        }
        chunk.set_local(LocalVoxelCoord { x: 1, y: 1, z: 1 }, BlockType::Stone);
        chunk.set_local(LocalVoxelCoord { x: 2, y: 1, z: 1 }, BlockType::Stone);

        let data = build_chunk_mesh(&chunk, |coord| {
            chunk.get_world(coord).unwrap_or(BlockType::Air)
        });
        // Ten exposed Minecraft-scale faces, with the shared face culled.
        assert_eq!(data.triangles, 20);
        assert_eq!(data.positions.len(), 10 * 4);
    }

    #[test]
    fn neighboring_edit_does_not_recolor_untouched_block_faces() {
        let mut before = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for x in 1..=5 {
            before.set_local(LocalVoxelCoord { x, y: 1, z: 1 }, BlockType::Stone);
        }
        let before_mesh = build_chunk_mesh(&before, |coord| block_or_air(&before, coord));

        let mut after = before.clone();
        after.set_local(LocalVoxelCoord { x: 3, y: 1, z: 1 }, BlockType::Air);
        let after_mesh = build_chunk_mesh(&after, |coord| block_or_air(&after, coord));

        let top_color = |mesh: &ChunkMeshData| {
            mesh.normals
                .iter()
                .zip(&mesh.colors)
                .find_map(|(normal, color)| (*normal == [0.0, 1.0, 0.0]).then_some(*color))
                .unwrap()
        };
        let expected = top_color(&before_mesh);
        assert_eq!(top_color(&after_mesh), expected);
        assert!(after_mesh
            .normals
            .iter()
            .zip(&after_mesh.colors)
            .filter(|(normal, _)| **normal == [0.0, 1.0, 0.0])
            .all(|(_, color)| *color == expected));
    }

    #[test]
    fn one_metre_blocks_keep_one_texture_tile_per_face() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for x in 0..8usize {
            for z in 0..8usize {
                chunk.set_local(LocalVoxelCoord { x, y: 0, z }, BlockType::Stone);
            }
        }
        let data = build_chunk_mesh(&chunk, |coord| {
            chunk.get_world(coord).unwrap_or(BlockType::Air)
        });
        // 64 top + 64 bottom + 32 edge faces, two triangles each. Keeping
        // these faces separate prevents atlas tiles changing scale on edits.
        assert_eq!(data.triangles, 320);
        assert!(data.uvs.chunks_exact(4).all(|quad| quad == &data.uvs[..4]));
        let max_x = data.positions.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!((max_x - 8.0).abs() < 1e-5);
    }

    #[test]
    fn solid_section_has_no_internal_geometry() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for y in 0..CHUNK_SIZE as usize {
            for z in 0..CHUNK_SIZE as usize {
                for x in 0..CHUNK_SIZE as usize {
                    chunk.set_local(LocalVoxelCoord { x, y, z }, BlockType::Stone);
                }
            }
        }

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        let exterior_faces = 6 * CHUNK_SIZE as usize * CHUNK_SIZE as usize;
        assert_eq!(data.triangles, exterior_faces * 2);
        assert_eq!(data.positions.len(), exterior_faces * 4);
    }

    #[test]
    fn adjacent_section_voxel_culls_boundary_face() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        chunk.set_local(
            LocalVoxelCoord {
                x: CHUNK_SIZE as usize - 1,
                y: 1,
                z: 1,
            },
            BlockType::Stone,
        );
        let across_boundary = VoxelCoord { x: 32, y: 1, z: 1 };
        let data = build_chunk_mesh(&chunk, |coord| {
            if coord == across_boundary {
                BlockType::Stone
            } else {
                block_or_air(&chunk, coord)
            }
        });

        assert_eq!(data.triangles, 10);
        assert!(!data.normals.contains(&[1.0, 0.0, 0.0]));
    }

    #[test]
    fn different_materials_do_not_greedy_merge() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        chunk.set_local(LocalVoxelCoord { x: 1, y: 1, z: 1 }, BlockType::Stone);
        chunk.set_local(LocalVoxelCoord { x: 2, y: 1, z: 1 }, BlockType::Dirt);

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        let top_vertices = data
            .normals
            .iter()
            .filter(|normal| **normal == [0.0, 1.0, 0.0])
            .count();
        assert_eq!(
            top_vertices, 8,
            "top faces must retain their material boundary"
        );
    }

    #[test]
    fn section_mesh_positions_are_world_voxel_coordinates() {
        let mut chunk = Chunk::new(ChunkCoord { x: 2, z: -3 }, -1);
        chunk.set_local(LocalVoxelCoord { x: 1, y: 2, z: 3 }, BlockType::Stone);

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        let min = data.positions.iter().fold(Vec3::splat(f32::MAX), |min, p| {
            min.min(Vec3::from_array(*p))
        });
        let max = data.positions.iter().fold(Vec3::splat(f32::MIN), |max, p| {
            max.max(Vec3::from_array(*p))
        });
        assert_eq!(min, Vec3::new(65.0, -30.0, -93.0));
        assert_eq!(max, Vec3::new(66.0, -29.0, -92.0));
    }

    #[test]
    fn emitted_triangle_winding_matches_normals() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        chunk.set_local(LocalVoxelCoord { x: 1, y: 1, z: 1 }, BlockType::Stone);
        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));

        for triangle in data.indices.chunks_exact(3) {
            let a = Vec3::from_array(data.positions[triangle[0] as usize]);
            let b = Vec3::from_array(data.positions[triangle[1] as usize]);
            let c = Vec3::from_array(data.positions[triangle[2] as usize]);
            let normal = Vec3::from_array(data.normals[triangle[0] as usize]);
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
        }
    }

    #[test]
    fn corner_blocks_darken_adjacent_faces_without_breaking_winding() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        // Flat slab with one raised block: the top faces around it must pick
        // up occlusion shading and every triangle keeps its outward winding.
        for x in 0..3 {
            for z in 0..3 {
                chunk.set_local(LocalVoxelCoord { x, y: 1, z }, BlockType::Stone);
            }
        }
        chunk.set_local(LocalVoxelCoord { x: 1, y: 2, z: 1 }, BlockType::Stone);

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        let shaded = data
            .colors
            .iter()
            .zip(&data.normals)
            .filter(|(_, normal)| **normal == [0.0, 1.0, 0.0])
            .map(|(color, _)| color[0])
            .any(|red| red < 1.05);
        assert!(shaded, "no ambient occlusion beside the raised block");

        for triangle in data.indices.chunks_exact(3) {
            let a = Vec3::from_array(data.positions[triangle[0] as usize]);
            let b = Vec3::from_array(data.positions[triangle[1] as usize]);
            let c = Vec3::from_array(data.positions[triangle[2] as usize]);
            let normal = Vec3::from_array(data.normals[triangle[0] as usize]);
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
        }
    }

    #[test]
    fn water_face_against_opaque_voxel_is_hidden() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        chunk.set_local(LocalVoxelCoord { x: 1, y: 1, z: 1 }, BlockType::Water);
        chunk.set_local(LocalVoxelCoord { x: 2, y: 1, z: 1 }, BlockType::Stone);

        let data = build_chunk_mesh(&chunk, |coord| block_or_air(&chunk, coord));
        assert!(!data.water_normals.contains(&[1.0, 0.0, 0.0]));
    }

    #[test]
    fn flowing_water_uses_shallow_sloped_surfaces_and_full_falling_strands() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        let source = LocalVoxelCoord { x: 4, y: 1, z: 4 };
        let shallow = LocalVoxelCoord { x: 5, y: 1, z: 4 };
        chunk.set_local(source, BlockType::Water);
        chunk.set_water_shape_local(
            source,
            WaterShape {
                level: 0,
                falling: false,
            },
        );
        chunk.set_local(shallow, BlockType::Water);
        chunk.set_water_shape_local(
            shallow,
            WaterShape {
                level: 6,
                falling: false,
            },
        );

        let sloped = build_chunk_mesh_with_water(
            &chunk,
            |coord| block_or_air(&chunk, coord),
            |coord| {
                chunk
                    .contains(coord)
                    .then(|| chunk.water_shape_local(coord.section_local()))
                    .flatten()
            },
        );
        let top_heights: Vec<_> = sloped
            .water_positions
            .iter()
            .zip(&sloped.water_normals)
            .filter_map(|(position, normal)| (*normal == [0.0, 1.0, 0.0]).then_some(position[1]))
            .collect();
        assert!(top_heights.iter().any(|height| *height > 1.65));
        assert!(top_heights.iter().any(|height| *height < 1.30));
        assert!(top_heights.iter().all(|height| *height < 1.90));

        let mut falling = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for y in 1..=2 {
            let local = LocalVoxelCoord { x: 4, y, z: 4 };
            falling.set_local(local, BlockType::Water);
            falling.set_water_shape_local(
                local,
                WaterShape {
                    level: 2,
                    falling: true,
                },
            );
        }
        let strand = build_chunk_mesh_with_water(
            &falling,
            |coord| block_or_air(&falling, coord),
            |coord| {
                falling
                    .contains(coord)
                    .then(|| falling.water_shape_local(coord.section_local()))
                    .flatten()
            },
        );
        assert!(strand
            .water_positions
            .iter()
            .any(|position| (position[1] - 3.0).abs() < 1e-5));
    }

    #[test]
    fn foliage_and_water_use_separate_meshes_with_inset_uvs() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        for y in 0..CHUNK_SIZE as usize {
            for z in 0..CHUNK_SIZE as usize {
                for x in 0..CHUNK_SIZE as usize {
                    chunk.set_local(LocalVoxelCoord { x, y, z }, BlockType::Air);
                }
            }
        }
        chunk.set_local(LocalVoxelCoord { x: 2, y: 2, z: 2 }, BlockType::Leaves);
        chunk.set_local(LocalVoxelCoord { x: 5, y: 2, z: 2 }, BlockType::Water);

        let data = build_chunk_mesh(&chunk, |coord| {
            chunk.get_world(coord).unwrap_or(BlockType::Air)
        });
        assert!(!data.transparent_positions.is_empty());
        assert!(!data.water_positions.is_empty());
        assert!(data
            .transparent_uvs
            .iter()
            .chain(&data.water_uvs)
            .all(|uv| uv[0] > 0.0 && uv[0] < 1.0 && uv[1] > 0.0 && uv[1] < 1.0));
    }
}

#[cfg(test)]
mod flower_debug {
    use super::*;
    use crate::world::{
        chunk::Chunk,
        coordinates::{ChunkCoord, LocalVoxelCoord},
    };

    #[test]
    fn flower_quads_are_vertical() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 0);
        let local = LocalVoxelCoord { x: 4, y: 1, z: 4 };
        chunk.set_local(local, BlockType::FlowersYellow);
        for y in 0..3 {
            for z in 0..8 {
                for x in 0..8 {
                    if (x, y, z) != (4, 1, 4) {
                        chunk.set_local(LocalVoxelCoord { x, y, z }, BlockType::Air);
                    }
                }
            }
        }
        let data = build_chunk_mesh(&chunk, |coord| {
            chunk.get_world(coord).unwrap_or(BlockType::Air)
        });
        let ys: Vec<f32> = data.transparent_positions.iter().map(|p| p[1]).collect();
        println!(
            "transparent verts: {}",
            data.transparent_positions.len() / 4
        );
        println!(
            "y range: {:?}..{:?}",
            ys.iter().cloned().fold(f32::MAX, f32::min),
            ys.iter().cloned().fold(f32::MIN, f32::max)
        );
        println!("positions: {:?}", data.transparent_positions);
    }
}
