//! What the player does: shooting, ADS, reloads, weapon swaps, rockets,
//! abilities (aimed through the crosshair) and super attacks.

use bevy::prelude::*;
use guardian_combat::ability::{AbilityDef, AbilitySlot};
use guardian_combat::combatant::CombatantId;
use guardian_combat::damage::{DamageInstance, SourceKind};
use guardian_combat::effect::{Effect, MoveKind};
use guardian_combat::element::DamageType;
use guardian_combat::sandbox::{AbilityTarget, ActionError, CombatEvent, Shot};
use guardian_combat::weapon::{AmmoType, FireError, WeaponArchetype};

use crate::combat::Combat;
use crate::common::{a3, v3, AppState, FrameEvents, MatchEntity, Phase, Rng};
use crate::enemies::{hitboxes, Enemy};
use crate::level::Level;
use crate::models::{self, Models, Rig};
use crate::player::{aim_ray, ForcedMove, Grab, Player};
use crate::vfx::{Fx, FxAssets};
use crate::viewmodel::Viewmodel;

/// What to cast once a delay or a forced move finishes.
#[derive(Clone, Debug)]
pub struct PendingCast {
    pub what: CastKind,
    pub target: AbilityTarget,
    pub delay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CastKind {
    Ability(AbilitySlot),
    SuperAttack { heavy: bool },
}

#[derive(Message)]
pub struct CastNow(pub PendingCast);

/// Crosshair feedback for the HUD.
#[derive(Resource, Default)]
pub struct Hitmarker {
    pub t: f32,
    pub crit: bool,
    pub kill: f32,
}

#[derive(Component)]
pub struct Rocket {
    pub vel: Vec3,
    pub life: f32,
    pub direct: f32,
    pub blast: f32,
    pub radius: f32,
    pub element: DamageType,
    pub anti: Option<guardian_combat::combatant::ChampionKind>,
}

pub struct ActionsPlugin;

impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CastNow>().init_resource::<Hitmarker>().add_systems(
            Update,
            (
                (weapons, abilities, tick_pending, cast_now, fly_rockets).chain().in_set(Phase::Act),
                (apply_moves, hitmarkers).in_set(Phase::React),
            )
                .run_if(in_state(AppState::Playing)),
        );
    }
}

/// The first thing a ray hits: an enemy (with precision flag) or the level.
pub struct Hit {
    pub enemy: Option<(CombatantId, bool)>,
    pub dist: f32,
}

pub fn raycast(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    level: &Level,
    enemies: &Query<(&Enemy, &Transform)>,
    assist: f32,
) -> Hit {
    let wall = level.ray(origin, dir, range).unwrap_or(range);
    let mut best: Option<(CombatantId, bool, f32)> = None;
    for (e, t) in enemies {
        if !e.targetable() {
            continue;
        }
        let (body, br, head, hr) = hitboxes(e, t);
        for (center, r, crit) in [(head, hr, true), (body, br, false)] {
            if let Some(d) = crate::common::ray_sphere(origin, dir, center, r * assist) {
                if d < wall && best.is_none_or(|b| d < b.2 - if crit { 0.05 } else { 0.0 }) {
                    best = Some((e.id, crit, d));
                }
            }
        }
    }
    match best {
        Some((id, crit, d)) => Hit { enemy: Some((id, crit)), dist: d },
        None => Hit { enemy: None, dist: wall },
    }
}

