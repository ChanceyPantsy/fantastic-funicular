//! Status effects ("verbs" and "keywords").
//!
//! Each [`StatusKind`] is described by a data-driven [`StatusDef`]: how it
//! stacks, how long it lasts, damage over time, healing, the damage
//! modifiers it grants, what it stops its holder from doing, and what it
//! turns into at a stack threshold (Scorch → Ignite, Slow → Freeze) or when
//! its holder takes damage (Jolt → chain lightning, Frozen → Shatter).
//!
//! Area effects are [`TriggerKind`]s with a [`TriggerDef`]; the
//! [`Sandbox`](crate::sandbox::Sandbox) resolves who they hit.

use std::collections::HashMap;

use crate::buffs::{Modifier, ModifierScope};
use crate::combatant::CombatantId;
use crate::element::DamageType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StatusKind {
    // Solar
    Scorch,
    Radiant,
    Restoration,
    // Arc
    Jolt,
    Blind,
    Amplified,
    // Void
    Volatile,
    Weaken,
    Suppress,
    Devour,
    Invisible,
    // Stasis
    Slow,
    Frozen,
    FrostArmor,
    // Strand
    Sever,
    Suspend,
    Unravel,
    WovenMail,
}

impl StatusKind {
    pub const ALL: [StatusKind; 18] = [
        StatusKind::Scorch,
        StatusKind::Radiant,
        StatusKind::Restoration,
        StatusKind::Jolt,
        StatusKind::Blind,
        StatusKind::Amplified,
        StatusKind::Volatile,
        StatusKind::Weaken,
        StatusKind::Suppress,
        StatusKind::Devour,
        StatusKind::Invisible,
        StatusKind::Slow,
        StatusKind::Frozen,
        StatusKind::FrostArmor,
        StatusKind::Sever,
        StatusKind::Suspend,
        StatusKind::Unravel,
        StatusKind::WovenMail,
    ];

    /// Statuses you put on enemies, as opposed to buffs you give yourself.
    pub fn is_debuff(self) -> bool {
        matches!(
            self,
            StatusKind::Scorch
                | StatusKind::Jolt
                | StatusKind::Blind
                | StatusKind::Volatile
                | StatusKind::Weaken
                | StatusKind::Suppress
                | StatusKind::Slow
                | StatusKind::Frozen
                | StatusKind::Sever
                | StatusKind::Suspend
                | StatusKind::Unravel
        )
    }
}

/// An area or chain effect produced by a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TriggerKind {
    /// Solar explosion when Scorch reaches its threshold.
    Ignite,
    /// Arc lightning that chains from a Jolted target when it takes damage.
    JoltChain,
    /// Void explosion when a Volatile target takes damage.
    VolatileExplosion,
    /// Stasis burst when a Frozen target takes damage.
    Shatter,
    /// Strand threads fired from an Unraveled target when it takes damage.
    UnravelThreads,
}

/// What happens when a status reaches its stack threshold. The status is
/// always removed first.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ThresholdAction {
    Trigger(TriggerKind),
    Become(StatusKind),
}

/// Actions a status prevents its holder from taking. The host game reads
/// these; the sandbox enforces `abilities` and `weapons` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Restrictions {
    pub movement: bool,
    pub weapons: bool,
    pub abilities: bool,
}

impl Restrictions {
    pub const NONE: Restrictions = Restrictions { movement: false, weapons: false, abilities: false };
    pub const ALL: Restrictions = Restrictions { movement: true, weapons: true, abilities: true };

