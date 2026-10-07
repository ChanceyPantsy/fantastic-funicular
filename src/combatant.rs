//! Combatants: guardians and enemies, ranks, teams and champions.

use std::collections::HashMap;

use crate::ability::{AbilityDef, Loadout};
use crate::buffs::Modifier;
use crate::element::{DamageType, GuardianClass};
use crate::health::{HealthPool, Regen};
use crate::stats::{StatBlock, StatRules};
use crate::status::{StatusKind, StatusSet};
use crate::weapon::{Weapon, WeaponDef};

/// Assigned by the [`Sandbox`](crate::sandbox::Sandbox) on spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CombatantId(pub u32);

/// Combatants on the same team don't damage each other unless friendly
/// fire is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Team(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Rank {
    Guardian,
    /// Red-bar enemies.
    Minor,
    /// Orange-bar enemies (elites).
    Major,
    /// Yellow-bar enemies.
    Miniboss,
    Boss,
    Vehicle,
}

impl Rank {
    /// Whether the Weapons stat's boss bonus (rather than the minor/major
    /// bonus) applies.
    pub fn is_boss_tier(self) -> bool {
        matches!(self, Rank::Miniboss | Rank::Boss | Rank::Vehicle)
    }
}

/// Champion types, each stopped by a matching anti-champion source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ChampionKind {
    /// Raises an immune barrier at health thresholds.
    Barrier,
    /// Regenerates health until disrupted.
    Overload,
    /// Shrugs off damage until stunned.
    Unstoppable,
}

/// Damage multipliers by rank, applied to damage taken.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RankRules {
    pub incoming_mult: HashMap<Rank, f32>,
}

impl Default for RankRules {
    fn default() -> Self {
        Self {
            incoming_mult: HashMap::from([
                (Rank::Guardian, 1.0),
                (Rank::Minor, 1.0),
                (Rank::Major, 1.0),
                (Rank::Miniboss, 0.9),
                (Rank::Boss, 0.8),
                (Rank::Vehicle, 0.8),
            ]),
        }
    }
}

