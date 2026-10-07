//! The arena: a lunar Hive ruin. Geometry, lights, sky, and the boxes used
//! for collision and line of sight.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::common::{Aabb, Rng};

pub const HALF: f32 = 38.0;
const WALL_H: f32 = 9.0;

#[derive(Resource)]
pub struct Level {
    pub boxes: Vec<Aabb>,
    pub spawns: Vec<Vec3>,
}

impl Level {
    /// Nearest hit along a ray against level geometry and the ground.
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let mut best = None::<f32>;
        if dir.y < -1e-4 {
            let t = -origin.y / dir.y;
            if t >= 0.0 {
                best = Some(t);
            }
        }
        for b in &self.boxes {
            if let Some(t) = b.ray(origin, dir) {
                best = Some(best.map_or(t, |x: f32| x.min(t)));
            }
        }
        best.filter(|&t| t <= max)
    }

    /// Nearest hit along a ray against level boxes only (not the ground).
    pub fn ray_walls(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.boxes.iter().filter_map(|b| b.ray(origin, dir)).filter(|&t| t <= max).min_by(|a, b| a.total_cmp(b))
    }

    /// Whether there's a clear line between two points.
    pub fn clear(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return true;
        }
        self.ray(a, d / len, len - 0.05).is_none()
    }

    /// Height of the floor under `p` (top of the highest box below it).
    pub fn floor_at(&self, p: Vec3, radius: f32) -> f32 {
        let mut y: f32 = 0.0;
        for b in &self.boxes {
            let inside =
                p.x + radius > b.min.x && p.x - radius < b.max.x && p.z + radius > b.min.z && p.z - radius < b.max.z;
            if inside && b.max.y <= p.y + 0.6 {
                y = y.max(b.max.y);
            }
        }
        y
    }

    /// Pushes a vertical cylinder (feet at `p`) out of the level boxes.
    /// Steps up onto boxes no taller than `step` above the feet.
    pub fn collide(&self, mut p: Vec3, radius: f32, height: f32, step: f32) -> Vec3 {
        for b in &self.boxes {
            let overlaps_y = p.y + height > b.min.y && p.y < b.max.y;
            if !overlaps_y {
                continue;
            }
            let cx = p.x.clamp(b.min.x, b.max.x);
            let cz = p.z.clamp(b.min.z, b.max.z);
            let dx = p.x - cx;
            let dz = p.z - cz;
            let d2 = dx * dx + dz * dz;
            if d2 >= radius * radius {
                continue;
            }
            if b.max.y - p.y <= step {
                p.y = b.max.y;
                continue;
            }
            if d2 > 1e-8 {
                let d = d2.sqrt();
                let push = (radius - d) / d;
                p.x += dx * push;
                p.z += dz * push;
            } else {
                // Centre inside the box: leave by the nearest side.
                let exits = [
                    (p.x - b.min.x + radius, Vec3::NEG_X),
                    (b.max.x - p.x + radius, Vec3::X),
                    (p.z - b.min.z + radius, Vec3::NEG_Z),
                    (b.max.z - p.z + radius, Vec3::Z),
                ];
                let (d, n) = exits.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
                p += n * d;
            }
        }
        p.x = p.x.clamp(-HALF + radius, HALF - radius);
        p.z = p.z.clamp(-HALF + radius, HALF - radius);
        p
    }
}

/// A tileable value-noise texture used to break up flat surfaces.
fn noise_texture(seed: u32, size: u32) -> Image {
    let mut rng = Rng(seed);
    let cells = 16usize;
    let grid: Vec<f32> = (0..cells * cells).map(|_| rng.f()).collect();
    let at = |x: usize, y: usize| grid[(y % cells) * cells + (x % cells)];
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut v = 0.0;
            let mut amp = 0.55;
            let mut freq = 1.0;
            for _ in 0..4 {
                let fx = x as f32 / size as f32 * cells as f32 * freq;
                let fy = y as f32 / size as f32 * cells as f32 * freq;
                let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
                let (tx, ty) = (fx.fract(), fy.fract());
                let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
                let a = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * sx;
                let b = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * sx;
                v += (a + (b - a) * sy) * amp;
                amp *= 0.5;
                freq *= 2.0;
            }
            let g = (155.0 + v * 100.0).clamp(0.0, 255.0) as u8;
            data.extend_from_slice(&[g, g, g, 255]);
        }
    }
    let mut img = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

