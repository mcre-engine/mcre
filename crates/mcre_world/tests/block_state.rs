use mcre_core::{Axis, Direction};
use mcre_world::{Block, BlockState, PropKey, PropVal};

#[test]
fn test_state_empty_props_returns_default() {
    for block in Block::all() {
        assert_eq!(
            block.state(&[]),
            Some(block.default_state()),
            "block {}",
            block.name()
        );
    }
}

#[test]
fn test_state_explicit_props() {
    let state = Block::GRASS_BLOCK.state(&[PropVal::Snowy(true)]).unwrap();
    assert_eq!(state.block(), Block::GRASS_BLOCK);
    assert!(state.is_snowy());

    let state = Block::OAK_LOG.state(&[PropVal::Axis(Axis::X)]).unwrap();
    assert_eq!(state.axis(), Axis::X);

    let state = Block::FURNACE
        .state(&[PropVal::Facing(Direction::East)])
        .unwrap();
    assert_eq!(state.facing(), Direction::East);
    assert!(!state.is_lit());

    let default_log = Block::OAK_LOG.default_state();
    assert_eq!(
        Block::OAK_LOG.state(&[]).unwrap().axis(),
        default_log.axis()
    );
}

#[test]
fn test_state_invalid_props() {
    assert!(
        Block::STONE
            .state(&[PropVal::Facing(Direction::East)])
            .is_none()
    );
    assert!(
        Block::FURNACE
            .state(&[PropVal::Facing(Direction::Up)])
            .is_none()
    );
    assert!(
        Block::FURNACE
            .state(&[
                PropVal::Facing(Direction::East),
                PropVal::Facing(Direction::West),
            ])
            .is_none()
    );
    assert!(Block::FURNACE.state(&[PropVal::Stage(1)]).is_none());
}

#[test]
fn test_state_roundtrip() {
    for block in Block::all() {
        let min: u16 = block.min_state().into();
        let max: u16 = block.max_state().into();
        for id in min..=max {
            let state = BlockState::from(id);
            let props: Vec<PropVal> = PropKey::ALL
                .iter()
                .filter_map(|key| state.get_prop(*key))
                .collect();
            assert_eq!(
                block.state(&props),
                Some(state),
                "{} state {}",
                block.name(),
                id
            );
        }
    }
}