impl RankRules {
    pub fn mult(&self, rank: Rank) -> f32 {
        self.incoming_mult.get(&rank).copied().unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChampionRules {
    pub stun_duration: f32,
    /// Seconds after a stun wears off before the champion can be stunned again.
    pub stun_immunity: f32,
    /// Statuses that stun each champion kind, in addition to anti-champion
    /// weapons. Which verbs stun which champions changes from season to
    /// season, so set this to suit your game.
    pub stunned_by: HashMap<ChampionKind, Vec<StatusKind>>,
    /// Damage taken multiplier while *not* stunned.
    pub unstunned_mult: HashMap<ChampionKind, f32>,
    /// Overload regeneration while not stunned, as a fraction of max health per second.
    pub overload_regen: f32,
    /// Health fractions at which a Barrier champion raises its barrier.
    pub barrier_thresholds: Vec<f32>,
}

impl Default for ChampionRules {
    fn default() -> Self {
        Self {
            stun_duration: 4.0,
            stun_immunity: 6.0,
            stunned_by: HashMap::from([
                (ChampionKind::Overload, vec![StatusKind::Jolt, StatusKind::Suppress, StatusKind::Slow]),
                (ChampionKind::Unstoppable, vec![StatusKind::Blind, StatusKind::Frozen, StatusKind::Suspend]),
                (ChampionKind::Barrier, vec![StatusKind::Volatile, StatusKind::Sever]),
            ]),
            unstunned_mult: HashMap::from([
                (ChampionKind::Barrier, 1.0),
                (ChampionKind::Overload, 1.0),
                (ChampionKind::Unstoppable, 0.5),
            ]),
            overload_regen: 0.03,
            barrier_thresholds: vec![0.66, 0.33],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChampionState {
    pub kind: ChampionKind,
    /// Seconds of stun left.
    pub stunned: f32,
    /// Seconds until it can be stunned again (counts down through the stun).
    pub stun_immune: f32,
    pub barrier_up: bool,
    /// Index into [`ChampionRules::barrier_thresholds`] of the next barrier.
    pub next_barrier: usize,
}

impl ChampionState {
    pub fn new(kind: ChampionKind) -> Self {
        Self { kind, stunned: 0.0, stun_immune: 0.0, barrier_up: false, next_barrier: 0 }
    }

    pub fn is_stunned(&self) -> bool {
        self.stunned > 0.0
    }

    /// Stuns unless already stunned or still immune. Returns whether it did.
    pub fn stun(&mut self, rules: &ChampionRules) -> bool {
        if self.is_stunned() || self.stun_immune > 0.0 {
            return false;
        }
        self.stunned = rules.stun_duration;
        self.stun_immune = rules.stun_duration + rules.stun_immunity;
        true
    }

    pub fn tick(&mut self, dt: f32) {
        self.stunned = (self.stunned - dt).max(0.0);
        self.stun_immune = (self.stun_immune - dt).max(0.0);
    }
}

/// Base guardian survivability. With `Health` stat above 100 the shield
/// grows (see [`StatRules::shield_bonus`]).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GuardianRules {
    pub health: f32,
    pub shield: f32,
    pub regen: Regen,
    pub overshield_decay_per_sec: f32,
}

impl Default for GuardianRules {
    fn default() -> Self {
        Self {
            health: 70.0,
            shield: 130.0,
            regen: Regen { delay: 4.0, health_per_sec: 35.0, shield_per_sec: 70.0 },
            overshield_decay_per_sec: 5.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Combatant {
    /// Set by the sandbox on spawn.
    pub id: CombatantId,
    pub name: String,
    pub team: Team,
    pub rank: Rank,
    pub class: Option<GuardianClass>,
    pub champion: Option<ChampionState>,
    pub health: HealthPool,
    pub statuses: StatusSet,
    pub stats: StatBlock,
    pub loadout: Loadout,
    pub weapons: Vec<Weapon>,
    pub active_weapon: usize,
    /// Buffs, debuffs and perks not tied to a status (timed or permanent).
    pub modifiers: Vec<Modifier>,
    /// World position, synced by the host. Needed for area effects to hit
    /// anything but their primary target.
    pub position: Option<[f32; 3]>,
}

impl Combatant {
    pub fn enemy(name: impl Into<String>, team: Team, rank: Rank, health: f32) -> Self {
        Self {
            id: CombatantId(0),
            name: name.into(),
            team,
            rank,
            class: None,
            champion: None,
            health: HealthPool::new(health),
            statuses: StatusSet::default(),
            stats: StatBlock::default(),
            loadout: Loadout::default(),
            weapons: Vec::new(),
            active_weapon: 0,
            modifiers: Vec::new(),
            position: None,
        }
    }

    pub fn guardian(
        name: impl Into<String>,
        team: Team,
        class: GuardianClass,
        stats: StatBlock,
        rules: &GuardianRules,
        stat_rules: &StatRules,
    ) -> Self {
        let derived = stat_rules.derive(&stats);
        let mut health = HealthPool::new(rules.health)
            .with_shield(rules.shield + derived.shield_bonus, None)
            .with_regen(rules.regen);
        health.shield_regen_mult = derived.shield_recharge;
        health.overshield_decay_per_sec = rules.overshield_decay_per_sec;
        Self { class: Some(class), stats, health, ..Self::enemy(name, team, Rank::Guardian, 0.0) }
    }

    pub fn with_shield(mut self, amount: f32, element: Option<DamageType>) -> Self {
        self.health = self.health.with_shield(amount, element);
        self
    }

    pub fn with_weapon(mut self, def: WeaponDef) -> Self {
        self.weapons.push(Weapon::new(def));
        self
    }

    pub fn with_ability(mut self, def: AbilityDef) -> Self {
        self.loadout.equip(def);
        self
    }

    pub fn with_modifier(mut self, m: Modifier) -> Self {
        self.modifiers.push(m);
        self
    }

    pub fn as_champion(mut self, kind: ChampionKind) -> Self {
        self.champion = Some(ChampionState::new(kind));
        self
    }

    pub fn at(mut self, position: [f32; 3]) -> Self {
        self.position = Some(position);
        self
    }

    pub fn is_alive(&self) -> bool {
        !self.health.is_dead()
    }

    pub fn weapon(&self) -> Option<&Weapon> {
        self.weapons.get(self.active_weapon)
    }

    pub fn weapon_mut(&mut self) -> Option<&mut Weapon> {
        self.weapons.get_mut(self.active_weapon)
    }

    pub fn distance_to(&self, other: &Combatant) -> Option<f32> {
        let (a, b) = (self.position?, other.position?);
        let d: f32 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum();
        Some(d.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardian_health_scales_with_health_stat() {
        let rules = GuardianRules::default();
        let sr = StatRules::default();
        let low = Combatant::guardian("a", Team(0), GuardianClass::Hunter, StatBlock::default(), &rules, &sr);
        let high =
            Combatant::guardian("b", Team(0), GuardianClass::Titan, StatBlock::new(0, 200, 0, 0, 0, 0), &rules, &sr);
        assert_eq!(low.health.max_total(), 200.0);
        assert_eq!(high.health.max_total(), 220.0);
        assert!(high.health.shield_regen_mult > low.health.shield_regen_mult);
    }

    #[test]
    fn distance_needs_both_positions() {
        let a = Combatant::enemy("a", Team(1), Rank::Minor, 10.0).at([0.0, 0.0, 0.0]);
        let b = Combatant::enemy("b", Team(1), Rank::Minor, 10.0).at([3.0, 4.0, 0.0]);
        let c = Combatant::enemy("c", Team(1), Rank::Minor, 10.0);
        assert_eq!(a.distance_to(&b), Some(5.0));
        assert_eq!(a.distance_to(&c), None);
    }
}
