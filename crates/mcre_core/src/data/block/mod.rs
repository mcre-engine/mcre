mod consts;
mod data;
use crate::{BlockState, FieldKey};
use serde::{Deserialize, Serialize};
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block(u16);
impl From<u16> for Block {
    fn from(id: u16) -> Self {
        Self(id)
    }
}
impl From<Block> for u16 {
    fn from(id: Block) -> Self {
        id.0
    }
}
impl From<Block> for BlockState {
    fn from(id: Block) -> Self {
        id.default_state()
    }
}
impl Block {
    pub const MAX: Self = Self(1165u16);
    pub fn name(self) -> &'static str {
        data::name::get(self.0)
    }
    pub fn display_name(self) -> &'static str {
        data::display_name::get(self.0)
    }
    pub fn default_state(self) -> BlockState {
        data::default_state::get(self.0).into()
    }
    pub fn min_state(self) -> BlockState {
        data::min_state::get(self.0).into()
    }
    pub fn max_state(self) -> BlockState {
        data::max_state::get(self.0).into()
    }
    pub fn is_field_present(self, field: FieldKey) -> bool {
        let fields_present = data::fields_present::get(self.0);
        ((fields_present >> (field as u8)) & 1) == 1
    }
    pub fn all() -> impl Iterator<Item = Self> {
        BlockIter::new(Block(0), Self::MAX)
    }
}
pub struct BlockIter {
    current: u16,
    end: u16,
}
impl BlockIter {
    pub fn new(start: Block, end: Block) -> Self {
        Self {
            current: start.0,
            end: end.0,
        }
    }
}
impl Iterator for BlockIter {
    type Item = Block;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current > self.end {
            None
        } else {
            let id = self.current;
            self.current += 1;
            Some(Block(id))
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = if self.current > self.end {
            0
        } else {
            (self.end - self.current + 1) as usize
        };
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for BlockIter {}
