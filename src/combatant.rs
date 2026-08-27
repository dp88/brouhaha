//! Combatants: validated sheets ready to fight.
//!
//! A [`Combatant`] is a snapshot of derived numbers plus mutable combat
//! state. The class trait is consulted once, when the sheet is built; combat
//! never asks it anything again.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ability::{Abilities, Modifier};
use crate::armour::{Armour, ArmourClass, DescendingAc, SHIELD_BONUS, Thac0};
use crate::attack::AttackBonus;
use crate::class::{Class, ClassAbility, Level};
use crate::combat::event::{CombatEvent, EventLog};
use crate::dice::DiceRoller;
use crate::effect::{Condition, Duration, Immunities};
use crate::health::{HitPoints, LifeState};
use crate::magic::{DeclaredTarget, Memorized, SpellRef};
use crate::monster::{MonsterAttack, MonsterKind};
use crate::morale::Morale;
use crate::nonempty::NonEmpty;
use crate::units::Feet;
use crate::weapon::Weapon;

/// Where a declared spell stands within the round.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellState {
    /// Declared and not yet cast.
    Pending,
    /// Disrupted by a hit or a failed save before the caster acted. The
    /// spell is lost from memory.
    Disrupted,
    /// Cast this round.
    Cast,
}

/// What a combatant committed to in the declaration phase.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum RoundCommitment {
    /// No commitment: free to move and attack.
    #[default]
    Uncommitted,
    /// Committed to casting: the sole action this round, no movement.
    SpellCommitted {
        /// The declared memorized spell.
        spell: SpellRef,
        /// The declared target.
        target: DeclaredTarget,
        /// Where the spell stands.
        state: SpellState,
    },
    /// Committed to a fighting withdrawal: half the encounter rate,
    /// backwards, still able to fight.
    FightingWithdrawal,
    /// Committed to a full retreat: no attack, easier to hit, shield
    /// ignored.
    Retreat,
}

/// An error building an adventurer's sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetError {
    /// The level exceeds the class maximum.
    LevelAboveClassMax,
    /// The class may not wear this armour or shield.
    ArmourForbidden,
    /// The class may not wield one of these weapons.
    WeaponForbidden,
    /// A shield cannot be carried with a two-handed weapon.
    ShieldWithTwoHandedWeapon,
    /// An adventurer needs at least one weapon.
    NoWeapons,
}

pub(crate) enum Arsenal {
    /// An adventurer's weapons. One is wielded at a time.
    Weapons(NonEmpty<Weapon>),
    /// A monster's attack routine. All of it runs each round.
    Routine(NonEmpty<MonsterAttack>),
}

impl Clone for Arsenal {
    fn clone(&self) -> Self {
        match self {
            Arsenal::Weapons(w) => Arsenal::Weapons(w.clone()),
            Arsenal::Routine(r) => Arsenal::Routine(r.clone()),
        }
    }
}

/// Temporary modifiers from effects, with their remaining durations.
#[derive(Clone, Debug, Default)]
pub(crate) struct TempModifiers {
    pub(crate) attack: Vec<(Modifier, Duration)>,
    pub(crate) ac: Vec<(Modifier, Duration)>,
}

impl TempModifiers {
    pub(crate) fn attack_sum(&self) -> Modifier {
        self.attack
            .iter()
            .fold(Modifier::ZERO, |sum, (m, _)| sum.plus(*m))
    }

    pub(crate) fn ac_sum(&self) -> Modifier {
        self.ac
            .iter()
            .fold(Modifier::ZERO, |sum, (m, _)| sum.plus(*m))
    }

    fn tick(&mut self) {
        for list in [&mut self.attack, &mut self.ac] {
            let mut i = 0;
            while i < list.len() {
                match list[i].1.ticked() {
                    Some(d) => {
                        list[i].1 = d;
                        i += 1;
                    }
                    None => {
                        list.remove(i);
                    }
                }
            }
        }
    }
}

