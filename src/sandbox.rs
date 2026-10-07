//! The runtime: owns combatants, resolves damage and chained effects, ticks
//! timers, and reports what happened as [`CombatEvent`]s.
//!
//! A host game typically:
//! 1. `spawn`s combatants and keeps their `position` in sync each frame,
//! 2. calls `fire_weapon` / `use_ability` / `deal_damage` from its own
//!    hit detection,
//! 3. calls `tick(dt)` once per frame,
//! 4. drains `events()` to drive VFX, audio, UI and death handling.

use std::collections::{BTreeMap, VecDeque};

use crate::ability::AbilitySlot;
use crate::combatant::{ChampionKind, ChampionRules, ChampionState, Combatant, CombatantId, Rank, Team};
use crate::config::SandboxConfig;
use crate::damage::{self, DamageCalc, DamageInstance, SourceKind};
use crate::element::DamageType;
use crate::health::Absorption;
use crate::status::{StatusKind, StatusOutcome, TriggerKind};
use crate::weapon::FireError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrbId(pub u32);

/// An Orb of Power waiting to be picked up by `team`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Orb {
    pub id: OrbId,
    pub team: Team,
    pub position: Option<[f32; 3]>,
    pub remaining: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CombatEvent {
    Damaged {
        target: CombatantId,
        attacker: Option<CombatantId>,
        damage_type: DamageType,
        kind: SourceKind,
        precision: bool,
        calc: DamageCalc,
        absorbed: Absorption,
    },
    Immune {
        target: CombatantId,
        attacker: Option<CombatantId>,
    },
    ShieldBroken {
        target: CombatantId,
        element: Option<DamageType>,
        by: Option<CombatantId>,
    },
    Killed {
        victim: CombatantId,
        killer: Option<CombatantId>,
        kind: SourceKind,
    },
    Healed {
        target: CombatantId,
        amount: f32,
    },
    StatusApplied {
        target: CombatantId,
        kind: StatusKind,
        stacks: u32,
    },
    StatusExpired {
        target: CombatantId,
        kind: StatusKind,
    },
    Triggered {
        kind: TriggerKind,
        origin: CombatantId,
        source: Option<CombatantId>,
    },
    ChampionStunned {
        target: CombatantId,
        kind: ChampionKind,
    },
    BarrierRaised {
        target: CombatantId,
    },
    BarrierBroken {
        target: CombatantId,
    },
    AbilityUsed {
        user: CombatantId,
        slot: AbilitySlot,
    },
    AbilityReady {
        user: CombatantId,
        slot: AbilitySlot,
    },
    WeaponFired {
        user: CombatantId,
        weapon: String,
    },
    ReloadFinished {
        user: CombatantId,
    },
    OrbSpawned {
        orb: OrbId,
        position: Option<[f32; 3]>,
    },
    OrbCollected {
        orb: OrbId,
        by: CombatantId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    UnknownCombatant,
    Dead,
    /// A status (Suppress, Frozen, Suspend...) prevents this.
    Restricted,
    NoAbilityInSlot,
    NotReady,
    NoWeapon,
    Weapon(FireError),
    WrongTeam,
}

/// Where and how a shot landed, from the host's hit detection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shot {
    pub precision: bool,
    /// Overrides the distance computed from positions.
    pub distance: Option<f32>,
    /// Pellets that hit, for multi-pellet weapons. `None` = all of them.
    pub pellets_hit: Option<u32>,
}

impl Shot {
    pub fn body() -> Self {
        Self::default()
    }

    pub fn precision() -> Self {
        Self { precision: true, ..Self::default() }
    }

    pub fn at_distance(mut self, d: f32) -> Self {
        self.distance = Some(d);
        self
    }
}

enum Job {
    Damage {
        target: CombatantId,
        inst: DamageInstance,
        depth: u32,
    },
    Area {
        origin: CombatantId,
        radius: f32,
        include_origin: bool,
        max_targets: Option<u32>,
        inst: DamageInstance,
        depth: u32,
    },
}

