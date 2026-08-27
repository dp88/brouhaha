//! Character classes: the trait, the level-band tables, and the four
//! built-in classes.
//!
//! A custom class is one `impl Class`, usually pure data. The built-in
//! progression tables are public constants, so a custom class can reuse
//! them. See the crate examples.

use core::num::NonZeroU8;

use crate::armour::Armour;
use crate::attack::AttackBonus;
use crate::dice::Die;
use crate::magic::SpellSlots;
use crate::save::SavingThrowProfile;
use crate::weapon::Weapon;

/// A character level, at least one. The sheet builder validates it against
/// the class maximum.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Level(NonZeroU8);

impl Level {
    /// Level one.
    pub const ONE: Level = Level(NonZeroU8::MIN);

    /// Validate a level. Returns `None` for zero.
    pub const fn new(level: u8) -> Option<Self> {
        match NonZeroU8::new(level) {
            Some(n) => Some(Level(n)),
            None => None,
        }
    }

    /// The raw level.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// A level-banded table as data: `(last level of the band, value)` pairs in
/// ascending order. Levels past the last band clamp to it.
#[derive(Clone, Copy, Debug)]
pub struct Progression<T: Copy + 'static> {
    bands: &'static [(u8, T)],
}

impl<T: Copy + 'static> Progression<T> {
    /// Build a table. Panics when `bands` is empty or the band bounds are
    /// not strictly ascending; intended for constants, where the panic is a
    /// compile-time error.
    pub const fn new(bands: &'static [(u8, T)]) -> Self {
        assert!(!bands.is_empty(), "a progression needs at least one band");
        let mut i = 1;
        while i < bands.len() {
            assert!(bands[i - 1].0 < bands[i].0, "band bounds must ascend");
            i += 1;
        }
        Progression { bands }
    }

    /// The value for a level.
    pub fn at(&self, level: Level) -> T {
        for &(bound, value) in self.bands {
            if level.get() <= bound {
                return value;
            }
        }
        self.bands[self.bands.len() - 1].1
    }
}

/// What the combat kernel needs from a character class.
///
/// The trait is consulted once, when a sheet is built. Combat holds only the
/// derived numbers and the [`ClassAbility`] hooks.
pub trait Class {
    /// The class name, for presentation.
    fn name(&self) -> &str;

    /// The hit die rolled per level.
    fn hit_die(&self) -> Die;

    /// The highest level a member of this class can reach.
    fn max_level(&self) -> Level;

    /// The attack bonus at a level.
    fn attack_bonus(&self, level: Level) -> AttackBonus;

    /// The saving throw profile at a level.
    fn saves(&self, level: Level) -> SavingThrowProfile;

    /// Spell slots at a level. All zero for a class that does not cast.
    fn spell_slots(&self, level: Level) -> SpellSlots {
        let _ = level;
        SpellSlots::NONE
    }

    /// Beyond ninth level a character gains flat hit points per level
    /// instead of hit dice, and the constitution modifier no longer
    /// applies. This is the flat gain.
    fn hp_bonus_per_level_beyond_ninth(&self) -> u8 {
        1
    }

    /// Whether the class may wear this armour, with or without a shield.
    fn may_wear(&self, armour: Armour, shield: bool) -> bool {
        let _ = (armour, shield);
        true
    }

    /// Whether the class may wield this weapon.
    fn may_wield(&self, weapon: &Weapon) -> bool {
        let _ = weapon;
        true
    }

    /// The combat hooks this class grants (see [`ClassAbility`]).
    fn abilities(&self) -> &[&'static dyn ClassAbility] {
        &[]
    }
}

/// Whether an attack is a melee strike or a missile shot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackKind {
    /// A melee strike.
    Melee,
    /// A missile shot.
    Missile,
}

/// A read-only view of an attack about to be rolled, for [`ClassAbility`]
/// hooks.
pub struct AttackContext<'a> {
    pub(crate) attacker: &'a crate::combatant::Combatant,
    pub(crate) target: &'a crate::combatant::Combatant,
    pub(crate) kind: AttackKind,
    pub(crate) target_surprised: bool,
}

