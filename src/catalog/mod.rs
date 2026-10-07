//! Every class's supers, grenades, melees, class abilities and signature
//! aspects, for all six subclasses (Arc, Solar, Void, Stasis, Strand,
//! Prismatic).
//!
//! Each ability is plain data built from [`Effect`]s, so a host can change
//! any of it. Numbers are approximations scaled for a sandbox where a
//! guardian has ~200 health and red-bar enemies a few hundred.
//!
//! ```
//! use guardian_combat::catalog;
//! use guardian_combat::element::{GuardianClass, SubclassElement};
//!
//! let kit = catalog::kit(GuardianClass::Warlock, SubclassElement::Solar);
//! assert!(kit.supers.iter().any(|s| s.name == "Well of Radiance"));
//! ```

pub mod grenades;
pub mod hunter;
pub mod titan;
pub mod warlock;

use crate::ability::{AbilityDef, AbilitySlot};
use crate::effect::{AspectDef, Effect};
use crate::element::{DamageType, GuardianClass, SubclassElement};

/// Everything one class can equip on one subclass.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SubclassKit {
    pub class: GuardianClass,
    pub element: SubclassElement,
    pub supers: Vec<AbilityDef>,
    pub grenades: Vec<AbilityDef>,
    pub melees: Vec<AbilityDef>,
    pub class_abilities: Vec<AbilityDef>,
    pub aspects: Vec<AspectDef>,
}

impl SubclassKit {
    pub fn options(&self, slot: AbilitySlot) -> &[AbilityDef] {
        match slot {
            AbilitySlot::Grenade => &self.grenades,
            AbilitySlot::Melee => &self.melees,
            AbilitySlot::ClassAbility => &self.class_abilities,
            AbilitySlot::Super => &self.supers,
        }
    }

    /// Every ability in the kit.
    pub fn abilities(&self) -> impl Iterator<Item = &AbilityDef> {
        self.supers.iter().chain(&self.grenades).chain(&self.melees).chain(&self.class_abilities)
    }
}

pub const CLASSES: [GuardianClass; 3] = [GuardianClass::Titan, GuardianClass::Hunter, GuardianClass::Warlock];

pub const SUBCLASSES: [SubclassElement; 6] = [
    SubclassElement::Arc,
    SubclassElement::Solar,
    SubclassElement::Void,
    SubclassElement::Stasis,
    SubclassElement::Strand,
    SubclassElement::Prismatic,
];

pub fn kit(class: GuardianClass, element: SubclassElement) -> SubclassKit {
    match class {
        GuardianClass::Titan => titan::kit(element),
        GuardianClass::Hunter => hunter::kit(element),
        GuardianClass::Warlock => warlock::kit(element),
    }
}

/// All 18 class/subclass kits.
pub fn all_kits() -> Vec<SubclassKit> {
    CLASSES.iter().flat_map(|&c| SUBCLASSES.iter().map(move |&e| kit(c, e))).collect()
}

/// Looks up any ability in the catalog by name.
pub fn ability_named(name: &str) -> Option<AbilityDef> {
    all_kits().into_iter().flat_map(|k| k.abilities().cloned().collect::<Vec<_>>()).find(|a| a.name == name)
}

/// Looks up any aspect in the catalog by name.
pub fn aspect_named(name: &str) -> Option<AspectDef> {
    all_kits().into_iter().flat_map(|k| k.aspects).find(|a| a.name == name)
}

/// Finds an ability or aspect in `list` by name. Panics if missing, since
/// that is a bug in the catalog itself.
pub(crate) fn pick<T: Clone>(list: &[T], names: &[&str], name_of: impl Fn(&T) -> &str) -> Vec<T> {
    names
        .iter()
        .map(|n| {
            list.iter().find(|x| name_of(x) == *n).unwrap_or_else(|| panic!("catalog has no entry named {n:?}")).clone()
        })
        .collect()
}

/// A class's choices for one subclass: super, melee, class ability and
/// aspect names, plus the grenade list.
pub(crate) type KitChoice = (
    &'static [&'static str],
    Vec<AbilityDef>,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

pub(crate) fn pick_abilities(list: &[AbilityDef], names: &[&str]) -> Vec<AbilityDef> {
    pick(list, names, |a| &a.name)
}

pub(crate) fn pick_aspects(list: &[AspectDef], names: &[&str]) -> Vec<AspectDef> {
    pick(list, names, |a| &a.name)
}

pub(crate) fn ability(
    name: &str,
    slot: AbilitySlot,
    damage_type: DamageType,
    cooldown: f32,
    range: f32,
    description: &str,
    effects: Vec<Effect>,
) -> AbilityDef {
    let mut a = AbilityDef::new(name, slot, damage_type, cooldown).describe(description).range(range);
    a.effects = effects;
    a
}

/// Standard cooldowns (seconds at a regen multiplier of 1.0).
pub(crate) mod cd {
    pub const SUPER: f32 = 450.0;
    pub const GRENADE: f32 = 105.0;
    pub const MELEE: f32 = 75.0;
    pub const CLASS: f32 = 45.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_kit_has_every_slot() {
        for k in all_kits() {
            for slot in AbilitySlot::ALL {
                assert!(!k.options(slot).is_empty(), "{:?} {:?} has no {slot:?}", k.class, k.element);
                for a in k.options(slot) {
                    assert_eq!(a.slot, slot, "{} is in the wrong list", a.name);
                    assert!(!a.description.is_empty(), "{} has no description", a.name);
                    assert!(!a.effects.is_empty(), "{} does nothing", a.name);
                }
            }
            assert!(k.aspects.len() >= 2, "{:?} {:?} needs aspects", k.class, k.element);
        }
    }

    #[test]
    fn non_prismatic_abilities_match_their_element() {
        for k in all_kits() {
            let Some(element) = k.element.damage_type() else { continue };
            for a in k.abilities().filter(|a| a.slot != AbilitySlot::ClassAbility) {
                assert_eq!(a.damage_type, element, "{} in {:?} {:?}", a.name, k.class, k.element);
            }
        }
    }

    #[test]
    fn names_are_unique_within_a_kit() {
        for k in all_kits() {
            let mut seen = HashSet::new();
            for a in k.abilities() {
                assert!(seen.insert(&a.name), "duplicate {} in {:?} {:?}", a.name, k.class, k.element);
            }
        }
    }

    #[test]
    fn catalog_size() {
        let mut abilities = HashSet::new();
        let mut aspects = HashSet::new();
        for k in all_kits() {
            abilities.extend(k.abilities().map(|a| (a.slot, a.name.clone())));
            aspects.extend(k.aspects.iter().map(|a| a.name.clone()));
        }
        assert!(abilities.len() >= 90, "{} unique abilities", abilities.len());
        assert!(aspects.len() >= 40, "{} unique aspects", aspects.len());
    }
}
