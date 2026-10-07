//! Visual effects: tracers, sparks, explosions, beams, lobbed grenades,
//! spawn portals, ability zones, and status/shield/champion auras on enemies.

use std::collections::HashMap;
use std::f32::consts::PI;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use guardian_combat::combatant::ChampionKind;
use guardian_combat::element::DamageType;
use guardian_combat::sandbox::{CombatEvent, ZoneId};
use guardian_combat::status::{StatusKind, TriggerKind};
use guardian_combat::weapon::AmmoType;

use crate::combat::Combat;
use crate::common::{element_color, glow, v3, AppState, FrameEvents, MatchEntity, Phase, Rng};
use crate::enemies::{Enemy, EnemyState};
use crate::player::Player;

const MAX_PARTICLES: usize = 450;

/// Shared meshes and materials for effects.
#[derive(Resource)]
pub struct FxAssets {
    sphere: Option<Handle<Mesh>>,
    cube: Option<Handle<Mesh>>,
    cylinder: Option<Handle<Mesh>>,
    torus: Option<Handle<Mesh>>,
    rocket: Option<Handle<Mesh>>,
    glows: HashMap<DamageType, Handle<StandardMaterial>>,
    ghosts: HashMap<DamageType, Handle<StandardMaterial>>,
    pub orb: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    special: Handle<StandardMaterial>,
    heavy: Handle<StandardMaterial>,
    barrier: Handle<StandardMaterial>,
    gold_ghost: Handle<StandardMaterial>,
    champion: HashMap<ChampionKind, Handle<StandardMaterial>>,
    queue: Vec<Fx>,
    particles: usize,
}

impl FxAssets {
    pub fn sphere(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.sphere.get_or_insert_with(|| meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap())).clone()
    }

    pub fn cube(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.cube.get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0))).clone()
    }

    fn cylinder(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.cylinder.get_or_insert_with(|| meshes.add(Cylinder::new(1.0, 1.0))).clone()
    }

    fn torus(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.torus.get_or_insert_with(|| meshes.add(Torus::new(0.9, 1.0))).clone()
    }

    pub fn rocket_mesh(&mut self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.rocket
            .get_or_insert_with(|| {
                meshes.add(Capsule3d::new(0.07, 0.35).mesh().build().rotated_by(Quat::from_rotation_x(PI / 2.0)))
            })
            .clone()
    }

    pub fn glow(&self, t: DamageType) -> Handle<StandardMaterial> {
        self.glows[&t].clone()
    }

    pub fn ghost(&self, t: DamageType) -> Handle<StandardMaterial> {
        self.ghosts[&t].clone()
    }

    pub fn ammo(&self, a: AmmoType) -> Handle<StandardMaterial> {
        if a == AmmoType::Heavy {
            self.heavy.clone()
        } else {
            self.special.clone()
        }
    }

    /// Queues an effect; it is created at the end of the frame.
    pub fn spawn(&mut self, _commands: &mut Commands, _meshes: &mut Assets<Mesh>, fx: Fx) {
        self.queue.push(fx);
    }
}

/// An effect to create.
#[derive(Clone, Debug)]
pub enum Fx {
    Tracer { from: Vec3, to: Vec3, el: DamageType },
    Sparks { at: Vec3, el: DamageType, n: u32 },
    Blast { at: Vec3, radius: f32, el: DamageType },
    Beam { from: Vec3, to: Vec3, el: DamageType },
    Lob { from: Vec3, to: Vec3, flight: f32, el: DamageType },
    Portal { at: Vec3, scale: f32 },
    Smoke { at: Vec3 },
}

impl Fx {
    pub fn tracer(from: Vec3, to: Vec3, el: DamageType) -> Self {
        Fx::Tracer { from, to, el }
    }
    pub fn sparks(at: Vec3, el: DamageType, n: u32) -> Self {
        Fx::Sparks { at, el, n }
    }
    pub fn blast(at: Vec3, radius: f32, el: DamageType) -> Self {
        Fx::Blast { at, radius, el }
    }
    pub fn lob(from: Vec3, to: Vec3, flight: f32, el: DamageType) -> Self {
        Fx::Lob { from, to, flight, el }
    }
    pub fn portal(at: Vec3, scale: f32) -> Self {
        Fx::Portal { at, scale }
    }
    pub fn smoke(at: Vec3) -> Self {
        Fx::Smoke { at }
    }
}

