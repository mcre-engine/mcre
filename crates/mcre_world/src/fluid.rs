//! Vanilla `FlowingFluid` state mapping used by the fluid renderer
//! (`FluidRenderer` heights, flow vectors and face culling).

use crate::BlockState;

/// Vanilla `FluidRenderer.MAX_FLUID_HEIGHT`: `8/9`, the own height of a
/// source or falling fluid.
pub const MAX_FLUID_HEIGHT: f32 = 0.888_888_9;

/// The two fluids with built-in render models (`FluidStateModelSet`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidKind {
    Water,
    Lava,
}

/// Vanilla `FluidState` subset: fluid type, amount and falling flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidState {
    pub kind: FluidKind,
    /// `FlowingFluid.getAmount` (1..=8; sources and falling levels are 8).
    pub amount: u8,
    pub falling: bool,
}

impl FluidState {
    /// `FlowingFluid.getOwnHeight`: `amount / 9`.
    pub fn own_height(self) -> f32 {
        f32::from(self.amount) / 9.0
    }

    /// `FlowingFluid.isSource` (the fluid-state `SOURCE` property).
    pub fn is_source(self) -> bool {
        !self.falling && self.amount == 8
    }
}

/// `BlockStateBase.getFluidState` for the two fluids we model.
///
/// A `LiquidBlock` state maps its `level` through `LiquidBlock.stateCache`
/// (`0` -> source, `1..=7` -> flowing `8 - level`, `8..=15` -> falling), and
/// any other block with a fluid is waterlogged and holds `getSource()`.
pub fn fluid_state(state: BlockState) -> Option<FluidState> {
    let kind = match state.fluid()? {
        "water" => FluidKind::Water,
        "lava" => FluidKind::Lava,
        _ => return None,
    };
    if state.block().liquid_fluid().is_some() {
        let level = state.level();
        if level == 0 {
            Some(FluidState {
                kind,
                amount: 8,
                falling: false,
            })
        } else if level < 8 {
            Some(FluidState {
                kind,
                amount: 8 - level,
                falling: false,
            })
        } else {
            Some(FluidState {
                kind,
                amount: 8,
                falling: true,
            })
        }
    } else {
        Some(FluidState {
            kind,
            amount: 8,
            falling: false,
        })
    }
}

/// Vanilla `BlockStateBase.isFaceSturdy(level, pos, direction)` for the
/// infinite empty getter: the cached `SupportType.FULL` face-support mask.
pub fn is_face_sturdy(state: BlockState, direction: mcre_core::Direction) -> bool {
    state.face_sturdy() >> direction.index() & 1 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Block;

    #[test]
    fn water_levels_map_through_liquid_block_cache() {
        let cases = [
            (0, 8, false),
            (1, 7, false),
            (4, 4, false),
            (7, 1, false),
            (8, 8, true),
            (12, 8, true),
            (15, 8, true),
        ];
        let range = u16::from(Block::WATER.min_state())..=u16::from(Block::WATER.max_state());
        for (level, amount, falling) in cases {
            let state = range
                .clone()
                .map(BlockState::from)
                .find(|state| state.level() == level)
                .expect("water level state");
            let fluid = fluid_state(state).expect("water fluid");
            assert_eq!(fluid.kind, FluidKind::Water);
            assert_eq!((fluid.amount, fluid.falling), (amount, falling));
            assert_eq!(fluid.is_source(), level == 0);
        }
    }

    #[test]
    fn waterlogged_blocks_hold_a_source() {
        let mut checked = 0;
        for block_id in 0..=u16::from(Block::MAX) {
            let block = Block::from(block_id);
            if block.liquid_fluid().is_some() {
                continue;
            }
            for id in u16::from(block.min_state())..=u16::from(block.max_state()) {
                let state = BlockState::from(id);
                let Some(fluid) = fluid_state(state) else {
                    continue;
                };
                assert!(fluid.is_source(), "state {state:?}");
                assert_eq!(fluid.own_height(), MAX_FLUID_HEIGHT);
                checked += 1;
            }
        }
        assert!(checked > 0, "no waterlogged states found");
    }

    #[test]
    fn lava_maps_like_water() {
        let lava = Block::LAVA.min_state();
        assert_eq!(
            fluid_state(lava).unwrap(),
            FluidState {
                kind: FluidKind::Lava,
                amount: 8,
                falling: false,
            }
        );
    }
}