pub struct Sandbox {
    pub config: SandboxConfig,
    combatants: BTreeMap<CombatantId, Combatant>,
    orbs: BTreeMap<OrbId, Orb>,
    next_id: u32,
    next_orb: u32,
    events: Vec<CombatEvent>,
    jobs: VecDeque<Job>,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            combatants: BTreeMap::new(),
            orbs: BTreeMap::new(),
            next_id: 1,
            next_orb: 1,
            events: Vec::new(),
            jobs: VecDeque::new(),
        }
    }

    // ---- combatants ---------------------------------------------------

    pub fn spawn(&mut self, mut c: Combatant) -> CombatantId {
        let id = CombatantId(self.next_id);
        self.next_id += 1;
        c.id = id;
        self.combatants.insert(id, c);
        id
    }

    pub fn despawn(&mut self, id: CombatantId) -> Option<Combatant> {
        self.combatants.remove(&id)
    }

    pub fn get(&self, id: CombatantId) -> Option<&Combatant> {
        self.combatants.get(&id)
    }

    pub fn get_mut(&mut self, id: CombatantId) -> Option<&mut Combatant> {
        self.combatants.get_mut(&id)
    }

    pub fn combatants(&self) -> impl Iterator<Item = &Combatant> {
        self.combatants.values()
    }

    pub fn set_position(&mut self, id: CombatantId, position: [f32; 3]) {
        if let Some(c) = self.combatants.get_mut(&id) {
            c.position = Some(position);
        }
    }

    pub fn orbs(&self) -> impl Iterator<Item = &Orb> {
        self.orbs.values()
    }

    /// Takes every event since the last call.
    pub fn events(&mut self) -> Vec<CombatEvent> {
        std::mem::take(&mut self.events)
    }

    // ---- actions ------------------------------------------------------

    /// Deals damage from any source (the host's own projectiles, hazards...).
    pub fn deal_damage(&mut self, target: CombatantId, inst: DamageInstance) {
        self.jobs.push_back(Job::Damage { target, inst, depth: 0 });
        self.process_jobs();
    }

    /// Applies a status from `source` (or the environment).
    pub fn apply_status(&mut self, target: CombatantId, kind: StatusKind, stacks: u32, source: Option<CombatantId>) {
        self.apply_status_inner(target, kind, stacks, source, 0);
        self.process_jobs();
    }

    pub fn heal(&mut self, target: CombatantId, amount: f32) -> f32 {
        let Some(c) = self.combatants.get_mut(&target) else { return 0.0 };
        let healed = c.health.heal(amount);
        if healed > 0.0 {
            self.events.push(CombatEvent::Healed { target, amount: healed });
        }
        healed
    }

    /// Fires `user`'s active weapon. `target` is what the host's hit
    /// detection says the shot hit (`None` for a miss).
    pub fn fire_weapon(
        &mut self,
        user: CombatantId,
        target: Option<CombatantId>,
        shot: Shot,
    ) -> Result<(), ActionError> {
        let rules = &self.config.statuses;
        let u = self.combatants.get_mut(&user).ok_or(ActionError::UnknownCombatant)?;
        if !u.is_alive() {
            return Err(ActionError::Dead);
        }
        if u.statuses.restrictions(rules).weapons {
            return Err(ActionError::Weapon(FireError::Restricted));
        }
        let w = u.weapon_mut().ok_or(ActionError::NoWeapon)?;
        w.fire().map_err(ActionError::Weapon)?;
        let def = w.def.clone();
        let user_pos = u.position;
        self.events.push(CombatEvent::WeaponFired { user, weapon: def.name.clone() });

        let Some(target) = target else { return Ok(()) };
        let target_pos = self.combatants.get(&target).and_then(|t| t.position);
        let distance = shot.distance.unwrap_or_else(|| match (user_pos, target_pos) {
            (Some(a), Some(b)) => (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt(),
            _ => 0.0,
        });
        let pellets = shot.pellets_hit.unwrap_or(def.pellets).min(def.pellets);
        let prec = if shot.precision { def.precision_mult } else { 1.0 };
        let direct = def.damage * prec * pellets as f32 * def.falloff.mult(distance);
        let kind = SourceKind::Weapon { ammo: def.ammo, archetype: def.archetype };
        let mut inst = DamageInstance::new(direct, def.damage_type, kind)
            .from(user)
            .precision(shot.precision)
            .anti_champion(def.anti_champion);
        inst.on_hit = def.on_hit.clone();

        if let Some(blast) = def.blast {
            inst.amount += blast.damage;
            let splash = DamageInstance::new(blast.damage, def.damage_type, kind).from(user);
            self.jobs.push_back(Job::Area {
                origin: target,
                radius: blast.radius,
                include_origin: false,
                max_targets: None,
                inst: splash,
                depth: 0,
            });
        }
        self.jobs.push_front(Job::Damage { target, inst, depth: 0 });
        self.process_jobs();
        Ok(())
    }

    /// Starts reloading the active weapon. Returns false if it can't.
    pub fn reload(&mut self, user: CombatantId) -> bool {
        let Some(u) = self.combatants.get_mut(&user) else { return false };
        let bonus = self.config.stats.derive(&u.stats).weapon_handling_bonus;
        u.weapon_mut().is_some_and(|w| w.reload(bonus))
    }

    /// Uses an ability. Damage hits `target` (and others within the
    /// ability's radius of it); self effects always apply.
    pub fn use_ability(
        &mut self,
        user: CombatantId,
        slot: AbilitySlot,
        target: Option<CombatantId>,
    ) -> Result<(), ActionError> {
        let rules = &self.config.statuses;
        let u = self.combatants.get_mut(&user).ok_or(ActionError::UnknownCombatant)?;
        if !u.is_alive() {
            return Err(ActionError::Dead);
        }
        if u.statuses.restrictions(rules).abilities {
            return Err(ActionError::Restricted);
        }
        let state = u.loadout.get_mut(slot).ok_or(ActionError::NoAbilityInSlot)?;
        if !state.consume() {
            return Err(ActionError::NotReady);
        }
        let def = state.def.clone();
        let derived = self.config.stats.derive(&u.stats);
        let mut overshield = def.overshield;
        if slot == AbilitySlot::ClassAbility {
            overshield += derived.class_overshield;
        }
        u.health.add_overshield(overshield);
        self.events.push(CombatEvent::AbilityUsed { user, slot });
        if def.heal > 0.0 {
            self.heal(user, def.heal);
        }
        for &(kind, stacks) in &def.on_self {
            self.apply_status_inner(user, kind, stacks, Some(user), 0);
        }

        if let Some(target) = target.filter(|_| def.damage > 0.0 || !def.on_hit.is_empty()) {
            let mut inst = DamageInstance::new(def.damage, def.damage_type, SourceKind::from_slot(slot)).from(user);
            inst.on_hit = def.on_hit.clone();
            if def.radius > 0.0 {
                self.jobs.push_back(Job::Area {
                    origin: target,
                    radius: def.radius,
                    include_origin: false,
                    max_targets: None,
                    inst: inst.clone(),
                    depth: 0,
                });
            }
            self.jobs.push_front(Job::Damage { target, inst, depth: 0 });
        }
        self.process_jobs();
        Ok(())
    }

    /// Drops an orb for `team` (normally done automatically on kills).
    pub fn spawn_orb(&mut self, team: Team, position: Option<[f32; 3]>) -> OrbId {
        let id = OrbId(self.next_orb);
        self.next_orb += 1;
        self.orbs.insert(id, Orb { id, team, position, remaining: self.config.orbs.lifetime });
        self.events.push(CombatEvent::OrbSpawned { orb: id, position });
        id
    }

    /// Picks up an orb: ability energy for every slot and Health-stat healing.
    pub fn collect_orb(&mut self, orb: OrbId, by: CombatantId) -> Result<(), ActionError> {
        let o = self.orbs.get(&orb).ok_or(ActionError::UnknownCombatant)?;
        let c = self.combatants.get_mut(&by).ok_or(ActionError::UnknownCombatant)?;
        if !c.is_alive() {
            return Err(ActionError::Dead);
        }
        if c.team != o.team {
            return Err(ActionError::WrongTeam);
        }
        self.orbs.remove(&orb);
        let r = self.config.orbs;
        let heal = self.config.stats.derive(&c.stats).orb_healing;
        for (slot, energy) in [
            (AbilitySlot::Grenade, r.grenade_energy),
            (AbilitySlot::Melee, r.melee_energy),
            (AbilitySlot::ClassAbility, r.class_energy),
            (AbilitySlot::Super, r.super_energy),
        ] {
            if let Some(a) = c.loadout.get_mut(slot) {
                if a.add_energy(energy) > 0 {
                    self.events.push(CombatEvent::AbilityReady { user: by, slot });
                }
            }
        }
        self.events.push(CombatEvent::OrbCollected { orb, by });
        self.heal(by, heal);
        Ok(())
    }

    /// Collects every friendly orb within `radius` of `by`. Returns how many.
    pub fn collect_orbs_near(&mut self, by: CombatantId, radius: f32) -> usize {
        let Some(c) = self.combatants.get(&by) else { return 0 };
        let Some(p) = c.position else { return 0 };
        let team = c.team;
        let ids: Vec<OrbId> = self
            .orbs
            .values()
            .filter(|o| o.team == team)
            .filter(|o| o.position.is_some_and(|q| dist(p, q) <= radius))
            .map(|o| o.id)
            .collect();
        ids.into_iter().filter(|&o| self.collect_orb(o, by).is_ok()).count()
    }

    // ---- time ---------------------------------------------------------

    pub fn tick(&mut self, dt: f32) {
        let ids: Vec<CombatantId> = self.combatants.keys().copied().collect();
        for id in ids {
            self.tick_combatant(id, dt);
        }
        self.orbs.retain(|_, o| {
            o.remaining -= dt;
            o.remaining > 0.0
        });
        self.process_jobs();
    }

    fn tick_combatant(&mut self, id: CombatantId, dt: f32) {
        let cfg = &self.config;
        let Some(c) = self.combatants.get_mut(&id) else { return };
        if !c.is_alive() {
            return;
        }
        c.health.tick(dt);

        let st = c.statuses.tick(dt, &cfg.statuses);
        for (source, status, damage_type, amount) in st.dots {
            let mut inst = DamageInstance::new(amount, damage_type, SourceKind::DamageOverTime(status));
            inst.attacker = source;
            self.jobs.push_back(Job::Damage { target: id, inst, depth: 0 });
        }
        for kind in st.expired {
            self.events.push(CombatEvent::StatusExpired { target: id, kind });
        }
        let healed = c.health.heal(st.heal);
        if healed > 0.0 {
            self.events.push(CombatEvent::Healed { target: id, amount: healed });
        }

        c.modifiers.retain_mut(|m| match &mut m.remaining {
            None => true,
            Some(t) => {
                *t -= dt;
                *t > 0.0
            }
        });

        let d = cfg.stats.derive(&c.stats);
        for (slot, mult) in [
            (AbilitySlot::Grenade, d.grenade_regen),
            (AbilitySlot::Melee, d.melee_regen),
            (AbilitySlot::ClassAbility, d.class_regen),
            (AbilitySlot::Super, d.super_regen),
        ] {
            if let Some(a) = c.loadout.get_mut(slot) {
                if a.tick(dt, mult) > 0 {
                    self.events.push(CombatEvent::AbilityReady { user: id, slot });
                }
            }
        }

        for w in &mut c.weapons {
            if w.tick(dt) {
                self.events.push(CombatEvent::ReloadFinished { user: id });
            }
        }

        if let Some(ch) = &mut c.champion {
            ch.tick(dt);
            if ch.kind == ChampionKind::Overload && !ch.is_stunned() {
                let amount = c.health.max_health * cfg.champions.overload_regen * dt;
                c.health.heal(amount);
            }
        }
    }

    // ---- internals ----------------------------------------------------

    fn process_jobs(&mut self) {
        while let Some(job) = self.jobs.pop_front() {
            match job {
                Job::Damage { target, inst, depth } => self.resolve_damage(target, inst, depth),
                Job::Area { origin, radius, include_origin, max_targets, inst, depth } => {
                    for t in self.area_targets(origin, radius, include_origin, max_targets, inst.attacker) {
                        self.jobs.push_back(Job::Damage { target: t, inst: inst.clone(), depth });
                    }
                }
            }
        }
    }

    /// Living combatants an area effect around `origin` hits, nearest first.
    /// Without an attacker the effect hits `origin`'s own team (e.g. an
    /// environmental explosion among enemies).
    fn area_targets(
        &self,
        origin: CombatantId,
        radius: f32,
        include_origin: bool,
        max_targets: Option<u32>,
        attacker: Option<CombatantId>,
    ) -> Vec<CombatantId> {
        let Some(o) = self.combatants.get(&origin) else { return Vec::new() };
        let attacker_team = attacker.and_then(|a| self.combatants.get(&a)).map(|a| a.team);
        let mut out = Vec::new();
        if include_origin && o.is_alive() {
            out.push(origin);
        }
        let Some(center) = o.position else { return out };
        let mut near: Vec<(f32, CombatantId)> = self
            .combatants
            .values()
            .filter(|c| c.id != origin && Some(c.id) != attacker && c.is_alive())
            .filter(|c| match attacker_team {
                Some(t) => c.team != t || self.config.friendly_fire,
                None => c.team == o.team,
            })
            .filter_map(|c| c.position.map(|p| (dist(center, p), c.id)))
            .filter(|&(d, _)| d <= radius)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let limit = max_targets.map_or(usize::MAX, |m| m as usize);
        out.extend(near.into_iter().take(limit).map(|(_, id)| id));
        out
    }

    fn resolve_damage(&mut self, target_id: CombatantId, inst: DamageInstance, depth: u32) {
        let cfg = &self.config;
        let Some(target) = self.combatants.get(&target_id) else { return };
        if !target.is_alive() || inst.attacker == Some(target_id) {
            return;
        }
        let attacker = inst.attacker.and_then(|a| self.combatants.get(&a));
        if let Some(a) = attacker {
            if a.team == target.team && !cfg.friendly_fire {
                return;
            }
        }
        let calc = damage::calculate(&inst, attacker, target, cfg);

        let target = self.combatants.get_mut(&target_id).expect("checked above");
        if let Some(ch) = &mut target.champion {
            if inst.anti_champion == Some(ch.kind) {
                counter_champion(&mut self.events, target_id, ch, &cfg.champions);
            }
            if ch.barrier_up {
                self.events.push(CombatEvent::Immune { target: target_id, attacker: inst.attacker });
                return;
            }
        }

        let shield_element = target.health.shield_element;
        let max_shield = target.health.max_shield;
        let absorbed = target.health.absorb(calc.final_amount, inst.damage_type, &cfg.shields);
        self.events.push(CombatEvent::Damaged {
            target: target_id,
            attacker: inst.attacker,
            damage_type: inst.damage_type,
            kind: inst.kind,
            precision: inst.precision,
            calc,
            absorbed,
        });

        if absorbed.shield_broken {
            self.events.push(CombatEvent::ShieldBroken {
                target: target_id,
                element: shield_element,
                by: inst.attacker,
            });
            if shield_element == Some(inst.damage_type) && depth < cfg.max_chain_depth {
                let mut boom = DamageInstance::new(
                    max_shield * cfg.shields.break_explosion_fraction,
                    inst.damage_type,
                    SourceKind::ShieldBreak,
                );
                boom.attacker = inst.attacker;
                self.jobs.push_back(Job::Area {
                    origin: target_id,
                    radius: cfg.shields.break_explosion_radius,
                    include_origin: false,
                    max_targets: None,
                    inst: boom,
                    depth: depth + 1,
                });
            }
        }

        // Barrier champions raise their barrier as health crosses thresholds.
        if let Some(ch) = &mut target.champion {
            if ch.kind == ChampionKind::Barrier && !absorbed.killed {
                let frac = target.health.health / target.health.max_health.max(f32::EPSILON);
                if let Some(&t) = cfg.champions.barrier_thresholds.get(ch.next_barrier) {
                    if frac <= t {
                        ch.next_barrier += 1;
                        ch.barrier_up = true;
                        self.events.push(CombatEvent::BarrierRaised { target: target_id });
                    }
                }
            }
        }

        // Super energy for the attacker.
        if let Some(a) = inst.attacker {
            if inst.kind != SourceKind::Super {
                self.grant_super_energy(a, absorbed.total() * cfg.energy.super_per_damage);
            }
        }

        if absorbed.killed {
            self.on_kill(inst.attacker, target_id, inst.kind);
            return;
        }

        // Statuses already on the target react to the hit, then the hit's
        // own statuses land (so a Volatile-applying hit doesn't detonate itself).
        if !matches!(inst.kind, SourceKind::DamageOverTime(_)) {
            let target = self.combatants.get_mut(&target_id).expect("still alive");
            for outcome in target.statuses.on_damaged(&self.config.statuses) {
                self.handle_outcome(target_id, outcome, depth);
            }
        }
        for (kind, stacks) in inst.on_hit {
            self.apply_status_inner(target_id, kind, stacks, inst.attacker, depth);
        }
    }

    fn grant_super_energy(&mut self, who: CombatantId, amount: f32) {
        let Some(s) = self.combatants.get_mut(&who).and_then(|c| c.loadout.super_.as_mut()) else { return };
        if s.add_energy(amount) > 0 {
            self.events.push(CombatEvent::AbilityReady { user: who, slot: AbilitySlot::Super });
        }
    }

    fn on_kill(&mut self, killer: Option<CombatantId>, victim: CombatantId, kind: SourceKind) {
        self.events.push(CombatEvent::Killed { victim, killer, kind });
        let (victim_rank, victim_pos) = match self.combatants.get_mut(&victim) {
            Some(v) => {
                v.statuses.clear();
                (v.rank, v.position)
            }
            None => return,
        };
        let Some(k) = killer else { return };
        let Some(kc) = self.combatants.get_mut(&k) else { return };
        let heal = kc.statuses.on_kill(&self.config.statuses);
        let team = kc.team;
        if heal > 0.0 {
            self.heal(k, heal);
        }
        self.grant_super_energy(k, self.config.energy.super_per_kill);

        let orbs = &self.config.orbs;
        let tough = !matches!(victim_rank, Rank::Minor | Rank::Guardian);
        if (orbs.on_super_kill && kind == SourceKind::Super) || (orbs.on_major_kill && tough) {
            self.spawn_orb(team, victim_pos);
        }
    }

    fn apply_status_inner(
        &mut self,
        target_id: CombatantId,
        kind: StatusKind,
        stacks: u32,
        source: Option<CombatantId>,
        depth: u32,
    ) {
        let Some(t) = self.combatants.get_mut(&target_id) else { return };
        if !t.is_alive() {
            return;
        }
        let outcome = t.statuses.apply(kind, stacks, source, &self.config.statuses);
        self.events.push(CombatEvent::StatusApplied { target: target_id, kind, stacks });

        if let Some(ch) = &mut t.champion {
            let stuns = self.config.champions.stunned_by.get(&ch.kind).is_some_and(|v| v.contains(&kind));
            if stuns {
                counter_champion(&mut self.events, target_id, ch, &self.config.champions);
            }
        }
        if let Some(o) = outcome {
            self.handle_outcome(target_id, o, depth);
        }
    }

    fn handle_outcome(&mut self, origin: CombatantId, outcome: StatusOutcome, depth: u32) {
        match outcome {
            StatusOutcome::Became { kind, source } => self.apply_status_inner(origin, kind, 1, source, depth),
            StatusOutcome::Trigger { kind, source } => {
                self.events.push(CombatEvent::Triggered { kind, origin, source });
                if depth >= self.config.max_chain_depth {
                    return;
                }
                let def = *self.config.statuses.trigger(kind);
                let mut inst = DamageInstance::new(def.damage, def.damage_type, SourceKind::Trigger(kind));
                inst.attacker = source;
                self.jobs.push_back(Job::Area {
                    origin,
                    radius: def.radius,
                    include_origin: def.hits_origin,
                    max_targets: def.max_targets,
                    inst,
                    depth: depth + 1,
                });
            }
        }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(SandboxConfig::default())
    }
}

/// A champion was hit by its counter: the barrier always breaks, and it is
/// stunned unless already stunned or recently recovered.
fn counter_champion(events: &mut Vec<CombatEvent>, target: CombatantId, ch: &mut ChampionState, rules: &ChampionRules) {
    if ch.barrier_up {
        ch.barrier_up = false;
        events.push(CombatEvent::BarrierBroken { target });
    }
    if ch.stun(rules) {
        events.push(CombatEvent::ChampionStunned { target, kind: ch.kind });
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}
