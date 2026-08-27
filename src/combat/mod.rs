//! The combat machine: a typestate pipeline over the combat round.
//!
//! A [`Combat`] is parameterised by its phase. Each phase type exposes only
//! the methods the rules allow there; calling out of turn does not compile.
//! The round runs:
//!
//! 1. [`Declaring`]: spell and melee-movement declarations, then
//!    [`Combat::roll_initiative`].
//! 2. Per acting group, in initiative order: [`MoraleStage`] →
//!    [`MovementStage`] → [`MissileStage`] → [`MagicStage`] →
//!    [`MeleeStage`].
//! 3. [`Combat::finish_melee`] returns a [`TurnEnd`]: the next group, a new
//!    round, or the end of the combat.
//!
//! Fallible operations take `&mut self`; consuming transitions never fail.
//! Every fact lands in the [`event::EventLog`].

pub mod command;
pub mod event;

use alloc::vec::Vec;
use core::marker::PhantomData;

use event::{CombatEvent, EventCursor, EventLog, MoveKind};

use crate::ability::Modifier;
use crate::attack::{AttackOutcome, resolve_attack};
use crate::class::{AttackAdjustment, AttackContext, AttackKind, DamageAdjustment};
use crate::combatant::{Arsenal, Combatant, RoundCommitment, SpellState};
use crate::dice::{DiceExpr, DiceRoller, Die};
use crate::effect::{Condition, Duration, Effect, TargetView, apply_effect};
use crate::magic::{DeclaredTarget, SpellRange, SpellRef, SpellTargeting};
use crate::morale::MoraleOutcome;
use crate::rules::{InitiativeRule, MoraleRule, ReloadRule, Rules, SlowWeaponRule, TieRule};
use crate::space::{
    LegalMove, MELEE_REACH, MeleeTarget, MissileTarget, SpatialOracle, TargetingError,
};
use crate::units::{CombatantId, Feet, RoundNumber, SideId};
use crate::weapon::{AttackReach, Cover};

// ---------------------------------------------------------------------------
// Phases
// ---------------------------------------------------------------------------

mod sealed {
    pub trait Sealed {}
}

/// A combat phase marker. Sealed: exactly the seven phase types implement
/// it.
pub trait Phase: sealed::Sealed {}

macro_rules! phase {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl Phase for $name {}
    };
}

phase!(
    /// The round's declaration phase: spells and melee movement.
    Declaring
);
phase!(
    /// The acting group's morale checks.
    MoraleStage
);
phase!(
    /// The acting group's movement.
    MovementStage
);
phase!(
    /// The acting group's missile attacks.
    MissileStage
);
phase!(
    /// The acting group's spell casting and special actions.
    MagicStage
);
phase!(
    /// The acting group's melee attacks.
    MeleeStage
);
phase!(
    /// The combat is over.
    Concluded
);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// An error building a combat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetupError {
    /// A combat needs at least two sides with at least one combatant each.
    NeedTwoSides,
    /// A side or combatant reference did not come from this builder.
    UnknownSide,
}

/// An error in the declaration phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeclareError {
    /// No such combatant in this combat.
    UnknownCombatant,
    /// The combatant is dead.
    Dead,
    /// The combatant's side is surprised and loses this round.
    Surprised,
    /// The combatant cannot act, move, or speak as required.
    Incapable,
    /// The combatant has no such spell available.
    NoSuchSpell,
    /// The declared target shape does not fit the spell.
    BadTarget,
    /// A withdrawal or retreat needs the combatant to be engaged in melee.
    NotEngaged,
    /// No such weapon to wield.
    NoSuchWeapon,
}

/// An error establishing a legal move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveError {
    /// No such combatant in this combat.
    UnknownCombatant,
    /// It is not this combatant's turn.
    NotActing,
    /// The combatant cannot move.
    CannotMove,
    /// The combatant already moved this round.
    AlreadyMoved,
    /// A caster cannot move in the round of the spell.
    CommittedToCasting,
    /// Engaged in melee: only a declared withdrawal or retreat may move.
    EngagedInMelee,
    /// The distance exceeds the allowance.
    TooFar,
}

/// An error resolving an attack from established evidence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActError {
    /// The world changed since the evidence was established: the attacker
    /// or target is down or has already acted.
    StaleEvidence,
}

/// An error checking morale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoraleError {
    /// The side owes no check right now.
    NotDue,
}

/// An error using a special action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionError {
    /// It is not this combatant's turn.
    NotActing,
    /// The combatant cannot act.
    CannotAct,
    /// The combatant already attacked or acted this round.
    AlreadyActed,
    /// The action committed to casting or retreating this round.
    CommittedElsewhere,
    /// A target of the action is not in this combat.
    UnknownTarget,
    /// The action's own preconditions failed.
    Invalid,
}

/// An error casting a declared spell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CastError {
    /// No such combatant in this combat.
    UnknownCombatant,
    /// It is not this combatant's turn.
    NotActing,
    /// No spell was declared this round.
    NotDeclared,
    /// The declared spell was disrupted and lost.
    Disrupted,
    /// The declared spell was already cast this round.
    AlreadyCast,
    /// The caster cannot speak and gesture freely. The spell stays in
    /// memory.
    CannotCast,
    /// The declared target shape does not fit the spell.
    BadTarget,
    /// A declared target is not in this combat.
    UnknownTarget,
    /// A declared target is dead.
    TargetDown,
    /// A declared target is out of the spell's range.
    OutOfRange,
    /// A declared target is off the field.
    NotOnField,
    /// The caster cannot see a declared target.
    NoLineOfSight,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

struct Entry {
    side: SideId,
    combatant: Combatant,
}

struct ActingGroup {
    /// The acting side, or `None` for the trailing slow-weapon group.
    side: Option<SideId>,
    members: Vec<CombatantId>,
    /// The trailing slow-weapon group may only attack.
    attacks_only: bool,
    /// Groups sharing a cluster act simultaneously: their damage lands
    /// together when the cluster finishes.
    cluster: usize,
}

struct MoraleBook {
    side: SideId,
    initial: usize,
    first_casualty_done: bool,
    half_done: bool,
    owed: u8,
    broken: bool,
}

struct Core {
    rules: Rules,
    entries: Vec<Entry>,
    side_count: u8,
    surprised: Vec<SideId>,
    events: EventLog,
    round: RoundNumber,
    order: Vec<ActingGroup>,
    current: usize,
    books: Vec<MoraleBook>,
    pending: Vec<(CombatantId, u16)>,
}

impl Core {
    fn entry(&self, id: CombatantId) -> Option<&Entry> {
        self.entries.get(id.0 as usize)
    }

