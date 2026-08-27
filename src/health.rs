//! Hit points and the life state.
//!
//! There is no `hp: i32` plus `dead: bool` pair anywhere. A living combatant
//! holds strictly positive hit points; a dead one holds nothing. Zero and
//! negative hit points are unrepresentable.

use core::num::NonZeroU16;

/// Strictly positive hit points.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct HitPoints(NonZeroU16);

impl HitPoints {
    /// Validate a total. Returns `None` for zero.
    pub const fn new(hp: u16) -> Option<Self> {
        match NonZeroU16::new(hp) {
            Some(n) => Some(HitPoints(n)),
            None => None,
        }
    }

    /// The raw total.
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

/// Alive with hit points, or dead. Nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifeState {
    /// Alive. `current` never exceeds `max`.
    Alive {
        /// The current hit point total. Always at least one.
        current: HitPoints,
        /// The maximum hit point total. Healing never exceeds it.
        max: HitPoints,
    },
    /// Dead. A creature at zero hit points or less is dead.
    Dead,
}

impl LifeState {
    /// A fresh combatant at full hit points.
    pub const fn fresh(max: HitPoints) -> Self {
        LifeState::Alive { current: max, max }
    }

    /// Apply damage. Damage that meets or exceeds the current total kills.
    #[must_use]
    pub fn damaged(self, amount: u16) -> LifeState {
        match self {
            LifeState::Alive { current, max } => {
                match HitPoints::new(current.get().saturating_sub(amount)) {
                    Some(current) => LifeState::Alive { current, max },
                    None => LifeState::Dead,
                }
            }
            LifeState::Dead => LifeState::Dead,
        }
    }

    /// Apply healing, clamped to the maximum. The dead stay dead.
    #[must_use]
    pub fn healed(self, amount: u16) -> LifeState {
        match self {
            LifeState::Alive { current, max } => {
                let raised = current.get().saturating_add(amount).min(max.get());
                let current =
                    HitPoints::new(raised).expect("healing a positive total stays positive");
                LifeState::Alive { current, max }
            }
            LifeState::Dead => LifeState::Dead,
        }
    }

    /// Whether the combatant lives.
    pub const fn is_alive(self) -> bool {
        matches!(self, LifeState::Alive { .. })
    }

    /// The current hit points, when alive.
    pub const fn current(self) -> Option<HitPoints> {
        match self {
            LifeState::Alive { current, .. } => Some(current),
            LifeState::Dead => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hp(n: u16) -> HitPoints {
        HitPoints::new(n).unwrap()
    }

    #[test]
    fn zero_hit_points_is_unrepresentable() {
        assert!(HitPoints::new(0).is_none());
    }

    #[test]
    fn exact_damage_kills() {
        let life = LifeState::fresh(hp(5));
        assert_eq!(life.damaged(4).current(), Some(hp(1)));
        assert_eq!(life.damaged(5), LifeState::Dead);
        assert_eq!(life.damaged(600), LifeState::Dead);
    }

    #[test]
    fn healing_clamps_at_max_and_skips_the_dead() {
        let hurt = LifeState::fresh(hp(8)).damaged(5);
        assert_eq!(hurt.healed(100).current(), Some(hp(8)));
        assert_eq!(LifeState::Dead.healed(100), LifeState::Dead);
    }
}
