mod enums;
use enums::{
    AttachFace, Axis, BambooLeaves, BedPart, BellAttachType, ChestType, ComparatorMode,
    CreakingHeartState, Direction, DoorHingeSide, DoubleBlockHalf, DripstoneThickness, FrontAndTop,
    Half, NoteBlockInstrument, PistonType, Pose, RailShape, RedstoneSide, SculkSensorPhase,
    SideChainPart, SlabType, StairsShape, StructureMode, TestBlockMode, Tilt, TrialSpawnerState,
    VaultState, WallSide,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StateId(u16);

impl StateId {
    pub fn trial_spawner_state(self) -> TrialSpawnerState {
        todo!()
    }

    pub fn axis(self) -> Axis {
        todo!()
    }

    pub fn facing(self) -> Direction {
        todo!()
    }

    pub fn instrument(self) -> NoteBlockInstrument {
        todo!()
    }

    pub fn part(self) -> BedPart {
        todo!()
    }

    pub fn stairs_shape(self) -> StairsShape {
        todo!()
    }

    pub fn half(self) -> Half {
        todo!()
    }

    pub fn slab_type(self) -> SlabType {
        todo!()
    }

    pub fn side_chain(self) -> SideChainPart {
        todo!()
    }

    pub fn creaking_heart_state(self) -> CreakingHeartState {
        todo!()
    }

    pub fn wall_east(self) -> WallSide {
        todo!()
    }

    pub fn wall_north(self) -> WallSide {
        todo!()
    }

    pub fn wall_south(self) -> WallSide {
        todo!()
    }

    pub fn wall_west(self) -> WallSide {
        todo!()
    }

    pub fn hinge(self) -> DoorHingeSide {
        todo!()
    }

    pub fn face(self) -> AttachFace {
        todo!()
    }

    pub fn testblock_mode(self) -> TestBlockMode {
        todo!()
    }

    pub fn leaves(self) -> BambooLeaves {
        todo!()
    }

    pub fn attachment(self) -> BellAttachType {
        todo!()
    }

    pub fn orientation(self) -> FrontAndTop {
        todo!()
    }

    pub fn sculk_sensor_phase(self) -> SculkSensorPhase {
        todo!()
    }

    pub fn copper_golem_pose(self) -> Pose {
        todo!()
    }

    pub fn thickness(self) -> DripstoneThickness {
        todo!()
    }

    pub fn vertical_direction(self) -> Direction {
        todo!()
    }

    pub fn tilt(self) -> Tilt {
        todo!()
    }

    pub fn rail_shape(self) -> RailShape {
        todo!()
    }

    pub fn doubleblock_half(self) -> DoubleBlockHalf {
        todo!()
    }

    pub fn piston_type(self) -> PistonType {
        todo!()
    }

    pub fn chest_type(self) -> ChestType {
        todo!()
    }

    pub fn redstone_east(self) -> RedstoneSide {
        todo!()
    }

    pub fn redstone_north(self) -> RedstoneSide {
        todo!()
    }

    pub fn redstone_south(self) -> RedstoneSide {
        todo!()
    }

    pub fn redstone_west(self) -> RedstoneSide {
        todo!()
    }

    pub fn comparator_mode(self) -> ComparatorMode {
        todo!()
    }

    pub fn structure_mode(self) -> StructureMode {
        todo!()
    }

    pub fn vault_state(self) -> VaultState {
        todo!()
    }
}
