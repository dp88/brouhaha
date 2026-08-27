//! Attack rolls.
//!
//! [`resolve_attack`] is the whole to-hit rule as one pure function. The
//! combat machine calls it; you can call it directly to use the crate as a
//! rules calculator.

use crate::ability::Modifier;
use crate::armour::ArmourClass;
use crate::dice::{DiceRoller, Die};

/// A class- and level-derived attack bonus.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct AttackBonus(i8);

impl AttackBonus {
    /// No bonus.
    pub const ZERO: AttackBonus = AttackBonus(0);

    /// A bonus of `value`.
    pub const fn new(value: i8) -> Self {
        AttackBonus(value)
    }

    /// The raw value.
    pub const fn get(self) -> i8 {
        self.0
    }
}

/// The raw face of a d20, in `1..=20` by construction. It carries the
/// automatic-hit and automatic-miss facts.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NaturalRoll(u8);

impl NaturalRoll {
    /// Validate a face. Returns `None` outside `1..=20`.
    pub const fn new(face: u8) -> Option<Self> {
        if face >= 1 && face <= 20 {
            Some(NaturalRoll(face))
        } else {
            None
        }
    }

    pub(crate) fn from_roller(r: &mut dyn DiceRoller) -> Self {
        NaturalRoll(r.roll(Die::D20).clamp(1, 20))
    }

    /// The raw face.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// A natural 1. An attack roll of natural 1 always misses.
    pub const fn is_natural_one(self) -> bool {
        self.0 == 1
    }

    /// A natural 20. An attack roll of natural 20 always hits.
    pub const fn is_natural_twenty(self) -> bool {
        self.0 == 20
    }
}

impl TryFrom<u8> for NaturalRoll {
    type Error = &'static str;

    fn try_from(face: u8) -> Result<Self, Self::Error> {
        NaturalRoll::new(face).ok_or("a d20 face is 1..=20")
    }
}

impl From<NaturalRoll> for u8 {
    fn from(roll: NaturalRoll) -> u8 {
        roll.get()
    }
}

/// The result of one attack roll.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackOutcome {
    /// The attack missed.
    Miss {
        /// The raw d20 face.
        natural: NaturalRoll,
        /// The modified total.
        total: i16,
    },
    /// The attack hit. Damage follows.
    Hit {
        /// The raw d20 face.
        natural: NaturalRoll,
        /// The modified total.
        total: i16,
    },
}

impl AttackOutcome {
    /// Whether the attack hit.
    pub const fn is_hit(self) -> bool {
        matches!(self, AttackOutcome::Hit { .. })
    }
}

/// Roll one attack.
///
/// The attack hits when `d20 + bonus + situational >= target`. A natural 20
/// always hits. A natural 1 always misses. The caller sums the situational
/// modifiers first: strength or dexterity, range band, cover, and any
/// bonus against a retreating target.
pub fn resolve_attack(
    r: &mut dyn DiceRoller,
    bonus: AttackBonus,
    situational: Modifier,
    target: ArmourClass,
) -> AttackOutcome {
    let natural = NaturalRoll::from_roller(r);
    let total = i16::from(natural.get()) + i16::from(bonus.get()) + i16::from(situational.value());
    let hits = if natural.is_natural_one() {
        false
    } else if natural.is_natural_twenty() {
        true
    } else {
        total >= i16::from(target.get())
    };
    if hits {
        AttackOutcome::Hit { natural, total }
    } else {
        AttackOutcome::Miss { natural, total }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A roller that returns a fixed sequence, for exact tests.
    pub(crate) struct Script(pub std::vec::Vec<u8>);

    impl DiceRoller for Script {
        fn roll(&mut self, _die: Die) -> u8 {
            self.0.remove(0)
        }
    }

    #[test]
    fn totals_compare_against_the_class() {
        let mut r = Script(std::vec![10, 10]);
        let target = ArmourClass::new(12);
        assert!(resolve_attack(&mut r, AttackBonus::new(2), Modifier::ZERO, target).is_hit());
        assert!(!resolve_attack(&mut r, AttackBonus::new(1), Modifier::ZERO, target).is_hit());
    }

    #[test]
    fn natural_one_always_misses() {
        let mut r = Script(std::vec![1]);
        let outcome = resolve_attack(
            &mut r,
            AttackBonus::new(90),
            Modifier::new(9),
            ArmourClass::new(-100),
        );
        assert!(!outcome.is_hit());
    }

    #[test]
    fn natural_twenty_always_hits() {
        let mut r = Script(std::vec![20]);
        let outcome = resolve_attack(
            &mut r,
            AttackBonus::new(-9),
            Modifier::new(-9),
            ArmourClass::new(90),
        );
        assert!(outcome.is_hit());
    }
}