    fn union(self, o: Restrictions) -> Restrictions {
        Restrictions {
            movement: self.movement || o.movement,
            weapons: self.weapons || o.weapons,
            abilities: self.abilities || o.abilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusDef {
    pub max_stacks: u32,
    /// Seconds; reapplying refreshes it.
    pub duration: f32,
    /// Stacks lost per second (Scorch-style decay). 0 = none.
    pub stack_decay_per_sec: f32,
    /// Damage per second per stack, dealt as `dot_type`.
    pub dot_per_stack: f32,
    pub dot_type: DamageType,
    /// Health restored per second per stack.
    pub heal_per_stack: f32,
    pub threshold: Option<(u32, ThresholdAction)>,
    /// Fired when the holder takes damage.
    pub on_damaged: Option<TriggerKind>,
    /// Remove the status after `on_damaged` fires (Volatile, Frozen).
    pub consume_on_trigger: bool,
    /// Internal cooldown between `on_damaged` triggers.
    pub trigger_cooldown: f32,
    /// Modifiers on damage the holder deals.
    pub outgoing: Vec<Modifier>,
    /// Modifiers on damage the holder takes.
    pub incoming: Vec<Modifier>,
    pub move_speed_mult: f32,
    pub restrictions: Restrictions,
    /// Health restored when the holder gets a kill (Devour).
    pub on_kill_heal: f32,
    /// Duration refreshes when the holder gets a kill (Devour).
    pub refresh_on_kill: bool,
}

impl Default for StatusDef {
    fn default() -> Self {
        Self {
            max_stacks: 1,
            duration: 5.0,
            stack_decay_per_sec: 0.0,
            dot_per_stack: 0.0,
            dot_type: DamageType::Kinetic,
            heal_per_stack: 0.0,
            threshold: None,
            on_damaged: None,
            consume_on_trigger: false,
            trigger_cooldown: 0.0,
            outgoing: Vec::new(),
            incoming: Vec::new(),
            move_speed_mult: 1.0,
            restrictions: Restrictions::NONE,
            on_kill_heal: 0.0,
            refresh_on_kill: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TriggerDef {
    pub damage: f32,
    pub damage_type: DamageType,
    pub radius: f32,
    /// Hit the combatant the effect came from.
    pub hits_origin: bool,
    /// Maximum other combatants hit. `None` = everyone in `radius`.
    pub max_targets: Option<u32>,
}

/// Tuning tables for every status and trigger.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusRules {
    pub statuses: HashMap<StatusKind, StatusDef>,
    pub triggers: HashMap<TriggerKind, TriggerDef>,
}

impl StatusRules {
    pub fn def(&self, kind: StatusKind) -> &StatusDef {
        self.statuses.get(&kind).unwrap_or_else(|| panic!("StatusRules has no definition for {kind:?}"))
    }

    pub fn trigger(&self, kind: TriggerKind) -> &TriggerDef {
        self.triggers.get(&kind).unwrap_or_else(|| panic!("StatusRules has no definition for {kind:?}"))
    }

    /// PvE tuning. Approximations: tune to taste.
    pub fn pve() -> Self {
        use StatusKind as S;
        let d = StatusDef::default;
        let statuses = HashMap::from([
            (
                S::Scorch,
                StatusDef {
                    max_stacks: 100,
                    duration: 4.0,
                    stack_decay_per_sec: 5.0,
                    dot_per_stack: 0.5,
                    dot_type: DamageType::Solar,
                    threshold: Some((100, ThresholdAction::Trigger(TriggerKind::Ignite))),
                    ..d()
                },
            ),
            (
                S::Radiant,
                StatusDef {
                    duration: 10.0,
                    outgoing: vec![Modifier::empowering("Radiant", ModifierScope::Weapons, 0.25)],
                    ..d()
                },
            ),
            (S::Restoration, StatusDef { max_stacks: 2, duration: 8.0, heal_per_stack: 20.0, ..d() }),
            (
                S::Jolt,
                StatusDef { duration: 6.0, on_damaged: Some(TriggerKind::JoltChain), trigger_cooldown: 0.7, ..d() },
            ),
            (
                S::Blind,
                StatusDef {
                    duration: 4.0,
                    restrictions: Restrictions { movement: false, weapons: true, abilities: true },
                    ..d()
                },
            ),
            (S::Amplified, StatusDef { duration: 10.0, move_speed_mult: 1.2, ..d() }),
            (
                S::Volatile,
                StatusDef {
                    duration: 10.0,
                    on_damaged: Some(TriggerKind::VolatileExplosion),
                    consume_on_trigger: true,
                    ..d()
                },
            ),
            (S::Weaken, StatusDef { duration: 7.0, incoming: vec![Modifier::debuff("Weaken", 0.15)], ..d() }),
            (
                S::Suppress,
                StatusDef {
                    duration: 4.0,
                    restrictions: Restrictions { movement: false, weapons: false, abilities: true },
                    ..d()
                },
            ),
            (S::Devour, StatusDef { duration: 10.0, on_kill_heal: 200.0, refresh_on_kill: true, ..d() }),
            (S::Invisible, StatusDef { duration: 6.0, ..d() }),
            (
                S::Slow,
                StatusDef {
                    max_stacks: 100,
                    duration: 5.0,
                    move_speed_mult: 0.6,
                    threshold: Some((100, ThresholdAction::Become(S::Frozen))),
                    ..d()
                },
            ),
            (
                S::Frozen,
                StatusDef {
                    duration: 3.0,
                    on_damaged: Some(TriggerKind::Shatter),
                    consume_on_trigger: true,
                    move_speed_mult: 0.0,
                    restrictions: Restrictions::ALL,
                    ..d()
                },
            ),
            (
                S::FrostArmor,
                StatusDef {
                    max_stacks: 5,
                    duration: 6.0,
                    incoming: vec![Modifier::resist("Frost Armor", ModifierScope::All, 0.05).per_stack()],
                    ..d()
                },
            ),
            (
                S::Sever,
                StatusDef {
                    duration: 5.0,
                    outgoing: vec![Modifier::multiplicative("Sever", ModifierScope::All, -0.4)],
                    ..d()
                },
            ),
            (S::Suspend, StatusDef { duration: 4.0, move_speed_mult: 0.0, restrictions: Restrictions::ALL, ..d() }),
            (
                S::Unravel,
                StatusDef {
                    duration: 6.0,
                    on_damaged: Some(TriggerKind::UnravelThreads),
                    trigger_cooldown: 0.5,
                    ..d()
                },
            ),
            (
                S::WovenMail,
                StatusDef {
                    duration: 8.0,
                    incoming: vec![Modifier::resist("Woven Mail", ModifierScope::All, 0.45)],
                    ..d()
                },
            ),
        ]);

        let triggers = HashMap::from([
            (
                TriggerKind::Ignite,
                TriggerDef {
                    damage: 300.0,
                    damage_type: DamageType::Solar,
                    radius: 5.0,
                    hits_origin: true,
                    max_targets: None,
                },
            ),
            (
                TriggerKind::JoltChain,
                TriggerDef {
                    damage: 40.0,
                    damage_type: DamageType::Arc,
                    radius: 8.0,
                    hits_origin: false,
                    max_targets: Some(3),
                },
            ),
            (
                TriggerKind::VolatileExplosion,
                TriggerDef {
                    damage: 120.0,
                    damage_type: DamageType::Void,
                    radius: 5.0,
                    hits_origin: true,
                    max_targets: None,
                },
            ),
            (
                TriggerKind::Shatter,
                TriggerDef {
                    damage: 150.0,
                    damage_type: DamageType::Stasis,
                    radius: 4.0,
                    hits_origin: true,
                    max_targets: None,
                },
            ),
            (
                TriggerKind::UnravelThreads,
                TriggerDef {
                    damage: 30.0,
                    damage_type: DamageType::Strand,
                    radius: 10.0,
                    hits_origin: false,
                    max_targets: Some(3),
                },
            ),
        ]);

        Self { statuses, triggers }
    }

    /// PvP tuning: weaker debuffs and damage reduction, smaller explosions
    /// relative to guardian health. Approximations: tune to taste.
    pub fn pvp() -> Self {
        let mut r = Self::pve();
        let set_incoming = |r: &mut Self, k: StatusKind, v: f32| {
            if let Some(m) = r.statuses.get_mut(&k).and_then(|d| d.incoming.first_mut()) {
                m.value = v;
            }
        };
        set_incoming(&mut r, StatusKind::Weaken, 0.10);
        set_incoming(&mut r, StatusKind::WovenMail, 0.30);
        set_incoming(&mut r, StatusKind::FrostArmor, 0.025);
        if let Some(m) = r.statuses.get_mut(&StatusKind::Radiant).and_then(|d| d.outgoing.first_mut()) {
            m.value = 0.20;
        }
        if let Some(m) = r.statuses.get_mut(&StatusKind::Sever).and_then(|d| d.outgoing.first_mut()) {
            m.value = -0.15;
        }
        if let Some(d) = r.statuses.get_mut(&StatusKind::Devour) {
            d.on_kill_heal = 100.0;
        }
        for (k, dmg) in [
            (TriggerKind::Ignite, 120.0),
            (TriggerKind::JoltChain, 25.0),
            (TriggerKind::VolatileExplosion, 70.0),
            (TriggerKind::Shatter, 70.0),
            (TriggerKind::UnravelThreads, 15.0),
        ] {
            if let Some(t) = r.triggers.get_mut(&k) {
                t.damage = dmg;
            }
        }
        r
    }
}

impl Default for StatusRules {
    fn default() -> Self {
        Self::pve()
    }
}

/// A status currently on a combatant.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusInstance {
    pub kind: StatusKind,
    pub stacks: u32,
    pub remaining: f32,
    /// Who applied it; DoT and trigger damage is credited to them.
    pub source: Option<CombatantId>,
    trigger_cooldown: f32,
    decay_accum: f32,
}

/// Something a status did that the sandbox has to resolve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusOutcome {
    Trigger { kind: TriggerKind, source: Option<CombatantId> },
    Became { kind: StatusKind, source: Option<CombatantId> },
}

/// Results of advancing statuses by one tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusTick {
    /// `(source, status, damage type, amount)` damage over time to deal.
    pub dots: Vec<(Option<CombatantId>, StatusKind, DamageType, f32)>,
    pub heal: f32,
    pub expired: Vec<StatusKind>,
}

/// All statuses on one combatant.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusSet {
    effects: Vec<StatusInstance>,
}

impl StatusSet {
    pub fn iter(&self) -> impl Iterator<Item = &StatusInstance> {
        self.effects.iter()
    }

