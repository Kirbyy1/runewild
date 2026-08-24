//! Deterministic tree archetypes and ground decorations.
//!
//! Every archetype enforces a proportion rule: the canopy grows with the
//! trunk, so tall trees always carry large crowns instead of looking like
//! poles with leaves attached. All builders are pure functions of
//! `(seed, x, z)` so chunk generation is seamless and reproducible.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::world::{coordinates::VoxelCoord, voxel::BlockType};

/// Lattice size (in blocks) used for tree candidates.
pub const SLOT_SIZE: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    YoungOak,
    Oak,
    TallOak,
    WideOak,
    Birch,
    Giant,
    PineSmall,
    PineMedium,
    PineTall,
    PineSnowy,
    JungleMedium,
    JungleGiant,
    Palm,
    Acacia,
    SwampWillow,
    AutumnOak,
}

/// A fully resolved tree shape. The crown radius is derived from the trunk
/// height so proportions stay believable across the whole height range.
#[derive(Debug, Clone, Copy)]
pub struct TreeSpec {
    pub kind: TreeKind,
    pub trunk_h: i32,
    /// 1 = single column, 2 = 2x2 core, 3 = thick base tapering to 1.
    pub thickness: u32,
    pub crown_radius: i32,
    pub crown_height: i32,
    pub wood: BlockType,
    pub leaf: BlockType,
    pub roots: bool,
    pub vines: bool,
}

impl TreeKind {
    #[cfg(test)]
    pub const ALL: [Self; 16] = [
        Self::YoungOak,
        Self::Oak,
        Self::TallOak,
        Self::WideOak,
        Self::Birch,
        Self::Giant,
        Self::PineSmall,
        Self::PineMedium,
        Self::PineTall,
        Self::PineSnowy,
        Self::JungleMedium,
        Self::JungleGiant,
        Self::Palm,
        Self::Acacia,
        Self::SwampWillow,
        Self::AutumnOak,
    ];