impl AttackContext<'_> {
    /// The attacker.
    pub fn attacker(&self) -> &crate::combatant::Combatant {
        self.attacker
    }

    /// The target.
    pub fn target(&self) -> &crate::combatant::Combatant {
        self.target
    }

    /// Melee or missile.
    pub fn kind(&self) -> AttackKind {
        self.kind
    }

    /// Whether the target's side is surprised this round.
    pub fn target_surprised(&self) -> bool {
        self.target_surprised
    }

    /// Whether the target is unaware of the attack: surprised, asleep, or
    /// paralysed. Back-stab keys on this.
    pub fn target_unaware(&self) -> bool {
        use crate::effect::Condition;
        self.target_surprised
            || self.target.has_condition(Condition::Asleep)
            || self.target.has_condition(Condition::Paralysed)
    }
}

/// How a hook changes the damage of a hit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DamageAdjustment {
    /// Damage as normal.
    #[default]
    Normal,
    /// Damage is doubled after all modifiers.
    Doubled,
    /// The damage dice are replaced.
    Replaced(crate::dice::DiceExpr),
}

/// A hook's adjustment to an attack about to be rolled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttackAdjustment {
    /// Added to the attack roll.
    pub bonus: crate::ability::Modifier,
    /// Applied to the damage of a hit.
    pub damage: DamageAdjustment,
}

impl AttackAdjustment {
    /// No adjustment.
    pub const NONE: AttackAdjustment = AttackAdjustment {
        bonus: crate::ability::Modifier::ZERO,
        damage: DamageAdjustment::Normal,
    };
}

/// A class feature that participates in combat.
///
/// The kernel calls these hooks at narrow points; it hardcodes no specific
/// ability. The built-in classes use the same hooks (the thief's back-stab
/// is [`BackStab`]). Implementations are usually zero-sized and `'static`.
pub trait ClassAbility {
    /// The ability name, for presentation.
    fn name(&self) -> &str;

    /// Called when the owner attacks, before the roll.
    fn modify_attack(&self, ctx: &AttackContext<'_>) -> AttackAdjustment {
        let _ = ctx;
        AttackAdjustment::NONE
    }

    /// Called when the owner is attacked, before the roll. The returned
    /// modifier adjusts the owner's armour class for this attack.
    fn modify_defence(&self, ctx: &AttackContext<'_>) -> crate::ability::Modifier {
        let _ = ctx;
        crate::ability::Modifier::ZERO
    }
}

/// The thief's back-stab: +4 to hit and doubled damage against an unaware
/// target (surprised, asleep, or paralysed).
pub struct BackStab;

/// Back-stab, as a ready-to-use constant.
pub const BACK_STAB: BackStab = BackStab;

impl ClassAbility for BackStab {
    fn name(&self) -> &str {
        "back-stab"
    }

    fn modify_attack(&self, ctx: &AttackContext<'_>) -> AttackAdjustment {
        if ctx.kind() == AttackKind::Melee && ctx.target_unaware() {
            AttackAdjustment {
                bonus: crate::ability::Modifier::new(4),
                damage: DamageAdjustment::Doubled,
            }
        } else {
            AttackAdjustment::NONE
        }
    }
}

// ---------------------------------------------------------------------------
// Attack progressions
// ---------------------------------------------------------------------------

const fn ab(bonus: i8) -> AttackBonus {
    AttackBonus::new(bonus)
}

/// The martial attack progression: +2 at 4, +5 at 7, +7 at 10, +9 at 13.
pub const MARTIAL_ATTACK: Progression<AttackBonus> =
    Progression::new(&[(3, ab(0)), (6, ab(2)), (9, ab(5)), (12, ab(7)), (14, ab(9))]);

/// The devout and stealthy attack progression: +2 at 5, +5 at 9, +7 at 13.
pub const DEVOUT_ATTACK: Progression<AttackBonus> =
    Progression::new(&[(4, ab(0)), (8, ab(2)), (12, ab(5)), (14, ab(7))]);

/// The arcane attack progression: +2 at 6, +5 at 11.
pub const ARCANE_ATTACK: Progression<AttackBonus> =
    Progression::new(&[(5, ab(0)), (10, ab(2)), (14, ab(5))]);

