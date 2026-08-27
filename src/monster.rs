//! Monsters: hit dice, attack routines, and derived statistics.

use alloc::string::String;

use crate::armour::ArmourClass;
use crate::attack::AttackBonus;
use crate::class::{Level, MARTIAL_ATTACK, MARTIAL_SAVES};
use crate::dice::{DiceExpr, DiceRoller, Die};
use crate::effect::{Effect, Immunities};
use crate::health::HitPoints;
use crate::morale::Morale;
use crate::nonempty::NonEmpty;
use crate::save::SavingThrowProfile;
use crate::units::Feet;
use crate::weapon::{AttackReach, MissileRanges};

/// A monster's hit dice: d8s, with a flat bonus or a fraction.
///
/// There is no `2.5` here. Write `2`, `2+1`, or a half die explicitly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitDice {
    /// Half a hit die: 1d4 hit points.
    Half,
    /// Whole dice: `dice` d8s plus `bonus` hit points.
    Whole {
        /// How many d8s. At least one.
        dice: u8,
        /// The flat bonus, such as the `+1` in `2+1`. May be negative, as
        /// in `1-1`.
        bonus: i8,
    },
}

impl HitDice {
    /// Plain whole hit dice with no bonus. Panics on zero; use
    /// [`HitDice::Half`] for weaker creatures.
    pub const fn of(dice: u8) -> Self {
        assert!(dice >= 1, "zero hit dice is not a creature");
        HitDice::Whole { dice, bonus: 0 }
    }

    /// Whole hit dice with a flat bonus, such as `2+1`.
    pub const fn plus(dice: u8, bonus: i8) -> Self {
        assert!(dice >= 1, "zero hit dice is not a creature");
        HitDice::Whole { dice, bonus }
    }

    /// Roll maximum hit points. Always at least one.
    pub fn roll_hp(self, r: &mut dyn DiceRoller) -> HitPoints {
        let rolled = match self {
            HitDice::Half => i16::from(r.roll(Die::D4)),
            HitDice::Whole { dice, bonus } => DiceExpr {
                count: dice,
                die: Die::D8,
                bonus,
            }
            .roll(r),
        };
        HitPoints::new(rolled.max(1) as u16).expect("at least one hit point")
    }

    /// The level this creature fights and saves as: its dice count, plus
    /// one for a positive bonus, at least one.
    fn effective_level(self) -> Level {
        let n = match self {
            HitDice::Half => 1,
            HitDice::Whole { dice, bonus } => {
                let step_up = if bonus > 0 { 1 } else { 0 };
                (dice + step_up).max(1)
            }
        };
        Level::new(n.min(14)).expect("clamped to a valid level")
    }

    /// The derived attack bonus: the martial progression at the effective
    /// level.
    pub fn attack_bonus(self) -> AttackBonus {
        MARTIAL_ATTACK.at(self.effective_level())
    }

    /// The derived saving throws: the martial progression at the effective
    /// level.
    pub fn saves(self) -> SavingThrowProfile {
        MARTIAL_SAVES.at(self.effective_level())
    }

    /// The base experience award for defeating this creature, plus the
    /// bonus for each special ability it has.
    pub fn xp(self, special_abilities: u8) -> u32 {
        let (dice, plus) = match self {
            HitDice::Half => (0, false),
            HitDice::Whole { dice, bonus } => {
                if bonus < 0 {
                    // A negative bonus drops a row: `1-1` awards as under 1.
                    (u32::from(dice.saturating_sub(1)), dice > 1)
                } else {
                    (u32::from(dice), bonus > 0)
                }
            }
        };
        let (base, per_ability) = match (dice, plus) {
            (0, _) => (5, 1),
            (1, false) => (10, 3),
            (1, true) => (15, 4),
            (2, false) => (20, 5),
            (2, true) => (25, 10),
            (3, false) => (35, 15),
            (3, true) => (50, 25),
            (4, false) => (75, 50),
            (4, true) => (125, 75),
            (5, false) => (175, 125),
            (5, true) => (225, 175),
            (6, false) => (275, 225),
            (6, true) => (350, 300),
            (7, _) => (450, 400),
            (8, _) => (650, 550),
            (9 | 10, _) => (900, 700),
            (11 | 12, _) => (1_100, 800),
            (13..=16, _) => (1_350, 950),
            (17..=20, _) => (2_000, 1_150),
            (21, _) => (2_500, 2_000),
            (n, _) => (2_500 + 250 * (n - 21), 2_000 + 250 * (n - 21)),
        };
        base + per_ability * u32::from(special_abilities)
    }
}

