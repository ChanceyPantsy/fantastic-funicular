//! Grenades, melees, class abilities and supers: energy, charges and
//! cooldowns.
//!
//! Every ability fills an energy bar from 0 to 1. A full bar becomes a
//! charge. Energy comes from passive regeneration (scaled by the matching
//! stat), Orbs of Power, and for supers, from dealing damage and getting kills.

use crate::element::DamageType;
use crate::status::StatusKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AbilitySlot {
    Grenade,
    Melee,
    ClassAbility,
    Super,
}

impl AbilitySlot {
    pub const ALL: [AbilitySlot; 4] =
        [AbilitySlot::Grenade, AbilitySlot::Melee, AbilitySlot::ClassAbility, AbilitySlot::Super];
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AbilityDef {
    pub name: String,
    pub slot: AbilitySlot,
    pub damage_type: DamageType,
    /// Seconds to regenerate one charge at a regen multiplier of 1.0.
    pub base_cooldown: f32,
    pub max_charges: u32,
    /// Damage to the target (0 for utility abilities).
    pub damage: f32,
    /// If above 0, the damage also hits other hostiles within this radius
    /// of the target.
    pub radius: f32,
    /// Statuses applied to whoever the damage hits, as `(status, stacks)`.
    pub on_hit: Vec<(StatusKind, u32)>,
    /// Statuses applied to the user on cast (e.g. Woven Mail, Radiant).
    pub on_self: Vec<(StatusKind, u32)>,
    /// Overshield granted to the user on cast.
    pub overshield: f32,
    /// Health restored to the user on cast.
    pub heal: f32,
}

impl AbilityDef {
    pub fn new(name: impl Into<String>, slot: AbilitySlot, damage_type: DamageType, base_cooldown: f32) -> Self {
        Self {
            name: name.into(),
            slot,
            damage_type,
            base_cooldown,
            max_charges: 1,
            damage: 0.0,
            radius: 0.0,
            on_hit: Vec::new(),
            on_self: Vec::new(),
            overshield: 0.0,
            heal: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AbilityState {
    pub def: AbilityDef,
    pub charges: u32,
    /// Progress toward the next charge, 0..1.
    pub energy: f32,
}

impl AbilityState {
    /// Starts with all charges available.
    pub fn new(def: AbilityDef) -> Self {
        Self { charges: def.max_charges, energy: 0.0, def }
    }

    /// Starts empty (typical for supers).
    pub fn empty(def: AbilityDef) -> Self {
        Self { charges: 0, energy: 0.0, def }
    }

    pub fn is_ready(&self) -> bool {
        self.charges > 0
    }

    pub fn is_full(&self) -> bool {
        self.charges >= self.def.max_charges
    }

    /// Fraction of the next charge, or 1.0 if all charges are full.
    pub fn progress(&self) -> f32 {
        if self.is_full() {
            1.0
        } else {
            self.energy
        }
    }

    /// Uses one charge. Returns false if none are available.
    pub fn consume(&mut self) -> bool {
        if self.charges == 0 {
            return false;
        }
        self.charges -= 1;
        true
    }

    /// Adds energy as a fraction of one charge. Returns the number of
    /// charges gained.
    pub fn add_energy(&mut self, fraction: f32) -> u32 {
        if self.is_full() || fraction <= 0.0 {
            return 0;
        }
        self.energy += fraction;
        let mut gained = 0;
        while self.energy >= 1.0 && !self.is_full() {
            self.energy -= 1.0;
            self.charges += 1;
            gained += 1;
        }
        if self.is_full() {
            self.energy = 0.0;
        }
        gained
    }

    /// Passive regeneration. Returns the number of charges gained.
    pub fn tick(&mut self, dt: f32, regen_mult: f32) -> u32 {
        if self.def.base_cooldown <= 0.0 {
            return self.add_energy(1.0);
        }
        self.add_energy(dt * regen_mult / self.def.base_cooldown)
    }
}

/// A combatant's four ability slots.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Loadout {
    pub grenade: Option<AbilityState>,
    pub melee: Option<AbilityState>,
    pub class_ability: Option<AbilityState>,
    pub super_: Option<AbilityState>,
}

impl Loadout {
    pub fn get(&self, slot: AbilitySlot) -> Option<&AbilityState> {
        match slot {
            AbilitySlot::Grenade => self.grenade.as_ref(),
            AbilitySlot::Melee => self.melee.as_ref(),
            AbilitySlot::ClassAbility => self.class_ability.as_ref(),
            AbilitySlot::Super => self.super_.as_ref(),
        }
    }

    pub fn get_mut(&mut self, slot: AbilitySlot) -> Option<&mut AbilityState> {
        match slot {
            AbilitySlot::Grenade => self.grenade.as_mut(),
            AbilitySlot::Melee => self.melee.as_mut(),
            AbilitySlot::ClassAbility => self.class_ability.as_mut(),
            AbilitySlot::Super => self.super_.as_mut(),
        }
    }

    /// Puts an ability in its slot. Supers start empty, everything else full.
    pub fn equip(&mut self, def: AbilityDef) {
        let slot = def.slot;
        let state = if slot == AbilitySlot::Super { AbilityState::empty(def) } else { AbilityState::new(def) };
        match slot {
            AbilitySlot::Grenade => self.grenade = Some(state),
            AbilitySlot::Melee => self.melee = Some(state),
            AbilitySlot::ClassAbility => self.class_ability = Some(state),
            AbilitySlot::Super => self.super_ = Some(state),
        }
    }
}

/// Example abilities, one or two per element. Approximate values.
pub mod presets {
    use super::*;

    pub fn incendiary_grenade() -> AbilityDef {
        AbilityDef {
            damage: 80.0,
            radius: 4.0,
            on_hit: vec![(StatusKind::Scorch, 60)],
            ..AbilityDef::new("Incendiary Grenade", AbilitySlot::Grenade, DamageType::Solar, 90.0)
        }
    }

    pub fn pulse_grenade() -> AbilityDef {
        AbilityDef {
            damage: 90.0,
            radius: 4.0,
            on_hit: vec![(StatusKind::Jolt, 1)],
            ..AbilityDef::new("Pulse Grenade", AbilitySlot::Grenade, DamageType::Arc, 90.0)
        }
    }

    pub fn vortex_grenade() -> AbilityDef {
        AbilityDef {
            damage: 110.0,
            radius: 4.0,
            on_hit: vec![(StatusKind::Volatile, 1)],
            ..AbilityDef::new("Vortex Grenade", AbilitySlot::Grenade, DamageType::Void, 105.0)
        }
    }

    pub fn coldsnap_grenade() -> AbilityDef {
        AbilityDef {
            damage: 40.0,
            on_hit: vec![(StatusKind::Slow, 100)],
            ..AbilityDef::new("Coldsnap Grenade", AbilitySlot::Grenade, DamageType::Stasis, 105.0)
        }
    }

    pub fn grapple_grenade() -> AbilityDef {
        AbilityDef { max_charges: 2, ..AbilityDef::new("Grapple", AbilitySlot::Grenade, DamageType::Strand, 60.0) }
    }

    pub fn shield_throw() -> AbilityDef {
        AbilityDef {
            damage: 70.0,
            max_charges: 1,
            on_hit: vec![(StatusKind::Weaken, 1)],
            on_self: vec![],
            overshield: 40.0,
            ..AbilityDef::new("Shield Throw", AbilitySlot::Melee, DamageType::Void, 70.0)
        }
    }

    pub fn threaded_spike() -> AbilityDef {
        AbilityDef {
            damage: 70.0,
            on_hit: vec![(StatusKind::Sever, 1)],
            ..AbilityDef::new("Threaded Spike", AbilitySlot::Melee, DamageType::Strand, 70.0)
        }
    }

    pub fn healing_rift() -> AbilityDef {
        AbilityDef {
            on_self: vec![(StatusKind::Restoration, 2)],
            ..AbilityDef::new("Healing Rift", AbilitySlot::ClassAbility, DamageType::Solar, 80.0)
        }
    }

    pub fn gambler_dodge() -> AbilityDef {
        AbilityDef::new("Gambler's Dodge", AbilitySlot::ClassAbility, DamageType::Kinetic, 45.0)
    }

    pub fn towering_barricade() -> AbilityDef {
        AbilityDef {
            on_self: vec![(StatusKind::Radiant, 1)],
            ..AbilityDef::new("Towering Barricade", AbilitySlot::ClassAbility, DamageType::Solar, 60.0)
        }
    }

    pub fn nova_bomb() -> AbilityDef {
        AbilityDef {
            damage: 3000.0,
            radius: 7.0,
            on_hit: vec![(StatusKind::Volatile, 1)],
            ..AbilityDef::new("Nova Bomb", AbilitySlot::Super, DamageType::Void, 450.0)
        }
    }

    pub fn thundercrash() -> AbilityDef {
        AbilityDef {
            damage: 4000.0,
            radius: 6.0,
            on_hit: vec![(StatusKind::Jolt, 1)],
            ..AbilityDef::new("Thundercrash", AbilitySlot::Super, DamageType::Arc, 450.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regenerates_into_charges() {
        let mut a = AbilityState::new(presets::grapple_grenade());
        assert!(a.consume() && a.consume());
        assert!(!a.consume());
        assert_eq!(a.tick(30.0, 1.0), 0);
        assert_eq!(a.tick(30.0, 1.0), 1);
        assert_eq!(a.tick(60.0, 2.0), 1);
        assert!(a.is_full());
        assert_eq!(a.tick(60.0, 1.0), 0, "full abilities do not bank energy");
    }

    #[test]
    fn energy_overflow_spills_into_next_charge() {
        let mut a = AbilityState::new(presets::grapple_grenade());
        a.consume();
        a.consume();
        assert_eq!(a.add_energy(1.5), 1);
        assert!((a.energy - 0.5).abs() < 1e-6);
    }

    #[test]
    fn supers_start_empty() {
        let mut l = Loadout::default();
        l.equip(presets::nova_bomb());
        l.equip(presets::vortex_grenade());
        assert!(!l.get(AbilitySlot::Super).unwrap().is_ready());
        assert!(l.get(AbilitySlot::Grenade).unwrap().is_ready());
        assert!(l.get(AbilitySlot::Melee).is_none());
    }
}