// ---------------------------------------------------------------------------
// Saving throw progressions
// ---------------------------------------------------------------------------

const fn sv(d: u8, w: u8, p: u8, b: u8, s: u8) -> SavingThrowProfile {
    SavingThrowProfile::of(d, w, p, b, s)
}

/// The martial saving throw progression.
pub const MARTIAL_SAVES: Progression<SavingThrowProfile> = Progression::new(&[
    (3, sv(12, 13, 14, 15, 16)),
    (6, sv(10, 11, 12, 13, 14)),
    (9, sv(8, 9, 10, 10, 12)),
    (12, sv(6, 7, 8, 8, 10)),
    (14, sv(4, 5, 6, 5, 8)),
]);

/// The devout saving throw progression.
pub const DEVOUT_SAVES: Progression<SavingThrowProfile> = Progression::new(&[
    (4, sv(11, 12, 14, 16, 15)),
    (8, sv(9, 10, 12, 14, 12)),
    (12, sv(6, 7, 9, 11, 9)),
    (14, sv(3, 5, 7, 8, 7)),
]);

/// The arcane saving throw progression.
pub const ARCANE_SAVES: Progression<SavingThrowProfile> = Progression::new(&[
    (5, sv(13, 14, 13, 16, 15)),
    (10, sv(11, 12, 11, 14, 12)),
    (14, sv(8, 9, 8, 11, 8)),
]);

/// The stealthy saving throw progression.
pub const STEALTHY_SAVES: Progression<SavingThrowProfile> = Progression::new(&[
    (4, sv(13, 14, 13, 16, 15)),
    (8, sv(12, 13, 11, 14, 13)),
    (12, sv(10, 11, 9, 12, 10)),
    (14, sv(8, 9, 7, 10, 8)),
]);

// ---------------------------------------------------------------------------
// Spell slot progressions
// ---------------------------------------------------------------------------