/// A validated, ready-to-fight combatant.
pub struct Combatant {
    pub(crate) name: String,
    pub(crate) armour_class: ArmourClass,
    pub(crate) shield: bool,
    pub(crate) attack_bonus: AttackBonus,
    pub(crate) melee_modifier: Modifier,
    pub(crate) missile_modifier: Modifier,
    pub(crate) magic_save_modifier: Modifier,
    pub(crate) initiative_modifier: Modifier,
    pub(crate) saves: crate::save::SavingThrowProfile,
    pub(crate) speed: Feet,
    pub(crate) arsenal: Arsenal,
    pub(crate) wielded: usize,
    pub(crate) morale: Option<Morale>,
    pub(crate) spells: Option<Memorized>,
    pub(crate) hooks: Vec<&'static dyn ClassAbility>,
    pub(crate) immunities: Immunities,
    pub(crate) xp_value: u32,
    // Mutable combat state.
    pub(crate) life: LifeState,
    pub(crate) conditions: Vec<(Condition, Duration)>,
    pub(crate) commitment: RoundCommitment,
    pub(crate) temp: TempModifiers,
    pub(crate) attacked_this_round: bool,
    pub(crate) moved_this_round: bool,
    pub(crate) fired_this_round: bool,
    pub(crate) fired_last_round: bool,
}

impl Clone for Combatant {
    fn clone(&self) -> Self {
        Combatant {
            name: self.name.clone(),
            armour_class: self.armour_class,
            shield: self.shield,
            attack_bonus: self.attack_bonus,
            melee_modifier: self.melee_modifier,
            missile_modifier: self.missile_modifier,
            magic_save_modifier: self.magic_save_modifier,
            initiative_modifier: self.initiative_modifier,
            saves: self.saves,
            speed: self.speed,
            arsenal: self.arsenal.clone(),
            wielded: self.wielded,
            morale: self.morale,
            spells: self.spells.clone(),
            hooks: self.hooks.clone(),
            immunities: self.immunities.clone(),
            xp_value: self.xp_value,
            life: self.life,
            conditions: self.conditions.clone(),
            commitment: self.commitment.clone(),
            temp: self.temp.clone(),
            attacked_this_round: self.attacked_this_round,
            moved_this_round: self.moved_this_round,
            fired_this_round: self.fired_this_round,
            fired_last_round: self.fired_last_round,
        }
    }
}

impl core::fmt::Debug for Combatant {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Combatant")
            .field("name", &self.name)
            .field("life", &self.life)
            .field("armour_class", &self.armour_class)
            .field("attack_bonus", &self.attack_bonus)
            .field("conditions", &self.conditions)
            .field("commitment", &self.commitment)
            .finish_non_exhaustive()
    }
}

impl Combatant {
    /// Spawn a combatant from a monster definition, rolling its hit points.
    pub fn monster(kind: &MonsterKind, r: &mut dyn DiceRoller) -> Combatant {
        let hp = kind.hit_dice.roll_hp(r);
        Combatant {
            name: kind.name.clone(),
            armour_class: kind.armour_class,
            shield: false,
            attack_bonus: kind
                .attack_bonus
                .unwrap_or_else(|| kind.hit_dice.attack_bonus()),
            melee_modifier: Modifier::ZERO,
            missile_modifier: Modifier::ZERO,
            magic_save_modifier: Modifier::ZERO,
            initiative_modifier: Modifier::ZERO,
            saves: kind.saves.unwrap_or_else(|| kind.hit_dice.saves()),
            speed: kind.speed,
            arsenal: Arsenal::Routine(kind.routine.clone()),
            wielded: 0,
            morale: Some(kind.morale),
            spells: None,
            hooks: Vec::new(),
            immunities: kind.immunities.clone(),
            xp_value: kind.hit_dice.xp(kind.special_abilities),
            life: LifeState::fresh(hp),
            conditions: Vec::new(),
            commitment: RoundCommitment::Uncommitted,
            temp: TempModifiers::default(),
            attacked_this_round: false,
            moved_this_round: false,
            fired_this_round: false,
            fired_last_round: false,
        }
    }

    /// The display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The life state.
    pub fn life(&self) -> LifeState {
        self.life
    }

    /// Whether the combatant lives.
    pub fn is_alive(&self) -> bool {
        self.life.is_alive()
    }

    /// The base ascending armour class, including armour, shield, and
    /// dexterity, before temporary effects.
    pub fn armour_class(&self) -> ArmourClass {
        self.armour_class
    }

    /// The descending armour class, for display.
    pub fn descending_ac(&self) -> DescendingAc {
        self.armour_class.descending()
    }

    /// The attack bonus.
    pub fn attack_bonus(&self) -> AttackBonus {
        self.attack_bonus
    }