    /// Resolves the concrete shape for one tree instance. The canopy radius
    /// scales with the rolled trunk height (the proportion rule).
    pub fn spec(self, seed: u64, x: i32, z: i32) -> TreeSpec {
        let v = |salt: i32| (hash(seed, x, z, salt) % 100) as i32;
        let wood = BlockType::Wood;
        match self {
            TreeKind::YoungOak => TreeSpec {
                kind: self,
                trunk_h: 4 + v(1) % 2,
                thickness: 1,
                crown_radius: 3,
                crown_height: 4,
                wood,
                leaf: BlockType::Leaves,
                roots: false,
                vines: false,
            },
            TreeKind::Oak => {
                let trunk_h = 6 + v(3) % 3; // 6..8
                TreeSpec {
                    kind: self,
                    trunk_h,
                    thickness: 1,
                    crown_radius: 4 + (trunk_h - 6) / 2,
                    crown_height: 5,
                    wood,
                    leaf: BlockType::Leaves,
                    roots: false,
                    vines: false,
                }
            }
            TreeKind::TallOak => {
                let trunk_h = 9 + v(5) % 4; // 9..12
                TreeSpec {
                    kind: self,
                    trunk_h,
                    thickness: 1,
                    // Canopy width always reaches at least the trunk height.
                    crown_radius: 5 + (trunk_h - 9) / 2,
                    crown_height: 6,
                    wood,
                    leaf: BlockType::Leaves,
                    roots: true,
                    vines: false,
                }
            }
            TreeKind::WideOak => TreeSpec {
                kind: self,
                trunk_h: 7 + v(7) % 3,
                thickness: 2,
                crown_radius: 6,
                crown_height: 5,
                wood,
                leaf: BlockType::Leaves,
                roots: true,
                vines: false,
            },
            TreeKind::Birch => {
                let trunk_h = 8 + v(9) % 4; // 8..11
                TreeSpec {
                    kind: self,
                    trunk_h,
                    thickness: 1,
                    // Birches stay deliberately narrower than oaks.
                    crown_radius: 3 + (trunk_h - 8) / 3,
                    crown_height: 6,
                    wood: BlockType::BirchWood,
                    leaf: BlockType::Leaves,
                    roots: false,
                    vines: false,
                }
            }
            TreeKind::Giant => {
                let trunk_h = 12 + v(11) % 4; // 12..15
                TreeSpec {
                    kind: self,
                    trunk_h,
                    thickness: 3,
                    crown_radius: 7 + (trunk_h - 12) / 2, // 7..8
                    crown_height: 6,
                    wood: BlockType::JungleWood,
                    leaf: BlockType::JungleLeaves,
                    roots: true,
                    vines: true,
                }
            }
            TreeKind::JungleGiant => {
                let trunk_h = 13 + v(29) % 4; // 13..16
                TreeSpec {
                    kind: self,
                    trunk_h,
                    thickness: 3,
                    crown_radius: 7 + (trunk_h - 13) / 2,
                    crown_height: 6,
                    wood: BlockType::JungleWood,
                    leaf: BlockType::JungleLeaves,
                    roots: true,
                    vines: true,
                }
            }
            TreeKind::PineSmall => pine(self, 5 + v(13) % 2, 2, wood),
            TreeKind::PineMedium => pine(self, 8 + v(15) % 3, 3, wood),
            TreeKind::PineTall => pine(self, 12 + v(17) % 4, 4, wood),
            TreeKind::PineSnowy => pine(self, 9 + v(19) % 3, 3, wood),
            TreeKind::JungleMedium => TreeSpec {
                kind: self,
                trunk_h: 9 + v(21) % 3,
                thickness: 1,
                crown_radius: 5,
                crown_height: 5,
                wood: BlockType::JungleWood,
                leaf: BlockType::JungleLeaves,
                roots: false,
                vines: true,
            },
            TreeKind::Palm => TreeSpec {
                kind: self,
                trunk_h: 8 + v(23) % 3,
                thickness: 1,
                crown_radius: 4,
                crown_height: 1,
                wood: BlockType::JungleWood,
                leaf: BlockType::PalmLeaves,
                roots: false,
                vines: false,
            },
            TreeKind::Acacia => TreeSpec {
                kind: self,
                trunk_h: 6 + v(25) % 3,
                thickness: 1,
                crown_radius: 4,
                crown_height: 2,
                wood,
                leaf: BlockType::AutumnLeaves,
                roots: false,
                vines: false,
            },
            TreeKind::SwampWillow => TreeSpec {
                kind: self,
                trunk_h: 7 + v(27) % 3,
                thickness: 2,
                crown_radius: 5,
                crown_height: 5,
                wood,
                leaf: BlockType::Leaves,
                roots: true,
                vines: true,
            },
            TreeKind::AutumnOak => {
                let base = TreeKind::Oak.spec(seed, x, z);
                TreeSpec {
                    kind: self,
                    leaf: BlockType::AutumnLeaves,
                    ..base
                }
            }
        }
    }

    /// Horizontal clearance demanded from other *large* structures,
    /// expressed in slot units of [`SLOT_SIZE`].
    pub fn exclusion_slots(self) -> i32 {
        match self {
            TreeKind::Giant | TreeKind::JungleGiant => 2,
            TreeKind::TallOak | TreeKind::WideOak | TreeKind::PineTall | TreeKind::JungleMedium => {
                1
            }
            _ => 0,
        }
    }

    /// Downgrade chain when a large candidate loses its spacing contest.
    /// Keeps density up while enforcing size separation deterministically.
    pub fn downgrade(self) -> Option<TreeKind> {
        match self {
            TreeKind::Giant => Some(TreeKind::WideOak),
            TreeKind::JungleGiant => Some(TreeKind::JungleMedium),
            TreeKind::TallOak | TreeKind::WideOak => Some(TreeKind::Oak),
            TreeKind::PineTall => Some(TreeKind::PineMedium),
            TreeKind::JungleMedium => Some(TreeKind::Oak),
            _ => None,
        }
    }
}

fn pine(kind: TreeKind, trunk_h: i32, crown_radius: i32, wood: BlockType) -> TreeSpec {
    TreeSpec {
        kind,
        trunk_h,
        thickness: 1,
        crown_radius,
        // Tapered cone occupies most of the trunk - never a bare pole.
        crown_height: (trunk_h - 2).max(2),
        wood,
        leaf: BlockType::PineLeaves,
        roots: false,
        vines: false,
    }
}