#[derive(Component)]
struct Life {
    age: f32,
    life: f32,
    kind: LifeKind,
}

enum LifeKind {
    Shrink,
    Particle { vel: Vec3, gravity: f32 },
    Grow { from: f32, to: f32, fade: Option<Handle<StandardMaterial>>, base: LinearRgba },
    Lob { from: Vec3, to: Vec3 },
    Light,
}

#[derive(Component)]
struct ZoneVisual(ZoneId);

#[derive(Component)]
pub struct EnemyAura {
    shield: Entity,
    status: Entity,
    ring: Entity,
}

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (events_to_fx, zones, attach_auras, update_auras, status_particles, spawn_queued, animate_fx)
                .chain()
                .in_set(Phase::React)
                .run_if(in_state(AppState::Playing)),
        );
    }
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut glows = HashMap::new();
    let mut ghosts = HashMap::new();
    for t in [
        DamageType::Kinetic,
        DamageType::Arc,
        DamageType::Solar,
        DamageType::Void,
        DamageType::Stasis,
        DamageType::Strand,
    ] {
        glows.insert(
            t,
            materials.add(StandardMaterial {
                base_color: element_color(t),
                emissive: glow(t, 12.0),
                unlit: true,
                ..default()
            }),
        );
        ghosts.insert(
            t,
            materials.add(StandardMaterial {
                base_color: element_color(t).with_alpha(0.16),
                emissive: glow(t, 0.8),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            }),
        );
    }
    let ghost = |c: Color, k: f32, materials: &mut Assets<StandardMaterial>| {
        let l = c.to_linear();
        materials.add(StandardMaterial {
            base_color: c.with_alpha(0.2),
            emissive: LinearRgba::rgb(l.red * k, l.green * k, l.blue * k),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    };
    let champion = HashMap::from([
        (ChampionKind::Barrier, ghost(Color::srgb(0.4, 0.8, 1.0), 4.0, &mut materials)),
        (ChampionKind::Overload, ghost(Color::srgb(0.8, 0.3, 1.0), 4.0, &mut materials)),
        (ChampionKind::Unstoppable, ghost(Color::srgb(1.0, 0.45, 0.15), 4.0, &mut materials)),
    ]);
    commands.insert_resource(FxAssets {
        sphere: None,
        cube: None,
        cylinder: None,
        torus: None,
        rocket: None,
        glows,
        ghosts,
        orb: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.4),
            emissive: LinearRgba::rgb(9.0, 6.5, 2.0),
            unlit: true,
            ..default()
        }),
        smoke: materials.add(StandardMaterial {
            base_color: Color::srgba(0.6, 0.6, 0.62, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        special: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 1.0, 0.5),
            emissive: LinearRgba::rgb(0.6, 4.0, 1.2),
            ..default()
        }),
        heavy: materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.4, 1.0),
            emissive: LinearRgba::rgb(2.6, 0.9, 4.5),
            ..default()
        }),
        barrier: ghost(Color::srgb(0.7, 0.9, 1.0), 2.5, &mut materials),
        gold_ghost: ghost(Color::srgb(1.0, 0.8, 0.35), 1.6, &mut materials),
        champion,
        queue: Vec::new(),
        particles: 0,
    });
}

