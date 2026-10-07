//! The damage pipeline.
//!
//! ```text
//! base (already includes precision + falloff)
//!   × attacker outgoing modifiers   (statuses + perks; see `buffs`)
//!   × target incoming modifiers     (Weaken, Woven Mail, Frost Armor...)
//!   × stat bonus                    (Weapons/Grenade/Melee/Super stat, PvE only)
//!   × rank multiplier               (bosses take less)
//!   × champion multiplier           (Unstoppable resists until stunned)
//!   → HealthPool::absorb            (overshield → elemental shield → health)
//! ```

use crate::ability::AbilitySlot;
use crate::buffs;
use crate::combatant::{ChampionKind, Combatant, CombatantId, Rank};
use crate::config::SandboxConfig;
use crate::element::DamageType;
use crate::status::{StatusKind, TriggerKind};
use crate::weapon::{AmmoType, WeaponArchetype};

/// Where damage came from. Modifier scopes, stat bonuses and super energy
/// gain all key off this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SourceKind {
    Weapon {
        ammo: AmmoType,
        archetype: WeaponArchetype,
    },
    Grenade,
    Melee,
    ClassAbility,
    Super,
    /// Burn and other damage over time from a status.
    DamageOverTime(StatusKind),
    /// Ignite, Shatter, chain lightning and the like.
    Trigger(TriggerKind),
    /// Explosion from breaking an elemental shield with its own element.
    ShieldBreak,
    Environment,
}

impl SourceKind {
    pub fn is_weapon(&self) -> bool {
        matches!(self, SourceKind::Weapon { .. })
    }

    pub fn ammo(&self) -> Option<AmmoType> {
        match self {
            SourceKind::Weapon { ammo, .. } => Some(*ammo),
            _ => None,
        }
    }

    pub fn from_slot(slot: AbilitySlot) -> Self {
        match slot {
            AbilitySlot::Grenade => SourceKind::Grenade,
            AbilitySlot::Melee => SourceKind::Melee,
            AbilitySlot::ClassAbility => SourceKind::ClassAbility,
            AbilitySlot::Super => SourceKind::Super,
        }
    }
}

/// One hit, before any buffs are applied.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DamageInstance {
    pub attacker: Option<CombatantId>,
    /// Damage after precision and falloff, before modifiers.
    pub amount: f32,
    pub damage_type: DamageType,
    pub kind: SourceKind,
    pub precision: bool,
    pub anti_champion: Option<ChampionKind>,
    /// Statuses applied to the target if the hit lands and doesn't kill.
    pub on_hit: Vec<(StatusKind, u32)>,
}

impl DamageInstance {
    pub fn new(amount: f32, damage_type: DamageType, kind: SourceKind) -> Self {
        Self { attacker: None, amount, damage_type, kind, precision: false, anti_champion: None, on_hit: Vec::new() }
    }

    pub fn from(mut self, attacker: CombatantId) -> Self {
        self.attacker = Some(attacker);
        self
    }

    pub fn precision(mut self, precision: bool) -> Self {
        self.precision = precision;
        self
    }

    pub fn anti_champion(mut self, kind: Option<ChampionKind>) -> Self {
        self.anti_champion = kind;
        self
    }

    pub fn applying(mut self, status: StatusKind, stacks: u32) -> Self {
        self.on_hit.push((status, stacks));
        self
    }
}

/// Every multiplier that went into a hit, for debugging and UI.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DamageCalc {
    pub base: f32,
    pub outgoing_mult: f32,
    pub incoming_mult: f32,
    pub stat_mult: f32,
    pub rank_mult: f32,
    pub champion_mult: f32,
    pub final_amount: f32,
}

/// Bonus from the attacker's stats for this kind of damage against this rank.
/// Stat damage bonuses only apply against non-guardians (PvE).
fn stat_bonus(kind: &SourceKind, attacker: &Combatant, target_rank: Rank, cfg: &SandboxConfig) -> f32 {
    if target_rank == Rank::Guardian {
        return 0.0;
    }
    let d = cfg.stats.derive(&attacker.stats);
    match kind {
        SourceKind::Weapon { .. } if target_rank.is_boss_tier() => d.weapon_damage_vs_boss,
        SourceKind::Weapon { .. } => d.weapon_damage_vs_minor,
        SourceKind::Grenade => d.grenade_damage_bonus,
        SourceKind::Melee => d.melee_damage_bonus,
        SourceKind::Super => d.super_damage_bonus,
        _ => 0.0,
    }
}

