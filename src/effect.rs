//! The effect algebra.
//!
//! Spells, monster attack riders, and special actions all describe their
//! mechanics as an [`Effect`]. One interpreter applies them, so no system
//! needs bespoke branching. [`CustomEffect`] is the escape hatch for the
//! rare effect the algebra cannot express.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ability::Modifier;
use crate::combat::event::{CombatEvent, EventLog};
use crate::combatant::Combatant;
use crate::dice::{DiceExpr, DiceRoller};
use crate::health::LifeState;
use crate::save::{SaveCategory, SaveOutcome, SavingThrowProfile, resolve_save};
use crate::units::{CombatantId, Rounds};

/// A condition on a combatant.
///
/// The kernel derives capabilities from conditions: a paralysed or sleeping
/// combatant cannot act; a bound combatant cannot move, attack, or cast; a
/// silenced combatant cannot cast; a fleeing combatant can only move. The
/// other conditions are data for the application and for effects.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Condition {
    /// Poison in the blood.
    Poisoned,
    /// Unable to move or act.
    Paralysed,
    /// Unable to speak; spell casting is impossible.
    Silenced,
    /// Tied up: no movement, attacks, or gestures.
    Bound,
    /// Unable to see.
    Blinded,
    /// Under another's influence.
    Charmed,
    /// In a magical sleep; unable to act.
    Asleep,
    /// Routed: will only flee.
    Fleeing,
}

/// How long an effect lasts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Duration {
    /// Applies once, immediately.
    Instant,
    /// Lasts this many rounds, then ends at the end of a round.
    Rounds(Rounds),
    /// Lasts until the combat ends.
    UntilCombatEnds,
    /// Does not end on its own.
    Permanent,
}

impl Duration {
    /// One round toward expiry. Returns the remaining duration, or `None`
    /// when it has expired.
    #[must_use]
    pub(crate) fn ticked(self) -> Option<Duration> {
        match self {
            Duration::Instant => None,
            Duration::Rounds(Rounds(n)) => {
                if n <= 1 {
                    None
                } else {
                    Some(Duration::Rounds(Rounds(n - 1)))
                }
            }
            Duration::UntilCombatEnds | Duration::Permanent => Some(self),
        }
    }
}

/// What a successful saving throw does to the effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveMitigation {
    /// A successful save negates the effect entirely.
    Negates,
    /// A successful save halves damage dealt by the effect (round down).
    HalvesDamage,
}

/// The mechanics of a spell, rider, or special action, as data.
#[derive(Debug)]
pub enum Effect {
    /// Roll and deal damage.
    Damage(DiceExpr),
    /// Roll and restore hit points, up to the maximum.
    Heal(DiceExpr),
    /// Apply a condition for a duration.
    Apply {
        /// The condition to apply.
        condition: Condition,
        /// How long it lasts.
        duration: Duration,
    },
    /// Remove a condition.
    Remove(Condition),
    /// Adjust the target's armour class for a duration.
    ModifyArmourClass {
        /// The adjustment. Positive helps the target.
        by: Modifier,
        /// How long it lasts.
        duration: Duration,
    },
    /// Adjust the target's attack rolls for a duration.
    ModifyAttack {
        /// The adjustment. Positive helps the target.
        by: Modifier,
        /// How long it lasts.
        duration: Duration,
    },
    /// Allow a saving throw, then apply the inner effect accordingly.
    Save {
        /// The saving throw category.
        category: SaveCategory,
        /// Whether the source is magical. The wisdom modifier applies to
        /// saves against magic.
        magical: bool,
        /// What a successful save does.
        mitigation: SaveMitigation,
        /// The effect at stake.
        effect: Box<Effect>,
    },
    /// Kill the target outright: death magic, destruction, or a failed
    /// save against lethal poison.
    Slay,
    /// Apply each effect in order.
    All(Vec<Effect>),
    /// An effect the algebra cannot express.
    Custom(Box<dyn CustomEffect>),
}

