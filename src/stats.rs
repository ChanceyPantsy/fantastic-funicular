//! Armor stats and what they do.
//!
//! Uses the six-stat, 0–200 model (Weapons, Health, Class, Grenade, Super,
//! Melee). Points 0–100 give each stat's base benefit; points 100–200 give
//! its "enhanced" benefit. Every effect is a [`Curve`] in [`StatRules`], so a
//! host can retune it or approximate the older 0–100 tier system instead.

/// One of the six armor stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Stat {
    Weapons,
    Health,
    Class,
    Grenade,
    Super,
    Melee,
}

impl Stat {
    pub const ALL: [Stat; 6] = [Stat::Weapons, Stat::Health, Stat::Class, Stat::Grenade, Stat::Super, Stat::Melee];

    fn index(self) -> usize {
        self as usize
    }
}

/// Highest value any single stat can reach.
pub const STAT_MAX: u16 = 200;
/// Where the base benefit ends and the enhanced benefit starts.
pub const STAT_ENHANCED_START: u16 = 100;

/// A character's six stat values, each clamped to `0..=STAT_MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatBlock {
    values: [u16; 6],
}

impl StatBlock {
    pub fn new(weapons: u16, health: u16, class: u16, grenade: u16, super_: u16, melee: u16) -> Self {
        let mut block = Self::default();
        block.set(Stat::Weapons, weapons);
        block.set(Stat::Health, health);
        block.set(Stat::Class, class);
        block.set(Stat::Grenade, grenade);
        block.set(Stat::Super, super_);
        block.set(Stat::Melee, melee);
        block
    }

    pub fn get(&self, stat: Stat) -> u16 {
        self.values[stat.index()]
    }

    pub fn set(&mut self, stat: Stat, value: u16) {
        self.values[stat.index()] = value.min(STAT_MAX);
    }

    /// Adds (or with a negative delta, removes) points, clamped to range.
    pub fn add(&mut self, stat: Stat, delta: i32) {
        let v = (self.get(stat) as i32 + delta).clamp(0, STAT_MAX as i32);
        self.set(stat, v as u16);
    }

    pub fn total(&self) -> u32 {
        self.values.iter().map(|&v| v as u32).sum()
    }
}

/// A piecewise-linear curve. Inputs outside the first/last point clamp to
/// the end values. Points must be sorted by `x`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Curve {
    pub points: Vec<(f32, f32)>,
}

impl Curve {
    pub fn new(points: Vec<(f32, f32)>) -> Self {
        debug_assert!(points.windows(2).all(|w| w[0].0 <= w[1].0), "curve points must be sorted by x");
        Self { points }
    }

    /// A straight line from `(x0, y0)` to `(x1, y1)`.
    pub fn linear(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(vec![(x0, y0), (x1, y1)])
    }

    pub fn constant(y: f32) -> Self {
        Self::new(vec![(0.0, y)])
    }

    pub fn eval(&self, x: f32) -> f32 {
        let pts = &self.points;
        match pts.len() {
            0 => 0.0,
            1 => pts[0].1,
            _ => {
                if x <= pts[0].0 {
                    return pts[0].1;
                }
                for w in pts.windows(2) {
                    let ((x0, y0), (x1, y1)) = (w[0], w[1]);
                    if x <= x1 {
                        if x1 == x0 {
                            return y1;
                        }
                        let t = (x - x0) / (x1 - x0);
                        return y0 + (y1 - y0) * t;
                    }
                }
                pts[pts.len() - 1].1
            }
        }
    }
}

/// How stat points turn into gameplay effects. Defaults are approximations
/// meant as a sensible starting point, not datamined values.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatRules {
    /// Grenade/Melee/Class/Super stat (0–100) → ability energy regen multiplier.
    /// Ability `base_cooldown`s are defined at a multiplier of 1.0.
    pub ability_regen: Curve,
    /// Grenade/Melee/Super stat (100–200) → bonus damage for that ability.
    pub ability_damage_bonus: Curve,
    /// Class stat (100–200) → overshield granted when the class ability is used.
    pub class_overshield: Curve,
    /// Weapons stat (0–100) → reload/handling bonus (fraction, e.g. 0.1 = 10% faster).
    pub weapon_handling_bonus: Curve,
    /// Weapons stat (0–100) → weapon damage bonus vs minor and major combatants (PvE).
    pub weapon_damage_vs_minor: Curve,
    /// Weapons stat (100–200) → weapon damage bonus vs bosses and vehicles (PvE).
    pub weapon_damage_vs_boss: Curve,
    /// Health stat (0–100) → health restored when picking up an Orb of Power.
    pub orb_healing: Curve,
    /// Health stat (0–100) → flinch reduction (fraction).
    pub flinch_resist: Curve,
    /// Health stat (100–200) → extra shield capacity.
    pub shield_bonus: Curve,
    /// Health stat (100–200) → shield recharge rate multiplier.
    pub shield_recharge: Curve,
}

