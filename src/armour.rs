//! Armour class.
//!
//! The crate computes with ascending armour class only. [`DescendingAc`] and
//! [`Thac0`] exist for display and for reading old material; convert at the
//! boundary and never carry both through a calculation.

use crate::ability::Modifier;
use crate::attack::AttackBonus;

/// Ascending armour class. Higher is better. An attack hits when
/// `d20 + attack bonus + situational modifiers >= target class`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ArmourClass(i8);

impl ArmourClass {
    /// The class of a creature with no armour: 10.
    pub const UNARMOURED: ArmourClass = ArmourClass(10);

    /// An ascending armour class of `value`.
    pub const fn new(value: i8) -> Self {
        ArmourClass(value)
    }

    /// The raw ascending value.
    pub const fn get(self) -> i8 {
        self.0
    }

    /// The equivalent descending armour class (`19 - ascending`).
    pub const fn descending(self) -> DescendingAc {
        DescendingAc(19 - self.0)
    }

    /// Convert a descending armour class (`19 - descending`).
    pub const fn from_descending(ac: DescendingAc) -> Self {
        ArmourClass(19 - ac.0)
    }

    /// The class adjusted by a modifier. A bonus raises the class.
    #[must_use]
    pub const fn adjusted(self, m: Modifier) -> Self {
        ArmourClass(self.0.saturating_add(m.value()))
    }
}

/// Descending armour class, for display and for reading old material.
/// Lower is better. Unarmoured is 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DescendingAc(pub i8);

/// "To hit armour class zero", for display and for reading old material.
/// Lower is better.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Thac0(pub u8);

impl Thac0 {
    /// The THAC0 equivalent to an attack bonus (`19 - bonus`).
    pub const fn from_attack_bonus(bonus: AttackBonus) -> Self {
        Thac0(19u8.saturating_add_signed(-bonus.get()))
    }

    /// The attack bonus equivalent to this THAC0 (`19 - thac0`).
    pub const fn attack_bonus(self) -> AttackBonus {
        AttackBonus::new(19i8.saturating_sub_unsigned(self.0))
    }
}

/// A suit of armour, by weight class.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Armour {
    /// No armour. Class 10.
    None,
    /// Light armour. Class 12.
    Light,
    /// Medium armour. Class 14.
    Medium,
    /// Heavy armour. Class 16.
    Heavy,
}

impl Armour {
    /// The base ascending armour class of a wearer, before shield and
    /// dexterity.
    pub const fn class(self) -> ArmourClass {
        ArmourClass(match self {
            Armour::None => 10,
            Armour::Light => 12,
            Armour::Medium => 14,
            Armour::Heavy => 16,
        })
    }
}

/// The ascending armour class bonus of a carried shield.
pub const SHIELD_BONUS: Modifier = Modifier::new(1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascending_and_descending_round_trip() {
        for value in -3..=19 {
            let ac = ArmourClass::new(value);
            assert_eq!(ArmourClass::from_descending(ac.descending()), ac);
        }
        assert_eq!(ArmourClass::UNARMOURED.descending().0, 9);
        assert_eq!(Armour::Heavy.class().descending().0, 3);
    }

    #[test]
    fn thac0_matches_attack_bonus() {
        assert_eq!(Thac0::from_attack_bonus(AttackBonus::new(0)).0, 19);
        assert_eq!(Thac0::from_attack_bonus(AttackBonus::new(9)).0, 10);
        assert_eq!(Thac0(19).attack_bonus(), AttackBonus::new(0));
        assert_eq!(Thac0(12).attack_bonus(), AttackBonus::new(7));
    }
}