/// One attack in a monster's routine.
#[derive(Clone, Debug)]
pub struct MonsterAttack {
    /// A display name, such as "claw". The kernel never interprets it.
    pub name: String,
    /// The damage dice.
    pub damage: DiceExpr,
    /// Melee or missile.
    pub reach: AttackReach,
    /// An effect applied to the victim on a hit, after damage. Poison,
    /// paralysis, and the like.
    pub on_hit: Option<Effect>,
}

impl MonsterAttack {
    /// A melee attack.
    pub fn melee(name: &str, damage: DiceExpr) -> Self {
        MonsterAttack {
            name: String::from(name),
            damage,
            reach: AttackReach::Melee,
            on_hit: None,
        }
    }

    /// A missile attack.
    pub fn missile(name: &str, damage: DiceExpr, ranges: MissileRanges) -> Self {
        MonsterAttack {
            name: String::from(name),
            damage,
            reach: AttackReach::Missile(ranges),
            on_hit: None,
        }
    }

    /// The same attack with an on-hit effect.
    #[must_use]
    pub fn with_effect(mut self, effect: Effect) -> Self {
        self.on_hit = Some(effect);
        self
    }
}

/// A monster species: the validated statistics a combatant is spawned from.
#[derive(Clone, Debug)]
pub struct MonsterKind {
    /// A display name. The kernel never interprets it.
    pub name: String,
    /// Hit dice.
    pub hit_dice: HitDice,
    /// Armour class.
    pub armour_class: ArmourClass,
    /// Base movement rate. The encounter rate is a third of it.
    pub speed: Feet,
    /// The attacks made each round, in order. `claw`, `claw`, `bite` is
    /// three entries.
    pub routine: NonEmpty<MonsterAttack>,
    /// The morale score.
    pub morale: Morale,
    /// Saving throws. `None` derives them from the hit dice.
    pub saves: Option<SavingThrowProfile>,
    /// Attack bonus. `None` derives it from the hit dice.
    pub attack_bonus: Option<AttackBonus>,
    /// What the creature cannot be harmed by.
    pub immunities: Immunities,
    /// How many special abilities the creature has, for the experience
    /// award bonus.
    pub special_abilities: u8,
}

impl MonsterKind {
    /// A monster with the given essentials, derived saves and attack bonus,
    /// morale 7, speed 120, and no immunities. Set fields directly for
    /// anything else.
    pub fn new(
        name: &str,
        hit_dice: HitDice,
        armour_class: ArmourClass,
        routine: NonEmpty<MonsterAttack>,
    ) -> Self {
        MonsterKind {
            name: String::from(name),
            hit_dice,
            armour_class,
            speed: Feet(120),
            routine,
            morale: Morale::new(7).expect("7 is a valid morale score"),
            saves: None,
            attack_bonus: None,
            immunities: Immunities::NONE,
            special_abilities: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::SeededDice;

    #[test]
    fn hp_is_at_least_one() {
        let mut r = SeededDice::seeded(5);
        for _ in 0..100 {
            assert!(HitDice::plus(1, -3).roll_hp(&mut r).get() >= 1);
        }
    }

    #[test]
    fn derived_attack_follows_the_martial_table() {
        assert_eq!(HitDice::Half.attack_bonus(), AttackBonus::new(0));
        assert_eq!(HitDice::of(3).attack_bonus(), AttackBonus::new(0));
        assert_eq!(HitDice::plus(3, 1).attack_bonus(), AttackBonus::new(2));
        assert_eq!(HitDice::of(7).attack_bonus(), AttackBonus::new(5));
    }

    #[test]
    fn xp_matches_the_award_table() {
        assert_eq!(HitDice::Half.xp(0), 5);
        assert_eq!(HitDice::plus(1, -1).xp(0), 5);
        assert_eq!(HitDice::of(1).xp(0), 10);
        assert_eq!(HitDice::plus(1, 1).xp(0), 15);
        assert_eq!(HitDice::of(2).xp(1), 25);
        assert_eq!(HitDice::plus(2, 2).xp(1), 35);
        assert_eq!(HitDice::of(6).xp(0), 275);
        assert_eq!(HitDice::of(9).xp(0), 900);
        assert_eq!(HitDice::of(22).xp(0), 2_750);
    }
}
