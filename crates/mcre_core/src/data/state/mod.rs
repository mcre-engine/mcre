mod data;
mod enums;
use crate::{Block, FieldKey, FieldVal, OffsetType, PropKey, PropVal};
pub use enums::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockState(u16);
impl From<u16> for BlockState {
    fn from(id: u16) -> Self {
        Self(id)
    }
}
impl From<BlockState> for u16 {
    fn from(id: BlockState) -> Self {
        id.0
    }
}
impl BlockState {
    pub const MAX: Self = Self(29670u16);
    pub fn block(self) -> Block {
        data::block::get(self.0).into()
    }
    pub fn light_emission(self) -> u8 {
        data::light_emission::get(self.0)
    }
    pub fn use_shape_for_light_occlusion(self) -> bool {
        data::use_shape_for_light_occlusion::get(self.0)
    }
    pub fn propagates_skylight_down(self) -> bool {
        data::propagates_skylight_down::get(self.0)
    }
    pub fn light_block(self) -> u8 {
        data::light_block::get(self.0)
    }
    pub fn solid_render(self) -> bool {
        data::solid_render::get(self.0)
    }
    pub fn is_air(self) -> bool {
        data::is_air::get(self.0)
    }
    pub fn ignited_by_lava(self) -> bool {
        data::ignited_by_lava::get(self.0)
    }
    pub fn can_occlude(self) -> bool {
        data::can_occlude::get(self.0)
    }
    pub fn is_randomly_ticking(self) -> bool {
        data::is_randomly_ticking::get(self.0)
    }
    pub fn replaceable(self) -> bool {
        data::replaceable::get(self.0)
    }
    pub fn spawn_terrain_particles(self) -> bool {
        data::spawn_terrain_particles::get(self.0)
    }
    pub fn requires_correct_tool_for_drops(self) -> bool {
        data::requires_correct_tool_for_drops::get(self.0)
    }
    pub fn destroy_speed(self) -> f32 {
        data::destroy_speed::get(self.0)
    }
    pub fn offset_type(self) -> OffsetType {
        unsafe { core::mem::transmute::<u8, OffsetType>(data::offset_type::get(self.0)) }
    }
    pub fn max_horizontal_offset(self) -> f32 {
        data::max_horizontal_offset::get(self.0)
    }
    pub fn max_vertical_offset(self) -> f32 {
        data::max_vertical_offset::get(self.0)
    }
    pub fn get_field(self, field: FieldKey) -> Option<FieldVal> {
        if !self.block().is_field_present(field) {
            return None;
        }
        match field {
            FieldKey::IsSnowy => Some(FieldVal::IsSnowy(self.is_snowy())),
            FieldKey::Axis => Some(FieldVal::Axis(self.axis())),
            FieldKey::Stage => Some(FieldVal::Stage(self.stage())),
            FieldKey::Age => Some(FieldVal::Age(self.age())),
            FieldKey::IsHanging => Some(FieldVal::IsHanging(self.is_hanging())),
            FieldKey::IsWaterlogged => Some(FieldVal::IsWaterlogged(self.is_waterlogged())),
            FieldKey::Level => Some(FieldVal::Level(self.level())),
            FieldKey::Dusted => Some(FieldVal::Dusted(self.dusted())),
            FieldKey::Distance => Some(FieldVal::Distance(self.distance())),
            FieldKey::IsPersistent => Some(FieldVal::IsPersistent(self.is_persistent())),
            FieldKey::Facing => Some(FieldVal::Facing(self.facing())),
            FieldKey::IsTriggered => Some(FieldVal::IsTriggered(self.is_triggered())),
            FieldKey::Instrument => Some(FieldVal::Instrument(self.instrument())),
            FieldKey::Note => Some(FieldVal::Note(self.note())),
            FieldKey::IsPowered => Some(FieldVal::IsPowered(self.is_powered())),
            FieldKey::IsOccupied => Some(FieldVal::IsOccupied(self.is_occupied())),
            FieldKey::Part => Some(FieldVal::Part(self.part())),
            FieldKey::IsExtended => Some(FieldVal::IsExtended(self.is_extended())),
            FieldKey::IsShort => Some(FieldVal::IsShort(self.is_short())),
            FieldKey::IsUnstable => Some(FieldVal::IsUnstable(self.is_unstable())),
            FieldKey::IsSlot0Occupied => Some(FieldVal::IsSlot0Occupied(self.is_slot_0_occupied())),
            FieldKey::IsSlot1Occupied => Some(FieldVal::IsSlot1Occupied(self.is_slot_1_occupied())),
            FieldKey::IsSlot2Occupied => Some(FieldVal::IsSlot2Occupied(self.is_slot_2_occupied())),
            FieldKey::IsSlot3Occupied => Some(FieldVal::IsSlot3Occupied(self.is_slot_3_occupied())),
            FieldKey::IsSlot4Occupied => Some(FieldVal::IsSlot4Occupied(self.is_slot_4_occupied())),
            FieldKey::IsSlot5Occupied => Some(FieldVal::IsSlot5Occupied(self.is_slot_5_occupied())),
            FieldKey::SideChain => Some(FieldVal::SideChain(self.side_chain())),
            FieldKey::IsEast => Some(FieldVal::IsEast(self.is_east())),
            FieldKey::IsNorth => Some(FieldVal::IsNorth(self.is_north())),
            FieldKey::IsSouth => Some(FieldVal::IsSouth(self.is_south())),
            FieldKey::IsUp => Some(FieldVal::IsUp(self.is_up())),
            FieldKey::IsWest => Some(FieldVal::IsWest(self.is_west())),
            FieldKey::CreakingHeartState => {
                Some(FieldVal::CreakingHeartState(self.creaking_heart_state()))
            }
            FieldKey::IsNatural => Some(FieldVal::IsNatural(self.is_natural())),
            FieldKey::Power => Some(FieldVal::Power(self.power())),
            FieldKey::Moisture => Some(FieldVal::Moisture(self.moisture())),
            FieldKey::IsLit => Some(FieldVal::IsLit(self.is_lit())),
            FieldKey::Rotation => Some(FieldVal::Rotation(self.rotation())),
            FieldKey::Hinge => Some(FieldVal::Hinge(self.hinge())),
            FieldKey::IsOpen => Some(FieldVal::IsOpen(self.is_open())),
            FieldKey::IsAttached => Some(FieldVal::IsAttached(self.is_attached())),
            FieldKey::Face => Some(FieldVal::Face(self.face())),
            FieldKey::Layers => Some(FieldVal::Layers(self.layers())),
            FieldKey::IsHasRecord => Some(FieldVal::IsHasRecord(self.is_has_record())),
            FieldKey::Bites => Some(FieldVal::Bites(self.bites())),
            FieldKey::Delay => Some(FieldVal::Delay(self.delay())),
            FieldKey::IsLocked => Some(FieldVal::IsLocked(self.is_locked())),
            FieldKey::IsDown => Some(FieldVal::IsDown(self.is_down())),
            FieldKey::IsInWall => Some(FieldVal::IsInWall(self.is_in_wall())),
            FieldKey::IsHasBottle0 => Some(FieldVal::IsHasBottle0(self.is_has_bottle_0())),
            FieldKey::IsHasBottle1 => Some(FieldVal::IsHasBottle1(self.is_has_bottle_1())),
            FieldKey::IsHasBottle2 => Some(FieldVal::IsHasBottle2(self.is_has_bottle_2())),
            FieldKey::IsEye => Some(FieldVal::IsEye(self.is_eye())),
            FieldKey::IsDisarmed => Some(FieldVal::IsDisarmed(self.is_disarmed())),
            FieldKey::IsConditional => Some(FieldVal::IsConditional(self.is_conditional())),
            FieldKey::IsInverted => Some(FieldVal::IsInverted(self.is_inverted())),
            FieldKey::IsEnabled => Some(FieldVal::IsEnabled(self.is_enabled())),
            FieldKey::Eggs => Some(FieldVal::Eggs(self.eggs())),
            FieldKey::Hatch => Some(FieldVal::Hatch(self.hatch())),
            FieldKey::Hydration => Some(FieldVal::Hydration(self.hydration())),
            FieldKey::Pickles => Some(FieldVal::Pickles(self.pickles())),
            FieldKey::Leaves => Some(FieldVal::Leaves(self.leaves())),
            FieldKey::IsDrag => Some(FieldVal::IsDrag(self.is_drag())),
            FieldKey::IsBottom => Some(FieldVal::IsBottom(self.is_bottom())),
            FieldKey::IsHasBook => Some(FieldVal::IsHasBook(self.is_has_book())),
            FieldKey::Attachment => Some(FieldVal::Attachment(self.attachment())),
            FieldKey::IsSignalFire => Some(FieldVal::IsSignalFire(self.is_signal_fire())),
            FieldKey::Orientation => Some(FieldVal::Orientation(self.orientation())),
            FieldKey::HoneyLevel => Some(FieldVal::HoneyLevel(self.honey_level())),
            FieldKey::Charges => Some(FieldVal::Charges(self.charges())),
            FieldKey::Candles => Some(FieldVal::Candles(self.candles())),
            FieldKey::SculkSensorPhase => {
                Some(FieldVal::SculkSensorPhase(self.sculk_sensor_phase()))
            }
            FieldKey::IsBloom => Some(FieldVal::IsBloom(self.is_bloom())),
            FieldKey::IsCanSummon => Some(FieldVal::IsCanSummon(self.is_can_summon())),
            FieldKey::IsShrieking => Some(FieldVal::IsShrieking(self.is_shrieking())),
            FieldKey::CopperGolemPose => Some(FieldVal::CopperGolemPose(self.copper_golem_pose())),
            FieldKey::Thickness => Some(FieldVal::Thickness(self.thickness())),
            FieldKey::VerticalDirection => {
                Some(FieldVal::VerticalDirection(self.vertical_direction()))
            }
            FieldKey::IsBerries => Some(FieldVal::IsBerries(self.is_berries())),
            FieldKey::FlowerAmount => Some(FieldVal::FlowerAmount(self.flower_amount())),
            FieldKey::SegmentAmount => Some(FieldVal::SegmentAmount(self.segment_amount())),
            FieldKey::Tilt => Some(FieldVal::Tilt(self.tilt())),
            FieldKey::IsCracked => Some(FieldVal::IsCracked(self.is_cracked())),
            FieldKey::IsCrafting => Some(FieldVal::IsCrafting(self.is_crafting())),
            FieldKey::IsOminous => Some(FieldVal::IsOminous(self.is_ominous())),
            FieldKey::TrialSpawnerState => {
                Some(FieldVal::TrialSpawnerState(self.trial_spawner_state()))
            }
            FieldKey::VaultState => Some(FieldVal::VaultState(self.vault_state())),
            FieldKey::IsTip => Some(FieldVal::IsTip(self.is_tip())),
            FieldKey::RailShape => Some(FieldVal::RailShape(self.rail_shape())),
            FieldKey::StairsShape => Some(FieldVal::StairsShape(self.stairs_shape())),
            FieldKey::DoubleblockHalf => Some(FieldVal::DoubleblockHalf(self.doubleblock_half())),
            FieldKey::Half => Some(FieldVal::Half(self.half())),
            FieldKey::PistonType => Some(FieldVal::PistonType(self.piston_type())),
            FieldKey::ChestType => Some(FieldVal::ChestType(self.chest_type())),
            FieldKey::SlabType => Some(FieldVal::SlabType(self.slab_type())),
            FieldKey::RedstoneEast => Some(FieldVal::RedstoneEast(self.redstone_east())),
            FieldKey::WallEast => Some(FieldVal::WallEast(self.wall_east())),
            FieldKey::RedstoneNorth => Some(FieldVal::RedstoneNorth(self.redstone_north())),
            FieldKey::WallNorth => Some(FieldVal::WallNorth(self.wall_north())),
            FieldKey::RedstoneSouth => Some(FieldVal::RedstoneSouth(self.redstone_south())),
            FieldKey::WallSouth => Some(FieldVal::WallSouth(self.wall_south())),
            FieldKey::RedstoneWest => Some(FieldVal::RedstoneWest(self.redstone_west())),
            FieldKey::WallWest => Some(FieldVal::WallWest(self.wall_west())),
            FieldKey::ComparatorMode => Some(FieldVal::ComparatorMode(self.comparator_mode())),
            FieldKey::StructureMode => Some(FieldVal::StructureMode(self.structure_mode())),
            FieldKey::TestblockMode => Some(FieldVal::TestblockMode(self.testblock_mode())),
        }
    }
    pub fn get_prop(self, prop: PropKey) -> Option<PropVal> {
        match prop {
            PropKey::Snowy => self
                .block()
                .is_field_present(FieldKey::IsSnowy)
                .then_some(PropVal::Snowy(self.is_snowy())),
            PropKey::Axis => self
                .block()
                .is_field_present(FieldKey::Axis)
                .then_some(PropVal::Axis(self.axis())),
            PropKey::Stage => self
                .block()
                .is_field_present(FieldKey::Stage)
                .then_some(PropVal::Stage(self.stage())),
            PropKey::Age => self
                .block()
                .is_field_present(FieldKey::Age)
                .then_some(PropVal::Age(self.age())),
            PropKey::Hanging => self
                .block()
                .is_field_present(FieldKey::IsHanging)
                .then_some(PropVal::Hanging(self.is_hanging())),
            PropKey::Waterlogged => self
                .block()
                .is_field_present(FieldKey::IsWaterlogged)
                .then_some(PropVal::Waterlogged(self.is_waterlogged())),
            PropKey::Level => self
                .block()
                .is_field_present(FieldKey::Level)
                .then_some(PropVal::Level(self.level())),
            PropKey::Dusted => self
                .block()
                .is_field_present(FieldKey::Dusted)
                .then_some(PropVal::Dusted(self.dusted())),
            PropKey::Distance => self
                .block()
                .is_field_present(FieldKey::Distance)
                .then_some(PropVal::Distance(self.distance())),
            PropKey::Persistent => self
                .block()
                .is_field_present(FieldKey::IsPersistent)
                .then_some(PropVal::Persistent(self.is_persistent())),
            PropKey::Facing => self
                .block()
                .is_field_present(FieldKey::Facing)
                .then_some(PropVal::Facing(self.facing())),
            PropKey::Triggered => self
                .block()
                .is_field_present(FieldKey::IsTriggered)
                .then_some(PropVal::Triggered(self.is_triggered())),
            PropKey::Instrument => self
                .block()
                .is_field_present(FieldKey::Instrument)
                .then_some(PropVal::Instrument(self.instrument())),
            PropKey::Note => self
                .block()
                .is_field_present(FieldKey::Note)
                .then_some(PropVal::Note(self.note())),
            PropKey::Powered => self
                .block()
                .is_field_present(FieldKey::IsPowered)
                .then_some(PropVal::Powered(self.is_powered())),
            PropKey::Occupied => self
                .block()
                .is_field_present(FieldKey::IsOccupied)
                .then_some(PropVal::Occupied(self.is_occupied())),
            PropKey::Part => self
                .block()
                .is_field_present(FieldKey::Part)
                .then_some(PropVal::Part(self.part())),
            PropKey::Shape => {
                if self.block().is_field_present(FieldKey::RailShape) {
                    Some(PropVal::Shape(self.rail_shape().into()))
                } else if self.block().is_field_present(FieldKey::StairsShape) {
                    Some(PropVal::Shape(self.stairs_shape().into()))
                } else {
                    None
                }
            }
            PropKey::Extended => self
                .block()
                .is_field_present(FieldKey::IsExtended)
                .then_some(PropVal::Extended(self.is_extended())),
            PropKey::Half => {
                if self.block().is_field_present(FieldKey::DoubleblockHalf) {
                    Some(PropVal::Half(self.doubleblock_half().into()))
                } else if self.block().is_field_present(FieldKey::Half) {
                    Some(PropVal::Half(self.half().into()))
                } else {
                    None
                }
            }
            PropKey::Short => self
                .block()
                .is_field_present(FieldKey::IsShort)
                .then_some(PropVal::Short(self.is_short())),
            PropKey::Type => {
                if self.block().is_field_present(FieldKey::PistonType) {
                    Some(PropVal::Type(self.piston_type().into()))
                } else if self.block().is_field_present(FieldKey::ChestType) {
                    Some(PropVal::Type(self.chest_type().into()))
                } else if self.block().is_field_present(FieldKey::SlabType) {
                    Some(PropVal::Type(self.slab_type().into()))
                } else {
                    None
                }
            }
            PropKey::Unstable => self
                .block()
                .is_field_present(FieldKey::IsUnstable)
                .then_some(PropVal::Unstable(self.is_unstable())),
            PropKey::Slot0Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot0Occupied)
                .then_some(PropVal::Slot0Occupied(self.is_slot_0_occupied())),
            PropKey::Slot1Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot1Occupied)
                .then_some(PropVal::Slot1Occupied(self.is_slot_1_occupied())),
            PropKey::Slot2Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot2Occupied)
                .then_some(PropVal::Slot2Occupied(self.is_slot_2_occupied())),
            PropKey::Slot3Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot3Occupied)
                .then_some(PropVal::Slot3Occupied(self.is_slot_3_occupied())),
            PropKey::Slot4Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot4Occupied)
                .then_some(PropVal::Slot4Occupied(self.is_slot_4_occupied())),
            PropKey::Slot5Occupied => self
                .block()
                .is_field_present(FieldKey::IsSlot5Occupied)
                .then_some(PropVal::Slot5Occupied(self.is_slot_5_occupied())),
            PropKey::SideChain => self
                .block()
                .is_field_present(FieldKey::SideChain)
                .then_some(PropVal::SideChain(self.side_chain())),
            PropKey::East => {
                if self.block().is_field_present(FieldKey::IsEast) {
                    Some(PropVal::East(self.is_east().into()))
                } else if self.block().is_field_present(FieldKey::RedstoneEast) {
                    Some(PropVal::East(self.redstone_east().into()))
                } else if self.block().is_field_present(FieldKey::WallEast) {
                    Some(PropVal::East(self.wall_east().into()))
                } else {
                    None
                }
            }
            PropKey::North => {
                if self.block().is_field_present(FieldKey::IsNorth) {
                    Some(PropVal::North(self.is_north().into()))
                } else if self.block().is_field_present(FieldKey::RedstoneNorth) {
                    Some(PropVal::North(self.redstone_north().into()))
                } else if self.block().is_field_present(FieldKey::WallNorth) {
                    Some(PropVal::North(self.wall_north().into()))
                } else {
                    None
                }
            }
            PropKey::South => {
                if self.block().is_field_present(FieldKey::IsSouth) {
                    Some(PropVal::South(self.is_south().into()))
                } else if self.block().is_field_present(FieldKey::RedstoneSouth) {
                    Some(PropVal::South(self.redstone_south().into()))
                } else if self.block().is_field_present(FieldKey::WallSouth) {
                    Some(PropVal::South(self.wall_south().into()))
                } else {
                    None
                }
            }
            PropKey::Up => self
                .block()
                .is_field_present(FieldKey::IsUp)
                .then_some(PropVal::Up(self.is_up())),
            PropKey::West => {
                if self.block().is_field_present(FieldKey::IsWest) {
                    Some(PropVal::West(self.is_west().into()))
                } else if self.block().is_field_present(FieldKey::RedstoneWest) {
                    Some(PropVal::West(self.redstone_west().into()))
                } else if self.block().is_field_present(FieldKey::WallWest) {
                    Some(PropVal::West(self.wall_west().into()))
                } else {
                    None
                }
            }
            PropKey::CreakingHeartState => self
                .block()
                .is_field_present(FieldKey::CreakingHeartState)
                .then_some(PropVal::CreakingHeartState(self.creaking_heart_state())),
            PropKey::Natural => self
                .block()
                .is_field_present(FieldKey::IsNatural)
                .then_some(PropVal::Natural(self.is_natural())),
            PropKey::Power => self
                .block()
                .is_field_present(FieldKey::Power)
                .then_some(PropVal::Power(self.power())),
            PropKey::Moisture => self
                .block()
                .is_field_present(FieldKey::Moisture)
                .then_some(PropVal::Moisture(self.moisture())),
            PropKey::Lit => self
                .block()
                .is_field_present(FieldKey::IsLit)
                .then_some(PropVal::Lit(self.is_lit())),
            PropKey::Rotation => self
                .block()
                .is_field_present(FieldKey::Rotation)
                .then_some(PropVal::Rotation(self.rotation())),
            PropKey::Hinge => self
                .block()
                .is_field_present(FieldKey::Hinge)
                .then_some(PropVal::Hinge(self.hinge())),
            PropKey::Open => self
                .block()
                .is_field_present(FieldKey::IsOpen)
                .then_some(PropVal::Open(self.is_open())),
            PropKey::Attached => self
                .block()
                .is_field_present(FieldKey::IsAttached)
                .then_some(PropVal::Attached(self.is_attached())),
            PropKey::Face => self
                .block()
                .is_field_present(FieldKey::Face)
                .then_some(PropVal::Face(self.face())),
            PropKey::Layers => self
                .block()
                .is_field_present(FieldKey::Layers)
                .then_some(PropVal::Layers(self.layers())),
            PropKey::HasRecord => self
                .block()
                .is_field_present(FieldKey::IsHasRecord)
                .then_some(PropVal::HasRecord(self.is_has_record())),
            PropKey::Bites => self
                .block()
                .is_field_present(FieldKey::Bites)
                .then_some(PropVal::Bites(self.bites())),
            PropKey::Delay => self
                .block()
                .is_field_present(FieldKey::Delay)
                .then_some(PropVal::Delay(self.delay())),
            PropKey::Locked => self
                .block()
                .is_field_present(FieldKey::IsLocked)
                .then_some(PropVal::Locked(self.is_locked())),
            PropKey::Down => self
                .block()
                .is_field_present(FieldKey::IsDown)
                .then_some(PropVal::Down(self.is_down())),
            PropKey::InWall => self
                .block()
                .is_field_present(FieldKey::IsInWall)
                .then_some(PropVal::InWall(self.is_in_wall())),
            PropKey::HasBottle0 => self
                .block()
                .is_field_present(FieldKey::IsHasBottle0)
                .then_some(PropVal::HasBottle0(self.is_has_bottle_0())),
            PropKey::HasBottle1 => self
                .block()
                .is_field_present(FieldKey::IsHasBottle1)
                .then_some(PropVal::HasBottle1(self.is_has_bottle_1())),
            PropKey::HasBottle2 => self
                .block()
                .is_field_present(FieldKey::IsHasBottle2)
                .then_some(PropVal::HasBottle2(self.is_has_bottle_2())),
            PropKey::Eye => self
                .block()
                .is_field_present(FieldKey::IsEye)
                .then_some(PropVal::Eye(self.is_eye())),
            PropKey::Disarmed => self
                .block()
                .is_field_present(FieldKey::IsDisarmed)
                .then_some(PropVal::Disarmed(self.is_disarmed())),
            PropKey::Conditional => self
                .block()
                .is_field_present(FieldKey::IsConditional)
                .then_some(PropVal::Conditional(self.is_conditional())),
            PropKey::Mode => {
                if self.block().is_field_present(FieldKey::ComparatorMode) {
                    Some(PropVal::Mode(self.comparator_mode().into()))
                } else if self.block().is_field_present(FieldKey::StructureMode) {
                    Some(PropVal::Mode(self.structure_mode().into()))
                } else if self.block().is_field_present(FieldKey::TestblockMode) {
                    Some(PropVal::Mode(self.testblock_mode().into()))
                } else {
                    None
                }
            }
            PropKey::Inverted => self
                .block()
                .is_field_present(FieldKey::IsInverted)
                .then_some(PropVal::Inverted(self.is_inverted())),
            PropKey::Enabled => self
                .block()
                .is_field_present(FieldKey::IsEnabled)
                .then_some(PropVal::Enabled(self.is_enabled())),
            PropKey::Eggs => self
                .block()
                .is_field_present(FieldKey::Eggs)
                .then_some(PropVal::Eggs(self.eggs())),
            PropKey::Hatch => self
                .block()
                .is_field_present(FieldKey::Hatch)
                .then_some(PropVal::Hatch(self.hatch())),
            PropKey::Hydration => self
                .block()
                .is_field_present(FieldKey::Hydration)
                .then_some(PropVal::Hydration(self.hydration())),
            PropKey::Pickles => self
                .block()
                .is_field_present(FieldKey::Pickles)
                .then_some(PropVal::Pickles(self.pickles())),
            PropKey::Leaves => self
                .block()
                .is_field_present(FieldKey::Leaves)
                .then_some(PropVal::Leaves(self.leaves())),
            PropKey::Drag => self
                .block()
                .is_field_present(FieldKey::IsDrag)
                .then_some(PropVal::Drag(self.is_drag())),
            PropKey::Bottom => self
                .block()
                .is_field_present(FieldKey::IsBottom)
                .then_some(PropVal::Bottom(self.is_bottom())),
            PropKey::HasBook => self
                .block()
                .is_field_present(FieldKey::IsHasBook)
                .then_some(PropVal::HasBook(self.is_has_book())),
            PropKey::Attachment => self
                .block()
                .is_field_present(FieldKey::Attachment)
                .then_some(PropVal::Attachment(self.attachment())),
            PropKey::SignalFire => self
                .block()
                .is_field_present(FieldKey::IsSignalFire)
                .then_some(PropVal::SignalFire(self.is_signal_fire())),
            PropKey::Orientation => self
                .block()
                .is_field_present(FieldKey::Orientation)
                .then_some(PropVal::Orientation(self.orientation())),
            PropKey::HoneyLevel => self
                .block()
                .is_field_present(FieldKey::HoneyLevel)
                .then_some(PropVal::HoneyLevel(self.honey_level())),
            PropKey::Charges => self
                .block()
                .is_field_present(FieldKey::Charges)
                .then_some(PropVal::Charges(self.charges())),
            PropKey::Candles => self
                .block()
                .is_field_present(FieldKey::Candles)
                .then_some(PropVal::Candles(self.candles())),
            PropKey::SculkSensorPhase => self
                .block()
                .is_field_present(FieldKey::SculkSensorPhase)
                .then_some(PropVal::SculkSensorPhase(self.sculk_sensor_phase())),
            PropKey::Bloom => self
                .block()
                .is_field_present(FieldKey::IsBloom)
                .then_some(PropVal::Bloom(self.is_bloom())),
            PropKey::CanSummon => self
                .block()
                .is_field_present(FieldKey::IsCanSummon)
                .then_some(PropVal::CanSummon(self.is_can_summon())),
            PropKey::Shrieking => self
                .block()
                .is_field_present(FieldKey::IsShrieking)
                .then_some(PropVal::Shrieking(self.is_shrieking())),
            PropKey::CopperGolemPose => self
                .block()
                .is_field_present(FieldKey::CopperGolemPose)
                .then_some(PropVal::CopperGolemPose(self.copper_golem_pose())),
            PropKey::Thickness => self
                .block()
                .is_field_present(FieldKey::Thickness)
                .then_some(PropVal::Thickness(self.thickness())),
            PropKey::VerticalDirection => self
                .block()
                .is_field_present(FieldKey::VerticalDirection)
                .then_some(PropVal::VerticalDirection(self.vertical_direction())),
            PropKey::Berries => self
                .block()
                .is_field_present(FieldKey::IsBerries)
                .then_some(PropVal::Berries(self.is_berries())),
            PropKey::FlowerAmount => self
                .block()
                .is_field_present(FieldKey::FlowerAmount)
                .then_some(PropVal::FlowerAmount(self.flower_amount())),
            PropKey::SegmentAmount => self
                .block()
                .is_field_present(FieldKey::SegmentAmount)
                .then_some(PropVal::SegmentAmount(self.segment_amount())),
            PropKey::Tilt => self
                .block()
                .is_field_present(FieldKey::Tilt)
                .then_some(PropVal::Tilt(self.tilt())),
            PropKey::Cracked => self
                .block()
                .is_field_present(FieldKey::IsCracked)
                .then_some(PropVal::Cracked(self.is_cracked())),
            PropKey::Crafting => self
                .block()
                .is_field_present(FieldKey::IsCrafting)
                .then_some(PropVal::Crafting(self.is_crafting())),
            PropKey::Ominous => self
                .block()
                .is_field_present(FieldKey::IsOminous)
                .then_some(PropVal::Ominous(self.is_ominous())),
            PropKey::TrialSpawnerState => self
                .block()
                .is_field_present(FieldKey::TrialSpawnerState)
                .then_some(PropVal::TrialSpawnerState(self.trial_spawner_state())),
            PropKey::VaultState => self
                .block()
                .is_field_present(FieldKey::VaultState)
                .then_some(PropVal::VaultState(self.vault_state())),
            PropKey::Tip => self
                .block()
                .is_field_present(FieldKey::IsTip)
                .then_some(PropVal::Tip(self.is_tip())),
        }
    }
    pub fn all() -> impl Iterator<Item = Self> {
        BlockStateIter::new(BlockState(0), Self::MAX)
    }
    pub fn is_snowy(self) -> bool {
        data::fields::is_snowy::get(self.0)
    }
    pub fn axis(self) -> Axis {
        unsafe { core::mem::transmute::<u8, Axis>(data::fields::axis::get(self.0)) }
    }
    pub fn stage(self) -> u8 {
        data::fields::stage::get(self.0)
    }
    pub fn age(self) -> u8 {
        data::fields::age::get(self.0)
    }
    pub fn is_hanging(self) -> bool {
        data::fields::is_hanging::get(self.0)
    }
    pub fn is_waterlogged(self) -> bool {
        data::fields::is_waterlogged::get(self.0)
    }
    pub fn level(self) -> u8 {
        data::fields::level::get(self.0)
    }
    pub fn dusted(self) -> u8 {
        data::fields::dusted::get(self.0)
    }
    pub fn distance(self) -> u8 {
        data::fields::distance::get(self.0)
    }
    pub fn is_persistent(self) -> bool {
        data::fields::is_persistent::get(self.0)
    }
    pub fn facing(self) -> Direction {
        unsafe { core::mem::transmute::<u8, Direction>(data::fields::facing::get(self.0)) }
    }
    pub fn is_triggered(self) -> bool {
        data::fields::is_triggered::get(self.0)
    }
    pub fn instrument(self) -> NoteBlockInstrument {
        unsafe {
            core::mem::transmute::<u8, NoteBlockInstrument>(data::fields::instrument::get(self.0))
        }
    }
    pub fn note(self) -> u8 {
        data::fields::note::get(self.0)
    }
    pub fn is_powered(self) -> bool {
        data::fields::is_powered::get(self.0)
    }
    pub fn is_occupied(self) -> bool {
        data::fields::is_occupied::get(self.0)
    }
    pub fn part(self) -> BedPart {
        unsafe { core::mem::transmute::<u8, BedPart>(data::fields::part::get(self.0)) }
    }
    pub fn is_extended(self) -> bool {
        data::fields::is_extended::get(self.0)
    }
    pub fn is_short(self) -> bool {
        data::fields::is_short::get(self.0)
    }
    pub fn is_unstable(self) -> bool {
        data::fields::is_unstable::get(self.0)
    }
    pub fn is_slot_0_occupied(self) -> bool {
        data::fields::is_slot_0_occupied::get(self.0)
    }
    pub fn is_slot_1_occupied(self) -> bool {
        data::fields::is_slot_1_occupied::get(self.0)
    }
    pub fn is_slot_2_occupied(self) -> bool {
        data::fields::is_slot_2_occupied::get(self.0)
    }
    pub fn is_slot_3_occupied(self) -> bool {
        data::fields::is_slot_3_occupied::get(self.0)
    }
    pub fn is_slot_4_occupied(self) -> bool {
        data::fields::is_slot_4_occupied::get(self.0)
    }
    pub fn is_slot_5_occupied(self) -> bool {
        data::fields::is_slot_5_occupied::get(self.0)
    }
    pub fn side_chain(self) -> SideChainPart {
        unsafe { core::mem::transmute::<u8, SideChainPart>(data::fields::side_chain::get(self.0)) }
    }
    pub fn is_east(self) -> bool {
        data::fields::is_east::get(self.0)
    }
    pub fn is_north(self) -> bool {
        data::fields::is_north::get(self.0)
    }
    pub fn is_south(self) -> bool {
        data::fields::is_south::get(self.0)
    }
    pub fn is_up(self) -> bool {
        data::fields::is_up::get(self.0)
    }
    pub fn is_west(self) -> bool {
        data::fields::is_west::get(self.0)
    }
    pub fn creaking_heart_state(self) -> CreakingHeartState {
        unsafe {
            core::mem::transmute::<u8, CreakingHeartState>(data::fields::creaking_heart_state::get(
                self.0,
            ))
        }
    }
    pub fn is_natural(self) -> bool {
        data::fields::is_natural::get(self.0)
    }
    pub fn power(self) -> u8 {
        data::fields::power::get(self.0)
    }
    pub fn moisture(self) -> u8 {
        data::fields::moisture::get(self.0)
    }
    pub fn is_lit(self) -> bool {
        data::fields::is_lit::get(self.0)
    }
    pub fn rotation(self) -> u8 {
        data::fields::rotation::get(self.0)
    }
    pub fn hinge(self) -> DoorHingeSide {
        unsafe { core::mem::transmute::<u8, DoorHingeSide>(data::fields::hinge::get(self.0)) }
    }
    pub fn is_open(self) -> bool {
        data::fields::is_open::get(self.0)
    }
    pub fn is_attached(self) -> bool {
        data::fields::is_attached::get(self.0)
    }
    pub fn face(self) -> AttachFace {
        unsafe { core::mem::transmute::<u8, AttachFace>(data::fields::face::get(self.0)) }
    }
    pub fn layers(self) -> u8 {
        data::fields::layers::get(self.0)
    }
    pub fn is_has_record(self) -> bool {
        data::fields::is_has_record::get(self.0)
    }
    pub fn bites(self) -> u8 {
        data::fields::bites::get(self.0)
    }
    pub fn delay(self) -> u8 {
        data::fields::delay::get(self.0)
    }
    pub fn is_locked(self) -> bool {
        data::fields::is_locked::get(self.0)
    }
    pub fn is_down(self) -> bool {
        data::fields::is_down::get(self.0)
    }
    pub fn is_in_wall(self) -> bool {
        data::fields::is_in_wall::get(self.0)
    }
    pub fn is_has_bottle_0(self) -> bool {
        data::fields::is_has_bottle_0::get(self.0)
    }
    pub fn is_has_bottle_1(self) -> bool {
        data::fields::is_has_bottle_1::get(self.0)
    }
    pub fn is_has_bottle_2(self) -> bool {
        data::fields::is_has_bottle_2::get(self.0)
    }
    pub fn is_eye(self) -> bool {
        data::fields::is_eye::get(self.0)
    }
    pub fn is_disarmed(self) -> bool {
        data::fields::is_disarmed::get(self.0)
    }
    pub fn is_conditional(self) -> bool {
        data::fields::is_conditional::get(self.0)
    }
    pub fn is_inverted(self) -> bool {
        data::fields::is_inverted::get(self.0)
    }
    pub fn is_enabled(self) -> bool {
        data::fields::is_enabled::get(self.0)
    }
    pub fn eggs(self) -> u8 {
        data::fields::eggs::get(self.0)
    }
    pub fn hatch(self) -> u8 {
        data::fields::hatch::get(self.0)
    }
    pub fn hydration(self) -> u8 {
        data::fields::hydration::get(self.0)
    }
    pub fn pickles(self) -> u8 {
        data::fields::pickles::get(self.0)
    }
    pub fn leaves(self) -> BambooLeaves {
        unsafe { core::mem::transmute::<u8, BambooLeaves>(data::fields::leaves::get(self.0)) }
    }
    pub fn is_drag(self) -> bool {
        data::fields::is_drag::get(self.0)
    }
    pub fn is_bottom(self) -> bool {
        data::fields::is_bottom::get(self.0)
    }
    pub fn is_has_book(self) -> bool {
        data::fields::is_has_book::get(self.0)
    }
    pub fn attachment(self) -> BellAttachType {
        unsafe { core::mem::transmute::<u8, BellAttachType>(data::fields::attachment::get(self.0)) }
    }
    pub fn is_signal_fire(self) -> bool {
        data::fields::is_signal_fire::get(self.0)
    }
    pub fn orientation(self) -> FrontAndTop {
        unsafe { core::mem::transmute::<u8, FrontAndTop>(data::fields::orientation::get(self.0)) }
    }
    pub fn honey_level(self) -> u8 {
        data::fields::honey_level::get(self.0)
    }
    pub fn charges(self) -> u8 {
        data::fields::charges::get(self.0)
    }
    pub fn candles(self) -> u8 {
        data::fields::candles::get(self.0)
    }
    pub fn sculk_sensor_phase(self) -> SculkSensorPhase {
        unsafe {
            core::mem::transmute::<u8, SculkSensorPhase>(data::fields::sculk_sensor_phase::get(
                self.0,
            ))
        }
    }
    pub fn is_bloom(self) -> bool {
        data::fields::is_bloom::get(self.0)
    }
    pub fn is_can_summon(self) -> bool {
        data::fields::is_can_summon::get(self.0)
    }
    pub fn is_shrieking(self) -> bool {
        data::fields::is_shrieking::get(self.0)
    }
    pub fn copper_golem_pose(self) -> Pose {
        unsafe { core::mem::transmute::<u8, Pose>(data::fields::copper_golem_pose::get(self.0)) }
    }
    pub fn thickness(self) -> DripstoneThickness {
        unsafe {
            core::mem::transmute::<u8, DripstoneThickness>(data::fields::thickness::get(self.0))
        }
    }
    pub fn vertical_direction(self) -> Direction {
        unsafe {
            core::mem::transmute::<u8, Direction>(data::fields::vertical_direction::get(self.0))
        }
    }
    pub fn is_berries(self) -> bool {
        data::fields::is_berries::get(self.0)
    }
    pub fn flower_amount(self) -> u8 {
        data::fields::flower_amount::get(self.0)
    }
    pub fn segment_amount(self) -> u8 {
        data::fields::segment_amount::get(self.0)
    }
    pub fn tilt(self) -> Tilt {
        unsafe { core::mem::transmute::<u8, Tilt>(data::fields::tilt::get(self.0)) }
    }
    pub fn is_cracked(self) -> bool {
        data::fields::is_cracked::get(self.0)
    }
    pub fn is_crafting(self) -> bool {
        data::fields::is_crafting::get(self.0)
    }
    pub fn is_ominous(self) -> bool {
        data::fields::is_ominous::get(self.0)
    }
    pub fn trial_spawner_state(self) -> TrialSpawnerState {
        unsafe {
            core::mem::transmute::<u8, TrialSpawnerState>(data::fields::trial_spawner_state::get(
                self.0,
            ))
        }
    }
    pub fn vault_state(self) -> VaultState {
        unsafe { core::mem::transmute::<u8, VaultState>(data::fields::vault_state::get(self.0)) }
    }
    pub fn is_tip(self) -> bool {
        data::fields::is_tip::get(self.0)
    }
    pub fn rail_shape(self) -> RailShape {
        unsafe { core::mem::transmute::<u8, RailShape>(data::fields::rail_shape::get(self.0)) }
    }
    pub fn stairs_shape(self) -> StairsShape {
        unsafe { core::mem::transmute::<u8, StairsShape>(data::fields::stairs_shape::get(self.0)) }
    }
    pub fn doubleblock_half(self) -> DoubleBlockHalf {
        unsafe {
            core::mem::transmute::<u8, DoubleBlockHalf>(data::fields::doubleblock_half::get(self.0))
        }
    }
    pub fn half(self) -> Half {
        unsafe { core::mem::transmute::<u8, Half>(data::fields::half::get(self.0)) }
    }
    pub fn piston_type(self) -> PistonType {
        unsafe { core::mem::transmute::<u8, PistonType>(data::fields::piston_type::get(self.0)) }
    }
    pub fn chest_type(self) -> ChestType {
        unsafe { core::mem::transmute::<u8, ChestType>(data::fields::chest_type::get(self.0)) }
    }
    pub fn slab_type(self) -> SlabType {
        unsafe { core::mem::transmute::<u8, SlabType>(data::fields::slab_type::get(self.0)) }
    }
    pub fn redstone_east(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::fields::redstone_east::get(self.0))
        }
    }
    pub fn wall_east(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::fields::wall_east::get(self.0)) }
    }
    pub fn redstone_north(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::fields::redstone_north::get(self.0))
        }
    }
    pub fn wall_north(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::fields::wall_north::get(self.0)) }
    }
    pub fn redstone_south(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::fields::redstone_south::get(self.0))
        }
    }
    pub fn wall_south(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::fields::wall_south::get(self.0)) }
    }
    pub fn redstone_west(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::fields::redstone_west::get(self.0))
        }
    }
    pub fn wall_west(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::fields::wall_west::get(self.0)) }
    }
    pub fn comparator_mode(self) -> ComparatorMode {
        unsafe {
            core::mem::transmute::<u8, ComparatorMode>(data::fields::comparator_mode::get(self.0))
        }
    }
    pub fn structure_mode(self) -> StructureMode {
        unsafe {
            core::mem::transmute::<u8, StructureMode>(data::fields::structure_mode::get(self.0))
        }
    }
    pub fn testblock_mode(self) -> TestBlockMode {
        unsafe {
            core::mem::transmute::<u8, TestBlockMode>(data::fields::testblock_mode::get(self.0))
        }
    }
}
pub struct BlockStateIter {
    current: u16,
    end: u16,
}
impl BlockStateIter {
    pub fn new(start: BlockState, end: BlockState) -> Self {
        Self {
            current: start.0,
            end: end.0,
        }
    }
}
impl Iterator for BlockStateIter {
    type Item = BlockState;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current > self.end {
            None
        } else {
            let id = self.current;
            self.current += 1;
            Some(BlockState(id))
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.end - self.current) as usize;
        (remaining, Some(remaining))
    }
}
