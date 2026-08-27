//! Morale.

/// A morale score in `2..=12`, by construction.
///
/// The endpoints have special behaviour: a score of 2 never fights, and a
/// score of 12 is fearless and never checks. The kernel honours both.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Morale(u8);

impl Morale {
    /// Validate a score. Returns `None` outside `2..=12`.
    pub const fn new(score: u8) -> Option<Self> {
        if score >= 2 && score <= 12 {
            Some(Morale(score))
        } else {
            None
        }
    }

    /// The raw score.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// A score of 2: never fights.
    pub const fn never_fights(self) -> bool {
        self.0 == 2
    }

    /// A score of 12: fearless, never checks.
    pub const fn fearless(self) -> bool {
        self.0 == 12
    }
}

/// The result of one morale check.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoraleOutcome {
    /// The side fights on.
    StandsFirm,
    /// The side breaks. The referee decides between flight and surrender;
    /// the kernel marks the side's creatures as fleeing.
    Breaks,
}