impl Default for StatRules {
    fn default() -> Self {
        let lo = 0.0;
        let mid = STAT_ENHANCED_START as f32;
        let hi = STAT_MAX as f32;
        Self {
            ability_regen: Curve::new(vec![(lo, 1.0), (50.0, 1.6), (mid, 2.5)]),
            ability_damage_bonus: Curve::linear(mid, 0.0, hi, 0.3),
            class_overshield: Curve::linear(mid, 0.0, hi, 40.0),
            weapon_handling_bonus: Curve::linear(lo, 0.0, mid, 0.1),
            weapon_damage_vs_minor: Curve::linear(lo, 0.0, mid, 0.15),
            weapon_damage_vs_boss: Curve::linear(mid, 0.0, hi, 0.15),
            orb_healing: Curve::linear(lo, 0.0, mid, 70.0),
            flinch_resist: Curve::linear(lo, 0.0, mid, 0.3),
            shield_bonus: Curve::linear(mid, 0.0, hi, 20.0),
            shield_recharge: Curve::linear(mid, 1.0, hi, 1.45),
        }
    }
}

/// The evaluated effects of a [`StatBlock`] under some [`StatRules`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DerivedStats {
    pub grenade_regen: f32,
    pub melee_regen: f32,
    pub class_regen: f32,
    pub super_regen: f32,
    pub grenade_damage_bonus: f32,
    pub melee_damage_bonus: f32,
    pub super_damage_bonus: f32,
    pub class_overshield: f32,
    pub weapon_handling_bonus: f32,
    pub weapon_damage_vs_minor: f32,
    pub weapon_damage_vs_boss: f32,
    pub orb_healing: f32,
    pub flinch_resist: f32,
    pub shield_bonus: f32,
    pub shield_recharge: f32,
}

impl StatRules {
    pub fn derive(&self, stats: &StatBlock) -> DerivedStats {
        let v = |s: Stat| stats.get(s) as f32;
        DerivedStats {
            grenade_regen: self.ability_regen.eval(v(Stat::Grenade)),
            melee_regen: self.ability_regen.eval(v(Stat::Melee)),
            class_regen: self.ability_regen.eval(v(Stat::Class)),
            super_regen: self.ability_regen.eval(v(Stat::Super)),
            grenade_damage_bonus: self.ability_damage_bonus.eval(v(Stat::Grenade)),
            melee_damage_bonus: self.ability_damage_bonus.eval(v(Stat::Melee)),
            super_damage_bonus: self.ability_damage_bonus.eval(v(Stat::Super)),
            class_overshield: self.class_overshield.eval(v(Stat::Class)),
            weapon_handling_bonus: self.weapon_handling_bonus.eval(v(Stat::Weapons)),
            weapon_damage_vs_minor: self.weapon_damage_vs_minor.eval(v(Stat::Weapons)),
            weapon_damage_vs_boss: self.weapon_damage_vs_boss.eval(v(Stat::Weapons)),
            orb_healing: self.orb_healing.eval(v(Stat::Health)),
            flinch_resist: self.flinch_resist.eval(v(Stat::Health)),
            shield_bonus: self.shield_bonus.eval(v(Stat::Health)),
            shield_recharge: self.shield_recharge.eval(v(Stat::Health)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_block_clamps() {
        let mut s = StatBlock::new(250, 0, 0, 0, 0, 0);
        assert_eq!(s.get(Stat::Weapons), STAT_MAX);
        s.add(Stat::Health, -10);
        assert_eq!(s.get(Stat::Health), 0);
        s.add(Stat::Health, 30);
        assert_eq!(s.total(), 230);
    }

    #[test]
    fn curve_interpolates_and_clamps() {
        let c = Curve::new(vec![(0.0, 1.0), (50.0, 2.0), (100.0, 4.0)]);
        assert_eq!(c.eval(-5.0), 1.0);
        assert_eq!(c.eval(25.0), 1.5);
        assert_eq!(c.eval(75.0), 3.0);
        assert_eq!(c.eval(500.0), 4.0);
    }

    #[test]
    fn enhanced_benefits_only_above_100() {
        let rules = StatRules::default();
        let d = rules.derive(&StatBlock::new(100, 100, 100, 100, 100, 100));
        assert_eq!(d.grenade_damage_bonus, 0.0);
        assert_eq!(d.shield_bonus, 0.0);
        let d = rules.derive(&StatBlock::new(200, 200, 200, 200, 200, 200));
        assert!(d.grenade_damage_bonus > 0.0);
        assert!(d.shield_bonus > 0.0);
        assert_eq!(d.grenade_regen, rules.ability_regen.eval(100.0));
    }
}
