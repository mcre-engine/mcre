pub use mcre_core::{Axis, Direction};

#[derive(Debug, Copy, Clone)]
pub enum NoteBlockInstrument {
    Harp = 0,
    Basedrum = 1,
    Snare = 2,
    Hat = 3,
    Bass = 4,
    Flute = 5,
    Bell = 6,
    Guitar = 7,
    Chime = 8,
    Xylophone = 9,
    IronXylophone = 10,
    CowBell = 11,
    Didgeridoo = 12,
    Bit = 13,
    Banjo = 14,
    Pling = 15,
    Zombie = 16,
    Skeleton = 17,
    Creeper = 18,
    Dragon = 19,
    WitherSkeleton = 20,
    Piglin = 21,
    CustomHead = 22,
}

#[derive(Debug, Copy, Clone)]
pub enum BedPart {
    Head = 0,
    Foot = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum RailShape {
    NorthSouth = 0,
    EastWest = 1,
    AscendingEast = 2,
    AscendingWest = 3,
    AscendingNorth = 4,
    AscendingSouth = 5,
    SouthEast = 6,
    SouthWest = 7,
    NorthWest = 8,
    NorthEast = 9,
}

#[derive(Debug, Copy, Clone)]
pub enum DoubleBlockHalf {
    Upper = 0,
    Lower = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum PistonType {
    Normal = 0,
    Sticky = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum SideChainPart {
    Unconnected = 0,
    Right = 1,
    Center = 2,
    Left = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum CreakingHeartState {
    Uprooted = 0,
    Dormant = 1,
    Awake = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum Half {
    Top = 0,
    Bottom = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum StairsShape {
    Straight = 0,
    InnerLeft = 1,
    InnerRight = 2,
    OuterLeft = 3,
    OuterRight = 4,
}

#[derive(Debug, Copy, Clone)]
pub enum ChestType {
    Single = 0,
    Left = 1,
    Right = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum RedstoneSide {
    Up = 0,
    Side = 1,
    None = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum DoorHingeSide {
    Left = 0,
    Right = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum AttachFace {
    Floor = 0,
    Wall = 1,
    Ceiling = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum SlabType {
    Top = 0,
    Bottom = 1,
    Double = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum WallSide {
    None = 0,
    Low = 1,
    Tall = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum ComparatorMode {
    Compare = 0,
    Subtract = 1,
}

#[derive(Debug, Copy, Clone)]
pub enum BambooLeaves {
    None = 0,
    Small = 1,
    Large = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum BellAttachType {
    Floor = 0,
    Ceiling = 1,
    SingleWall = 2,
    DoubleWall = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum StructureMode {
    Save = 0,
    Load = 1,
    Corner = 2,
    Data = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum FrontAndTop {
    DownEast = 0,
    DownNorth = 1,
    DownSouth = 2,
    DownWest = 3,
    UpEast = 4,
    UpNorth = 5,
    UpSouth = 6,
    UpWest = 7,
    WestUp = 8,
    EastUp = 9,
    NorthUp = 10,
    SouthUp = 11,
}

#[derive(Debug, Copy, Clone)]
pub enum TestBlockMode {
    Start = 0,
    Log = 1,
    Fail = 2,
    Accept = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum SculkSensorPhase {
    Inactive = 0,
    Active = 1,
    Cooldown = 2,
}

#[derive(Debug, Copy, Clone)]
pub enum Pose {
    Standing = 0,
    Sitting = 1,
    Running = 2,
    Star = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum DripstoneThickness {
    TipMerge = 0,
    Tip = 1,
    Frustum = 2,
    Middle = 3,
    Base = 4,
}

#[derive(Debug, Copy, Clone)]
pub enum Tilt {
    None = 0,
    Unstable = 1,
    Partial = 2,
    Full = 3,
}

#[derive(Debug, Copy, Clone)]
pub enum TrialSpawnerState {
    Inactive = 0,
    WaitingForPlayers = 1,
    Active = 2,
    WaitingForRewardEjection = 3,
    EjectingReward = 4,
    Cooldown = 5,
}

#[derive(Debug, Copy, Clone)]
pub enum VaultState {
    Inactive = 0,
    Active = 1,
    Unlocking = 2,
    Ejecting = 3,
}