const fn slots(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> SpellSlots {
    SpellSlots::new([a, b, c, d, e, f])
}

/// The devout spell slot progression (spell levels one to five).
pub const DEVOUT_SLOTS: Progression<SpellSlots> = Progression::new(&[
    (1, SpellSlots::NONE),
    (2, slots(1, 0, 0, 0, 0, 0)),
    (3, slots(2, 0, 0, 0, 0, 0)),
    (4, slots(2, 1, 0, 0, 0, 0)),
    (5, slots(2, 2, 0, 0, 0, 0)),
    (6, slots(2, 2, 1, 1, 0, 0)),
    (7, slots(2, 2, 2, 1, 1, 0)),
    (8, slots(3, 3, 2, 2, 1, 0)),
    (9, slots(3, 3, 3, 2, 2, 0)),
    (10, slots(4, 4, 3, 3, 2, 0)),
    (11, slots(4, 4, 4, 3, 3, 0)),
    (12, slots(5, 5, 4, 4, 3, 0)),
    (13, slots(5, 5, 5, 4, 4, 0)),
    (14, slots(6, 5, 5, 5, 4, 0)),
]);

/// The arcane spell slot progression (spell levels one to six).
pub const ARCANE_SLOTS: Progression<SpellSlots> = Progression::new(&[
    (1, slots(1, 0, 0, 0, 0, 0)),
    (2, slots(2, 0, 0, 0, 0, 0)),
    (3, slots(2, 1, 0, 0, 0, 0)),
    (4, slots(2, 2, 0, 0, 0, 0)),
    (5, slots(2, 2, 1, 0, 0, 0)),
    (6, slots(2, 2, 2, 0, 0, 0)),
    (7, slots(3, 2, 2, 1, 0, 0)),
    (8, slots(3, 3, 2, 2, 0, 0)),
    (9, slots(3, 3, 3, 2, 1, 0)),
    (10, slots(3, 3, 3, 3, 2, 0)),
    (11, slots(4, 3, 3, 3, 2, 1)),
    (12, slots(4, 4, 3, 3, 3, 2)),
    (13, slots(4, 4, 4, 3, 3, 3)),
    (14, slots(4, 4, 4, 4, 3, 3)),
]);

// ---------------------------------------------------------------------------
// The four built-in classes
// ---------------------------------------------------------------------------

const LEVEL_14: Level = match Level::new(14) {
    Some(l) => l,
    None => unreachable!(),
};

/// The fighter: d8 hit die, the best attack progression, any armour, any
/// weapon.
pub struct Fighter;

/// The fighter, as a ready-to-use constant.
pub const FIGHTER: Fighter = Fighter;

impl Class for Fighter {
    fn name(&self) -> &str {
        "fighter"
    }

    fn hit_die(&self) -> Die {
        Die::D8
    }

    fn max_level(&self) -> Level {
        LEVEL_14
    }

    fn attack_bonus(&self, level: Level) -> AttackBonus {
        MARTIAL_ATTACK.at(level)
    }

    fn saves(&self, level: Level) -> SavingThrowProfile {
        MARTIAL_SAVES.at(level)
    }

    fn hp_bonus_per_level_beyond_ninth(&self) -> u8 {
        2
    }
}

/// The devout warrior: d6 hit die, any armour, blunt weapons only, divine
/// spells from second level.
pub struct Cleric;

/// The cleric, as a ready-to-use constant.
pub const CLERIC: Cleric = Cleric;

impl Class for Cleric {
    fn name(&self) -> &str {
        "cleric"
    }

    fn hit_die(&self) -> Die {
        Die::D6
    }

    fn max_level(&self) -> Level {
        LEVEL_14
    }

    fn attack_bonus(&self, level: Level) -> AttackBonus {
        DEVOUT_ATTACK.at(level)
    }

    fn saves(&self, level: Level) -> SavingThrowProfile {
        DEVOUT_SAVES.at(level)
    }

    fn spell_slots(&self, level: Level) -> SpellSlots {
        DEVOUT_SLOTS.at(level)
    }

    fn may_wield(&self, weapon: &Weapon) -> bool {
        weapon.blunt
    }
}

/// The arcane caster: d4 hit die, no armour, small weapons only, arcane
/// spells from first level.
pub struct MagicUser;

/// The magic-user, as a ready-to-use constant.
pub const MAGIC_USER: MagicUser = MagicUser;

impl Class for MagicUser {
    fn name(&self) -> &str {
        "magic-user"
    }

    fn hit_die(&self) -> Die {
        Die::D4
    }

    fn max_level(&self) -> Level {
        LEVEL_14
    }

    fn attack_bonus(&self, level: Level) -> AttackBonus {
        ARCANE_ATTACK.at(level)
    }

    fn saves(&self, level: Level) -> SavingThrowProfile {
        ARCANE_SAVES.at(level)
    }

    fn spell_slots(&self, level: Level) -> SpellSlots {
        ARCANE_SLOTS.at(level)
    }

    fn may_wear(&self, armour: Armour, shield: bool) -> bool {
        matches!(armour, Armour::None) && !shield
    }

    fn may_wield(&self, weapon: &Weapon) -> bool {
        weapon.small
    }
}

/// The skulker: d4 hit die, light armour, no shield, any weapon.
pub struct Thief;

/// The thief, as a ready-to-use constant.
pub const THIEF: Thief = Thief;

impl Class for Thief {
    fn name(&self) -> &str {
        "thief"
    }

    fn hit_die(&self) -> Die {
        Die::D4
    }

    fn max_level(&self) -> Level {
        LEVEL_14
    }

    fn attack_bonus(&self, level: Level) -> AttackBonus {
        DEVOUT_ATTACK.at(level)
    }

    fn saves(&self, level: Level) -> SavingThrowProfile {
        STEALTHY_SAVES.at(level)
    }

    fn hp_bonus_per_level_beyond_ninth(&self) -> u8 {
        2
    }

    fn may_wear(&self, armour: Armour, shield: bool) -> bool {
        matches!(armour, Armour::None | Armour::Light) && !shield
    }

    fn abilities(&self) -> &[&'static dyn ClassAbility] {
        &[&BACK_STAB]
    }
}

// ---------------------------------------------------------------------------
// Turning the undead
// ---------------------------------------------------------------------------

/// The cleric's power to repel the undead, as a special action for the
/// magic stage (see `Combat::special`).
///
/// The kernel cannot know which combatants are undead or their hit dice;
/// the application supplies both.
pub struct TurnUndead {
    /// The cleric's level. It selects the turning table row.
    pub cleric_level: Level,
    /// The undead targets: combatant, hit dice, and whether the creature
    /// has a special ability (the starred column of the turning table).
    pub undead: alloc::vec::Vec<(crate::units::CombatantId, crate::monster::HitDice, bool)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Turning {
    No,
    Roll(u8),
    Turn,
    Destroy,
}

fn turning_entry(cleric_level: Level, hit_dice: crate::monster::HitDice, special: bool) -> Turning {
    use Turning::{Destroy as D, No, Roll, Turn as T};
    let column = match hit_dice {
        crate::monster::HitDice::Half => 0,
        crate::monster::HitDice::Whole { dice, .. } => match dice {
            0..=1 => 0,
            2 if special => 2,
            2 => 1,
            3 => 3,
            4 => 4,
            5 => 5,
            6 => 6,
            7..=9 => 7,
            _ => return No,
        },
    };
    let row: [Turning; 8] = match cleric_level.get() {
        1 => [Roll(7), Roll(9), Roll(11), No, No, No, No, No],
        2 => [T, Roll(7), Roll(9), Roll(11), No, No, No, No],
        3 => [T, T, Roll(7), Roll(9), Roll(11), No, No, No],
        4 => [D, T, T, Roll(7), Roll(9), Roll(11), No, No],
        5 => [D, D, T, T, Roll(7), Roll(9), Roll(11), No],
        6 => [D, D, D, T, T, Roll(7), Roll(9), Roll(11)],
        7 => [D, D, D, D, T, T, Roll(7), Roll(9)],
        8 => [D, D, D, D, D, T, T, Roll(7)],
        9 => [D, D, D, D, D, D, T, T],
        10 => [D, D, D, D, D, D, D, T],
        _ => [D, D, D, D, D, D, D, D],
    };
    row[column]
}

impl crate::combat::SpecialAction for TurnUndead {
    fn name(&self) -> &str {
        "turn undead"
    }

    fn resolve(
        &self,
        ctx: &crate::combat::ActionContext<'_>,
        r: &mut dyn crate::dice::DiceRoller,
    ) -> Result<
        alloc::vec::Vec<(crate::units::CombatantId, crate::effect::Effect)>,
        crate::combat::ActionError,
    > {
        use crate::dice::Die;
        use crate::effect::{Condition, Duration, Effect};

        for (id, _, _) in &self.undead {
            if ctx.combatant(*id).is_none() {
                return Err(crate::combat::ActionError::UnknownTarget);
            }
        }
        let attempt = r.roll(Die::D6) + r.roll(Die::D6);
        // Lowest hit dice are affected first.
        let mut targets = self.undead.clone();
        targets.sort_by_key(|(_, hd, _)| match hd {
            crate::monster::HitDice::Half => 0u16,
            crate::monster::HitDice::Whole { dice, .. } => u16::from(*dice) * 2 + 1,
        });
        let mut out = alloc::vec::Vec::new();
        // 2d6 hit dice are affected, rolled once the attempt succeeds. At
        // least one creature is always affected on a success.
        let mut pool: Option<i32> = None;
        for (id, hit_dice, special) in targets {
            if !ctx.combatant(id).is_some_and(|c| c.is_alive()) {
                continue;
            }
            let outcome = match turning_entry(self.cleric_level, hit_dice, special) {
                Turning::No => continue,
                Turning::Roll(need) if attempt < need => continue,
                Turning::Roll(_) | Turning::Turn => false,
                Turning::Destroy => true,
            };
            let cost = i32::from(match hit_dice {
                crate::monster::HitDice::Half => 1,
                crate::monster::HitDice::Whole { dice, .. } => dice.max(1),
            });
            let remaining = *pool
                .get_or_insert_with(|| i32::from(r.roll(Die::D6)) + i32::from(r.roll(Die::D6)));
            if !out.is_empty() && remaining < cost {
                continue;
            }
            pool = Some(remaining - cost);
            let effect = if outcome {
                Effect::Slay
            } else {
                Effect::Apply {
                    condition: Condition::Fleeing,
                    duration: Duration::UntilCombatEnds,
                }
            };
            out.push((id, effect));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::stock;

    fn level(n: u8) -> Level {
        Level::new(n).unwrap()
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn attack_band_boundaries_are_exact() {
        let cases: [(&dyn Class, &[(u8, i8)]); 4] = [
            (
                &FIGHTER,
                &[
                    (1, 0),
                    (3, 0),
                    (4, 2),
                    (6, 2),
                    (7, 5),
                    (9, 5),
                    (10, 7),
                    (12, 7),
                    (13, 9),
                    (14, 9),
                ],
            ),
            (
                &CLERIC,
                &[
                    (1, 0),
                    (4, 0),
                    (5, 2),
                    (8, 2),
                    (9, 5),
                    (12, 5),
                    (13, 7),
                    (14, 7),
                ],
            ),
            (&THIEF, &[(4, 0), (5, 2), (12, 5), (13, 7)]),
            (
                &MAGIC_USER,
                &[(1, 0), (5, 0), (6, 2), (10, 2), (11, 5), (14, 5)],
            ),
        ];
        for (class, bands) in cases {
            for &(lvl, bonus) in bands {
                assert_eq!(
                    class.attack_bonus(level(lvl)),
                    AttackBonus::new(bonus),
                    "{} {lvl}",
                    class.name()
                );
            }
        }
    }

    #[test]
    fn save_band_boundaries_are_exact() {
        let f = FIGHTER.saves(level(7));
        assert_eq!(
            (
                f.death.get(),
                f.wands.get(),
                f.paralysis.get(),
                f.breath.get(),
                f.spells.get()
            ),
            (8, 9, 10, 10, 12)
        );
        let c = CLERIC.saves(level(13));
        assert_eq!((c.death.get(), c.spells.get()), (3, 7));
        let m = MAGIC_USER.saves(level(11));
        assert_eq!((m.death.get(), m.breath.get()), (8, 11));
        let t = THIEF.saves(level(5));
        assert_eq!((t.paralysis.get(), t.spells.get()), (11, 13));
    }

    #[test]
    fn spell_slot_boundaries_are_exact() {
        assert_eq!(CLERIC.spell_slots(level(1)), SpellSlots::NONE);
        assert_eq!(CLERIC.spell_slots(level(2)).total(), 1);
        assert_eq!(
            CLERIC.spell_slots(level(14)),
            SpellSlots::new([6, 5, 5, 5, 4, 0])
        );
        assert_eq!(
            MAGIC_USER.spell_slots(level(1)),
            SpellSlots::new([1, 0, 0, 0, 0, 0])
        );
        assert_eq!(
            MAGIC_USER.spell_slots(level(9)),
            SpellSlots::new([3, 3, 3, 2, 1, 0])
        );
        assert_eq!(
            MAGIC_USER.spell_slots(level(14)),
            SpellSlots::new([4, 4, 4, 4, 3, 3])
        );
        assert_eq!(FIGHTER.spell_slots(level(14)), SpellSlots::NONE);
    }

    #[test]
    fn equipment_rules_hold() {
        assert!(CLERIC.may_wield(&stock::mace()));
        assert!(!CLERIC.may_wield(&stock::sword()));
        assert!(MAGIC_USER.may_wield(&stock::dagger()));
        assert!(!MAGIC_USER.may_wield(&stock::staff()));
        assert!(!MAGIC_USER.may_wear(Armour::Light, false));
        assert!(!THIEF.may_wear(Armour::Medium, false));
        assert!(THIEF.may_wear(Armour::Light, false));
        assert!(!THIEF.may_wear(Armour::Light, true));
        assert!(FIGHTER.may_wear(Armour::Heavy, true));
    }

    #[test]
    fn progressions_clamp_past_the_last_band() {
        assert_eq!(MARTIAL_ATTACK.at(level(99)), AttackBonus::new(9));
    }
}
