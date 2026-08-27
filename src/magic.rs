//! Spells, spell slots, and memorization.
//!
//! A spell's mechanics are an [`crate::effect::Effect`]. The combat
//! machine enforces casting legality: the declaration, freedom of speech and
//! gesture, line of sight, and disruption.

use alloc::string::String;
use alloc::vec::Vec;
use core::num::NonZeroU8;

use crate::effect::Effect;
use crate::units::Feet;

/// A spell level in `1..=6`, by construction.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SpellLevel(NonZeroU8);

impl SpellLevel {
    /// Validate a spell level. Returns `None` outside `1..=6`.
    pub const fn new(level: u8) -> Option<Self> {
        if level >= 1 && level <= 6 {
            match NonZeroU8::new(level) {
                Some(n) => Some(SpellLevel(n)),
                None => None,
            }
        } else {
            None
        }
    }

    /// The raw spell level.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// Spell slots per spell level, one through six.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SpellSlots([u8; 6]);

impl SpellSlots {
    /// No slots at all: a class that does not cast.
    pub const NONE: SpellSlots = SpellSlots([0; 6]);

    /// Slots from an array indexed by spell level minus one.
    pub const fn new(slots: [u8; 6]) -> Self {
        SpellSlots(slots)
    }

    /// The slot count for one spell level.
    pub const fn at(self, level: SpellLevel) -> u8 {
        self.0[(level.get() - 1) as usize]
    }

    /// The total slot count across all spell levels.
    pub fn total(self) -> u16 {
        self.0.iter().map(|&n| u16::from(n)).sum()
    }
}

/// How far a spell reaches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellRange {
    /// The spell affects only the caster.
    Caster,
    /// The spell reaches a target within this distance.
    Feet(Feet),
}

/// What shape of target a spell accepts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellTargeting {
    /// The caster only.
    Caster,
    /// One creature.
    Single,
    /// One or more creatures. The spell description bounds the count; the
    /// application enforces it when declaring.
    Many,
}

/// A spell: identity, level, reach, targeting, and its mechanics as an
/// effect.
#[derive(Clone, Debug)]
pub struct Spell {
    /// A display name. The kernel never interprets it.
    pub name: String,
    /// The spell level.
    pub level: SpellLevel,
    /// How far the spell reaches.
    pub range: SpellRange,
    /// What shape of target it accepts.
    pub targeting: SpellTargeting,
    /// Whether the caster needs line of sight to the target. Most spells do.
    pub needs_sight: bool,
    /// Whether the effect counts as magic for saving throw bonuses. Almost
    /// always true for a spell.
    pub magical: bool,
    /// The mechanics.
    pub effect: Effect,
}

/// The target a caster declared for a spell.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DeclaredTarget {
    /// The caster targets themself.
    SelfCast,
    /// One creature.
    One(crate::units::CombatantId),
    /// Several creatures. The application bounds the count per the spell
    /// description.
    Many(Vec<crate::units::CombatantId>),
}

/// A stable handle to one memorized spell of one combatant.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SpellRef(pub(crate) u8);

/// An error building a [`Memorized`] set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemorizeError {
    /// More spells of one level than the slots allow.
    TooManyOfLevel {
        /// The overfull spell level.
        level: SpellLevel,
    },
    /// More than 255 spells.
    TooManySpells,
}

/// The spells one combatant holds in memory, and which are spent.
///
/// Casting a spell erases it from memory until it is memorized again. The
/// same spell may appear more than once when slots allow.
#[derive(Clone, Debug)]
pub struct Memorized {
    entries: Vec<(Spell, bool)>,
}

impl Memorized {
    /// Validate a set of spells against a slot allowance.
    pub fn prepare(slots: SpellSlots, spells: Vec<Spell>) -> Result<Self, MemorizeError> {
        if spells.len() > usize::from(u8::MAX) {
            return Err(MemorizeError::TooManySpells);
        }
        let mut counts = [0u8; 6];
        for spell in &spells {
            let i = (spell.level.get() - 1) as usize;
            counts[i] += 1;
            if counts[i] > slots.at(spell.level) {
                return Err(MemorizeError::TooManyOfLevel { level: spell.level });
            }
        }
        Ok(Memorized {
            entries: spells.into_iter().map(|s| (s, false)).collect(),
        })
    }

    /// The spells still available to cast.
    pub fn available(&self) -> impl Iterator<Item = (SpellRef, &Spell)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, (_, cast))| !cast)
            .map(|(i, (spell, _))| (SpellRef(i as u8), spell))
    }

    /// The spell behind a handle, spent or not.
    pub fn spell(&self, handle: SpellRef) -> Option<&Spell> {
        self.entries.get(usize::from(handle.0)).map(|(s, _)| s)
    }

    /// Whether the spell behind a handle is still available.
    pub fn is_available(&self, handle: SpellRef) -> bool {
        matches!(self.entries.get(usize::from(handle.0)), Some((_, false)))
    }

    /// Spend a spell: erase it from memory. Idempotent.
    pub(crate) fn expend(&mut self, handle: SpellRef) {
        if let Some((_, cast)) = self.entries.get_mut(usize::from(handle.0)) {
            *cast = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::{DiceExpr, Die};

    fn spell(name: &str, level: u8) -> Spell {
        Spell {
            name: String::from(name),
            level: SpellLevel::new(level).unwrap(),
            range: SpellRange::Feet(Feet(120)),
            targeting: SpellTargeting::Single,
            needs_sight: true,
            magical: true,
            effect: Effect::Damage(DiceExpr::of(1, Die::D6)),
        }
    }

    #[test]
    fn slots_bound_memorization() {
        let slots = SpellSlots::new([2, 1, 0, 0, 0, 0]);
        let ok = Memorized::prepare(
            slots,
            alloc::vec![spell("a", 1), spell("a", 1), spell("b", 2)],
        );
        assert!(ok.is_ok());
        let err = Memorized::prepare(
            slots,
            alloc::vec![spell("a", 1), spell("a", 1), spell("c", 1)],
        );
        assert_eq!(
            err.unwrap_err(),
            MemorizeError::TooManyOfLevel {
                level: SpellLevel::new(1).unwrap()
            }
        );
    }

    #[test]
    fn expending_removes_from_available() {
        let slots = SpellSlots::new([2, 0, 0, 0, 0, 0]);
        let mut m = Memorized::prepare(slots, alloc::vec![spell("a", 1), spell("b", 1)]).unwrap();
        let first = m.available().next().unwrap().0;
        m.expend(first);
        assert!(!m.is_available(first));
        assert_eq!(m.available().count(), 1);
        assert!(m.spell(first).is_some());
    }
}
