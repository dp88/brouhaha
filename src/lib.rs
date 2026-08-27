//! A type-driven combat kernel for old-school basic/expert fantasy rules.
//!
//! `brouhaha` models side-based combat rounds: declaration, initiative,
//! morale, movement, missiles, magic, and melee. The design goal is to make
//! illegal program states unrepresentable. Validated newtypes carry proof of
//! their range. Evidence types carry proof of a legal action. The combat
//! round is a typestate pipeline: a method exists only on the phase where the
//! rules allow it.
//!
//! The crate is `no_std` (with `alloc`) and has no required dependencies.
//!
//! # Layers
//!
//! - Pure rules data and math: [`dice`], [`ability`], [`health`], [`armour`],
//!   [`attack`], [`save`], [`class`], [`monster`], [`weapon`], [`effect`],
//!   [`magic`], [`morale`], [`rules`]. Usable without a combat.
//! - The combat machine: [`combat`], driven with [`combatant`]s and a
//!   [`space::SpatialOracle`].
//! - Optional grid support: the `spacewalk` feature adds `grid`.
//!
//! # Example
//!
//! A fighter duels a wild boar of two hit dice. The application supplies
//! the world through a [`space::SpatialOracle`]; here the built-in
//! [`space::AbstractField`] asserts they stand toe to toe.
//!
//! ```
//! use brouhaha::prelude::*;
//! use brouhaha::weapon::stock;
//!
//! let mut dice = SeededDice::seeded(48317);
//!
//! // A third-level fighter in medium armour with a shield and a sword.
//! let scores = Abilities::flat(AbilityScore::new(13).unwrap());
//! let aldra = AdventurerSheet::new("Aldra", &FIGHTER, Level::new(3).unwrap(), scores)
//!     .armour(Armour::Medium)
//!     .shield()
//!     .weapon(stock::sword())
//!     .build(&mut dice)
//!     .unwrap();
//!
//! // A boar: two hit dice, a d6 tusk, morale 9.
//! let mut boar_kind = MonsterKind::new(
//!     "boar",
//!     HitDice::of(2),
//!     ArmourClass::new(12),
//!     NonEmpty::of(MonsterAttack::melee("tusk", DiceExpr::of(1, Die::D6))),
//! );
//! boar_kind.morale = Morale::new(9).unwrap();
//! let boar = Combatant::monster(&boar_kind, &mut dice);
//!
//! // Two sides, toe to toe.
//! let mut builder = CombatBuilder::new(Rules::default());
//! let heroes = builder.side();
//! let beasts = builder.side();
//! let a = builder.join(heroes, aldra);
//! let b = builder.join(beasts, boar);
//! let mut field = AbstractField::new();
//! field.set_distance(a, b, Feet(5));
//! field.set_engaged(a, true);
//! field.set_engaged(b, true);
//!
//! // Run rounds until one side falls. Each phase type only offers the
//! // methods the rules allow there.
//! let mut combat = builder.begin().unwrap();
//! let winners = 'combat: loop {
//!     let mut turn = combat.roll_initiative(&mut dice);
//!     loop {
//!         let mut melee = turn
//!             .finish_morale()
//!             .finish_movement()
//!             .finish_missiles()
//!             .finish_magic();
//!         for attacker in melee.acting_members().to_vec() {
//!             let target = if attacker == a { b } else { a };
//!             if let Ok(strike) = melee.witness_melee(attacker, target, &field) {
//!                 melee.strike(strike, &mut dice).unwrap();
//!             }
//!         }
//!         match melee.finish_melee() {
//!             TurnEnd::NextGroup(next) => turn = next,
//!             TurnEnd::NewRound(next) => {
//!                 combat = next;
//!                 break;
//!             }
//!             TurnEnd::Over(done) => break 'combat done.standing(),
//!         }
//!     }
//! };
//! assert_eq!(winners.len(), 1);
//! ```
#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

/// Compiles the README examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod ability;
pub mod armour;
pub mod attack;
pub mod class;
pub mod combat;
pub mod combatant;
pub mod dice;
pub mod effect;
#[cfg(feature = "spacewalk")]
pub mod grid;
pub mod health;
pub mod magic;
pub mod monster;
pub mod morale;
pub mod nonempty;
pub mod rules;
pub mod save;
pub mod space;
pub mod units;
pub mod weapon;

pub mod prelude {
    //! One-line import for the common types.
    pub use crate::ability::{Abilities, AbilityScore, Modifier};
    pub use crate::armour::{Armour, ArmourClass, DescendingAc, Thac0};
    pub use crate::attack::{AttackBonus, AttackOutcome, NaturalRoll, resolve_attack};
    pub use crate::class::{
        AttackAdjustment, AttackContext, AttackKind, BACK_STAB, CLERIC, Class, ClassAbility,
        DamageAdjustment, FIGHTER, Level, MAGIC_USER, Progression, THIEF, TurnUndead,
    };
    pub use crate::combat::command::{AnyCombat, Command, CommandError, CommandLog};
    pub use crate::combat::event::{CombatEvent, EventCursor, EventLog, MoveKind};
    pub use crate::combat::{
        ActError, ActionContext, ActionError, CastError, Combat, CombatBuilder, Concluded,
        DeclareError, Declaring, MagicStage, MeleeStage, MissileStage, MoraleError, MoraleStage,
        MoveError, MovementStage, SetupError, SpecialAction, TurnEnd,
    };
    pub use crate::combatant::{AdventurerSheet, Combatant, RoundCommitment, SheetError};
    pub use crate::dice::{DiceExpr, DiceRoller, Die, SeededDice};
    pub use crate::effect::{
        Condition, CustomEffect, Duration, Effect, Immunities, SaveMitigation,
    };
    pub use crate::health::{HitPoints, LifeState};
    pub use crate::magic::{DeclaredTarget, Memorized, Spell, SpellLevel, SpellRef, SpellSlots};
    pub use crate::monster::{HitDice, MonsterAttack, MonsterKind};
    pub use crate::morale::{Morale, MoraleOutcome};
    pub use crate::nonempty::NonEmpty;
    pub use crate::rules::{
        DamageRule, InitiativeRule, MoraleRule, ReloadRule, Rules, SlowWeaponRule, TieRule,
    };
    pub use crate::save::{
        SaveCategory, SaveOutcome, SaveTarget, SavingThrowProfile, resolve_save,
    };
    pub use crate::space::{
        AbstractField, LegalMove, MELEE_REACH, MeleeTarget, MissileTarget, SpatialOracle,
        TargetingError,
    };
    pub use crate::units::{CombatantId, Feet, RoundNumber, Rounds, SideId};
    pub use crate::weapon::{
        AttackMode, AttackReach, Cover, CoverPenalty, MissileRanges, RangeBand, Weapon,
    };
}