    pub fn get(&self, kind: StatusKind) -> Option<&StatusInstance> {
        self.effects.iter().find(|e| e.kind == kind)
    }

    pub fn has(&self, kind: StatusKind) -> bool {
        self.get(kind).is_some()
    }

    pub fn stacks(&self, kind: StatusKind) -> u32 {
        self.get(kind).map_or(0, |e| e.stacks)
    }

    pub fn remove(&mut self, kind: StatusKind) -> bool {
        let before = self.effects.len();
        self.effects.retain(|e| e.kind != kind);
        before != self.effects.len()
    }

    pub fn clear(&mut self) {
        self.effects.clear();
    }

    /// Adds stacks (capped) and refreshes duration. Returns what happened if
    /// the stack threshold was reached; the status is removed in that case.
    pub fn apply(
        &mut self,
        kind: StatusKind,
        stacks: u32,
        source: Option<CombatantId>,
        rules: &StatusRules,
    ) -> Option<StatusOutcome> {
        let def = rules.def(kind);
        let idx = match self.effects.iter().position(|e| e.kind == kind) {
            Some(i) => i,
            None => {
                self.effects.push(StatusInstance {
                    kind,
                    stacks: 0,
                    remaining: 0.0,
                    source,
                    trigger_cooldown: 0.0,
                    decay_accum: 0.0,
                });
                self.effects.len() - 1
            }
        };
        let e = &mut self.effects[idx];
        e.stacks = (e.stacks + stacks.max(1)).min(def.max_stacks);
        e.remaining = def.duration;
        if source.is_some() {
            e.source = source;
        }
        let (stacks_now, src) = (e.stacks, e.source);

        match def.threshold {
            Some((at, action)) if stacks_now >= at => {
                self.effects.swap_remove(idx);
                Some(match action {
                    ThresholdAction::Trigger(kind) => StatusOutcome::Trigger { kind, source: src },
                    ThresholdAction::Become(kind) => StatusOutcome::Became { kind, source: src },
                })
            }
            _ => None,
        }
    }