    fn combatant(&self, id: CombatantId) -> Option<&Combatant> {
        self.entry(id).map(|e| &e.combatant)
    }

    fn combatant_mut(&mut self, id: CombatantId) -> Option<&mut Combatant> {
        self.entries
            .get_mut(id.0 as usize)
            .map(|e| &mut e.combatant)
    }

    fn ids(&self) -> impl Iterator<Item = CombatantId> + '_ {
        (0..self.entries.len() as u32).map(CombatantId)
    }

    fn group(&self) -> Option<&ActingGroup> {
        self.order.get(self.current)
    }

    fn view<'a>(&'a mut self, id: CombatantId) -> TargetView<'a> {
        let tied = self
            .order
            .get(self.current)
            .map(|g| g.cluster)
            .is_some_and(|c| self.order.iter().filter(|g| g.cluster == c).count() > 1);
        let entry = self.entries.get_mut(id.0 as usize).expect("validated id");
        TargetView {
            id,
            target: &mut entry.combatant,
            events: &mut self.events,
            deferred: if tied { Some(&mut self.pending) } else { None },
        }
    }

    /// Land the banked simultaneous damage.
    fn drain_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let cursor = self.events.cursor();
        let pending = core::mem::take(&mut self.pending);
        for (id, amount) in pending {
            let entry = self.entries.get_mut(id.0 as usize).expect("validated id");
            let mut view = TargetView {
                id,
                target: &mut entry.combatant,
                events: &mut self.events,
                deferred: None,
            };
            view.damage(amount);
        }
        self.absorb_consequences(cursor);
    }

    /// A side stands while at least one member lives and is not fleeing.
    fn standing_sides(&self) -> Vec<SideId> {
        let mut standing = Vec::new();
        for side in (0..self.side_count).map(SideId) {
            let fights = self.entries.iter().any(|e| {
                e.side == side
                    && e.combatant.is_alive()
                    && !e.combatant.has_condition(Condition::Fleeing)
            });
            if fights {
                standing.push(side);
            }
        }
        standing
    }

    fn is_over(&self) -> bool {
        self.standing_sides().len() <= 1
    }

    /// Whether this combatant's attacks wait for the trailing slow group.
    fn acts_last(&self, id: CombatantId) -> bool {
        if !matches!(self.rules.slow_weapons, SlowWeaponRule::ActLast) {
            return false;
        }
        match self.combatant(id) {
            Some(c) => match &c.arsenal {
                Arsenal::Weapons(_) => c.wielded().is_some_and(|w| w.modes.iter().any(|m| m.slow)),
                Arsenal::Routine(_) => false,
            },
            None => false,
        }
    }

    /// Scan the events appended since `from` for consequences the rules
    /// attach to facts: spell disruption and morale triggers.
    fn absorb_consequences(&mut self, from: EventCursor) {
        let mut disrupted: Vec<CombatantId> = Vec::new();
        let mut died: Vec<CombatantId> = Vec::new();
        let (events, _) = self.events.since(from);
        for e in events {
            match e {
                CombatEvent::AttackRolled {
                    target, hit: true, ..
                } => disrupted.push(*target),
                CombatEvent::SaveRolled {
                    who,
                    outcome: crate::save::SaveOutcome::Failure,
                    ..
                } => {
                    disrupted.push(*who);
                }
                CombatEvent::Died { who } => died.push(*who),
                _ => {}
            }
        }
        for id in disrupted {
            self.disrupt(id);
        }
        for id in died {
            let side = match self.entry(id) {
                Some(e) => e.side,
                None => continue,
            };
            self.mark_morale_triggers(side);
        }
    }

    /// A hit or failed save disrupts a pending declared spell: the spell is
    /// lost as if cast.
    fn disrupt(&mut self, id: CombatantId) {
        let Some(c) = self.combatant_mut(id) else {
            return;
        };
        if let RoundCommitment::SpellCommitted { spell, state, .. } = &mut c.commitment
            && *state == SpellState::Pending
        {
            *state = SpellState::Disrupted;
            let spell = *spell;
            if let Some(memorized) = &mut c.spells {
                memorized.expend(spell);
            }
            self.events
                .push(CombatEvent::SpellDisrupted { caster: id, spell });
        }
    }

    fn mark_morale_triggers(&mut self, side: SideId) {
        if !matches!(self.rules.morale, MoraleRule::Checked) {
            return;
        }
        let living = self
            .entries
            .iter()
            .filter(|e| e.side == side && e.combatant.is_alive())
            .count();
        let Some(book) = self.books.iter_mut().find(|b| b.side == side) else {
            return;
        };
        if book.broken {
            return;
        }
        if !book.first_casualty_done {
            book.first_casualty_done = true;
            book.owed += 1;
        }
        if !book.half_done && living * 2 <= book.initial {
            book.half_done = true;
            book.owed += 1;
        }
    }

    /// The lowest morale score among a side's living scored members.
    /// `None` when nobody on the side checks morale.
    fn side_morale(&self, side: SideId) -> Option<crate::morale::Morale> {
        self.entries
            .iter()
            .filter(|e| e.side == side && e.combatant.is_alive())
            .filter_map(|e| e.combatant.morale)
            .min()
    }

    fn side_owes_check(&self, side: SideId) -> bool {
        if !matches!(self.rules.morale, MoraleRule::Checked) {
            return false;
        }
        let Some(book) = self.books.iter().find(|b| b.side == side) else {
            return false;
        };
        if book.broken || book.owed == 0 {
            return false;
        }
        match self.side_morale(side) {
            Some(score) => !score.fearless(),
            None => false,
        }
    }

    fn active_sides(&self) -> Vec<SideId> {
        (0..self.side_count)
            .map(SideId)
            .filter(|side| {
                let alive = self
                    .entries
                    .iter()
                    .any(|e| e.side == *side && e.combatant.is_alive());
                let surprised = self.round == RoundNumber::FIRST && self.surprised.contains(side);
                alive && !surprised
            })
            .collect()
    }

    fn build_turn_order(&mut self, r: &mut dyn DiceRoller) {
        let sides = self.active_sides();
        let mut groups: Vec<ActingGroup> = Vec::new();
        match self.rules.initiative {
            InitiativeRule::PerSide { ties } => {
                let mut rolls: Vec<(SideId, u8)> = Vec::new();
                for attempt in 0..100 {
                    rolls.clear();
                    for &side in &sides {
                        let roll = r.roll(Die::D6);
                        self.events.push(CombatEvent::SideInitiative { side, roll });
                        rolls.push((side, roll));
                    }
                    let mut sorted: Vec<u8> = rolls.iter().map(|(_, roll)| *roll).collect();
                    sorted.sort_unstable();
                    sorted.dedup();
                    let distinct = sorted.len() == rolls.len();
                    let reroll = matches!(ties, TieRule::Reroll) && !distinct && attempt < 99;
                    if !reroll {
                        break;
                    }
                }
                // Highest roll acts first. Ties surviving here (the
                // simultaneous rule, or the reroll safety cap) keep a
                // stable side order.
                rolls.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(&b.0.0)));
                // Under the simultaneous rule, sides with equal rolls share
                // a cluster; otherwise every group is its own cluster.
                let mut clusters: Vec<usize> = Vec::new();
                for (i, (_, roll)) in rolls.iter().enumerate() {
                    let same_as_previous =
                        i > 0 && matches!(ties, TieRule::Simultaneous) && rolls[i - 1].1 == *roll;
                    let cluster = if same_as_previous { clusters[i - 1] } else { i };
                    clusters.push(cluster);
                }
                for (i, (side, _)) in rolls.into_iter().enumerate() {
                    let members: Vec<CombatantId> = self
                        .ids()
                        .filter(|id| {
                            let e = self.entry(*id).expect("id in range");
                            e.side == side && e.combatant.is_alive()
                        })
                        .collect();
                    groups.push(ActingGroup {
                        side: Some(side),
                        members,
                        attacks_only: false,
                        cluster: clusters[i],
                    });
                }
            }
            InitiativeRule::Individual => {
                let mut totals: Vec<(CombatantId, SideId, i8)> = Vec::new();
                for id in self.ids().collect::<Vec<_>>() {
                    let e = self.entry(id).expect("id in range");
                    if !e.combatant.is_alive() || !sides.contains(&e.side) {
                        continue;
                    }
                    let side = e.side;
                    let total = (r.roll(Die::D6) as i8)
                        .saturating_add(e.combatant.initiative_modifier.value());
                    self.events
                        .push(CombatEvent::IndividualInitiative { who: id, total });
                    totals.push((id, side, total));
                }
                totals.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.0.cmp(&b.1.0)).then(a.0.cmp(&b.0)));
                // Consecutive members of the same side with the same total
                // act as one group; everyone else acts alone, in order.
                let mut last: Option<(SideId, i8)> = None;
                for (id, side, total) in totals {
                    if last == Some((side, total)) {
                        groups.last_mut().expect("a group exists").members.push(id);
                    } else {
                        let cluster = groups.len();
                        groups.push(ActingGroup {
                            side: Some(side),
                            members: alloc::vec![id],
                            attacks_only: false,
                            cluster,
                        });
                        last = Some((side, total));
                    }
                }
            }
        }
        if matches!(self.rules.slow_weapons, SlowWeaponRule::ActLast) {
            let slow: Vec<CombatantId> = self
                .ids()
                .filter(|id| {
                    self.acts_last(*id)
                        && self
                            .entry(*id)
                            .is_some_and(|e| e.combatant.is_alive() && sides.contains(&e.side))
                })
                .collect();
            if !slow.is_empty() {
                let cluster = groups.len();
                groups.push(ActingGroup {
                    side: None,
                    members: slow,
                    attacks_only: true,
                    cluster,
                });
            }
        }
        self.events.push(CombatEvent::TurnOrderSet {
            order: groups.iter().filter_map(|g| g.side).collect(),
        });
        self.order = groups;
        self.current = 0;
        self.skip_finished_groups();
    }

    fn skip_finished_groups(&mut self) {
        while let Some(group) = self.order.get(self.current) {
            let anyone = group
                .members
                .iter()
                .any(|id| self.combatant(*id).is_some_and(|c| c.is_alive()));
            if anyone {
                break;
            }
            self.current += 1;
        }
    }

    // -- attack resolution ---------------------------------------------------

    /// Resolve one attack from `attacker` on `target` and apply damage and
    /// riders. The caller has validated everything.
    #[allow(clippy::too_many_arguments)]
    fn resolve_one_attack(
        &mut self,
        attacker: CombatantId,
        target: CombatantId,
        kind: AttackKind,
        situational: Modifier,
        damage_dice: DiceExpr,
        damage_bonus: Modifier,
        magical: bool,
        rider: Option<&Effect>,
        r: &mut dyn DiceRoller,
    ) {
        let (bonus, defence, hook_bonus, damage_adjustment) = {
            let a = self.combatant(attacker).expect("validated attacker");
            let t = self.combatant(target).expect("validated target");
            let target_side = self.entry(target).expect("validated target").side;
            let ctx = AttackContext {
                attacker: a,
                target: t,
                kind,
                target_surprised: self.round == RoundNumber::FIRST
                    && self.surprised.contains(&target_side),
            };
            let mut hook_bonus = Modifier::ZERO;
            let mut damage_adjustment = DamageAdjustment::Normal;
            for hook in &a.hooks {
                let AttackAdjustment { bonus, damage } = hook.modify_attack(&ctx);
                hook_bonus = hook_bonus.plus(bonus);
                damage_adjustment = match (damage_adjustment, damage) {
                    (_, DamageAdjustment::Replaced(d)) | (DamageAdjustment::Replaced(d), _) => {
                        DamageAdjustment::Replaced(d)
                    }
                    (_, DamageAdjustment::Doubled) | (DamageAdjustment::Doubled, _) => {
                        DamageAdjustment::Doubled
                    }
                    _ => DamageAdjustment::Normal,
                };
            }
            let mut defence_bonus = Modifier::ZERO;
            for hook in &t.hooks {
                defence_bonus = defence_bonus.plus(hook.modify_defence(&ctx));
            }
            (
                a.attack_bonus,
                t.defence_class().adjusted(defence_bonus),
                hook_bonus,
                damage_adjustment,
            )
        };
        let outcome = resolve_attack(r, bonus, situational.plus(hook_bonus), defence);
        let (natural, total, hit) = match outcome {
            AttackOutcome::Hit { natural, total } => (natural, total, true),
            AttackOutcome::Miss { natural, total } => (natural, total, false),
        };
        self.events.push(CombatEvent::AttackRolled {
            attacker,
            target,
            natural,
            total,
            hit,
        });
        if !hit {
            return;
        }
        let immune = {
            let t = self.combatant(target).expect("validated target");
            t.immunities.non_magical_weapons && !magical
        };
        if immune {
            return;
        }
        let dice = match damage_adjustment {
            DamageAdjustment::Replaced(replacement) => replacement,
            _ => damage_dice,
        };
        let rolled = dice.roll(r) + i16::from(damage_bonus.value());
        // A hit always deals at least one point.
        let mut amount = rolled.max(1) as u16;
        if matches!(damage_adjustment, DamageAdjustment::Doubled) {
            amount *= 2;
        }
        let mut view = self.view(target);
        view.damage(amount);
        if let Some(effect) = rider
            && self.combatant(target).is_some_and(|c| c.is_alive())
        {
            let mut view = self.view(target);
            apply_effect(effect, &mut view, r);
        }
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Assembles the sides and combatants of a combat.
pub struct CombatBuilder {
    rules: Rules,
    entries: Vec<Entry>,
    side_count: u8,
    surprised: Vec<SideId>,
}

impl CombatBuilder {
    /// Start building a combat under a rules configuration.
    pub fn new(rules: Rules) -> Self {
        CombatBuilder {
            rules,
            entries: Vec::new(),
            side_count: 0,
            surprised: Vec::new(),
        }
    }

    /// Create a new side and return its id.
    pub fn side(&mut self) -> SideId {
        let id = SideId(self.side_count);
        self.side_count += 1;
        id
    }

    /// Add a combatant to a side and return its id.
    pub fn join(&mut self, side: SideId, combatant: Combatant) -> CombatantId {
        assert!(side.0 < self.side_count, "side id from another builder");
        let id = CombatantId(self.entries.len() as u32);
        self.entries.push(Entry { side, combatant });
        id
    }

    /// Mark a side as surprised: it cannot act in the first round.
    pub fn surprised(&mut self, side: SideId) {
        if !self.surprised.contains(&side) {
            self.surprised.push(side);
        }
    }

    /// Begin the combat. Needs at least two sides with a combatant each.
    pub fn begin(self) -> Result<Combat<Declaring>, SetupError> {
        let populated = (0..self.side_count)
            .map(SideId)
            .filter(|s| self.entries.iter().any(|e| e.side == *s))
            .count();
        if populated < 2 {
            return Err(SetupError::NeedTwoSides);
        }
        let books = (0..self.side_count)
            .map(SideId)
            .map(|side| MoraleBook {
                side,
                initial: self.entries.iter().filter(|e| e.side == side).count(),
                first_casualty_done: false,
                half_done: false,
                owed: 0,
                broken: false,
            })
            .collect();
        let mut core = Core {
            rules: self.rules,
            entries: self.entries,
            side_count: self.side_count,
            surprised: self.surprised,
            events: EventLog::new(),
            round: RoundNumber::FIRST,
            order: Vec::new(),
            current: 0,
            books,
            pending: Vec::new(),
        };
        core.events
            .push(CombatEvent::RoundStarted { round: core.round });
        Ok(Combat {
            core,
            _phase: PhantomData,
        })
    }
}

// ---------------------------------------------------------------------------
// Combat<P>
// ---------------------------------------------------------------------------

/// A combat in phase `P`. Build one with [`CombatBuilder`].
pub struct Combat<P: Phase> {
    core: Core,
    _phase: PhantomData<P>,
}

impl<P: Phase> Combat<P> {
    fn shift<Q: Phase>(self) -> Combat<Q> {
        Combat {
            core: self.core,
            _phase: PhantomData,
        }
    }

    /// The current round number.
    pub fn round(&self) -> RoundNumber {
        self.core.round
    }

    /// The rules configuration.
    pub fn rules(&self) -> &Rules {
        &self.core.rules
    }

    /// The event log so far.
    pub fn log(&self) -> &EventLog {
        &self.core.events
    }

    /// A combatant by id.
    pub fn combatant(&self, id: CombatantId) -> Option<&Combatant> {
        self.core.combatant(id)
    }

    /// The side a combatant belongs to.
    pub fn side_of(&self, id: CombatantId) -> Option<SideId> {
        self.core.entry(id).map(|e| e.side)
    }

    /// Every combatant, with its id and side.
    pub fn combatants(&self) -> impl Iterator<Item = (CombatantId, SideId, &Combatant)> {
        self.core
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (CombatantId(i as u32), e.side, &e.combatant))
    }

    /// The acting side, when a group is acting. `None` in the declaration
    /// phase, after the combat ends, and during the trailing slow-weapon
    /// group.
    pub fn acting(&self) -> Option<SideId> {
        self.core.group().and_then(|g| g.side)
    }

    /// The members of the acting group, in order.
    pub fn acting_members(&self) -> &[CombatantId] {
        self.core
            .group()
            .map(|g| g.members.as_slice())
            .unwrap_or(&[])
    }

    /// The sides still standing: at least one living member not fleeing.
    pub fn standing(&self) -> Vec<SideId> {
        self.core.standing_sides()
    }
}