impl Clone for Effect {
    fn clone(&self) -> Self {
        match self {
            Effect::Damage(d) => Effect::Damage(*d),
            Effect::Heal(d) => Effect::Heal(*d),
            Effect::Apply {
                condition,
                duration,
            } => Effect::Apply {
                condition: *condition,
                duration: *duration,
            },
            Effect::Remove(c) => Effect::Remove(*c),
            Effect::ModifyArmourClass { by, duration } => Effect::ModifyArmourClass {
                by: *by,
                duration: *duration,
            },
            Effect::ModifyAttack { by, duration } => Effect::ModifyAttack {
                by: *by,
                duration: *duration,
            },
            Effect::Save {
                category,
                magical,
                mitigation,
                effect,
            } => Effect::Save {
                category: *category,
                magical: *magical,
                mitigation: *mitigation,
                effect: effect.clone(),
            },
            Effect::Slay => Effect::Slay,
            Effect::All(effects) => Effect::All(effects.clone()),
            Effect::Custom(c) => Effect::Custom(c.boxed_clone()),
        }
    }
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Effect::Damage(a), Effect::Damage(b)) => a == b,
            (Effect::Heal(a), Effect::Heal(b)) => a == b,
            (
                Effect::Apply {
                    condition: ca,
                    duration: da,
                },
                Effect::Apply {
                    condition: cb,
                    duration: db,
                },
            ) => ca == cb && da == db,
            (Effect::Remove(a), Effect::Remove(b)) => a == b,
            (
                Effect::ModifyArmourClass {
                    by: a,
                    duration: da,
                },
                Effect::ModifyArmourClass {
                    by: b,
                    duration: db,
                },
            ) => a == b && da == db,
            (
                Effect::ModifyAttack {
                    by: a,
                    duration: da,
                },
                Effect::ModifyAttack {
                    by: b,
                    duration: db,
                },
            ) => a == b && da == db,
            (
                Effect::Save {
                    category: ca,
                    magical: ma,
                    mitigation: mia,
                    effect: ea,
                },
                Effect::Save {
                    category: cb,
                    magical: mb,
                    mitigation: mib,
                    effect: eb,
                },
            ) => ca == cb && ma == mb && mia == mib && ea == eb,
            (Effect::Slay, Effect::Slay) => true,
            (Effect::All(a), Effect::All(b)) => a == b,
            // Custom effects compare by identity: the same box, not the
            // same behaviour.
            (Effect::Custom(a), Effect::Custom(b)) => {
                core::ptr::eq(&raw const **a as *const (), &raw const **b as *const ())
            }
            _ => false,
        }
    }
}

/// An effect the built-in algebra cannot express.
///
/// The implementation sees one target through a narrow [`TargetView`]; it
/// cannot reach the rest of the combat. Prefer the built-in algebra: it
/// covers the basic rules, and custom effects do not serialize.
pub trait CustomEffect: core::fmt::Debug {
    /// Apply the effect to one target.
    fn apply(&self, target: &mut TargetView<'_>, r: &mut dyn DiceRoller);

    /// Clone into a box. Implement as `Box::new(self.clone())`.
    fn boxed_clone(&self) -> Box<dyn CustomEffect>;
}

/// What a combatant cannot be harmed by.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Immunities {
    /// Weapon attacks without an enchantment bonus hit but deal no damage.
    pub non_magical_weapons: bool,
    /// Conditions that never apply.
    pub conditions: Vec<Condition>,
}

impl Immunities {
    /// No immunities.
    pub const NONE: Immunities = Immunities {
        non_magical_weapons: false,
        conditions: Vec::new(),
    };
}

/// A mutable view of one combatant, for applying effects.
///
/// This is the whole surface a [`CustomEffect`] can touch. Every mutation
/// appends the matching event to the combat log.
pub struct TargetView<'a> {
    pub(crate) id: CombatantId,
    pub(crate) target: &'a mut Combatant,
    pub(crate) events: &'a mut EventLog,
    /// Under simultaneous initiative ties, damage lands when the tied
    /// cluster finishes, so both sides may fell each other. `Some` routes
    /// damage into that bucket.
    pub(crate) deferred: Option<&'a mut Vec<(CombatantId, u16)>>,
}