#[derive(Debug, Clone)]
pub struct TreeVoxel {
    pub coord: VoxelCoord,
    pub block: BlockType,
}

/// Ground decoration kinds placed by the contextual decor pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundKind {
    Pebble,
    Rock,
    Boulder,
    MegaBoulder,
    FallenLog,
    Bush,
}

/// Builds the full voxel list for one tree.
pub fn build_tree(seed: u64, base: VoxelCoord, spec: &TreeSpec) -> Vec<TreeVoxel> {
    let mut out = Vec::with_capacity(160);
    let top_y = base.y + spec.trunk_h;

    if spec.roots {
        for (dx, dz) in [(1i32, 0), (-1, 0), (0, 1), (0, -1)] {
            if !hash(seed, base.x + dx * 3, base.z + dz * 3, 41).is_multiple_of(4) {
                push(&mut out, base.offset(dx, 0, dz), spec.wood);
                if spec.thickness > 1 {
                    push(&mut out, base.offset(dx, 1, dz), spec.wood);
                }
            }
        }
    }

    for dy in 0..spec.trunk_h {
        match spec.thickness {
            3 => {
                push(&mut out, base.offset(0, dy, 0), spec.wood);
                if dy < spec.trunk_h * 2 / 3 {
                    push(&mut out, base.offset(1, dy, 0), spec.wood);
                    push(&mut out, base.offset(0, dy, 1), spec.wood);
                    push(&mut out, base.offset(1, dy, 1), spec.wood);
                }
            }
            2 => {
                push(&mut out, base.offset(0, dy, 0), spec.wood);
                push(&mut out, base.offset(1, dy, 0), spec.wood);
                push(&mut out, base.offset(0, dy, 1), spec.wood);
                push(&mut out, base.offset(1, dy, 1), spec.wood);
            }
            _ => push(&mut out, base.offset(0, dy, 0), spec.wood),
        }
    }

    match spec.kind {
        TreeKind::Palm => rosette(seed, base, top_y, spec, &mut out),
        TreeKind::Acacia => acacia_crown(seed, base, spec, &mut out),
        TreeKind::Giant | TreeKind::JungleGiant => ancient_crown(seed, base, spec, &mut out),
        TreeKind::JungleMedium => branching_crown(seed, base, spec, 5, &mut out),
        TreeKind::SwampWillow => willow_crown(seed, base, spec, &mut out),
        TreeKind::Birch => birch_crown(seed, base, spec, &mut out),
        _ if is_conifer(spec.kind) => layered_conifer(seed, base, spec, &mut out),
        _ => branching_crown(seed, base, spec, 4, &mut out),
    }

    deduplicate_tree(&mut out);
    retain_connected_to_trunk(&mut out, base);
    out
}

fn is_conifer(kind: TreeKind) -> bool {
    matches!(
        kind,
        TreeKind::PineSmall | TreeKind::PineMedium | TreeKind::PineTall | TreeKind::PineSnowy
    )
}

const BRANCH_DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Broadleaf crown made from a central mass and offset branch lobes. The
/// overlapping lobes create an irregular outline without detached leaves.
fn branching_crown(
    seed: u64,
    base: VoxelCoord,
    spec: &TreeSpec,
    arms: usize,
    out: &mut Vec<TreeVoxel>,
) {
    let center_y = spec.trunk_h - 1;
    leaf_cluster(
        seed,
        base.offset(0, center_y, 0),
        (spec.crown_radius - 1).max(2),
        (spec.crown_height / 2).max(2),
        spec.leaf,
        101,
        out,
    );

    let rotation = (hash(seed, base.x, base.z, 103) % 8) as usize;
    for arm in 0..arms {
        let direction = BRANCH_DIRECTIONS[(rotation + arm * 2 + arm / 3) % 8];
        let length = (spec.crown_radius - 1
            + (hash(seed, base.x + arm as i32, base.z, 107) % 2) as i32)
            .max(2);
        let rise = if arm.is_multiple_of(3) { 2 } else { 1 };
        let end = grow_branch(base, center_y - 1, direction, length, rise, spec.wood, out);
        leaf_cluster(
            seed,
            end,
            (spec.crown_radius / 2 + 1).max(2),
            (spec.crown_height / 2).max(2),
            spec.leaf,
            109 + arm as i32 * 11,
            out,
        );
    }
}