pub fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let noise = images.add(noise_texture(7, 256));
    let ground_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.42, 0.44),
        base_color_texture: Some(noise.clone()),
        uv_transform: Affine2::from_scale(Vec2::splat(14.0)),
        perceptual_roughness: 0.95,
        ..default()
    });
    let rock_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.30, 0.33),
        base_color_texture: Some(noise.clone()),
        uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
        perceptual_roughness: 0.9,
        ..default()
    });
    let hive_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.15, 0.14),
        base_color_texture: Some(noise.clone()),
        uv_transform: Affine2::from_scale(Vec2::splat(2.0)),
        perceptual_roughness: 0.7,
        metallic: 0.2,
        ..default()
    });
    let trim_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.50, 0.40),
        perceptual_roughness: 0.5,
        metallic: 0.6,
        ..default()
    });
    let crystal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.9, 0.45),
        emissive: LinearRgba::rgb(0.6, 6.0, 1.6),
        perceptual_roughness: 0.2,
        ..default()
    });

    let mut boxes = Vec::new();
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut block = |commands: &mut Commands, center: Vec3, size: Vec3, mat: &Handle<StandardMaterial>, solid: bool| {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(center).with_scale(size),
        ));
        if solid {
            boxes.push(Aabb::from_center(center, size));
        }
    };

    // Ground.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(HALF * 2.0 + 40.0, HALF * 2.0 + 40.0))),
        MeshMaterial3d(ground_mat),
    ));

    // Cliffs around the edge: uneven blocks.
    let mut rng = Rng(42);
    let seg = 8.0;
    let mut t = -HALF;
    while t < HALF {
        for (center, size) in [
            (Vec3::new(t + seg / 2.0, 0.0, -HALF - 2.0), Vec3::new(seg + 1.0, 0.0, 4.0)),
            (Vec3::new(t + seg / 2.0, 0.0, HALF + 2.0), Vec3::new(seg + 1.0, 0.0, 4.0)),
            (Vec3::new(-HALF - 2.0, 0.0, t + seg / 2.0), Vec3::new(4.0, 0.0, seg + 1.0)),
            (Vec3::new(HALF + 2.0, 0.0, t + seg / 2.0), Vec3::new(4.0, 0.0, seg + 1.0)),
        ] {
            let h = WALL_H + rng.range(-2.5, 6.0);
            block(&mut commands, center + Vec3::Y * h / 2.0, Vec3::new(size.x, h, size.z), &rock_mat, true);
        }
        t += seg;
    }

    // Central ziggurat: a raised platform with stairs on two sides.
    block(&mut commands, Vec3::new(0.0, 1.25, 0.0), Vec3::new(12.0, 2.5, 12.0), &hive_mat, true);
    block(&mut commands, Vec3::new(0.0, 2.6, 0.0), Vec3::new(12.4, 0.2, 12.4), &trim_mat, false);
    for i in 0..5 {
        let h = 0.5 * (i + 1) as f32;
        let z = 6.0 + 0.8 * (5 - i) as f32 - 0.4;
        block(&mut commands, Vec3::new(0.0, h / 2.0, z), Vec3::new(4.0, h, 0.8), &hive_mat, true);
        block(&mut commands, Vec3::new(0.0, h / 2.0, -z), Vec3::new(4.0, h, 0.8), &hive_mat, true);
    }
    // Obelisk on top.
    block(&mut commands, Vec3::new(0.0, 5.0, 0.0), Vec3::new(1.2, 5.0, 1.2), &hive_mat, true);
    block(&mut commands, Vec3::new(0.0, 7.8, 0.0), Vec3::new(0.5, 0.6, 0.5), &crystal_mat, false);

    // Pillars and cover.
    for &(x, z) in &[(-16.0, -16.0), (16.0, -16.0), (-16.0, 16.0), (16.0, 16.0)] {
        block(&mut commands, Vec3::new(x, 4.0, z), Vec3::new(2.2, 8.0, 2.2), &hive_mat, true);
        block(&mut commands, Vec3::new(x, 8.2, z), Vec3::new(2.8, 0.4, 2.8), &trim_mat, false);
        block(&mut commands, Vec3::new(x, 3.0, z + 1.15), Vec3::new(0.3, 3.5, 0.1), &crystal_mat, false);
    }
    let cover = [
        (Vec3::new(-9.0, 0.6, -24.0), Vec3::new(6.0, 1.2, 1.2)),
        (Vec3::new(10.0, 0.6, 24.0), Vec3::new(6.0, 1.2, 1.2)),
        (Vec3::new(-24.0, 0.6, 6.0), Vec3::new(1.2, 1.2, 7.0)),
        (Vec3::new(24.0, 0.6, -7.0), Vec3::new(1.2, 1.2, 7.0)),
        (Vec3::new(-26.0, 1.5, -26.0), Vec3::new(4.0, 3.0, 4.0)),
        (Vec3::new(26.0, 1.5, 26.0), Vec3::new(4.0, 3.0, 4.0)),
        (Vec3::new(27.0, 1.0, -25.0), Vec3::new(3.0, 2.0, 5.0)),
        (Vec3::new(-27.0, 1.0, 25.0), Vec3::new(3.0, 2.0, 5.0)),
        (Vec3::new(8.0, 0.75, -10.0), Vec3::new(2.0, 1.5, 2.0)),
        (Vec3::new(-8.0, 0.75, 10.0), Vec3::new(2.0, 1.5, 2.0)),
    ];
    for (c, s) in cover {
        block(&mut commands, c, s, &rock_mat, true);
    }

    // Hive crystals with green light.
    let crystal = meshes.add(Cuboid::new(0.6, 2.4, 0.6));
    for &(x, z, r) in &[
        (-30.0, 0.0, 0.3),
        (30.0, 0.0, -0.4),
        (0.0, -30.0, 0.5),
        (0.0, 30.0, -0.2),
        (-12.0, 28.0, 0.2),
        (12.0, -28.0, 0.1),
    ] {
        let base = Vec3::new(x, 1.0, z);
        for k in 0..3 {
            let off = Vec3::new(k as f32 * 0.6 - 0.6, -0.3 * (k % 2) as f32, 0.2 * k as f32);
            commands.spawn((
                Mesh3d(crystal.clone()),
                MeshMaterial3d(crystal_mat.clone()),
                Transform::from_translation(base + off)
                    .with_rotation(Quat::from_rotation_z(r + 0.3 * (k as f32 - 1.0))),
                NotShadowCaster,
            ));
        }
        commands.spawn((
            PointLight { color: Color::srgb(0.3, 1.0, 0.5), intensity: 120_000.0, range: 14.0, ..default() },
            Transform::from_translation(base + Vec3::Y * 1.5),
        ));
    }

    // Sky: stars and the Earth.
    let star = meshes.add(Sphere::new(0.6).mesh().ico(1).unwrap());
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::rgb(6.0, 6.0, 7.0),
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    for _ in 0..260 {
        let mut d = Vec3::new(rng.range(-1.0, 1.0), rng.range(0.05, 1.0), rng.range(-1.0, 1.0));
        d = d.normalize();
        commands.spawn((
            Mesh3d(star.clone()),
            MeshMaterial3d(star_mat.clone()),
            Transform::from_translation(d * 380.0).with_scale(Vec3::splat(rng.range(0.4, 1.4))),
            NotShadowCaster,
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(60.0).mesh().uv(48, 24))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.6, 1.0),
            base_color_texture: Some(noise),
            unlit: true,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_xyz(-180.0, 140.0, -330.0),
        NotShadowCaster,
    ));

    // Moonlight.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.85, 0.9, 1.0),
            illuminance: 6_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-20.0, 40.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 60.0, ..default() }.build(),
    ));

    let mut spawns = Vec::new();
    for i in 0..16 {
        let a = i as f32 / 16.0 * std::f32::consts::TAU;
        spawns.push(Vec3::new(a.cos() * (HALF - 4.0), 0.0, a.sin() * (HALF - 4.0)));
    }
    spawns.extend([
        Vec3::new(-20.0, 0.0, 0.0),
        Vec3::new(20.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 22.0),
        Vec3::new(0.0, 0.0, -22.0),
    ]);
    commands.insert_resource(Level { boxes, spawns });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level() -> Level {
        Level {
            boxes: vec![
                Aabb::from_center(Vec3::new(0.0, 1.0, 0.0), Vec3::new(4.0, 2.0, 4.0)),
                Aabb::from_center(Vec3::new(5.0, 0.25, 0.0), Vec3::new(2.0, 0.5, 2.0)),
            ],
            spawns: vec![],
        }
    }

    #[test]
    fn walls_push_out_and_steps_lift() {
        let l = level();
        let p = l.collide(Vec3::new(2.2, 0.0, 0.0), 0.4, 1.8, 0.55);
        assert!((p.x - 2.4).abs() < 1e-4, "{p}");
        let p = l.collide(Vec3::new(4.5, 0.0, 0.0), 0.4, 1.8, 0.55);
        assert_eq!(p.y, 0.5, "steps onto the low box");
        assert!(l.clear(Vec3::new(-5.0, 3.0, 0.0), Vec3::new(5.0, 3.0, 0.0)));
        assert!(!l.clear(Vec3::new(-5.0, 1.0, 0.0), Vec3::new(5.0, 1.0, 0.0)));
        assert_eq!(l.floor_at(Vec3::new(0.0, 2.0, 0.0), 0.4), 2.0);
    }
}