    /// Called when the holder takes damage. Returns triggers to fire.
    pub fn on_damaged(&mut self, rules: &StatusRules) -> Vec<StatusOutcome> {
        let mut out = Vec::new();
        self.effects.retain_mut(|e| {
            let def = rules.def(e.kind);
            let Some(trigger) = def.on_damaged else { return true };
            if e.trigger_cooldown > 0.0 {
                return true;
            }
            out.push(StatusOutcome::Trigger { kind: trigger, source: e.source });
            e.trigger_cooldown = def.trigger_cooldown;
            !def.consume_on_trigger
        });
        out
    }

    /// Called when the holder gets a kill. Returns health to restore.
    pub fn on_kill(&mut self, rules: &StatusRules) -> f32 {
        let mut heal = 0.0;
        for e in &mut self.effects {
            let def = rules.def(e.kind);
            heal += def.on_kill_heal;
            if def.refresh_on_kill {
                e.remaining = def.duration;
            }
        }
        heal
    }

    pub fn tick(&mut self, dt: f32, rules: &StatusRules) -> StatusTick {
        let mut out = StatusTick::default();
        self.effects.retain_mut(|e| {
            let def = rules.def(e.kind);
            if def.dot_per_stack > 0.0 {
                out.dots.push((e.source, e.kind, def.dot_type, def.dot_per_stack * e.stacks as f32 * dt));
            }
            out.heal += def.heal_per_stack * e.stacks as f32 * dt;
            e.trigger_cooldown = (e.trigger_cooldown - dt).max(0.0);
            if def.stack_decay_per_sec > 0.0 {
                e.decay_accum += def.stack_decay_per_sec * dt;
                let lost = e.decay_accum.floor();
                e.decay_accum -= lost;
                e.stacks = e.stacks.saturating_sub(lost as u32);
            }
            e.remaining -= dt;
            let alive = e.remaining > 0.0 && e.stacks > 0;
            if !alive {
                out.expired.push(e.kind);
            }
            alive
        });
        out
    }