fn ancient_crown(seed: u64, base: VoxelCoord, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    let fork_y = spec.trunk_h * 2 / 3;
    let rotation = (hash(seed, base.x, base.z, 127) % 8) as usize;
    for arm in 0..6usize {
        let direction = BRANCH_DIRECTIONS[(rotation + arm) % 8];
        let length = spec.crown_radius - 1 + i32::from(arm.is_multiple_of(2));
        let end = grow_branch(
            base,
            fork_y + (arm % 3) as i32,
            direction,
            length,
            3 + (arm % 2) as i32,
            spec.wood,
            out,
        );
        leaf_cluster(
            seed,
            end,
            3 + (arm % 2) as i32,
            3,
            spec.leaf,
            131 + arm as i32 * 13,
            out,
        );
    }
    leaf_cluster(
        seed,
        base.offset(0, spec.trunk_h, 0),
        4,
        3,
        spec.leaf,
        197,
        out,
    );
    if spec.vines {
        hang_vines(
            seed,
            base.offset(0, spec.trunk_h + 1, 0),
            spec.crown_radius,
            spec.leaf,
            out,
        );
    }
}

/// High fork and broad, shallow pads give acacias their recognisable
/// umbrella silhouette while avoiding the old perfectly circular plate.
fn acacia_crown(seed: u64, base: VoxelCoord, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    let rotation = (hash(seed, base.x, base.z, 149) % 8) as usize;
    for arm in 0..3usize {
        let direction = BRANCH_DIRECTIONS[(rotation + arm * 3) % 8];
        let end = grow_branch(
            base,
            spec.trunk_h - 1,
            direction,
            spec.crown_radius - 1,
            1,
            spec.wood,
            out,
        );
        flat_leaf_pad(seed, end, 3, spec.leaf, 151 + arm as i32 * 7, out);
    }
    flat_leaf_pad(
        seed,
        base.offset(0, spec.trunk_h, 0),
        3,
        spec.leaf,
        179,
        out,
    );
}

fn willow_crown(seed: u64, base: VoxelCoord, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    branching_crown(seed, base, spec, 5, out);
    let crown_y = spec.trunk_h + 1;
    let r = spec.crown_radius;
    for (index, (dx, dz)) in BRANCH_DIRECTIONS.into_iter().enumerate() {
        let length = 2 + (hash(seed, base.x + dx, base.z + dz, 211) % 3) as i32;
        let anchor = base.offset(dx * (r - 1), crown_y, dz * (r - 1));
        // Bridge the hanging strand back into the crown rim.
        push(out, anchor.offset(-dx, 0, -dz), spec.leaf);
        for drop in 0..length {
            if drop == length - 1 && index.is_multiple_of(3) {
                continue;
            }
            push(out, anchor.offset(0, -drop, 0), spec.leaf);
        }
    }
}

fn birch_crown(seed: u64, base: VoxelCoord, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    let bottom = (spec.trunk_h - spec.crown_height).max(2);
    for (layer, y) in (bottom..=spec.trunk_h).step_by(2).enumerate() {
        let radius = if y == spec.trunk_h {
            1
        } else {
            spec.crown_radius
        };
        leaf_cluster(
            seed,
            base.offset(0, y, 0),
            radius,
            2,
            spec.leaf,
            223 + layer as i32 * 5,
            out,
        );
    }
}