fn events_to_fx(frame: Res<FrameEvents>, combat: Res<Combat>, mut fx: ResMut<FxAssets>, mut rng: ResMut<Rng>) {
    for e in &frame.0 {
        match e {
            CombatEvent::Blast { center, radius, damage_type } => {
                fx.queue.push(Fx::blast(v3(*center), *radius, *damage_type))
            }
            CombatEvent::Beam { from, to, damage_type } => {
                fx.queue.push(Fx::Beam { from: v3(*from), to: v3(*to), el: *damage_type })
            }
            CombatEvent::Triggered { kind, origin, .. } => {
                let Some(p) = combat.sb.get(*origin).and_then(|c| c.position) else { continue };
                let el = match kind {
                    TriggerKind::Ignite => DamageType::Solar,
                    TriggerKind::JoltChain => DamageType::Arc,
                    TriggerKind::VolatileExplosion => DamageType::Void,
                    TriggerKind::Shatter => DamageType::Stasis,
                    TriggerKind::UnravelThreads => DamageType::Strand,
                };
                fx.queue.push(Fx::sparks(v3(p), el, 14));
            }
            CombatEvent::ShieldBroken { target, element, .. } => {
                let Some(p) = combat.sb.get(*target).and_then(|c| c.position) else { continue };
                fx.queue.push(Fx::sparks(v3(p), element.unwrap_or(DamageType::Kinetic), 18));
            }
            CombatEvent::Healed { target, amount } if *target == combat.player && *amount > 5.0 => {
                if let Some(p) = combat.sb.get(*target).and_then(|c| c.position) {
                    let at = v3(p) + Vec3::new(rng.range(-0.5, 0.5), 0.4, rng.range(-0.5, 0.5));
                    fx.queue.push(Fx::sparks(at, DamageType::Strand, 3));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_queued(
    mut commands: Commands,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rng: ResMut<Rng>,
) {
    let queue = std::mem::take(&mut fx.queue);
    for f in queue {
        match f {
            Fx::Tracer { from, to, el } => {
                let d = to - from;
                let len = d.length();
                if len < 0.05 {
                    continue;
                }
                let mesh = fx.cube(&mut meshes);
                commands.spawn((
                    MatchEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(fx.glow(el)),
                    Transform::from_translation(from + d / 2.0)
                        .looking_to(d, Vec3::Y)
                        .with_scale(Vec3::new(0.018, 0.018, len)),
                    NotShadowCaster,
                    Life { age: 0.0, life: 0.07, kind: LifeKind::Shrink },
                ));
            }
            Fx::Sparks { at, el, n } => {
                let mesh = fx.cube(&mut meshes);
                for _ in 0..n {
                    if fx.particles >= MAX_PARTICLES {
                        break;
                    }
                    fx.particles += 1;
                    let vel = rng.dir() * rng.range(2.0, 7.0) + Vec3::Y * 2.0;
                    commands.spawn((
                        MatchEntity,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(fx.glow(el)),
                        Transform::from_translation(at).with_scale(Vec3::splat(rng.range(0.04, 0.09))),
                        NotShadowCaster,
                        Life { age: 0.0, life: rng.range(0.25, 0.6), kind: LifeKind::Particle { vel, gravity: 9.0 } },
                    ));
                }
            }
            Fx::Smoke { at } => {
                if fx.particles >= MAX_PARTICLES {
                    continue;
                }
                fx.particles += 1;
                let mesh = fx.sphere(&mut meshes);
                commands.spawn((
                    MatchEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(fx.smoke.clone()),
                    Transform::from_translation(at).with_scale(Vec3::splat(0.12)),
                    NotShadowCaster,
                    Life {
                        age: 0.0,
                        life: 0.7,
                        kind: LifeKind::Particle { vel: Vec3::Y * 0.6 + rng.dir() * 0.3, gravity: -0.5 },
                    },
                ));
            }
            Fx::Blast { at, radius, el } => {
                let base = glow(el, 6.0);
                let mat = materials.add(StandardMaterial {
                    base_color: element_color(el).with_alpha(0.5),
                    emissive: base,
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    ..default()
                });
                let mesh = fx.sphere(&mut meshes);
                commands.spawn((
                    MatchEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(at).with_scale(Vec3::splat(radius * 0.3)),
                    NotShadowCaster,
                    Life {
                        age: 0.0,
                        life: 0.45,
                        kind: LifeKind::Grow { from: radius * 0.3, to: radius, fade: Some(mat), base },
                    },
                ));
                commands.spawn((
                    MatchEntity,
                    PointLight {
                        color: element_color(el),
                        intensity: 400_000.0 * radius.min(6.0),
                        range: radius * 4.0,
                        ..default()
                    },
                    Transform::from_translation(at + Vec3::Y * 0.5),
                    Life { age: 0.0, life: 0.3, kind: LifeKind::Light },
                ));
                fx.queue.push(Fx::sparks(at, el, (radius * 3.0) as u32 + 4));
            }
            Fx::Beam { from, to, el } => {
                let mesh = fx.cube(&mut meshes);
                let jag = matches!(el, DamageType::Arc);
                let segs = if jag { 5 } else { 1 };
                let mut a = from;
                for i in 1..=segs {
                    let k = i as f32 / segs as f32;
                    let mut b = from.lerp(to, k);
                    if jag && i < segs {
                        b += rng.dir() * 0.35;
                    }
                    let d = b - a;
                    commands.spawn((
                        MatchEntity,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(fx.glow(el)),
                        Transform::from_translation(a + d / 2.0).looking_to(d, Vec3::Y).with_scale(Vec3::new(
                            0.05,
                            0.05,
                            d.length(),
                        )),
                        NotShadowCaster,
                        Life { age: 0.0, life: 0.22, kind: LifeKind::Shrink },
                    ));
                    a = b;
                }
            }
            Fx::Lob { from, to, flight, el } => {
                let mesh = fx.sphere(&mut meshes);
                commands.spawn((
                    MatchEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(fx.glow(el)),
                    Transform::from_translation(from).with_scale(Vec3::splat(0.12)),
                    NotShadowCaster,
                    PointLight { color: element_color(el), intensity: 30_000.0, range: 5.0, ..default() },
                    Life { age: 0.0, life: flight, kind: LifeKind::Lob { from, to } },
                ));
            }
            Fx::Portal { at, scale } => {
                let mesh = fx.torus(&mut meshes);
                let mat = materials.add(StandardMaterial {
                    base_color: Color::srgba(0.3, 1.0, 0.5, 0.8),
                    emissive: LinearRgba::rgb(0.8, 8.0, 2.0),
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    ..default()
                });
                commands.spawn((
                    MatchEntity,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(at + Vec3::Y * 0.05).with_scale(Vec3::new(1.0, 0.05, 1.0) * scale),
                    NotShadowCaster,
                    Life {
                        age: 0.0,
                        life: 1.6,
                        kind: LifeKind::Grow {
                            from: 0.3 * scale,
                            to: 1.6 * scale,
                            fade: Some(mat),
                            base: LinearRgba::rgb(0.8, 8.0, 2.0),
                        },
                    },
                ));
                commands.spawn((
                    MatchEntity,
                    PointLight { color: Color::srgb(0.3, 1.0, 0.5), intensity: 200_000.0, range: 8.0, ..default() },
                    Transform::from_translation(at + Vec3::Y),
                    Life { age: 0.0, life: 1.2, kind: LifeKind::Light },
                ));
                fx.queue.push(Fx::sparks(at + Vec3::Y * 0.3, DamageType::Strand, 8));
            }
        }
    }
}

fn animate_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FxAssets>,
    mut q: Query<(Entity, &mut Life, &mut Transform, Option<&mut PointLight>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (e, mut l, mut t, light) in &mut q {
        l.age += dt;
        let k = (l.age / l.life).min(1.0);
        if k >= 1.0 {
            if matches!(l.kind, LifeKind::Particle { .. }) {
                fx.particles = fx.particles.saturating_sub(1);
            }
            if let LifeKind::Grow { fade: Some(m), .. } = &l.kind {
                materials.remove(m);
            }
            commands.entity(e).despawn();
            continue;
        }
        match &mut l.kind {
            LifeKind::Shrink => {
                t.scale.x *= 1.0 - 0.6 * k;
                t.scale.y *= 1.0 - 0.6 * k;
            }
            LifeKind::Particle { vel, gravity } => {
                vel.y -= *gravity * dt;
                t.translation += *vel * dt;
                t.scale *= 1.0 - dt * 2.0;
            }
            LifeKind::Grow { from, to, fade, base } => {
                let s = *from + (*to - *from) * (1.0 - (1.0 - k) * (1.0 - k));
                let flat = t.scale.y < t.scale.x * 0.2;
                t.scale = if flat { Vec3::new(s, s * 0.05, s) } else { Vec3::splat(s) };
                if let Some(mut m) = fade.as_ref().and_then(|h| materials.get_mut(h)) {
                    let a = 1.0 - k;
                    m.emissive = LinearRgba::rgb(base.red * a, base.green * a, base.blue * a);
                }
            }
            LifeKind::Lob { from, to } => {
                let mut p = from.lerp(*to, k);
                p.y += 3.0 * 4.0 * k * (1.0 - k) * (from.distance(*to) / 15.0).clamp(0.3, 1.5);
                t.translation = p;
            }
            LifeKind::Light => {}
        }
        if let Some(mut light) = light {
            light.intensity *= 1.0 - (dt * 6.0).min(0.9);
        }
    }
}

/// Keeps one visual per sandbox zone (rifts, wells, bubbles, barricades...).
#[allow(clippy::too_many_arguments)]
fn zones(
    mut commands: Commands,
    combat: Res<Combat>,
    time: Res<Time>,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    player: Single<&Transform, With<Player>>,
    mut visuals: Query<(Entity, &ZoneVisual, &mut Transform), Without<Player>>,
) {
    let live: HashMap<ZoneId, _> = combat.sb.zones().map(|z| (z.id, z)).collect();
    let mut shown = Vec::new();
    for (e, v, mut t) in &mut visuals {
        match live.get(&v.0) {
            Some(z) => {
                shown.push(v.0);
                if let Some(c) = z.center {
                    let c = v3(c);
                    t.translation.x = c.x;
                    t.translation.z = c.z;
                }
                let pulse = 1.0 + 0.03 * (time.elapsed_secs() * 4.0).sin();
                let fade = (z.remaining / 0.4).min(1.0);
                if !z.def.blocks_projectiles || z.def.radius >= 5.0 {
                    let r = z.def.radius * pulse * fade;
                    t.scale = Vec3::new(r, t.scale.y.max(0.01), r);
                }
            }
            None => commands.entity(e).despawn(),
        }
    }
    for (id, z) in live {
        if shown.contains(&id) {
            continue;
        }
        let Some(c) = z.center.map(v3) else { continue };
        let ground = Vec3::new(c.x, (c.y - 1.0).max(0.0), c.z);
        let name = z.name.to_lowercase();
        let el = z.damage_type;
        let is_gold = name.contains("rift") || name.contains("well") || el == DamageType::Kinetic;
        let ghost = if is_gold { fx.gold_ghost.clone() } else { fx.ghost(el) };
        let r = z.def.radius;
        let entity =
            if z.def.blocks_projectiles && r < 5.0 {
                // Barricades: a wall between the player and the enemies.
                let to = (c - player.translation).with_y(0.0).normalize_or(Vec3::NEG_Z);
                let mesh = fx.cube(&mut meshes);
                commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(fx.barrier.clone()),
                        Transform::from_translation(ground + Vec3::Y * 1.3)
                            .looking_to(to, Vec3::Y)
                            .with_scale(Vec3::new(r * 1.4, 2.6, 0.3)),
                    ))
                    .id()
            } else if name.contains("rift") {
                let mesh = fx.cylinder(&mut meshes);
                commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(ghost),
                        Transform::from_translation(ground + Vec3::Y * 0.05).with_scale(Vec3::new(r, 0.1, r)),
                    ))
                    .id()
            } else {
                let mesh = fx.sphere(&mut meshes);
                let squash = if z.def.blocks_projectiles { 1.0 } else { 0.55 };
                commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(ghost),
                        Transform::from_translation(ground).with_scale(Vec3::new(r, r * squash, r)),
                    ))
                    .id()
            };
        commands.entity(entity).insert((MatchEntity, ZoneVisual(id), NotShadowCaster));
        if name.contains("well") {
            let mesh = fx.cube(&mut meshes);
            let sword = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(fx.glow(DamageType::Solar)),
                    Transform::from_xyz(0.0, 1.5 / r, 0.0).with_scale(Vec3::new(0.12, 3.0, 0.4) / r),
                ))
                .id();
            commands.entity(entity).add_child(sword);
        }
        let light = commands
            .spawn((
                PointLight {
                    color: if is_gold { Color::srgb(1.0, 0.8, 0.4) } else { element_color(el) },
                    intensity: 150_000.0,
                    range: r * 2.5,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.0 / r.max(0.1), 0.0),
            ))
            .id();
        commands.entity(entity).add_child(light);
    }
}

