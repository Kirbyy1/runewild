use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::{CHUNK_SIZE, VOXEL_SIZE_M};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

/// Identifies one vertical 32-block section of a chunk column.
///
/// `x`/`z` are the usual horizontal chunk coordinates while `y` is the
/// section index: the section covers world Y in
/// `[y * CHUNK_SIZE, y * CHUNK_SIZE + CHUNK_SIZE)`. Negative indices are
/// fully supported so the world extends below Y = 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SectionCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectionCoord {
    /// Horizontal column this section belongs to.
    pub fn column(self) -> ChunkCoord {
        ChunkCoord {
            x: self.x,
            z: self.z,
        }
    }

    pub fn from_column_and_y(column: ChunkCoord, section_y: i32) -> Self {
        Self {
            x: column.x,
            y: section_y,
            z: column.z,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct VoxelCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalVoxelCoord {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl VoxelCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Converts a world-space position (metres) to the terrain voxel that
    /// contains it (floor division by the configured voxel size).
    pub fn from_world_pos(pos: Vec3) -> Self {
        Self {
            x: world_axis_to_voxel(pos.x),
            y: world_axis_to_voxel(pos.y),
            z: world_axis_to_voxel(pos.z),
        }
    }

    /// World-space position (metres) of this voxel's lowest corner.
    pub fn world_pos_m(self) -> Vec3 {
        Vec3::new(
            self.x as f32 * VOXEL_SIZE_M,
            self.y as f32 * VOXEL_SIZE_M,
            self.z as f32 * VOXEL_SIZE_M,
        )
    }

    pub fn chunk(self) -> ChunkCoord {
        ChunkCoord {
            x: floor_div(self.x, CHUNK_SIZE),
            z: floor_div(self.z, CHUNK_SIZE),
        }
    }

    /// Vertical section containing this voxel. Works for negative Y.
    pub fn section(self) -> SectionCoord {
        SectionCoord {
            x: floor_div(self.x, CHUNK_SIZE),
            y: floor_div(self.y, CHUNK_SIZE),
            z: floor_div(self.z, CHUNK_SIZE),
        }
    }

    /// Position inside the section returned by [`VoxelCoord::section`],
    /// always within `0..CHUNK_SIZE` (floor modulo, negative-safe).
    pub fn section_local(self) -> LocalVoxelCoord {
        LocalVoxelCoord {
            x: floor_mod(self.x, CHUNK_SIZE) as usize,
            y: floor_mod(self.y, CHUNK_SIZE) as usize,
            z: floor_mod(self.z, CHUNK_SIZE) as usize,
        }
    }

    pub fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            z: self.z + dz,
        }
    }
}

/// Converts one world-space axis in metres to its containing terrain voxel.
/// `floor` is required here: integer casts truncate toward zero and would map
/// small negative positions into the wrong voxel.
pub fn world_axis_to_voxel(metres: f32) -> i32 {
    (metres / VOXEL_SIZE_M).floor() as i32
}

impl LocalVoxelCoord {
    pub fn index(self) -> usize {
        self.x + self.z * CHUNK_SIZE as usize + self.y * CHUNK_SIZE as usize * CHUNK_SIZE as usize
    }
}

pub(crate) fn floor_div(a: i32, b: i32) -> i32 {
    let mut q = a / b;
    let r = a % b;
    if r != 0 && ((r > 0) != (b > 0)) {
        q -= 1;
    }
    q
}

fn floor_mod(a: i32, b: i32) -> i32 {
    a - floor_div(a, b) * b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_chunk_handles_negative_boundaries() {
        assert_eq!(
            VoxelCoord { x: 0, y: 0, z: 0 }.chunk(),
            ChunkCoord { x: 0, z: 0 }
        );
        assert_eq!(
            VoxelCoord { x: 31, y: 0, z: 31 }.chunk(),
            ChunkCoord { x: 0, z: 0 }
        );
        assert_eq!(
            VoxelCoord { x: 32, y: 0, z: 32 }.chunk(),
            ChunkCoord { x: 1, z: 1 }
        );
        assert_eq!(
            VoxelCoord { x: -1, y: 0, z: -1 }.chunk(),
            ChunkCoord { x: -1, z: -1 }
        );
        assert_eq!(
            VoxelCoord {
                x: -32,
                y: 0,
                z: -32
            }
            .chunk(),
            ChunkCoord { x: -1, z: -1 }
        );
        assert_eq!(
            VoxelCoord {
                x: -33,
                y: 0,
                z: -33
            }
            .chunk(),
            ChunkCoord { x: -2, z: -2 }
        );
    }

    #[test]
    fn sections_support_negative_y() {
        let voxel = VoxelCoord { x: 5, y: -1, z: 7 };
        assert_eq!(voxel.section().y, -1);
        assert_eq!(voxel.section_local().y, 31);

        let deep = VoxelCoord { x: 0, y: -33, z: 0 };
        assert_eq!(deep.section().y, -2);
        assert_eq!(deep.section_local().y, 31);

        let high = VoxelCoord { x: 0, y: 64, z: 0 };
        assert_eq!(high.section().y, 2);
        assert_eq!(high.section_local().y, 0);
    }

    #[test]
    fn section_local_wraps_negative_world_positions() {
        assert_eq!(
            VoxelCoord { x: -1, y: 5, z: -1 }.section_local(),
            LocalVoxelCoord { x: 31, y: 5, z: 31 }
        );
    }

    #[test]
    fn local_index_boundaries() {
        assert_eq!(LocalVoxelCoord { x: 0, y: 0, z: 0 }.index(), 0);
        assert_eq!(
            LocalVoxelCoord {
                x: 31,
                y: 31,
                z: 31
            }
            .index(),
            32 * 32 * 32 - 1
        );
    }

    #[test]
    fn metre_positions_map_to_one_metre_voxels_with_floor_semantics() {
        for (metres, expected) in [
            (0.0, 0),
            (0.24, 0),
            (0.25, 0),
            (0.99, 0),
            (1.0, 1),
            (-0.01, -1),
            (-0.25, -1),
            (-0.26, -1),
            (-1.0, -1),
            (-1.01, -2),
        ] {
            assert_eq!(world_axis_to_voxel(metres), expected, "metres={metres}");
        }
    }

    #[test]
    fn section_and_local_coordinates_cover_boundaries() {
        for (voxel, section, local) in [
            (31, 0, 31),
            (32, 1, 0),
            (33, 1, 1),
            (-1, -1, 31),
            (-32, -1, 0),
            (-33, -2, 31),
        ] {
            let coord = VoxelCoord {
                x: voxel,
                y: voxel,
                z: voxel,
            };
            assert_eq!(
                coord.section(),
                SectionCoord {
                    x: section,
                    y: section,
                    z: section
                }
            );
            assert_eq!(
                coord.section_local(),
                LocalVoxelCoord {
                    x: local,
                    y: local,
                    z: local,
                }
            );
        }
    }
}
