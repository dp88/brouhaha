//! The boundary between the rules and the world: the spatial oracle and the
//! targeting evidence types.
//!
//! The kernel never sees a grid. It asks a [`SpatialOracle`] four questions,
//! in feet, and mints evidence types from the answers. A grid adapter (the
//! `spacewalk` feature), a theatre-of-the-mind table ([`AbstractField`]), or
//! a test stub all answer the same questions.

use alloc::vec::Vec;

use crate::units::{CombatantId, Feet};
use crate::weapon::{Cover, RangeBand};

/// Melee reach: opponents five feet apart or less are in melee.
pub const MELEE_REACH: Feet = Feet(5);

/// Facts about where everyone is. Implemented by the application.
pub trait SpatialOracle {
    /// The distance between two combatants, or `None` when either is off
    /// the field.
    fn distance(&self, a: CombatantId, b: CombatantId) -> Option<Feet>;

    /// Whether `a` can see `b`.
    fn line_of_sight(&self, a: CombatantId, b: CombatantId) -> bool;

    /// The cover `target` enjoys against `attacker`.
    fn cover(&self, attacker: CombatantId, target: CombatantId) -> Cover;

    /// Whether `who` is in melee contact with an enemy. Engagement governs
    /// free movement and missile fire.
    fn engaged(&self, who: CombatantId) -> bool;
}

/// Why a target could not be established.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetingError {
    /// No such combatant in this combat.
    UnknownCombatant,
    /// The oracle reports a combatant off the field.
    NotOnField,
    /// The attacker is dead or unable to attack.
    CannotAttack,
    /// The target is already dead.
    TargetDown,
    /// The attacker committed to casting or retreating this round.
    CommittedElsewhere,
    /// It is not this combatant's turn to act.
    NotActing,
    /// The combatant already attacked this round.
    AlreadyAttacked,
    /// The target is more than five feet away.
    OutOfMeleeReach,
    /// The target is within five feet: too close for missiles.
    TooCloseForMissiles,
    /// The target is beyond the weapon's long range.
    OutOfRange,
    /// No line of sight to the target.
    NoLineOfSight,
    /// The target is behind total cover.
    TotalCover,
    /// The attacker is engaged in melee and cannot fire.
    AttackerEngaged,
    /// The attacker has no attack of the required kind.
    NoSuchMode,
    /// The weapon needs a round to reload.
    NeedsReload,
    /// A slow weapon acts at the end of the round.
    ActsLast,
}

/// Evidence of a legal melee strike: attacker and target validated, within
/// reach, this stage. Minted by the combat machine; the fields are private
/// so a strike cannot name an arbitrary pair.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MeleeTarget {
    pub(crate) attacker: CombatantId,
    pub(crate) target: CombatantId,
}

impl MeleeTarget {
    /// The attacker.
    pub fn attacker(&self) -> CombatantId {
        self.attacker
    }

    /// The target.
    pub fn target(&self) -> CombatantId {
        self.target
    }
}

/// Evidence of a legal missile shot: in some range band (never out of
/// range), never behind total cover, attacker unengaged, target beyond
/// melee reach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MissileTarget {
    pub(crate) attacker: CombatantId,
    pub(crate) target: CombatantId,
    pub(crate) band: RangeBand,
    pub(crate) cover: Cover,
}

impl MissileTarget {
    /// The attacker.
    pub fn attacker(&self) -> CombatantId {
        self.attacker
    }

    /// The target.
    pub fn target(&self) -> CombatantId {
        self.target
    }

    /// The range band the shot is in.
    pub fn band(&self) -> RangeBand {
        self.band
    }

    /// The partial cover applied to the shot. Never total.
    pub fn cover(&self) -> Cover {
        self.cover
    }
}

/// Evidence of a legal move this round: within the mover's allowance and
/// consistent with engagement and declarations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LegalMove {
    pub(crate) who: CombatantId,
    pub(crate) kind: crate::combat::event::MoveKind,
    pub(crate) distance: Feet,
}

impl LegalMove {
    /// The mover.
    pub fn who(&self) -> CombatantId {
        self.who
    }

    /// The distance, in feet.
    pub fn distance(&self) -> Feet {
        self.distance
    }
}

/// A theatre-of-the-mind oracle: the application asserts pairwise
/// distances, engagement, sight, and cover.
///
/// Distances are symmetric. A pair with no asserted distance is off the
/// field for each other. Engagement is exactly what was asserted; the field
/// does not derive it, because it does not know who is whose enemy.
#[derive(Clone, Debug, Default)]
pub struct AbstractField {
    distances: Vec<((CombatantId, CombatantId), Feet)>,
    engaged: Vec<CombatantId>,
    blocked_sight: Vec<(CombatantId, CombatantId)>,
    cover: Vec<((CombatantId, CombatantId), Cover)>,
}

fn ordered(a: CombatantId, b: CombatantId) -> (CombatantId, CombatantId) {
    if a <= b { (a, b) } else { (b, a) }
}

impl AbstractField {
    /// An empty field.
    pub fn new() -> Self {
        AbstractField::default()
    }

    /// Assert the distance between two combatants. Symmetric.
    pub fn set_distance(&mut self, a: CombatantId, b: CombatantId, distance: Feet) {
        let key = ordered(a, b);
        if let Some(entry) = self.distances.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = distance;
        } else {
            self.distances.push((key, distance));
        }
    }

    /// Assert whether a combatant is engaged in melee.
    pub fn set_engaged(&mut self, who: CombatantId, engaged: bool) {
        if engaged {
            if !self.engaged.contains(&who) {
                self.engaged.push(who);
            }
        } else {
            self.engaged.retain(|c| *c != who);
        }
    }

    /// Block line of sight between two combatants. Symmetric.
    pub fn block_sight(&mut self, a: CombatantId, b: CombatantId) {
        let key = ordered(a, b);
        if !self.blocked_sight.contains(&key) {
            self.blocked_sight.push(key);
        }
    }

    /// Assert the cover `target` enjoys against `attacker`. Directional.
    pub fn set_cover(&mut self, attacker: CombatantId, target: CombatantId, cover: Cover) {
        let key = (attacker, target);
        if let Some(entry) = self.cover.iter_mut().find(|(k, _)| *k == key) {
            entry.1 = cover;
        } else {
            self.cover.push((key, cover));
        }
    }
}

impl SpatialOracle for AbstractField {
    fn distance(&self, a: CombatantId, b: CombatantId) -> Option<Feet> {
        if a == b {
            return Some(Feet::ZERO);
        }
        let key = ordered(a, b);
        self.distances
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, d)| *d)
    }

    fn line_of_sight(&self, a: CombatantId, b: CombatantId) -> bool {
        !self.blocked_sight.contains(&ordered(a, b))
    }

    fn cover(&self, attacker: CombatantId, target: CombatantId) -> Cover {
        self.cover
            .iter()
            .find(|(k, _)| *k == (attacker, target))
            .map(|(_, c)| *c)
            .unwrap_or(Cover::None)
    }

    fn engaged(&self, who: CombatantId) -> bool {
        self.engaged.contains(&who)
    }
}
