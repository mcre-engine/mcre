use super::palette::PalettedSection;
use super::u256::U256;
use crate::{Block, BlockState};

fn air() -> BlockState {
    Block::AIR.default_state()
}

#[derive(Debug, Clone)]
pub struct NonPalettedSection {
    pub blocks: [BlockState; 4096],
    pub dirty: bool,
}

impl NonPalettedSection {
    pub fn set_block(&mut self, block_idx: usize, block: BlockState) {
        if self.blocks[block_idx] != block {
            self.dirty = true;
        }
        self.blocks[block_idx] = block;
    }

    pub fn get_block(&self, block_idx: usize) -> BlockState {
        self.blocks[block_idx]
    }

    pub fn try_into_paletted(&self) -> Option<PalettedSection> {
        if !self.dirty {
            return None;
        }

        let mut palette = [air(); 256];
        let mut counts = [0u16; 256];
        let mut occupied_palette_slots = U256::ZERO;
        let mut blocks = [0u8; 4096];
        let mut len = 0usize;

        for (i, &block) in self.blocks.iter().enumerate() {
            let mut existing = None;
            for (p, &palette_block) in palette[..len].iter().enumerate() {
                if palette_block == block {
                    existing = Some(p);
                    break;
                }
            }

            let idx = match existing {
                Some(idx) => idx,
                None => {
                    if len > 255 {
                        return None;
                    }
                    palette[len] = block;
                    occupied_palette_slots.set_bit(len as u8);
                    let idx = len;
                    len += 1;
                    idx
                }
            };

            counts[idx] += 1;
            blocks[i] = idx as u8;
        }

        Some(PalettedSection {
            palette,
            counts,
            occupied_palette_slots,
            blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone() -> BlockState {
        Block::STONE.default_state()
    }

    fn dirt() -> BlockState {
        Block::DIRT.default_state()
    }

    fn non_paletted(dirty: bool) -> NonPalettedSection {
        NonPalettedSection {
            blocks: [air(); 4096],
            dirty,
        }
    }

    #[test]
    fn test_not_dirty_cannot_demote() {
        let section = non_paletted(false);
        assert!(section.try_into_paletted().is_none());
    }

    #[test]
    fn test_roundtrip_to_paletted() {
        let mut section = non_paletted(true);
        section.set_block(0, stone());
        section.set_block(1, dirt());

        let paletted = section.try_into_paletted().expect("two unique blocks fit");
        assert_eq!(paletted.get_block(0), stone());
        assert_eq!(paletted.get_block(1), dirt());
        assert_eq!(paletted.get_block(2), air());
        assert_eq!(paletted.occupied_palette_slots.count_ones(), 3);
        let total: u16 = paletted.counts.iter().sum();
        assert_eq!(total, 4096);
        assert_eq!(paletted.counts[0], 1);
        assert_eq!(paletted.counts[1], 1);
        assert_eq!(paletted.counts[2], 4094);
    }

    #[test]
    fn test_too_many_uniques_stay_non_paletted() {
        let mut section = non_paletted(true);
        for i in 0..=256 {
            section.set_block(i, BlockState::from(i as u16));
        }
        assert!(section.try_into_paletted().is_none());
    }
}
