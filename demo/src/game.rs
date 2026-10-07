//! A small top-down arena built on `guardian_combat`.
//!
//! The library does the rules; this file does everything a host game has to:
//! movement, enemy AI, hit detection (ray casts against circles), enemy
//! projectiles, waves, ammo pickups, carrying out movement requests from
//! abilities, and turning sandbox events into effects for the renderer.

use std::collections::{BTreeMap, VecDeque};

use guardian_combat::ability::AbilitySlot;
use guardian_combat::catalog;
use guardian_combat::combatant::{ChampionKind, Combatant, CombatantId, Rank, Team};
use guardian_combat::config::SandboxConfig;
use guardian_combat::damage::{DamageInstance, SourceKind};
use guardian_combat::effect::MoveKind;
use guardian_combat::element::{DamageType, GuardianClass, SubclassElement};
use guardian_combat::sandbox::{AbilityTarget, ActionError, CombatEvent, Sandbox, Shot};
use guardian_combat::stats::{Curve, StatBlock};
use guardian_combat::status::{StatusKind, TriggerKind};
use guardian_combat::weapon::{presets, AmmoType, FireError, WeaponArchetype};
use serde::{Deserialize, Serialize};

pub const ARENA_W: f32 = 48.0;
pub const ARENA_H: f32 = 30.0;
const PLAYER_TEAM: Team = Team(0);
const ENEMY_TEAM: Team = Team(1);
const PLAYER_RADIUS: f32 = 0.5;
const PLAYER_SPEED: f32 = 7.0;
const PICKUP_RADIUS: f32 = 1.6;

/// The player's chosen class, subclass and abilities, by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadoutChoice {
    pub class: GuardianClass,
    pub element: SubclassElement,
    #[serde(rename = "super")]
    pub super_: String,
    pub grenade: String,
    pub melee: String,
    pub class_ability: String,
    pub aspects: Vec<String>,
}

