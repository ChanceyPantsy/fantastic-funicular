//! The runtime: owns combatants, resolves damage and chained effects, runs
//! ability effects, zones, roaming supers and aspect passives, ticks
//! timers, and reports what happened as [`CombatEvent`]s.
//!
//! A host game typically:
//! 1. `spawn`s combatants and keeps their `position` in sync each frame,
//! 2. calls `fire_weapon` / `use_ability` / `super_attack` / `deal_damage`
//!    from its own input and hit detection,
//! 3. calls `tick(dt)` once per frame,
//! 4. drains `events()` to drive movement requests, VFX, audio, UI and
//!    death handling.

use std::collections::{BTreeMap, VecDeque};

use crate::ability::AbilitySlot;
use crate::buffs::{Modifier, ModifierScope};
use crate::combatant::{ChampionKind, ChampionRules, ChampionState, Combatant, CombatantId, Rank, Team};
use crate::config::SandboxConfig;
use crate::damage::{self, DamageCalc, DamageInstance, SourceKind};
use crate::effect::{ActiveSuper, Anchor, Effect, MoveKind, PassiveTrigger, ZoneDef};
use crate::element::DamageType;
use crate::health::Absorption;
use crate::status::{StatusKind, StatusOutcome, TriggerKind};
use crate::weapon::FireError;

/// Name of the damage-resist modifier a roaming super grants.
const SUPER_RESIST: &str = "Super damage resistance";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ZoneId(pub u32);

/// A lingering area created by an ability.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Zone {
    pub id: ZoneId,
    /// The ability that created it.
    pub name: String,
    pub owner: CombatantId,
    pub team: Team,
    pub damage_type: DamageType,
    pub kind: SourceKind,
    pub center: Option<[f32; 3]>,
    pub def: ZoneDef,
    pub remaining: f32,
    next_pulse: f32,
}

/// Where an ability is aimed. Build it from a combatant, a point, or
/// `None` (self-cast).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AbilityTarget {
    pub combatant: Option<CombatantId>,
    pub point: Option<[f32; 3]>,
}

impl From<Option<CombatantId>> for AbilityTarget {
    fn from(c: Option<CombatantId>) -> Self {
        Self { combatant: c, point: None }
    }
}

impl From<CombatantId> for AbilityTarget {
    fn from(c: CombatantId) -> Self {
        Self { combatant: Some(c), point: None }
    }
}

