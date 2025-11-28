mod data;
mod enums;

use crate::BlockId;
use enums::*;
use mcre_core::OffsetType;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StateId(u16);

impl From<u16> for StateId {
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<StateId> for u16 {
    fn from(id: StateId) -> Self {
        id.0
    }
}

impl StateId {
    pub fn block_id(self) -> BlockId {
        data::block_id::get(self.0).into()
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

    pub fn is_snowy(self) -> bool {
        data::state_fields::is_snowy::get(self.0)
    }

    pub fn axis(self) -> Axis {
        unsafe { core::mem::transmute::<u8, Axis>(data::state_fields::axis::get(self.0)) }
    }

    pub fn stage(self) -> u8 {
        data::state_fields::stage::get(self.0)
    }

    pub fn age(self) -> u8 {
        data::state_fields::age::get(self.0)
    }

    pub fn is_hanging(self) -> bool {
        data::state_fields::is_hanging::get(self.0)
    }

    pub fn is_waterlogged(self) -> bool {
        data::state_fields::is_waterlogged::get(self.0)
    }

    pub fn level(self) -> u8 {
        data::state_fields::level::get(self.0)
    }

    pub fn dusted(self) -> u8 {
        data::state_fields::dusted::get(self.0)
    }

    pub fn distance(self) -> u8 {
        data::state_fields::distance::get(self.0)
    }

    pub fn is_persistent(self) -> bool {
        data::state_fields::is_persistent::get(self.0)
    }

    pub fn facing(self) -> Direction {
        unsafe { core::mem::transmute::<u8, Direction>(data::state_fields::facing::get(self.0)) }
    }

    pub fn is_triggered(self) -> bool {
        data::state_fields::is_triggered::get(self.0)
    }

    pub fn instrument(self) -> NoteBlockInstrument {
        unsafe {
            core::mem::transmute::<u8, NoteBlockInstrument>(data::state_fields::instrument::get(
                self.0,
            ))
        }
    }

    pub fn note(self) -> u8 {
        data::state_fields::note::get(self.0)
    }

    pub fn is_powered(self) -> bool {
        data::state_fields::is_powered::get(self.0)
    }

    pub fn is_occupied(self) -> bool {
        data::state_fields::is_occupied::get(self.0)
    }

    pub fn part(self) -> BedPart {
        unsafe { core::mem::transmute::<u8, BedPart>(data::state_fields::part::get(self.0)) }
    }

    pub fn is_extended(self) -> bool {
        data::state_fields::is_extended::get(self.0)
    }

    pub fn is_short(self) -> bool {
        data::state_fields::is_short::get(self.0)
    }

    pub fn is_unstable(self) -> bool {
        data::state_fields::is_unstable::get(self.0)
    }

    pub fn is_slot_0_occupied(self) -> bool {
        data::state_fields::is_slot_0_occupied::get(self.0)
    }

    pub fn is_slot_1_occupied(self) -> bool {
        data::state_fields::is_slot_1_occupied::get(self.0)
    }

    pub fn is_slot_2_occupied(self) -> bool {
        data::state_fields::is_slot_2_occupied::get(self.0)
    }

    pub fn is_slot_3_occupied(self) -> bool {
        data::state_fields::is_slot_3_occupied::get(self.0)
    }

    pub fn is_slot_4_occupied(self) -> bool {
        data::state_fields::is_slot_4_occupied::get(self.0)
    }

    pub fn is_slot_5_occupied(self) -> bool {
        data::state_fields::is_slot_5_occupied::get(self.0)
    }

    pub fn side_chain(self) -> SideChainPart {
        unsafe {
            core::mem::transmute::<u8, SideChainPart>(data::state_fields::side_chain::get(self.0))
        }
    }

    pub fn is_east(self) -> bool {
        data::state_fields::is_east::get(self.0)
    }

    pub fn is_north(self) -> bool {
        data::state_fields::is_north::get(self.0)
    }

    pub fn is_south(self) -> bool {
        data::state_fields::is_south::get(self.0)
    }

    pub fn is_up(self) -> bool {
        data::state_fields::is_up::get(self.0)
    }

    pub fn is_west(self) -> bool {
        data::state_fields::is_west::get(self.0)
    }

    pub fn creaking_heart_state(self) -> CreakingHeartState {
        unsafe {
            core::mem::transmute::<u8, CreakingHeartState>(
                data::state_fields::creaking_heart_state::get(self.0),
            )
        }
    }

    pub fn is_natural(self) -> bool {
        data::state_fields::is_natural::get(self.0)
    }

    pub fn power(self) -> u8 {
        data::state_fields::power::get(self.0)
    }

    pub fn moisture(self) -> u8 {
        data::state_fields::moisture::get(self.0)
    }

    pub fn is_lit(self) -> bool {
        data::state_fields::is_lit::get(self.0)
    }

    pub fn rotation(self) -> u8 {
        data::state_fields::rotation::get(self.0)
    }

    pub fn hinge(self) -> DoorHingeSide {
        unsafe { core::mem::transmute::<u8, DoorHingeSide>(data::state_fields::hinge::get(self.0)) }
    }

    pub fn is_open(self) -> bool {
        data::state_fields::is_open::get(self.0)
    }

    pub fn is_attached(self) -> bool {
        data::state_fields::is_attached::get(self.0)
    }

    pub fn face(self) -> AttachFace {
        unsafe { core::mem::transmute::<u8, AttachFace>(data::state_fields::face::get(self.0)) }
    }

    pub fn layers(self) -> u8 {
        data::state_fields::layers::get(self.0)
    }

    pub fn is_has_record(self) -> bool {
        data::state_fields::is_has_record::get(self.0)
    }

    pub fn bites(self) -> u8 {
        data::state_fields::bites::get(self.0)
    }

    pub fn delay(self) -> u8 {
        data::state_fields::delay::get(self.0)
    }

    pub fn is_locked(self) -> bool {
        data::state_fields::is_locked::get(self.0)
    }

    pub fn is_down(self) -> bool {
        data::state_fields::is_down::get(self.0)
    }

    pub fn is_in_wall(self) -> bool {
        data::state_fields::is_in_wall::get(self.0)
    }

    pub fn is_has_bottle_0(self) -> bool {
        data::state_fields::is_has_bottle_0::get(self.0)
    }

    pub fn is_has_bottle_1(self) -> bool {
        data::state_fields::is_has_bottle_1::get(self.0)
    }

    pub fn is_has_bottle_2(self) -> bool {
        data::state_fields::is_has_bottle_2::get(self.0)
    }

    pub fn is_eye(self) -> bool {
        data::state_fields::is_eye::get(self.0)
    }

    pub fn is_disarmed(self) -> bool {
        data::state_fields::is_disarmed::get(self.0)
    }

    pub fn is_conditional(self) -> bool {
        data::state_fields::is_conditional::get(self.0)
    }

    pub fn is_inverted(self) -> bool {
        data::state_fields::is_inverted::get(self.0)
    }

    pub fn is_enabled(self) -> bool {
        data::state_fields::is_enabled::get(self.0)
    }

    pub fn eggs(self) -> u8 {
        data::state_fields::eggs::get(self.0)
    }

    pub fn hatch(self) -> u8 {
        data::state_fields::hatch::get(self.0)
    }

    pub fn hydration(self) -> u8 {
        data::state_fields::hydration::get(self.0)
    }

    pub fn pickles(self) -> u8 {
        data::state_fields::pickles::get(self.0)
    }

    pub fn leaves(self) -> BambooLeaves {
        unsafe { core::mem::transmute::<u8, BambooLeaves>(data::state_fields::leaves::get(self.0)) }
    }

    pub fn is_drag(self) -> bool {
        data::state_fields::is_drag::get(self.0)
    }

    pub fn is_bottom(self) -> bool {
        data::state_fields::is_bottom::get(self.0)
    }

    pub fn is_has_book(self) -> bool {
        data::state_fields::is_has_book::get(self.0)
    }

    pub fn attachment(self) -> BellAttachType {
        unsafe {
            core::mem::transmute::<u8, BellAttachType>(data::state_fields::attachment::get(self.0))
        }
    }

    pub fn is_signal_fire(self) -> bool {
        data::state_fields::is_signal_fire::get(self.0)
    }

    pub fn orientation(self) -> FrontAndTop {
        unsafe {
            core::mem::transmute::<u8, FrontAndTop>(data::state_fields::orientation::get(self.0))
        }
    }

    pub fn honey_level(self) -> u8 {
        data::state_fields::honey_level::get(self.0)
    }

    pub fn charges(self) -> u8 {
        data::state_fields::charges::get(self.0)
    }

    pub fn candles(self) -> u8 {
        data::state_fields::candles::get(self.0)
    }

    pub fn sculk_sensor_phase(self) -> SculkSensorPhase {
        unsafe {
            core::mem::transmute::<u8, SculkSensorPhase>(
                data::state_fields::sculk_sensor_phase::get(self.0),
            )
        }
    }

    pub fn is_bloom(self) -> bool {
        data::state_fields::is_bloom::get(self.0)
    }

    pub fn is_can_summon(self) -> bool {
        data::state_fields::is_can_summon::get(self.0)
    }

    pub fn is_shrieking(self) -> bool {
        data::state_fields::is_shrieking::get(self.0)
    }

    pub fn copper_golem_pose(self) -> Pose {
        unsafe {
            core::mem::transmute::<u8, Pose>(data::state_fields::copper_golem_pose::get(self.0))
        }
    }

    pub fn thickness(self) -> DripstoneThickness {
        unsafe {
            core::mem::transmute::<u8, DripstoneThickness>(data::state_fields::thickness::get(
                self.0,
            ))
        }
    }

    pub fn vertical_direction(self) -> Direction {
        unsafe {
            core::mem::transmute::<u8, Direction>(data::state_fields::vertical_direction::get(
                self.0,
            ))
        }
    }

    pub fn is_berries(self) -> bool {
        data::state_fields::is_berries::get(self.0)
    }

    pub fn flower_amount(self) -> u8 {
        data::state_fields::flower_amount::get(self.0)
    }

    pub fn segment_amount(self) -> u8 {
        data::state_fields::segment_amount::get(self.0)
    }

    pub fn tilt(self) -> Tilt {
        unsafe { core::mem::transmute::<u8, Tilt>(data::state_fields::tilt::get(self.0)) }
    }

    pub fn is_cracked(self) -> bool {
        data::state_fields::is_cracked::get(self.0)
    }

    pub fn is_crafting(self) -> bool {
        data::state_fields::is_crafting::get(self.0)
    }

    pub fn is_ominous(self) -> bool {
        data::state_fields::is_ominous::get(self.0)
    }

    pub fn trial_spawner_state(self) -> TrialSpawnerState {
        unsafe {
            core::mem::transmute::<u8, TrialSpawnerState>(
                data::state_fields::trial_spawner_state::get(self.0),
            )
        }
    }

    pub fn vault_state(self) -> VaultState {
        unsafe {
            core::mem::transmute::<u8, VaultState>(data::state_fields::vault_state::get(self.0))
        }
    }

    pub fn is_tip(self) -> bool {
        data::state_fields::is_tip::get(self.0)
    }

    pub fn rail_shape(self) -> RailShape {
        unsafe {
            core::mem::transmute::<u8, RailShape>(data::state_fields::rail_shape::get(self.0))
        }
    }

    pub fn stairs_shape(self) -> StairsShape {
        unsafe {
            core::mem::transmute::<u8, StairsShape>(data::state_fields::stairs_shape::get(self.0))
        }
    }

    pub fn doubleblock_half(self) -> DoubleBlockHalf {
        unsafe {
            core::mem::transmute::<u8, DoubleBlockHalf>(data::state_fields::doubleblock_half::get(
                self.0,
            ))
        }
    }

    pub fn half(self) -> Half {
        unsafe { core::mem::transmute::<u8, Half>(data::state_fields::half::get(self.0)) }
    }

    pub fn piston_type(self) -> PistonType {
        unsafe {
            core::mem::transmute::<u8, PistonType>(data::state_fields::piston_type::get(self.0))
        }
    }

    pub fn chest_type(self) -> ChestType {
        unsafe {
            core::mem::transmute::<u8, ChestType>(data::state_fields::chest_type::get(self.0))
        }
    }

    pub fn slab_type(self) -> SlabType {
        unsafe { core::mem::transmute::<u8, SlabType>(data::state_fields::slab_type::get(self.0)) }
    }

    pub fn redstone_east(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::state_fields::redstone_east::get(self.0))
        }
    }