impl LoadoutChoice {
    /// The first option in every slot and the first two aspects.
    pub fn default_for(class: GuardianClass, element: SubclassElement) -> Self {
        let kit = catalog::kit(class, element);
        Self {
            class,
            element,
            super_: kit.supers[0].name.clone(),
            grenade: kit.grenades[0].name.clone(),
            melee: kit.melees[0].name.clone(),
            class_ability: kit.class_abilities[0].name.clone(),
            aspects: kit.aspects.iter().take(2).map(|a| a.name.clone()).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Input {
    pub move_x: f32,
    pub move_y: f32,
    pub aim_x: f32,
    pub aim_y: f32,
    pub fire: bool,
    /// Aim at the nearest enemy and fire whenever one is in range (touch play).
    pub auto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Ability(AbilitySlot),
    HeavyAttack,
    Reload,
    Weapon(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EnemyKind {
    Thrall,
    Acolyte,
    Knight,
    Champion(ChampionKind),
    Ogre,
}

#[derive(Debug, Clone)]
struct Brain {
    kind: EnemyKind,
    radius: f32,
    speed: f32,
    /// Ranged enemies keep about this far away; 0 = melee rusher.
    keep_away: f32,
    attack_cooldown: f32,
    attack_timer: f32,
    damage: f32,
    element: DamageType,
    strafe: f32,
}

#[derive(Debug, Clone)]
struct Projectile {
    pos: [f32; 2],
    vel: [f32; 2],
    damage: f32,
    element: DamageType,
    owner: CombatantId,
    life: f32,
}

#[derive(Debug, Clone)]
struct AmmoBrick {
    pos: [f32; 2],
    ammo: AmmoType,
    life: f32,
}

/// A short-lived visual for the renderer.
#[derive(Debug, Clone, Serialize)]
pub struct Fx {
    pub kind: &'static str,
    pub x: f32,
    pub y: f32,
    pub x2: f32,
    pub y2: f32,
    pub r: f32,
    pub el: Option<DamageType>,
    pub text: String,
    pub crit: bool,
    pub age: f32,
    pub life: f32,
}

impl Fx {
    fn new(kind: &'static str, x: f32, y: f32, life: f32) -> Self {
        Self { kind, x, y, x2: x, y2: y, r: 0.0, el: None, text: String::new(), crit: false, age: 0.0, life }
    }
}

pub struct Game {
    sb: Sandbox,
    player: CombatantId,
    pub choice: LoadoutChoice,
    brains: BTreeMap<CombatantId, Brain>,
    projectiles: Vec<Projectile>,
    bricks: Vec<AmmoBrick>,
    fx: Vec<Fx>,
    log: VecDeque<String>,
    input: Input,
    aim: [f32; 2],
    rng: u32,
    time: f32,
    pub wave: u32,
    wave_delay: f32,
    pub kills: u32,
    pub game_over: bool,
    hint: Option<(String, f32)>,
}

fn v2(p: [f32; 3]) -> [f32; 2] {
    [p[0], p[1]]
}

fn v3(p: [f32; 2]) -> [f32; 3] {
    [p[0], p[1], 0.0]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

fn norm(a: [f32; 2]) -> [f32; 2] {
    let l = len(a);
    if l < 1e-6 {
        [0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l]
    }
}

fn clamp_arena(p: [f32; 2], r: f32) -> [f32; 2] {
    [p[0].clamp(r, ARENA_W - r), p[1].clamp(r, ARENA_H - r)]
}

fn element_of(e: SubclassElement) -> DamageType {
    e.damage_type().unwrap_or(DamageType::Void)
}

fn slot_name(s: AbilitySlot) -> &'static str {
    match s {
        AbilitySlot::Grenade => "Grenade",
        AbilitySlot::Melee => "Melee",
        AbilitySlot::ClassAbility => "Class ability",
        AbilitySlot::Super => "Super",
    }
}

fn demo_config() -> SandboxConfig {
    let mut cfg = SandboxConfig::pve();
    // Faster cooldowns than the real game so a short demo shows everything.
    cfg.stats.ability_regen = Curve::linear(0.0, 1.0, 100.0, 6.0);
    cfg.energy.super_per_damage = 1.0 / 5000.0;
    cfg.energy.super_per_kill = 0.02;
    cfg.orbs.lifetime = 20.0;
    cfg
}

impl Game {
    pub fn new(choice: LoadoutChoice) -> Result<Self, String> {
        let kit = catalog::kit(choice.class, choice.element);
        let find = |slot: AbilitySlot, name: &str| {
            kit.options(slot)
                .iter()
                .find(|a| a.name == name)
                .cloned()
                .ok_or_else(|| format!("{name:?} is not a {} for this subclass", slot_name(slot)))
        };
        let abilities = [
            find(AbilitySlot::Super, &choice.super_)?,
            find(AbilitySlot::Grenade, &choice.grenade)?,
            find(AbilitySlot::Melee, &choice.melee)?,
            find(AbilitySlot::ClassAbility, &choice.class_ability)?,
        ];
        if choice.aspects.len() > 2 {
            return Err("pick at most two aspects".into());
        }

        let mut sb = Sandbox::new(demo_config());
        let element = element_of(choice.element);
        let mut primary = presets::hand_cannon_140(DamageType::Kinetic);
        primary.name = "Hand Cannon".into();
        primary.anti_champion = Some(ChampionKind::Barrier);
        let mut special = presets::shotgun_55(element);
        special.name = "Shotgun".into();
        special.anti_champion = Some(ChampionKind::Overload);
        let mut heavy = presets::rocket_launcher(DamageType::Solar);
        heavy.name = "Rocket Launcher".into();
        heavy.anti_champion = Some(ChampionKind::Unstoppable);

        let mut p = Combatant::guardian(
            "Guardian",
            PLAYER_TEAM,
            choice.class,
            StatBlock::new(100, 100, 100, 100, 100, 100),
            &sb.config.guardian,
            &sb.config.stats,
        )
        .at([ARENA_W / 2.0, ARENA_H / 2.0, 0.0])
        .with_weapon(primary)
        .with_weapon(special)
        .with_weapon(heavy);
        for a in abilities {
            p = p.with_ability(a);
        }
        for name in &choice.aspects {
            let a = kit
                .aspects
                .iter()
                .find(|a| &a.name == name)
                .ok_or_else(|| format!("{name:?} is not an aspect for this subclass"))?;
            p = p.with_aspect(a.clone());
        }
        if let Some(s) = p.loadout.super_.as_mut() {
            s.add_energy(0.6);
        }
        let player = sb.spawn(p);

        let mut g = Self {
            sb,
            player,
            choice,
            brains: BTreeMap::new(),
            projectiles: Vec::new(),
            bricks: Vec::new(),
            fx: Vec::new(),
            log: VecDeque::new(),
            input: Input::default(),
            aim: [ARENA_W / 2.0 + 5.0, ARENA_H / 2.0],
            rng: 0x9E37_79B9,
            time: 0.0,
            wave: 0,
            wave_delay: 1.5,
            kills: 0,
            game_over: false,
            hint: None,
        };
        g.say(format!("{:?} {:?} ready", g.choice.element, g.choice.class));
        Ok(g)
    }

    // ---- helpers ------------------------------------------------------

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn say(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > 6 {
            self.log.pop_front();
        }
    }

    fn hint(&mut self, text: impl Into<String>) {
        self.hint = Some((text.into(), 1.5));
    }

    fn pos(&self, id: CombatantId) -> Option<[f32; 2]> {
        self.sb.get(id).and_then(|c| c.position).map(v2)
    }

    fn player_pos(&self) -> [f32; 2] {
        self.pos(self.player).unwrap_or([ARENA_W / 2.0, ARENA_H / 2.0])
    }

    fn set_pos(&mut self, id: CombatantId, p: [f32; 2], r: f32) {
        self.sb.set_position(id, v3(clamp_arena(p, r)));
    }

    fn alive_enemies(&self) -> impl Iterator<Item = (CombatantId, [f32; 2], f32)> + '_ {
        self.brains.iter().filter_map(|(&id, b)| {
            let c = self.sb.get(id).filter(|c| c.is_alive())?;
            Some((id, v2(c.position?), b.radius))
        })
    }

    fn nearest_enemy(&self, to: [f32; 2], within: f32) -> Option<(CombatantId, [f32; 2])> {
        self.alive_enemies()
            .map(|(id, p, _)| (id, p, len(sub(p, to))))
            .filter(|&(_, _, d)| d <= within)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(id, p, _)| (id, p))
    }

    /// First enemy hit by a ray: `(id, precision, distance)`.
    fn raycast(&self, from: [f32; 2], dir: [f32; 2], range: f32) -> Option<(CombatantId, bool, f32)> {
        self.alive_enemies()
            .filter_map(|(id, p, r)| {
                let rel = sub(p, from);
                let t = rel[0] * dir[0] + rel[1] * dir[1];
                if t < 0.0 || t > range + r {
                    return None;
                }
                let perp = len(sub(rel, [dir[0] * t, dir[1] * t]));
                (perp <= r).then_some((id, perp <= r * 0.4, t))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
    }

    fn player_in_super(&self) -> bool {
        self.sb.get(self.player).is_some_and(|c| c.in_super())
    }

    /// Where an ability or super attack with `range` should land.
    /// `None` means there is nothing to hit (a melee with no one close).
    fn target_for(&self, range: f32, close: bool) -> Option<AbilityTarget> {
        if range <= 0.0 {
            return Some(AbilityTarget::default());
        }
        let me = self.player_pos();
        if close {
            return self.nearest_enemy(me, range + 1.0).map(|(id, _)| id.into());
        }
        let near_aim = self.nearest_enemy(self.aim, 3.0).filter(|&(_, p)| len(sub(p, me)) <= range + 1.0);
        if let Some((id, _)) = near_aim {
            return Some(id.into());
        }
        let d = sub(self.aim, me);
        let l = len(d).min(range);
        let n = norm(d);
        Some(v3([me[0] + n[0] * l, me[1] + n[1] * l]).into())
    }

    // ---- input ----------------------------------------------------------

    pub fn set_input(&mut self, input: Input) {
        self.input = input;
        if !input.auto {
            self.aim = [input.aim_x, input.aim_y];
        }
    }

    pub fn command(&mut self, cmd: Command) {
        if self.game_over {
            return;
        }
        let p = self.player;
        match cmd {
            Command::Ability(slot) => {
                let Some(def) = self.sb.get(p).and_then(|c| c.loadout.get(slot)).map(|a| a.def.clone()) else {
                    return;
                };
                let close = slot == AbilitySlot::Melee && def.range <= 8.0;
                let Some(target) = self.target_for(def.range, close) else {
                    self.hint("No target in melee range");
                    return;
                };
                match self.sb.use_ability(p, slot, target) {
                    Ok(()) => {}
                    Err(ActionError::NotReady) => self.hint(format!("{} not ready", slot_name(slot))),
                    Err(ActionError::Restricted) => self.hint("Abilities suppressed!"),
                    Err(ActionError::InSuper) => self.hint("Can't use that during your super"),
                    Err(e) => self.hint(format!("{e:?}")),
                }
            }
            Command::HeavyAttack => {
                if !self.player_in_super() {
                    return;
                }
                self.super_attack(true);
            }
            Command::Reload => {
                self.sb.reload(p);
            }
            Command::Weapon(i) => {
                if let Some(c) = self.sb.get_mut(p) {
                    if i < c.weapons.len() {
                        c.active_weapon = i;
                    }
                }
            }
        }
    }

    fn super_attack(&mut self, heavy: bool) {
        let Some(m) = self.sb.get(self.player).and_then(|c| c.super_mode.clone()) else { return };
        let attack = if heavy { m.def.heavy.clone() } else { Some(m.def.light.clone()) };
        let Some(attack) = attack else { return };
        let close = attack.range > 0.0 && attack.range <= 8.0;
        let Some(target) = self.target_for(attack.range, close) else { return };
        let _ = self.sb.super_attack(self.player, heavy, target);
    }

    fn fire(&mut self) {
        if self.player_in_super() {
            self.super_attack(false);
            return;
        }
        let me = self.player_pos();
        let dir = norm(sub(self.aim, me));
        if dir == [0.0, 0.0] {
            return;
        }
        let Some(w) = self.sb.get(self.player).and_then(|c| c.weapon()).map(|w| (w.def.clone(), w.can_fire())) else {
            return;
        };
        let (def, ready) = w;
        match ready {
            Err(FireError::EmptyMagazine) => {
                self.sb.reload(self.player);
                return;
            }
            Err(_) => return,
            Ok(()) => {}
        }
        let range = if def.falloff.end.is_finite() { def.falloff.end * 1.3 } else { 60.0 };
        let hit = self.raycast(me, dir, range);
        let shot = Shot { precision: hit.is_some_and(|h| h.1), distance: hit.map(|h| h.2), pellets_hit: None };
        if self.sb.fire_weapon(self.player, hit.map(|h| h.0), shot).is_ok() {
            let reach = hit.map_or(range, |h| h.2);
            let mut fx = Fx::new("tracer", me[0], me[1], 0.08);
            fx.x2 = me[0] + dir[0] * reach;
            fx.y2 = me[1] + dir[1] * reach;
            fx.el = Some(def.damage_type);
            self.fx.push(fx);
        }
    }

    // ---- waves ------------------------------------------------------------

    fn spawn_enemy(&mut self, kind: EnemyKind) {
        let scale = 1.0 + 0.12 * self.wave.saturating_sub(1) as f32;
        let elements = [DamageType::Arc, DamageType::Solar, DamageType::Void, DamageType::Stasis, DamageType::Strand];
        let shield_el = elements[(self.rand() * elements.len() as f32) as usize % elements.len()];
        let (c, brain) = match kind {
            EnemyKind::Thrall => (
                Combatant::enemy("Thrall", ENEMY_TEAM, Rank::Minor, 150.0 * scale),
                Brain {
                    kind,
                    radius: 0.5,
                    speed: 5.5,
                    keep_away: 0.0,
                    attack_cooldown: 1.0,
                    attack_timer: 0.0,
                    damage: 20.0,
                    element: DamageType::Kinetic,
                    strafe: 1.0,
                },
            ),
            EnemyKind::Acolyte => (
                Combatant::enemy("Acolyte", ENEMY_TEAM, Rank::Minor, 260.0 * scale),
                Brain {
                    kind,
                    radius: 0.55,
                    speed: 3.5,
                    keep_away: 12.0,
                    attack_cooldown: 1.4,
                    attack_timer: 1.0,
                    damage: 10.0,
                    element: DamageType::Solar,
                    strafe: 1.0,
                },
            ),
            EnemyKind::Knight => (
                Combatant::enemy("Knight", ENEMY_TEAM, Rank::Major, 900.0 * scale)
                    .with_shield(300.0 * scale, Some(shield_el)),
                Brain {
                    kind,
                    radius: 0.75,
                    speed: 3.0,
                    keep_away: 9.0,
                    attack_cooldown: 2.0,
                    attack_timer: 1.5,
                    damage: 18.0,
                    element: DamageType::Void,
                    strafe: 1.0,
                },
            ),
            EnemyKind::Champion(ck) => {
                let name = match ck {
                    ChampionKind::Barrier => "Barrier Hobgoblin",
                    ChampionKind::Overload => "Overload Captain",
                    ChampionKind::Unstoppable => "Unstoppable Ogre",
                };
                let (speed, keep, cd, dmg) = match ck {
                    ChampionKind::Unstoppable => (4.5, 0.0, 1.2, 30.0),
                    ChampionKind::Overload => (3.0, 10.0, 0.6, 8.0),
                    ChampionKind::Barrier => (2.5, 14.0, 1.2, 15.0),
                };
                (
                    Combatant::enemy(name, ENEMY_TEAM, Rank::Major, 2600.0 * scale).as_champion(ck),
                    Brain {
                        kind,
                        radius: 0.85,
                        speed,
                        keep_away: keep,
                        attack_cooldown: cd,
                        attack_timer: 2.0,
                        damage: dmg,
                        element: DamageType::Arc,
                        strafe: 1.0,
                    },
                )
            }
            EnemyKind::Ogre => (
                Combatant::enemy("Ogre", ENEMY_TEAM, Rank::Boss, 9000.0 * scale)
                    .with_shield(1500.0 * scale, Some(shield_el)),
                Brain {
                    kind,
                    radius: 1.3,
                    speed: 2.0,
                    keep_away: 8.0,
                    attack_cooldown: 2.5,
                    attack_timer: 3.0,
                    damage: 30.0,
                    element: DamageType::Void,
                    strafe: 1.0,
                },
            ),
        };
        // Spawn on an edge, away from the player.
        let me = self.player_pos();
        let mut p = [0.0, 0.0];
        for _ in 0..10 {
            let t = self.rand();
            let edge = (self.rand() * 4.0) as u32;
            p = match edge {
                0 => [t * ARENA_W, 1.0],
                1 => [t * ARENA_W, ARENA_H - 1.0],
                2 => [1.0, t * ARENA_H],
                _ => [ARENA_W - 1.0, t * ARENA_H],
            };
            if len(sub(p, me)) > 12.0 {
                break;
            }
        }
        let strafe = if self.rand() < 0.5 { -1.0 } else { 1.0 };
        let id = self.sb.spawn(c.at(v3(p)));
        self.brains.insert(id, Brain { strafe, ..brain });
    }

    fn next_wave(&mut self) {
        self.wave += 1;
        let n = self.wave;
        for _ in 0..(3 + n).min(10) {
            self.spawn_enemy(EnemyKind::Thrall);
        }
        for _ in 0..(1 + n / 2).min(5) {
            self.spawn_enemy(EnemyKind::Acolyte);
        }
        for _ in 0..(n / 2).min(4) {
            self.spawn_enemy(EnemyKind::Knight);
        }
        if n >= 2 {
            let kinds = [ChampionKind::Barrier, ChampionKind::Overload, ChampionKind::Unstoppable];
            self.spawn_enemy(EnemyKind::Champion(kinds[(n as usize) % 3]));
        }
        if n.is_multiple_of(5) {
            self.spawn_enemy(EnemyKind::Ogre);
        }
        self.say(format!("Wave {n}"));
        let mut fx = Fx::new("banner", ARENA_W / 2.0, ARENA_H / 2.0, 1.5);
        fx.text = format!("WAVE {n}");
        self.fx.push(fx);
    }

    // ---- simulation -------------------------------------------------------

    pub fn tick(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        for f in &mut self.fx {
            f.age += dt;
        }
        self.fx.retain(|f| f.age < f.life);
        if let Some((_, t)) = &mut self.hint {
            *t -= dt;
            if *t <= 0.0 {
                self.hint = None;
            }
        }
        if self.game_over {
            return;
        }
        self.time += dt;

        self.move_player(dt);
        if self.input.auto {
            let me = self.player_pos();
            if let Some((_, p)) = self.nearest_enemy(me, 35.0) {
                self.aim = p;
                self.fire();
            }
        } else if self.input.fire {
            self.fire();
        }
        self.sb.collect_orbs_near(self.player, PICKUP_RADIUS);
        self.collect_bricks(dt);
        self.run_enemies(dt);
        self.move_projectiles(dt);
        self.sb.tick(dt);
        self.handle_events();
        self.separate();

        if self.brains.is_empty() {
            self.wave_delay -= dt;
            if self.wave_delay <= 0.0 {
                self.next_wave();
                self.wave_delay = 3.0;
            }
        }
        if !self.sb.get(self.player).is_some_and(|c| c.is_alive()) {
            self.game_over = true;
            self.say(format!("Fell on wave {} with {} kills", self.wave, self.kills));
        }
    }

    fn move_player(&mut self, dt: f32) {
        let Some(c) = self.sb.get(self.player) else { return };
        let rules = &self.sb.config.statuses;
        if c.statuses.restrictions(rules).movement {
            return;
        }
        let mut speed = PLAYER_SPEED * c.statuses.move_speed_mult(rules);
        if let Some(m) = &c.super_mode {
            speed *= m.def.move_speed_mult;
        }
        let dir = norm([self.input.move_x, self.input.move_y]);
        let me = self.player_pos();
        self.set_pos(self.player, [me[0] + dir[0] * speed * dt, me[1] + dir[1] * speed * dt], PLAYER_RADIUS);
    }

    fn collect_bricks(&mut self, dt: f32) {
        let me = self.player_pos();
        let mut picked = Vec::new();
        self.bricks.retain_mut(|b| {
            b.life -= dt;
            if len(sub(b.pos, me)) <= PICKUP_RADIUS {
                picked.push(b.ammo);
                return false;
            }
            b.life > 0.0
        });
        for ammo in picked {
            let Some(c) = self.sb.get_mut(self.player) else { return };
            for w in &mut c.weapons {
                if w.def.ammo == ammo {
                    let (amount, cap) = if ammo == AmmoType::Heavy { (2, 12) } else { (4, 30) };
                    w.add_reserves(amount, cap);
                }
            }
            self.hint(format!("{ammo:?} ammo"));
        }
    }

    fn run_enemies(&mut self, dt: f32) {
        let me = self.player_pos();
        let rules = self.sb.config.statuses.clone();
        let invisible = self.sb.get(self.player).is_some_and(|c| c.statuses.has(StatusKind::Invisible));
        let ids: Vec<CombatantId> = self.brains.keys().copied().collect();
        for id in ids {
            let Some(c) = self.sb.get(id) else { continue };
            if !c.is_alive() {
                continue;
            }
            let Some(pos) = c.position.map(v2) else { continue };
            let restrict = c.statuses.restrictions(&rules);
            let speed_mult = c.statuses.move_speed_mult(&rules);
            let brain = self.brains.get_mut(&id).expect("listed");
            brain.attack_timer -= dt;

            let to_me = sub(me, pos);
            let dist = len(to_me);
            let dir = norm(to_me);
            let mut step = [0.0, 0.0];
            if !restrict.movement {
                if invisible {
                    // Lost track of the player: drift sideways.
                    step = [-dir[1] * brain.strafe * 0.4, dir[0] * brain.strafe * 0.4];
                } else if brain.keep_away <= 0.0 || dist > brain.keep_away + 2.0 {
                    step = dir;
                } else if dist < brain.keep_away - 2.0 {
                    step = [-dir[0], -dir[1]];
                } else {
                    step = [-dir[1] * brain.strafe * 0.6, dir[0] * brain.strafe * 0.6];
                }
                if self.time % 4.0 < dt {
                    brain.strafe = -brain.strafe;
                }
            }
            let speed = brain.speed * speed_mult;
            let radius = brain.radius;
            let next = [pos[0] + step[0] * speed * dt, pos[1] + step[1] * speed * dt];

            let can_attack = !restrict.weapons && !invisible && brain.attack_timer <= 0.0;
            let mut attack = None;
            if can_attack {
                if brain.keep_away <= 0.0 && dist <= radius + PLAYER_RADIUS + 1.0 {
                    attack = Some((false, brain.damage, brain.element));
                } else if brain.keep_away > 0.0 && dist <= brain.keep_away + 8.0 {
                    attack = Some((true, brain.damage, brain.element));
                }
                if attack.is_some() {
                    brain.attack_timer = brain.attack_cooldown;
                }
            }
            self.set_pos(id, next, radius);
            match attack {
                Some((false, dmg, el)) => {
                    self.sb.deal_damage(self.player, DamageInstance::new(dmg, el, SourceKind::Melee).from(id));
                    let mut fx = Fx::new("slash", me[0], me[1], 0.2);
                    fx.el = Some(el);
                    fx.r = 1.0;
                    self.fx.push(fx);
                }
                Some((true, dmg, el)) => {
                    let speed = 16.0;
                    self.projectiles.push(Projectile {
                        pos,
                        vel: [dir[0] * speed, dir[1] * speed],
                        damage: dmg,
                        element: el,
                        owner: id,
                        life: 3.0,
                    });
                }
                None => {}
            }
        }
    }

    fn move_projectiles(&mut self, dt: f32) {
        let me = self.player_pos();
        // Player-team cover zones (barricades, bubbles) stop enemy shots.
        let cover: Vec<([f32; 2], f32)> = self
            .sb
            .zones()
            .filter(|z| z.team == PLAYER_TEAM && z.def.blocks_projectiles)
            .filter_map(|z| z.center.map(|c| (v2(c), z.def.radius)))
            .collect();
        let mut hits = Vec::new();
        self.projectiles.retain_mut(|p| {
            p.pos = [p.pos[0] + p.vel[0] * dt, p.pos[1] + p.vel[1] * dt];
            p.life -= dt;
            if cover.iter().any(|&(c, r)| len(sub(p.pos, c)) <= r && len(sub(me, c)) <= r + 1.0) {
                return false;
            }
            if len(sub(p.pos, me)) <= PLAYER_RADIUS + 0.2 {
                hits.push((p.owner, p.damage, p.element));
                return false;
            }
            let inside = p.pos[0] >= 0.0 && p.pos[0] <= ARENA_W && p.pos[1] >= 0.0 && p.pos[1] <= ARENA_H;
            p.life > 0.0 && inside
        });
        for (owner, dmg, el) in hits {
            let kind = SourceKind::Weapon { ammo: AmmoType::Primary, archetype: WeaponArchetype::AutoRifle };
            self.sb.deal_damage(self.player, DamageInstance::new(dmg, el, kind).from(owner));
        }
    }

    /// Pushes overlapping combatants apart.
    fn separate(&mut self) {
        let mut bodies: Vec<(CombatantId, [f32; 2], f32)> = self.alive_enemies().collect();
        bodies.push((self.player, self.player_pos(), PLAYER_RADIUS));
        let n = bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let d = sub(bodies[j].1, bodies[i].1);
                let l = len(d);
                let min = bodies[i].2 + bodies[j].2;
                if l < min && l > 1e-4 {
                    let push = (min - l) / 2.0;
                    let n2 = norm(d);
                    bodies[i].1 = [bodies[i].1[0] - n2[0] * push, bodies[i].1[1] - n2[1] * push];
                    bodies[j].1 = [bodies[j].1[0] + n2[0] * push, bodies[j].1[1] + n2[1] * push];
                }
            }
        }
        for (id, p, r) in bodies {
            self.set_pos(id, p, r);
        }
    }

    fn handle_events(&mut self) {
        let mut dealt: BTreeMap<CombatantId, (f32, bool, DamageType)> = BTreeMap::new();
        for e in self.sb.events() {
            match e {
                CombatEvent::Damaged { target, absorbed, precision, damage_type, .. } => {
                    let entry = dealt.entry(target).or_insert((0.0, false, damage_type));
                    entry.0 += absorbed.total();
                    entry.1 |= precision;
                }
                CombatEvent::Killed { victim, killer, kind } => self.on_killed(victim, killer, kind),
                CombatEvent::Blast { center, radius, damage_type } => {
                    let mut fx = Fx::new("blast", center[0], center[1], 0.45);
                    fx.r = radius;
                    fx.el = Some(damage_type);
                    self.fx.push(fx);
                }
                CombatEvent::Beam { from, to, damage_type } => {
                    let mut fx = Fx::new("beam", from[0], from[1], 0.25);
                    fx.x2 = to[0];
                    fx.y2 = to[1];
                    fx.el = Some(damage_type);
                    self.fx.push(fx);
                }
                CombatEvent::Triggered { kind, origin, .. } => {
                    if let Some(p) = self.pos(origin) {
                        let mut fx = Fx::new("text", p[0], p[1] - 1.2, 0.9);
                        fx.text = trigger_name(kind).to_uppercase();
                        self.fx.push(fx);
                    }
                }
                CombatEvent::ShieldBroken { target, element, .. } => {
                    if let Some(p) = self.pos(target) {
                        let mut fx = Fx::new("text", p[0], p[1] - 1.4, 0.8);
                        fx.text = "SHIELD BROKEN".into();
                        fx.el = element;
                        self.fx.push(fx);
                    }
                }
                CombatEvent::ChampionStunned { target, .. } => {
                    if let Some(p) = self.pos(target) {
                        let mut fx = Fx::new("text", p[0], p[1] - 1.6, 1.0);
                        fx.text = "STUNNED".into();
                        self.fx.push(fx);
                    }
                    self.say("Champion stunned".into());
                }
                CombatEvent::BarrierRaised { .. } => self.hint("Barrier up: use the Hand Cannon (1) to break it"),
                CombatEvent::AbilityUsed { user, name, .. } if user == self.player => self.say(name),
                CombatEvent::AbilityReady { user, slot } if user == self.player && slot == AbilitySlot::Super => {
                    self.hint("Super ready! (F)");
                }
                CombatEvent::SuperStarted { user, name } if user == self.player => {
                    let me = self.player_pos();
                    let mut fx = Fx::new("banner", me[0], me[1], 1.2);
                    fx.text = name.to_uppercase();
                    self.fx.push(fx);
                }
                CombatEvent::AspectTriggered { owner, aspect } if owner == self.player => {
                    let me = self.player_pos();
                    let mut fx = Fx::new("text", me[0], me[1] - 1.3, 0.9);
                    fx.text = aspect;
                    fx.r = 1.0;
                    self.fx.push(fx);
                }
                CombatEvent::Move { user, kind, distance, toward } if user == self.player => {
                    self.apply_move(kind, distance, toward.map(v2));
                }
                CombatEvent::OrbCollected { by, .. } if by == self.player => self.hint("Orb of Power"),
                _ => {}
            }
        }
        for (target, (amount, crit, el)) in dealt {
            if amount < 0.5 {
                continue;
            }
            let Some(p) = self.pos(target) else { continue };
            let mut fx = Fx::new(if target == self.player { "hurt" } else { "dmg" }, p[0], p[1] - 0.6, 0.7);
            fx.text = format!("{}", amount.round() as i64);
            fx.crit = crit;
            fx.el = Some(el);
            fx.x += (self.rand() - 0.5) * 0.8;
            self.fx.push(fx);
        }
    }

    fn on_killed(&mut self, victim: CombatantId, killer: Option<CombatantId>, kind: SourceKind) {
        let Some(brain) = self.brains.remove(&victim) else { return };
        let pos = self.pos(victim);
        self.sb.despawn(victim);
        if killer != Some(self.player) {
            return;
        }
        self.kills += 1;
        let Some(p) = pos else { return };
        let mut fx = Fx::new("death", p[0], p[1], 0.5);
        fx.r = brain.radius;
        self.fx.push(fx);

        let ability_kill = !kind.is_weapon();
        if ability_kill && self.rand() < 0.35 {
            self.sb.spawn_orb(PLAYER_TEAM, Some(v3(p)));
        }
        let tough = !matches!(brain.kind, EnemyKind::Thrall | EnemyKind::Acolyte);
        let roll = self.rand();
        let ammo = if roll < if tough { 0.5 } else { 0.12 } {
            Some(AmmoType::Special)
        } else if roll < if tough { 0.8 } else { 0.16 } {
            Some(AmmoType::Heavy)
        } else {
            None
        };
        if let Some(ammo) = ammo {
            self.bricks.push(AmmoBrick { pos: p, ammo, life: 20.0 });
        }
        if let EnemyKind::Champion(_) | EnemyKind::Ogre = brain.kind {
            let name = if brain.kind == EnemyKind::Ogre { "Ogre" } else { "Champion" };
            self.say(format!("{name} defeated"));
        }
    }

    fn apply_move(&mut self, kind: MoveKind, distance: f32, toward: Option<[f32; 2]>) {
        let me = self.player_pos();
        let moving = norm([self.input.move_x, self.input.move_y]);
        let aim_dir = norm(sub(self.aim, me));
        let (dir, travel) = match kind {
            MoveKind::Hop => return,
            MoveKind::Dash => (if moving == [0.0, 0.0] { aim_dir } else { moving }, distance),
            MoveKind::Blink => (aim_dir, distance),
            MoveKind::Lunge => match toward {
                Some(t) => (norm(sub(t, me)), distance.min((len(sub(t, me)) - 1.2).max(0.0))),
                None => (aim_dir, distance * 0.5),
            },
            MoveKind::Slam | MoveKind::Grapple => match toward {
                Some(t) => (norm(sub(t, me)), distance.min(len(sub(t, me)))),
                None => (aim_dir, distance * 0.5),
            },
        };
        let to = [me[0] + dir[0] * travel, me[1] + dir[1] * travel];
        let mut fx = Fx::new("dash", me[0], me[1], 0.3);
        fx.x2 = to[0];
        fx.y2 = to[1];
        self.fx.push(fx);
        self.set_pos(self.player, to, PLAYER_RADIUS);
    }

    // ---- output -----------------------------------------------------------

    pub fn state(&self) -> State {
        let rules = &self.sb.config.statuses;
        let p = self.sb.get(self.player).expect("player exists");
        let me = self.player_pos();
        let abilities = AbilitySlot::ALL
            .iter()
            .filter_map(|&slot| {
                let a = p.loadout.get(slot)?;
                Some(AbilityOut {
                    slot: slot_name(slot),
                    name: a.def.name.clone(),
                    progress: a.progress(),
                    charges: a.charges,
                    max: a.def.max_charges,
                    el: a.def.damage_type,
                })
            })
            .collect();
        let weapons = p
            .weapons
            .iter()
            .enumerate()
            .map(|(i, w)| WeaponOut {
                name: w.def.name.clone(),
                el: w.def.damage_type,
                ammo: w.def.ammo,
                mag: w.magazine,
                reserves: w.reserves,
                reloading: w.is_reloading(),
                active: i == p.active_weapon,
                anti: w.def.anti_champion,
            })
            .collect();
        let player = PlayerOut {
            x: me[0],
            y: me[1],
            r: PLAYER_RADIUS,
            health: p.health.health,
            max_health: p.health.max_health,
            shield: p.health.shield,
            max_shield: p.health.max_shield,
            overshield: p.health.overshield,
            statuses: p.statuses.iter().map(|s| status_name(s.kind)).collect(),
            invisible: p.statuses.has(StatusKind::Invisible),
            super_mode: p.super_mode.as_ref().map(|m| SuperOut {
                name: m.name.clone(),
                remaining: m.remaining,
                duration: m.def.duration,
                light: m.def.light.name.clone(),
                heavy: m.def.heavy.as_ref().map(|h| h.name.clone()),
                uses_left: m.light_uses_left,
                el: m.damage_type,
            }),
            abilities,
            weapons,
            aspects: p.aspects.iter().map(|a| a.def.name.clone()).collect(),
            buffs: p.modifiers.iter().filter(|m| m.remaining.is_some()).map(|m| m.name.clone()).collect(),
            class: p.class.unwrap_or(GuardianClass::Titan),
            element: self.choice.element,
        };
        let enemies = self
            .brains
            .iter()
            .filter_map(|(&id, b)| {
                let c = self.sb.get(id)?;
                let pos = v2(c.position?);
                Some(EnemyOut {
                    id: id.0,
                    x: pos[0],
                    y: pos[1],
                    r: b.radius,
                    name: c.name.clone(),
                    rank: c.rank,
                    health: c.health.health,
                    max_health: c.health.max_health,
                    shield: c.health.shield,
                    max_shield: c.health.max_shield,
                    shield_el: c.health.shield_element,
                    statuses: c.statuses.iter().map(|s| status_name(s.kind)).collect(),
                    champion: c.champion.as_ref().map(|ch| ch.kind),
                    stunned: c.champion.as_ref().is_some_and(|ch| ch.is_stunned()),
                    barrier: c.champion.as_ref().is_some_and(|ch| ch.barrier_up),
                    frozen: c.statuses.has(StatusKind::Frozen),
                    suspended: c.statuses.has(StatusKind::Suspend),
                    slowed: c.statuses.move_speed_mult(rules) < 1.0,
                })
            })
            .collect();
        let zones = self
            .sb
            .zones()
            .filter_map(|z| {
                let c = v2(z.center?);
                Some(ZoneOut {
                    x: c[0],
                    y: c[1],
                    r: z.def.radius,
                    el: z.damage_type,
                    name: z.name.clone(),
                    left: z.remaining / z.def.duration.max(0.01),
                    cover: z.def.blocks_projectiles,
                    friendly: z.team == PLAYER_TEAM,
                })
            })
            .collect();
        State {
            time: self.time,
            wave: self.wave,
            kills: self.kills,
            game_over: self.game_over,
            arena: [ARENA_W, ARENA_H],
            aim: self.aim,
            player,
            enemies,
            zones,
            orbs: self.sb.orbs().filter_map(|o| o.position.map(v2)).collect(),
            projectiles: self.projectiles.iter().map(|p| (p.pos, p.element)).collect(),
            bricks: self.bricks.iter().map(|b| (b.pos, b.ammo)).collect(),
            fx: self.fx.clone(),
            log: self.log.iter().cloned().collect(),
            hint: self.hint.as_ref().map(|h| h.0.clone()),
        }
    }
}

fn trigger_name(k: TriggerKind) -> &'static str {
    match k {
        TriggerKind::Ignite => "Ignite",
        TriggerKind::JoltChain => "Jolt",
        TriggerKind::VolatileExplosion => "Volatile",
        TriggerKind::Shatter => "Shatter",
        TriggerKind::UnravelThreads => "Unravel",
    }
}

fn status_name(k: StatusKind) -> String {
    match k {
        StatusKind::FrostArmor => "Frost Armor".into(),
        StatusKind::WovenMail => "Woven Mail".into(),
        other => format!("{other:?}"),
    }
}

#[derive(Debug, Serialize)]
pub struct State {
    pub time: f32,
    pub wave: u32,
    pub kills: u32,
    pub game_over: bool,
    pub arena: [f32; 2],
    pub aim: [f32; 2],
    pub player: PlayerOut,
    pub enemies: Vec<EnemyOut>,
    pub zones: Vec<ZoneOut>,
    pub orbs: Vec<[f32; 2]>,
    pub projectiles: Vec<([f32; 2], DamageType)>,
    pub bricks: Vec<([f32; 2], AmmoType)>,
    pub fx: Vec<Fx>,
    pub log: Vec<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlayerOut {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub health: f32,
    pub max_health: f32,
    pub shield: f32,
    pub max_shield: f32,
    pub overshield: f32,
    pub statuses: Vec<String>,
    pub invisible: bool,
    pub super_mode: Option<SuperOut>,
    pub abilities: Vec<AbilityOut>,
    pub weapons: Vec<WeaponOut>,
    pub aspects: Vec<String>,
    pub buffs: Vec<String>,
    pub class: GuardianClass,
    pub element: SubclassElement,
}

#[derive(Debug, Serialize)]
pub struct SuperOut {
    pub name: String,
    pub remaining: f32,
    pub duration: f32,
    pub light: String,
    pub heavy: Option<String>,
    pub uses_left: Option<u32>,
    pub el: DamageType,
}

#[derive(Debug, Serialize)]
pub struct AbilityOut {
    pub slot: &'static str,
    pub name: String,
    pub progress: f32,
    pub charges: u32,
    pub max: u32,
    pub el: DamageType,
}

#[derive(Debug, Serialize)]
pub struct WeaponOut {
    pub name: String,
    pub el: DamageType,
    pub ammo: AmmoType,
    pub mag: u32,
    pub reserves: Option<u32>,
    pub reloading: bool,
    pub active: bool,
    pub anti: Option<ChampionKind>,
}

#[derive(Debug, Serialize)]
pub struct EnemyOut {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub name: String,
    pub rank: Rank,
    pub health: f32,
    pub max_health: f32,
    pub shield: f32,
    pub max_shield: f32,
    pub shield_el: Option<DamageType>,
    pub statuses: Vec<String>,
    pub champion: Option<ChampionKind>,
    pub stunned: bool,
    pub barrier: bool,
    pub frozen: bool,
    pub suspended: bool,
    pub slowed: bool,
}

#[derive(Debug, Serialize)]
pub struct ZoneOut {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub el: DamageType,
    pub name: String,
    pub left: f32,
    pub cover: bool,
    pub friendly: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays every kit on autopilot: moves in a circle, auto-aims and fires,
    /// and uses every ability and super attack as soon as it can.
    fn autopilot(choice: LoadoutChoice, seconds: f32) -> Game {
        let mut g = Game::new(choice).expect("valid loadout");
        if let Some(s) = g.sb.get_mut(g.player).and_then(|c| c.loadout.super_.as_mut()) {
            s.add_energy(1.0);
        }
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        while t < seconds && !g.game_over {
            let a = t * 0.8;
            g.set_input(Input { move_x: a.cos(), move_y: a.sin(), aim_x: 0.0, aim_y: 0.0, fire: false, auto: true });
            let frame = (t / dt) as u32;
            match frame % 60 {
                5 => g.command(Command::Ability(AbilitySlot::Super)),
                15 => g.command(Command::Ability(AbilitySlot::Grenade)),
                25 => g.command(Command::Ability(AbilitySlot::Melee)),
                35 => g.command(Command::Ability(AbilitySlot::ClassAbility)),
                45 => g.command(Command::HeavyAttack),
                50 => g.command(Command::Weapon(((frame / 600) % 3) as usize)),
                _ => {}
            }
            g.tick(dt);
            let _ = serde_json::to_string(&g.state()).expect("state serializes");
            t += dt;
        }
        g
    }

    #[test]
    fn every_class_and_subclass_plays() {
        for class in catalog::CLASSES {
            for element in catalog::SUBCLASSES {
                let g = autopilot(LoadoutChoice::default_for(class, element), 45.0);
                assert!(g.kills > 0, "{class:?} {element:?} got no kills");
                assert!(g.wave >= 1);
            }
        }
    }

    #[test]
    fn every_ability_choice_is_playable() {
        for kit in catalog::all_kits() {
            for slot in AbilitySlot::ALL {
                for a in kit.options(slot) {
                    let mut choice = LoadoutChoice::default_for(kit.class, kit.element);
                    match slot {
                        AbilitySlot::Super => choice.super_ = a.name.clone(),
                        AbilitySlot::Grenade => choice.grenade = a.name.clone(),
                        AbilitySlot::Melee => choice.melee = a.name.clone(),
                        AbilitySlot::ClassAbility => choice.class_ability = a.name.clone(),
                    }
                    autopilot(choice, 8.0);
                }
            }
            for pair in kit.aspects.windows(2) {
                let mut choice = LoadoutChoice::default_for(kit.class, kit.element);
                choice.aspects = pair.iter().map(|a| a.name.clone()).collect();
                autopilot(choice, 8.0);
            }
        }
    }

    #[test]
    #[ignore = "balance report: cargo test -p guardian_arena --release -- --ignored --nocapture"]
    fn balance_report() {
        for class in catalog::CLASSES {
            for element in catalog::SUBCLASSES {
                let g = autopilot(LoadoutChoice::default_for(class, element), 240.0);
                println!(
                    "{:<8} {:<10} wave {:>2} kills {:>3} alive {} t={:.0}s",
                    format!("{class:?}"),
                    format!("{element:?}"),
                    g.wave,
                    g.kills,
                    !g.game_over,
                    g.time
                );
            }
        }
    }

    #[test]
    fn rejects_abilities_from_another_subclass() {
        let mut choice = LoadoutChoice::default_for(GuardianClass::Titan, SubclassElement::Arc);
        choice.grenade = "Vortex Grenade".into();
        assert!(Game::new(choice).is_err());
    }

    #[test]
    fn waves_escalate_and_the_player_can_die() {
        let mut g = Game::new(LoadoutChoice::default_for(GuardianClass::Hunter, SubclassElement::Void)).unwrap();
        // Stand still and never shoot.
        let mut t = 0.0;
        while t < 120.0 && !g.game_over {
            g.tick(1.0 / 30.0);
            t += 1.0 / 30.0;
        }
        assert!(g.game_over, "an idle player should eventually fall");
    }
}
