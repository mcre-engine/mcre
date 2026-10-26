use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEntity {
    id: String,
    position: (i32, i32, i32),
    nbt: Vec<u8>,
}

impl BlockEntity {
    pub fn new(id: String, position: (i32, i32, i32), nbt: Vec<u8>) -> Self {
        Self { id, position, nbt }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn position(&self) -> (i32, i32, i32) {
        self.position
    }

    pub fn nbt(&self) -> &[u8] {
        &self.nbt
    }
}
