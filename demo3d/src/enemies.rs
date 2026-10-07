//! The Hive (played by KayKit skeletons): waves, spawning, AI, attacks,
//! hit reactions and deaths.

use bevy::prelude::*;
use guardian_combat::combatant::{ChampionKind, Combatant, CombatantId, Rank};
use guardian_combat::damage::{DamageInstance, SourceKind};
use guardian_combat::element::DamageType;
use guardian_combat::sandbox::CombatEvent;
use guardian_combat::status::StatusKind;
use guardian_combat::weapon::{AmmoType, WeaponArchetype};

use crate::combat::{AmmoBrick, Combat, ENEMY_TEAM, PLAYER_TEAM};
use crate::common::{a3, v3, AppState, FrameEvents, MatchEntity, Phase, Rng};
use crate::level::Level;
use crate::models::{self, ModelKind, Models, Prop, Rig};
use crate::player::Player;
use crate::vfx::{Fx, FxAssets};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind {
    Thrall,
    Acolyte,
    Wizard,
    Knight,
    Champion(ChampionKind),
    Ogre,
}

struct Profile {
    name: &'static str,
    model: ModelKind,
    props: &'static [(Prop, bool)],
    scale: f32,
    hp: f32,
    shield: f32,
    rank: Rank,
    speed: f32,
    /// Ranged enemies hold about this distance; 0 = melee.
    keep_away: f32,
    cooldown: f32,
    damage: f32,
    element: DamageType,
}