// ---------------------------------------------------------------------------
// Declaring
// ---------------------------------------------------------------------------

impl Combat<Declaring> {
    fn declarable(&self, who: CombatantId) -> Result<(), DeclareError> {
        let entry = self.core.entry(who).ok_or(DeclareError::UnknownCombatant)?;
        if !entry.combatant.is_alive() {
            return Err(DeclareError::Dead);
        }
        if self.core.round == RoundNumber::FIRST && self.core.surprised.contains(&entry.side) {
            return Err(DeclareError::Surprised);
        }
        Ok(())
    }

    /// Declare a spell for this round. Casting is the caster's sole action:
    /// no movement, no attack.
    pub fn declare_spell(
        &mut self,
        caster: CombatantId,
        spell: SpellRef,
        target: DeclaredTarget,
    ) -> Result<EventCursor, DeclareError> {
        self.declarable(caster)?;
        let c = self.core.combatant(caster).expect("validated id");
        if !c.can_cast() {
            return Err(DeclareError::Incapable);
        }
        let memorized = c.spells().ok_or(DeclareError::NoSuchSpell)?;
        if !memorized.is_available(spell) {
            return Err(DeclareError::NoSuchSpell);
        }
        let targeting = memorized
            .spell(spell)
            .expect("available spell exists")
            .targeting;
        let shape_fits = match (&target, targeting) {
            (DeclaredTarget::SelfCast, SpellTargeting::Caster) => true,
            (DeclaredTarget::One(_), SpellTargeting::Single | SpellTargeting::Many) => true,
            (DeclaredTarget::Many(ids), SpellTargeting::Many) => !ids.is_empty(),
            _ => false,
        };
        if !shape_fits {
            return Err(DeclareError::BadTarget);
        }
        let cursor = self.core.events.cursor();
        let c = self.core.combatant_mut(caster).expect("validated id");
        c.commitment = RoundCommitment::SpellCommitted {
            spell,
            target,
            state: SpellState::Pending,
        };
        self.core
            .events
            .push(CombatEvent::SpellDeclared { caster, spell });
        Ok(cursor)
    }