/// Palm crown: a stepped wind-shaped trunk tip and asymmetric drooping fronds.
fn rosette(seed: u64, base: VoxelCoord, top_y: i32, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    let r = spec.crown_radius;
    let lean = match hash(seed, base.x, base.z, 241) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    };
    let trunk_top = top_y - base.y - 1;
    push(out, base.offset(lean.0, trunk_top, lean.1), spec.wood);
    push(out, base.offset(lean.0, trunk_top + 1, lean.1), spec.wood);
    push(
        out,
        base.offset(lean.0 * 2, trunk_top + 1, lean.1 * 2),
        spec.wood,
    );
    let crown = base.offset(lean.0 * 2, trunk_top + 2, lean.1 * 2);
    push(out, crown, spec.leaf);
    push(out, crown.offset(0, 1, 0), spec.leaf);

    for (index, (dx, dz)) in [
        (1i32, 0i32),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ]
    .into_iter()
    .enumerate()
    {
        let length = r - i32::from(
            (hash(seed, base.x + dx, base.z + dz, 251) + index as u32).is_multiple_of(3),
        );
        for step in 1..=length {
            let drop = if step == length {
                2
            } else if step >= (length + 1) / 2 {
                1
            } else {
                0
            };
            push(out, crown.offset(dx * step, -drop, dz * step), spec.leaf);
            if dx != 0 && dz != 0 {
                push(
                    out,
                    crown.offset(dx * step, -drop, dz * (step - 1)),
                    spec.leaf,
                );
            } else if step <= 2 {
                let side = if dx == 0 { (1, 0) } else { (0, 1) };
                push(
                    out,
                    crown.offset(dx * step + side.0, -drop, dz * step + side.1),
                    spec.leaf,
                );
            }
        }
    }
}

fn push(out: &mut Vec<TreeVoxel>, coord: VoxelCoord, block: BlockType) {
    out.push(TreeVoxel { coord, block });
}

fn deduplicate_tree(out: &mut Vec<TreeVoxel>) {
    let mut indices: HashMap<VoxelCoord, usize> = HashMap::with_capacity(out.len());
    let mut unique: Vec<TreeVoxel> = Vec::with_capacity(out.len());
    for voxel in out.drain(..) {
        if let Some(&index) = indices.get(&voxel.coord) {
            if block_priority(voxel.block) > block_priority(unique[index].block) {
                unique[index].block = voxel.block;
            }
        } else {
            indices.insert(voxel.coord, unique.len());
            unique.push(voxel);
        }
    }
    *out = unique;
}

fn block_priority(block: BlockType) -> u8 {
    match block {
        BlockType::Wood | BlockType::BirchWood | BlockType::JungleWood => 3,
        BlockType::Snow => 2,
        _ => 1,
    }
}