impl EnemyKind {
    fn profile(self) -> Profile {
        match self {
            EnemyKind::Thrall => Profile {
                name: "Thrall",
                model: ModelKind::Minion,
                props: &[],
                scale: 0.85,
                hp: 150.0,
                shield: 0.0,
                rank: Rank::Minor,
                speed: 5.6,
                keep_away: 0.0,
                cooldown: 1.1,
                damage: 18.0,
                element: DamageType::Kinetic,
            },
            EnemyKind::Acolyte => Profile {
                name: "Acolyte",
                model: ModelKind::SkeletonRogue,
                props: &[(Prop::Crossbow, false)],
                scale: 0.9,
                hp: 260.0,
                shield: 0.0,
                rank: Rank::Minor,
                speed: 3.4,
                keep_away: 14.0,
                cooldown: 1.6,
                damage: 10.0,
                element: DamageType::Solar,
            },
            EnemyKind::Wizard => Profile {
                name: "Wizard",
                model: ModelKind::SkeletonMage,
                props: &[(Prop::Staff, false)],
                scale: 0.95,
                hp: 600.0,
                shield: 250.0,
                rank: Rank::Major,
                speed: 3.0,
                keep_away: 16.0,
                cooldown: 1.9,
                damage: 14.0,
                element: DamageType::Arc,
            },
            EnemyKind::Knight => Profile {
                name: "Knight",
                model: ModelKind::SkeletonWarrior,
                props: &[(Prop::Blade, false), (Prop::Shield, true)],
                scale: 1.3,
                hp: 1100.0,
                shield: 400.0,
                rank: Rank::Major,
                speed: 3.2,
                keep_away: 9.0,
                cooldown: 2.2,
                damage: 18.0,
                element: DamageType::Void,
            },
            EnemyKind::Champion(k) => Profile {
                name: match k {
                    ChampionKind::Barrier => "Barrier Knight",
                    ChampionKind::Overload => "Overload Captain",
                    ChampionKind::Unstoppable => "Unstoppable Ogre",
                },
                model: ModelKind::SkeletonWarrior,
                props: &[(Prop::Axe, false)],
                scale: 1.6,
                hp: 3000.0,
                shield: 0.0,
                rank: Rank::Major,
                speed: if k == ChampionKind::Unstoppable { 4.6 } else { 3.0 },
                keep_away: if k == ChampionKind::Unstoppable { 0.0 } else { 12.0 },
                cooldown: if k == ChampionKind::Overload { 0.8 } else { 1.4 },
                damage: if k == ChampionKind::Unstoppable { 32.0 } else { 12.0 },
                element: DamageType::Arc,
            },
            EnemyKind::Ogre => Profile {
                name: "Ogre",
                model: ModelKind::SkeletonWarrior,
                props: &[(Prop::Axe, false)],
                scale: 2.5,
                hp: 9000.0,
                shield: 1500.0,
                rank: Rank::Boss,
                speed: 2.2,
                keep_away: 10.0,
                cooldown: 2.6,
                damage: 30.0,
                element: DamageType::Void,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EnemyState {
    Spawning(f32),
    Active,
    Dying(f32),
}

#[derive(Component)]
pub struct Enemy {
    pub id: CombatantId,
    pub kind: EnemyKind,
    pub name: &'static str,
    pub scale: f32,
    pub state: EnemyState,
    speed: f32,
    keep_away: f32,
    cooldown: f32,
    damage: f32,
    element: DamageType,
    attack_timer: f32,
    windup: Option<(f32, bool)>,
    hit_cd: f32,
    strafe: f32,
    pub lift: f32,
    paused: bool,
    sees: bool,
}

impl Enemy {
    pub fn targetable(&self) -> bool {
        !matches!(self.state, EnemyState::Dying(_))
    }

    /// AI state, for the debug snapshot.
    pub fn debug(&self, t: &Transform, player: Vec3) -> String {
        format!(
            "{} {:?} d={:.1} atk={:.1} wind={:?} sees={}",
            self.name,
            self.state,
            t.translation.distance(player),
            self.attack_timer,
            self.windup,
            self.sees
        )
    }
}

/// (body centre, body radius, head centre, head radius) in world space.
pub fn hitboxes(e: &Enemy, t: &Transform) -> (Vec3, f32, Vec3, f32) {
    let s = e.scale;
    let base = t.translation;
    (base + Vec3::Y * 0.78 * s, 0.5 * s, base + Vec3::Y * 1.68 * s, 0.42 * s)
}

#[derive(Component)]
pub struct EnemyBolt {
    vel: Vec3,
    damage: f32,
    element: DamageType,
    owner: CombatantId,
    life: f32,
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (waves, ai).chain().in_set(Phase::Act),
                bolts.in_set(Phase::Act).after(ai),
                reactions.in_set(Phase::React),
            )
                .run_if(in_state(AppState::Playing)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn waves(
    mut commands: Commands,
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    level: Res<Level>,
    models: Res<Models>,
    mut rng: ResMut<Rng>,
    player: Single<&Transform, With<Player>>,
    enemies: Query<&Enemy>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    let c = &mut *combat;
    if c.over {
        return;
    }
    let alive = enemies.iter().filter(|e| e.targetable()).count();
    if alive == 0 && c.spawn_queue.is_empty() {
        c.wave_delay -= dt;
        if c.wave_delay <= 0.0 {
            c.wave += 1;
            c.wave_delay = 4.0;
            let n = c.wave;
            let q = &mut c.spawn_queue;
            for _ in 0..(3 + n).min(10) {
                q.push_back(EnemyKind::Thrall);
            }
            for _ in 0..(1 + n / 2).min(5) {
                q.push_back(EnemyKind::Acolyte);
            }
            for _ in 0..(n / 2).min(3) {
                q.push_back(EnemyKind::Wizard);
            }
            for _ in 0..(n / 3).min(3) {
                q.push_back(EnemyKind::Knight);
            }
            if n >= 2 {
                let kinds = [ChampionKind::Barrier, ChampionKind::Overload, ChampionKind::Unstoppable];
                q.push_back(EnemyKind::Champion(kinds[(n as usize) % 3]));
            }
            if n.is_multiple_of(5) {
                q.push_back(EnemyKind::Ogre);
            }
            c.banner(format!("WAVE {n}"));
        }
        return;
    }
    c.spawn_timer -= dt;
    if c.spawn_timer > 0.0 {
        return;
    }
    let Some(kind) = c.spawn_queue.pop_front() else { return };
    c.spawn_timer = 0.35;

    let me = player.translation;
    let mut at = level.spawns[0];
    for _ in 0..12 {
        let cand = level.spawns[(rng.f() * level.spawns.len() as f32) as usize % level.spawns.len()];
        at = cand + Vec3::new(rng.range(-2.5, 2.5), 0.0, rng.range(-2.5, 2.5));
        if at.distance(me) > 14.0 {
            break;
        }
    }
    at.y = level.floor_at(at, 0.5);

    let p = kind.profile();
    let scale_hp = 1.0 + 0.12 * c.wave.saturating_sub(1) as f32;
    let elements = [DamageType::Arc, DamageType::Solar, DamageType::Void, DamageType::Stasis, DamageType::Strand];
    let shield_el = elements[(rng.f() * 5.0) as usize % 5];
    let mut cb = Combatant::enemy(p.name, ENEMY_TEAM, p.rank, p.hp * scale_hp).at(a3(at + Vec3::Y * 0.9 * p.scale));
    if p.shield > 0.0 {
        cb = cb.with_shield(p.shield * scale_hp, Some(shield_el));
    }
    if let EnemyKind::Champion(k) = kind {
        cb = cb.as_champion(k);
    }
    let id = c.sb.spawn(cb);

    let mut rig = Rig::new(p.model);
    for &(prop, left) in p.props {
        rig = rig.with_prop(prop, left);
    }
    let face = me - at;
    let transform = Transform::from_translation(at)
        .with_rotation(Quat::from_rotation_y(face.x.atan2(face.z)))
        .with_scale(Vec3::splat(p.scale));
    let e = models.spawn(&mut commands, rig, transform);
    commands.entity(e).insert((
        MatchEntity,
        Enemy {
            id,
            kind,
            name: p.name,
            scale: p.scale,
            state: EnemyState::Spawning(1.9),
            speed: p.speed,
            keep_away: p.keep_away,
            cooldown: p.cooldown,
            damage: p.damage,
            element: p.element,
            attack_timer: 1.0 + rng.f(),
            windup: None,
            hit_cd: 0.0,
            strafe: if rng.f() < 0.5 { -1.0 } else { 1.0 },
            lift: 0.0,
            paused: false,
            sees: false,
        },
    ));
    c.enemies.insert(id, e);
    fx.spawn(&mut commands, &mut meshes, Fx::portal(at, p.scale));
}

#[allow(clippy::too_many_arguments)]
fn ai(
    mut commands: Commands,
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    level: Res<Level>,
    models: Res<Models>,
    player: Single<&Transform, (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(Entity, &mut Enemy, &mut Transform, &mut Rig)>,
    mut anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs().min(0.05);
    let me = player.translation;
    let chest = me + Vec3::Y * 1.1;
    let c = &mut *combat;
    let rules = c.sb.config.statuses.clone();
    let invisible = c.sb.get(c.player).is_some_and(|p| p.statuses.has(StatusKind::Invisible));
    let player_alive = c.player_alive() && !c.over;
    let wobble = c.time;

    let mut positions: Vec<(Entity, Vec3, f32)> = Vec::new();
    for (entity, mut e, mut t, mut rig) in &mut enemies {
        match e.state {
            EnemyState::Spawning(left) => {
                if rig.player.is_some() && rig.current.is_empty() {
                    models::play(&mut rig, &models, &mut anim, "Spawn_Ground_Skeletons", false, 1.0);
                }
                let left = left - dt;
                e.state = if left <= 0.0 { EnemyState::Active } else { EnemyState::Spawning(left) };
                positions.push((entity, t.translation, e.scale));
                continue;
            }
            EnemyState::Dying(left) => {
                let left = left - dt;
                if left < 1.2 {
                    t.translation.y -= dt * 0.8 * e.scale;
                }
                if left <= 0.0 {
                    commands.entity(entity).despawn();
                    c.sb.despawn(e.id);
                    c.enemies.remove(&e.id);
                } else {
                    e.state = EnemyState::Dying(left);
                }
                continue;
            }
            EnemyState::Active => {}
        }
        let Some(cb) = c.sb.get(e.id) else { continue };
        let restrict = cb.statuses.restrictions(&rules);
        let speed_mult = cb.statuses.move_speed_mult(&rules);
        let suspended = cb.statuses.has(StatusKind::Suspend);
        let stunned = cb.champion.as_ref().is_some_and(|ch| ch.is_stunned());
        let held = restrict.movement || stunned;

        // Frozen and suspended enemies stop mid-motion.
        let paused = restrict.movement && !stunned;
        if paused != e.paused {
            models::set_paused(&rig, &mut anim, paused);
            e.paused = paused;
        }
        let lift_target = if suspended { 1.4 } else { 0.0 };
        e.lift += (lift_target - e.lift) * (dt * 4.0).min(1.0);

        e.attack_timer -= dt;
        e.hit_cd -= dt;
        let base = Vec3::new(t.translation.x, level.floor_at(t.translation, 0.3), t.translation.z);
        let to_me = Vec3::new(me.x - base.x, 0.0, me.z - base.z);
        let dist = to_me.length();
        let dir = to_me.normalize_or_zero();
        let eye = base + Vec3::Y * 1.5 * e.scale;
        let sees = player_alive && !invisible && level.clear(eye, chest);
        e.sees = sees;

        // Finish an attack that's winding up.
        if let Some((left, ranged)) = &mut e.windup {
            *left -= dt;
            if *left <= 0.0 {
                let ranged = *ranged;
                e.windup = None;
                if !restrict.weapons && player_alive {
                    if ranged {
                        let from = base + t.rotation * Vec3::new(-0.3 * e.scale, 1.2 * e.scale, 0.6 * e.scale);
                        let aim = (chest - from).normalize_or_zero();
                        let speed = if e.kind == EnemyKind::Ogre { 15.0 } else { 21.0 };
                        let mesh = fx.sphere(&mut meshes);
                        commands.spawn((
                            MatchEntity,
                            EnemyBolt {
                                vel: aim * speed,
                                damage: e.damage,
                                element: e.element,
                                owner: e.id,
                                life: 4.0,
                            },
                            Mesh3d(mesh),
                            MeshMaterial3d(fx.glow(e.element)),
                            Transform::from_translation(from).with_scale(Vec3::splat(if e.kind == EnemyKind::Ogre {
                                0.6
                            } else {
                                0.22
                            })),
                        ));
                    } else if dist <= 1.6 + 0.5 * e.scale {
                        let inst = DamageInstance::new(e.damage, e.element, SourceKind::Melee).from(e.id);
                        c.sb.deal_damage(c.player, inst);
                    }
                }
            }
        }

        let mut step = Vec3::ZERO;
        if !held {
            if !player_alive || invisible {
                step = Vec3::new(-dir.z, 0.0, dir.x) * e.strafe * 0.3;
            } else if e.keep_away <= 0.0 && dist < 1.0 + 0.4 * e.scale {
                // In melee range: stand and swing.
                step = Vec3::ZERO;
            } else if e.keep_away <= 0.0 || dist > e.keep_away + 3.0 || !sees {
                step = dir;
            } else if dist < e.keep_away - 3.0 {
                step = -dir;
            } else {
                step = Vec3::new(-dir.z, 0.0, dir.x) * e.strafe * 0.55;
            }
            if (wobble + e.id.0 as f32 * 0.7) % 4.0 < dt {
                e.strafe = -e.strafe;
            }
            if e.windup.is_some() {
                step *= 0.2;
            }
        }
        let moving = step.length_squared() > 0.01;
        let speed = e.speed * speed_mult;
        let (mut next, step) = steer(&level, base, step, speed * dt, 0.35 * e.scale, 1.8 * e.scale, e.strafe);
        next.y = level.floor_at(next, 0.3) + e.lift;
        t.translation = next;
        let look = if sees && (e.windup.is_some() || !moving || e.keep_away > 0.0) { dir } else { step };
        if look.length_squared() > 1e-4 && !held {
            let want = Quat::from_rotation_y(look.x.atan2(look.z));
            t.rotation = t.rotation.slerp(want, (dt * 8.0).min(1.0));
        }

        // Start an attack.
        let can_attack = !restrict.weapons && !held && sees && e.attack_timer <= 0.0 && e.windup.is_none();
        if can_attack {
            let melee_range = 1.4 + 0.5 * e.scale;
            if e.keep_away <= 0.0 && dist <= melee_range {
                e.windup = Some((0.35, false));
                e.attack_timer = e.cooldown;
                let a =
                    if e.kind == EnemyKind::Thrall { "Unarmed_Melee_Attack_Punch_A" } else { "1H_Melee_Attack_Chop" };
                models::play(&mut rig, &models, &mut anim, a, false, 1.3);
            } else if e.keep_away > 0.0 && dist <= e.keep_away + 12.0 {
                e.windup = Some((0.45, true));
                e.attack_timer = e.cooldown;
                let a = match e.kind {
                    EnemyKind::Acolyte => "1H_Ranged_Shoot",
                    EnemyKind::Ogre => "2H_Melee_Attack_Spin",
                    _ => "Spellcast_Shoot",
                };
                models::play(&mut rig, &models, &mut anim, a, false, 1.2);
            }
        }
        if !rig.busy() {
            let (name, sp) = if !moving {
                ("Idle_Combat", 1.0)
            } else if e.kind == EnemyKind::Knight || e.kind == EnemyKind::Ogre {
                ("Walking_D_Skeletons", speed / 2.0)
            } else {
                ("Running_A", (speed / 4.5).max(0.6))
            };
            models::play(&mut rig, &models, &mut anim, name, true, sp);
        }
        positions.push((entity, t.translation, e.scale));
    }

    // Keep enemies out of the player and from overlapping each other.
    for (_, p, s) in &mut positions {
        let d = Vec3::new(p.x - me.x, 0.0, p.z - me.z);
        let min = 0.4 + 0.35 * *s;
        let l = d.length();
        if l < min {
            *p += if l > 1e-4 { d / l * (min - l) } else { Vec3::X * min };
        }
    }
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            let (a, b) = (positions[i].1, positions[j].1);
            let d = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
            let min = 0.45 * (positions[i].2 + positions[j].2);
            let l = d.length();
            if l < min && l > 1e-4 {
                let push = d / l * (min - l) * 0.5;
                positions[i].1 -= push;
                positions[j].1 += push;
            }
        }
    }
    for (entity, p, _) in positions {
        if let Ok((_, e, mut t, _)) = enemies.get_mut(entity) {
            if e.state == EnemyState::Active {
                t.translation.x = p.x;
                t.translation.z = p.z;
            }
        }
    }
}

/// Moves along `step`, turning to slide around walls when blocked. Tries
/// the wanted direction first, then angles away from it on the `side` the
/// enemy prefers, then the other side.
fn steer(level: &Level, from: Vec3, step: Vec3, dist: f32, radius: f32, height: f32, side: f32) -> (Vec3, Vec3) {
    if step == Vec3::ZERO || dist <= 0.0 {
        return (level.collide(from, radius, height, 0.6), step);
    }
    let mut best = (level.collide(from + step * dist, radius, height, 0.6), step);
    let progress = |p: Vec3| Vec3::new(p.x - from.x, 0.0, p.z - from.z).dot(step.normalize_or_zero());
    if progress(best.0) >= dist * 0.6 {
        return best;
    }
    for deg in [45.0_f32, 80.0, 120.0] {
        for s in [side, -side] {
            let dir = Quat::from_rotation_y((deg * s).to_radians()) * step;
            let p = level.collide(from + dir * dist, radius, height, 0.6);
            let moved = Vec3::new(p.x - from.x, 0.0, p.z - from.z).length();
            if moved >= dist * 0.6 {
                return (p, dir);
            }
            if moved > Vec3::new(best.0.x - from.x, 0.0, best.0.z - from.z).length() {
                best = (p, dir);
            }
        }
    }
    best
}

#[allow(clippy::too_many_arguments)]
fn bolts(
    mut commands: Commands,
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    level: Res<Level>,
    player: Single<&Transform, (With<Player>, Without<EnemyBolt>)>,
    mut q: Query<(Entity, &mut EnemyBolt, &mut Transform)>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = time.delta_secs();
    let me = player.translation;
    let cover: Vec<(Vec3, f32)> = combat
        .sb
        .zones()
        .filter(|z| z.team == PLAYER_TEAM && z.def.blocks_projectiles)
        .filter_map(|z| z.center.map(|c| (v3(c), z.def.radius)))
        .collect();
    for (e, mut b, mut t) in &mut q {
        b.life -= dt;
        let step = b.vel * dt;
        let from = t.translation;
        let to = from + step;
        // Closest approach to the player's body (a segment from feet to head).
        let body_point = Vec3::new(me.x, to.y.clamp(me.y + 0.2, me.y + 1.7), me.z);
        let hit_player = to.distance(body_point) < 0.55 || from.distance(body_point) < 0.55;
        let blocked = cover.iter().any(|&(c, r)| to.distance(c) < r && me.distance(c) < r + 1.5);
        let wall = level.ray(from, step.normalize_or_zero(), step.length()).is_some();
        if hit_player && !blocked {
            let kind = SourceKind::Weapon { ammo: AmmoType::Primary, archetype: WeaponArchetype::AutoRifle };
            let pid = combat.player;
            combat.sb.deal_damage(pid, DamageInstance::new(b.damage, b.element, kind).from(b.owner));
        }
        if hit_player || blocked || wall || b.life <= 0.0 {
            fx.spawn(&mut commands, &mut meshes, Fx::sparks(to, b.element, 4));
            commands.entity(e).despawn();
        } else {
            t.translation = to;
        }
    }
}

/// Hit flinches, deaths and loot.
#[allow(clippy::too_many_arguments)]
fn reactions(
    mut commands: Commands,
    frame: Res<FrameEvents>,
    mut combat: ResMut<Combat>,
    models: Res<Models>,
    mut rng: ResMut<Rng>,
    mut enemies: Query<(&mut Enemy, &mut Rig, &Transform)>,
    mut anim: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for ev in &frame.0 {
        match ev {
            CombatEvent::Damaged { target, absorbed, .. } if !absorbed.killed && absorbed.total() > 0.0 => {
                let Some(&entity) = combat.enemies.get(target) else { continue };
                let Ok((mut e, mut rig, _)) = enemies.get_mut(entity) else { continue };
                if e.state == EnemyState::Active && e.hit_cd <= 0.0 && e.windup.is_none() && !e.paused {
                    let big = matches!(e.kind, EnemyKind::Ogre | EnemyKind::Champion(_));
                    if !big || absorbed.total() > 400.0 {
                        models::play(&mut rig, &models, &mut anim, "Hit_A", false, 1.3);
                        e.hit_cd = if big { 3.0 } else { 0.9 };
                    }
                }
            }
            CombatEvent::ChampionStunned { target, .. } => {
                let Some(&entity) = combat.enemies.get(target) else { continue };
                if let Ok((mut e, mut rig, _)) = enemies.get_mut(entity) {
                    e.windup = None;
                    models::play(&mut rig, &models, &mut anim, "Hit_A", false, 0.6);
                }
                combat.hint("Champion stunned!");
            }
            CombatEvent::BarrierRaised { .. } => combat.hint("Barrier up: break it with the Hand Cannon (1)"),
            CombatEvent::Killed { victim, killer, kind } => {
                let Some(&entity) = combat.enemies.get(victim) else { continue };
                let Ok((mut e, mut rig, t)) = enemies.get_mut(entity) else { continue };
                if !e.targetable() {
                    continue;
                }
                e.state = EnemyState::Dying(3.0);
                e.windup = None;
                if e.paused {
                    models::set_paused(&rig, &mut anim, false);
                    e.paused = false;
                }
                models::play(&mut rig, &models, &mut anim, "Death_C_Skeletons", false, 1.0);
                rig.oneshot = Some(("Death_C_Skeletons".into(), 1e9));
                if *killer != Some(combat.player) {
                    continue;
                }
                combat.kills += 1;
                let at = t.translation;
                fx.spawn(&mut commands, &mut meshes, Fx::sparks(at + Vec3::Y * 1.0 * e.scale, DamageType::Strand, 10));
                if !kind.is_weapon() && rng.f() < 0.35 {
                    combat.sb.spawn_orb(PLAYER_TEAM, Some(a3(at + Vec3::Y * 0.6)));
                }
                let tough = !matches!(e.kind, EnemyKind::Thrall | EnemyKind::Acolyte);
                let roll = rng.f();
                let ammo = if roll < if tough { 0.5 } else { 0.12 } {
                    Some(AmmoType::Special)
                } else if roll < if tough { 0.8 } else { 0.16 } {
                    Some(AmmoType::Heavy)
                } else {
                    None
                };
                if let Some(ammo) = ammo {
                    let mesh = fx.cube(&mut meshes);
                    commands.spawn((
                        MatchEntity,
                        AmmoBrick { ammo, life: 25.0 },
                        Mesh3d(mesh),
                        MeshMaterial3d(fx.ammo(ammo)),
                        Transform::from_translation(at + Vec3::Y * 0.35).with_scale(Vec3::new(0.45, 0.25, 0.3)),
                    ));
                }
                if matches!(e.kind, EnemyKind::Champion(_) | EnemyKind::Ogre) {
                    combat.hint(format!("{} defeated", e.name));
                }
            }
            _ => {}
        }
    }
}
