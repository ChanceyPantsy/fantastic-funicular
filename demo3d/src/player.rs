//! First-person controller: mouse look, movement, class jumps, collision,
//! forced moves from abilities, and the camera switching to third person
//! for supers and death.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use guardian_combat::element::GuardianClass;

use crate::actions::PendingCast;
use crate::combat::Combat;
use crate::common::{AppState, LoadoutChoice, MatchEntity, Phase};
use crate::level::Level;
use crate::models::{self, Models, Rig};

pub const RADIUS: f32 = 0.4;
pub const HEIGHT: f32 = 1.8;
pub const EYE: f32 = 1.62;
const SPEED: f32 = 6.5;
const GRAVITY: f32 = 22.0;
const JUMP: f32 = 7.2;
pub const BASE_FOV: f32 = 78.0;

/// Movement an ability imposes (dodges, lunges, slams, grapples).
#[derive(Clone)]
pub struct ForcedMove {
    pub from: Vec3,
    pub to: Vec3,
    pub t: f32,
    pub dur: f32,
    pub arc: f32,
    pub then: Option<PendingCast>,
}

#[derive(Component)]
pub struct Player {
    pub class: GuardianClass,
    pub yaw: f32,
    pub pitch: f32,
    pub vel: Vec3,
    pub grounded: bool,
    pub air_jump_used: bool,
    pub lift_fuel: f32,
    pub sprinting: bool,
    pub moving: bool,
    pub body: Entity,
    pub camera: Entity,
    /// 0 = first person, 1 = third person.
    pub third: f32,
    /// Seconds of forced third person (super casts, cinematics).
    pub third_timer: f32,
    pub forced: Option<ForcedMove>,
    pub shake: f32,
    pub ads: f32,
    pub recoil: f32,
    pub hurt: f32,
    pub pending: Option<PendingCast>,
}

#[derive(Component)]
pub struct PlayerCamera;

/// Whether the mouse is captured.
#[derive(Resource, Default)]
pub struct Grab(pub bool);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Grab>().add_systems(
            Update,
            (
                (grab_cursor, look, movement).chain().in_set(Phase::Input),
                (camera_follow, animate_body).chain().in_set(Phase::React),
            )
                .run_if(in_state(AppState::Playing)),
        );
    }
}

pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    start: Vec3,
    body: Entity,
    choice: &LoadoutChoice,
) {
    let camera = commands
        .spawn((
            PlayerCamera,
            Camera3d::default(),
            Camera { order: 1, ..default() },
            Hdr,
            Tonemapping::TonyMcMapface,
            Bloom::NATURAL,
            Projection::from(PerspectiveProjection { fov: BASE_FOV.to_radians(), near: 0.03, ..default() }),
            DistanceFog {
                color: Color::srgb(0.03, 0.04, 0.07),
                falloff: FogFalloff::Linear { start: 35.0, end: 170.0 },
                ..default()
            },
            Transform::from_xyz(0.0, EYE, 0.0),
        ))
        .id();
    crate::viewmodel::spawn_viewmodel(commands, meshes, materials, camera, choice.element);
    commands.entity(body).insert(Transform::from_rotation(Quat::from_rotation_y(PI)).with_scale(Vec3::splat(0.85)));
    let player = commands
        .spawn((
            MatchEntity,
            Player {
                class: choice.class,
                yaw: 0.0,
                pitch: 0.0,
                vel: Vec3::ZERO,
                grounded: true,
                air_jump_used: false,
                lift_fuel: 0.0,
                sprinting: false,
                moving: false,
                body,
                camera,
                third: 0.0,
                third_timer: 0.0,
                forced: None,
                shake: 0.0,
                ads: 0.0,
                recoil: 0.0,
                hurt: 0.0,
                pending: None,
            },
            Transform::from_translation(start),
            Visibility::default(),
        ))
        .id();
    commands.entity(player).add_children(&[camera, body]);
}