#[allow(clippy::too_many_arguments)]
fn weapons(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    grab: Res<Grab>,
    level: Res<Level>,
    mut combat: ResMut<Combat>,
    player: Single<(&mut Player, &Transform)>,
    enemies: Query<(&Enemy, &Transform)>,
    mut vm: Single<&mut Viewmodel>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rng: ResMut<Rng>,
    mut marker: ResMut<Hitmarker>,
    mut casts: MessageWriter<CastNow>,
) {
    let dt = time.delta_secs();
    let (mut p, t) = player.into_inner();
    let pid = combat.player;
    let alive = combat.player_alive();
    let in_super = combat.sb.get(pid).is_some_and(|c| c.in_super());
    let ads_target =
        if grab.0 && alive && !in_super && mouse.pressed(MouseButton::Right) && !p.sprinting { 1.0 } else { 0.0 };
    p.ads += (ads_target - p.ads) * (dt * 12.0).min(1.0);
    if !grab.0 || !alive || combat.over || p.forced.is_some() {
        return;
    }

    if in_super {
        let heavy = mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::KeyF);
        let light = mouse.pressed(MouseButton::Left);
        if heavy || light {
            casts.write(CastNow(PendingCast {
                what: CastKind::SuperAttack { heavy },
                target: AbilityTarget::default(),
                delay: 0.0,
            }));
        }
        return;
    }

    for (key, i) in [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1), (KeyCode::Digit3, 2)] {
        if keys.just_pressed(key) {
            if let Some(c) = combat.sb.get_mut(pid) {
                if c.active_weapon != i {
                    c.active_weapon = i;
                    vm.swap = 1.0;
                }
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyR) && combat.sb.reload(pid) {
        vm.reload = 1.0;
    }
    if !mouse.pressed(MouseButton::Left) || p.sprinting || vm.swap > 0.3 {
        return;
    }
    let Some(w) = combat.sb.get(pid).and_then(|c| c.weapon()).map(|w| (w.def.clone(), w.can_fire())) else { return };
    let (def, ready) = w;
    match ready {
        Ok(()) => {}
        Err(FireError::EmptyMagazine) => {
            if combat.sb.reload(pid) {
                vm.reload = 1.0;
            }
            return;
        }
        Err(_) => return,
    }

    let (origin, dir) = aim_ray(&p, t);
    let muzzle = vm.muzzle_world;
    let element = def.damage_type;
    let spread = |rng: &mut Rng, s: f32| (dir + rng.dir() * s).normalize();
    let mut hit_any = false;
    match def.archetype {
        WeaponArchetype::RocketLauncher => {
            if combat.sb.fire_weapon(pid, None, Shot::body()).is_err() {
                return;
            }
            let mesh = fx.rocket_mesh(&mut meshes);
            commands.spawn((
                MatchEntity,
                Rocket {
                    vel: dir * 42.0,
                    life: 2.5,
                    direct: def.damage,
                    blast: def.blast.map_or(0.0, |b| b.damage),
                    radius: def.blast.map_or(4.0, |b| b.radius),
                    element,
                    anti: def.anti_champion,
                },
                Mesh3d(mesh),
                MeshMaterial3d(fx.glow(element)),
                Transform::from_translation(muzzle).looking_to(dir, Vec3::Y),
                PointLight { color: Color::srgb(1.0, 0.6, 0.3), intensity: 80_000.0, range: 8.0, ..default() },
            ));
            p.recoil += 2.5;
            p.shake = p.shake.max(0.3);
        }
        WeaponArchetype::Shotgun => {
            let mut counts: Vec<(CombatantId, u32, bool, f32)> = Vec::new();
            let s = 0.05 - 0.015 * p.ads;
            for _ in 0..def.pellets {
                let d = spread(&mut rng, s);
                let h = raycast(origin, d, 40.0, &level, &enemies, 1.0);
                fx.spawn(&mut commands, &mut meshes, Fx::tracer(muzzle, origin + d * h.dist, element));
                if let Some((id, crit)) = h.enemy {
                    match counts.iter_mut().find(|c| c.0 == id) {
                        Some(c) => {
                            c.1 += 1;
                            c.2 |= crit;
                        }
                        None => counts.push((id, 1, crit, h.dist)),
                    }
                    fx.spawn(&mut commands, &mut meshes, Fx::sparks(origin + d * h.dist, element, 3));
                } else if h.dist < 40.0 {
                    fx.spawn(&mut commands, &mut meshes, Fx::sparks(origin + d * h.dist, DamageType::Kinetic, 2));
                }
            }
            let best = counts.iter().max_by_key(|c| c.1).copied();
            let shot = Shot {
                precision: best.is_some_and(|b| b.2),
                distance: best.map(|b| b.3),
                pellets_hit: Some(best.map_or(0, |b| b.1)),
            };
            if combat.sb.fire_weapon(pid, best.map(|b| b.0), shot).is_ok() {
                hit_any = best.is_some();
                marker.crit = shot.precision;
            }
            p.recoil += 1.8;
            p.shake = p.shake.max(0.15);
        }
        _ => {
            let d = spread(&mut rng, 0.012 * (1.0 - p.ads) + 0.0015);
            let h = raycast(origin, d, 120.0, &level, &enemies, 1.0);
            let shot = Shot { precision: h.enemy.is_some_and(|e| e.1), distance: Some(h.dist), pellets_hit: None };
            if combat.sb.fire_weapon(pid, h.enemy.map(|e| e.0), shot).is_ok() {
                let end = origin + d * h.dist;
                fx.spawn(&mut commands, &mut meshes, Fx::tracer(muzzle, end, element));
                let el = if h.enemy.is_some() { element } else { DamageType::Kinetic };
                fx.spawn(&mut commands, &mut meshes, Fx::sparks(end, el, 5));
                hit_any = h.enemy.is_some();
                marker.crit = shot.precision;
            }
            p.recoil += 1.0;
        }
    }
    vm.fire = 1.0;
    if hit_any {
        marker.t = 0.15;
    }
}

/// Works out where an ability or super attack should land. `None` means
/// nothing to hit (a melee with no one close enough).
#[allow(clippy::too_many_arguments)]
pub fn ability_target(
    p: &Player,
    t: &Transform,
    range: f32,
    close: bool,
    level: &Level,
    enemies: &Query<(&Enemy, &Transform)>,
) -> Option<AbilityTarget> {
    if range <= 0.0 {
        return Some(AbilityTarget::default());
    }
    let (origin, dir) = aim_ray(p, t);
    if close {
        let fwd = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let me = t.translation;
        return enemies
            .iter()
            .filter(|(e, _)| e.targetable())
            .filter_map(|(e, et)| {
                let to = et.translation - me;
                let flat = Vec3::new(to.x, 0.0, to.z);
                let d = flat.length();
                (d <= range + 1.2 + e.scale * 0.4 && flat.normalize_or_zero().dot(fwd) > 0.55).then_some((e.id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id.into());
    }
    let h = raycast(origin, dir, range + 2.0, level, enemies, 1.6);
    if let Some((id, _)) = h.enemy {
        return Some(id.into());
    }
    let mut point = origin + dir * h.dist.min(range);
    if h.dist >= range {
        // Nothing in the way: land on the floor below the aim point.
        point.y = level.floor_at(point.with_y(point.y.max(0.0) + 0.5), 0.1);
    }
    Some(a3(point).into())
}

fn first_move(def_effects: &[Effect]) -> Option<(MoveKind, f32)> {
    def_effects.iter().find_map(|e| match e {
        Effect::Move { kind: k @ (MoveKind::Lunge | MoveKind::Slam), distance } => Some((*k, *distance)),
        _ => None,
    })
}

/// The class model's animation for a super or super attack, by name.
fn super_anim(name: &str, heavy: bool) -> &'static str {
    let n = name.to_lowercase();
    if heavy {
        return if n.contains("blink") || n.contains("grapple") { "Dodge_Forward" } else { "2H_Melee_Attack_Spin" };
    }
    if n.contains("golden gun")
        || n.contains("deadshot")
        || n.contains("marksman")
        || n.contains("shadowshot")
        || n.contains("quiver")
    {
        "1H_Ranged_Shoot"
    } else if n.contains("hammer")
        || n.contains("throw")
        || n.contains("barrage")
        || n.contains("arsenal")
        || n.contains("squall")
        || (n.contains("storm") && n.contains("gathering"))
        || n.contains("sword of light")
        || n.contains("daybreak")
    {
        "Throw"
    } else if n.contains("slam") || n.contains("quake") || n.contains("maul") || n.contains("thundercrash") {
        "2H_Melee_Attack_Chop"
    } else if n.contains("strike")
        || n.contains("slash")
        || n.contains("bash")
        || n.contains("dart")
        || n.contains("blade")
        || n.contains("staff")
        || n.contains("fists")
    {
        "1H_Melee_Attack_Chop"
    } else if n.contains("ward") || n.contains("sentinel") {
        "Block"
    } else if n.contains("well") || n.contains("song") {
        "Spellcast_Raise"
    } else {
        "Spellcast_Shoot"
    }
}

#[allow(clippy::too_many_arguments)]
fn abilities(
    keys: Res<ButtonInput<KeyCode>>,
    grab: Res<Grab>,
    level: Res<Level>,
    mut combat: ResMut<Combat>,
    player: Single<(&mut Player, &Transform)>,
    enemies: Query<(&Enemy, &Transform)>,
    models: Res<Models>,
    mut rigs: Query<&mut Rig>,
    mut anim_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    mut vm: Single<&mut Viewmodel>,
    mut fx: ResMut<FxAssets>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut casts: MessageWriter<CastNow>,
) {
    let (mut p, t) = player.into_inner();
    if !grab.0 || combat.over || p.forced.is_some() || p.pending.is_some() {
        return;
    }
    let pid = combat.player;
    let Some(me) = combat.sb.get(pid) else { return };
    if me.in_super() {
        return;
    }
    let slot = if keys.just_pressed(KeyCode::KeyQ) {
        AbilitySlot::Grenade
    } else if keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::KeyV) {
        AbilitySlot::Melee
    } else if keys.just_pressed(KeyCode::KeyC) {
        AbilitySlot::ClassAbility
    } else if keys.just_pressed(KeyCode::KeyF) {
        AbilitySlot::Super
    } else {
        return;
    };
    let Some(state) = me.loadout.get(slot) else { return };
    let def: AbilityDef = state.def.clone();
    if !state.is_ready() {
        combat.hint(format!("{} recharging", def.name));
        return;
    }
    if me.statuses.restrictions(&combat.sb.config.statuses).abilities {
        combat.hint("Abilities suppressed");
        return;
    }
    let close = slot == AbilitySlot::Melee && def.range <= 8.0;
    let Some(target) = ability_target(&p, t, def.range, close, &level, &enemies) else {
        combat.hint("No target in melee range");
        return;
    };
    let target_pos = target
        .point
        .map(v3)
        .or_else(|| target.combatant.and_then(|id| combat.sb.get(id)).and_then(|c| c.position).map(v3));
    let cast = PendingCast { what: CastKind::Ability(slot), target, delay: 0.0 };

    let body = p.body;
    let mut play = |name: &str| {
        if let Ok(mut rig) = rigs.get_mut(body) {
            models::play(&mut rig, &models, &mut anim_players, name, false, 1.0);
        }
    };

    // Melee lunges and slam supers move first, then hit.
    if let (Some((kind, dist)), Some(dest)) = (first_move(&def.effects), target_pos) {
        let from = t.translation;
        let to_flat = Vec3::new(dest.x - from.x, 0.0, dest.z - from.z);
        let stop = if kind == MoveKind::Lunge { 1.3 } else { 0.0 };
        let travel = (to_flat.length() - stop).clamp(0.0, dist);
        let mut to = from + to_flat.normalize_or_zero() * travel;
        to.y = level.floor_at(to, 0.4);
        let (dur, arc) = if kind == MoveKind::Slam { (0.8, 6.0) } else { (0.16, 0.2) };
        if slot == AbilitySlot::Super {
            p.third_timer = dur + 1.3;
            play("Jump_Idle");
        } else {
            vm.melee = 1.0;
        }
        p.forced = Some(ForcedMove { from, to, t: 0.0, dur, arc, then: Some(cast) });
        return;
    }

    match slot {
        AbilitySlot::Super if def.roaming().is_none() => {
            // Thrown and cast supers: a short third-person moment, then the hit.
            p.third_timer = 1.6;
            play(super_anim(&def.name, false));
            p.pending = Some(PendingCast { delay: 0.55, ..cast });
        }
        AbilitySlot::Super => {
            play("Spellcast_Raise");
            casts.write(CastNow(cast));
        }
        AbilitySlot::Grenade if def.is_targeted() || def.range > 0.0 => {
            // Lob the grenade; it goes off where it lands.
            vm.cast = 1.0;
            vm.cast_element = def.damage_type;
            let from = vm.muzzle_world - Vec3::Y * 0.1;
            let to = target_pos.unwrap_or(t.translation);
            let flight = (from.distance(to) / 28.0).clamp(0.15, 0.7);
            fx.spawn(&mut commands, &mut meshes, Fx::lob(from, to, flight, def.damage_type));
            p.pending = Some(PendingCast { delay: flight, ..cast });
        }
        AbilitySlot::Melee => {
            vm.melee = 1.0;
            casts.write(CastNow(cast));
        }
        _ => {
            vm.cast = 1.0;
            vm.cast_element = def.damage_type;
            casts.write(CastNow(cast));
        }
    }
}

fn tick_pending(time: Res<Time>, mut player: Single<&mut Player>, mut casts: MessageWriter<CastNow>) {
    if let Some(c) = &mut player.pending {
        c.delay -= time.delta_secs();
        if c.delay <= 0.0 {
            let c = player.pending.take().expect("checked");
            casts.write(CastNow(c));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cast_now(
    mut msgs: MessageReader<CastNow>,
    mut combat: ResMut<Combat>,
    level: Res<Level>,
    player: Single<(&mut Player, &Transform)>,
    enemies: Query<(&Enemy, &Transform)>,
    models: Res<Models>,
    mut rigs: Query<&mut Rig>,
    mut anim_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let (mut p, t) = player.into_inner();
    let pid = combat.player;
    for CastNow(cast) in msgs.read() {
        match cast.what {
            CastKind::Ability(slot) => match combat.sb.use_ability(pid, slot, cast.target) {
                Ok(()) => {}
                Err(ActionError::NotReady) => combat.hint("Not ready"),
                Err(ActionError::Restricted) => combat.hint("Abilities suppressed"),
                Err(e) => combat.hint(format!("{e:?}")),
            },
            CastKind::SuperAttack { heavy } => {
                let Some(m) = combat.sb.get(pid).and_then(|c| c.super_mode.clone()) else { continue };
                let attack = if heavy { m.def.heavy.clone() } else { Some(m.def.light.clone()) };
                let Some(attack) = attack else { continue };
                let ready = if heavy { m.heavy_cooldown <= 0.0 } else { m.light_cooldown <= 0.0 };
                if !ready {
                    continue;
                }
                let close = attack.range > 0.0 && attack.range <= 8.0;
                let target = ability_target(&p, t, attack.range, close, &level, &enemies).unwrap_or_default();
                // Lunging attacks close the gap first.
                if let Some((MoveKind::Lunge, dist)) = first_move(&attack.effects) {
                    if let Some(dest) =
                        target.combatant.and_then(|id| combat.sb.get(id)).and_then(|c| c.position).map(v3)
                    {
                        let from = t.translation;
                        let flat = Vec3::new(dest.x - from.x, 0.0, dest.z - from.z);
                        let travel = (flat.length() - 1.4).clamp(0.0, dist);
                        let to = from + flat.normalize_or_zero() * travel;
                        if travel > 0.5 {
                            p.forced = Some(ForcedMove { from, to, t: 0.0, dur: 0.12, arc: 0.1, then: None });
                        }
                    }
                }
                let result = combat.sb.super_attack(pid, heavy, target);
                let line = format!(
                    "{} heavy={heavy} target={:?} point={:?} -> {result:?}",
                    attack.name,
                    target.combatant,
                    target.point.map(|p| [p[0] as i32, p[1] as i32, p[2] as i32])
                );
                combat.cast_log.push_back(line);
                if combat.cast_log.len() > 8 {
                    combat.cast_log.pop_front();
                }
                if result.is_ok() {
                    if let Ok(mut rig) = rigs.get_mut(p.body) {
                        models::play(&mut rig, &models, &mut anim_players, super_anim(&attack.name, heavy), false, 1.6);
                    }
                }
            }
        }
    }
}

/// Moves requested by the sandbox (dodges, blinks, grapples, hops).
fn apply_moves(
    frame: Res<FrameEvents>,
    combat: Res<Combat>,
    level: Res<Level>,
    keys: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Player, &Transform)>,
) {
    let (mut p, t) = player.into_inner();
    for e in &frame.0 {
        let CombatEvent::Move { user, kind, distance, toward } = e else { continue };
        if *user != combat.player || p.forced.is_some() {
            continue;
        }
        let from = t.translation;
        let fwd = Quat::from_rotation_y(p.yaw) * Vec3::NEG_Z;
        let mut wish = Vec3::ZERO;
        for (k, d) in [
            (KeyCode::KeyW, Vec3::NEG_Z),
            (KeyCode::KeyS, Vec3::Z),
            (KeyCode::KeyA, Vec3::NEG_X),
            (KeyCode::KeyD, Vec3::X),
        ] {
            if keys.pressed(k) {
                wish += d;
            }
        }
        let wish = Quat::from_rotation_y(p.yaw) * wish.normalize_or_zero();
        let (dir, dist, dur, arc) = match kind {
            MoveKind::Dash => (if wish == Vec3::ZERO { fwd } else { wish }, *distance, 0.22, 0.25),
            MoveKind::Blink => (fwd, *distance, 0.07, 0.0),
            MoveKind::Grapple => {
                let to = toward.map(v3).unwrap_or(from + fwd * *distance);
                let d = to - from;
                (d.normalize_or_zero(), d.length().min(*distance), (d.length() / 30.0).max(0.15), 2.0)
            }
            MoveKind::Hop => {
                p.vel.y = 7.5;
                continue;
            }
            MoveKind::Lunge | MoveKind::Slam => continue,
        };
        let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let max = level.ray(from + Vec3::Y * 0.8, flat, dist).map_or(dist, |d| (d - 0.6).max(0.0));
        let mut to = from + flat * max;
        to.y = level.floor_at(to, 0.4).max(if *kind == MoveKind::Grapple { from.y + dir.y * dist } else { 0.0 });
        p.forced = Some(ForcedMove { from, to, t: 0.0, dur, arc, then: None });
    }
}

#[allow(clippy::too_many_arguments)]
fn fly_rockets(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    mut combat: ResMut<Combat>,
    mut rockets: Query<(Entity, &mut Rocket, &mut Transform), Without<Enemy>>,
    enemies: Query<(&Enemy, &Transform)>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut player: Single<&mut Player>,
) {
    let dt = time.delta_secs();
    for (e, mut r, mut t) in &mut rockets {
        r.life -= dt;
        let step = r.vel * dt;
        let len = step.length();
        let dir = step / len.max(1e-6);
        let h = raycast(t.translation, dir, len, &level, &enemies, 1.0);
        fx.spawn(&mut commands, &mut meshes, Fx::smoke(t.translation));
        if h.dist >= len && r.life > 0.0 {
            t.translation += step;
            continue;
        }
        let at = t.translation + dir * h.dist;
        commands.entity(e).despawn();
        let pid = combat.player;
        let kind = SourceKind::Weapon { ammo: AmmoType::Heavy, archetype: WeaponArchetype::RocketLauncher };
        let direct = h.enemy.map(|x| x.0);
        if let Some(id) = direct {
            let inst = DamageInstance::new(r.direct + r.blast, r.element, kind).from(pid).anti_champion(r.anti);
            combat.sb.deal_damage(id, inst);
        }
        for (en, et) in &enemies {
            if Some(en.id) == direct || !en.targetable() {
                continue;
            }
            let d = (et.translation + Vec3::Y * 0.9 * en.scale).distance(at);
            if d <= r.radius + 0.5 * en.scale {
                let falloff = 1.0 - 0.5 * (d / r.radius).min(1.0);
                let inst = DamageInstance::new(r.blast * falloff, r.element, kind).from(pid).anti_champion(r.anti);
                combat.sb.deal_damage(en.id, inst);
            }
        }
        fx.spawn(&mut commands, &mut meshes, Fx::blast(at, r.radius, r.element));
        let dist = player_dist(&combat, at);
        player.shake = player.shake.max((1.0 - dist / 25.0).clamp(0.0, 0.8));
    }
}

fn player_dist(combat: &Combat, at: Vec3) -> f32 {
    combat.sb.get(combat.player).and_then(|c| c.position).map_or(99.0, |p| v3(p).distance(at))
}

fn hitmarkers(
    time: Res<Time>,
    frame: Res<FrameEvents>,
    combat: Res<Combat>,
    mut marker: ResMut<Hitmarker>,
    mut player: Single<&mut Player>,
) {
    let dt = time.delta_secs();
    marker.t = (marker.t - dt).max(0.0);
    marker.kill = (marker.kill - dt).max(0.0);
    for e in &frame.0 {
        match e {
            CombatEvent::Killed { killer, .. } if *killer == Some(combat.player) => marker.kill = 0.35,
            CombatEvent::Damaged { target, absorbed, .. } if *target == combat.player && absorbed.total() > 0.5 => {
                player.hurt = 0.6;
                player.shake = player.shake.max(0.25);
            }
            _ => {}
        }
    }
}
