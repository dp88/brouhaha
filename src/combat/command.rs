//! The flat boundary: one combat value in any phase, one command enum.
//!
//! The typed phase methods are the kernel. [`AnyCombat`] and [`Command`]
//! collapse them into a single untyped surface for storage, user
//! interfaces, and networks. A recorded command list plus the dice seed
//! replays a whole fight.

use alloc::vec::Vec;

use super::event::{EventCursor, EventLog};
use super::{
    ActError, CastError, Combat, Concluded, DeclareError, Declaring, MagicStage, MeleeStage,
    MissileStage, MoraleError, MoraleStage, MoveError, MovementStage, TurnEnd,
};
use crate::dice::DiceRoller;
use crate::magic::{DeclaredTarget, SpellRef};
use crate::space::{SpatialOracle, TargetingError};
use crate::units::{CombatantId, Feet, SideId};

/// A combat in whichever phase: what an application stores.
pub enum AnyCombat {
    /// Declarations are open.
    Declaring(Combat<Declaring>),
    /// The acting group's morale checks.
    Morale(Combat<MoraleStage>),
    /// The acting group's movement.
    Movement(Combat<MovementStage>),
    /// The acting group's missile attacks.
    Missiles(Combat<MissileStage>),
    /// The acting group's spell casting.
    Magic(Combat<MagicStage>),
    /// The acting group's melee attacks.
    Melee(Combat<MeleeStage>),
    /// The combat is over.
    Concluded(Combat<Concluded>),
}

macro_rules! from_phase {
    ($phase:ty, $variant:ident) => {
        impl From<Combat<$phase>> for AnyCombat {
            fn from(combat: Combat<$phase>) -> Self {
                AnyCombat::$variant(combat)
            }
        }
    };
}

from_phase!(Declaring, Declaring);
from_phase!(MoraleStage, Morale);
from_phase!(MovementStage, Movement);
from_phase!(MissileStage, Missiles);
from_phase!(MagicStage, Magic);
from_phase!(MeleeStage, Melee);
from_phase!(Concluded, Concluded);

impl From<TurnEnd> for AnyCombat {
    fn from(end: TurnEnd) -> Self {
        match end {
            TurnEnd::NextGroup(c) => AnyCombat::Morale(c),
            TurnEnd::NewRound(c) => AnyCombat::Declaring(c),
            TurnEnd::Over(c) => AnyCombat::Concluded(c),
        }
    }
}

/// One request into the combat, as data.
///
/// Special actions carry trait objects and stay on the typed API.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Command {
    /// Declare a spell for this round.
    DeclareSpell {
        /// The caster.
        caster: CombatantId,
        /// The memorized spell.
        spell: SpellRef,
        /// The declared target.
        target: DeclaredTarget,
    },
    /// Declare a fighting withdrawal.
    DeclareWithdrawal {
        /// The engaged combatant.
        who: CombatantId,
    },
    /// Declare a full retreat.
    DeclareRetreat {
        /// The engaged combatant.
        who: CombatantId,
    },
    /// Switch the wielded weapon between rounds.
    Wield {
        /// The combatant.
        who: CombatantId,
        /// The index into the carried weapons.
        weapon_index: usize,
    },
    /// Close declarations and roll initiative.
    RollInitiative,
    /// Resolve a due morale check.
    CheckMorale {
        /// The side owing the check.
        side: SideId,
    },
    /// Move an acting combatant.
    Move {
        /// The mover.
        who: CombatantId,
        /// The distance, in feet.
        distance: Feet,
    },
    /// Fire at a target.
    Shoot {
        /// The attacker.
        attacker: CombatantId,
        /// The target.
        target: CombatantId,
    },
    /// Cast the spell declared this round.
    Cast {
        /// The caster.
        caster: CombatantId,
    },
    /// Strike a target in melee.
    Strike {
        /// The attacker.
        attacker: CombatantId,
        /// The target.
        target: CombatantId,
    },
    /// Finish the current stage and advance.
    FinishStage,
}

/// Why a command was refused. The combat is unchanged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandError {
    /// The command does not exist in the current phase.
    WrongPhase,
    /// A declaration failed.
    Declare(DeclareError),
    /// A morale check failed.
    Morale(MoraleError),
    /// A move failed.
    Move(MoveError),
    /// Targeting failed.
    Targeting(TargetingError),
    /// An attack failed against stale evidence.
    Act(ActError),
    /// A cast failed.
    Cast(CastError),
}

impl AnyCombat {
    /// The event log so far.
    pub fn log(&self) -> &EventLog {
        match self {
            AnyCombat::Declaring(c) => c.log(),
            AnyCombat::Morale(c) => c.log(),
            AnyCombat::Movement(c) => c.log(),
            AnyCombat::Missiles(c) => c.log(),
            AnyCombat::Magic(c) => c.log(),
            AnyCombat::Melee(c) => c.log(),
            AnyCombat::Concluded(c) => c.log(),
        }
    }