impl TargetView<'_> {
    /// The target's id.
    pub fn id(&self) -> CombatantId {
        self.id
    }

    /// The target's life state.
    pub fn life(&self) -> LifeState {
        self.target.life
    }

    /// The target's saving throw profile.
    pub fn saves(&self) -> SavingThrowProfile {
        self.target.saves
    }

    /// Whether the target has a condition.
    pub fn has_condition(&self, condition: Condition) -> bool {
        self.target.has_condition(condition)
    }

    /// Deal damage. A dead target takes none. Under simultaneous ties the
    /// damage is banked and lands when the tied cluster finishes.
    pub fn damage(&mut self, amount: u16) {
        if !self.target.life.is_alive() {
            return;
        }
        if let Some(bucket) = &mut self.deferred {
            bucket.push((self.id, amount));
            return;
        }
        self.events.push(CombatEvent::DamageDealt {
            target: self.id,
            amount,
        });
        self.target.life = self.target.life.damaged(amount);
        if !self.target.life.is_alive() {
            self.events.push(CombatEvent::Died { who: self.id });
        }
    }

    /// Restore hit points, up to the maximum. The dead stay dead.
    pub fn heal(&mut self, amount: u16) {
        if !self.target.life.is_alive() {
            return;
        }
        self.target.life = self.target.life.healed(amount);
        self.events.push(CombatEvent::Healed {
            target: self.id,
            amount,
        });
    }

    /// Apply a condition, unless the target is immune to it.
    pub fn apply_condition(&mut self, condition: Condition, duration: Duration) {
        if self.target.immunities.conditions.contains(&condition) {
            return;
        }
        self.target.conditions.push((condition, duration));
        self.events.push(CombatEvent::ConditionApplied {
            who: self.id,
            condition,
        });
    }

    /// Remove every instance of a condition.
    pub fn remove_condition(&mut self, condition: Condition) {
        let before = self.target.conditions.len();
        self.target.conditions.retain(|(c, _)| *c != condition);
        if self.target.conditions.len() < before {
            self.events.push(CombatEvent::ConditionEnded {
                who: self.id,
                condition,
            });
        }
    }

    /// Adjust the target's armour class for a duration.
    pub fn modify_armour_class(&mut self, by: Modifier, duration: Duration) {
        self.target.temp.ac.push((by, duration));
    }

    /// Adjust the target's attack rolls for a duration.
    pub fn modify_attack(&mut self, by: Modifier, duration: Duration) {
        self.target.temp.attack.push((by, duration));
    }

    /// Kill the target outright.
    pub fn slay(&mut self) {
        if let Some(current) = self.target.life.current() {
            self.damage(current.get());
        }
    }

    /// Roll a saving throw for the target and log it.
    pub fn save(
        &mut self,
        category: SaveCategory,
        magical: bool,
        r: &mut dyn DiceRoller,
    ) -> SaveOutcome {
        let bonus = if magical {
            self.target.magic_save_modifier
        } else {
            Modifier::ZERO
        };
        let (natural, outcome) = resolve_save(r, self.target.saves, category, bonus);
        self.events.push(CombatEvent::SaveRolled {
            who: self.id,
            category,
            natural,
            outcome,
        });
        outcome
    }

    /// Append a free-text note to the combat log, for custom effects that
    /// need narration.
    pub fn note(&mut self, text: String) {
        self.events.push(CombatEvent::Note {
            who: Some(self.id),
            text,
        });
    }
}

/// Apply an effect to one target.
pub(crate) fn apply_effect(effect: &Effect, view: &mut TargetView<'_>, r: &mut dyn DiceRoller) {
    apply_scaled(effect, view, r, false);
}

fn apply_scaled(effect: &Effect, view: &mut TargetView<'_>, r: &mut dyn DiceRoller, halve: bool) {
    match effect {
        Effect::Damage(dice) => {
            let rolled = dice.roll(r).max(0) as u16;
            let amount = if halve { rolled / 2 } else { rolled };
            view.damage(amount);
        }
        Effect::Heal(dice) => {
            let rolled = dice.roll(r).max(0) as u16;
            view.heal(rolled);
        }
        Effect::Apply {
            condition,
            duration,
        } => view.apply_condition(*condition, *duration),
        Effect::Remove(condition) => view.remove_condition(*condition),
        Effect::ModifyArmourClass { by, duration } => view.modify_armour_class(*by, *duration),
        Effect::ModifyAttack { by, duration } => view.modify_attack(*by, *duration),
        Effect::Save {
            category,
            magical,
            mitigation,
            effect,
        } => match (view.save(*category, *magical, r), mitigation) {
            (SaveOutcome::Success, SaveMitigation::Negates) => {}
            (SaveOutcome::Success, SaveMitigation::HalvesDamage) => {
                apply_scaled(effect, view, r, true);
            }
            (SaveOutcome::Failure, _) => apply_scaled(effect, view, r, halve),
        },
        Effect::Slay => view.slay(),
        Effect::All(effects) => {
            for e in effects {
                apply_scaled(e, view, r, halve);
            }
        }
        Effect::Custom(custom) => custom.apply(view, r),
    }
}