fn grab_cursor(
    mut cursor: Single<&mut CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut grab: ResMut<Grab>,
    combat: Res<Combat>,
) {
    if let Some(locked) = crate::web::pointer_locked() {
        if combat.over && locked {
            crate::web::unlock();
        }
        grab.0 = locked && !combat.over;
        return;
    }
    if mouse.just_pressed(MouseButton::Left) && !grab.0 && !combat.over {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
        grab.0 = true;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && grab.0 {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
        grab.0 = false;
    }
    if combat.over && grab.0 {
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
        grab.0 = false;
    }
}

fn look(motion: Res<AccumulatedMouseMotion>, grab: Res<Grab>, mut player: Single<&mut Player>) {
    if !grab.0 {
        return;
    }
    let sens = 0.0022 * (1.0 - 0.45 * player.ads);
    player.yaw -= motion.delta.x * sens;
    player.pitch = (player.pitch - motion.delta.y * sens).clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
}

#[allow(clippy::too_many_arguments)]
fn movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    level: Res<Level>,
    mut combat: ResMut<Combat>,
    player: Single<(&mut Player, &mut Transform)>,
    mut casts: MessageWriter<crate::actions::CastNow>,
) {
    let dt = time.delta_secs().min(0.05);
    let (mut p, mut t) = player.into_inner();
    p.hurt = (p.hurt - dt).max(0.0);
    p.shake = (p.shake - dt * 3.0).max(0.0);
    p.recoil = (p.recoil - dt * 6.0).max(0.0);
    p.third_timer = (p.third_timer - dt).max(0.0);
    t.rotation = Quat::from_rotation_y(p.yaw);

    let alive = combat.player_alive();
    let rules = combat.sb.config.statuses.clone();
    let (speed_mult, frozen) = combat
        .sb
        .get(combat.player)
        .map(|c| {
            let mut m = c.statuses.move_speed_mult(&rules);
            if let Some(s) = &c.super_mode {
                m *= s.def.move_speed_mult;
            }
            (m, c.statuses.restrictions(&rules).movement)
        })
        .unwrap_or((1.0, false));

    // Abilities that move you take over until they finish.
    if let Some(f) = &mut p.forced {
        f.t += dt;
        let k = (f.t / f.dur).min(1.0);
        let mut pos = f.from.lerp(f.to, k);
        pos.y += f.arc * 4.0 * k * (1.0 - k);
        t.translation = level.collide(pos, RADIUS, HEIGHT, 0.6);
        if k >= 1.0 {
            let done = p.forced.take().expect("checked");
            p.vel = Vec3::ZERO;
            if let Some(cast) = done.then {
                casts.write(crate::actions::CastNow(cast));
            }
        }
        return;
    }

    let mut wish = Vec3::ZERO;
    if alive && !frozen && !combat.over {
        if keys.pressed(KeyCode::KeyW) {
            wish.z -= 1.0;
        }
        if keys.pressed(KeyCode::KeyS) {
            wish.z += 1.0;
        }
        if keys.pressed(KeyCode::KeyA) {
            wish.x -= 1.0;
        }
        if keys.pressed(KeyCode::KeyD) {
            wish.x += 1.0;
        }
    }
    let wish = t.rotation * wish.normalize_or_zero();
    p.moving = wish != Vec3::ZERO;
    p.sprinting = p.moving && keys.pressed(KeyCode::ShiftLeft) && keys.pressed(KeyCode::KeyW) && p.ads < 0.2;
    let speed = SPEED * speed_mult * if p.sprinting { 1.45 } else { 1.0 } * (1.0 - 0.35 * p.ads);

    let accel = if p.grounded { 14.0 } else { 4.0 };
    let target = wish * speed;
    let horiz = Vec3::new(p.vel.x, 0.0, p.vel.z);
    let new_h = horiz + (target - horiz) * (accel * dt).min(1.0);
    p.vel.x = new_h.x;
    p.vel.z = new_h.z;

    // Jumping, including each class's second jump.
    let jump = keys.just_pressed(KeyCode::Space) && alive && !frozen;
    let hold = keys.pressed(KeyCode::Space) && alive;
    if jump && p.grounded {
        p.vel.y = JUMP;
        p.grounded = false;
        p.air_jump_used = false;
        p.lift_fuel = 1.0;
    } else if !p.grounded {
        match p.class {
            GuardianClass::Hunter => {
                if jump && !p.air_jump_used {
                    p.vel.y = JUMP * 1.05;
                    p.air_jump_used = true;
                }
            }
            GuardianClass::Titan => {
                if hold && p.lift_fuel > 0.0 && p.vel.y < 4.0 {
                    p.vel.y += 34.0 * dt;
                    p.lift_fuel -= dt * 0.9;
                }
            }
            GuardianClass::Warlock => {
                if hold && p.vel.y < -1.2 {
                    p.vel.y = -1.2;
                    p.vel.x += wish.x * 6.0 * dt;
                    p.vel.z += wish.z * 6.0 * dt;
                }
            }
        }
    }
    p.vel.y -= GRAVITY * dt;

    let mut pos = t.translation + p.vel * dt;
    pos = level.collide(pos, RADIUS, HEIGHT, if p.grounded { 0.55 } else { 0.15 });
    let floor = level.floor_at(pos, RADIUS * 0.7);
    if pos.y <= floor {
        pos.y = floor;
        p.vel.y = p.vel.y.max(0.0);
        p.grounded = true;
    } else {
        p.grounded = pos.y - floor < 0.05 && p.vel.y <= 0.0;
    }
    t.translation = pos;
    if !alive {
        combat.hint = None;
    }
}