    /// Modifiers on damage the holder deals, paired with their stack counts.
    pub fn outgoing<'a>(&'a self, rules: &'a StatusRules) -> impl Iterator<Item = (&'a Modifier, u32)> + 'a {
        self.effects.iter().flat_map(move |e| rules.def(e.kind).outgoing.iter().map(move |m| (m, e.stacks)))
    }

    /// Modifiers on damage the holder takes, paired with their stack counts.
    pub fn incoming<'a>(&'a self, rules: &'a StatusRules) -> impl Iterator<Item = (&'a Modifier, u32)> + 'a {
        self.effects.iter().flat_map(move |e| rules.def(e.kind).incoming.iter().map(move |m| (m, e.stacks)))
    }

    /// Combined movement speed multiplier (multiplicative).
    pub fn move_speed_mult(&self, rules: &StatusRules) -> f32 {
        self.effects.iter().map(|e| rules.def(e.kind).move_speed_mult).product()
    }

    pub fn restrictions(&self, rules: &StatusRules) -> Restrictions {
        self.effects.iter().fold(Restrictions::NONE, |acc, e| acc.union(rules.def(e.kind).restrictions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_and_trigger_has_a_definition() {
        for rules in [StatusRules::pve(), StatusRules::pvp()] {
            for k in StatusKind::ALL {
                rules.def(k);
            }
            for t in [
                TriggerKind::Ignite,
                TriggerKind::JoltChain,
                TriggerKind::VolatileExplosion,
                TriggerKind::Shatter,
                TriggerKind::UnravelThreads,
            ] {
                rules.trigger(t);
            }
        }
    }

    #[test]
    fn scorch_ignites_at_threshold() {
        let rules = StatusRules::pve();
        let mut s = StatusSet::default();
        assert_eq!(s.apply(StatusKind::Scorch, 60, None, &rules), None);
        let out = s.apply(StatusKind::Scorch, 60, Some(CombatantId(7)), &rules);
        assert_eq!(out, Some(StatusOutcome::Trigger { kind: TriggerKind::Ignite, source: Some(CombatantId(7)) }));
        assert!(!s.has(StatusKind::Scorch));
    }

    #[test]
    fn slow_becomes_frozen() {
        let rules = StatusRules::pve();
        let mut s = StatusSet::default();
        let out = s.apply(StatusKind::Slow, 100, None, &rules);
        assert_eq!(out, Some(StatusOutcome::Became { kind: StatusKind::Frozen, source: None }));
    }

    #[test]
    fn volatile_is_consumed_and_jolt_respects_cooldown() {
        let rules = StatusRules::pve();
        let mut s = StatusSet::default();
        s.apply(StatusKind::Volatile, 1, None, &rules);
        s.apply(StatusKind::Jolt, 1, None, &rules);
        assert_eq!(s.on_damaged(&rules).len(), 2);
        assert!(!s.has(StatusKind::Volatile));
        assert!(s.has(StatusKind::Jolt));
        assert!(s.on_damaged(&rules).is_empty(), "jolt is on cooldown");
        s.tick(1.0, &rules);
        assert_eq!(s.on_damaged(&rules).len(), 1);
    }

    #[test]
    fn scorch_burns_and_decays_then_expires() {
        let rules = StatusRules::pve();
        let mut s = StatusSet::default();
        s.apply(StatusKind::Scorch, 20, None, &rules);
        let t = s.tick(1.0, &rules);
        assert_eq!(t.dots.len(), 1);
        assert!((t.dots[0].3 - 10.0).abs() < 1e-4);
        assert_eq!(s.stacks(StatusKind::Scorch), 15);
        let t = s.tick(10.0, &rules);
        assert_eq!(t.expired, vec![StatusKind::Scorch]);
    }

    #[test]
    fn restrictions_and_speed_combine() {
        let rules = StatusRules::pve();
        let mut s = StatusSet::default();
        s.apply(StatusKind::Suppress, 1, None, &rules);
        s.apply(StatusKind::Slow, 10, None, &rules);
        let r = s.restrictions(&rules);
        assert!(r.abilities && !r.weapons && !r.movement);
        assert!((s.move_speed_mult(&rules) - 0.6).abs() < 1e-5);
    }
}