    /// Declare a fighting withdrawal: half the encounter rate, still
    /// fighting. Only for a combatant engaged in melee.
    pub fn declare_withdrawal(
        &mut self,
        who: CombatantId,
        oracle: &dyn SpatialOracle,
    ) -> Result<EventCursor, DeclareError> {
        self.declare_melee_movement(who, oracle, MoveKind::Withdrawal)
    }

    /// Declare a full retreat: up to the encounter rate, no attack this
    /// round, +2 to be hit, shield ignored. Only for a combatant engaged in
    /// melee.
    pub fn declare_retreat(
        &mut self,
        who: CombatantId,
        oracle: &dyn SpatialOracle,
    ) -> Result<EventCursor, DeclareError> {
        self.declare_melee_movement(who, oracle, MoveKind::Retreat)
    }

    fn declare_melee_movement(
        &mut self,
        who: CombatantId,
        oracle: &dyn SpatialOracle,
        kind: MoveKind,
    ) -> Result<EventCursor, DeclareError> {
        self.declarable(who)?;
        let c = self.core.combatant(who).expect("validated id");
        if !c.can_move() {
            return Err(DeclareError::Incapable);
        }
        if !oracle.engaged(who) {
            return Err(DeclareError::NotEngaged);
        }
        let cursor = self.core.events.cursor();
        let c = self.core.combatant_mut(who).expect("validated id");
        c.commitment = match kind {
            MoveKind::Withdrawal => RoundCommitment::FightingWithdrawal,
            _ => RoundCommitment::Retreat,
        };
        self.core
            .events
            .push(CombatEvent::MovementDeclared { who, kind });
        Ok(cursor)
    }

