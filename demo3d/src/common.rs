//! Types and helpers shared by every part of the game.

use bevy::prelude::*;
use guardian_combat::catalog;
use guardian_combat::element::{DamageType, GuardianClass, SubclassElement};
use guardian_combat::sandbox::CombatEvent;
use serde::{Deserialize, Serialize};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Waiting for models to load.
    #[default]
    Loading,
    /// Waiting for a loadout (the web page shows its picker).
    Menu,
    Playing,
}

/// Order of work inside one frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Read input, move the player.
    Input,
    /// Fire weapons, cast abilities, run enemy AI.
    Act,
    /// Advance projectiles and the combat sandbox.
    Sim,
    /// React to what happened: animations, effects, HUD.
    React,
}

/// Every match entity carries this so a restart can clear them all.
#[derive(Component)]
pub struct MatchEntity;

/// The player's chosen class, subclass and abilities, by name.
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
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
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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

/// A loadout waiting to start a match (from the web page or the command line).
#[derive(Resource, Default)]
pub struct PendingLoadout(pub Option<LoadoutChoice>);

/// Sandbox events from this frame, for everything that reacts to them.
#[derive(Resource, Default)]
pub struct FrameEvents(pub Vec<CombatEvent>);

pub fn element_color(t: DamageType) -> Color {
    match t {
        DamageType::Kinetic => Color::srgb(0.92, 0.90, 0.84),
        DamageType::Arc => Color::srgb(0.45, 0.82, 1.0),
        DamageType::Solar => Color::srgb(1.0, 0.55, 0.15),
        DamageType::Void => Color::srgb(0.70, 0.45, 1.0),
        DamageType::Stasis => Color::srgb(0.35, 0.55, 1.0),
        DamageType::Strand => Color::srgb(0.25, 0.95, 0.55),
    }
}

pub fn subclass_color(e: SubclassElement) -> Color {
    match e.damage_type() {
        Some(t) => element_color(t),
        None => Color::srgb(0.95, 0.5, 0.8),
    }
}

/// HDR emissive version of an element colour, `k` times as bright.
pub fn glow(t: DamageType, k: f32) -> LinearRgba {
    let c = element_color(t).to_linear();
    LinearRgba::rgb(c.red * k, c.green * k, c.blue * k)
}

pub fn v3(p: [f32; 3]) -> Vec3 {
    Vec3::from_array(p)
}

pub fn a3(v: Vec3) -> [f32; 3] {
    v.to_array()
}

/// An axis-aligned box in the level.
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_center(center: Vec3, size: Vec3) -> Self {
        Self { min: center - size / 2.0, max: center + size / 2.0 }
    }

    /// Distance along the ray to the box, if hit.
    pub fn ray(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let inv = Vec3::ONE / dir;
        let t1 = (self.min - origin) * inv;
        let t2 = (self.max - origin) * inv;
        let tmin = t1.min(t2).max_element();
        let tmax = t1.max(t2).min_element();
        (tmax >= tmin.max(0.0)).then_some(tmin.max(0.0))
    }
}

/// Distance along a ray to a sphere, if hit.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.length_squared() - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t = -b - s;
    if t >= 0.0 {
        Some(t)
    } else if -b + s >= 0.0 {
        Some(0.0)
    } else {
        None
    }
}

/// Small deterministic random numbers (xorshift).
#[derive(Resource)]
pub struct Rng(pub u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x9E37_79B9)
    }
}

impl Rng {
    pub fn f(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }

    pub fn dir(&mut self) -> Vec3 {
        let v = Vec3::new(self.range(-1.0, 1.0), self.range(-1.0, 1.0), self.range(-1.0, 1.0));
        v.normalize_or(Vec3::Y)
    }
}

/// Fonts used by the HUD.
#[derive(Resource, Clone)]
pub struct Fonts {
    pub hud: Handle<Font>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits() {
        let b = Aabb::from_center(Vec3::new(0.0, 0.0, -5.0), Vec3::splat(2.0));
        assert_eq!(b.ray(Vec3::ZERO, Vec3::NEG_Z), Some(4.0));
        assert_eq!(b.ray(Vec3::ZERO, Vec3::Z), None);
        assert!((ray_sphere(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.0, 0.0, -5.0), 1.0).unwrap() - 4.0).abs() < 1e-5);
        assert_eq!(ray_sphere(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, -5.0), 1.0), None);
    }
}
