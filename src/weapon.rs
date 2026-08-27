//! Weapons, attack modes, missile ranges, and cover.

use alloc::string::String;

use crate::ability::Modifier;
use crate::dice::{DiceExpr, Die};
use crate::nonempty::NonEmpty;
use crate::units::Feet;

/// The three missile range bands. There is no out-of-range variant: a
/// distance beyond long range has no band, and no missile attack against it
/// can be built.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RangeBand {
    /// Short range: +1 to the attack roll.
    Short,
    /// Medium range: no modifier.
    Medium,
    /// Long range: -1 to the attack roll.
    Long,
}

impl RangeBand {
    /// The attack roll modifier for the band.
    pub const fn attack_modifier(self) -> Modifier {
        Modifier::new(match self {
            RangeBand::Short => 1,
            RangeBand::Medium => 0,
            RangeBand::Long => -1,
        })
    }
}

/// The range brackets of a missile weapon. `short < medium < long` by
/// construction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MissileRanges {
    short: Feet,
    medium: Feet,
    long: Feet,
}

impl MissileRanges {
    /// Validate a set of brackets. Returns `None` unless
    /// `short < medium < long`.
    pub const fn new(short: Feet, medium: Feet, long: Feet) -> Option<Self> {
        if short.0 < medium.0 && medium.0 < long.0 {
            Some(MissileRanges {
                short,
                medium,
                long,
            })
        } else {
            None
        }
    }

    /// The upper bound of short range.
    pub const fn short(self) -> Feet {
        self.short
    }

    /// The upper bound of medium range.
    pub const fn medium(self) -> Feet {
        self.medium
    }

    /// The upper bound of long range.
    pub const fn long(self) -> Feet {
        self.long
    }

    /// The band a distance falls in. Returns `None` beyond long range.
    pub const fn band(self, distance: Feet) -> Option<RangeBand> {
        if distance.0 <= self.short.0 {
            Some(RangeBand::Short)
        } else if distance.0 <= self.medium.0 {
            Some(RangeBand::Medium)
        } else if distance.0 <= self.long.0 {
            Some(RangeBand::Long)
        } else {
            None
        }
    }
}

/// A cover penalty in `1..=4`, by construction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CoverPenalty(u8);

impl CoverPenalty {
    /// Validate a penalty. Returns `None` outside `1..=4`.
    pub const fn new(penalty: u8) -> Option<Self> {
        if penalty >= 1 && penalty <= 4 {
            Some(CoverPenalty(penalty))
        } else {
            None
        }
    }

    /// The attack roll modifier: `-penalty`.
    pub const fn attack_modifier(self) -> Modifier {
        Modifier::new(-(self.0 as i8))
    }
}

/// The cover a target enjoys against one attacker.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Cover {
    /// No cover.
    #[default]
    None,
    /// Partial cover: a penalty of one to four on the attack roll.
    Partial(CoverPenalty),
    /// Total cover: the target cannot be hit by missiles.
    Total,
}

/// How an attack mode reaches its target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackReach {
    /// Close quarters: opponents five feet apart or less.
    Melee,
    /// Thrown or fired: opponents more than five feet apart, out to the
    /// long-range bracket.
    Missile(MissileRanges),
}

/// One way to attack with a weapon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttackMode {
    /// Melee or missile.
    pub reach: AttackReach,
    /// The damage dice, used under the variable weapon damage rule.
    pub damage: DiceExpr,
    /// A slow mode acts last in the round when the slow weapon rule is on.
    pub slow: bool,
    /// A reload mode fires every second round when the reload rule is on.
    pub reload: bool,
}

impl AttackMode {
    /// A melee mode with the given damage die.
    pub const fn melee(damage: DiceExpr) -> Self {
        AttackMode {
            reach: AttackReach::Melee,
            damage,
            slow: false,
            reload: false,
        }
    }

    /// A missile mode with the given damage die and ranges.
    pub const fn missile(damage: DiceExpr, ranges: MissileRanges) -> Self {
        AttackMode {
            reach: AttackReach::Missile(ranges),
            damage,
            slow: false,
            reload: false,
        }
    }

    /// The same mode, marked slow.
    #[must_use]
    pub const fn slow(self) -> Self {
        AttackMode { slow: true, ..self }
    }

    /// The same mode, marked as needing a round to reload.
    #[must_use]
    pub const fn reloading(self) -> Self {
        AttackMode {
            reload: true,
            ..self
        }
    }
}

/// A weapon: a name, one or more attack modes, and its qualities.
///
/// The name is plain data for presentation. The kernel reads only the
/// mechanics.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Weapon {
    /// A display name, such as "sword". The kernel never interprets it.
    pub name: String,
    /// The ways this weapon can attack. A dagger has a melee mode and a
    /// thrown missile mode.
    pub modes: NonEmpty<AttackMode>,
    /// A two-handed weapon cannot be used with a shield.
    pub two_handed: bool,
    /// A blunt weapon has no edge or point. Some classes can wield only
    /// blunt weapons.
    pub blunt: bool,
    /// A small, light weapon. Some classes can wield only small weapons.
    pub small: bool,
    /// The enchantment bonus. It adds to attack and damage rolls, and a
    /// nonzero bonus marks the weapon as magical.
    pub bonus: Modifier,
}

impl Weapon {
    /// A plain weapon with one or more modes and no special qualities.
    pub fn new(name: &str, modes: NonEmpty<AttackMode>) -> Self {
        Weapon {
            name: String::from(name),
            modes,
            two_handed: false,
            blunt: false,
            small: false,
            bonus: Modifier::ZERO,
        }
    }

