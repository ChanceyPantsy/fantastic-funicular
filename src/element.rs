//! Damage types (elements) and subclass elements.

/// The damage type carried by a weapon, ability or effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DamageType {
    Kinetic,
    Arc,
    Solar,
    Void,
    Stasis,
    Strand,
}

impl DamageType {
    pub const ALL: [DamageType; 6] = [
        DamageType::Kinetic,
        DamageType::Arc,
        DamageType::Solar,
        DamageType::Void,
        DamageType::Stasis,
        DamageType::Strand,
    ];

    /// Arc, Solar and Void are Light elements.
    pub fn is_light(self) -> bool {
        matches!(self, DamageType::Arc | DamageType::Solar | DamageType::Void)
    }

    /// Stasis and Strand are Darkness elements.
    pub fn is_darkness(self) -> bool {
        matches!(self, DamageType::Stasis | DamageType::Strand)
    }

    /// Every type except Kinetic is elemental and can pop a matching shield.
    pub fn is_elemental(self) -> bool {
        self != DamageType::Kinetic
    }
}

/// The element a subclass (and therefore its abilities) is built around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SubclassElement {
    Arc,
    Solar,
    Void,
    Stasis,
    Strand,
    /// Mixes Light and Darkness abilities; each ability carries its own type.
    Prismatic,
}

impl SubclassElement {
    /// The damage type abilities of this subclass deal by default.
    /// Prismatic has no single type, so `None`.
    pub fn damage_type(self) -> Option<DamageType> {
        match self {
            SubclassElement::Arc => Some(DamageType::Arc),
            SubclassElement::Solar => Some(DamageType::Solar),
            SubclassElement::Void => Some(DamageType::Void),
            SubclassElement::Stasis => Some(DamageType::Stasis),
            SubclassElement::Strand => Some(DamageType::Strand),
            SubclassElement::Prismatic => None,
        }
    }
}

/// The three guardian classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GuardianClass {
    Titan,
    Hunter,
    Warlock,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_darkness_are_disjoint() {
        for t in DamageType::ALL {
            assert!(!(t.is_light() && t.is_darkness()));
        }
        assert!(!DamageType::Kinetic.is_light());
        assert!(!DamageType::Kinetic.is_darkness());
        assert!(!DamageType::Kinetic.is_elemental());
    }
}
