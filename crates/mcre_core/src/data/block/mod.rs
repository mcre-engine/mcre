mod consts;
mod data;
use crate::{FieldKey, StateId};
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(u16);
impl From<u16> for BlockId {
    fn from(id: u16) -> Self {
        Self(id)
    }
}
impl From<BlockId> for u16 {
    fn from(id: BlockId) -> Self {
        id.0
    }
}
impl From<BlockId> for StateId {
    fn from(id: BlockId) -> Self {
        id.default_state_id()
    }
}
impl BlockId {
    pub const MAX: Self = Self(1165u16);
    pub fn name(self) -> &'static str {
        data::name::get(self.0)
    }
    pub fn display_name(self) -> &'static str {
        data::display_name::get(self.0)
    }
    pub fn default_state_id(self) -> StateId {
        data::default_state_id::get(self.0).into()
    }
    pub fn min_state_id(self) -> StateId {
        data::min_state_id::get(self.0).into()
    }
    pub fn max_state_id(self) -> StateId {
        data::max_state_id::get(self.0).into()
    }
    pub fn is_field_present(self, field: FieldKey) -> bool {
        let fields_present = data::fields_present::get(self.0);
        ((fields_present >> (field as u8)) & 1) == 1
    }
    pub fn all() -> impl Iterator<Item = Self> {
        BlockIdIter::new(BlockId(0), Self::MAX)
    }
}
pub struct BlockIdIter {
    current: u16,
    end: u16,
}
impl BlockIdIter {
    pub fn new(start: BlockId, end: BlockId) -> Self {
        Self {
            current: start.0,
            end: end.0,
        }
    }
}
impl Iterator for BlockIdIter {
    type Item = BlockId;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current > self.end {
            None
        } else {
            let id = self.current;
            self.current += 1;
            Some(BlockId(id))
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
impl ExactSizeIterator for BlockIdIter {}
