//! The gun in your hands: three weapons built from shapes, animated in code
//! (bob, recoil, reload, swap, sprint, aim down sights, melee, ability casts).

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use guardian_combat::element::{DamageType, SubclassElement};
use guardian_combat::weapon::WeaponArchetype;

use crate::combat::Combat;
use crate::common::{element_color, glow, AppState, Phase};
use crate::player::{Player, PlayerCamera};

#[derive(Component)]
pub struct Viewmodel {
    /// Animation drivers, each counting down from 1.
    pub fire: f32,
    pub reload: f32,
    pub swap: f32,
    pub melee: f32,
    pub cast: f32,
    pub cast_element: DamageType,
    pub bob: f32,
    pub sprint: f32,
    /// Where shots leave the barrel, in world space.
    pub muzzle_world: Vec3,
    guns: [Entity; 3],
    muzzles: [Vec3; 3],
    flash: Entity,
    orb: Entity,
    pump: Entity,
}

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, animate.in_set(Phase::React).run_if(in_state(AppState::Playing)));
    }
}

fn part(
    commands: &mut Commands,
    parent: Entity,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    t: Transform,
) -> Entity {
    let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), t, NotShadowCaster)).id();
    commands.entity(parent).add_child(e);
    e
}

pub fn spawn_viewmodel(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    camera: Entity,
    element: SubclassElement,
) {
    let accent_el = element.damage_type().unwrap_or(DamageType::Void);
    let metal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.17, 0.19),
        metallic: 0.8,
        perceptual_roughness: 0.35,
        ..default()
    });
    let polymer = materials.add(StandardMaterial {
        base_color: Color::srgb(0.09, 0.09, 0.1),
        perceptual_roughness: 0.8,
        ..default()
    });
    let brass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.75, 0.6, 0.35),
        metallic: 0.9,
        perceptual_roughness: 0.3,
        ..default()
    });
    let accent = |t: DamageType, materials: &mut Assets<StandardMaterial>| {
        materials.add(StandardMaterial { base_color: element_color(t), emissive: glow(t, 8.0), ..default() })
    };
    let kin_accent = accent(DamageType::Kinetic, materials);
    let el_accent = accent(accent_el, materials);
    let solar_accent = accent(DamageType::Solar, materials);
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let cyl = meshes.add(Cylinder::new(0.5, 1.0));
    let along_z = Quat::from_rotation_x(FRAC_PI_2);

    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    commands.entity(camera).add_child(root);

    // Hand cannon.
    let hc = commands.spawn((Transform::default(), Visibility::Inherited)).id();
    commands.entity(root).add_child(hc);
    // Grip, frame, cylinder, heavy barrel with a top rib, hammer and front sight.
    part(
        commands,
        hc,
        cube.clone(),
        polymer.clone(),
        Transform::from_xyz(0.0, -0.062, 0.03)
            .with_rotation(Quat::from_rotation_x(0.35))
            .with_scale(Vec3::new(0.032, 0.09, 0.042)),
    );
    part(
        commands,
        hc,
        cube.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, -0.005, -0.03).with_scale(Vec3::new(0.038, 0.05, 0.1)),
    );
    part(
        commands,
        hc,
        cyl.clone(),
        brass.clone(),
        Transform::from_xyz(0.0, 0.004, -0.055).with_rotation(along_z).with_scale(Vec3::new(0.068, 0.055, 0.068)),
    );
    part(
        commands,
        hc,
        cube.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.012, -0.145).with_scale(Vec3::new(0.034, 0.04, 0.13)),
    );
    part(
        commands,
        hc,
        cyl.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.006, -0.2).with_rotation(along_z).with_scale(Vec3::new(0.026, 0.03, 0.026)),
    );
    part(
        commands,
        hc,
        cube.clone(),
        kin_accent.clone(),
        Transform::from_xyz(0.0, 0.034, -0.13).with_scale(Vec3::new(0.012, 0.005, 0.14)),
    );
    part(
        commands,
        hc,
        cube.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.032, 0.022)
            .with_rotation(Quat::from_rotation_x(-0.5))
            .with_scale(Vec3::new(0.012, 0.03, 0.012)),
    );
    part(
        commands,
        hc,
        cube.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.04, -0.205).with_scale(Vec3::new(0.006, 0.014, 0.01)),
    );

    // Shotgun.
    let sg = commands.spawn((Transform::default(), Visibility::Hidden)).id();
    commands.entity(root).add_child(sg);
    part(
        commands,
        sg,
        cube.clone(),
        polymer.clone(),
        Transform::from_xyz(0.0, -0.035, 0.13).with_scale(Vec3::new(0.05, 0.085, 0.2)),
    );
    part(
        commands,
        sg,
        cube.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.0, -0.08).with_scale(Vec3::new(0.062, 0.085, 0.24)),
    );
    part(
        commands,
        sg,
        cyl.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.022, -0.4).with_rotation(along_z).with_scale(Vec3::new(0.05, 0.46, 0.05)),
    );
    part(
        commands,
        sg,
        cube.clone(),
        el_accent.clone(),
        Transform::from_xyz(0.032, 0.0, -0.08).with_scale(Vec3::new(0.004, 0.02, 0.2)),
    );
    part(
        commands,
        sg,
        cube.clone(),
        el_accent.clone(),
        Transform::from_xyz(-0.032, 0.0, -0.08).with_scale(Vec3::new(0.004, 0.02, 0.2)),
    );
    let pump = part(
        commands,
        sg,
        cube.clone(),
        polymer.clone(),
        Transform::from_xyz(0.0, -0.03, -0.33).with_scale(Vec3::new(0.066, 0.05, 0.15)),
    );

    // Rocket launcher.
    let rl = commands.spawn((Transform::default(), Visibility::Hidden)).id();
    commands.entity(root).add_child(rl);
    part(
        commands,
        rl,
        cyl.clone(),
        metal.clone(),
        Transform::from_xyz(0.0, 0.05, -0.18).with_rotation(along_z).with_scale(Vec3::new(0.11, 0.75, 0.11)),
    );
    part(
        commands,
        rl,
        cyl.clone(),
        solar_accent.clone(),
        Transform::from_xyz(0.0, 0.05, -0.55).with_rotation(along_z).with_scale(Vec3::new(0.12, 0.025, 0.12)),
    );
    part(
        commands,
        rl,
        cyl.clone(),
        polymer.clone(),
        Transform::from_xyz(0.0, 0.05, 0.2).with_rotation(along_z).with_scale(Vec3::new(0.12, 0.05, 0.12)),
    );
    part(
        commands,
        rl,
        cube.clone(),
        polymer.clone(),
        Transform::from_xyz(0.0, -0.06, 0.0)
            .with_rotation(Quat::from_rotation_x(0.25))
            .with_scale(Vec3::new(0.04, 0.12, 0.05)),
    );
    part(
        commands,
        rl,
        cube.clone(),
        polymer.clone(),
        Transform::from_xyz(-0.07, 0.15, -0.1).with_scale(Vec3::new(0.03, 0.05, 0.12)),
    );
    part(
        commands,
        rl,
        cube.clone(),
        solar_accent.clone(),
        Transform::from_xyz(-0.07, 0.18, -0.1).with_scale(Vec3::new(0.02, 0.01, 0.04)),
    );

    // Muzzle flash and the ability orb in the off hand.
    let flash_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.5),
        emissive: LinearRgba::rgb(30.0, 18.0, 6.0),
        unlit: true,
        ..default()
    });
    let flash = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap())),
            MeshMaterial3d(flash_mat),
            Transform::from_scale(Vec3::ZERO),
            NotShadowCaster,
            PointLight { color: Color::srgb(1.0, 0.75, 0.4), intensity: 0.0, range: 10.0, ..default() },
        ))
        .id();
    commands.entity(root).add_child(flash);
    let orb = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(0.5).mesh().ico(2).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                emissive: LinearRgba::rgb(4.0, 4.0, 4.0),
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(-0.16, -0.12, -0.38).with_scale(Vec3::ZERO),
            NotShadowCaster,
            PointLight { intensity: 0.0, range: 6.0, ..default() },
        ))
        .id();
    commands.entity(root).add_child(orb);

    commands.entity(root).insert(Viewmodel {
        fire: 0.0,
        reload: 0.0,
        swap: 1.0,
        melee: 0.0,
        cast: 0.0,
        cast_element: accent_el,
        bob: 0.0,
        sprint: 0.0,
        muzzle_world: Vec3::ZERO,
        guns: [hc, sg, rl],
        muzzles: [Vec3::new(0.0, 0.006, -0.22), Vec3::new(0.0, 0.022, -0.64), Vec3::new(0.0, 0.05, -0.57)],
        flash,
        orb,
        pump,
    });
}