    /// Switch the wielded weapon. Weapons change between rounds, not
    /// mid-round.
    pub fn wield(&mut self, who: CombatantId, weapon_index: usize) -> Result<(), DeclareError> {
        self.declarable(who)?;
        let c = self.core.combatant_mut(who).expect("validated id");
        if c.set_wielded(weapon_index) {
            Ok(())
        } else {
            Err(DeclareError::NoSuchWeapon)
        }
    }

    /// Roll initiative per the rules, fix the turn order, and hand the
    /// first acting group its morale stage.
    pub fn roll_initiative(mut self, r: &mut impl DiceRoller) -> Combat<MoraleStage> {
        self.core.build_turn_order(r);
        self.shift()
    }
}

// ---------------------------------------------------------------------------
// MoraleStage
// ---------------------------------------------------------------------------

impl Combat<MoraleStage> {
    /// The sides that owe a morale check right now: sides with a member in
    /// the acting group and an unresolved trigger.
    pub fn due(&self) -> Vec<SideId> {
        let Some(group) = self.core.group() else {
            return Vec::new();
        };
        let mut sides: Vec<SideId> = Vec::new();
        for id in &group.members {
            if let Some(e) = self.core.entry(*id)
                && !sides.contains(&e.side)
                && self.core.side_owes_check(e.side)
            {
                sides.push(e.side);
            }
        }
        sides
    }

    /// Resolve one due morale check: 2d6 against the side's lowest morale
    /// score. A score of 2 breaks without a roll. On a break, the side's
    /// scored members flee.
    pub fn check(
        &mut self,
        side: SideId,
        r: &mut impl DiceRoller,
    ) -> Result<MoraleOutcome, MoraleError> {
        if !self.due().contains(&side) {
            return Err(MoraleError::NotDue);
        }
        let score = self.core.side_morale(side).expect("a due side has a score");
        let (roll, outcome) = if score.never_fights() {
            (0, MoraleOutcome::Breaks)
        } else {
            let roll = r.roll(Die::D6) + r.roll(Die::D6);
            let outcome = if roll <= score.get() {
                MoraleOutcome::StandsFirm
            } else {
                MoraleOutcome::Breaks
            };
            (roll, outcome)
        };
        self.core.events.push(CombatEvent::MoraleChecked {
            side,
            roll,
            outcome,
        });
        let book = self
            .core
            .books
            .iter_mut()
            .find(|b| b.side == side)
            .expect("side has a book");
        match outcome {
            MoraleOutcome::StandsFirm => {
                book.owed = book.owed.saturating_sub(1);
            }
            MoraleOutcome::Breaks => {
                book.owed = 0;
                book.broken = true;
                self.core.events.push(CombatEvent::SideBroke { side });
                let fleeing: Vec<CombatantId> = self
                    .core
                    .ids()
                    .filter(|id| {
                        self.core.entry(*id).is_some_and(|e| {
                            e.side == side && e.combatant.is_alive() && e.combatant.morale.is_some()
                        })
                    })
                    .collect();
                for id in fleeing {
                    let mut view = self.core.view(id);
                    view.apply_condition(Condition::Fleeing, Duration::UntilCombatEnds);
                }
            }
        }
        Ok(outcome)
    }

    /// Move on to movement. Unresolved due checks stay due for this side's
    /// next morale stage.
    pub fn finish_morale(self) -> Combat<MovementStage> {
        self.shift()
    }
}

// ---------------------------------------------------------------------------
// MovementStage
// ---------------------------------------------------------------------------

impl Combat<MovementStage> {
    /// Establish a legal move for an acting combatant: within the
    /// allowance, consistent with engagement and this round's declaration.
    pub fn witness_move(
        &self,
        who: CombatantId,
        distance: Feet,
        oracle: &dyn SpatialOracle,
    ) -> Result<LegalMove, MoveError> {
        let c = self
            .core
            .combatant(who)
            .ok_or(MoveError::UnknownCombatant)?;
        let group = self.core.group().ok_or(MoveError::NotActing)?;
        if group.attacks_only || !group.members.contains(&who) {
            return Err(MoveError::NotActing);
        }
        if !c.can_move() {
            return Err(MoveError::CannotMove);
        }
        if c.moved_this_round {
            return Err(MoveError::AlreadyMoved);
        }
        let allowance = c.encounter_rate();
        let (kind, max) = match &c.commitment {
            RoundCommitment::SpellCommitted { .. } => return Err(MoveError::CommittedToCasting),
            RoundCommitment::FightingWithdrawal => (MoveKind::Withdrawal, Feet(allowance.0 / 2)),
            RoundCommitment::Retreat => (MoveKind::Retreat, allowance),
            RoundCommitment::Uncommitted => {
                if oracle.engaged(who) {
                    return Err(MoveError::EngagedInMelee);
                }
                (MoveKind::Normal, allowance)
            }
        };
        if distance > max {
            return Err(MoveError::TooFar);
        }
        Ok(LegalMove {
            who,
            kind,
            distance,
        })
    }