    /// The THAC0 equivalent of the attack bonus, for display.
    pub fn thac0(&self) -> Thac0 {
        Thac0::from_attack_bonus(self.attack_bonus)
    }

    /// The base movement rate.
    pub fn speed(&self) -> Feet {
        self.speed
    }

    /// The movement allowance per combat round: a third of the base rate.
    pub fn encounter_rate(&self) -> Feet {
        Feet(self.speed.0 / 3)
    }

    /// The saving throw profile.
    pub fn saves(&self) -> crate::save::SavingThrowProfile {
        self.saves
    }

    /// This round's commitment.
    pub fn commitment(&self) -> &RoundCommitment {
        &self.commitment
    }

    /// The memorized spells, for a caster.
    pub fn spells(&self) -> Option<&Memorized> {
        self.spells.as_ref()
    }

    /// The morale score. `None` never checks morale.
    pub fn morale(&self) -> Option<Morale> {
        self.morale
    }

    /// Whether the combatant has a condition.
    pub fn has_condition(&self, condition: Condition) -> bool {
        self.conditions.iter().any(|(c, _)| *c == condition)
    }

    /// The conditions currently in force.
    pub fn conditions(&self) -> impl Iterator<Item = Condition> + '_ {
        self.conditions.iter().map(|(c, _)| *c)
    }

    /// The wielded weapon, for an adventurer. `None` for a monster.
    pub fn wielded(&self) -> Option<&Weapon> {
        match &self.arsenal {
            Arsenal::Weapons(weapons) => weapons.get(self.wielded),
            Arsenal::Routine(_) => None,
        }
    }

    /// The carried weapons, for an adventurer. Empty for a monster.
    pub fn weapons(&self) -> impl Iterator<Item = &Weapon> {
        let weapons = match &self.arsenal {
            Arsenal::Weapons(weapons) => Some(weapons.iter()),
            Arsenal::Routine(_) => None,
        };
        weapons.into_iter().flatten()
    }

    pub(crate) fn set_wielded(&mut self, index: usize) -> bool {
        match &self.arsenal {
            Arsenal::Weapons(weapons) if weapons.get(index).is_some() => {
                self.wielded = index;
                true
            }
            _ => false,
        }
    }

    // Capability checks. Conditions gate them; see `Condition`.

    pub(crate) fn can_act(&self) -> bool {
        self.life.is_alive()
            && !self.has_condition(Condition::Paralysed)
            && !self.has_condition(Condition::Asleep)
    }

    pub(crate) fn can_move(&self) -> bool {
        self.can_act() && !self.has_condition(Condition::Bound)
    }

    pub(crate) fn can_attack(&self) -> bool {
        self.can_move() && !self.has_condition(Condition::Fleeing)
    }

    pub(crate) fn can_cast(&self) -> bool {
        self.can_attack() && !self.has_condition(Condition::Silenced)
    }

    /// The armour class an attacker must beat right now, with temporary
    /// effects, and with the shield ignored against a retreating target.
    pub(crate) fn defence_class(&self) -> ArmourClass {
        let mut class = self.armour_class.adjusted(self.temp.ac_sum());
        if self.shield && matches!(self.commitment, RoundCommitment::Retreat) {
            class = class.adjusted(Modifier::new(-SHIELD_BONUS.value()));
        }
        class
    }

    /// End-of-round bookkeeping: expire durations, clear the commitment.
    pub(crate) fn tick_round(&mut self, id: crate::units::CombatantId, events: &mut EventLog) {
        self.temp.tick();
        let mut expired: Vec<Condition> = Vec::new();
        let mut i = 0;
        while i < self.conditions.len() {
            match self.conditions[i].1.ticked() {
                Some(d) => {
                    self.conditions[i].1 = d;
                    i += 1;
                }
                None => {
                    expired.push(self.conditions[i].0);
                    self.conditions.remove(i);
                }
            }
        }
        for condition in expired {
            if !self.has_condition(condition) {
                events.push(CombatEvent::ConditionEnded { who: id, condition });
            }
        }
        self.commitment = RoundCommitment::Uncommitted;
        self.attacked_this_round = false;
        self.moved_this_round = false;
        self.fired_last_round = self.fired_this_round;
        self.fired_this_round = false;
    }
}