/// What the animation touches on each view-model part.
type PartData = (
    &'static mut Transform,
    Option<&'static mut Visibility>,
    Option<&'static mut PointLight>,
    Option<&'static MeshMaterial3d<StandardMaterial>>,
);

#[allow(clippy::too_many_arguments)]
fn animate(
    time: Res<Time>,
    combat: Res<Combat>,
    player: Single<&Player>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    vm: Single<(Entity, &mut Viewmodel, &mut Transform, &mut Visibility)>,
    mut parts: Query<PartData, Without<Viewmodel>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs().min(0.05);
    let (_, mut vm, mut t, mut vis) = vm.into_inner();
    let Some(me) = combat.sb.get(combat.player) else { return };
    let hidden = player.third > 0.2 || !me.is_alive();
    *vis = if hidden { Visibility::Hidden } else { Visibility::Inherited };

    let active = me.active_weapon.min(2);
    let reload_len = me.weapon().map_or(2.0, |w| w.def.reload_time.max(0.3));
    let reloading = me.weapon().is_some_and(|w| w.is_reloading());
    let archetype = me.weapon().map(|w| w.def.archetype);
    for (i, &g) in vm.guns.iter().enumerate() {
        if let Ok((_, Some(mut v), _, _)) = parts.get_mut(g) {
            *v = if i == active { Visibility::Inherited } else { Visibility::Hidden };
        }
    }

    vm.fire = (vm.fire - dt * 7.0).max(0.0);
    vm.swap = (vm.swap - dt * 3.0).max(0.0);
    vm.melee = (vm.melee - dt * 3.0).max(0.0);
    vm.cast = (vm.cast - dt * 2.5).max(0.0);
    vm.reload = if reloading { (vm.reload.max(0.01) - dt / reload_len).max(0.01) } else { 0.0 };
    let sprint_target = if player.sprinting { 1.0 } else { 0.0 };
    vm.sprint += (sprint_target - vm.sprint) * (dt * 8.0).min(1.0);
    let speed = Vec2::new(player.vel.x, player.vel.z).length();
    if player.grounded {
        vm.bob += dt * speed * 1.6;
    }

    let rocket = archetype == Some(WeaponArchetype::RocketLauncher);
    let hip = if rocket { Vec3::new(0.24, -0.23, -0.48) } else { Vec3::new(0.2, -0.19, -0.42) };
    let ads = if rocket { Vec3::new(0.1, -0.19, -0.42) } else { Vec3::new(0.0, -0.105, -0.3) };
    let a = player.ads;
    let mut pos = hip.lerp(ads, a);
    let mut rot = Quat::IDENTITY;

    let bob_amt = (speed / 6.5).min(1.4) * (1.0 - 0.8 * a);
    pos += Vec3::new(vm.bob.sin() * 0.012, -(vm.bob * 2.0).cos().abs() * 0.01, 0.0) * bob_amt;
    if !player.grounded {
        pos.y += (player.vel.y * -0.004).clamp(-0.03, 0.03);
    }
    let kick = match archetype {
        Some(WeaponArchetype::Shotgun) => 1.6,
        Some(WeaponArchetype::RocketLauncher) => 2.0,
        _ => 1.0,
    };
    let f = vm.fire * vm.fire;
    pos.z += 0.05 * f * kick;
    rot *= Quat::from_rotation_x(0.22 * f * kick);
    if vm.reload > 0.0 {
        let r = (vm.reload * PI).sin();
        pos.y -= 0.08 * r;
        rot *= Quat::from_rotation_x(-0.5 * r) * Quat::from_rotation_z(0.7 * r);
    }
    let s = vm.sprint;
    pos += Vec3::new(-0.04, -0.06, 0.04) * s;
    rot *= Quat::from_rotation_y(0.7 * s) * Quat::from_rotation_x(-0.35 * s);
    pos.y -= 0.35 * vm.swap * vm.swap;
    let m = (vm.melee * PI).sin();
    pos += Vec3::new(-0.12, -0.05, -0.08) * m;
    rot *= Quat::from_rotation_z(0.9 * m) * Quat::from_rotation_y(-0.5 * m);
    let c = (vm.cast * PI).sin();
    pos.y -= 0.12 * c;
    rot *= Quat::from_rotation_z(-0.25 * c);
    t.translation = pos;
    t.rotation = rot;

    // Shotgun pump racks back after each shot.
    if let Ok((mut pt, ..)) = parts.get_mut(vm.pump) {
        pt.translation.z = -0.33 + 0.08 * ((1.0 - vm.fire) * PI).sin() * (vm.fire > 0.01) as u8 as f32;
    }

    let muzzle = vm.muzzles[active];
    if let Ok((mut ft, _, Some(mut light), _)) = parts.get_mut(vm.flash) {
        let on = vm.fire > 0.75;
        ft.translation = muzzle;
        ft.scale = Vec3::splat(if on { 0.07 + 0.05 * kick } else { 0.0 });
        light.intensity = if on { 60_000.0 * kick } else { 0.0 };
    }
    if let Ok((mut ot, _, Some(mut light), mat)) = parts.get_mut(vm.orb) {
        ot.scale = Vec3::splat(0.035 * c);
        light.intensity = 40_000.0 * c;
        light.color = element_color(vm.cast_element);
        if let Some(mut m) = mat.and_then(|m| materials.get_mut(&m.0)) {
            m.emissive = glow(vm.cast_element, 10.0);
            m.base_color = element_color(vm.cast_element);
        }
    }
    if let Ok(cam) = camera.single() {
        vm.muzzle_world = cam.transform_point(pos + rot * muzzle);
    }
}
