//! # guardian_combat
//!
//! An engine-agnostic combat sandbox modelled on Destiny 2's game mechanics,
//! for use in other games and mods.
//!
//! It handles the rules: damage types, elemental shields, buff/debuff
//! stacking, status effects (Scorch/Ignite, Jolt, Volatile, Slow/Freeze/
//! Shatter, Suspend, Unravel...), weapons and time-to-kill, ability energy
//! and cooldowns, armor stats, Orbs of Power, and champions. It does **not**
//! do rendering, physics or hit detection. Your engine reports hits; the
//! sandbox works out what they do.
//!
//! All numbers live in [`SandboxConfig`](config::SandboxConfig) and are approximations meant as a
//! starting point. Change them in code, or enable the `serde` feature and
//! load them from a data file.
//!
//! ```
//! use guardian_combat::prelude::*;
//!
//! let mut sb = Sandbox::new(SandboxConfig::pve());
//! let player = sb.spawn(
//!     Combatant::guardian("Player", Team(0), GuardianClass::Warlock,
//!         StatBlock::new(100, 50, 50, 100, 50, 50),
//!         &sb.config.guardian, &sb.config.stats)
//!     .with_weapon(weapon::presets::hand_cannon_140(DamageType::Solar))
//!     .with_ability(catalog::ability_named("Incendiary Grenade").unwrap()),
//! );
//! let enemy = sb.spawn(
//!     Combatant::enemy("Acolyte", Team(1), Rank::Minor, 250.0)
//!         .with_shield(60.0, Some(DamageType::Solar)),
//! );
//!
//! sb.fire_weapon(player, Some(enemy), Shot::precision()).unwrap();
//! sb.use_ability(player, AbilitySlot::Grenade, Some(enemy)).unwrap();
//! sb.tick(1.0 / 60.0);
//!
//! for event in sb.events() {
//!     if let CombatEvent::Killed { victim, .. } = event {
//!         println!("{victim:?} died");
//!     }
//! }
//! ```

pub mod ability;
pub mod buffs;
pub mod catalog;
pub mod combatant;
pub mod config;
pub mod damage;
pub mod effect;
pub mod element;
pub mod health;
pub mod sandbox;
pub mod stats;
pub mod status;
pub mod weapon;

/// Everything most hosts need.
pub mod prelude {
    pub use crate::ability::{AbilityDef, AbilitySlot, AbilityState, Loadout};
    pub use crate::buffs::{Modifier, ModifierCategory, ModifierScope};
    pub use crate::catalog::{self, SubclassKit};
    pub use crate::combatant::{ChampionKind, Combatant, CombatantId, Rank, Team};
    pub use crate::config::{GameMode, SandboxConfig};
    pub use crate::damage::{DamageInstance, SourceKind};
    pub use crate::effect::{AspectDef, Effect, MoveKind};
    pub use crate::element::{DamageType, GuardianClass, SubclassElement};
    pub use crate::health::HealthPool;
    pub use crate::sandbox::{AbilityTarget, ActionError, CombatEvent, Sandbox, Shot};
    pub use crate::stats::{Stat, StatBlock};
    pub use crate::status::{StatusKind, TriggerKind};
    pub use crate::weapon::{self, AmmoType, WeaponArchetype, WeaponDef};
}
