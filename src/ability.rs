//! Grenades, melees, class abilities and supers: energy, charges and
//! cooldowns.
//!
//! Every ability fills an energy bar from 0 to 1. A full bar becomes a
//! charge. Energy comes from passive regeneration (scaled by the matching
//! stat), Orbs of Power, and for supers, from dealing damage and getting kills.

use crate::effect::{Effect, RoamingSuper};
use crate::element::DamageType;

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
    pub description: String,
    pub slot: AbilitySlot,
    pub damage_type: DamageType,
    /// Seconds to regenerate one charge at a regen multiplier of 1.0.
    pub base_cooldown: f32,
    pub max_charges: u32,
    /// How far away the host should let the user target (melee ~4,
    /// grenades ~20).
    pub range: f32,
    /// What happens on cast, in order.
    pub effects: Vec<Effect>,
}

impl AbilityDef {
    pub fn new(name: impl Into<String>, slot: AbilitySlot, damage_type: DamageType, base_cooldown: f32) -> Self {
        let range = match slot {
            AbilitySlot::Melee => 4.0,
            AbilitySlot::Grenade => 20.0,
            AbilitySlot::ClassAbility => 0.0,
            AbilitySlot::Super => 25.0,
        };
        Self {
            name: name.into(),
            description: String::new(),
            slot,
            damage_type,
            base_cooldown,
            max_charges: 1,
            range,
            effects: Vec::new(),
        }
    }

    pub fn describe(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    pub fn charges(mut self, n: u32) -> Self {
        self.max_charges = n;
        self
    }

    pub fn range(mut self, r: f32) -> Self {
        self.range = r;
        self
    }

    pub fn effect(mut self, e: Effect) -> Self {
        self.effects.push(e);
        self
    }

    /// Whether any effect needs a target to do something.
    pub fn is_targeted(&self) -> bool {
        self.effects.iter().any(Effect::needs_target)
    }

    /// The roaming super this ability starts, if any.
    pub fn roaming(&self) -> Option<&RoamingSuper> {
        self.effects.iter().find_map(|e| match e {
            Effect::Roam(r) => Some(r),
            _ => None,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn grapple() -> AbilityDef {
        AbilityDef::new("Grapple", AbilitySlot::Grenade, DamageType::Strand, 60.0).charges(2)
    }

    #[test]
    fn regenerates_into_charges() {
        let mut a = AbilityState::new(grapple());
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
        let mut a = AbilityState::new(grapple());
        a.consume();
        a.consume();
        assert_eq!(a.add_energy(1.5), 1);
        assert!((a.energy - 0.5).abs() < 1e-6);
    }

    #[test]
    fn supers_start_empty() {
        let mut l = Loadout::default();
        l.equip(AbilityDef::new("Nova Bomb", AbilitySlot::Super, DamageType::Void, 450.0));
        l.equip(AbilityDef::new("Vortex Grenade", AbilitySlot::Grenade, DamageType::Void, 105.0));
        assert!(!l.get(AbilitySlot::Super).unwrap().is_ready());
        assert!(l.get(AbilitySlot::Grenade).unwrap().is_ready());
        assert!(l.get(AbilitySlot::Melee).is_none());
    }
}
