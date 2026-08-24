use std::collections::HashMap;

use crate::world::{
    coordinates::{ChunkCoord, LocalVoxelCoord, SectionCoord, VoxelCoord},
    voxel::BlockType,
    CHUNK_SIZE_USIZE,
};

/// Sparse visual state for runtime water. Generated basin water has no entry
/// and renders as a full source; simulated flow records its horizontal level
/// and whether it belongs to a vertical falling strand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterShape {
    pub level: u8,
    pub falling: bool,
}

/// One vertical 32-block section of a chunk column. Pure block storage:
/// world generation lives in [`crate::world::chunk_manager`].
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Horizontal column coordinate.
    pub coord: ChunkCoord,
    /// Vertical section index (may be negative).
    pub section_y: i32,
    blocks: Vec<BlockType>,
    water_shapes: HashMap<usize, WaterShape>,
    pub dirty: bool,
}

impl Chunk {
    pub fn new(coord: ChunkCoord, section_y: i32) -> Self {
        Self {
            coord,
            section_y,
            blocks: vec![BlockType::Air; CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE * CHUNK_SIZE_USIZE],
            water_shapes: HashMap::new(),
            dirty: true,
        }
    }

    /// The lookup key for this section inside the world map.
    pub fn key(&self) -> SectionCoord {
        SectionCoord::from_column_and_y(self.coord, self.section_y)
    }

    /// Whether `coord` addresses a voxel inside this section.
    pub fn contains(&self, coord: VoxelCoord) -> bool {
        coord.section() == self.key()
    }

    pub fn get_local(&self, local: LocalVoxelCoord) -> BlockType {
        self.blocks[local.index()]
    }

    pub fn set_local(&mut self, local: LocalVoxelCoord, block: BlockType) {
        let index = local.index();
        self.blocks[index] = block;
        if block != BlockType::Water {
            self.water_shapes.remove(&index);
        }
        self.dirty = true;
    }

    pub fn water_shape_local(&self, local: LocalVoxelCoord) -> Option<WaterShape> {
        self.water_shapes.get(&local.index()).copied()
    }

    pub fn set_water_shape_local(&mut self, local: LocalVoxelCoord, shape: WaterShape) -> bool {
        if self.get_local(local) != BlockType::Water {
            return false;
        }
        if self.water_shapes.insert(local.index(), shape) == Some(shape) {
            return false;
        }
        self.dirty = true;
        true
    }

    #[cfg(test)]
    pub fn get_world(&self, coord: VoxelCoord) -> Option<BlockType> {
        if !self.contains(coord) {
            return None;
        }
        Some(self.get_local(coord.section_local()))
    }

    pub fn set_world(&mut self, coord: VoxelCoord, block: BlockType) -> bool {
        if !self.contains(coord) {
            return false;
        }
        self.set_local(coord.section_local(), block);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_placement_and_removal_inside_section() {
        let mut chunk = Chunk::new(ChunkCoord { x: 0, z: 0 }, 1);
        // Section y=1 covers world Y in [32, 64).
        let coord = VoxelCoord { x: 4, y: 52, z: 4 };
        assert!(chunk.set_world(coord, BlockType::Stone));
        assert_eq!(chunk.get_world(coord), Some(BlockType::Stone));
        assert_eq!(
            chunk.get_world(VoxelCoord { x: 4, y: 20, z: 4 }),
            None,
            "voxel belongs to section 0, not this section"
        );
        assert!(chunk.set_world(coord, BlockType::Air));
        assert_eq!(chunk.get_world(coord), Some(BlockType::Air));
    }

    #[test]
    fn negative_sections_store_blocks() {
        let mut chunk = Chunk::new(ChunkCoord { x: -1, z: -1 }, -2);
        // Column (-1,-1), world Y -40 -> section y=-2, local y=24.
        let coord = VoxelCoord {
            x: -5,
            y: -40,
            z: -5,
        };
        assert!(chunk.contains(coord));
        assert!(chunk.set_world(coord, BlockType::Grass));
        assert_eq!(chunk.get_world(coord), Some(BlockType::Grass));
    }
}
