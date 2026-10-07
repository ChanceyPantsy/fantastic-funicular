//! What abilities and aspects *do*, as data.
//!
//! An ability is a list of [`Effect`]s run when it is cast: an explosion,
//! a lingering zone (rift, well, bubble, storm), homing projectiles, chain
//! lightning, buffs, healing, movement, or a roaming super with its own
//! light and heavy attacks. Aspects are [`Passive`]s: effects that run when
//! something happens (a kill, a cast, a status applied, an Ignite...).

use crate::ability::AbilitySlot;
use crate::buffs::Modifier;
use crate::damage::SourceKind;
use crate::element::DamageType;
use crate::status::{StatusKind, TriggerKind};

/// Movement the host should perform. The sandbox only reports it, as
/// [`CombatEvent::Move`](crate::sandbox::CombatEvent::Move).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MoveKind {
    /// Quick ground dash (dodges, Icarus Dash).
    Dash,
    /// Short-range teleport (Blink, Nova Warp).
    Blink,
    /// Lunge toward the target (melee lunges, shoulder charges).
    Lunge,
    /// Leap up and slam down at the target (Thundercrash, Glacial Quake).
    Slam,
    /// Pull to a point (Grapple).
    Grapple,
    /// Short vertical boost (Phoenix Dive, Thruster).
    Hop,
}

/// Whether a zone stays where it was cast or follows its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Anchor {
    Point,
    Caster,
}

/// A lingering area: rifts, wells, bubbles, barricades, storm grenades,
/// Duskfields, Shackle fields...
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ZoneDef {
    pub radius: f32,
    pub duration: f32,
    /// Seconds between pulses.
    pub tick: f32,
    pub anchor: Anchor,
    /// Damage to each hostile inside per pulse.
    pub enemy_damage: f32,
    /// Statuses applied to each hostile inside per pulse.
    pub enemy_statuses: Vec<(StatusKind, u32)>,
    /// Healing for each ally inside (including the owner) per pulse.
    pub ally_heal: f32,
    pub ally_overshield: f32,
    pub ally_statuses: Vec<(StatusKind, u32)>,
    /// Modifiers allies inside keep while they stay in (Weapons of Light,
    /// barricade cover...). Refreshed each pulse; lapse shortly after leaving.
    pub ally_modifiers: Vec<Modifier>,
    /// The host should treat it as cover that blocks enemy projectiles.
    pub blocks_projectiles: bool,
}

impl ZoneDef {
    pub fn new(radius: f32, duration: f32, anchor: Anchor) -> Self {
        Self {
            radius,
            duration,
            tick: 0.5,
            anchor,
            enemy_damage: 0.0,
            enemy_statuses: Vec::new(),
            ally_heal: 0.0,
            ally_overshield: 0.0,
            ally_statuses: Vec::new(),
            ally_modifiers: Vec::new(),
            blocks_projectiles: false,
        }
    }

    pub fn damage(mut self, per_pulse: f32) -> Self {
        self.enemy_damage = per_pulse;
        self
    }

    pub fn enemy_status(mut self, kind: StatusKind, stacks: u32) -> Self {
        self.enemy_statuses.push((kind, stacks));
        self
    }

    pub fn heal(mut self, per_pulse: f32) -> Self {
        self.ally_heal = per_pulse;
        self
    }

    pub fn overshield(mut self, per_pulse: f32) -> Self {
        self.ally_overshield = per_pulse;
        self
    }

    pub fn ally_status(mut self, kind: StatusKind, stacks: u32) -> Self {
        self.ally_statuses.push((kind, stacks));
        self
    }

    pub fn ally_modifier(mut self, m: Modifier) -> Self {
        self.ally_modifiers.push(m);
        self
    }

    pub fn cover(mut self) -> Self {
        self.blocks_projectiles = true;
        self
    }

    pub fn pulse(mut self, seconds: f32) -> Self {
        self.tick = seconds;
        self
    }
}

