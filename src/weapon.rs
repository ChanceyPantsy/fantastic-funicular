//! Weapons: archetypes, ammo, damage falloff, magazines/reloads and
//! time-to-kill math.

use crate::combatant::ChampionKind;
use crate::element::DamageType;
use crate::status::StatusKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AmmoType {
    Primary,
    Special,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WeaponArchetype {
    AutoRifle,
    HandCannon,
    PulseRifle,
    ScoutRifle,
    Sidearm,
    SubmachineGun,
    Bow,
    Shotgun,
    SniperRifle,
    FusionRifle,
    GrenadeLauncher,
    Glaive,
    TraceRifle,
    LinearFusionRifle,
    MachineGun,
    RocketLauncher,
    Sword,
}

impl WeaponArchetype {
    /// The ammo type this archetype usually uses. Some archetypes exist in
    /// more than one (e.g. special and heavy grenade launchers), so a
    /// [`WeaponDef`] stores its own ammo type.
    pub fn default_ammo(self) -> AmmoType {
        use WeaponArchetype::*;
        match self {
            AutoRifle | HandCannon | PulseRifle | ScoutRifle | Sidearm | SubmachineGun | Bow => AmmoType::Primary,
            Shotgun | SniperRifle | FusionRifle | GrenadeLauncher | Glaive | TraceRifle => AmmoType::Special,
            LinearFusionRifle | MachineGun | RocketLauncher | Sword => AmmoType::Heavy,
        }
    }
}

/// Damage drops linearly from 100% at `start` to `min_mult` at `end`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Falloff {
    pub start: f32,
    pub end: f32,
    pub min_mult: f32,
}

impl Falloff {
    pub const NONE: Falloff = Falloff { start: f32::INFINITY, end: f32::INFINITY, min_mult: 1.0 };

    pub fn mult(&self, distance: f32) -> f32 {
        if distance <= self.start {
            1.0
        } else if distance >= self.end {
            self.min_mult
        } else {
            let t = (distance - self.start) / (self.end - self.start);
            1.0 + (self.min_mult - 1.0) * t
        }
    }
}

/// Area damage dealt on impact (rockets, grenade launchers).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Blast {
    pub damage: f32,
    pub radius: f32,
}

/// Static description of a weapon.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WeaponDef {
    pub name: String,
    pub archetype: WeaponArchetype,
    pub ammo: AmmoType,
    pub damage_type: DamageType,
    /// Shots per minute.
    pub rpm: f32,
    /// Body-shot damage per projectile/pellet.
    pub damage: f32,
    pub precision_mult: f32,
    /// Projectiles per shot (shotgun pellets, fusion bolts).
    pub pellets: u32,
    pub falloff: Falloff,
    pub magazine: u32,
    /// Spare ammo. `None` = unlimited (primaries).
    pub reserves: Option<u32>,
    /// Seconds.
    pub reload_time: f32,
    /// Seconds of charge before each shot (fusions, bows, linears).
    pub charge_time: f32,
    pub blast: Option<Blast>,
    pub anti_champion: Option<ChampionKind>,
    /// Statuses applied to the target on hit, as `(status, stacks)`.
    pub on_hit: Vec<(StatusKind, u32)>,
}

impl WeaponDef {
    pub fn new(name: impl Into<String>, archetype: WeaponArchetype, damage_type: DamageType) -> Self {
        Self {
            name: name.into(),
            archetype,
            ammo: archetype.default_ammo(),
            damage_type,
            rpm: 120.0,
            damage: 10.0,
            precision_mult: 1.5,
            pellets: 1,
            falloff: Falloff::NONE,
            magazine: 10,
            reserves: None,
            reload_time: 2.0,
            charge_time: 0.0,
            blast: None,
            anti_champion: None,
            on_hit: Vec::new(),
        }
    }

