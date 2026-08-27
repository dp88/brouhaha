//! Identifiers and quantities.

/// A distance in feet.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Feet(pub u16);

impl Feet {
    /// Zero feet.
    pub const ZERO: Feet = Feet(0);
}

/// A duration in combat rounds. One round is ten seconds of game time.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Rounds(pub u16);

/// The ordinal number of the current round. Starts at one.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RoundNumber(pub(crate) u32);

impl RoundNumber {
    /// The first round.
    pub const FIRST: RoundNumber = RoundNumber(1);

    /// The round number as an integer, starting at one.
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub(crate) const fn next(self) -> RoundNumber {
        RoundNumber(self.0 + 1)
    }
}

/// The identity of one combatant in a combat. Issued by the combat builder.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CombatantId(pub(crate) u32);

impl CombatantId {
    /// The id as an integer. Stable for the life of the combat.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The identity of one side in a combat. Issued by the combat builder.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SideId(pub(crate) u8);

impl SideId {
    /// The id as an integer. Stable for the life of the combat.
    pub const fn get(self) -> u8 {
        self.0
    }
}