    /// The first melee mode, if any.
    pub fn melee_mode(&self) -> Option<&AttackMode> {
        self.modes
            .iter()
            .find(|m| matches!(m.reach, AttackReach::Melee))
    }

    /// The first missile mode, if any.
    pub fn missile_mode(&self) -> Option<&AttackMode> {
        self.modes
            .iter()
            .find(|m| matches!(m.reach, AttackReach::Missile(_)))
    }

    /// Whether the weapon is magical (has a nonzero enchantment bonus).
    pub fn is_magical(&self) -> bool {
        self.bonus != Modifier::ZERO
    }
}

/// Ready-made weapons with the classic mechanics. All of them are plain
/// data; build your own [`Weapon`] for anything else.
pub mod stock {
    use super::*;

    const fn ranges(short: u16, medium: u16, long: u16) -> MissileRanges {
        MissileRanges::new(Feet(short), Feet(medium), Feet(long)).expect("stock ranges are ordered")
    }

    /// A dagger: small, d4, melee or thrown (10'/20'/30').
    pub fn dagger() -> Weapon {
        let mut modes = NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D4)));
        modes.push(AttackMode::missile(
            DiceExpr::of(1, Die::D4),
            ranges(10, 20, 30),
        ));
        Weapon {
            small: true,
            ..Weapon::new("dagger", modes)
        }
    }

    /// A sword: d8, melee.
    pub fn sword() -> Weapon {
        Weapon::new(
            "sword",
            NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D8))),
        )
    }

    /// A two-handed sword: d10, melee, slow, two-handed.
    pub fn two_handed_sword() -> Weapon {
        let modes = NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D10)).slow());
        Weapon {
            two_handed: true,
            ..Weapon::new("two-handed sword", modes)
        }
    }

    /// A mace: blunt, d6, melee.
    pub fn mace() -> Weapon {
        let modes = NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D6)));
        Weapon {
            blunt: true,
            ..Weapon::new("mace", modes)
        }
    }

    /// A staff: blunt, d4, melee, slow, two-handed.
    pub fn staff() -> Weapon {
        let modes = NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D4)).slow());
        Weapon {
            blunt: true,
            two_handed: true,
            ..Weapon::new("staff", modes)
        }
    }

    /// A sling: blunt, d4, missile (40'/80'/160').
    pub fn sling() -> Weapon {
        let modes = NonEmpty::of(AttackMode::missile(
            DiceExpr::of(1, Die::D4),
            ranges(40, 80, 160),
        ));
        Weapon {
            blunt: true,
            ..Weapon::new("sling", modes)
        }
    }

    /// A short bow: d6, missile (50'/100'/150'), two-handed.
    pub fn short_bow() -> Weapon {
        let modes = NonEmpty::of(AttackMode::missile(
            DiceExpr::of(1, Die::D6),
            ranges(50, 100, 150),
        ));
        Weapon {
            two_handed: true,
            ..Weapon::new("short bow", modes)
        }
    }

    /// A long bow: d6, missile (70'/140'/210'), two-handed.
    pub fn long_bow() -> Weapon {
        let modes = NonEmpty::of(AttackMode::missile(
            DiceExpr::of(1, Die::D6),
            ranges(70, 140, 210),
        ));
        Weapon {
            two_handed: true,
            ..Weapon::new("long bow", modes)
        }
    }

    /// A crossbow: d6, missile (80'/160'/240'), slow, reloading, two-handed.
    pub fn crossbow() -> Weapon {
        let modes = NonEmpty::of(
            AttackMode::missile(DiceExpr::of(1, Die::D6), ranges(80, 160, 240))
                .slow()
                .reloading(),
        );
        Weapon {
            two_handed: true,
            ..Weapon::new("crossbow", modes)
        }
    }

    /// A spear: d6, melee or thrown (20'/40'/60').
    pub fn spear() -> Weapon {
        let mut modes = NonEmpty::of(AttackMode::melee(DiceExpr::of(1, Die::D6)));
        modes.push(AttackMode::missile(
            DiceExpr::of(1, Die::D6),
            ranges(20, 40, 60),
        ));
        Weapon::new("spear", modes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disordered_ranges_are_rejected() {
        assert!(MissileRanges::new(Feet(100), Feet(50), Feet(30)).is_none());
        assert!(MissileRanges::new(Feet(50), Feet(50), Feet(150)).is_none());
    }

    #[test]
    fn bands_have_no_out_of_range_variant() {
        let r = MissileRanges::new(Feet(50), Feet(100), Feet(150)).unwrap();
        assert_eq!(r.band(Feet(50)), Some(RangeBand::Short));
        assert_eq!(r.band(Feet(51)), Some(RangeBand::Medium));
        assert_eq!(r.band(Feet(150)), Some(RangeBand::Long));
        assert_eq!(r.band(Feet(151)), None);
    }

    #[test]
    fn cover_penalty_range_is_enforced() {
        assert!(CoverPenalty::new(0).is_none());
        assert!(CoverPenalty::new(5).is_none());
        assert_eq!(
            CoverPenalty::new(4).unwrap().attack_modifier(),
            Modifier::new(-4)
        );
    }

    #[test]
    fn stock_weapons_are_coherent() {
        assert!(stock::dagger().melee_mode().is_some());
        assert!(stock::dagger().missile_mode().is_some());
        assert!(stock::sword().missile_mode().is_none());
        assert!(stock::crossbow().missile_mode().unwrap().reload);
        assert!(stock::two_handed_sword().melee_mode().unwrap().slow);
        assert!(!stock::sword().is_magical());
    }
}
