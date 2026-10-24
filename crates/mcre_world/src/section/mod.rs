pub mod non_palette;
pub mod palette;
pub mod u256;

use alloc::boxed::Box;

use non_palette::NonPalettedSection;
use palette::PalettedSection;

use crate::BlockState;

#[derive(Debug, Clone)]
pub enum Section {
    Paletted(Box<PalettedSection>),
    NonPaletted(Box<NonPalettedSection>),
    Homogeneous(BlockState),
}

impl Section {
    pub fn block_idx(pos: (i32, i32, i32)) -> usize {
        let x = pos.0 as usize;
        let y = pos.1 as usize;
        let z = pos.2 as usize;
        y * 256 + z * 16 + x
    }

    pub fn set_block(&mut self, pos: (i32, i32, i32), block: BlockState) {
        let idx = Self::block_idx(pos);
        match self {
            Self::Homogeneous(state) => {
                if *state == block {
                    return;
                }

                let mut section = PalettedSection::from_homogeneous(*state);
                assert!(section.set_block(idx, block));
                *self = Self::Paletted(Box::new(section));
            }
            Self::Paletted(paletted) => {
                if !paletted.set_block(idx, block) {
                    let mut section = paletted.upgrade();
                    section.set_block(idx, block);
                    *self = Self::NonPaletted(Box::new(section));
                } else if paletted.occupied_palette_slots.count_ones() == 1 {
                    let palette_idx = paletted.occupied_palette_slots.first_one().unwrap();
                    let block = paletted.palette[palette_idx as usize];
                    *self = Self::Homogeneous(block);
                }
            }
            Self::NonPaletted(section) => {
                section.set_block(idx, block);
            }
        }
    }

    pub fn get_block(&self, pos: (i32, i32, i32)) -> BlockState {
        let idx = Self::block_idx(pos);
        match self {
            Self::Homogeneous(block) => *block,
            Self::Paletted(paletted) => paletted.get_block(idx),
            Self::NonPaletted(section) => section.get_block(idx),
        }
    }

    pub fn optimize(&mut self) {
        if let Self::NonPaletted(section) = self
            && let Some(section) = section.try_into_paletted()
        {
            *self = Self::Paletted(Box::new(section));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Block;

    fn air() -> BlockState {
        Block::AIR.default_state()
    }

    fn make_pos(i: usize) -> (i32, i32, i32) {
        let y = i / 256;
        let rem = i % 256;
        let z = rem / 16;
        let x = rem % 16;
        (x as i32, y as i32, z as i32)
    }

    #[test]
    fn test_homogeneous_to_paletted() {
        let mut section = Section::Homogeneous(air());

        section.set_block((0, 0, 0), air());
        assert!(matches!(section, Section::Homogeneous(_)));

        section.set_block((0, 0, 0), BlockState::from(1));

        match section {
            Section::Paletted(ref p) => {
                assert_eq!(p.get_block(0), BlockState::from(1));
                assert_eq!(p.get_block(1), air());
            }
            _ => panic!("Should be Paletted"),
        }
    }

    #[test]
    fn test_paletted_overflow_to_non_paletted() {
        let mut section = Section::Homogeneous(air());

        for i in 1..256 {
            section.set_block(make_pos(i), BlockState::from(i as u16));
        }

        assert!(matches!(section, Section::Paletted(_)));

        section.set_block(make_pos(256), BlockState::from(300));

        assert!(matches!(section, Section::NonPaletted(_)));

        assert_eq!(section.get_block(make_pos(1)), BlockState::from(1));
        assert_eq!(section.get_block(make_pos(256)), BlockState::from(300));
    }

    #[test]
    fn test_paletted_demotion_to_homogeneous() {
        let mut section = Section::Homogeneous(air());

        section.set_block((0, 0, 0), BlockState::from(1));
        assert!(matches!(section, Section::Paletted(_)));

        section.set_block((0, 0, 0), air());

        match section {
            Section::Homogeneous(b) => assert_eq!(b, air()),
            _ => panic!("Should have demoted to Homogeneous"),
        }
    }

    #[test]
    fn test_optimize_non_paletted() {
        let mut section = Section::Homogeneous(air());

        for i in 1..=260 {
            section.set_block(make_pos(i), BlockState::from(i as u16));
        }
        assert!(matches!(section, Section::NonPaletted(_)));

        for i in 200..=260 {
            section.set_block(make_pos(i), air());
        }

        assert!(matches!(section, Section::NonPaletted(_)));

        section.optimize();

        assert!(matches!(section, Section::Paletted(_)));
    }
}
