use super::state::*;
use core::str::FromStr;
use alloc::{vec::Vec, boxed::Box};
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ShapePropVal {
    NorthSouth,
    EastWest,
    AscendingEast,
    AscendingWest,
    AscendingNorth,
    AscendingSouth,
    SouthEast,
    SouthWest,
    NorthWest,
    NorthEast,
    Straight,
    InnerLeft,
    InnerRight,
    OuterLeft,
    OuterRight,
}
impl ShapePropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NorthSouth => "north_south",
            Self::EastWest => "east_west",
            Self::AscendingEast => "ascending_east",
            Self::AscendingWest => "ascending_west",
            Self::AscendingNorth => "ascending_north",
            Self::AscendingSouth => "ascending_south",
            Self::SouthEast => "south_east",
            Self::SouthWest => "south_west",
            Self::NorthWest => "north_west",
            Self::NorthEast => "north_east",
            Self::Straight => "straight",
            Self::InnerLeft => "inner_left",
            Self::InnerRight => "inner_right",
            Self::OuterLeft => "outer_left",
            Self::OuterRight => "outer_right",
        }
    }
}
impl FromStr for ShapePropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "north_south" => Ok(Self::NorthSouth),
            "east_west" => Ok(Self::EastWest),
            "ascending_east" => Ok(Self::AscendingEast),
            "ascending_west" => Ok(Self::AscendingWest),
            "ascending_north" => Ok(Self::AscendingNorth),
            "ascending_south" => Ok(Self::AscendingSouth),
            "south_east" => Ok(Self::SouthEast),
            "south_west" => Ok(Self::SouthWest),
            "north_west" => Ok(Self::NorthWest),
            "north_east" => Ok(Self::NorthEast),
            "straight" => Ok(Self::Straight),
            "inner_left" => Ok(Self::InnerLeft),
            "inner_right" => Ok(Self::InnerRight),
            "outer_left" => Ok(Self::OuterLeft),
            "outer_right" => Ok(Self::OuterRight),
            _ => Err(()),
        }
    }
}
impl From<RailShape> for ShapePropVal {
    fn from(e: RailShape) -> Self {
        match e {
            RailShape::NorthSouth => Self::NorthSouth,
            RailShape::EastWest => Self::EastWest,
            RailShape::AscendingEast => Self::AscendingEast,
            RailShape::AscendingWest => Self::AscendingWest,
            RailShape::AscendingNorth => Self::AscendingNorth,
            RailShape::AscendingSouth => Self::AscendingSouth,
            RailShape::SouthEast => Self::SouthEast,
            RailShape::SouthWest => Self::SouthWest,
            RailShape::NorthWest => Self::NorthWest,
            RailShape::NorthEast => Self::NorthEast,
        }
    }
}
impl From<StairsShape> for ShapePropVal {
    fn from(e: StairsShape) -> Self {
        match e {
            StairsShape::Straight => Self::Straight,
            StairsShape::InnerLeft => Self::InnerLeft,
            StairsShape::InnerRight => Self::InnerRight,
            StairsShape::OuterLeft => Self::OuterLeft,
            StairsShape::OuterRight => Self::OuterRight,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HalfPropVal {
    Upper,
    Lower,
    Top,
    Bottom,
}
impl HalfPropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}
impl FromStr for HalfPropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "upper" => Ok(Self::Upper),
            "lower" => Ok(Self::Lower),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(()),
        }
    }
}
impl From<DoubleBlockHalf> for HalfPropVal {
    fn from(e: DoubleBlockHalf) -> Self {
        match e {
            DoubleBlockHalf::Upper => Self::Upper,
            DoubleBlockHalf::Lower => Self::Lower,
        }
    }
}
impl From<Half> for HalfPropVal {
    fn from(e: Half) -> Self {
        match e {
            Half::Top => Self::Top,
            Half::Bottom => Self::Bottom,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TypePropVal {
    Normal,
    Sticky,
    Single,
    Left,
    Right,
    Top,
    Bottom,
    Double,
}
impl TypePropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Sticky => "sticky",
            Self::Single => "single",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Double => "double",
        }
    }
}
impl FromStr for TypePropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "normal" => Ok(Self::Normal),
            "sticky" => Ok(Self::Sticky),
            "single" => Ok(Self::Single),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            "double" => Ok(Self::Double),
            _ => Err(()),
        }
    }
}
impl From<PistonType> for TypePropVal {
    fn from(e: PistonType) -> Self {
        match e {
            PistonType::Normal => Self::Normal,
            PistonType::Sticky => Self::Sticky,
        }
    }
}
impl From<ChestType> for TypePropVal {
    fn from(e: ChestType) -> Self {
        match e {
            ChestType::Single => Self::Single,
            ChestType::Left => Self::Left,
            ChestType::Right => Self::Right,
        }
    }
}
impl From<SlabType> for TypePropVal {
    fn from(e: SlabType) -> Self {
        match e {
            SlabType::Top => Self::Top,
            SlabType::Bottom => Self::Bottom,
            SlabType::Double => Self::Double,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum EastPropVal {
    True,
    False,
    Up,
    Side,
    None,
    Low,
    Tall,
}
impl EastPropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Up => "up",
            Self::Side => "side",
            Self::None => "none",
            Self::Low => "low",
            Self::Tall => "tall",
        }
    }
}
impl FromStr for EastPropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "up" => Ok(Self::Up),
            "side" => Ok(Self::Side),
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "tall" => Ok(Self::Tall),
            _ => Err(()),
        }
    }
}
impl From<bool> for EastPropVal {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}
impl From<RedstoneSide> for EastPropVal {
    fn from(e: RedstoneSide) -> Self {
        match e {
            RedstoneSide::Up => Self::Up,
            RedstoneSide::Side => Self::Side,
            RedstoneSide::None => Self::None,
        }
    }
}
impl From<WallSide> for EastPropVal {
    fn from(e: WallSide) -> Self {
        match e {
            WallSide::None => Self::None,
            WallSide::Low => Self::Low,
            WallSide::Tall => Self::Tall,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NorthPropVal {
    True,
    False,
    Up,
    Side,
    None,
    Low,
    Tall,
}
impl NorthPropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Up => "up",
            Self::Side => "side",
            Self::None => "none",
            Self::Low => "low",
            Self::Tall => "tall",
        }
    }
}
impl FromStr for NorthPropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "up" => Ok(Self::Up),
            "side" => Ok(Self::Side),
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "tall" => Ok(Self::Tall),
            _ => Err(()),
        }
    }
}
impl From<bool> for NorthPropVal {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}
impl From<RedstoneSide> for NorthPropVal {
    fn from(e: RedstoneSide) -> Self {
        match e {
            RedstoneSide::Up => Self::Up,
            RedstoneSide::Side => Self::Side,
            RedstoneSide::None => Self::None,
        }
    }
}
impl From<WallSide> for NorthPropVal {
    fn from(e: WallSide) -> Self {
        match e {
            WallSide::None => Self::None,
            WallSide::Low => Self::Low,
            WallSide::Tall => Self::Tall,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SouthPropVal {
    True,
    False,
    Up,
    Side,
    None,
    Low,
    Tall,
}
impl SouthPropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Up => "up",
            Self::Side => "side",
            Self::None => "none",
            Self::Low => "low",
            Self::Tall => "tall",
        }
    }
}
impl FromStr for SouthPropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "up" => Ok(Self::Up),
            "side" => Ok(Self::Side),
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "tall" => Ok(Self::Tall),
            _ => Err(()),
        }
    }
}
impl From<bool> for SouthPropVal {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}
impl From<RedstoneSide> for SouthPropVal {
    fn from(e: RedstoneSide) -> Self {
        match e {
            RedstoneSide::Up => Self::Up,
            RedstoneSide::Side => Self::Side,
            RedstoneSide::None => Self::None,
        }
    }
}
impl From<WallSide> for SouthPropVal {
    fn from(e: WallSide) -> Self {
        match e {
            WallSide::None => Self::None,
            WallSide::Low => Self::Low,
            WallSide::Tall => Self::Tall,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WestPropVal {
    True,
    False,
    Up,
    Side,
    None,
    Low,
    Tall,
}
impl WestPropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Up => "up",
            Self::Side => "side",
            Self::None => "none",
            Self::Low => "low",
            Self::Tall => "tall",
        }
    }
}
impl FromStr for WestPropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            "up" => Ok(Self::Up),
            "side" => Ok(Self::Side),
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "tall" => Ok(Self::Tall),
            _ => Err(()),
        }
    }
}
impl From<bool> for WestPropVal {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}
impl From<RedstoneSide> for WestPropVal {
    fn from(e: RedstoneSide) -> Self {
        match e {
            RedstoneSide::Up => Self::Up,
            RedstoneSide::Side => Self::Side,
            RedstoneSide::None => Self::None,
        }
    }
}
impl From<WallSide> for WestPropVal {
    fn from(e: WallSide) -> Self {
        match e {
            WallSide::None => Self::None,
            WallSide::Low => Self::Low,
            WallSide::Tall => Self::Tall,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ModePropVal {
    Compare,
    Subtract,
    Save,
    Load,
    Corner,
    Data,
    Start,
    Log,
    Fail,
    Accept,
}
impl ModePropVal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compare => "compare",
            Self::Subtract => "subtract",
            Self::Save => "save",
            Self::Load => "load",
            Self::Corner => "corner",
            Self::Data => "data",
            Self::Start => "start",
            Self::Log => "log",
            Self::Fail => "fail",
            Self::Accept => "accept",
        }
    }
}
impl FromStr for ModePropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "compare" => Ok(Self::Compare),
            "subtract" => Ok(Self::Subtract),
            "save" => Ok(Self::Save),
            "load" => Ok(Self::Load),
            "corner" => Ok(Self::Corner),
            "data" => Ok(Self::Data),
            "start" => Ok(Self::Start),
            "log" => Ok(Self::Log),
            "fail" => Ok(Self::Fail),
            "accept" => Ok(Self::Accept),
            _ => Err(()),
        }
    }
}
impl From<ComparatorMode> for ModePropVal {
    fn from(e: ComparatorMode) -> Self {
        match e {
            ComparatorMode::Compare => Self::Compare,
            ComparatorMode::Subtract => Self::Subtract,
        }
    }
}
impl From<StructureMode> for ModePropVal {
    fn from(e: StructureMode) -> Self {
        match e {
            StructureMode::Save => Self::Save,
            StructureMode::Load => Self::Load,
            StructureMode::Corner => Self::Corner,
            StructureMode::Data => Self::Data,
        }
    }
}
impl From<TestBlockMode> for ModePropVal {
    fn from(e: TestBlockMode) -> Self {
        match e {
            TestBlockMode::Start => Self::Start,
            TestBlockMode::Log => Self::Log,
            TestBlockMode::Fail => Self::Fail,
            TestBlockMode::Accept => Self::Accept,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PropKey {
    Snowy = 0u8,
    Axis = 1u8,
    Stage = 2u8,
    Age = 3u8,
    Hanging = 4u8,
    Waterlogged = 5u8,
    Level = 6u8,
    Dusted = 7u8,
    Distance = 8u8,
    Persistent = 9u8,
    Facing = 10u8,
    Triggered = 11u8,
    Instrument = 12u8,
    Note = 13u8,
    Powered = 14u8,
    Occupied = 15u8,
    Part = 16u8,
    Shape = 17u8,
    Extended = 18u8,
    Half = 19u8,
    Short = 20u8,
    Type = 21u8,
    Unstable = 22u8,
    Slot0Occupied = 23u8,
    Slot1Occupied = 24u8,
    Slot2Occupied = 25u8,
    Slot3Occupied = 26u8,
    Slot4Occupied = 27u8,
    Slot5Occupied = 28u8,
    SideChain = 29u8,
    East = 30u8,
    North = 31u8,
    South = 32u8,
    Up = 33u8,
    West = 34u8,
    CreakingHeartState = 35u8,
    Natural = 36u8,
    Power = 37u8,
    Moisture = 38u8,
    Lit = 39u8,
    Rotation = 40u8,
    Hinge = 41u8,
    Open = 42u8,
    Attached = 43u8,
    Face = 44u8,
    Layers = 45u8,
    HasRecord = 46u8,
    Bites = 47u8,
    Delay = 48u8,
    Locked = 49u8,
    Down = 50u8,
    InWall = 51u8,
    HasBottle0 = 52u8,
    HasBottle1 = 53u8,
    HasBottle2 = 54u8,
    Eye = 55u8,
    Disarmed = 56u8,
    Conditional = 57u8,
    Mode = 58u8,
    Inverted = 59u8,
    Enabled = 60u8,
    Eggs = 61u8,
    Hatch = 62u8,
    Hydration = 63u8,
    Pickles = 64u8,
    Leaves = 65u8,
    Drag = 66u8,
    Bottom = 67u8,
    HasBook = 68u8,
    Attachment = 69u8,
    SignalFire = 70u8,
    Orientation = 71u8,
    HoneyLevel = 72u8,
    Charges = 73u8,
    Candles = 74u8,
    SculkSensorPhase = 75u8,
    Bloom = 76u8,
    CanSummon = 77u8,
    Shrieking = 78u8,
    CopperGolemPose = 79u8,
    Thickness = 80u8,
    VerticalDirection = 81u8,
    Berries = 82u8,
    FlowerAmount = 83u8,
    SegmentAmount = 84u8,
    Tilt = 85u8,
    Cracked = 86u8,
    Crafting = 87u8,
    Ominous = 88u8,
    TrialSpawnerState = 89u8,
    VaultState = 90u8,
    Tip = 91u8,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PropVal {
    Snowy(bool) = 0u8,
    Axis(Axis) = 1u8,
    Stage(u8) = 2u8,
    Age(u8) = 3u8,
    Hanging(bool) = 4u8,
    Waterlogged(bool) = 5u8,
    Level(u8) = 6u8,
    Dusted(u8) = 7u8,
    Distance(u8) = 8u8,
    Persistent(bool) = 9u8,
    Facing(Direction) = 10u8,
    Triggered(bool) = 11u8,
    Instrument(NoteBlockInstrument) = 12u8,
    Note(u8) = 13u8,
    Powered(bool) = 14u8,
    Occupied(bool) = 15u8,
    Part(BedPart) = 16u8,
    Shape(ShapePropVal) = 17u8,
    Extended(bool) = 18u8,
    Half(HalfPropVal) = 19u8,
    Short(bool) = 20u8,
    Type(TypePropVal) = 21u8,
    Unstable(bool) = 22u8,
    Slot0Occupied(bool) = 23u8,
    Slot1Occupied(bool) = 24u8,
    Slot2Occupied(bool) = 25u8,
    Slot3Occupied(bool) = 26u8,
    Slot4Occupied(bool) = 27u8,
    Slot5Occupied(bool) = 28u8,
    SideChain(SideChainPart) = 29u8,
    East(EastPropVal) = 30u8,
    North(NorthPropVal) = 31u8,
    South(SouthPropVal) = 32u8,
    Up(bool) = 33u8,
    West(WestPropVal) = 34u8,
    CreakingHeartState(CreakingHeartState) = 35u8,
    Natural(bool) = 36u8,
    Power(u8) = 37u8,
    Moisture(u8) = 38u8,
    Lit(bool) = 39u8,
    Rotation(u8) = 40u8,
    Hinge(DoorHingeSide) = 41u8,
    Open(bool) = 42u8,
    Attached(bool) = 43u8,
    Face(AttachFace) = 44u8,
    Layers(u8) = 45u8,
    HasRecord(bool) = 46u8,
    Bites(u8) = 47u8,
    Delay(u8) = 48u8,
    Locked(bool) = 49u8,
    Down(bool) = 50u8,
    InWall(bool) = 51u8,
    HasBottle0(bool) = 52u8,
    HasBottle1(bool) = 53u8,
    HasBottle2(bool) = 54u8,
    Eye(bool) = 55u8,
    Disarmed(bool) = 56u8,
    Conditional(bool) = 57u8,
    Mode(ModePropVal) = 58u8,
    Inverted(bool) = 59u8,
    Enabled(bool) = 60u8,
    Eggs(u8) = 61u8,
    Hatch(u8) = 62u8,
    Hydration(u8) = 63u8,
    Pickles(u8) = 64u8,
    Leaves(BambooLeaves) = 65u8,
    Drag(bool) = 66u8,
    Bottom(bool) = 67u8,
    HasBook(bool) = 68u8,
    Attachment(BellAttachType) = 69u8,
    SignalFire(bool) = 70u8,
    Orientation(FrontAndTop) = 71u8,
    HoneyLevel(u8) = 72u8,
    Charges(u8) = 73u8,
    Candles(u8) = 74u8,
    SculkSensorPhase(SculkSensorPhase) = 75u8,
    Bloom(bool) = 76u8,
    CanSummon(bool) = 77u8,
    Shrieking(bool) = 78u8,
    CopperGolemPose(Pose) = 79u8,
    Thickness(DripstoneThickness) = 80u8,
    VerticalDirection(Direction) = 81u8,
    Berries(bool) = 82u8,
    FlowerAmount(u8) = 83u8,
    SegmentAmount(u8) = 84u8,
    Tilt(Tilt) = 85u8,
    Cracked(bool) = 86u8,
    Crafting(bool) = 87u8,
    Ominous(bool) = 88u8,
    TrialSpawnerState(TrialSpawnerState) = 89u8,
    VaultState(VaultState) = 90u8,
    Tip(bool) = 91u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PropFilter {
    Snowy(Box<[bool]>) = 0u8,
    Axis(Box<[Axis]>) = 1u8,
    Stage(Box<[u8]>) = 2u8,
    Age(Box<[u8]>) = 3u8,
    Hanging(Box<[bool]>) = 4u8,
    Waterlogged(Box<[bool]>) = 5u8,
    Level(Box<[u8]>) = 6u8,
    Dusted(Box<[u8]>) = 7u8,
    Distance(Box<[u8]>) = 8u8,
    Persistent(Box<[bool]>) = 9u8,
    Facing(Box<[Direction]>) = 10u8,
    Triggered(Box<[bool]>) = 11u8,
    Instrument(Box<[NoteBlockInstrument]>) = 12u8,
    Note(Box<[u8]>) = 13u8,
    Powered(Box<[bool]>) = 14u8,
    Occupied(Box<[bool]>) = 15u8,
    Part(Box<[BedPart]>) = 16u8,
    Shape(Box<[ShapePropVal]>) = 17u8,
    Extended(Box<[bool]>) = 18u8,
    Half(Box<[HalfPropVal]>) = 19u8,
    Short(Box<[bool]>) = 20u8,
    Type(Box<[TypePropVal]>) = 21u8,
    Unstable(Box<[bool]>) = 22u8,
    Slot0Occupied(Box<[bool]>) = 23u8,
    Slot1Occupied(Box<[bool]>) = 24u8,
    Slot2Occupied(Box<[bool]>) = 25u8,
    Slot3Occupied(Box<[bool]>) = 26u8,
    Slot4Occupied(Box<[bool]>) = 27u8,
    Slot5Occupied(Box<[bool]>) = 28u8,
    SideChain(Box<[SideChainPart]>) = 29u8,
    East(Box<[EastPropVal]>) = 30u8,
    North(Box<[NorthPropVal]>) = 31u8,
    South(Box<[SouthPropVal]>) = 32u8,
    Up(Box<[bool]>) = 33u8,
    West(Box<[WestPropVal]>) = 34u8,
    CreakingHeartState(Box<[CreakingHeartState]>) = 35u8,
    Natural(Box<[bool]>) = 36u8,
    Power(Box<[u8]>) = 37u8,
    Moisture(Box<[u8]>) = 38u8,
    Lit(Box<[bool]>) = 39u8,
    Rotation(Box<[u8]>) = 40u8,
    Hinge(Box<[DoorHingeSide]>) = 41u8,
    Open(Box<[bool]>) = 42u8,
    Attached(Box<[bool]>) = 43u8,
    Face(Box<[AttachFace]>) = 44u8,
    Layers(Box<[u8]>) = 45u8,
    HasRecord(Box<[bool]>) = 46u8,
    Bites(Box<[u8]>) = 47u8,
    Delay(Box<[u8]>) = 48u8,
    Locked(Box<[bool]>) = 49u8,
    Down(Box<[bool]>) = 50u8,
    InWall(Box<[bool]>) = 51u8,
    HasBottle0(Box<[bool]>) = 52u8,
    HasBottle1(Box<[bool]>) = 53u8,
    HasBottle2(Box<[bool]>) = 54u8,
    Eye(Box<[bool]>) = 55u8,
    Disarmed(Box<[bool]>) = 56u8,
    Conditional(Box<[bool]>) = 57u8,
    Mode(Box<[ModePropVal]>) = 58u8,
    Inverted(Box<[bool]>) = 59u8,
    Enabled(Box<[bool]>) = 60u8,
    Eggs(Box<[u8]>) = 61u8,
    Hatch(Box<[u8]>) = 62u8,
    Hydration(Box<[u8]>) = 63u8,
    Pickles(Box<[u8]>) = 64u8,
    Leaves(Box<[BambooLeaves]>) = 65u8,
    Drag(Box<[bool]>) = 66u8,
    Bottom(Box<[bool]>) = 67u8,
    HasBook(Box<[bool]>) = 68u8,
    Attachment(Box<[BellAttachType]>) = 69u8,
    SignalFire(Box<[bool]>) = 70u8,
    Orientation(Box<[FrontAndTop]>) = 71u8,
    HoneyLevel(Box<[u8]>) = 72u8,
    Charges(Box<[u8]>) = 73u8,
    Candles(Box<[u8]>) = 74u8,
    SculkSensorPhase(Box<[SculkSensorPhase]>) = 75u8,
    Bloom(Box<[bool]>) = 76u8,
    CanSummon(Box<[bool]>) = 77u8,
    Shrieking(Box<[bool]>) = 78u8,
    CopperGolemPose(Box<[Pose]>) = 79u8,
    Thickness(Box<[DripstoneThickness]>) = 80u8,
    VerticalDirection(Box<[Direction]>) = 81u8,
    Berries(Box<[bool]>) = 82u8,
    FlowerAmount(Box<[u8]>) = 83u8,
    SegmentAmount(Box<[u8]>) = 84u8,
    Tilt(Box<[Tilt]>) = 85u8,
    Cracked(Box<[bool]>) = 86u8,
    Crafting(Box<[bool]>) = 87u8,
    Ominous(Box<[bool]>) = 88u8,
    TrialSpawnerState(Box<[TrialSpawnerState]>) = 89u8,
    VaultState(Box<[VaultState]>) = 90u8,
    Tip(Box<[bool]>) = 91u8,
}
impl PropKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Snowy => "snowy",
            Self::Axis => "axis",
            Self::Stage => "stage",
            Self::Age => "age",
            Self::Hanging => "hanging",
            Self::Waterlogged => "waterlogged",
            Self::Level => "level",
            Self::Dusted => "dusted",
            Self::Distance => "distance",
            Self::Persistent => "persistent",
            Self::Facing => "facing",
            Self::Triggered => "triggered",
            Self::Instrument => "instrument",
            Self::Note => "note",
            Self::Powered => "powered",
            Self::Occupied => "occupied",
            Self::Part => "part",
            Self::Shape => "shape",
            Self::Extended => "extended",
            Self::Half => "half",
            Self::Short => "short",
            Self::Type => "type",
            Self::Unstable => "unstable",
            Self::Slot0Occupied => "slot_0_occupied",
            Self::Slot1Occupied => "slot_1_occupied",
            Self::Slot2Occupied => "slot_2_occupied",
            Self::Slot3Occupied => "slot_3_occupied",
            Self::Slot4Occupied => "slot_4_occupied",
            Self::Slot5Occupied => "slot_5_occupied",
            Self::SideChain => "side_chain",
            Self::East => "east",
            Self::North => "north",
            Self::South => "south",
            Self::Up => "up",
            Self::West => "west",
            Self::CreakingHeartState => "creaking_heart_state",
            Self::Natural => "natural",
            Self::Power => "power",
            Self::Moisture => "moisture",
            Self::Lit => "lit",
            Self::Rotation => "rotation",
            Self::Hinge => "hinge",
            Self::Open => "open",
            Self::Attached => "attached",
            Self::Face => "face",
            Self::Layers => "layers",
            Self::HasRecord => "has_record",
            Self::Bites => "bites",
            Self::Delay => "delay",
            Self::Locked => "locked",
            Self::Down => "down",
            Self::InWall => "in_wall",
            Self::HasBottle0 => "has_bottle_0",
            Self::HasBottle1 => "has_bottle_1",
            Self::HasBottle2 => "has_bottle_2",
            Self::Eye => "eye",
            Self::Disarmed => "disarmed",
            Self::Conditional => "conditional",
            Self::Mode => "mode",
            Self::Inverted => "inverted",
            Self::Enabled => "enabled",
            Self::Eggs => "eggs",
            Self::Hatch => "hatch",
            Self::Hydration => "hydration",
            Self::Pickles => "pickles",
            Self::Leaves => "leaves",
            Self::Drag => "drag",
            Self::Bottom => "bottom",
            Self::HasBook => "has_book",
            Self::Attachment => "attachment",
            Self::SignalFire => "signal_fire",
            Self::Orientation => "orientation",
            Self::HoneyLevel => "honey_level",
            Self::Charges => "charges",
            Self::Candles => "candles",
            Self::SculkSensorPhase => "sculk_sensor_phase",
            Self::Bloom => "bloom",
            Self::CanSummon => "can_summon",
            Self::Shrieking => "shrieking",
            Self::CopperGolemPose => "copper_golem_pose",
            Self::Thickness => "thickness",
            Self::VerticalDirection => "vertical_direction",
            Self::Berries => "berries",
            Self::FlowerAmount => "flower_amount",
            Self::SegmentAmount => "segment_amount",
            Self::Tilt => "tilt",
            Self::Cracked => "cracked",
            Self::Crafting => "crafting",
            Self::Ominous => "ominous",
            Self::TrialSpawnerState => "trial_spawner_state",
            Self::VaultState => "vault_state",
            Self::Tip => "tip",
        }
    }
}
impl PropVal {
    pub fn key(self) -> PropKey {
        match self {
            Self::Snowy(_) => PropKey::Snowy,
            Self::Axis(_) => PropKey::Axis,
            Self::Stage(_) => PropKey::Stage,
            Self::Age(_) => PropKey::Age,
            Self::Hanging(_) => PropKey::Hanging,
            Self::Waterlogged(_) => PropKey::Waterlogged,
            Self::Level(_) => PropKey::Level,
            Self::Dusted(_) => PropKey::Dusted,
            Self::Distance(_) => PropKey::Distance,
            Self::Persistent(_) => PropKey::Persistent,
            Self::Facing(_) => PropKey::Facing,
            Self::Triggered(_) => PropKey::Triggered,
            Self::Instrument(_) => PropKey::Instrument,
            Self::Note(_) => PropKey::Note,
            Self::Powered(_) => PropKey::Powered,
            Self::Occupied(_) => PropKey::Occupied,
            Self::Part(_) => PropKey::Part,
            Self::Shape(_) => PropKey::Shape,
            Self::Extended(_) => PropKey::Extended,
            Self::Half(_) => PropKey::Half,
            Self::Short(_) => PropKey::Short,
            Self::Type(_) => PropKey::Type,
            Self::Unstable(_) => PropKey::Unstable,
            Self::Slot0Occupied(_) => PropKey::Slot0Occupied,
            Self::Slot1Occupied(_) => PropKey::Slot1Occupied,
            Self::Slot2Occupied(_) => PropKey::Slot2Occupied,
            Self::Slot3Occupied(_) => PropKey::Slot3Occupied,
            Self::Slot4Occupied(_) => PropKey::Slot4Occupied,
            Self::Slot5Occupied(_) => PropKey::Slot5Occupied,
            Self::SideChain(_) => PropKey::SideChain,
            Self::East(_) => PropKey::East,
            Self::North(_) => PropKey::North,
            Self::South(_) => PropKey::South,
            Self::Up(_) => PropKey::Up,
            Self::West(_) => PropKey::West,
            Self::CreakingHeartState(_) => PropKey::CreakingHeartState,
            Self::Natural(_) => PropKey::Natural,
            Self::Power(_) => PropKey::Power,
            Self::Moisture(_) => PropKey::Moisture,
            Self::Lit(_) => PropKey::Lit,
            Self::Rotation(_) => PropKey::Rotation,
            Self::Hinge(_) => PropKey::Hinge,
            Self::Open(_) => PropKey::Open,
            Self::Attached(_) => PropKey::Attached,
            Self::Face(_) => PropKey::Face,
            Self::Layers(_) => PropKey::Layers,
            Self::HasRecord(_) => PropKey::HasRecord,
            Self::Bites(_) => PropKey::Bites,
            Self::Delay(_) => PropKey::Delay,
            Self::Locked(_) => PropKey::Locked,
            Self::Down(_) => PropKey::Down,
            Self::InWall(_) => PropKey::InWall,
            Self::HasBottle0(_) => PropKey::HasBottle0,
            Self::HasBottle1(_) => PropKey::HasBottle1,
            Self::HasBottle2(_) => PropKey::HasBottle2,
            Self::Eye(_) => PropKey::Eye,
            Self::Disarmed(_) => PropKey::Disarmed,
            Self::Conditional(_) => PropKey::Conditional,
            Self::Mode(_) => PropKey::Mode,
            Self::Inverted(_) => PropKey::Inverted,
            Self::Enabled(_) => PropKey::Enabled,
            Self::Eggs(_) => PropKey::Eggs,
            Self::Hatch(_) => PropKey::Hatch,
            Self::Hydration(_) => PropKey::Hydration,
            Self::Pickles(_) => PropKey::Pickles,
            Self::Leaves(_) => PropKey::Leaves,
            Self::Drag(_) => PropKey::Drag,
            Self::Bottom(_) => PropKey::Bottom,
            Self::HasBook(_) => PropKey::HasBook,
            Self::Attachment(_) => PropKey::Attachment,
            Self::SignalFire(_) => PropKey::SignalFire,
            Self::Orientation(_) => PropKey::Orientation,
            Self::HoneyLevel(_) => PropKey::HoneyLevel,
            Self::Charges(_) => PropKey::Charges,
            Self::Candles(_) => PropKey::Candles,
            Self::SculkSensorPhase(_) => PropKey::SculkSensorPhase,
            Self::Bloom(_) => PropKey::Bloom,
            Self::CanSummon(_) => PropKey::CanSummon,
            Self::Shrieking(_) => PropKey::Shrieking,
            Self::CopperGolemPose(_) => PropKey::CopperGolemPose,
            Self::Thickness(_) => PropKey::Thickness,
            Self::VerticalDirection(_) => PropKey::VerticalDirection,
            Self::Berries(_) => PropKey::Berries,
            Self::FlowerAmount(_) => PropKey::FlowerAmount,
            Self::SegmentAmount(_) => PropKey::SegmentAmount,
            Self::Tilt(_) => PropKey::Tilt,
            Self::Cracked(_) => PropKey::Cracked,
            Self::Crafting(_) => PropKey::Crafting,
            Self::Ominous(_) => PropKey::Ominous,
            Self::TrialSpawnerState(_) => PropKey::TrialSpawnerState,
            Self::VaultState(_) => PropKey::VaultState,
            Self::Tip(_) => PropKey::Tip,
        }
    }
    pub fn parse_with_key(key: PropKey, s: &str) -> Option<Self> {
        match key {
            PropKey::Snowy => Some(Self::Snowy(bool::from_str(s).ok()?)),
            PropKey::Axis => Some(Self::Axis(Axis::from_str(s).ok()?)),
            PropKey::Stage => Some(Self::Stage(u8::from_str(s).ok()?)),
            PropKey::Age => Some(Self::Age(u8::from_str(s).ok()?)),
            PropKey::Hanging => Some(Self::Hanging(bool::from_str(s).ok()?)),
            PropKey::Waterlogged => Some(Self::Waterlogged(bool::from_str(s).ok()?)),
            PropKey::Level => Some(Self::Level(u8::from_str(s).ok()?)),
            PropKey::Dusted => Some(Self::Dusted(u8::from_str(s).ok()?)),
            PropKey::Distance => Some(Self::Distance(u8::from_str(s).ok()?)),
            PropKey::Persistent => Some(Self::Persistent(bool::from_str(s).ok()?)),
            PropKey::Facing => Some(Self::Facing(Direction::from_str(s).ok()?)),
            PropKey::Triggered => Some(Self::Triggered(bool::from_str(s).ok()?)),
            PropKey::Instrument => {
                Some(Self::Instrument(NoteBlockInstrument::from_str(s).ok()?))
            }
            PropKey::Note => Some(Self::Note(u8::from_str(s).ok()?)),
            PropKey::Powered => Some(Self::Powered(bool::from_str(s).ok()?)),
            PropKey::Occupied => Some(Self::Occupied(bool::from_str(s).ok()?)),
            PropKey::Part => Some(Self::Part(BedPart::from_str(s).ok()?)),
            PropKey::Shape => Some(Self::Shape(ShapePropVal::from_str(s).ok()?)),
            PropKey::Extended => Some(Self::Extended(bool::from_str(s).ok()?)),
            PropKey::Half => Some(Self::Half(HalfPropVal::from_str(s).ok()?)),
            PropKey::Short => Some(Self::Short(bool::from_str(s).ok()?)),
            PropKey::Type => Some(Self::Type(TypePropVal::from_str(s).ok()?)),
            PropKey::Unstable => Some(Self::Unstable(bool::from_str(s).ok()?)),
            PropKey::Slot0Occupied => Some(Self::Slot0Occupied(bool::from_str(s).ok()?)),
            PropKey::Slot1Occupied => Some(Self::Slot1Occupied(bool::from_str(s).ok()?)),
            PropKey::Slot2Occupied => Some(Self::Slot2Occupied(bool::from_str(s).ok()?)),
            PropKey::Slot3Occupied => Some(Self::Slot3Occupied(bool::from_str(s).ok()?)),
            PropKey::Slot4Occupied => Some(Self::Slot4Occupied(bool::from_str(s).ok()?)),
            PropKey::Slot5Occupied => Some(Self::Slot5Occupied(bool::from_str(s).ok()?)),
            PropKey::SideChain => Some(Self::SideChain(SideChainPart::from_str(s).ok()?)),
            PropKey::East => Some(Self::East(EastPropVal::from_str(s).ok()?)),
            PropKey::North => Some(Self::North(NorthPropVal::from_str(s).ok()?)),
            PropKey::South => Some(Self::South(SouthPropVal::from_str(s).ok()?)),
            PropKey::Up => Some(Self::Up(bool::from_str(s).ok()?)),
            PropKey::West => Some(Self::West(WestPropVal::from_str(s).ok()?)),
            PropKey::CreakingHeartState => {
                Some(Self::CreakingHeartState(CreakingHeartState::from_str(s).ok()?))
            }
            PropKey::Natural => Some(Self::Natural(bool::from_str(s).ok()?)),
            PropKey::Power => Some(Self::Power(u8::from_str(s).ok()?)),
            PropKey::Moisture => Some(Self::Moisture(u8::from_str(s).ok()?)),
            PropKey::Lit => Some(Self::Lit(bool::from_str(s).ok()?)),
            PropKey::Rotation => Some(Self::Rotation(u8::from_str(s).ok()?)),
            PropKey::Hinge => Some(Self::Hinge(DoorHingeSide::from_str(s).ok()?)),
            PropKey::Open => Some(Self::Open(bool::from_str(s).ok()?)),
            PropKey::Attached => Some(Self::Attached(bool::from_str(s).ok()?)),
            PropKey::Face => Some(Self::Face(AttachFace::from_str(s).ok()?)),
            PropKey::Layers => Some(Self::Layers(u8::from_str(s).ok()?)),
            PropKey::HasRecord => Some(Self::HasRecord(bool::from_str(s).ok()?)),
            PropKey::Bites => Some(Self::Bites(u8::from_str(s).ok()?)),
            PropKey::Delay => Some(Self::Delay(u8::from_str(s).ok()?)),
            PropKey::Locked => Some(Self::Locked(bool::from_str(s).ok()?)),
            PropKey::Down => Some(Self::Down(bool::from_str(s).ok()?)),
            PropKey::InWall => Some(Self::InWall(bool::from_str(s).ok()?)),
            PropKey::HasBottle0 => Some(Self::HasBottle0(bool::from_str(s).ok()?)),
            PropKey::HasBottle1 => Some(Self::HasBottle1(bool::from_str(s).ok()?)),
            PropKey::HasBottle2 => Some(Self::HasBottle2(bool::from_str(s).ok()?)),
            PropKey::Eye => Some(Self::Eye(bool::from_str(s).ok()?)),
            PropKey::Disarmed => Some(Self::Disarmed(bool::from_str(s).ok()?)),
            PropKey::Conditional => Some(Self::Conditional(bool::from_str(s).ok()?)),
            PropKey::Mode => Some(Self::Mode(ModePropVal::from_str(s).ok()?)),
            PropKey::Inverted => Some(Self::Inverted(bool::from_str(s).ok()?)),
            PropKey::Enabled => Some(Self::Enabled(bool::from_str(s).ok()?)),
            PropKey::Eggs => Some(Self::Eggs(u8::from_str(s).ok()?)),
            PropKey::Hatch => Some(Self::Hatch(u8::from_str(s).ok()?)),
            PropKey::Hydration => Some(Self::Hydration(u8::from_str(s).ok()?)),
            PropKey::Pickles => Some(Self::Pickles(u8::from_str(s).ok()?)),
            PropKey::Leaves => Some(Self::Leaves(BambooLeaves::from_str(s).ok()?)),
            PropKey::Drag => Some(Self::Drag(bool::from_str(s).ok()?)),
            PropKey::Bottom => Some(Self::Bottom(bool::from_str(s).ok()?)),
            PropKey::HasBook => Some(Self::HasBook(bool::from_str(s).ok()?)),
            PropKey::Attachment => {
                Some(Self::Attachment(BellAttachType::from_str(s).ok()?))
            }
            PropKey::SignalFire => Some(Self::SignalFire(bool::from_str(s).ok()?)),
            PropKey::Orientation => {
                Some(Self::Orientation(FrontAndTop::from_str(s).ok()?))
            }
            PropKey::HoneyLevel => Some(Self::HoneyLevel(u8::from_str(s).ok()?)),
            PropKey::Charges => Some(Self::Charges(u8::from_str(s).ok()?)),
            PropKey::Candles => Some(Self::Candles(u8::from_str(s).ok()?)),
            PropKey::SculkSensorPhase => {
                Some(Self::SculkSensorPhase(SculkSensorPhase::from_str(s).ok()?))
            }
            PropKey::Bloom => Some(Self::Bloom(bool::from_str(s).ok()?)),
            PropKey::CanSummon => Some(Self::CanSummon(bool::from_str(s).ok()?)),
            PropKey::Shrieking => Some(Self::Shrieking(bool::from_str(s).ok()?)),
            PropKey::CopperGolemPose => {
                Some(Self::CopperGolemPose(Pose::from_str(s).ok()?))
            }
            PropKey::Thickness => {
                Some(Self::Thickness(DripstoneThickness::from_str(s).ok()?))
            }
            PropKey::VerticalDirection => {
                Some(Self::VerticalDirection(Direction::from_str(s).ok()?))
            }
            PropKey::Berries => Some(Self::Berries(bool::from_str(s).ok()?)),
            PropKey::FlowerAmount => Some(Self::FlowerAmount(u8::from_str(s).ok()?)),
            PropKey::SegmentAmount => Some(Self::SegmentAmount(u8::from_str(s).ok()?)),
            PropKey::Tilt => Some(Self::Tilt(Tilt::from_str(s).ok()?)),
            PropKey::Cracked => Some(Self::Cracked(bool::from_str(s).ok()?)),
            PropKey::Crafting => Some(Self::Crafting(bool::from_str(s).ok()?)),
            PropKey::Ominous => Some(Self::Ominous(bool::from_str(s).ok()?)),
            PropKey::TrialSpawnerState => {
                Some(Self::TrialSpawnerState(TrialSpawnerState::from_str(s).ok()?))
            }
            PropKey::VaultState => Some(Self::VaultState(VaultState::from_str(s).ok()?)),
            PropKey::Tip => Some(Self::Tip(bool::from_str(s).ok()?)),
        }
    }
}
impl PropFilter {
    pub fn key(&self) -> PropKey {
        match self {
            Self::Snowy(_) => PropKey::Snowy,
            Self::Axis(_) => PropKey::Axis,
            Self::Stage(_) => PropKey::Stage,
            Self::Age(_) => PropKey::Age,
            Self::Hanging(_) => PropKey::Hanging,
            Self::Waterlogged(_) => PropKey::Waterlogged,
            Self::Level(_) => PropKey::Level,
            Self::Dusted(_) => PropKey::Dusted,
            Self::Distance(_) => PropKey::Distance,
            Self::Persistent(_) => PropKey::Persistent,
            Self::Facing(_) => PropKey::Facing,
            Self::Triggered(_) => PropKey::Triggered,
            Self::Instrument(_) => PropKey::Instrument,
            Self::Note(_) => PropKey::Note,
            Self::Powered(_) => PropKey::Powered,
            Self::Occupied(_) => PropKey::Occupied,
            Self::Part(_) => PropKey::Part,
            Self::Shape(_) => PropKey::Shape,
            Self::Extended(_) => PropKey::Extended,
            Self::Half(_) => PropKey::Half,
            Self::Short(_) => PropKey::Short,
            Self::Type(_) => PropKey::Type,
            Self::Unstable(_) => PropKey::Unstable,
            Self::Slot0Occupied(_) => PropKey::Slot0Occupied,
            Self::Slot1Occupied(_) => PropKey::Slot1Occupied,
            Self::Slot2Occupied(_) => PropKey::Slot2Occupied,
            Self::Slot3Occupied(_) => PropKey::Slot3Occupied,
            Self::Slot4Occupied(_) => PropKey::Slot4Occupied,
            Self::Slot5Occupied(_) => PropKey::Slot5Occupied,
            Self::SideChain(_) => PropKey::SideChain,
            Self::East(_) => PropKey::East,
            Self::North(_) => PropKey::North,
            Self::South(_) => PropKey::South,
            Self::Up(_) => PropKey::Up,
            Self::West(_) => PropKey::West,
            Self::CreakingHeartState(_) => PropKey::CreakingHeartState,
            Self::Natural(_) => PropKey::Natural,
            Self::Power(_) => PropKey::Power,
            Self::Moisture(_) => PropKey::Moisture,
            Self::Lit(_) => PropKey::Lit,
            Self::Rotation(_) => PropKey::Rotation,
            Self::Hinge(_) => PropKey::Hinge,
            Self::Open(_) => PropKey::Open,
            Self::Attached(_) => PropKey::Attached,
            Self::Face(_) => PropKey::Face,
            Self::Layers(_) => PropKey::Layers,
            Self::HasRecord(_) => PropKey::HasRecord,
            Self::Bites(_) => PropKey::Bites,
            Self::Delay(_) => PropKey::Delay,
            Self::Locked(_) => PropKey::Locked,
            Self::Down(_) => PropKey::Down,
            Self::InWall(_) => PropKey::InWall,
            Self::HasBottle0(_) => PropKey::HasBottle0,
            Self::HasBottle1(_) => PropKey::HasBottle1,
            Self::HasBottle2(_) => PropKey::HasBottle2,
            Self::Eye(_) => PropKey::Eye,
            Self::Disarmed(_) => PropKey::Disarmed,
            Self::Conditional(_) => PropKey::Conditional,
            Self::Mode(_) => PropKey::Mode,
            Self::Inverted(_) => PropKey::Inverted,
            Self::Enabled(_) => PropKey::Enabled,
            Self::Eggs(_) => PropKey::Eggs,
            Self::Hatch(_) => PropKey::Hatch,
            Self::Hydration(_) => PropKey::Hydration,
            Self::Pickles(_) => PropKey::Pickles,
            Self::Leaves(_) => PropKey::Leaves,
            Self::Drag(_) => PropKey::Drag,
            Self::Bottom(_) => PropKey::Bottom,
            Self::HasBook(_) => PropKey::HasBook,
            Self::Attachment(_) => PropKey::Attachment,
            Self::SignalFire(_) => PropKey::SignalFire,
            Self::Orientation(_) => PropKey::Orientation,
            Self::HoneyLevel(_) => PropKey::HoneyLevel,
            Self::Charges(_) => PropKey::Charges,
            Self::Candles(_) => PropKey::Candles,
            Self::SculkSensorPhase(_) => PropKey::SculkSensorPhase,
            Self::Bloom(_) => PropKey::Bloom,
            Self::CanSummon(_) => PropKey::CanSummon,
            Self::Shrieking(_) => PropKey::Shrieking,
            Self::CopperGolemPose(_) => PropKey::CopperGolemPose,
            Self::Thickness(_) => PropKey::Thickness,
            Self::VerticalDirection(_) => PropKey::VerticalDirection,
            Self::Berries(_) => PropKey::Berries,
            Self::FlowerAmount(_) => PropKey::FlowerAmount,
            Self::SegmentAmount(_) => PropKey::SegmentAmount,
            Self::Tilt(_) => PropKey::Tilt,
            Self::Cracked(_) => PropKey::Cracked,
            Self::Crafting(_) => PropKey::Crafting,
            Self::Ominous(_) => PropKey::Ominous,
            Self::TrialSpawnerState(_) => PropKey::TrialSpawnerState,
            Self::VaultState(_) => PropKey::VaultState,
            Self::Tip(_) => PropKey::Tip,
        }
    }
    pub fn test(&self, val: PropVal) -> bool {
        match (self, val) {
            (Self::Snowy(values), PropVal::Snowy(value)) => values.contains(&value),
            (Self::Axis(values), PropVal::Axis(value)) => values.contains(&value),
            (Self::Stage(values), PropVal::Stage(value)) => values.contains(&value),
            (Self::Age(values), PropVal::Age(value)) => values.contains(&value),
            (Self::Hanging(values), PropVal::Hanging(value)) => values.contains(&value),
            (Self::Waterlogged(values), PropVal::Waterlogged(value)) => {
                values.contains(&value)
            }
            (Self::Level(values), PropVal::Level(value)) => values.contains(&value),
            (Self::Dusted(values), PropVal::Dusted(value)) => values.contains(&value),
            (Self::Distance(values), PropVal::Distance(value)) => values.contains(&value),
            (Self::Persistent(values), PropVal::Persistent(value)) => {
                values.contains(&value)
            }
            (Self::Facing(values), PropVal::Facing(value)) => values.contains(&value),
            (Self::Triggered(values), PropVal::Triggered(value)) => {
                values.contains(&value)
            }
            (Self::Instrument(values), PropVal::Instrument(value)) => {
                values.contains(&value)
            }
            (Self::Note(values), PropVal::Note(value)) => values.contains(&value),
            (Self::Powered(values), PropVal::Powered(value)) => values.contains(&value),
            (Self::Occupied(values), PropVal::Occupied(value)) => values.contains(&value),
            (Self::Part(values), PropVal::Part(value)) => values.contains(&value),
            (Self::Shape(values), PropVal::Shape(value)) => values.contains(&value),
            (Self::Extended(values), PropVal::Extended(value)) => values.contains(&value),
            (Self::Half(values), PropVal::Half(value)) => values.contains(&value),
            (Self::Short(values), PropVal::Short(value)) => values.contains(&value),
            (Self::Type(values), PropVal::Type(value)) => values.contains(&value),
            (Self::Unstable(values), PropVal::Unstable(value)) => values.contains(&value),
            (Self::Slot0Occupied(values), PropVal::Slot0Occupied(value)) => {
                values.contains(&value)
            }
            (Self::Slot1Occupied(values), PropVal::Slot1Occupied(value)) => {
                values.contains(&value)
            }
            (Self::Slot2Occupied(values), PropVal::Slot2Occupied(value)) => {
                values.contains(&value)
            }
            (Self::Slot3Occupied(values), PropVal::Slot3Occupied(value)) => {
                values.contains(&value)
            }
            (Self::Slot4Occupied(values), PropVal::Slot4Occupied(value)) => {
                values.contains(&value)
            }
            (Self::Slot5Occupied(values), PropVal::Slot5Occupied(value)) => {
                values.contains(&value)
            }
            (Self::SideChain(values), PropVal::SideChain(value)) => {
                values.contains(&value)
            }
            (Self::East(values), PropVal::East(value)) => values.contains(&value),
            (Self::North(values), PropVal::North(value)) => values.contains(&value),
            (Self::South(values), PropVal::South(value)) => values.contains(&value),
            (Self::Up(values), PropVal::Up(value)) => values.contains(&value),
            (Self::West(values), PropVal::West(value)) => values.contains(&value),
            (Self::CreakingHeartState(values), PropVal::CreakingHeartState(value)) => {
                values.contains(&value)
            }
            (Self::Natural(values), PropVal::Natural(value)) => values.contains(&value),
            (Self::Power(values), PropVal::Power(value)) => values.contains(&value),
            (Self::Moisture(values), PropVal::Moisture(value)) => values.contains(&value),
            (Self::Lit(values), PropVal::Lit(value)) => values.contains(&value),
            (Self::Rotation(values), PropVal::Rotation(value)) => values.contains(&value),
            (Self::Hinge(values), PropVal::Hinge(value)) => values.contains(&value),
            (Self::Open(values), PropVal::Open(value)) => values.contains(&value),
            (Self::Attached(values), PropVal::Attached(value)) => values.contains(&value),
            (Self::Face(values), PropVal::Face(value)) => values.contains(&value),
            (Self::Layers(values), PropVal::Layers(value)) => values.contains(&value),
            (Self::HasRecord(values), PropVal::HasRecord(value)) => {
                values.contains(&value)
            }
            (Self::Bites(values), PropVal::Bites(value)) => values.contains(&value),
            (Self::Delay(values), PropVal::Delay(value)) => values.contains(&value),
            (Self::Locked(values), PropVal::Locked(value)) => values.contains(&value),
            (Self::Down(values), PropVal::Down(value)) => values.contains(&value),
            (Self::InWall(values), PropVal::InWall(value)) => values.contains(&value),
            (Self::HasBottle0(values), PropVal::HasBottle0(value)) => {
                values.contains(&value)
            }
            (Self::HasBottle1(values), PropVal::HasBottle1(value)) => {
                values.contains(&value)
            }
            (Self::HasBottle2(values), PropVal::HasBottle2(value)) => {
                values.contains(&value)
            }
            (Self::Eye(values), PropVal::Eye(value)) => values.contains(&value),
            (Self::Disarmed(values), PropVal::Disarmed(value)) => values.contains(&value),
            (Self::Conditional(values), PropVal::Conditional(value)) => {
                values.contains(&value)
            }
            (Self::Mode(values), PropVal::Mode(value)) => values.contains(&value),
            (Self::Inverted(values), PropVal::Inverted(value)) => values.contains(&value),
            (Self::Enabled(values), PropVal::Enabled(value)) => values.contains(&value),
            (Self::Eggs(values), PropVal::Eggs(value)) => values.contains(&value),
            (Self::Hatch(values), PropVal::Hatch(value)) => values.contains(&value),
            (Self::Hydration(values), PropVal::Hydration(value)) => {
                values.contains(&value)
            }
            (Self::Pickles(values), PropVal::Pickles(value)) => values.contains(&value),
            (Self::Leaves(values), PropVal::Leaves(value)) => values.contains(&value),
            (Self::Drag(values), PropVal::Drag(value)) => values.contains(&value),
            (Self::Bottom(values), PropVal::Bottom(value)) => values.contains(&value),
            (Self::HasBook(values), PropVal::HasBook(value)) => values.contains(&value),
            (Self::Attachment(values), PropVal::Attachment(value)) => {
                values.contains(&value)
            }
            (Self::SignalFire(values), PropVal::SignalFire(value)) => {
                values.contains(&value)
            }
            (Self::Orientation(values), PropVal::Orientation(value)) => {
                values.contains(&value)
            }
            (Self::HoneyLevel(values), PropVal::HoneyLevel(value)) => {
                values.contains(&value)
            }
            (Self::Charges(values), PropVal::Charges(value)) => values.contains(&value),
            (Self::Candles(values), PropVal::Candles(value)) => values.contains(&value),
            (Self::SculkSensorPhase(values), PropVal::SculkSensorPhase(value)) => {
                values.contains(&value)
            }
            (Self::Bloom(values), PropVal::Bloom(value)) => values.contains(&value),
            (Self::CanSummon(values), PropVal::CanSummon(value)) => {
                values.contains(&value)
            }
            (Self::Shrieking(values), PropVal::Shrieking(value)) => {
                values.contains(&value)
            }
            (Self::CopperGolemPose(values), PropVal::CopperGolemPose(value)) => {
                values.contains(&value)
            }
            (Self::Thickness(values), PropVal::Thickness(value)) => {
                values.contains(&value)
            }
            (Self::VerticalDirection(values), PropVal::VerticalDirection(value)) => {
                values.contains(&value)
            }
            (Self::Berries(values), PropVal::Berries(value)) => values.contains(&value),
            (Self::FlowerAmount(values), PropVal::FlowerAmount(value)) => {
                values.contains(&value)
            }
            (Self::SegmentAmount(values), PropVal::SegmentAmount(value)) => {
                values.contains(&value)
            }
            (Self::Tilt(values), PropVal::Tilt(value)) => values.contains(&value),
            (Self::Cracked(values), PropVal::Cracked(value)) => values.contains(&value),
            (Self::Crafting(values), PropVal::Crafting(value)) => values.contains(&value),
            (Self::Ominous(values), PropVal::Ominous(value)) => values.contains(&value),
            (Self::TrialSpawnerState(values), PropVal::TrialSpawnerState(value)) => {
                values.contains(&value)
            }
            (Self::VaultState(values), PropVal::VaultState(value)) => {
                values.contains(&value)
            }
            (Self::Tip(values), PropVal::Tip(value)) => values.contains(&value),
            _ => false,
        }
    }
    pub fn parse_with_key(key: PropKey, s: &str) -> Option<Self> {
        let parts = s.split("|");
        match key {
            PropKey::Snowy => {
                Some(
                    Self::Snowy(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Axis => {
                Some(
                    Self::Axis(
                        parts
                            .map(|part| Axis::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Stage => {
                Some(
                    Self::Stage(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Age => {
                Some(
                    Self::Age(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Hanging => {
                Some(
                    Self::Hanging(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Waterlogged => {
                Some(
                    Self::Waterlogged(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Level => {
                Some(
                    Self::Level(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Dusted => {
                Some(
                    Self::Dusted(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Distance => {
                Some(
                    Self::Distance(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Persistent => {
                Some(
                    Self::Persistent(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Facing => {
                Some(
                    Self::Facing(
                        parts
                            .map(|part| Direction::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Triggered => {
                Some(
                    Self::Triggered(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Instrument => {
                Some(
                    Self::Instrument(
                        parts
                            .map(|part| NoteBlockInstrument::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Note => {
                Some(
                    Self::Note(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Powered => {
                Some(
                    Self::Powered(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Occupied => {
                Some(
                    Self::Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Part => {
                Some(
                    Self::Part(
                        parts
                            .map(|part| BedPart::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Shape => {
                Some(
                    Self::Shape(
                        parts
                            .map(|part| ShapePropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Extended => {
                Some(
                    Self::Extended(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Half => {
                Some(
                    Self::Half(
                        parts
                            .map(|part| HalfPropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Short => {
                Some(
                    Self::Short(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Type => {
                Some(
                    Self::Type(
                        parts
                            .map(|part| TypePropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Unstable => {
                Some(
                    Self::Unstable(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot0Occupied => {
                Some(
                    Self::Slot0Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot1Occupied => {
                Some(
                    Self::Slot1Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot2Occupied => {
                Some(
                    Self::Slot2Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot3Occupied => {
                Some(
                    Self::Slot3Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot4Occupied => {
                Some(
                    Self::Slot4Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Slot5Occupied => {
                Some(
                    Self::Slot5Occupied(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::SideChain => {
                Some(
                    Self::SideChain(
                        parts
                            .map(|part| SideChainPart::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::East => {
                Some(
                    Self::East(
                        parts
                            .map(|part| EastPropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::North => {
                Some(
                    Self::North(
                        parts
                            .map(|part| NorthPropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::South => {
                Some(
                    Self::South(
                        parts
                            .map(|part| SouthPropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Up => {
                Some(
                    Self::Up(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::West => {
                Some(
                    Self::West(
                        parts
                            .map(|part| WestPropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::CreakingHeartState => {
                Some(
                    Self::CreakingHeartState(
                        parts
                            .map(|part| CreakingHeartState::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Natural => {
                Some(
                    Self::Natural(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Power => {
                Some(
                    Self::Power(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Moisture => {
                Some(
                    Self::Moisture(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Lit => {
                Some(
                    Self::Lit(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Rotation => {
                Some(
                    Self::Rotation(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Hinge => {
                Some(
                    Self::Hinge(
                        parts
                            .map(|part| DoorHingeSide::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Open => {
                Some(
                    Self::Open(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Attached => {
                Some(
                    Self::Attached(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Face => {
                Some(
                    Self::Face(
                        parts
                            .map(|part| AttachFace::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Layers => {
                Some(
                    Self::Layers(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HasRecord => {
                Some(
                    Self::HasRecord(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Bites => {
                Some(
                    Self::Bites(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Delay => {
                Some(
                    Self::Delay(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Locked => {
                Some(
                    Self::Locked(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Down => {
                Some(
                    Self::Down(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::InWall => {
                Some(
                    Self::InWall(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HasBottle0 => {
                Some(
                    Self::HasBottle0(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HasBottle1 => {
                Some(
                    Self::HasBottle1(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HasBottle2 => {
                Some(
                    Self::HasBottle2(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Eye => {
                Some(
                    Self::Eye(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Disarmed => {
                Some(
                    Self::Disarmed(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Conditional => {
                Some(
                    Self::Conditional(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Mode => {
                Some(
                    Self::Mode(
                        parts
                            .map(|part| ModePropVal::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Inverted => {
                Some(
                    Self::Inverted(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Enabled => {
                Some(
                    Self::Enabled(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Eggs => {
                Some(
                    Self::Eggs(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Hatch => {
                Some(
                    Self::Hatch(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Hydration => {
                Some(
                    Self::Hydration(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Pickles => {
                Some(
                    Self::Pickles(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Leaves => {
                Some(
                    Self::Leaves(
                        parts
                            .map(|part| BambooLeaves::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Drag => {
                Some(
                    Self::Drag(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Bottom => {
                Some(
                    Self::Bottom(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HasBook => {
                Some(
                    Self::HasBook(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Attachment => {
                Some(
                    Self::Attachment(
                        parts
                            .map(|part| BellAttachType::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::SignalFire => {
                Some(
                    Self::SignalFire(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Orientation => {
                Some(
                    Self::Orientation(
                        parts
                            .map(|part| FrontAndTop::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::HoneyLevel => {
                Some(
                    Self::HoneyLevel(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Charges => {
                Some(
                    Self::Charges(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Candles => {
                Some(
                    Self::Candles(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::SculkSensorPhase => {
                Some(
                    Self::SculkSensorPhase(
                        parts
                            .map(|part| SculkSensorPhase::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Bloom => {
                Some(
                    Self::Bloom(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::CanSummon => {
                Some(
                    Self::CanSummon(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Shrieking => {
                Some(
                    Self::Shrieking(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::CopperGolemPose => {
                Some(
                    Self::CopperGolemPose(
                        parts
                            .map(|part| Pose::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Thickness => {
                Some(
                    Self::Thickness(
                        parts
                            .map(|part| DripstoneThickness::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::VerticalDirection => {
                Some(
                    Self::VerticalDirection(
                        parts
                            .map(|part| Direction::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Berries => {
                Some(
                    Self::Berries(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::FlowerAmount => {
                Some(
                    Self::FlowerAmount(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::SegmentAmount => {
                Some(
                    Self::SegmentAmount(
                        parts
                            .map(|part| u8::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Tilt => {
                Some(
                    Self::Tilt(
                        parts
                            .map(|part| Tilt::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Cracked => {
                Some(
                    Self::Cracked(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Crafting => {
                Some(
                    Self::Crafting(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Ominous => {
                Some(
                    Self::Ominous(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::TrialSpawnerState => {
                Some(
                    Self::TrialSpawnerState(
                        parts
                            .map(|part| TrialSpawnerState::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::VaultState => {
                Some(
                    Self::VaultState(
                        parts
                            .map(|part| VaultState::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
            PropKey::Tip => {
                Some(
                    Self::Tip(
                        parts
                            .map(|part| bool::from_str(part).ok())
                            .collect::<Option<Box<_>>>()?,
                    ),
                )
            }
        }
    }
}
impl FromStr for PropKey {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "snowy" => Ok(Self::Snowy),
            "axis" => Ok(Self::Axis),
            "stage" => Ok(Self::Stage),
            "age" => Ok(Self::Age),
            "hanging" => Ok(Self::Hanging),
            "waterlogged" => Ok(Self::Waterlogged),
            "level" => Ok(Self::Level),
            "dusted" => Ok(Self::Dusted),
            "distance" => Ok(Self::Distance),
            "persistent" => Ok(Self::Persistent),
            "facing" => Ok(Self::Facing),
            "triggered" => Ok(Self::Triggered),
            "instrument" => Ok(Self::Instrument),
            "note" => Ok(Self::Note),
            "powered" => Ok(Self::Powered),
            "occupied" => Ok(Self::Occupied),
            "part" => Ok(Self::Part),
            "shape" => Ok(Self::Shape),
            "extended" => Ok(Self::Extended),
            "half" => Ok(Self::Half),
            "short" => Ok(Self::Short),
            "type" => Ok(Self::Type),
            "unstable" => Ok(Self::Unstable),
            "slot_0_occupied" => Ok(Self::Slot0Occupied),
            "slot_1_occupied" => Ok(Self::Slot1Occupied),
            "slot_2_occupied" => Ok(Self::Slot2Occupied),
            "slot_3_occupied" => Ok(Self::Slot3Occupied),
            "slot_4_occupied" => Ok(Self::Slot4Occupied),
            "slot_5_occupied" => Ok(Self::Slot5Occupied),
            "side_chain" => Ok(Self::SideChain),
            "east" => Ok(Self::East),
            "north" => Ok(Self::North),
            "south" => Ok(Self::South),
            "up" => Ok(Self::Up),
            "west" => Ok(Self::West),
            "creaking_heart_state" => Ok(Self::CreakingHeartState),
            "natural" => Ok(Self::Natural),
            "power" => Ok(Self::Power),
            "moisture" => Ok(Self::Moisture),
            "lit" => Ok(Self::Lit),
            "rotation" => Ok(Self::Rotation),
            "hinge" => Ok(Self::Hinge),
            "open" => Ok(Self::Open),
            "attached" => Ok(Self::Attached),
            "face" => Ok(Self::Face),
            "layers" => Ok(Self::Layers),
            "has_record" => Ok(Self::HasRecord),
            "bites" => Ok(Self::Bites),
            "delay" => Ok(Self::Delay),
            "locked" => Ok(Self::Locked),
            "down" => Ok(Self::Down),
            "in_wall" => Ok(Self::InWall),
            "has_bottle_0" => Ok(Self::HasBottle0),
            "has_bottle_1" => Ok(Self::HasBottle1),
            "has_bottle_2" => Ok(Self::HasBottle2),
            "eye" => Ok(Self::Eye),
            "disarmed" => Ok(Self::Disarmed),
            "conditional" => Ok(Self::Conditional),
            "mode" => Ok(Self::Mode),
            "inverted" => Ok(Self::Inverted),
            "enabled" => Ok(Self::Enabled),
            "eggs" => Ok(Self::Eggs),
            "hatch" => Ok(Self::Hatch),
            "hydration" => Ok(Self::Hydration),
            "pickles" => Ok(Self::Pickles),
            "leaves" => Ok(Self::Leaves),
            "drag" => Ok(Self::Drag),
            "bottom" => Ok(Self::Bottom),
            "has_book" => Ok(Self::HasBook),
            "attachment" => Ok(Self::Attachment),
            "signal_fire" => Ok(Self::SignalFire),
            "orientation" => Ok(Self::Orientation),
            "honey_level" => Ok(Self::HoneyLevel),
            "charges" => Ok(Self::Charges),
            "candles" => Ok(Self::Candles),
            "sculk_sensor_phase" => Ok(Self::SculkSensorPhase),
            "bloom" => Ok(Self::Bloom),
            "can_summon" => Ok(Self::CanSummon),
            "shrieking" => Ok(Self::Shrieking),
            "copper_golem_pose" => Ok(Self::CopperGolemPose),
            "thickness" => Ok(Self::Thickness),
            "vertical_direction" => Ok(Self::VerticalDirection),
            "berries" => Ok(Self::Berries),
            "flower_amount" => Ok(Self::FlowerAmount),
            "segment_amount" => Ok(Self::SegmentAmount),
            "tilt" => Ok(Self::Tilt),
            "cracked" => Ok(Self::Cracked),
            "crafting" => Ok(Self::Crafting),
            "ominous" => Ok(Self::Ominous),
            "trial_spawner_state" => Ok(Self::TrialSpawnerState),
            "vault_state" => Ok(Self::VaultState),
            "tip" => Ok(Self::Tip),
            _ => Err(()),
        }
    }
}
impl FromStr for PropVal {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        let parts: Vec<_> = s.split("=").collect();
        if parts.len() != 2 {
            return Err(());
        }
        let key = PropKey::from_str(parts[0])?;
        Self::parse_with_key(key, parts[1]).ok_or(())
    }
}