/// A builder for an adventurer's sheet.
///
/// ```
/// use brouhaha::ability::{Abilities, AbilityScore};
/// use brouhaha::class::{FIGHTER, Level};
/// use brouhaha::combatant::AdventurerSheet;
/// use brouhaha::armour::Armour;
/// use brouhaha::dice::SeededDice;
/// use brouhaha::weapon::stock;
///
/// let scores = Abilities::flat(AbilityScore::new(13).unwrap());
/// let mut dice = SeededDice::seeded(1);
/// let fighter = AdventurerSheet::new("Aldra", &FIGHTER, Level::new(3).unwrap(), scores)
///     .armour(Armour::Medium)
///     .shield()
///     .weapon(stock::sword())
///     .build(&mut dice)
///     .unwrap();
/// assert!(fighter.is_alive());
/// ```
pub struct AdventurerSheet<'a> {
    class: &'a dyn Class,
    name: String,
    level: Level,
    abilities: Abilities,
    armour: Armour,
    shield: bool,
    weapons: Vec<Weapon>,
    speed: Feet,
    morale: Option<Morale>,
    spells: Option<Memorized>,
}

impl<'a> AdventurerSheet<'a> {
    /// Start a sheet: unarmoured, no shield, no weapons, speed 120, no
    /// morale checks (adventurers are heroic), no spells.
    pub fn new(name: &str, class: &'a dyn Class, level: Level, abilities: Abilities) -> Self {
        AdventurerSheet {
            class,
            name: String::from(name),
            level,
            abilities,
            armour: Armour::None,
            shield: false,
            weapons: Vec::new(),
            speed: Feet(120),
            morale: None,
            spells: None,
        }
    }

    /// Wear armour.
    #[must_use]
    pub fn armour(mut self, armour: Armour) -> Self {
        self.armour = armour;
        self
    }

    /// Carry a shield.
    #[must_use]
    pub fn shield(mut self) -> Self {
        self.shield = true;
        self
    }

    /// Carry a weapon. The first weapon added is wielded first.
    #[must_use]
    pub fn weapon(mut self, weapon: Weapon) -> Self {
        self.weapons.push(weapon);
        self
    }

    /// Set the base movement rate. The default is 120 feet; lower it for
    /// encumbrance.
    #[must_use]
    pub fn speed(mut self, speed: Feet) -> Self {
        self.speed = speed;
        self
    }

    /// Give the adventurer a morale score, so it checks morale like a
    /// monster. Retainers do; player characters usually do not.
    #[must_use]
    pub fn morale(mut self, morale: Morale) -> Self {
        self.morale = Some(morale);
        self
    }

    /// Memorized spells. Validate them with
    /// [`Memorized::prepare`](crate::magic::Memorized::prepare) against the
    /// class slots.
    #[must_use]
    pub fn spells(mut self, spells: Memorized) -> Self {
        self.spells = Some(spells);
        self
    }

