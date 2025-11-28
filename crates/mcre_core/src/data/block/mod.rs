mod data;

use crate::StateId;

#[derive(Debug, Copy, Clone, Hash)]
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

impl BlockId {
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
}
