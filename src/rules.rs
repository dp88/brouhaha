//! Rules configuration.
//!
//! Each optional rule is a small strategy enum, not a boolean, so an
//! impossible combination cannot be written and each variant documents its
//! behaviour.

/// How initiative is rolled.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitiativeRule {
    /// One d6 per side each round. The default.
    PerSide {
        /// What happens on a tie.
        ties: TieRule,
    },
    /// One d6 per combatant, modified by dexterity. An optional rule.
    Individual,
}

/// What happens when initiative rolls tie.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TieRule {
    /// Roll again until the tie breaks.
    Reroll,
    /// Act simultaneously: damage and conditions from tied groups land
    /// together, so both sides may fell each other.
    Simultaneous,
}

/// How weapon damage is rolled.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageRule {
    /// Every weapon deals 1d6. The default.
    FlatD6,
    /// Each weapon deals its own damage dice. An optional rule.
    ByWeapon,
}

/// Whether slow weapons act last.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlowWeaponRule {
    /// Slowness is ignored. The default.
    Ignored,
    /// A combatant attacking with a slow weapon acts last in the round, as
    /// if their side had lost initiative.
    ActLast,
}

/// Whether monsters check morale.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoraleRule {
    /// Nobody checks morale.
    AllFearless,
    /// Creatures with a morale score check at the first casualty on their
    /// side and when half their side is down. The default.
    Checked,
}

/// Whether reloading weapons skip rounds.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReloadRule {
    /// Reloading is ignored. The default.
    Ignored,
    /// A reloading weapon fires at most every second round.
    EveryOtherRound,
}

/// The full rules configuration for one combat.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rules {
    /// How initiative is rolled.
    pub initiative: InitiativeRule,
    /// How weapon damage is rolled.
    pub damage: DamageRule,
    /// Whether slow weapons act last.
    pub slow_weapons: SlowWeaponRule,
    /// Whether monsters check morale.
    pub morale: MoraleRule,
    /// Whether reloading weapons skip rounds.
    pub reload: ReloadRule,
}

impl Default for Rules {
    /// The core rules: per-side initiative with rerolled ties, flat d6
    /// damage, no slow weapons, morale checked, no reload tracking.
    fn default() -> Self {
        Rules {
            initiative: InitiativeRule::PerSide {
                ties: TieRule::Reroll,
            },
            damage: DamageRule::FlatD6,
            slow_weapons: SlowWeaponRule::Ignored,
            morale: MoraleRule::Checked,
            reload: ReloadRule::Ignored,
        }
    }
}
