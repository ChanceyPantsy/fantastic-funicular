//! Health, shields and overshields.
//!
//! Damage is absorbed in order: overshield → shield → health. A shield can
//! have an element; elemental shields take extra damage from a matching
//! element and less from others (see [`ShieldRules`]).

use crate::element::DamageType;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShieldRules {
    /// Damage multiplier against an elemental shield of the same element.
    pub matching_mult: f32,
    /// Damage multiplier against an elemental shield of another element.
    pub mismatched_mult: f32,
    /// Explosion damage (as a fraction of the shield's max) when a shield is
    /// broken by its matching element.
    pub break_explosion_fraction: f32,
    pub break_explosion_radius: f32,
}

impl Default for ShieldRules {
    fn default() -> Self {
        Self { matching_mult: 2.0, mismatched_mult: 0.5, break_explosion_fraction: 0.5, break_explosion_radius: 4.0 }
    }
}

impl ShieldRules {
    /// Multiplier for `damage_type` against a shield of `shield_element`
    /// (`None` = a plain guardian-style shield, always 1.0).
    pub fn mult(&self, shield_element: Option<DamageType>, damage_type: DamageType) -> f32 {
        match shield_element {
            None => 1.0,
            Some(e) if e == damage_type => self.matching_mult,
            Some(_) => self.mismatched_mult,
        }
    }
}

/// Regeneration settings. Regeneration starts `delay` seconds after the
/// last damage taken; health refills before the shield.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Regen {
    pub delay: f32,
    pub health_per_sec: f32,
    pub shield_per_sec: f32,
}

impl Regen {
    pub const NONE: Regen = Regen { delay: f32::INFINITY, health_per_sec: 0.0, shield_per_sec: 0.0 };
}

/// How a hit was absorbed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Absorption {
    pub overshield: f32,
    pub shield: f32,
    pub health: f32,
    /// Damage left over after health hit zero.
    pub overkill: f32,
    pub shield_broken: bool,
    pub killed: bool,
}