    /// Record an established move. The application moves the miniature; the
    /// kernel records the fact.
    pub fn make_move(&mut self, mv: LegalMove) -> EventCursor {
        let cursor = self.core.events.cursor();
        if let Some(c) = self.core.combatant_mut(mv.who) {
            c.moved_this_round = true;
        }
        self.core.events.push(CombatEvent::Moved {
            who: mv.who,
            kind: mv.kind,
            distance: mv.distance,
        });
        cursor
    }

    /// Move on to missile attacks.
    pub fn finish_movement(self) -> Combat<MissileStage> {
        self.shift()
    }
}

// ---------------------------------------------------------------------------
// Attack witnessing shared by the missile and melee stages
// ---------------------------------------------------------------------------

impl Core {
    fn attacker_ready(&self, attacker: CombatantId) -> Result<(), TargetingError> {
        let c = self
            .combatant(attacker)
            .ok_or(TargetingError::UnknownCombatant)?;
        let group = self.group().ok_or(TargetingError::NotActing)?;
        if !group.members.contains(&attacker) {
            return Err(TargetingError::NotActing);
        }
        if !group.attacks_only && self.acts_last(attacker) {
            return Err(TargetingError::ActsLast);
        }
        if !c.can_attack() {
            return Err(TargetingError::CannotAttack);
        }
        if c.attacked_this_round {
            return Err(TargetingError::AlreadyAttacked);
        }
        if matches!(
            c.commitment,
            RoundCommitment::SpellCommitted { .. } | RoundCommitment::Retreat
        ) {
            return Err(TargetingError::CommittedElsewhere);
        }
        Ok(())
    }

