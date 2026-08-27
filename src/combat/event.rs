//! Combat events: the typed facts a combat emits.
//!
//! The kernel appends every fact to one [`EventLog`]. A presentation layer
//! reads the log and turns facts into animation, sound, or prose; it never
//! needs to know why a fact occurred.

use alloc::string::String;
use alloc::vec::Vec;

use crate::attack::NaturalRoll;
use crate::effect::Condition;
use crate::magic::SpellRef;
use crate::morale::MoraleOutcome;
use crate::save::{SaveCategory, SaveOutcome};
use crate::units::{CombatantId, Feet, RoundNumber, SideId};

/// How a combatant moved.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveKind {
    /// Ordinary movement, up to the encounter rate.
    Normal,
    /// A fighting withdrawal: backwards, at up to half the encounter rate,
    /// while staying engaged.
    Withdrawal,
    /// A full retreat from melee: up to the encounter rate, no attack this
    /// round, easier to hit.
    Retreat,
}

/// One fact from a combat.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum CombatEvent {
    /// A round began.
    RoundStarted {
        /// The round number, starting at one.
        round: RoundNumber,
    },
    /// A combatant declared a spell for this round.
    SpellDeclared {
        /// The declaring caster.
        caster: CombatantId,
        /// The memorized spell.
        spell: SpellRef,
    },
    /// A combatant declared a fighting withdrawal or retreat.
    MovementDeclared {
        /// The declaring combatant.
        who: CombatantId,
        /// Withdrawal or retreat.
        kind: MoveKind,
    },
    /// A side rolled initiative.
    SideInitiative {
        /// The side.
        side: SideId,
        /// The d6 result.
        roll: u8,
    },
    /// A combatant rolled individual initiative.
    IndividualInitiative {
        /// The combatant.
        who: CombatantId,
        /// The d6 result plus the dexterity modifier.
        total: i8,
    },
    /// The turn order for the round is fixed.
    TurnOrderSet {
        /// The sides in acting order. A side appears once per acting group.
        order: Vec<SideId>,
    },
    /// A side owed and made a morale check.
    MoraleChecked {
        /// The side.
        side: SideId,
        /// The 2d6 result.
        roll: u8,
        /// Held or broke.
        outcome: MoraleOutcome,
    },
    /// A side broke: its creatures flee or surrender.
    SideBroke {
        /// The side.
        side: SideId,
    },
    /// A combatant moved.
    Moved {
        /// The mover.
        who: CombatantId,
        /// How.
        kind: MoveKind,
        /// How far, in feet.
        distance: Feet,
    },
    /// An attack was rolled.
    AttackRolled {
        /// The attacker.
        attacker: CombatantId,
        /// The target.
        target: CombatantId,
        /// The raw d20 face.
        natural: NaturalRoll,
        /// The modified total.
        total: i16,
        /// Whether it hit.
        hit: bool,
    },
    /// Damage was applied.
    DamageDealt {
        /// The victim.
        target: CombatantId,
        /// The amount, after modifiers.
        amount: u16,
    },
    /// Hit points were restored.
    Healed {
        /// The recipient.
        target: CombatantId,
        /// The amount restored.
        amount: u16,
    },
    /// A combatant died.
    Died {
        /// The deceased.
        who: CombatantId,
    },
    /// A saving throw was rolled.
    SaveRolled {
        /// The saver.
        who: CombatantId,
        /// The category.
        category: SaveCategory,
        /// The raw d20 face.
        natural: NaturalRoll,
        /// Success or failure.
        outcome: SaveOutcome,
    },
    /// A condition was applied.
    ConditionApplied {
        /// The affected combatant.
        who: CombatantId,
        /// The condition.
        condition: Condition,
    },
    /// A condition ended.
    ConditionEnded {
        /// The affected combatant.
        who: CombatantId,
        /// The condition.
        condition: Condition,
    },
    /// A declared spell was cast.
    SpellCast {
        /// The caster.
        caster: CombatantId,
        /// The memorized spell.
        spell: SpellRef,
    },
    /// A declared spell was disrupted and lost.
    SpellDisrupted {
        /// The caster.
        caster: CombatantId,
        /// The lost spell.
        spell: SpellRef,
    },
    /// A round ended.
    RoundEnded {
        /// The round number.
        round: RoundNumber,
    },
    /// The combat ended.
    CombatEnded {
        /// The sides still standing. Empty on mutual destruction.
        standing: Vec<SideId>,
    },
    /// Free-text narration from a custom effect or ability.
    Note {
        /// The combatant concerned, if any.
        who: Option<CombatantId>,
        /// The text.
        text: String,
    },
}

/// A position in an [`EventLog`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct EventCursor(usize);

/// The append-only log of one combat.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default)]
pub struct EventLog {
    events: Vec<CombatEvent>,
}

impl EventLog {
    pub(crate) fn new() -> Self {
        EventLog { events: Vec::new() }
    }

    pub(crate) fn push(&mut self, event: CombatEvent) {
        self.events.push(event);
    }

    /// Every event so far, in order.
    pub fn all(&self) -> &[CombatEvent] {
        &self.events
    }

    /// The current end of the log. Pass it to [`EventLog::since`] later to
    /// read only what happened after this point.
    pub fn cursor(&self) -> EventCursor {
        EventCursor(self.events.len())
    }

    /// The events appended since a cursor, and a new cursor at the end.
    pub fn since(&self, cursor: EventCursor) -> (&[CombatEvent], EventCursor) {
        let start = cursor.0.min(self.events.len());
        (&self.events[start..], self.cursor())
    }
}