/// One attack of a roaming super (or anything else with a cooldown).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SuperAttack {
    pub name: String,
    pub cooldown: f32,
    /// How far from the user the host should allow targeting.
    pub range: f32,
    pub effects: Vec<Effect>,
}

impl SuperAttack {
    pub fn new(name: impl Into<String>, cooldown: f32, range: f32, effects: Vec<Effect>) -> Self {
        Self { name: name.into(), cooldown, range, effects }
    }
}

/// A super you stay in for a while (Golden Gun, Stormtrance, Hammer of Sol...).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoamingSuper {
    pub duration: f32,
    pub light: SuperAttack,
    pub heavy: Option<SuperAttack>,
    /// Damage resistance while active.
    pub damage_resist: f32,
    pub move_speed_mult: f32,
    /// Ends after this many light attacks (Golden Gun's shots).
    pub light_uses: Option<u32>,
}

impl RoamingSuper {
    pub fn new(duration: f32, light: SuperAttack) -> Self {
        Self { duration, light, heavy: None, damage_resist: 0.5, move_speed_mult: 1.0, light_uses: None }
    }

    pub fn heavy(mut self, attack: SuperAttack) -> Self {
        self.heavy = Some(attack);
        self
    }

    pub fn uses(mut self, n: u32) -> Self {
        self.light_uses = Some(n);
        self
    }

    pub fn speed(mut self, mult: f32) -> Self {
        self.move_speed_mult = mult;
        self
    }

    pub fn resist(mut self, r: f32) -> Self {
        self.damage_resist = r;
        self
    }
}

/// One thing an ability, super attack or aspect does.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Effect {
    /// Damage at the target. With a target combatant it is hit directly;
    /// with `radius > 0` everything hostile within `radius` is hit too.
    Blast {
        damage: f32,
        radius: f32,
        statuses: Vec<(StatusKind, u32)>,
    },
    Zone(ZoneDef),
    /// Homing projectiles: hit up to `count` nearest hostiles within
    /// `range` of the target point (the target combatant first).
    Seekers {
        count: u32,
        damage: f32,
        range: f32,
        statuses: Vec<(StatusKind, u32)>,
    },
    /// Hits the target, then jumps up to `jumps` times to the nearest new
    /// hostile within `range`.
    Chain {
        damage: f32,
        jumps: u32,
        range: f32,
        statuses: Vec<(StatusKind, u32)>,
    },
    SelfStatus(StatusKind, u32),
    /// Status for the user and allies within `radius`.
    AllyStatus {
        kind: StatusKind,
        stacks: u32,
        radius: f32,
    },
    /// A damage modifier on the user (use [`Modifier::timed`]).
    Buff(Modifier),
    Overshield(f32),
    Heal(f32),
    /// Heals the user and allies within `radius`.
    HealAllies {
        amount: f32,
        radius: f32,
    },
    /// Ability energy for the user, as a fraction of a charge.
    Energy {
        slot: AbilitySlot,
        amount: f32,
    },
    /// Instantly refills the active weapon's magazine.
    Reload,
    Move {
        kind: MoveKind,
        distance: f32,
    },
    /// Enter a roaming super.
    Roam(RoamingSuper),
}

impl Effect {
    pub fn blast(damage: f32, radius: f32) -> Self {
        Effect::Blast { damage, radius, statuses: Vec::new() }
    }

    pub fn blast_with(damage: f32, radius: f32, statuses: &[(StatusKind, u32)]) -> Self {
        Effect::Blast { damage, radius, statuses: statuses.to_vec() }
    }

    pub fn seekers(count: u32, damage: f32, range: f32, statuses: &[(StatusKind, u32)]) -> Self {
        Effect::Seekers { count, damage, range, statuses: statuses.to_vec() }
    }

    pub fn chain(damage: f32, jumps: u32, range: f32, statuses: &[(StatusKind, u32)]) -> Self {
        Effect::Chain { damage, jumps, range, statuses: statuses.to_vec() }
    }

    pub fn energy(slot: AbilitySlot, amount: f32) -> Self {
        Effect::Energy { slot, amount }
    }

