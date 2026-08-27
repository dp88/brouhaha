//! Saving throws.

use crate::ability::Modifier;
use crate::attack::NaturalRoll;
use crate::dice::DiceRoller;

/// The five saving throw categories.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SaveCategory {
    /// Death rays and poison.
    Death,
    /// Magical wands.
    Wands,
    /// Paralysis and petrification.
    Paralysis,
    /// Breath attacks.
    Breath,
    /// Harmful spells, magical rods, and staves.
    Spells,
}

/// A saving throw target in `2..=20`. A d20 result greater than or equal to
/// the target succeeds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SaveTarget(u8);

impl SaveTarget {
    /// Validate a target. Returns `None` outside `2..=20`.
    pub const fn new(target: u8) -> Option<Self> {
        if target >= 2 && target <= 20 {
            Some(SaveTarget(target))
        } else {
            None
        }
    }

    /// The raw target.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// One saving throw target per category. Five named fields: a profile cannot
/// be missing a category.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SavingThrowProfile {
    /// Versus death rays and poison.
    pub death: SaveTarget,
    /// Versus magical wands.
    pub wands: SaveTarget,
    /// Versus paralysis and petrification.
    pub paralysis: SaveTarget,
    /// Versus breath attacks.
    pub breath: SaveTarget,
    /// Versus harmful spells, rods, and staves.
    pub spells: SaveTarget,
}

impl SavingThrowProfile {
    /// Build a profile from five raw targets. Panics when a target is
    /// outside `2..=20`; intended for constant tables.
    pub const fn of(death: u8, wands: u8, paralysis: u8, breath: u8, spells: u8) -> Self {
        SavingThrowProfile {
            death: SaveTarget::new(death).expect("valid save target"),
            wands: SaveTarget::new(wands).expect("valid save target"),
            paralysis: SaveTarget::new(paralysis).expect("valid save target"),
            breath: SaveTarget::new(breath).expect("valid save target"),
            spells: SaveTarget::new(spells).expect("valid save target"),
        }
    }

    /// The target for one category.
    pub const fn target(&self, category: SaveCategory) -> SaveTarget {
        match category {
            SaveCategory::Death => self.death,
            SaveCategory::Wands => self.wands,
            SaveCategory::Paralysis => self.paralysis,
            SaveCategory::Breath => self.breath,
            SaveCategory::Spells => self.spells,
        }
    }
}

/// The result of one saving throw.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveOutcome {
    /// The save succeeded. A damaging effect deals half damage; any other
    /// effect is negated.
    Success,
    /// The save failed. The effect applies in full.
    Failure,
}

/// Roll one saving throw.
///
/// The save succeeds when `d20 + bonus >= target`. Pass the wisdom modifier
/// as `bonus` against magical effects, and [`Modifier::ZERO`] otherwise; the
/// caller decides applicability.
pub fn resolve_save(
    r: &mut dyn DiceRoller,
    profile: SavingThrowProfile,
    category: SaveCategory,
    bonus: Modifier,
) -> (NaturalRoll, SaveOutcome) {
    let natural = NaturalRoll::from_roller(r);
    let total = i16::from(natural.get()) + i16::from(bonus.value());
    let outcome = if total >= i16::from(profile.target(category).get()) {
        SaveOutcome::Success
    } else {
        SaveOutcome::Failure
    };
    (natural, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::Die;

    struct Fixed(u8);

    impl DiceRoller for Fixed {
        fn roll(&mut self, _die: Die) -> u8 {
            self.0
        }
    }

    const PROFILE: SavingThrowProfile = SavingThrowProfile::of(12, 13, 14, 15, 16);

    #[test]
    fn target_range_is_enforced() {
        assert!(SaveTarget::new(1).is_none());
        assert!(SaveTarget::new(21).is_none());
    }

    #[test]
    fn meeting_the_target_succeeds() {
        let (_, outcome) =
            resolve_save(&mut Fixed(12), PROFILE, SaveCategory::Death, Modifier::ZERO);
        assert_eq!(outcome, SaveOutcome::Success);
        let (_, outcome) =
            resolve_save(&mut Fixed(11), PROFILE, SaveCategory::Death, Modifier::ZERO);
        assert_eq!(outcome, SaveOutcome::Failure);
    }

    #[test]
    fn the_bonus_applies() {
        let (_, outcome) = resolve_save(
            &mut Fixed(14),
            PROFILE,
            SaveCategory::Spells,
            Modifier::new(2),
        );
        assert_eq!(outcome, SaveOutcome::Success);
    }
}