    fn target_up(&self, target: CombatantId) -> Result<(), TargetingError> {
        let t = self
            .combatant(target)
            .ok_or(TargetingError::UnknownCombatant)?;
        if !t.is_alive() {
            return Err(TargetingError::TargetDown);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MissileStage
// ---------------------------------------------------------------------------

impl Combat<MissileStage> {
    /// Establish a legal missile shot: armed, unengaged, in range, in
    /// sight, and at most partial cover.
    pub fn witness_shot(
        &self,
        attacker: CombatantId,
        target: CombatantId,
        oracle: &dyn SpatialOracle,
    ) -> Result<MissileTarget, TargetingError> {
        self.core.attacker_ready(attacker)?;
        self.core.target_up(target)?;
        let c = self.core.combatant(attacker).expect("validated attacker");
        let (ranges, reload) = match &c.arsenal {
            Arsenal::Weapons(_) => {
                let mode = c
                    .wielded()
                    .and_then(|w| w.missile_mode().copied())
                    .ok_or(TargetingError::NoSuchMode)?;
                let AttackReach::Missile(ranges) = mode.reach else {
                    return Err(TargetingError::NoSuchMode);
                };
                (ranges, mode.reload)
            }
            Arsenal::Routine(routine) => {
                let first = routine
                    .iter()
                    .find_map(|a| match a.reach {
                        AttackReach::Missile(ranges) => Some(ranges),
                        AttackReach::Melee => None,
                    })
                    .ok_or(TargetingError::NoSuchMode)?;
                (first, false)
            }
        };
        if matches!(self.core.rules.reload, ReloadRule::EveryOtherRound)
            && reload
            && c.fired_last_round
        {
            return Err(TargetingError::NeedsReload);
        }
        if oracle.engaged(attacker) {
            return Err(TargetingError::AttackerEngaged);
        }
        let distance = oracle
            .distance(attacker, target)
            .ok_or(TargetingError::NotOnField)?;
        if distance <= MELEE_REACH {
            return Err(TargetingError::TooCloseForMissiles);
        }
        let band = ranges.band(distance).ok_or(TargetingError::OutOfRange)?;
        if !oracle.line_of_sight(attacker, target) {
            return Err(TargetingError::NoLineOfSight);
        }
        let cover = oracle.cover(attacker, target);
        if matches!(cover, Cover::Total) {
            return Err(TargetingError::TotalCover);
        }
        Ok(MissileTarget {
            attacker,
            target,
            band,
            cover,
        })
    }

    /// Fire an established shot. Rolls the attack, applies damage and any
    /// riders, and logs everything.
    pub fn shoot(
        &mut self,
        shot: MissileTarget,
        r: &mut impl DiceRoller,
    ) -> Result<EventCursor, ActError> {
        if self.core.attacker_ready(shot.attacker).is_err()
            || self.core.target_up(shot.target).is_err()
        {
            return Err(ActError::StaleEvidence);
        }
        let cursor = self.core.events.cursor();
        let situational_base = shot
            .band
            .attack_modifier()
            .plus(match shot.cover {
                Cover::Partial(p) => p.attack_modifier(),
                _ => Modifier::ZERO,
            })
            .plus(self.retreat_bonus(shot.target));

        // Gather the attacks first so the roster borrow ends.
        struct Volley {
            damage: DiceExpr,
            bonus: Modifier,
            magical: bool,
            rider: Option<Effect>,
            reload: bool,
        }
        let mut volley: Vec<Volley> = Vec::new();
        let mut situational = situational_base;
        {
            let c = self
                .core
                .combatant(shot.attacker)
                .expect("validated attacker");
            situational = situational
                .plus(c.missile_modifier)
                .plus(c.temp.attack_sum());
            match &c.arsenal {
                Arsenal::Weapons(_) => {
                    let weapon = c.wielded().expect("witnessed missile mode");
                    let mode = weapon.missile_mode().expect("witnessed missile mode");
                    situational = situational.plus(weapon.bonus);
                    volley.push(Volley {
                        damage: self.weapon_damage(mode.damage),
                        bonus: weapon.bonus,
                        magical: weapon.is_magical(),
                        rider: None,
                        reload: mode.reload,
                    });
                }
                Arsenal::Routine(routine) => {
                    for attack in routine.iter() {
                        if matches!(attack.reach, AttackReach::Missile(_)) {
                            volley.push(Volley {
                                damage: self.weapon_damage(attack.damage),
                                bonus: Modifier::ZERO,
                                magical: false,
                                rider: attack.on_hit.clone(),
                                reload: false,
                            });
                        }
                    }
                }
            }
        }
        if let Some(c) = self.core.combatant_mut(shot.attacker) {
            c.attacked_this_round = true;
            if volley.iter().any(|v| v.reload) {
                c.fired_this_round = true;
            }
        }
        for v in &volley {
            if self
                .core
                .combatant(shot.target)
                .is_some_and(|t| t.is_alive())
            {
                self.core.resolve_one_attack(
                    shot.attacker,
                    shot.target,
                    AttackKind::Missile,
                    situational,
                    v.damage,
                    v.bonus,
                    v.magical,
                    v.rider.as_ref(),
                    r,
                );
            }
        }
        self.core.absorb_consequences(cursor);
        Ok(cursor)
    }

    /// Move on to spell casting.
    pub fn finish_missiles(self) -> Combat<MagicStage> {
        self.shift()
    }
}

impl<P: Phase> Combat<P> {
    /// +2 to hit a combatant that declared a full retreat.
    fn retreat_bonus(&self, target: CombatantId) -> Modifier {
        match self.core.combatant(target).map(|c| &c.commitment) {
            Some(RoundCommitment::Retreat) => Modifier::new(2),
            _ => Modifier::ZERO,
        }
    }

    /// The damage dice under the damage rule: flat d6, or the weapon's own.
    fn weapon_damage(&self, by_weapon: DiceExpr) -> DiceExpr {
        match self.core.rules.damage {
            crate::rules::DamageRule::FlatD6 => DiceExpr::of(1, Die::D6),
            crate::rules::DamageRule::ByWeapon => by_weapon,
        }
    }
}

// ---------------------------------------------------------------------------
// MagicStage
// ---------------------------------------------------------------------------

impl Combat<MagicStage> {
    /// Cast the spell declared this round. Checks freedom, targets, range,
    /// and sight; spends the spell; applies its effect to each target.
    pub fn cast(
        &mut self,
        caster: CombatantId,
        oracle: &dyn SpatialOracle,
        r: &mut impl DiceRoller,
    ) -> Result<EventCursor, CastError> {
        let c = self
            .core
            .combatant(caster)
            .ok_or(CastError::UnknownCombatant)?;
        let group = self.core.group().ok_or(CastError::NotActing)?;
        if group.attacks_only || !group.members.contains(&caster) {
            return Err(CastError::NotActing);
        }
        let (spell_ref, declared) = match &c.commitment {
            RoundCommitment::SpellCommitted {
                spell,
                target,
                state,
            } => match state {
                SpellState::Pending => (*spell, target.clone()),
                SpellState::Disrupted => return Err(CastError::Disrupted),
                SpellState::Cast => return Err(CastError::AlreadyCast),
            },
            _ => return Err(CastError::NotDeclared),
        };
        if !c.can_cast() {
            return Err(CastError::CannotCast);
        }
        let spell = c
            .spells()
            .and_then(|m| m.spell(spell_ref))
            .ok_or(CastError::NotDeclared)?
            .clone();
        let targets: Vec<CombatantId> = match &declared {
            DeclaredTarget::SelfCast => alloc::vec![caster],
            DeclaredTarget::One(id) => alloc::vec![*id],
            DeclaredTarget::Many(ids) => ids.clone(),
        };
        if targets.is_empty() {
            return Err(CastError::BadTarget);
        }
        for &t in &targets {
            let target = self.core.combatant(t).ok_or(CastError::UnknownTarget)?;
            if !target.is_alive() {
                return Err(CastError::TargetDown);
            }
            if t == caster {
                continue;
            }
            match spell.range {
                SpellRange::Caster => return Err(CastError::OutOfRange),
                SpellRange::Feet(max) => {
                    let d = oracle.distance(caster, t).ok_or(CastError::NotOnField)?;
                    if d > max {
                        return Err(CastError::OutOfRange);
                    }
                }
            }
            if spell.needs_sight && !oracle.line_of_sight(caster, t) {
                return Err(CastError::NoLineOfSight);
            }
        }

        let cursor = self.core.events.cursor();
        let c = self.core.combatant_mut(caster).expect("validated caster");
        if let Some(memorized) = &mut c.spells {
            memorized.expend(spell_ref);
        }
        if let RoundCommitment::SpellCommitted { state, .. } = &mut c.commitment {
            *state = SpellState::Cast;
        }
        self.core.events.push(CombatEvent::SpellCast {
            caster,
            spell: spell_ref,
        });
        for t in targets {
            let mut view = self.core.view(t);
            apply_effect(&spell.effect, &mut view, r);
        }
        self.core.absorb_consequences(cursor);
        Ok(cursor)
    }

    /// Use a special class action, such as the cleric's turn undead. The
    /// action takes the place of the combatant's attack this round.
    pub fn special(
        &mut self,
        who: CombatantId,
        action: &dyn SpecialAction,
        r: &mut impl DiceRoller,
    ) -> Result<EventCursor, ActionError> {
        let c = self.core.combatant(who).ok_or(ActionError::NotActing)?;
        let group = self.core.group().ok_or(ActionError::NotActing)?;
        if group.attacks_only || !group.members.contains(&who) {
            return Err(ActionError::NotActing);
        }
        if !c.can_act() {
            return Err(ActionError::CannotAct);
        }
        if c.attacked_this_round {
            return Err(ActionError::AlreadyActed);
        }
        if matches!(
            c.commitment,
            RoundCommitment::SpellCommitted { .. } | RoundCommitment::Retreat
        ) {
            return Err(ActionError::CommittedElsewhere);
        }
        let cursor = self.core.events.cursor();
        let effects = action.resolve(
            &ActionContext {
                actor: who,
                core: &self.core,
            },
            r,
        )?;
        for (id, _) in &effects {
            if self.core.combatant(*id).is_none() {
                return Err(ActionError::UnknownTarget);
            }
        }
        self.core
            .combatant_mut(who)
            .expect("validated actor")
            .attacked_this_round = true;
        self.core.events.push(CombatEvent::Note {
            who: Some(who),
            text: alloc::string::String::from(action.name()),
        });
        for (id, effect) in effects {
            if self.core.combatant(id).is_some_and(|c| c.is_alive()) {
                let mut view = self.core.view(id);
                apply_effect(&effect, &mut view, r);
            }
        }
        self.core.absorb_consequences(cursor);
        Ok(cursor)
    }

    /// Move on to melee.
    pub fn finish_magic(self) -> Combat<MeleeStage> {
        self.shift()
    }
}

/// A read-only view of the combat for a [`SpecialAction`].
pub struct ActionContext<'a> {
    actor: CombatantId,
    core: &'a Core,
}

impl ActionContext<'_> {
    /// The acting combatant's id.
    pub fn actor_id(&self) -> CombatantId {
        self.actor
    }

    /// The acting combatant.
    pub fn actor(&self) -> &Combatant {
        self.core.combatant(self.actor).expect("the actor exists")
    }

    /// A combatant by id.
    pub fn combatant(&self, id: CombatantId) -> Option<&Combatant> {
        self.core.combatant(id)
    }
}

/// A class-granted action resolved in the magic stage, such as turning the
/// undead. The action validates itself against the [`ActionContext`] and
/// returns the effects to apply.
pub trait SpecialAction {
    /// The action name, for the log.
    fn name(&self) -> &str;