impl Absorption {
    pub fn total(&self) -> f32 {
        self.overshield + self.shield + self.health
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HealthPool {
    pub max_health: f32,
    pub health: f32,
    pub max_shield: f32,
    pub shield: f32,
    pub shield_element: Option<DamageType>,
    /// Whether damage that breaks the shield carries into health.
    pub bleed_through: bool,
    pub overshield: f32,
    pub max_overshield: f32,
    pub overshield_decay_per_sec: f32,
    pub regen: Regen,
    /// Multiplier on shield regeneration (e.g. from the Health stat).
    pub shield_regen_mult: f32,
    since_damage: f32,
}

impl HealthPool {
    /// Health only, no shield and no regeneration.
    pub fn new(max_health: f32) -> Self {
        Self {
            max_health,
            health: max_health,
            max_shield: 0.0,
            shield: 0.0,
            shield_element: None,
            bleed_through: true,
            overshield: 0.0,
            max_overshield: 100.0,
            overshield_decay_per_sec: 0.0,
            regen: Regen::NONE,
            shield_regen_mult: 1.0,
            since_damage: 0.0,
        }
    }

    pub fn with_shield(mut self, max_shield: f32, element: Option<DamageType>) -> Self {
        self.max_shield = max_shield;
        self.shield = max_shield;
        self.shield_element = element;
        self
    }

    pub fn with_regen(mut self, regen: Regen) -> Self {
        self.regen = regen;
        self
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }

    pub fn total(&self) -> f32 {
        self.health + self.shield + self.overshield
    }

    pub fn max_total(&self) -> f32 {
        self.max_health + self.max_shield
    }

    pub fn seconds_since_damage(&self) -> f32 {
        self.since_damage
    }

    /// Applies `amount` damage of `damage_type`. Elemental shield multipliers
    /// apply only to the part absorbed by the shield.
    pub fn absorb(&mut self, amount: f32, damage_type: DamageType, rules: &ShieldRules) -> Absorption {
        let mut out = Absorption::default();
        if self.is_dead() || amount <= 0.0 {
            return out;
        }
        self.since_damage = 0.0;
        let mut left = amount;

        let os = left.min(self.overshield);
        self.overshield -= os;
        out.overshield = os;
        left -= os;

        if left > 0.0 && self.shield > 0.0 {
            let mult = rules.mult(self.shield_element, damage_type);
            let wanted = left * mult;
            let taken = wanted.min(self.shield);
            self.shield -= taken;
            out.shield = taken;
            left -= taken / mult;
            if self.shield <= 0.0 {
                self.shield = 0.0;
                out.shield_broken = true;
                if !self.bleed_through {
                    left = 0.0;
                }
            }
        }

        if left > 0.0 {
            let h = left.min(self.health);
            self.health -= h;
            out.health = h;
            out.overkill = left - h;
            if self.health <= 0.0 {
                self.health = 0.0;
                out.killed = true;
            }
        }
        out
    }

    /// Restores health (not shield). Returns the amount actually restored.
    pub fn heal(&mut self, amount: f32) -> f32 {
        if self.is_dead() {
            return 0.0;
        }
        let before = self.health;
        self.health = (self.health + amount.max(0.0)).min(self.max_health);
        self.health - before
    }

    /// Fully restores health and shield.
    pub fn restore_full(&mut self) {
        self.health = self.max_health;
        self.shield = self.max_shield;
    }

    pub fn add_overshield(&mut self, amount: f32) {
        self.overshield = (self.overshield + amount.max(0.0)).min(self.max_overshield);
    }

    pub fn tick(&mut self, dt: f32) {
        if self.is_dead() {
            return;
        }
        self.since_damage += dt;
        self.overshield = (self.overshield - self.overshield_decay_per_sec * dt).max(0.0);
        if self.since_damage < self.regen.delay {
            return;
        }
        let mut budget = dt;
        if self.health < self.max_health && self.regen.health_per_sec > 0.0 {
            let missing = self.max_health - self.health;
            let need = missing / self.regen.health_per_sec;
            let used = need.min(budget);
            self.health += self.regen.health_per_sec * used;
            budget -= used;
        }
        let shield_rate = self.regen.shield_per_sec * self.shield_regen_mult;
        if budget > 0.0 && self.shield < self.max_shield && shield_rate > 0.0 {
            self.shield = (self.shield + shield_rate * budget).min(self.max_shield);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_absorb_in_order() {
        let mut h = HealthPool::new(70.0).with_shield(130.0, None);
        h.add_overshield(30.0);
        let a = h.absorb(180.0, DamageType::Kinetic, &ShieldRules::default());
        assert_eq!((a.overshield, a.shield, a.health), (30.0, 130.0, 20.0));
        assert!(a.shield_broken && !a.killed);
        let a = h.absorb(100.0, DamageType::Kinetic, &ShieldRules::default());
        assert!(a.killed);
        assert_eq!(a.overkill, 50.0);
    }

    #[test]
    fn matching_element_shreds_shields() {
        let rules = ShieldRules::default();
        let mut h = HealthPool::new(100.0).with_shield(100.0, Some(DamageType::Arc));
        let a = h.absorb(60.0, DamageType::Arc, &rules);
        assert_eq!(a.shield, 100.0);
        assert!(a.shield_broken);
        assert_eq!(a.health, 10.0, "50 raw broke the shield, 10 carried through");

        let mut h = HealthPool::new(100.0).with_shield(100.0, Some(DamageType::Arc));
        let a = h.absorb(60.0, DamageType::Solar, &rules);
        assert_eq!(a.shield, 30.0);
        assert_eq!(h.health, 100.0);
    }

    #[test]
    fn no_bleed_through_stops_at_shield() {
        let mut h = HealthPool::new(100.0).with_shield(10.0, None);
        h.bleed_through = false;
        let a = h.absorb(50.0, DamageType::Kinetic, &ShieldRules::default());
        assert_eq!(a.health, 0.0);
        assert!(a.shield_broken);
    }

    #[test]
    fn regen_waits_then_refills_health_before_shield() {
        let regen = Regen { delay: 2.0, health_per_sec: 50.0, shield_per_sec: 100.0 };
        let mut h = HealthPool::new(70.0).with_shield(130.0, None).with_regen(regen);
        h.absorb(150.0, DamageType::Kinetic, &ShieldRules::default());
        h.tick(1.0);
        assert_eq!(h.health, 50.0);
        // Regen starts: 0.4s refills the missing 20 health, 0.6s goes to shield.
        h.tick(1.0);
        assert_eq!(h.health, 70.0);
        assert!((h.shield - 60.0).abs() < 1e-3);
        h.tick(1.0);
        assert_eq!(h.shield, 130.0);
    }
}