fn retain_connected_to_trunk(out: &mut Vec<TreeVoxel>, base: VoxelCoord) {
    let occupied: HashSet<_> = out.iter().map(|voxel| voxel.coord).collect();
    if !occupied.contains(&base) {
        out.clear();
        return;
    }
    let mut connected = HashSet::from([base]);
    let mut queue = VecDeque::from([base]);
    while let Some(coord) = queue.pop_front() {
        for neighbor in face_neighbors(coord) {
            if occupied.contains(&neighbor) && connected.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }
    out.retain(|voxel| connected.contains(&voxel.coord));
}

fn face_neighbors(coord: VoxelCoord) -> [VoxelCoord; 6] {
    [
        coord.offset(1, 0, 0),
        coord.offset(-1, 0, 0),
        coord.offset(0, 1, 0),
        coord.offset(0, -1, 0),
        coord.offset(0, 0, 1),
        coord.offset(0, 0, -1),
    ]
}

/// Builds one ground decoration (rocks, logs, bushes).
pub fn build_ground(
    seed: u64,
    base: VoxelCoord,
    kind: GroundKind,
    bush_leaf: BlockType,
) -> Vec<TreeVoxel> {
    let mut out = Vec::with_capacity(32);
    let stone = match hash(seed, base.x, base.z, 211) % 3 {
        0 => BlockType::MossStone,
        1 => BlockType::Stone,
        _ => BlockType::Gravel,
    };
    match kind {
        GroundKind::Pebble => push(&mut out, base, stone),
        GroundKind::Rock => {
            push(&mut out, base, stone);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if hash(seed, base.x + dx, base.z + dz, 213).is_multiple_of(2) {
                    push(&mut out, base.offset(dx, 0, dz), stone);
                }
            }
            if hash(seed, base.x, base.z, 217).is_multiple_of(3) {
                push(&mut out, base.offset(0, 1, 0), stone);
            }
        }
        GroundKind::Boulder | GroundKind::MegaBoulder => {
            let r = if kind == GroundKind::MegaBoulder {
                3
            } else {
                2
            };
            for dx in -r..=r {
                for dz in -r..=r {
                    for dy in 0..=r {
                        let dist = (dx * dx + dz * dz) as f64 + (dy as f64 * 1.6).powi(2);
                        if dist <= (r as f64 + 0.4).powi(2)
                            && !(dist > (r * r) as f64
                                && hash(seed, base.x + dx * 7, base.z + dz * 7 + dy, 223) % 100
                                    < 35)
                        {
                            push(&mut out, base.offset(dx, dy, dz), stone);
                        }
                    }
                }
            }
        }
        GroundKind::FallenLog => {
            let dir = match hash(seed, base.x, base.z, 181) % 4 {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            let length = 3 + (hash(seed, base.x, base.z, 193) % 4) as i32;
            for i in 0..length {
                let block = if i == 0 || i == length - 1 {
                    BlockType::MossStone
                } else {
                    BlockType::Wood
                };
                push(&mut out, base.offset(dir.0 * i, 0, dir.1 * i), block);
            }
            // Occasional branch stub / regrowth on top of the log.
            let mid = length / 2;
            if hash(seed, base.x, base.z, 227).is_multiple_of(3) {
                push(
                    &mut out,
                    base.offset(dir.0 * mid, 1, dir.1 * mid),
                    bush_leaf,
                );
            }
        }
        GroundKind::Bush => {
            let r = 1 + (hash(seed, base.x, base.z, 233) % 2) as i32;
            for dx in -r..=r {
                for dz in -r..=r {
                    for dy in 0..2 {
                        let dist = (dx * dx + dz * dz) + dy * dy;
                        if dist <= r * r
                            && !hash(seed, base.x + dx, base.z + dz + dy, 239).is_multiple_of(5)
                        {
                            push(&mut out, base.offset(dx, dy, dz), bush_leaf);
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn hash01(seed: u64, x: i32, z: i32, salt: i32) -> f32 {
    (hash(seed, x, z, salt) as f32) / (u32::MAX as f32)
}

pub fn hash(seed: u64, x: i32, z: i32, salt: i32) -> u32 {
    let mut n = seed ^ ((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    n ^= (z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= (salt as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D0_49BB_1331_11EB);
    (n ^ (n >> 31)) as u32
}

/// Hanging vine strands under canopy rims.
fn hang_vines(
    seed: u64,
    rim_center: VoxelCoord,
    radius: i32,
    leaf: BlockType,
    out: &mut Vec<TreeVoxel>,
) {
    for dx in [-radius, -radius / 2, 0, radius / 2, radius] {
        for dz in [-radius, radius] {
            if hash(seed, rim_center.x + dx, rim_center.z + dz, 83).is_multiple_of(3) {
                continue;
            }
            let length = 1 + (hash(seed, rim_center.x + dx, rim_center.z + dz, 89) % 3) as i32;
            for vy in 0..length {
                push(out, rim_center.offset(dx, -vy, dz), leaf);
            }
        }
    }
}

/// A stepped conifer with separated bough tiers. Small gaps between layers
/// keep the outline readable instead of producing one solid pyramid.
fn layered_conifer(seed: u64, base: VoxelCoord, spec: &TreeSpec, out: &mut Vec<TreeVoxel>) {
    let start = 2.max(spec.trunk_h - spec.crown_height);
    let crown_span = (spec.trunk_h - start).max(1);
    for (tier, y) in (start..spec.trunk_h).step_by(2).enumerate() {
        let t = (y - start) as f64 / crown_span as f64;
        let radius = (((1.0 - t) * spec.crown_radius as f64).ceil() as i32).max(1);
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let edge = dx.abs() + dz.abs();
                if edge <= radius + 1
                    && !(edge == radius + 1
                        && hash(seed, base.x + dx, base.z + dz + y, 241) % 100 < 42)
                {
                    push(out, base.offset(dx, y, dz), spec.leaf);
                    if edge <= (radius - 1).max(0) && (dx + dz + tier as i32) % 3 != 0 {
                        push(out, base.offset(dx, y + 1, dz), spec.leaf);
                    }
                }
            }
        }
    }
    push(out, base.offset(0, spec.trunk_h, 0), spec.leaf);
    if spec.kind == TreeKind::PineSnowy {
        for y in (start + 2..spec.trunk_h).step_by(2) {
            let t = (y - start) as f64 / crown_span as f64;
            let radius = (((1.0 - t) * spec.crown_radius as f64).round() as i32).clamp(0, 2);
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    if dx.abs() + dz.abs() <= radius {
                        push(out, base.offset(dx, y, dz), BlockType::Snow);
                    }
                }
            }
        }
    }
}

fn grow_branch(
    base: VoxelCoord,
    start_y: i32,
    direction: (i32, i32),
    length: i32,
    rise: i32,
    wood: BlockType,
    out: &mut Vec<TreeVoxel>,
) -> VoxelCoord {
    let mut current = base.offset(0, start_y, 0);
    push(out, current, wood);
    for step in 1..=length {
        let target_x = direction.0 * step;
        let target_z = direction.1 * step;
        let target_y = start_y + rise * step / length.max(1);
        while current.x != base.x + target_x {
            current.x += (base.x + target_x - current.x).signum();
            push(out, current, wood);
        }
        while current.z != base.z + target_z {
            current.z += (base.z + target_z - current.z).signum();
            push(out, current, wood);
        }
        while current.y != base.y + target_y {
            current.y += (base.y + target_y - current.y).signum();
            push(out, current, wood);
        }
    }
    current
}

fn leaf_cluster(
    seed: u64,
    center: VoxelCoord,
    radius: i32,
    half_height: i32,
    leaf: BlockType,
    salt: i32,
    out: &mut Vec<TreeVoxel>,
) {
    for dy in -half_height..=half_height {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let horizontal = (dx * dx + dz * dz) as f64 / (radius * radius).max(1) as f64;
                let vertical = (dy * dy) as f64 / (half_height * half_height).max(1) as f64;
                let distance = horizontal + vertical;
                if distance > 1.18 {
                    continue;
                }
                let edge_roll =
                    hash(seed, center.x + dx * 5 + dy, center.z + dz * 7 - dy, salt) % 100;
                if distance > 0.70 && edge_roll < 24 {
                    continue;
                }
                push(out, center.offset(dx, dy, dz), leaf);
            }
        }
    }
}

fn flat_leaf_pad(
    seed: u64,
    center: VoxelCoord,
    radius: i32,
    leaf: BlockType,
    salt: i32,
    out: &mut Vec<TreeVoxel>,
) {
    for dy in 0..=1 {
        let layer_radius = radius - dy;
        for dx in -layer_radius..=layer_radius {
            for dz in -layer_radius..=layer_radius {
                let distance = dx * dx + dz * dz;
                if distance <= layer_radius * layer_radius + 1
                    && !(distance > (layer_radius - 1).max(0).pow(2)
                        && hash(seed, center.x + dx, center.z + dz, salt + dy) % 100 < 28)
                {
                    push(out, center.offset(dx, dy, dz), leaf);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tree_archetype_is_face_connected_to_its_trunk() {
        let base = VoxelCoord::new(-17, 29, 43);
        for kind in TreeKind::ALL {
            for seed in [0, 7, 42, 1337] {
                let spec = kind.spec(seed, base.x, base.z);
                let tree = build_tree(seed, base, &spec);
                let occupied: HashSet<_> = tree.iter().map(|voxel| voxel.coord).collect();
                assert!(occupied.contains(&base), "{kind:?} has no trunk base");

                let mut connected = HashSet::from([base]);
                let mut pending = VecDeque::from([base]);
                while let Some(coord) = pending.pop_front() {
                    for neighbor in face_neighbors(coord) {
                        if occupied.contains(&neighbor) && connected.insert(neighbor) {
                            pending.push_back(neighbor);
                        }
                    }
                }
                assert_eq!(
                    connected.len(),
                    occupied.len(),
                    "{kind:?} seed {seed} contains disconnected blocks"
                );
            }
        }
    }

    #[test]
    fn umbrella_crown_is_above_its_trunk() {
        let base = VoxelCoord::new(8, 37, -4);
        let spec = TreeSpec {
            kind: TreeKind::Acacia,
            trunk_h: 5,
            thickness: 1,
            crown_radius: 3,
            crown_height: 2,
            wood: BlockType::Wood,
            leaf: BlockType::AutumnLeaves,
            roots: false,
            vines: false,
        };
        let tree = build_tree(7, base, &spec);
        let lowest_leaf = tree
            .iter()
            .filter(|voxel| voxel.block == spec.leaf)
            .map(|voxel| voxel.coord.y)
            .min()
            .unwrap();
        assert!(lowest_leaf >= base.y + spec.trunk_h);
    }

    #[test]
    fn conifer_crown_does_not_add_world_y_twice() {
        let base = VoxelCoord::new(-3, 41, 9);
        let spec = pine(TreeKind::PineMedium, 7, 3, BlockType::Wood);
        let tree = build_tree(11, base, &spec);
        let highest = tree.iter().map(|voxel| voxel.coord.y).max().unwrap();
        assert_eq!(highest, base.y + spec.trunk_h);
    }

    #[test]
    fn mature_broadleaf_archetypes_grow_structural_branches() {
        let base = VoxelCoord::new(13, 28, -21);
        for kind in [
            TreeKind::Oak,
            TreeKind::TallOak,
            TreeKind::WideOak,
            TreeKind::Giant,
            TreeKind::JungleMedium,
            TreeKind::JungleGiant,
            TreeKind::Acacia,
            TreeKind::SwampWillow,
            TreeKind::AutumnOak,
        ] {
            let spec = kind.spec(42, base.x, base.z);
            let tree = build_tree(42, base, &spec);
            let has_branch = tree.iter().any(|voxel| {
                voxel.block == spec.wood
                    && (voxel.coord.x != base.x || voxel.coord.z != base.z)
                    && voxel.coord.y >= base.y + spec.trunk_h / 2
            });
            assert!(has_branch, "{kind:?} has no horizontal crown structure");
        }
    }

    #[test]
    fn common_tree_silhouettes_are_substantial_and_species_specific() {
        let base = VoxelCoord::new(0, 30, 0);
        let dimensions = |kind: TreeKind| {
            let spec = kind.spec(1337, base.x, base.z);
            let tree = build_tree(1337, base, &spec);
            let min_x = tree.iter().map(|voxel| voxel.coord.x).min().unwrap();
            let max_x = tree.iter().map(|voxel| voxel.coord.x).max().unwrap();
            let min_y = tree.iter().map(|voxel| voxel.coord.y).min().unwrap();
            let max_y = tree.iter().map(|voxel| voxel.coord.y).max().unwrap();
            let min_z = tree.iter().map(|voxel| voxel.coord.z).min().unwrap();
            let max_z = tree.iter().map(|voxel| voxel.coord.z).max().unwrap();
            (max_x - min_x + 1, max_y - min_y + 1, max_z - min_z + 1)
        };

        let oak = dimensions(TreeKind::Oak);
        let birch = dimensions(TreeKind::Birch);
        let pine = dimensions(TreeKind::PineTall);
        let acacia = dimensions(TreeKind::Acacia);
        assert!(oak.0 >= 9 && oak.2 >= 9, "oak crown is undersized: {oak:?}");
        assert!(birch.1 > birch.0, "birch is not tall and narrow: {birch:?}");
        assert!(pine.1 > pine.0, "pine is not vertically tapered: {pine:?}");
        assert!(
            acacia.0 > acacia.1 || acacia.2 > acacia.1,
            "acacia lacks an umbrella silhouette: {acacia:?}"
        );
        assert_ne!(oak, birch, "oak and birch silhouettes collapsed together");
        assert_ne!(oak, pine, "oak and pine silhouettes collapsed together");
    }
}