/// First-person eye, or a third-person boom during supers and on death.
fn camera_follow(
    time: Res<Time>,
    combat: Res<Combat>,
    level: Res<Level>,
    player: Single<(&mut Player, &GlobalTransform)>,
    mut cams: Query<(&mut Transform, &mut Projection), With<PlayerCamera>>,
    mut vis: Query<&mut Visibility>,
) {
    let dt = time.delta_secs();
    let (mut p, gt) = player.into_inner();
    let in_super = combat.sb.get(combat.player).is_some_and(|c| c.in_super());
    let want_third = in_super || p.third_timer > 0.0 || combat.over;
    if combat.over {
        // Death cam: pull up and look down at the body.
        p.pitch += (-0.55 - p.pitch) * (dt * 2.0).min(1.0);
    }
    let target = if want_third { 1.0 } else { 0.0 };
    p.third += (target - p.third) * (dt * 6.0).min(1.0);
    let third = p.third;

    let Ok((mut ct, mut proj)) = cams.get_mut(p.camera) else { return };
    // The boom follows pitch only partly, so looking up doesn't swing the
    // camera into the floor.
    let boom_pitch = Quat::from_rotation_x(p.pitch.clamp(-0.9, 0.3));
    let eye = Vec3::new(0.0, EYE, 0.0);
    let boom = boom_pitch * Vec3::new(0.7, 0.4, 4.2);
    // Keep the third-person camera out of walls.
    let world_eye = gt.translation() + gt.rotation() * eye;
    let world_dir = (gt.rotation() * boom).normalize();
    let max = level.ray_walls(world_eye, world_dir, boom.length()).map_or(boom.length(), |d| (d - 0.3).max(0.8));
    let mut local = eye + boom.normalize() * max * third;
    // Looking up swings the boom down; keep it above the floor.
    local.y = local.y.max(0.4 + 0.6 * third);
    let shake = if p.shake > 0.0 {
        let s = p.shake * 0.08;
        let tt = time.elapsed_secs() * 47.0;
        Vec3::new(tt.sin() * s, (tt * 1.3).cos() * s, 0.0)
    } else {
        Vec3::ZERO
    };
    ct.translation = local + shake;
    ct.rotation = Quat::from_rotation_x(p.pitch + p.recoil * 0.04);

    if let Projection::Perspective(pp) = proj.as_mut() {
        let fov = BASE_FOV - 22.0 * p.ads * (1.0 - third) + if p.sprinting { 6.0 } else { 0.0 } + 8.0 * third;
        pp.fov += (fov.to_radians() - pp.fov) * (dt * 10.0).min(1.0);
    }
    if let Ok(mut v) = vis.get_mut(p.body) {
        *v = if third > 0.25 { Visibility::Inherited } else { Visibility::Hidden };
    }
}

/// Picks the class model's animation from what the player is doing.
fn animate_body(
    combat: Res<Combat>,
    models: Res<Models>,
    player: Single<&Player>,
    mut rigs: Query<&mut Rig>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Ok(mut rig) = rigs.get_mut(player.body) else { return };
    if combat.over {
        if rig.current != "Death_A" {
            models::play(&mut rig, &models, &mut players, "Death_A", false, 1.0);
            rig.oneshot = Some(("Death_A".into(), 1e9));
        }
        return;
    }
    if rig.busy() {
        return;
    }
    let local = Quat::from_rotation_y(-player.yaw) * player.vel;
    let (name, speed) = if !player.grounded && player.forced.is_none() {
        ("Jump_Idle", 1.0)
    } else if Vec2::new(local.x, local.z).length() < 0.5 {
        ("Idle", 1.0)
    } else if local.z > 0.5 && local.z.abs() > local.x.abs() {
        ("Walking_Backwards", 1.3)
    } else if local.x.abs() > local.z.abs() {
        (if local.x > 0.0 { "Running_Strafe_Right" } else { "Running_Strafe_Left" }, 1.0)
    } else {
        ("Running_A", if player.sprinting { 1.3 } else { 1.0 })
    };
    models::play(&mut rig, &models, &mut players, name, true, speed);
}

/// Where the crosshair points: (camera origin, direction).
pub fn aim_ray(p: &Player, t: &Transform) -> (Vec3, Vec3) {
    let rot = Quat::from_rotation_y(p.yaw) * Quat::from_rotation_x(p.pitch);
    (t.translation + Vec3::Y * EYE, rot * Vec3::NEG_Z)
}
