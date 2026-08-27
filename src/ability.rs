//! Ability scores and modifiers.

/// An ability score in `3..=18`, by construction.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AbilityScore(u8);

impl AbilityScore {
    /// Validate a score. Returns `None` outside `3..=18`.
    pub const fn new(score: u8) -> Option<Self> {
        if score >= 3 && score <= 18 {
            Some(AbilityScore(score))
        } else {
            None
        }
    }

    /// The raw score.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The derived modifier. The score owns its modifier; store the score,
    /// never the modifier, and the two cannot disagree.
    pub const fn modifier(self) -> Modifier {
        Modifier(match self.0 {
            3 => -3,
            4..=5 => -2,
            6..=8 => -1,
            9..=12 => 0,
            13..=15 => 1,
            16..=17 => 2,
            _ => 3,
        })
    }
}

/// A small additive modifier to a d20 roll, a damage roll, or an armour
/// class. Ability modifiers, range bands, cover penalties, and situational
/// bonuses are all `Modifier`s. Addition saturates.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Modifier(i8);

impl Modifier {
    /// No modifier.
    pub const ZERO: Modifier = Modifier(0);

    /// A modifier of `value`.
    pub const fn new(value: i8) -> Self {
        Modifier(value)
    }

    /// The raw value.
    pub const fn value(self) -> i8 {
        self.0
    }

    /// The sum of two modifiers, saturating at the `i8` bounds.
    #[must_use]
    pub const fn plus(self, other: Modifier) -> Modifier {
        Modifier(self.0.saturating_add(other.0))
    }
}

impl core::ops::Add for Modifier {
    type Output = Modifier;

    fn add(self, rhs: Modifier) -> Modifier {
        self.plus(rhs)
    }
}

/// The six ability scores of an adventurer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Abilities {
    /// Brawn. Modifies melee attack and damage.
    pub strength: AbilityScore,
    /// Learning and reason.
    pub intelligence: AbilityScore,
    /// Willpower and intuition. Modifies saves against magic.
    pub wisdom: AbilityScore,
    /// Agility. Modifies armour class, missile attack, and individual
    /// initiative.
    pub dexterity: AbilityScore,
    /// Health. Modifies hit points per hit die.
    pub constitution: AbilityScore,
    /// Force of personality.
    pub charisma: AbilityScore,
}

impl Abilities {
    /// All six scores at the same value. Convenient for tests and simple
    /// monsters-as-adventurers.
    pub const fn flat(score: AbilityScore) -> Self {
        Abilities {
            strength: score,
            intelligence: score,
            wisdom: score,
            dexterity: score,
            constitution: score,
            charisma: score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_range_is_enforced() {
        assert!(AbilityScore::new(2).is_none());
        assert!(AbilityScore::new(19).is_none());
        assert!(AbilityScore::new(3).is_some());
        assert!(AbilityScore::new(18).is_some());
    }

    #[test]
    fn modifier_table_is_exact() {
        let table = [
            (3, -3),
            (4, -2),
            (5, -2),
            (6, -1),
            (7, -1),
            (8, -1),
            (9, 0),
            (10, 0),
            (11, 0),
            (12, 0),
            (13, 1),
            (14, 1),
            (15, 1),
            (16, 2),
            (17, 2),
            (18, 3),
        ];
        for (score, modifier) in table {
            assert_eq!(
                AbilityScore::new(score).unwrap().modifier(),
                Modifier::new(modifier),
                "score {score}"
            );
        }
    }

    #[test]
    fn modifier_addition_saturates() {
        let big = Modifier::new(i8::MAX);
        assert_eq!(big + Modifier::new(1), Modifier::new(i8::MAX));
    }
}