fn attach_auras(
    mut commands: Commands,
    mut fx: ResMut<FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    enemies: Query<Entity, (With<Enemy>, Without<EnemyAura>)>,
) {
    for e in &enemies {
        let sphere = fx.sphere(&mut meshes);
        let torus = fx.torus(&mut meshes);
        let shield = commands
            .spawn((
                Mesh3d(sphere.clone()),
                MeshMaterial3d(fx.ghost(DamageType::Arc)),
                Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(0.85, 1.15, 0.85)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        let status = commands
            .spawn((
                Mesh3d(sphere),
                MeshMaterial3d(fx.ghost(DamageType::Arc)),
                Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(0.75, 1.05, 0.75)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        let ring = commands
            .spawn((
                Mesh3d(torus),
                MeshMaterial3d(fx.ghost(DamageType::Arc)),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.9, 0.05, 0.9)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        commands.entity(e).add_children(&[shield, status, ring]).insert(EnemyAura { shield, status, ring });
    }
}

/// The status shown on an enemy, most important first.
fn dominant(statuses: &[StatusKind]) -> Option<DamageType> {
    use StatusKind::*;
    let order = [
        (Frozen, DamageType::Stasis),
        (Suspend, DamageType::Strand),
        (Volatile, DamageType::Void),
        (Jolt, DamageType::Arc),
        (Scorch, DamageType::Solar),
        (Unravel, DamageType::Strand),
        (Weaken, DamageType::Void),
        (Slow, DamageType::Stasis),
        (Sever, DamageType::Strand),
        (Blind, DamageType::Arc),
        (Suppress, DamageType::Void),
    ];
    order.iter().find(|(k, _)| statuses.contains(k)).map(|&(_, t)| t)
}

fn update_auras(
    combat: Res<Combat>,
    fx: Res<FxAssets>,
    time: Res<Time>,
    enemies: Query<(&Enemy, &EnemyAura)>,
    mut parts: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>, &mut Transform)>,
) {
    let flicker = (time.elapsed_secs() * 10.0).sin() > 0.0;
    for (e, aura) in &enemies {
        let Some(c) = combat.sb.get(e.id) else { continue };
        let dying = matches!(e.state, EnemyState::Dying(_));
        let statuses: Vec<StatusKind> = c.statuses.iter().map(|s| s.kind).collect();
        if let Ok((mut v, mut m, _)) = parts.get_mut(aura.shield) {
            let barrier = c.champion.as_ref().is_some_and(|ch| ch.barrier_up);
            if barrier {
                *v = Visibility::Inherited;
                m.0 = fx.barrier.clone();
            } else if c.health.shield > 0.0 && !dying {
                *v = Visibility::Inherited;
                m.0 = fx.ghost(c.health.shield_element.unwrap_or(DamageType::Kinetic));
            } else {
                *v = Visibility::Hidden;
            }
        }
        if let Ok((mut v, mut m, mut t)) = parts.get_mut(aura.status) {
            match dominant(&statuses).filter(|_| !dying) {
                Some(el) => {
                    *v = Visibility::Inherited;
                    m.0 = fx.ghost(el);
                    let frozen = statuses.contains(&StatusKind::Frozen);
                    t.scale = if frozen { Vec3::new(0.95, 1.2, 0.95) } else { Vec3::new(0.75, 1.05, 0.75) };
                }
                None => *v = Visibility::Hidden,
            }
        }
        if let Ok((mut v, mut m, _)) = parts.get_mut(aura.ring) {
            match c.champion.as_ref() {
                Some(ch) if !dying => {
                    let stunned = ch.is_stunned();
                    *v = if stunned && flicker { Visibility::Hidden } else { Visibility::Inherited };
                    m.0 = fx.champion[&ch.kind].clone();
                }
                _ => *v = Visibility::Hidden,
            }
        }
    }
}

fn status_particles(
    combat: Res<Combat>,
    mut fx: ResMut<FxAssets>,
    mut rng: ResMut<Rng>,
    enemies: Query<(&Enemy, &Transform)>,
    time: Res<Time>,
) {
    let chance = (time.delta_secs() * 6.0).min(1.0);
    for (e, t) in &enemies {
        if !e.targetable() {
            continue;
        }
        let Some(c) = combat.sb.get(e.id) else { continue };
        for (k, el) in [
            (StatusKind::Scorch, DamageType::Solar),
            (StatusKind::Jolt, DamageType::Arc),
            (StatusKind::Unravel, DamageType::Strand),
            (StatusKind::Volatile, DamageType::Void),
        ] {
            if c.statuses.has(k) && rng.f() < chance {
                let at = t.translation
                    + Vec3::new(rng.range(-0.4, 0.4), rng.range(0.3, 1.8), rng.range(-0.4, 0.4)) * e.scale;
                fx.queue.push(Fx::sparks(at, el, 1));
            }
        }
    }
}
