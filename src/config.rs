//! All tuning in one place. Build with [`SandboxConfig::pve`] or
//! [`SandboxConfig::pvp`], then change anything you like, or load it from
//! a data file with the `serde` feature.

use crate::buffs::StackingRules;
use crate::combatant::{ChampionRules, GuardianRules, RankRules};
use crate::health::ShieldRules;
use crate::stats::StatRules;
use crate::status::StatusRules;

/// Orb of Power settings.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrbRules {
    /// Energy (fraction of one charge) granted per orb to each slot.
    pub grenade_energy: f32,
    pub melee_energy: f32,
    pub class_energy: f32,
    pub super_energy: f32,
    /// Seconds before an uncollected orb disappears.
    pub lifetime: f32,
    /// Super kills drop an orb.
    pub on_super_kill: bool,
    /// Killing a Major or tougher enemy drops an orb.
    pub on_major_kill: bool,
}

impl Default for OrbRules {
    fn default() -> Self {
        Self {
            grenade_energy: 0.1,
            melee_energy: 0.1,
            class_energy: 0.1,
            super_energy: 0.05,
            lifetime: 30.0,
            on_super_kill: true,
            on_major_kill: true,
        }
    }
}

/// Super energy earned from combat.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EnergyRules {
    /// Super energy per point of damage dealt (excluding super damage).
    pub super_per_damage: f32,
    /// Super energy per kill.
    pub super_per_kill: f32,
}

impl Default for EnergyRules {
    fn default() -> Self {
        Self { super_per_damage: 1.0 / 20_000.0, super_per_kill: 0.01 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameMode {
    PvE,
    PvP,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SandboxConfig {
    pub mode: GameMode,
    pub stacking: StackingRules,
    pub statuses: StatusRules,
    pub shields: ShieldRules,
    pub stats: StatRules,
    pub ranks: RankRules,
    pub champions: ChampionRules,
    pub guardian: GuardianRules,
    pub orbs: OrbRules,
    pub energy: EnergyRules,
    pub friendly_fire: bool,
    /// Limit on effects causing effects (explosion → ignite → chain...).
    pub max_chain_depth: u32,
}

impl SandboxConfig {
    pub fn pve() -> Self {
        Self {
            mode: GameMode::PvE,
            stacking: StackingRules::default(),
            statuses: StatusRules::pve(),
            shields: ShieldRules::default(),
            stats: StatRules::default(),
            ranks: RankRules::default(),
            champions: ChampionRules::default(),
            guardian: GuardianRules::default(),
            orbs: OrbRules::default(),
            energy: EnergyRules::default(),
            friendly_fire: false,
            max_chain_depth: 8,
        }
    }

    pub fn pvp() -> Self {
        Self {
            mode: GameMode::PvP,
            statuses: StatusRules::pvp(),
            energy: EnergyRules { super_per_damage: 1.0 / 4_000.0, super_per_kill: 0.05 },
            orbs: OrbRules { on_major_kill: false, ..OrbRules::default() },
            ..Self::pve()
        }
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self::pve()
    }
}