impl From<[f32; 3]> for AbilityTarget {
    fn from(p: [f32; 3]) -> Self {
        Self { combatant: None, point: Some(p) }
    }
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
        name: String,
    },
    AbilityReady {
        user: CombatantId,
        slot: AbilitySlot,
    },
    SuperStarted {
        user: CombatantId,
        name: String,
    },
    SuperEnded {
        user: CombatantId,
    },
    AspectTriggered {
        owner: CombatantId,
        aspect: String,
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
    ZoneCreated {
        zone: ZoneId,
    },
    ZoneExpired {
        zone: ZoneId,
    },
    /// An explosion, for VFX.
    Blast {
        center: [f32; 3],
        radius: f32,
        damage_type: DamageType,
    },
    /// A projectile or lightning arc between two points, for VFX.
    Beam {
        from: [f32; 3],
        to: [f32; 3],
        damage_type: DamageType,
    },
    /// The host should move `user` (toward `toward`, if given).
    Move {
        user: CombatantId,
        kind: MoveKind,
        distance: f32,
        toward: Option<[f32; 3]>,
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
    /// Weapons and abilities are unavailable during a roaming super.
    InSuper,
    NotInSuper,
    /// The active super has no heavy attack.
    NoHeavyAttack,
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

#[derive(Debug, Clone, Copy)]
enum Center {
    Combatant(CombatantId),
    Point([f32; 3]),
}

enum Job {
    Damage {
        target: CombatantId,
        inst: DamageInstance,
        depth: u32,
    },
    Area {
        center: Center,
        radius: f32,
        /// Hit the combatant at the center (for `Center::Combatant`).
        include_origin: bool,
        exclude: Option<CombatantId>,
        max_targets: Option<u32>,
        inst: DamageInstance,
        depth: u32,
    },
}

/// Who is running a list of effects, and at what.
#[derive(Debug, Clone)]
struct Ctx {
    user: CombatantId,
    name: String,
    kind: SourceKind,
    damage_type: DamageType,
    target: Option<CombatantId>,
    point: Option<[f32; 3]>,
    depth: u32,
}

pub struct Sandbox {
    pub config: SandboxConfig,
    combatants: BTreeMap<CombatantId, Combatant>,
    orbs: BTreeMap<OrbId, Orb>,
    zones: BTreeMap<ZoneId, Zone>,
    next_id: u32,
    next_orb: u32,
    next_zone: u32,
    events: Vec<CombatEvent>,
    jobs: VecDeque<Job>,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            combatants: BTreeMap::new(),
            orbs: BTreeMap::new(),
            zones: BTreeMap::new(),
            next_id: 1,
            next_orb: 1,
            next_zone: 1,
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

    pub fn zones(&self) -> impl Iterator<Item = &Zone> {
        self.zones.values()
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
        if u.in_super() {
            return Err(ActionError::InSuper);
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
            (Some(a), Some(b)) => dist(a, b),
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
            if let Some(p) = target_pos {
                self.events.push(CombatEvent::Blast { center: p, radius: blast.radius, damage_type: def.damage_type });
            }
            self.jobs.push_back(Job::Area {
                center: Center::Combatant(target),
                radius: blast.radius,
                include_origin: false,
                exclude: None,
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

    /// Casts the ability in `slot` at `target` (a combatant, a point, or
    /// `None` to self-cast). Runs its effects, then any aspect passives
    /// that react to the cast.
    pub fn use_ability(
        &mut self,
        user: CombatantId,
        slot: AbilitySlot,
        target: impl Into<AbilityTarget>,
    ) -> Result<(), ActionError> {
        let target = target.into();
        let rules = &self.config.statuses;
        let u = self.combatants.get_mut(&user).ok_or(ActionError::UnknownCombatant)?;
        if !u.is_alive() {
            return Err(ActionError::Dead);
        }
        if u.in_super() {
            return Err(ActionError::InSuper);
        }
        if u.statuses.restrictions(rules).abilities {
            return Err(ActionError::Restricted);
        }
        let state = u.loadout.get_mut(slot).ok_or(ActionError::NoAbilityInSlot)?;
        if !state.consume() {
            return Err(ActionError::NotReady);
        }
        let def = state.def.clone();
        if slot == AbilitySlot::ClassAbility {
            let bonus = self.config.stats.derive(&u.stats).class_overshield;
            u.health.add_overshield(bonus);
        }
        self.events.push(CombatEvent::AbilityUsed { user, slot, name: def.name.clone() });

        let ctx = Ctx {
            user,
            name: def.name.clone(),
            kind: SourceKind::from_slot(slot),
            damage_type: def.damage_type,
            target: target.combatant,
            point: self.resolve_point(user, target),
            depth: 0,
        };
        self.run_effects(&def.effects, &ctx);
        self.fire_passives(user, |t| *t == PassiveTrigger::Cast(slot), target.combatant, ctx.point, 0);
        self.process_jobs();
        Ok(())
    }

    /// A light (or heavy) attack during a roaming super.
    pub fn super_attack(
        &mut self,
        user: CombatantId,
        heavy: bool,
        target: impl Into<AbilityTarget>,
    ) -> Result<(), ActionError> {
        let target = target.into();
        let rules = &self.config.statuses;
        let u = self.combatants.get_mut(&user).ok_or(ActionError::UnknownCombatant)?;
        if !u.is_alive() {
            return Err(ActionError::Dead);
        }
        if u.statuses.restrictions(rules).abilities {
            return Err(ActionError::Restricted);
        }
        let m = u.super_mode.as_mut().ok_or(ActionError::NotInSuper)?;
        let (attack, cooldown) = if heavy {
            (m.def.heavy.clone().ok_or(ActionError::NoHeavyAttack)?, &mut m.heavy_cooldown)
        } else {
            (m.def.light.clone(), &mut m.light_cooldown)
        };
        if *cooldown > 0.0 {
            return Err(ActionError::NotReady);
        }
        *cooldown = attack.cooldown;
        let mut ends = false;
        if !heavy {
            if let Some(n) = &mut m.light_uses_left {
                *n = n.saturating_sub(1);
                ends = *n == 0;
            }
        }
        let ctx = Ctx {
            user,
            name: attack.name.clone(),
            kind: SourceKind::Super,
            damage_type: m.damage_type,
            target: target.combatant,
            point: None,
            depth: 0,
        };
        let ctx = Ctx { point: self.resolve_point(user, target), ..ctx };
        self.run_effects(&attack.effects, &ctx);
        if ends {
            self.end_super(user);
        }
        self.process_jobs();
        Ok(())
    }

    /// Ends `user`'s roaming super early.
    pub fn end_super(&mut self, user: CombatantId) {
        let Some(c) = self.combatants.get_mut(&user) else { return };
        if c.super_mode.take().is_some() {
            c.modifiers.retain(|m| m.name != SUPER_RESIST);
            self.events.push(CombatEvent::SuperEnded { user });
        }
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
        self.tick_zones(dt);
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

        for a in &mut c.aspects {
            for cd in &mut a.cooldowns {
                *cd = (*cd - dt).max(0.0);
            }
        }

        let mut super_over = false;
        if let Some(m) = &mut c.super_mode {
            m.remaining -= dt;
            m.light_cooldown = (m.light_cooldown - dt).max(0.0);
            m.heavy_cooldown = (m.heavy_cooldown - dt).max(0.0);
            super_over = m.remaining <= 0.0;
        }

        let d = cfg.stats.derive(&c.stats);
        let in_super = c.in_super();
        for (slot, mult) in [
            (AbilitySlot::Grenade, d.grenade_regen),
            (AbilitySlot::Melee, d.melee_regen),
            (AbilitySlot::ClassAbility, d.class_regen),
            (AbilitySlot::Super, d.super_regen),
        ] {
            if slot == AbilitySlot::Super && in_super {
                continue;
            }
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

        if super_over {
            self.end_super(id);
        }
    }

    fn tick_zones(&mut self, dt: f32) {
        let ids: Vec<ZoneId> = self.zones.keys().copied().collect();
        for id in ids {
            let Some(z) = self.zones.get_mut(&id) else { continue };
            let owner = self.combatants.get(&z.owner);
            if z.def.anchor == Anchor::Caster {
                match owner {
                    Some(o) if o.is_alive() => z.center = o.position,
                    _ => z.remaining = 0.0,
                }
            }
            z.next_pulse -= dt;
            let mut pulses = 0;
            while z.next_pulse <= 0.0 && z.remaining > 0.0 {
                z.next_pulse += z.def.tick.max(0.05);
                pulses += 1;
            }
            z.remaining -= dt;
            let expired = z.remaining <= 0.0;
            for _ in 0..pulses {
                self.pulse_zone(id);
            }
            if expired {
                self.zones.remove(&id);
                self.events.push(CombatEvent::ZoneExpired { zone: id });
            }
        }
    }

    fn pulse_zone(&mut self, id: ZoneId) {
        let Some(z) = self.zones.get(&id) else { return };
        let def = z.def.clone();
        let (owner, team, center, kind, damage_type) = (z.owner, z.team, z.center, z.kind, z.damage_type);

        if def.enemy_damage > 0.0 || !def.enemy_statuses.is_empty() {
            if let Some(p) = center {
                let mut inst = DamageInstance::new(def.enemy_damage, damage_type, kind).from(owner);
                inst.on_hit = def.enemy_statuses.clone();
                self.jobs.push_back(Job::Area {
                    center: Center::Point(p),
                    radius: def.radius,
                    include_origin: false,
                    exclude: None,
                    max_targets: None,
                    inst,
                    depth: 0,
                });
            }
        }

        let allies: Vec<CombatantId> = match center {
            Some(p) => self.allies_near(team, p, def.radius),
            None => vec![owner],
        };
        for a in allies {
            if def.ally_heal > 0.0 {
                self.heal(a, def.ally_heal);
            }
            if let Some(c) = self.combatants.get_mut(&a) {
                c.health.add_overshield(def.ally_overshield);
                for m in &def.ally_modifiers {
                    c.refresh_modifier(m.clone().timed(def.tick + 0.25));
                }
            }
            for &(k, s) in &def.ally_statuses {
                self.apply_status_inner(a, k, s, Some(owner), 0);
            }
        }
    }

    // ---- effects ------------------------------------------------------

    /// Where effects land: the given point, else the target's position,
    /// else the user's.
    fn resolve_point(&self, user: CombatantId, target: AbilityTarget) -> Option<[f32; 3]> {
        target
            .point
            .or_else(|| target.combatant.and_then(|t| self.combatants.get(&t)).and_then(|t| t.position))
            .or_else(|| self.combatants.get(&user).and_then(|u| u.position))
    }

    fn position(&self, id: CombatantId) -> Option<[f32; 3]> {
        self.combatants.get(&id).and_then(|c| c.position)
    }

    /// Living hostiles to `team` within `radius` of `center`, nearest first.
    fn hostiles_near(&self, team: Team, center: [f32; 3], radius: f32) -> Vec<CombatantId> {
        let mut near: Vec<(f32, CombatantId)> = self
            .combatants
            .values()
            .filter(|c| c.is_alive() && c.team != team)
            .filter_map(|c| c.position.map(|p| (dist(center, p), c.id)))
            .filter(|&(d, _)| d <= radius)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.into_iter().map(|(_, id)| id).collect()
    }

    /// Living members of `team` within `radius` of `center`.
    fn allies_near(&self, team: Team, center: [f32; 3], radius: f32) -> Vec<CombatantId> {
        self.combatants
            .values()
            .filter(|c| c.is_alive() && c.team == team)
            .filter(|c| c.position.is_some_and(|p| dist(center, p) <= radius))
            .map(|c| c.id)
            .collect()
    }

    fn run_effects(&mut self, effects: &[Effect], ctx: &Ctx) {
        let Some(user) = self.combatants.get(&ctx.user) else { return };
        let team = user.team;
        let user_pos = user.position;
        let hit = |amount: f32, statuses: &[(StatusKind, u32)]| {
            let mut inst = DamageInstance::new(amount, ctx.damage_type, ctx.kind).from(ctx.user);
            inst.on_hit = statuses.to_vec();
            inst
        };

        for effect in effects {
            match effect {
                Effect::Blast { damage, radius, statuses } => {
                    let inst = hit(*damage, statuses);
                    if let Some(t) = ctx.target {
                        self.jobs.push_back(Job::Damage { target: t, inst: inst.clone(), depth: ctx.depth });
                    }
                    if *radius > 0.0 {
                        if let Some(p) = ctx.point {
                            self.events.push(CombatEvent::Blast {
                                center: p,
                                radius: *radius,
                                damage_type: ctx.damage_type,
                            });
                            self.jobs.push_back(Job::Area {
                                center: Center::Point(p),
                                radius: *radius,
                                include_origin: false,
                                exclude: ctx.target,
                                max_targets: None,
                                inst,
                                depth: ctx.depth,
                            });
                        }
                    }
                }
                Effect::Zone(def) => self.spawn_zone(def.clone(), ctx, team),
                Effect::Seekers { count, damage, range, statuses } => {
                    let mut targets: Vec<CombatantId> = ctx.target.into_iter().collect();
                    if let Some(p) = ctx.point {
                        for id in self.hostiles_near(team, p, *range) {
                            if !targets.contains(&id) {
                                targets.push(id);
                            }
                        }
                    }
                    for t in targets.into_iter().take(*count as usize) {
                        if let (Some(from), Some(to)) = (user_pos, self.position(t)) {
                            self.events.push(CombatEvent::Beam { from, to, damage_type: ctx.damage_type });
                        }
                        self.jobs.push_back(Job::Damage { target: t, inst: hit(*damage, statuses), depth: ctx.depth });
                    }
                }
                Effect::Chain { damage, jumps, range, statuses } => {
                    let first = ctx
                        .target
                        .or_else(|| ctx.point.and_then(|p| self.hostiles_near(team, p, *range).first().copied()));
                    let Some(mut current) = first else { continue };
                    let mut from = user_pos;
                    let mut visited = vec![current];
                    loop {
                        let to = self.position(current);
                        if let (Some(a), Some(b)) = (from, to) {
                            self.events.push(CombatEvent::Beam { from: a, to: b, damage_type: ctx.damage_type });
                        }
                        self.jobs.push_back(Job::Damage {
                            target: current,
                            inst: hit(*damage, statuses),
                            depth: ctx.depth,
                        });
                        if visited.len() > *jumps as usize {
                            break;
                        }
                        let Some(here) = to else { break };
                        let next = self.hostiles_near(team, here, *range).into_iter().find(|id| !visited.contains(id));
                        let Some(next) = next else { break };
                        visited.push(next);
                        from = to;
                        current = next;
                    }
                }
                Effect::SelfStatus(kind, stacks) => {
                    self.apply_status_inner(ctx.user, *kind, *stacks, Some(ctx.user), ctx.depth)
                }
                Effect::AllyStatus { kind, stacks, radius } => {
                    let allies = match user_pos {
                        Some(p) => self.allies_near(team, p, *radius),
                        None => vec![ctx.user],
                    };
                    for a in allies {
                        self.apply_status_inner(a, *kind, *stacks, Some(ctx.user), ctx.depth);
                    }
                }
                Effect::Buff(m) => {
                    if let Some(c) = self.combatants.get_mut(&ctx.user) {
                        c.refresh_modifier(m.clone());
                    }
                }
                Effect::Overshield(amount) => {
                    if let Some(c) = self.combatants.get_mut(&ctx.user) {
                        c.health.add_overshield(*amount);
                    }
                }
                Effect::Heal(amount) => {
                    self.heal(ctx.user, *amount);
                }
                Effect::HealAllies { amount, radius } => {
                    let allies = match user_pos {
                        Some(p) => self.allies_near(team, p, *radius),
                        None => vec![ctx.user],
                    };
                    for a in allies {
                        self.heal(a, *amount);
                    }
                }
                Effect::Energy { slot, amount } => {
                    let Some(c) = self.combatants.get_mut(&ctx.user) else { continue };
                    if let Some(a) = c.loadout.get_mut(*slot) {
                        if a.add_energy(*amount) > 0 {
                            self.events.push(CombatEvent::AbilityReady { user: ctx.user, slot: *slot });
                        }
                    }
                }
                Effect::Reload => {
                    if let Some(w) = self.combatants.get_mut(&ctx.user).and_then(|c| c.weapon_mut()) {
                        w.instant_reload();
                    }
                }
                Effect::Move { kind, distance } => self.events.push(CombatEvent::Move {
                    user: ctx.user,
                    kind: *kind,
                    distance: *distance,
                    toward: ctx.point.filter(|&p| Some(p) != user_pos),
                }),
                Effect::Roam(r) => {
                    let Some(c) = self.combatants.get_mut(&ctx.user) else { continue };
                    c.super_mode = Some(ActiveSuper {
                        name: ctx.name.clone(),
                        damage_type: ctx.damage_type,
                        def: r.clone(),
                        remaining: r.duration,
                        light_cooldown: 0.0,
                        heavy_cooldown: 0.0,
                        light_uses_left: r.light_uses,
                    });
                    c.refresh_modifier(
                        Modifier::resist(SUPER_RESIST, ModifierScope::All, r.damage_resist).timed(r.duration),
                    );
                    self.events.push(CombatEvent::SuperStarted { user: ctx.user, name: ctx.name.clone() });
                }
            }
        }
    }

    fn spawn_zone(&mut self, def: ZoneDef, ctx: &Ctx, team: Team) {
        let center = match def.anchor {
            Anchor::Caster => self.position(ctx.user),
            Anchor::Point => ctx.point.or_else(|| self.position(ctx.user)),
        };
        let id = ZoneId(self.next_zone);
        self.next_zone += 1;
        self.zones.insert(
            id,
            Zone {
                id,
                name: ctx.name.clone(),
                owner: ctx.user,
                team,
                damage_type: ctx.damage_type,
                kind: ctx.kind,
                center,
                remaining: def.duration,
                def,
                next_pulse: 0.0,
            },
        );
        self.events.push(CombatEvent::ZoneCreated { zone: id });
        // First pulse lands immediately.
        if let Some(z) = self.zones.get_mut(&id) {
            z.next_pulse = z.def.tick.max(0.05);
        }
        self.pulse_zone(id);
    }

    /// Runs every ready passive of `owner`'s aspects whose trigger matches.
    fn fire_passives(
        &mut self,
        owner: CombatantId,
        matches: impl Fn(&PassiveTrigger) -> bool,
        target: Option<CombatantId>,
        point: Option<[f32; 3]>,
        depth: u32,
    ) {
        if depth >= self.config.max_chain_depth {
            return;
        }
        let Some(c) = self.combatants.get_mut(&owner) else { return };
        if !c.is_alive() {
            return;
        }
        let mut to_run = Vec::new();
        for a in &mut c.aspects {
            for (i, p) in a.def.passives.iter().enumerate() {
                if matches(&p.trigger) && a.cooldowns[i] <= 0.0 {
                    a.cooldowns[i] = p.cooldown;
                    to_run.push((a.def.name.clone(), a.def.damage_type, p.effects.clone()));
                }
            }
        }
        for (name, damage_type, effects) in to_run {
            self.events.push(CombatEvent::AspectTriggered { owner, aspect: name.clone() });
            let target = target.filter(|t| self.combatants.get(t).is_some_and(|c| c.is_alive()));
            let point = point.or_else(|| self.resolve_point(owner, target.into()));
            let ctx = Ctx { user: owner, name, kind: SourceKind::Aspect, damage_type, target, point, depth: depth + 1 };
            self.run_effects(&effects, &ctx);
        }
    }

    // ---- internals ----------------------------------------------------

    fn process_jobs(&mut self) {
        while let Some(job) = self.jobs.pop_front() {
            match job {
                Job::Damage { target, inst, depth } => self.resolve_damage(target, inst, depth),
                Job::Area { center, radius, include_origin, exclude, max_targets, inst, depth } => {
                    let targets = self.area_targets(center, radius, include_origin, max_targets, inst.attacker);
                    for t in targets.into_iter().filter(|&t| Some(t) != exclude) {
                        self.jobs.push_back(Job::Damage { target: t, inst: inst.clone(), depth });
                    }
                }
            }
        }
    }

    /// Living combatants an area effect hits, nearest first. With an
    /// attacker it hits the attacker's enemies. Without one, an effect
    /// centred on a combatant hits that combatant's team (an environmental
    /// explosion among enemies) and one centred on a point hits everyone.
    fn area_targets(
        &self,
        center: Center,
        radius: f32,
        include_origin: bool,
        max_targets: Option<u32>,
        attacker: Option<CombatantId>,
    ) -> Vec<CombatantId> {
        let attacker_team = attacker.and_then(|a| self.combatants.get(&a)).map(|a| a.team);
        let mut out = Vec::new();
        let (origin, origin_team, pos) = match center {
            Center::Combatant(id) => {
                let Some(o) = self.combatants.get(&id) else { return out };
                if include_origin && o.is_alive() {
                    out.push(id);
                }
                (Some(id), Some(o.team), o.position)
            }
            Center::Point(p) => (None, None, Some(p)),
        };
        let Some(pos) = pos else { return out };
        let mut near: Vec<(f32, CombatantId)> = self
            .combatants
            .values()
            .filter(|c| Some(c.id) != origin && Some(c.id) != attacker && c.is_alive())
            .filter(|c| match (attacker_team, origin_team) {
                (Some(t), _) => c.team != t || self.config.friendly_fire,
                (None, Some(t)) => c.team == t,
                (None, None) => true,
            })
            .filter_map(|c| c.position.map(|p| (dist(pos, p), c.id)))
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

        // Status-only hits (e.g. a slowing field) skip straight to statuses.
        if inst.amount <= 0.0 {
            for (kind, stacks) in inst.on_hit {
                self.apply_status_inner(target_id, kind, stacks, inst.attacker, depth);
            }
            return;
        }

        let shield_element = target.health.shield_element;
        let max_shield = target.health.max_shield;
        let absorbed = target.health.absorb(calc.final_amount, inst.damage_type, &cfg.shields);
        let target_pos = target.position;
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
                if let Some(p) = target_pos {
                    self.events.push(CombatEvent::Blast {
                        center: p,
                        radius: cfg.shields.break_explosion_radius,
                        damage_type: inst.damage_type,
                    });
                }
                self.jobs.push_back(Job::Area {
                    center: Center::Combatant(target_id),
                    radius: cfg.shields.break_explosion_radius,
                    include_origin: false,
                    exclude: None,
                    max_targets: None,
                    inst: boom,
                    depth: depth + 1,
                });
            }
        }

        // Barrier champions raise their barrier as health crosses thresholds.
        let target = self.combatants.get_mut(&target_id).expect("checked above");
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
            self.on_kill(inst.attacker, target_id, inst.kind, inst.precision, depth);
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
        let Some(c) = self.combatants.get_mut(&who) else { return };
        if c.in_super() {
            return;
        }
        let Some(s) = c.loadout.super_.as_mut() else { return };
        if s.add_energy(amount) > 0 {
            self.events.push(CombatEvent::AbilityReady { user: who, slot: AbilitySlot::Super });
        }
    }

    fn on_kill(
        &mut self,
        killer: Option<CombatantId>,
        victim: CombatantId,
        kind: SourceKind,
        precision: bool,
        depth: u32,
    ) {
        self.events.push(CombatEvent::Killed { victim, killer, kind });
        let (victim_rank, victim_pos, had) = match self.combatants.get_mut(&victim) {
            Some(v) => {
                let had: Vec<StatusKind> = v.statuses.iter().map(|e| e.kind).collect();
                v.statuses.clear();
                (v.rank, v.position, had)
            }
            None => return,
        };
        self.end_super(victim);
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

        self.fire_passives(
            k,
            |t| matches!(t, PassiveTrigger::Kill(f) if f.matches(&kind, precision, &had)),
            None,
            victim_pos,
            depth,
        );
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
        if let Some(src) = source.filter(|&s| s != target_id) {
            self.fire_passives(src, |t| *t == PassiveTrigger::Applies(kind), Some(target_id), None, depth);
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
                if let Some(p) = self.position(origin) {
                    if def.hits_origin || def.max_targets.is_none() {
                        self.events.push(CombatEvent::Blast {
                            center: p,
                            radius: def.radius,
                            damage_type: def.damage_type,
                        });
                    }
                }
                let mut inst = DamageInstance::new(def.damage, def.damage_type, SourceKind::Trigger(kind));
                inst.attacker = source;
                if def.max_targets.is_some() && !def.hits_origin {
                    // Chains and threads: draw a beam to each target.
                    let targets =
                        self.area_targets(Center::Combatant(origin), def.radius, false, def.max_targets, source);
                    let from = self.position(origin);
                    for t in targets {
                        if let (Some(a), Some(b)) = (from, self.position(t)) {
                            self.events.push(CombatEvent::Beam { from: a, to: b, damage_type: def.damage_type });
                        }
                        self.jobs.push_back(Job::Damage { target: t, inst: inst.clone(), depth: depth + 1 });
                    }
                } else {
                    self.jobs.push_back(Job::Area {
                        center: Center::Combatant(origin),
                        radius: def.radius,
                        include_origin: def.hits_origin,
                        exclude: None,
                        max_targets: def.max_targets,
                        inst,
                        depth: depth + 1,
                    });
                }
                if let Some(src) = source {
                    self.fire_passives(src, |t| *t == PassiveTrigger::Causes(kind), Some(origin), None, depth);
                }
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
