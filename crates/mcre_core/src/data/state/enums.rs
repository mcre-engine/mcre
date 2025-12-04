pub use crate::{Axis, Direction};
use core::str::FromStr;
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NoteBlockInstrument {
    Harp = 0u8,
    Basedrum = 1u8,
    Snare = 2u8,
    Hat = 3u8,
    Bass = 4u8,
    Flute = 5u8,
    Bell = 6u8,
    Guitar = 7u8,
    Chime = 8u8,
    Xylophone = 9u8,
    IronXylophone = 10u8,
    CowBell = 11u8,
    Didgeridoo = 12u8,
    Bit = 13u8,
    Banjo = 14u8,
    Pling = 15u8,
    Zombie = 16u8,
    Skeleton = 17u8,
    Creeper = 18u8,
    Dragon = 19u8,
    WitherSkeleton = 20u8,
    Piglin = 21u8,
    CustomHead = 22u8,
}
impl NoteBlockInstrument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Harp => "harp",
            Self::Basedrum => "basedrum",
            Self::Snare => "snare",
            Self::Hat => "hat",
            Self::Bass => "bass",
            Self::Flute => "flute",
            Self::Bell => "bell",
            Self::Guitar => "guitar",
            Self::Chime => "chime",
            Self::Xylophone => "xylophone",
            Self::IronXylophone => "iron_xylophone",
            Self::CowBell => "cow_bell",
            Self::Didgeridoo => "didgeridoo",
            Self::Bit => "bit",
            Self::Banjo => "banjo",
            Self::Pling => "pling",
            Self::Zombie => "zombie",
            Self::Skeleton => "skeleton",
            Self::Creeper => "creeper",
            Self::Dragon => "dragon",
            Self::WitherSkeleton => "wither_skeleton",
            Self::Piglin => "piglin",
            Self::CustomHead => "custom_head",
        }
    }
}
impl FromStr for NoteBlockInstrument {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "harp" => Ok(Self::Harp),
            "basedrum" => Ok(Self::Basedrum),
            "snare" => Ok(Self::Snare),
            "hat" => Ok(Self::Hat),
            "bass" => Ok(Self::Bass),
            "flute" => Ok(Self::Flute),
            "bell" => Ok(Self::Bell),
            "guitar" => Ok(Self::Guitar),
            "chime" => Ok(Self::Chime),
            "xylophone" => Ok(Self::Xylophone),
            "iron_xylophone" => Ok(Self::IronXylophone),
            "cow_bell" => Ok(Self::CowBell),
            "didgeridoo" => Ok(Self::Didgeridoo),
            "bit" => Ok(Self::Bit),
            "banjo" => Ok(Self::Banjo),
            "pling" => Ok(Self::Pling),
            "zombie" => Ok(Self::Zombie),
            "skeleton" => Ok(Self::Skeleton),
            "creeper" => Ok(Self::Creeper),
            "dragon" => Ok(Self::Dragon),
            "wither_skeleton" => Ok(Self::WitherSkeleton),
            "piglin" => Ok(Self::Piglin),
            "custom_head" => Ok(Self::CustomHead),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BedPart {
    Head = 0u8,
    Foot = 1u8,
}
impl BedPart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Foot => "foot",
        }
    }
}
impl FromStr for BedPart {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "head" => Ok(Self::Head),
            "foot" => Ok(Self::Foot),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RailShape {
    NorthSouth = 0u8,
    EastWest = 1u8,
    AscendingEast = 2u8,
    AscendingWest = 3u8,
    AscendingNorth = 4u8,
    AscendingSouth = 5u8,
    SouthEast = 6u8,
    SouthWest = 7u8,
    NorthWest = 8u8,
    NorthEast = 9u8,
}
impl RailShape {
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
        }
    }
}
impl FromStr for RailShape {
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
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DoubleBlockHalf {
    Upper = 0u8,
    Lower = 1u8,
}
impl DoubleBlockHalf {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
        }
    }
}
impl FromStr for DoubleBlockHalf {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "upper" => Ok(Self::Upper),
            "lower" => Ok(Self::Lower),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PistonType {
    Normal = 0u8,
    Sticky = 1u8,
}
impl PistonType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Sticky => "sticky",
        }
    }
}
impl FromStr for PistonType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "normal" => Ok(Self::Normal),
            "sticky" => Ok(Self::Sticky),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SideChainPart {
    Unconnected = 0u8,
    Right = 1u8,
    Center = 2u8,
    Left = 3u8,
}
impl SideChainPart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unconnected => "unconnected",
            Self::Right => "right",
            Self::Center => "center",
            Self::Left => "left",
        }
    }
}
impl FromStr for SideChainPart {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "unconnected" => Ok(Self::Unconnected),
            "right" => Ok(Self::Right),
            "center" => Ok(Self::Center),
            "left" => Ok(Self::Left),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CreakingHeartState {
    Uprooted = 0u8,
    Dormant = 1u8,
    Awake = 2u8,
}
impl CreakingHeartState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uprooted => "uprooted",
            Self::Dormant => "dormant",
            Self::Awake => "awake",
        }
    }
}
impl FromStr for CreakingHeartState {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "uprooted" => Ok(Self::Uprooted),
            "dormant" => Ok(Self::Dormant),
            "awake" => Ok(Self::Awake),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Half {
    Top = 0u8,
    Bottom = 1u8,
}
impl Half {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}
impl FromStr for Half {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StairsShape {
    Straight = 0u8,
    InnerLeft = 1u8,
    InnerRight = 2u8,
    OuterLeft = 3u8,
    OuterRight = 4u8,
}
impl StairsShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Straight => "straight",
            Self::InnerLeft => "inner_left",
            Self::InnerRight => "inner_right",
            Self::OuterLeft => "outer_left",
            Self::OuterRight => "outer_right",
        }
    }
}
impl FromStr for StairsShape {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "straight" => Ok(Self::Straight),
            "inner_left" => Ok(Self::InnerLeft),
            "inner_right" => Ok(Self::InnerRight),
            "outer_left" => Ok(Self::OuterLeft),
            "outer_right" => Ok(Self::OuterRight),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ChestType {
    Single = 0u8,
    Left = 1u8,
    Right = 2u8,
}
impl ChestType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}
impl FromStr for ChestType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "single" => Ok(Self::Single),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RedstoneSide {
    Up = 0u8,
    Side = 1u8,
    None = 2u8,
}
impl RedstoneSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Side => "side",
            Self::None => "none",
        }
    }
}
impl FromStr for RedstoneSide {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "up" => Ok(Self::Up),
            "side" => Ok(Self::Side),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DoorHingeSide {
    Left = 0u8,
    Right = 1u8,
}
impl DoorHingeSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}
impl FromStr for DoorHingeSide {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AttachFace {
    Floor = 0u8,
    Wall = 1u8,
    Ceiling = 2u8,
}
impl AttachFace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Wall => "wall",
            Self::Ceiling => "ceiling",
        }
    }
}
impl FromStr for AttachFace {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "floor" => Ok(Self::Floor),
            "wall" => Ok(Self::Wall),
            "ceiling" => Ok(Self::Ceiling),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SlabType {
    Top = 0u8,
    Bottom = 1u8,
    Double = 2u8,
}
impl SlabType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Double => "double",
        }
    }
}
impl FromStr for SlabType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            "double" => Ok(Self::Double),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WallSide {
    None = 0u8,
    Low = 1u8,
    Tall = 2u8,
}
impl WallSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Tall => "tall",
        }
    }
}
impl FromStr for WallSide {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "none" => Ok(Self::None),
            "low" => Ok(Self::Low),
            "tall" => Ok(Self::Tall),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ComparatorMode {
    Compare = 0u8,
    Subtract = 1u8,
}
impl ComparatorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compare => "compare",
            Self::Subtract => "subtract",
        }
    }
}
impl FromStr for ComparatorMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "compare" => Ok(Self::Compare),
            "subtract" => Ok(Self::Subtract),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BambooLeaves {
    None = 0u8,
    Small = 1u8,
    Large = 2u8,
}
impl BambooLeaves {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Small => "small",
            Self::Large => "large",
        }
    }
}
impl FromStr for BambooLeaves {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "none" => Ok(Self::None),
            "small" => Ok(Self::Small),
            "large" => Ok(Self::Large),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BellAttachType {
    Floor = 0u8,
    Ceiling = 1u8,
    SingleWall = 2u8,
    DoubleWall = 3u8,
}
impl BellAttachType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Ceiling => "ceiling",
            Self::SingleWall => "single_wall",
            Self::DoubleWall => "double_wall",
        }
    }
}
impl FromStr for BellAttachType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "floor" => Ok(Self::Floor),
            "ceiling" => Ok(Self::Ceiling),
            "single_wall" => Ok(Self::SingleWall),
            "double_wall" => Ok(Self::DoubleWall),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StructureMode {
    Save = 0u8,
    Load = 1u8,
    Corner = 2u8,
    Data = 3u8,
}
impl StructureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Load => "load",
            Self::Corner => "corner",
            Self::Data => "data",
        }
    }
}
impl FromStr for StructureMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "save" => Ok(Self::Save),
            "load" => Ok(Self::Load),
            "corner" => Ok(Self::Corner),
            "data" => Ok(Self::Data),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FrontAndTop {
    DownEast = 0u8,
    DownNorth = 1u8,
    DownSouth = 2u8,
    DownWest = 3u8,
    UpEast = 4u8,
    UpNorth = 5u8,
    UpSouth = 6u8,
    UpWest = 7u8,
    WestUp = 8u8,
    EastUp = 9u8,
    NorthUp = 10u8,
    SouthUp = 11u8,
}
impl FrontAndTop {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DownEast => "down_east",
            Self::DownNorth => "down_north",
            Self::DownSouth => "down_south",
            Self::DownWest => "down_west",
            Self::UpEast => "up_east",
            Self::UpNorth => "up_north",
            Self::UpSouth => "up_south",
            Self::UpWest => "up_west",
            Self::WestUp => "west_up",
            Self::EastUp => "east_up",
            Self::NorthUp => "north_up",
            Self::SouthUp => "south_up",
        }
    }
}
impl FromStr for FrontAndTop {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "down_east" => Ok(Self::DownEast),
            "down_north" => Ok(Self::DownNorth),
            "down_south" => Ok(Self::DownSouth),
            "down_west" => Ok(Self::DownWest),
            "up_east" => Ok(Self::UpEast),
            "up_north" => Ok(Self::UpNorth),
            "up_south" => Ok(Self::UpSouth),
            "up_west" => Ok(Self::UpWest),
            "west_up" => Ok(Self::WestUp),
            "east_up" => Ok(Self::EastUp),
            "north_up" => Ok(Self::NorthUp),
            "south_up" => Ok(Self::SouthUp),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TestBlockMode {
    Start = 0u8,
    Log = 1u8,
    Fail = 2u8,
    Accept = 3u8,
}
impl TestBlockMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Log => "log",
            Self::Fail => "fail",
            Self::Accept => "accept",
        }
    }
}
impl FromStr for TestBlockMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "start" => Ok(Self::Start),
            "log" => Ok(Self::Log),
            "fail" => Ok(Self::Fail),
            "accept" => Ok(Self::Accept),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SculkSensorPhase {
    Inactive = 0u8,
    Active = 1u8,
    Cooldown = 2u8,
}
impl SculkSensorPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Cooldown => "cooldown",
        }
    }
}
impl FromStr for SculkSensorPhase {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "inactive" => Ok(Self::Inactive),
            "active" => Ok(Self::Active),
            "cooldown" => Ok(Self::Cooldown),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pose {
    Standing = 0u8,
    Sitting = 1u8,
    Running = 2u8,
    Star = 3u8,
}
impl Pose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Sitting => "sitting",
            Self::Running => "running",
            Self::Star => "star",
        }
    }
}
impl FromStr for Pose {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "standing" => Ok(Self::Standing),
            "sitting" => Ok(Self::Sitting),
            "running" => Ok(Self::Running),
            "star" => Ok(Self::Star),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DripstoneThickness {
    TipMerge = 0u8,
    Tip = 1u8,
    Frustum = 2u8,
    Middle = 3u8,
    Base = 4u8,
}
impl DripstoneThickness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TipMerge => "tip_merge",
            Self::Tip => "tip",
            Self::Frustum => "frustum",
            Self::Middle => "middle",
            Self::Base => "base",
        }
    }
}
impl FromStr for DripstoneThickness {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "tip_merge" => Ok(Self::TipMerge),
            "tip" => Ok(Self::Tip),
            "frustum" => Ok(Self::Frustum),
            "middle" => Ok(Self::Middle),
            "base" => Ok(Self::Base),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tilt {
    None = 0u8,
    Unstable = 1u8,
    Partial = 2u8,
    Full = 3u8,
}
impl Tilt {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Unstable => "unstable",
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }
}
impl FromStr for Tilt {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "none" => Ok(Self::None),
            "unstable" => Ok(Self::Unstable),
            "partial" => Ok(Self::Partial),
            "full" => Ok(Self::Full),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TrialSpawnerState {
    Inactive = 0u8,
    WaitingForPlayers = 1u8,
    Active = 2u8,
    WaitingForRewardEjection = 3u8,
    EjectingReward = 4u8,
    Cooldown = 5u8,
}
impl TrialSpawnerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::WaitingForPlayers => "waiting_for_players",
            Self::Active => "active",
            Self::WaitingForRewardEjection => "waiting_for_reward_ejection",
            Self::EjectingReward => "ejecting_reward",
            Self::Cooldown => "cooldown",
        }
    }
}
impl FromStr for TrialSpawnerState {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "inactive" => Ok(Self::Inactive),
            "waiting_for_players" => Ok(Self::WaitingForPlayers),
            "active" => Ok(Self::Active),
            "waiting_for_reward_ejection" => Ok(Self::WaitingForRewardEjection),
            "ejecting_reward" => Ok(Self::EjectingReward),
            "cooldown" => Ok(Self::Cooldown),
            _ => Err(()),
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VaultState {
    Inactive = 0u8,
    Active = 1u8,
    Unlocking = 2u8,
    Ejecting = 3u8,
}
impl VaultState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Unlocking => "unlocking",
            Self::Ejecting => "ejecting",
        }
    }
}
impl FromStr for VaultState {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "inactive" => Ok(Self::Inactive),
            "active" => Ok(Self::Active),
            "unlocking" => Ok(Self::Unlocking),
            "ejecting" => Ok(Self::Ejecting),
            _ => Err(()),
        }
    }
}