    /// Validate everything and roll hit points.
    pub fn build(self, r: &mut dyn DiceRoller) -> Result<Combatant, SheetError> {
        if self.level > self.class.max_level() {
            return Err(SheetError::LevelAboveClassMax);
        }
        if !self.class.may_wear(self.armour, self.shield) {
            return Err(SheetError::ArmourForbidden);
        }
        for weapon in &self.weapons {
            if !self.class.may_wield(weapon) {
                return Err(SheetError::WeaponForbidden);
            }
            if weapon.two_handed && self.shield {
                return Err(SheetError::ShieldWithTwoHandedWeapon);
            }
        }
        let weapons = NonEmpty::from_vec(self.weapons).ok_or(SheetError::NoWeapons)?;

        let con = self.abilities.constitution.modifier();
        let die = self.class.hit_die();
        let mut hp: u16 = 0;
        for _ in 0..self.level.get().min(9) {
            let rolled = i16::from(r.roll(die)) + i16::from(con.value());
            hp += rolled.max(1) as u16;
        }
        if self.level.get() > 9 {
            hp += u16::from(self.class.hp_bonus_per_level_beyond_ninth())
                * u16::from(self.level.get() - 9);
        }
        let hp = HitPoints::new(hp).expect("each hit die grants at least one point");

        let shield_bonus = if self.shield {
            SHIELD_BONUS
        } else {
            Modifier::ZERO
        };
        let dex = self.abilities.dexterity.modifier();
        let armour_class = self.armour.class().adjusted(shield_bonus).adjusted(dex);

        Ok(Combatant {
            name: self.name,
            armour_class,
            shield: self.shield,
            attack_bonus: self.class.attack_bonus(self.level),
            melee_modifier: self.abilities.strength.modifier(),
            missile_modifier: dex,
            magic_save_modifier: self.abilities.wisdom.modifier(),
            initiative_modifier: dex,
            saves: self.class.saves(self.level),
            speed: self.speed,
            arsenal: Arsenal::Weapons(weapons),
            wielded: 0,
            morale: self.morale,
            spells: self.spells,
            hooks: self.class.abilities().to_vec(),
            immunities: Immunities::NONE,
            xp_value: 0,
            life: LifeState::fresh(hp),
            conditions: Vec::new(),
            commitment: RoundCommitment::Uncommitted,
            temp: TempModifiers::default(),
            attacked_this_round: false,
            moved_this_round: false,
            fired_this_round: false,
            fired_last_round: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::AbilityScore;
    use crate::class::{CLERIC, FIGHTER, MAGIC_USER, THIEF};
    use crate::dice::SeededDice;
    use crate::weapon::stock;

    fn scores(all: u8) -> Abilities {
        Abilities::flat(AbilityScore::new(all).unwrap())
    }

    fn level(n: u8) -> Level {
        Level::new(n).unwrap()
    }

    #[test]
    fn sheet_validation_catches_illegal_gear() {
        let mut r = SeededDice::seeded(2);
        let e = AdventurerSheet::new("a", &CLERIC, level(1), scores(10))
            .weapon(stock::sword())
            .build(&mut r)
            .unwrap_err();
        assert_eq!(e, SheetError::WeaponForbidden);

        let e = AdventurerSheet::new("b", &MAGIC_USER, level(1), scores(10))
            .armour(Armour::Light)
            .weapon(stock::dagger())
            .build(&mut r)
            .unwrap_err();
        assert_eq!(e, SheetError::ArmourForbidden);

        let e = AdventurerSheet::new("c", &FIGHTER, level(1), scores(10))
            .shield()
            .weapon(stock::two_handed_sword())
            .build(&mut r)
            .unwrap_err();
        assert_eq!(e, SheetError::ShieldWithTwoHandedWeapon);

        let e = AdventurerSheet::new("d", &THIEF, level(15), scores(10))
            .weapon(stock::dagger())
            .build(&mut r)
            .unwrap_err();
        assert_eq!(e, SheetError::LevelAboveClassMax);

        let e = AdventurerSheet::new("e", &FIGHTER, level(1), scores(10))
            .build(&mut r)
            .unwrap_err();
        assert_eq!(e, SheetError::NoWeapons);
    }

    #[test]
    fn derived_numbers_are_computed_once() {
        let mut r = SeededDice::seeded(9);
        let c = AdventurerSheet::new("f", &FIGHTER, level(4), scores(16))
            .armour(Armour::Medium)
            .shield()
            .weapon(stock::sword())
            .build(&mut r)
            .unwrap();
        // Medium 14 + shield 1 + dex +2.
        assert_eq!(c.armour_class(), ArmourClass::new(17));
        assert_eq!(c.attack_bonus(), AttackBonus::new(2));
        assert_eq!(c.encounter_rate(), Feet(40));
        assert_eq!(c.thac0().0, 17);
        // 4 dice at 16 CON: at least 4 * (1) and at most 4 * (8 + 2).
        let hp = c.life().current().unwrap().get();
        assert!((4..=40).contains(&hp));
    }

    #[test]
    fn hp_beyond_ninth_level_is_flat() {
        // Level 14 fighter: 9d8 (+CON each, min 1) + 5 * 2 flat.
        let mut r = SeededDice::seeded(11);
        let c = AdventurerSheet::new("g", &FIGHTER, level(14), scores(18))
            .weapon(stock::sword())
            .build(&mut r)
            .unwrap();
        let hp = c.life().current().unwrap().get();
        assert!((9 + 10..=9 * 11 + 10).contains(&hp));
    }

    #[test]
    fn retreat_forfeits_the_shield_bonus() {
        let mut r = SeededDice::seeded(4);
        let mut c = AdventurerSheet::new("h", &FIGHTER, level(1), scores(10))
            .armour(Armour::Light)
            .shield()
            .weapon(stock::sword())
            .build(&mut r)
            .unwrap();
        assert_eq!(c.defence_class(), ArmourClass::new(13));
        c.commitment = RoundCommitment::Retreat;
        assert_eq!(c.defence_class(), ArmourClass::new(12));
    }
}