/// Runs the pipeline without changing anything. The sandbox then feeds
/// `final_amount` to the target's [`HealthPool`](crate::health::HealthPool).
pub fn calculate(
    inst: &DamageInstance,
    attacker: Option<&Combatant>,
    target: &Combatant,
    cfg: &SandboxConfig,
) -> DamageCalc {
    let rules = &cfg.statuses;
    let outgoing_mult = attacker.map_or(1.0, |a| {
        let mods = a.modifiers.iter().map(|m| (m, 1)).chain(a.statuses.outgoing(rules));
        buffs::resolve(mods, &inst.kind, inst.damage_type, &cfg.stacking)
    });
    let incoming_mods = target.modifiers.iter().map(|m| (m, 1)).chain(target.statuses.incoming(rules));
    let incoming_mult = buffs::resolve(incoming_mods, &inst.kind, inst.damage_type, &cfg.stacking);
    let stat_mult = 1.0 + attacker.map_or(0.0, |a| stat_bonus(&inst.kind, a, target.rank, cfg));
    let rank_mult = cfg.ranks.mult(target.rank);
    let champion_mult = match &target.champion {
        Some(c) if !c.is_stunned() => cfg.champions.unstunned_mult.get(&c.kind).copied().unwrap_or(1.0),
        _ => 1.0,
    };
    let final_amount = (inst.amount * outgoing_mult * incoming_mult * stat_mult * rank_mult * champion_mult).max(0.0);
    DamageCalc { base: inst.amount, outgoing_mult, incoming_mult, stat_mult, rank_mult, champion_mult, final_amount }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffs::{Modifier, ModifierScope};
    use crate::combatant::{ChampionKind, Team};
    use crate::stats::StatBlock;

    fn weapon_hit(amount: f32) -> DamageInstance {
        DamageInstance::new(
            amount,
            DamageType::Solar,
            SourceKind::Weapon { ammo: AmmoType::Primary, archetype: WeaponArchetype::HandCannon },
        )
    }

    #[test]
    fn buffs_debuffs_and_rank_combine() {
        let cfg = SandboxConfig::pve();
        let mut attacker = Combatant::enemy("a", Team(0), Rank::Guardian, 100.0);
        attacker.statuses.apply(StatusKind::Radiant, 1, None, &cfg.statuses);
        let mut target = Combatant::enemy("b", Team(1), Rank::Boss, 1000.0);
        target.statuses.apply(StatusKind::Weaken, 1, None, &cfg.statuses);

        let c = calculate(&weapon_hit(100.0), Some(&attacker), &target, &cfg);
        let expected = 100.0 * 1.25 * 1.15 * cfg.ranks.mult(Rank::Boss);
        assert!((c.final_amount - expected).abs() < 1e-3, "{c:?}");
    }

    #[test]
    fn weapons_stat_bonus_is_pve_only_and_rank_aware() {
        let cfg = SandboxConfig::pve();
        let mut a = Combatant::enemy("a", Team(0), Rank::Guardian, 100.0);
        a.stats = StatBlock::new(200, 0, 0, 0, 0, 0);
        let minor = Combatant::enemy("m", Team(1), Rank::Minor, 100.0);
        let boss = Combatant::enemy("b", Team(1), Rank::Boss, 100.0);
        let guardian = Combatant::enemy("g", Team(1), Rank::Guardian, 100.0);
        let hit = weapon_hit(100.0);
        assert!((calculate(&hit, Some(&a), &minor, &cfg).stat_mult - 1.15).abs() < 1e-5);
        assert!((calculate(&hit, Some(&a), &boss, &cfg).stat_mult - 1.15).abs() < 1e-5);
        assert_eq!(calculate(&hit, Some(&a), &guardian, &cfg).stat_mult, 1.0);
    }

    #[test]
    fn unstoppable_resists_until_stunned() {
        let cfg = SandboxConfig::pve();
        let mut t = Combatant::enemy("u", Team(1), Rank::Miniboss, 1000.0).as_champion(ChampionKind::Unstoppable);
        let hit = weapon_hit(100.0);
        assert_eq!(calculate(&hit, None, &t, &cfg).champion_mult, 0.5);
        t.champion.as_mut().unwrap().stunned = 1.0;
        assert_eq!(calculate(&hit, None, &t, &cfg).champion_mult, 1.0);
    }

    #[test]
    fn scoped_modifier_ignores_other_sources() {
        let cfg = SandboxConfig::pve();
        let a = Combatant::enemy("a", Team(0), Rank::Guardian, 100.0).with_modifier(Modifier::multiplicative(
            "melee perk",
            ModifierScope::Melee,
            1.0,
        ));
        let t = Combatant::enemy("t", Team(1), Rank::Minor, 100.0);
        assert_eq!(calculate(&weapon_hit(10.0), Some(&a), &t, &cfg).final_amount, 10.0);
        let melee = DamageInstance::new(10.0, DamageType::Arc, SourceKind::Melee);
        assert_eq!(calculate(&melee, Some(&a), &t, &cfg).final_amount, 20.0);
    }
}