    /// Validate and resolve the action into effects per target.
    fn resolve(
        &self,
        ctx: &ActionContext<'_>,
        r: &mut dyn DiceRoller,
    ) -> Result<Vec<(CombatantId, Effect)>, ActionError>;
}

// ---------------------------------------------------------------------------
// MeleeStage
// ---------------------------------------------------------------------------

impl Combat<MeleeStage> {
    /// Establish a legal melee strike: armed, able, and within reach.
    pub fn witness_melee(
        &self,
        attacker: CombatantId,
        target: CombatantId,
        oracle: &dyn SpatialOracle,
    ) -> Result<MeleeTarget, TargetingError> {
        self.core.attacker_ready(attacker)?;
        self.core.target_up(target)?;
        let c = self.core.combatant(attacker).expect("validated attacker");
        let armed = match &c.arsenal {
            Arsenal::Weapons(_) => c.wielded().is_some_and(|w| w.melee_mode().is_some()),
            Arsenal::Routine(routine) => routine
                .iter()
                .any(|a| matches!(a.reach, AttackReach::Melee)),
        };
        if !armed {
            return Err(TargetingError::NoSuchMode);
        }
        let distance = oracle
            .distance(attacker, target)
            .ok_or(TargetingError::NotOnField)?;
        if distance > MELEE_REACH {
            return Err(TargetingError::OutOfMeleeReach);
        }
        Ok(MeleeTarget { attacker, target })
    }

    /// Strike an established melee target. Rolls each melee attack in the
    /// attacker's routine, applies damage and riders, and logs everything.
    pub fn strike(
        &mut self,
        target: MeleeTarget,
        r: &mut impl DiceRoller,
    ) -> Result<EventCursor, ActError> {
        if self.core.attacker_ready(target.attacker).is_err()
            || self.core.target_up(target.target).is_err()
        {
            return Err(ActError::StaleEvidence);
        }
        let cursor = self.core.events.cursor();
        struct Blow {
            damage: DiceExpr,
            bonus: Modifier,
            magical: bool,
            rider: Option<Effect>,
        }
        let mut blows: Vec<Blow> = Vec::new();
        let mut situational = self.retreat_bonus(target.target);
        {
            let c = self
                .core
                .combatant(target.attacker)
                .expect("validated attacker");
            situational = situational.plus(c.melee_modifier).plus(c.temp.attack_sum());
            match &c.arsenal {
                Arsenal::Weapons(_) => {
                    let weapon = c.wielded().expect("witnessed melee mode");
                    let mode = weapon.melee_mode().expect("witnessed melee mode");
                    situational = situational.plus(weapon.bonus);
                    // Strength modifies melee damage under both damage
                    // rules.
                    blows.push(Blow {
                        damage: self.weapon_damage(mode.damage),
                        bonus: weapon.bonus.plus(c.melee_modifier),
                        magical: weapon.is_magical(),
                        rider: None,
                    });
                }
                Arsenal::Routine(routine) => {
                    for attack in routine.iter() {
                        if matches!(attack.reach, AttackReach::Melee) {
                            blows.push(Blow {
                                damage: self.weapon_damage(attack.damage),
                                bonus: Modifier::ZERO,
                                magical: false,
                                rider: attack.on_hit.clone(),
                            });
                        }
                    }
                }
            }
        }
        if let Some(c) = self.core.combatant_mut(target.attacker) {
            c.attacked_this_round = true;
        }
        for blow in &blows {
            if self
                .core
                .combatant(target.target)
                .is_some_and(|t| t.is_alive())
            {
                self.core.resolve_one_attack(
                    target.attacker,
                    target.target,
                    AttackKind::Melee,
                    situational,
                    blow.damage,
                    blow.bonus,
                    blow.magical,
                    blow.rider.as_ref(),
                    r,
                );
            }
        }
        self.core.absorb_consequences(cursor);
        Ok(cursor)
    }

    /// End this group's turn. The return value is the round-flow control:
    /// the next group, a fresh round, or the end of the combat.
    pub fn finish_melee(mut self) -> TurnEnd {
        let leaving = self.core.group().map(|g| g.cluster);
        self.core.current += 1;
        self.core.skip_finished_groups();
        if self.core.group().map(|g| g.cluster) != leaving {
            self.core.drain_pending();
        }
        if self.core.is_over() {
            return self.conclude();
        }
        if self.core.group().is_some() {
            return TurnEnd::NextGroup(self.shift());
        }
        // The round is over: expire durations, reset commitments and
        // flags, then either conclude or open the next round.
        let round = self.core.round;
        for id in self.core.ids().collect::<Vec<_>>() {
            let entry = self
                .core
                .entries
                .get_mut(id.0 as usize)
                .expect("id in range");
            entry.combatant.tick_round(id, &mut self.core.events);
        }
        self.core.events.push(CombatEvent::RoundEnded { round });
        if self.core.is_over() {
            return self.conclude();
        }
        self.core.round = self.core.round.next();
        self.core.order = Vec::new();
        self.core.current = 0;
        self.core.events.push(CombatEvent::RoundStarted {
            round: self.core.round,
        });
        TurnEnd::NewRound(self.shift())
    }

    fn conclude(mut self) -> TurnEnd {
        let standing = self.core.standing_sides();
        self.core.events.push(CombatEvent::CombatEnded {
            standing: standing.clone(),
        });
        self.core.order = Vec::new();
        TurnEnd::Over(self.shift())
    }
}

/// Where the combat stands after a group finishes its melee.
pub enum TurnEnd {
    /// Another group still acts this round.
    NextGroup(Combat<MoraleStage>),
    /// The round ended; declare again.
    NewRound(Combat<Declaring>),
    /// The combat is over.
    Over(Combat<Concluded>),
}

// ---------------------------------------------------------------------------
// Concluded
// ---------------------------------------------------------------------------

impl Combat<Concluded> {
    /// The survivors: living combatants, with their ids and sides.
    pub fn survivors(&self) -> impl Iterator<Item = (CombatantId, SideId, &Combatant)> {
        self.combatants().filter(|(_, _, c)| c.is_alive())
    }

    /// The experience award for one side: the sum of the experience values
    /// of defeated enemies — the dead, and those who fled or broke.
    pub fn xp_award(&self, side: SideId) -> u32 {
        self.combatants()
            .filter(|(_, s, c)| {
                *s != side && (!c.is_alive() || c.has_condition(Condition::Fleeing))
            })
            .map(|(_, _, c)| c.xp_value)
            .sum()
    }
}