    /// Seconds between shots.
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rpm.max(f32::EPSILON)
    }

    /// Damage of one full shot (all pellets hit), before buffs.
    pub fn shot_damage(&self, precision: bool, distance: f32) -> f32 {
        let p = if precision { self.precision_mult } else { 1.0 };
        let blast = self.blast.map_or(0.0, |b| b.damage);
        (self.damage * p * self.pellets as f32 + blast) * self.falloff.mult(distance)
    }

    /// Shots needed to deal `hp` damage, ignoring reloads.
    pub fn shots_to_kill(&self, hp: f32, precision: bool, distance: f32) -> Option<u32> {
        let per = self.shot_damage(precision, distance);
        if per <= 0.0 {
            return None;
        }
        Some((hp / per).ceil().max(1.0) as u32)
    }

    /// Time to kill in seconds: the first shot lands after `charge_time`,
    /// each further shot one fire interval later. Ignores reloads.
    pub fn time_to_kill(&self, hp: f32, precision: bool, distance: f32) -> Option<f32> {
        self.shots_to_kill(hp, precision, distance).map(|n| self.charge_time + (n - 1) as f32 * self.fire_interval())
    }

    /// Fewest shots and fastest time with the best mix of precision and body
    /// shots: the lowest shot count, then as few precision hits as allowed.
    pub fn optimal_kill(&self, hp: f32, distance: f32) -> Option<KillProfile> {
        let n = self.shots_to_kill(hp, true, distance)?;
        let crit = self.shot_damage(true, distance);
        let body = self.shot_damage(false, distance);
        let crits = (0..=n).find(|&c| c as f32 * crit + (n - c) as f32 * body >= hp)?;
        Some(KillProfile {
            shots: n,
            precision_hits: crits,
            seconds: self.charge_time + (n - 1) as f32 * self.fire_interval(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KillProfile {
    pub shots: u32,
    pub precision_hits: u32,
    pub seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireError {
    EmptyMagazine,
    Reloading,
    /// Still within the fire interval of the last shot.
    Cooldown,
    /// The wielder is restricted (Frozen, Suspended, Blinded...).
    Restricted,
}

/// A weapon instance with live ammo and timers.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Weapon {
    pub def: WeaponDef,
    pub magazine: u32,
    pub reserves: Option<u32>,
    cooldown: f32,
    reload_remaining: Option<f32>,
}

impl Weapon {
    pub fn new(def: WeaponDef) -> Self {
        Self { magazine: def.magazine, reserves: def.reserves, def, cooldown: 0.0, reload_remaining: None }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload_remaining.is_some()
    }

    pub fn can_fire(&self) -> Result<(), FireError> {
        if self.is_reloading() {
            Err(FireError::Reloading)
        } else if self.magazine == 0 {
            Err(FireError::EmptyMagazine)
        } else if self.cooldown > 0.0 {
            Err(FireError::Cooldown)
        } else {
            Ok(())
        }
    }

    /// Spends one round. Charge time is the host's job (play the charge,
    /// then call this).
    pub fn fire(&mut self) -> Result<(), FireError> {
        self.can_fire()?;
        self.magazine -= 1;
        self.cooldown = self.def.fire_interval();
        Ok(())
    }

    /// Starts a reload. `speed_bonus` shortens it (0.1 = 10% faster).
    /// Returns false if the magazine is full, already reloading, or there
    /// are no reserves.
    pub fn reload(&mut self, speed_bonus: f32) -> bool {
        if self.is_reloading() || self.magazine >= self.def.magazine || self.reserves == Some(0) {
            return false;
        }
        self.reload_remaining = Some(self.def.reload_time / (1.0 + speed_bonus.max(0.0)));
        true
    }

    /// Adds spare ammo up to `cap`.
    pub fn add_reserves(&mut self, amount: u32, cap: u32) {
        if let Some(r) = &mut self.reserves {
            *r = (*r + amount).min(cap);
        }
    }

    /// Advances timers. Returns true if a reload finished this tick.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.cooldown = (self.cooldown - dt).max(0.0);
        let Some(left) = &mut self.reload_remaining else { return false };
        *left -= dt;
        if *left > 0.0 {
            return false;
        }
        self.reload_remaining = None;
        let need = self.def.magazine - self.magazine;
        let got = match &mut self.reserves {
            None => need,
            Some(r) => {
                let got = need.min(*r);
                *r -= got;
                got
            }
        };
        self.magazine += got;
        true
    }
}

/// Example weapon templates, tuned so a guardian built with the default
/// [`GuardianRules`](crate::combatant::GuardianRules) (~200 health) dies in
/// roughly the familiar number of hits. Approximations, not datamined.
pub mod presets {
    use super::*;

    pub fn hand_cannon_140(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 140.0,
            damage: 45.0,
            precision_mult: 1.6,
            falloff: Falloff { start: 30.0, end: 50.0, min_mult: 0.5 },
            magazine: 12,
            reload_time: 2.3,
            ..WeaponDef::new("Adaptive Hand Cannon", WeaponArchetype::HandCannon, damage_type)
        }
    }

    pub fn auto_rifle_600(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 600.0,
            damage: 14.0,
            precision_mult: 1.6,
            falloff: Falloff { start: 22.0, end: 40.0, min_mult: 0.5 },
            magazine: 42,
            reload_time: 2.4,
            ..WeaponDef::new("Adaptive Auto Rifle", WeaponArchetype::AutoRifle, damage_type)
        }
    }

    pub fn scout_rifle_260(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 260.0,
            damage: 29.0,
            precision_mult: 1.45,
            falloff: Falloff { start: 50.0, end: 80.0, min_mult: 0.5 },
            magazine: 18,
            reload_time: 2.2,
            ..WeaponDef::new("Rapid-Fire Scout Rifle", WeaponArchetype::ScoutRifle, damage_type)
        }
    }

    pub fn shotgun_55(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 55.0,
            damage: 18.0,
            precision_mult: 1.05,
            pellets: 12,
            falloff: Falloff { start: 5.0, end: 14.0, min_mult: 0.3 },
            magazine: 5,
            reserves: Some(20),
            reload_time: 0.7,
            ..WeaponDef::new("Aggressive Shotgun", WeaponArchetype::Shotgun, damage_type)
        }
    }

    pub fn sniper_72(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 72.0,
            damage: 110.0,
            precision_mult: 2.0,
            magazine: 3,
            reserves: Some(18),
            reload_time: 3.0,
            ..WeaponDef::new("Aggressive Sniper Rifle", WeaponArchetype::SniperRifle, damage_type)
        }
    }

    pub fn fusion_rifle(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 72.0,
            damage: 30.0,
            precision_mult: 1.0,
            pellets: 7,
            charge_time: 0.46,
            falloff: Falloff { start: 12.0, end: 22.0, min_mult: 0.5 },
            magazine: 5,
            reserves: Some(20),
            reload_time: 2.5,
            ..WeaponDef::new("Rapid-Fire Fusion Rifle", WeaponArchetype::FusionRifle, damage_type)
        }
    }

    pub fn rocket_launcher(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 25.0,
            damage: 50.0,
            precision_mult: 1.0,
            blast: Some(Blast { damage: 250.0, radius: 4.5 }),
            magazine: 1,
            reserves: Some(6),
            reload_time: 2.8,
            ..WeaponDef::new("Precision Rocket Launcher", WeaponArchetype::RocketLauncher, damage_type)
        }
    }

    pub fn sword(damage_type: DamageType) -> WeaponDef {
        WeaponDef {
            rpm: 85.0,
            damage: 150.0,
            precision_mult: 1.0,
            falloff: Falloff { start: 4.0, end: 4.5, min_mult: 0.0 },
            magazine: 40,
            reserves: Some(40),
            reload_time: 0.0,
            ..WeaponDef::new("Vortex Sword", WeaponArchetype::Sword, damage_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::presets::*;
    use super::*;

    #[test]
    fn falloff_is_linear_between_start_and_end() {
        let f = Falloff { start: 10.0, end: 20.0, min_mult: 0.5 };
        assert_eq!(f.mult(5.0), 1.0);
        assert_eq!(f.mult(15.0), 0.75);
        assert_eq!(f.mult(30.0), 0.5);
        assert_eq!(Falloff::NONE.mult(1e6), 1.0);
    }

    #[test]
    fn hand_cannon_is_a_three_tap() {
        let hc = hand_cannon_140(DamageType::Kinetic);
        assert_eq!(hc.shots_to_kill(200.0, true, 10.0), Some(3));
        assert_eq!(hc.shots_to_kill(200.0, false, 10.0), Some(5));
        let ttk = hc.time_to_kill(200.0, true, 10.0).unwrap();
        assert!((ttk - 2.0 * 60.0 / 140.0).abs() < 1e-4);
        assert!(hc.shots_to_kill(200.0, true, 60.0).unwrap() > 3, "falloff adds shots");
    }

    #[test]
    fn optimal_kill_uses_fewest_crits() {
        let hc = hand_cannon_140(DamageType::Kinetic);
        // 3 shots: 72+72+45 = 189 < 200, so all three need to be crits.
        let p = hc.optimal_kill(200.0, 0.0).unwrap();
        assert_eq!((p.shots, p.precision_hits), (3, 3));
        // 180 hp: 72+72+45 = 189 ≥ 180, so two crits suffice.
        let p = hc.optimal_kill(180.0, 0.0).unwrap();
        assert_eq!((p.shots, p.precision_hits), (3, 2));
    }

    #[test]
    fn magazine_and_reload_cycle() {
        let mut w = Weapon::new(sniper_72(DamageType::Kinetic));
        assert_eq!(w.fire(), Ok(()));
        assert_eq!(w.fire(), Err(FireError::Cooldown));
        for _ in 0..2 {
            w.tick(w.def.fire_interval());
            assert_eq!(w.fire(), Ok(()));
        }
        assert_eq!(w.fire(), Err(FireError::EmptyMagazine));
        assert!(w.reload(0.0));
        assert_eq!(w.fire(), Err(FireError::Reloading));
        assert!(w.tick(3.0));
        assert_eq!((w.magazine, w.reserves), (3, Some(15)));
    }

    #[test]
    fn archetype_ammo() {
        assert_eq!(WeaponArchetype::HandCannon.default_ammo(), AmmoType::Primary);
        assert_eq!(WeaponArchetype::Shotgun.default_ammo(), AmmoType::Special);
        assert_eq!(WeaponArchetype::Sword.default_ammo(), AmmoType::Heavy);
    }
}
