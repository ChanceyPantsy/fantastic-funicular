//! Damage modifiers and the buff/debuff stacking rules.
//!
//! Destiny's damage buffs do not simply multiply together. They fall into
//! categories with their own stacking rule:
//!
//! | Category         | Lives on  | Rule                                         | Example                        |
//! |------------------|-----------|----------------------------------------------|--------------------------------|
//! | `Empowering`     | attacker  | only the strongest applies                   | Radiant, Well of Radiance      |
//! | `Debuff`         | target    | only the strongest applies                   | Weaken, Tractor Cannon         |
//! | `Surge`          | attacker  | summed, then capped                          | Elemental surge mods           |
//! | `Multiplicative` | either    | every one multiplies                         | Weapon perks (Kill Clip, etc.) |
//! | `Resist`         | target    | `1 - value` multiplied together, then capped | Woven Mail, Frost Armor        |
//!
//! The final multiplier is the product of the category results.

use crate::damage::SourceKind;
use crate::element::DamageType;
use crate::weapon::AmmoType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ModifierCategory {
    Empowering,
    Debuff,
    Surge,
    Multiplicative,
    Resist,
}

/// What kind of damage a modifier affects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ModifierScope {
    All,
    Weapons,
    /// Grenade, melee and class ability damage.
    Abilities,
    Grenade,
    Melee,
    Super,
    Ammo(AmmoType),
    Element(DamageType),
}

impl ModifierScope {
    pub fn matches(self, kind: &SourceKind, damage_type: DamageType) -> bool {
        match self {
            ModifierScope::All => true,
            ModifierScope::Weapons => kind.is_weapon(),
            ModifierScope::Abilities => {
                matches!(kind, SourceKind::Grenade | SourceKind::Melee | SourceKind::ClassAbility)
            }
            ModifierScope::Grenade => matches!(kind, SourceKind::Grenade),
            ModifierScope::Melee => matches!(kind, SourceKind::Melee),
            ModifierScope::Super => matches!(kind, SourceKind::Super),
            ModifierScope::Ammo(a) => kind.ammo() == Some(a),
            ModifierScope::Element(e) => damage_type == e,
        }
    }
}

/// A single damage modifier. `value` is a fraction: `0.25` means +25% for
/// boosting categories, and 25% damage reduction for `Resist`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Modifier {
    pub name: String,
    pub category: ModifierCategory,
    pub scope: ModifierScope,
    pub value: f32,
    /// Multiply `value` by the stack count of the status that provides it.
    pub per_stack: bool,
    /// Seconds left; `None` means it lasts until removed.
    pub remaining: Option<f32>,
}

impl Modifier {
    pub fn new(name: impl Into<String>, category: ModifierCategory, scope: ModifierScope, value: f32) -> Self {
        Self { name: name.into(), category, scope, value, per_stack: false, remaining: None }
    }

    pub fn per_stack(mut self) -> Self {
        self.per_stack = true;
        self
    }

    pub fn timed(mut self, seconds: f32) -> Self {
        self.remaining = Some(seconds);
        self
    }

    pub fn empowering(name: impl Into<String>, scope: ModifierScope, value: f32) -> Self {
        Self::new(name, ModifierCategory::Empowering, scope, value)
    }

    pub fn debuff(name: impl Into<String>, value: f32) -> Self {
        Self::new(name, ModifierCategory::Debuff, ModifierScope::All, value)
    }

    pub fn surge(name: impl Into<String>, scope: ModifierScope, value: f32) -> Self {
        Self::new(name, ModifierCategory::Surge, scope, value)
    }

    pub fn multiplicative(name: impl Into<String>, scope: ModifierScope, value: f32) -> Self {
        Self::new(name, ModifierCategory::Multiplicative, scope, value)
    }

    pub fn resist(name: impl Into<String>, scope: ModifierScope, value: f32) -> Self {
        Self::new(name, ModifierCategory::Resist, scope, value)
    }
}

/// Caps applied when combining modifiers.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StackingRules {
    /// Maximum total bonus from `Surge` modifiers.
    pub surge_cap: f32,
    /// Maximum total damage reduction from `Resist` modifiers.
    pub resist_cap: f32,
}

impl Default for StackingRules {
    fn default() -> Self {
        Self { surge_cap: 0.5, resist_cap: 0.8 }
    }
}

/// Combines every modifier that applies to this damage into one multiplier.
/// Each item is a modifier and the stack count of whatever provides it
/// (use 1 for plain buffs).
pub fn resolve<'a>(
    mods: impl IntoIterator<Item = (&'a Modifier, u32)>,
    kind: &SourceKind,
    damage_type: DamageType,
    rules: &StackingRules,
) -> f32 {
    let mut empowering: f32 = 0.0;
    let mut debuff: f32 = 0.0;
    let mut surge: f32 = 0.0;
    let mut product: f32 = 1.0;
    let mut kept: f32 = 1.0;

    for (m, stacks) in mods {
        if !m.scope.matches(kind, damage_type) {
            continue;
        }
        let v = if m.per_stack { m.value * stacks as f32 } else { m.value };
        match m.category {
            ModifierCategory::Empowering => empowering = empowering.max(v),
            ModifierCategory::Debuff => debuff = debuff.max(v),
            ModifierCategory::Surge => surge += v,
            ModifierCategory::Multiplicative => product *= 1.0 + v,
            ModifierCategory::Resist => kept *= 1.0 - v.clamp(0.0, 1.0),
        }
    }

    let surge = surge.min(rules.surge_cap);
    let kept = kept.max(1.0 - rules.resist_cap);
    (1.0 + empowering) * (1.0 + debuff) * (1.0 + surge) * product * kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weapon() -> SourceKind {
        SourceKind::Weapon { ammo: AmmoType::Primary, archetype: crate::weapon::WeaponArchetype::HandCannon }
    }

    fn total(mods: &[Modifier]) -> f32 {
        resolve(mods.iter().map(|m| (m, 1)), &weapon(), DamageType::Solar, &StackingRules::default())
    }

    #[test]
    fn empowering_buffs_do_not_stack() {
        let mods = [
            Modifier::empowering("radiant", ModifierScope::All, 0.25),
            Modifier::empowering("bubble", ModifierScope::All, 0.35),
        ];
        assert!((total(&mods) - 1.35).abs() < 1e-5);
    }

    #[test]
    fn categories_multiply_together() {
        let mods = [
            Modifier::empowering("radiant", ModifierScope::All, 0.25),
            Modifier::debuff("weaken", 0.15),
            Modifier::multiplicative("kill clip", ModifierScope::Weapons, 0.25),
        ];
        assert!((total(&mods) - 1.25 * 1.15 * 1.25).abs() < 1e-5);
    }

    #[test]
    fn surges_sum_up_to_cap() {
        let mods = [Modifier::surge("a", ModifierScope::All, 0.3), Modifier::surge("b", ModifierScope::All, 0.3)];
        assert!((total(&mods) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn resist_is_capped_and_scope_filtered() {
        let mods = [
            Modifier::resist("woven mail", ModifierScope::All, 0.6),
            Modifier::resist("frost armor", ModifierScope::All, 0.6),
            Modifier::empowering("melee only", ModifierScope::Melee, 1.0),
        ];
        assert!((total(&mods) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn per_stack_scales_by_stacks() {
        let m = Modifier::resist("frost armor", ModifierScope::All, 0.05).per_stack();
        let r = resolve([(&m, 4)], &weapon(), DamageType::Kinetic, &StackingRules::default());
        assert!((r - 0.8).abs() < 1e-5);
    }
}
