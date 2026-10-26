use alloc::{string::String, vec::Vec};

use crate::ChunkPos;

#[derive(Debug, Clone, PartialEq)]
pub struct WorldEntity {
    id: String,
    position: [f64; 3],
    nbt: Vec<u8>,
}

impl WorldEntity {
    pub fn new(id: String, position: [f64; 3], nbt: Vec<u8>) -> Self {
        Self { id, position, nbt }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn position(&self) -> [f64; 3] {
        self.position
    }

    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos::from_block(
            block_coordinate(self.position[0]),
            block_coordinate(self.position[2]),
        )
    }

    pub fn nbt(&self) -> &[u8] {
        &self.nbt
    }
}

fn block_coordinate(value: f64) -> i32 {
    let truncated = value as i32;
    if value < truncated as f64 {
        truncated.saturating_sub(1)
    } else {
        truncated
    }
}
