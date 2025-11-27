mod default_state_id;
mod display_name;
mod max_state_id;
mod min_state_id;
mod name;

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
        name::NAME_VALUES[self.0 as usize]
    }

    pub fn display_name(self) -> &'static str {
        display_name::DISPLAY_NAME_VALUES[self.0 as usize]
    }

    pub fn default_state_id(self) -> StateId {
        default_state_id::DEFAULT_STATE_ID_VALUES[self.0 as usize].into()
    }

    pub fn min_state_id(self) -> StateId {
        min_state_id::MIN_STATE_ID_VALUES[self.0 as usize].into()
    }

    pub fn max_state_id(self) -> StateId {
        max_state_id::MAX_STATE_ID_VALUES[self.0 as usize].into()
    }
}
