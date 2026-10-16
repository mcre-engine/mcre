use super::NonPalettedSection;
use super::u256::U256;
use crate::{Block, BlockState};

fn air() -> BlockState {
    Block::AIR.default_state()
}

#[derive(Debug, Clone)]
pub struct PalettedSection {
    pub palette: [BlockState; 256],
    pub counts: [u16; 256],
    pub occupied_palette_slots: U256,
    pub blocks: [u8; 4096],
}

impl PalettedSection {
    fn get_palette_idx(&self, block_idx: usize) -> u8 {
        self.blocks[block_idx]
    }

    fn set_palette_idx(&mut self, block_idx: usize, palette_idx: u8) {
        self.blocks[block_idx] = palette_idx;
    }

    fn unset_palette_idx(&mut self, block_idx: usize) {
        let prev_value = self.blocks[block_idx];
        self.counts[prev_value as usize] -= 1;
        if self.counts[prev_value as usize] == 0 {
            self.occupied_palette_slots.clear_bit(prev_value);
        }
    }

    fn update_palette_idx(&mut self, block_idx: usize, palette_idx: u8) {
        if self.blocks[block_idx] == palette_idx {
            return;
        }
        self.unset_palette_idx(block_idx);
        self.counts[palette_idx as usize] += 1;
        self.set_palette_idx(block_idx, palette_idx);
    }

    pub fn set_block(&mut self, block_idx: usize, block: BlockState) -> bool {
        for palette_idx in self.occupied_palette_slots.iter_bits() {
            if self.palette[palette_idx as usize] == block {
                self.update_palette_idx(block_idx, palette_idx);
                return true;
            }
        }

        self.unset_palette_idx(block_idx);

        let Some(new_palette_idx) = self.occupied_palette_slots.first_zero() else {
            let palette_idx = self.blocks[block_idx];
            self.counts[palette_idx as usize] += 1;
            return false;
        };

        self.palette[new_palette_idx as usize] = block;
        self.counts[new_palette_idx as usize] = 1;
        self.occupied_palette_slots.set_bit(new_palette_idx);

        self.set_palette_idx(block_idx, new_palette_idx);

        true
    }

    pub fn get_block(&self, block_idx: usize) -> BlockState {
        let palette_idx = self.get_palette_idx(block_idx);
        self.palette[palette_idx as usize]
    }

    pub fn upgrade(&self) -> NonPalettedSection {
        let mut blocks = [air(); 4096];

        for (idx, &palette_idx) in self.blocks.iter().enumerate() {
            blocks[idx] = self.palette[palette_idx as usize];
        }

        NonPalettedSection {
            blocks,
            dirty: false,
        }
    }

    pub fn from_homogeneous(block: BlockState) -> Self {
        let mut palette = [air(); 256];
        let mut counts = [0; 256];
        let mut occupied_palette_slots = U256::ZERO;
        let blocks = [0; 4096];

        palette[0] = block;
        occupied_palette_slots.set_bit(0);
        counts[0] = 4096;

        Self {
            palette,
            counts,
            occupied_palette_slots,
            blocks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_compaction() {
        let mut section = PalettedSection::from_homogeneous(air());

        assert!(section.set_block(0, BlockState::from(1)));

        assert!(section.occupied_palette_slots.is_set(0));
        assert!(section.occupied_palette_slots.is_set(1));

        assert!(section.set_block(0, air()));

        assert_eq!(section.occupied_palette_slots.count_ones(), 1);
        assert_eq!(section.counts[0], 4096);
    }
}