    pub fn wall_east(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::state_fields::wall_east::get(self.0)) }
    }

    pub fn redstone_north(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::state_fields::redstone_north::get(
                self.0,
            ))
        }
    }

    pub fn wall_north(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::state_fields::wall_north::get(self.0)) }
    }

    pub fn redstone_south(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::state_fields::redstone_south::get(
                self.0,
            ))
        }
    }

    pub fn wall_south(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::state_fields::wall_south::get(self.0)) }
    }

    pub fn redstone_west(self) -> RedstoneSide {
        unsafe {
            core::mem::transmute::<u8, RedstoneSide>(data::state_fields::redstone_west::get(self.0))
        }
    }

    pub fn wall_west(self) -> WallSide {
        unsafe { core::mem::transmute::<u8, WallSide>(data::state_fields::wall_west::get(self.0)) }
    }

    pub fn comparator_mode(self) -> ComparatorMode {
        unsafe {
            core::mem::transmute::<u8, ComparatorMode>(data::state_fields::comparator_mode::get(
                self.0,
            ))
        }
    }

    pub fn structure_mode(self) -> StructureMode {
        unsafe {
            core::mem::transmute::<u8, StructureMode>(data::state_fields::structure_mode::get(
                self.0,
            ))
        }
    }

    pub fn testblock_mode(self) -> TestBlockMode {
        unsafe {
            core::mem::transmute::<u8, TestBlockMode>(data::state_fields::testblock_mode::get(
                self.0,
            ))
        }
    }
}