    /// A short name for the current phase, for display and logs.
    pub fn phase(&self) -> &'static str {
        match self {
            AnyCombat::Declaring(_) => "declaring",
            AnyCombat::Morale(_) => "morale",
            AnyCombat::Movement(_) => "movement",
            AnyCombat::Missiles(_) => "missiles",
            AnyCombat::Magic(_) => "magic",
            AnyCombat::Melee(_) => "melee",
            AnyCombat::Concluded(_) => "concluded",
        }
    }

    /// Whether the combat is over.
    pub fn is_over(&self) -> bool {
        matches!(self, AnyCombat::Concluded(_))
    }

    /// Apply one command. On success and on failure alike, the combat
    /// comes back to the caller; a refused command changes nothing.
    // The large Err variant is the point: the combat survives refusal.
    #[allow(clippy::result_large_err)]
    pub fn apply(
        self,
        command: &Command,
        oracle: &dyn SpatialOracle,
        r: &mut impl DiceRoller,
    ) -> Result<(AnyCombat, EventCursor), (AnyCombat, CommandError)> {
        use AnyCombat as A;
        use Command as C;

        /// Map a fallible `&mut` call: hand the combat back either way.
        macro_rules! fallible {
            ($combat:expr, $call:expr, $wrap:path) => {{
                let mut combat = $combat;
                match $call(&mut combat) {
                    Ok(cursor) => Ok((combat.into(), cursor)),
                    Err(e) => Err((combat.into(), $wrap(e))),
                }
            }};
        }

        match (self, command) {
            (
                A::Declaring(c),
                C::DeclareSpell {
                    caster,
                    spell,
                    target,
                },
            ) => {
                fallible!(
                    c,
                    |c: &mut Combat<Declaring>| c.declare_spell(*caster, *spell, target.clone()),
                    CommandError::Declare
                )
            }
            (A::Declaring(c), C::DeclareWithdrawal { who }) => {
                fallible!(
                    c,
                    |c: &mut Combat<Declaring>| c.declare_withdrawal(*who, oracle),
                    CommandError::Declare
                )
            }
            (A::Declaring(c), C::DeclareRetreat { who }) => {
                fallible!(
                    c,
                    |c: &mut Combat<Declaring>| c.declare_retreat(*who, oracle),
                    CommandError::Declare
                )
            }
            (A::Declaring(mut c), C::Wield { who, weapon_index }) => {
                let cursor = c.log().cursor();
                match c.wield(*who, *weapon_index) {
                    Ok(()) => Ok((c.into(), cursor)),
                    Err(e) => Err((c.into(), CommandError::Declare(e))),
                }
            }
            (A::Declaring(c), C::RollInitiative) => {
                let cursor = c.log().cursor();
                Ok((c.roll_initiative(r).into(), cursor))
            }
            (A::Morale(mut c), C::CheckMorale { side }) => {
                let cursor = c.log().cursor();
                match c.check(*side, r) {
                    Ok(_) => Ok((c.into(), cursor)),
                    Err(e) => Err((c.into(), CommandError::Morale(e))),
                }
            }
            (A::Morale(c), C::FinishStage) => {
                let cursor = c.log().cursor();
                Ok((c.finish_morale().into(), cursor))
            }
            (A::Movement(mut c), C::Move { who, distance }) => {
                match c.witness_move(*who, *distance, oracle) {
                    Ok(legal) => {
                        let cursor = c.make_move(legal);
                        Ok((c.into(), cursor))
                    }
                    Err(e) => Err((c.into(), CommandError::Move(e))),
                }
            }
            (A::Movement(c), C::FinishStage) => {
                let cursor = c.log().cursor();
                Ok((c.finish_movement().into(), cursor))
            }
            (A::Missiles(mut c), C::Shoot { attacker, target }) => {
                match c.witness_shot(*attacker, *target, oracle) {
                    Ok(shot) => match c.shoot(shot, r) {
                        Ok(cursor) => Ok((c.into(), cursor)),
                        Err(e) => Err((c.into(), CommandError::Act(e))),
                    },
                    Err(e) => Err((c.into(), CommandError::Targeting(e))),
                }
            }
            (A::Missiles(c), C::FinishStage) => {
                let cursor = c.log().cursor();
                Ok((c.finish_missiles().into(), cursor))
            }
            (A::Magic(c), C::Cast { caster }) => {
                fallible!(
                    c,
                    |c: &mut Combat<MagicStage>| c.cast(*caster, oracle, r),
                    CommandError::Cast
                )
            }
            (A::Magic(c), C::FinishStage) => {
                let cursor = c.log().cursor();
                Ok((c.finish_magic().into(), cursor))
            }
            (A::Melee(mut c), C::Strike { attacker, target }) => {
                match c.witness_melee(*attacker, *target, oracle) {
                    Ok(strike) => match c.strike(strike, r) {
                        Ok(cursor) => Ok((c.into(), cursor)),
                        Err(e) => Err((c.into(), CommandError::Act(e))),
                    },
                    Err(e) => Err((c.into(), CommandError::Targeting(e))),
                }
            }
            (A::Melee(c), C::FinishStage) => {
                let cursor = c.log().cursor();
                Ok((c.finish_melee().into(), cursor))
            }
            (combat, _) => Err((combat, CommandError::WrongPhase)),
        }
    }

    /// Apply a recorded command list in order, ignoring refused commands.
    /// With the same setup and the same dice, this replays a fight exactly.
    pub fn replay(
        mut self,
        commands: &[Command],
        oracle: &dyn SpatialOracle,
        r: &mut impl DiceRoller,
    ) -> AnyCombat {
        for command in commands {
            self = match self.apply(command, oracle, r) {
                Ok((next, _)) => next,
                Err((next, _)) => next,
            };
        }
        self
    }
}

/// A recorder: collect the commands you apply, for later replay.
#[derive(Clone, Debug, Default)]
pub struct CommandLog {
    commands: Vec<Command>,
}

impl CommandLog {
    /// An empty log.
    pub fn new() -> Self {
        CommandLog::default()
    }

    /// Record one command.
    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// The recorded commands, in order.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
}