    pub fn moves(kind: MoveKind, distance: f32) -> Self {
        Effect::Move { kind, distance }
    }

    /// Whether this effect needs a target (combatant or point) to do anything.
    pub fn needs_target(&self) -> bool {
        matches!(self, Effect::Blast { .. } | Effect::Seekers { .. } | Effect::Chain { .. })
    }
}

/// Which kills a passive reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum KillFilter {
    Any,
    Weapon,
    Precision,
    Ability(AbilitySlot),
    /// Grenade, melee or class ability.
    AnyAbility,
    Super,
    /// The victim had this status when it died.
    VictimHad(StatusKind),
}

impl KillFilter {
    pub fn matches(self, kind: &SourceKind, precision: bool, victim_statuses: &[StatusKind]) -> bool {
        match self {
            KillFilter::Any => true,
            KillFilter::Weapon => kind.is_weapon(),
            KillFilter::Precision => kind.is_weapon() && precision,
            KillFilter::Ability(slot) => *kind == SourceKind::from_slot(slot),
            KillFilter::AnyAbility => {
                matches!(kind, SourceKind::Grenade | SourceKind::Melee | SourceKind::ClassAbility)
            }
            KillFilter::Super => *kind == SourceKind::Super,
            KillFilter::VictimHad(s) => victim_statuses.contains(&s),
        }
    }
}

/// What sets a passive off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PassiveTrigger {
    /// The owner gets a kill.
    Kill(KillFilter),
    /// The owner casts an ability.
    Cast(AbilitySlot),
    /// The owner applies this status to an enemy.
    Applies(StatusKind),
    /// A trigger (Ignite, Shatter...) the owner caused goes off.
    Causes(TriggerKind),
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Passive {
    pub trigger: PassiveTrigger,
    pub effects: Vec<Effect>,
    /// Minimum seconds between activations.
    pub cooldown: f32,
}

impl Passive {
    pub fn new(trigger: PassiveTrigger, cooldown: f32, effects: Vec<Effect>) -> Self {
        Self { trigger, effects, cooldown }
    }
}

/// A subclass aspect: always-on modifiers plus passives.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AspectDef {
    pub name: String,
    pub description: String,
    /// Damage type of any damage its passives deal.
    pub damage_type: DamageType,
    pub modifiers: Vec<Modifier>,
    pub passives: Vec<Passive>,
}

impl AspectDef {
    pub fn new(name: impl Into<String>, damage_type: DamageType, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            damage_type,
            modifiers: Vec::new(),
            passives: Vec::new(),
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifiers.push(m);
        self
    }

    pub fn on(mut self, trigger: PassiveTrigger, cooldown: f32, effects: Vec<Effect>) -> Self {
        self.passives.push(Passive::new(trigger, cooldown, effects));
        self
    }
}

/// An equipped aspect with its passives' cooldown timers.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AspectState {
    pub def: AspectDef,
    pub cooldowns: Vec<f32>,
}

impl AspectState {
    pub fn new(def: AspectDef) -> Self {
        Self { cooldowns: vec![0.0; def.passives.len()], def }
    }
}

/// A roaming super in progress.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveSuper {
    pub name: String,
    pub damage_type: DamageType,
    pub def: RoamingSuper,
    pub remaining: f32,
    pub light_cooldown: f32,
    pub heavy_cooldown: f32,
    pub light_uses_left: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_filters() {
        let w = SourceKind::Weapon {
            ammo: crate::weapon::AmmoType::Primary,
            archetype: crate::weapon::WeaponArchetype::HandCannon,
        };
        assert!(KillFilter::Precision.matches(&w, true, &[]));
        assert!(!KillFilter::Precision.matches(&w, false, &[]));
        assert!(KillFilter::AnyAbility.matches(&SourceKind::Melee, false, &[]));
        assert!(!KillFilter::AnyAbility.matches(&SourceKind::Super, false, &[]));
        assert!(KillFilter::VictimHad(StatusKind::Scorch).matches(&w, false, &[StatusKind::Scorch]));
    }
}
