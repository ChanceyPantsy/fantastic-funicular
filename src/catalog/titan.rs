//! Titan supers, melees, barricades and aspects.

use super::{ability, cd, grenades, pick_abilities, pick_aspects, KitChoice, SubclassKit};
use crate::ability::{AbilityDef, AbilitySlot::*};
use crate::buffs::{Modifier, ModifierScope};
use crate::effect::{
    Anchor, AspectDef, Effect, KillFilter, MoveKind, PassiveTrigger as On, RoamingSuper, SuperAttack, ZoneDef,
};
use crate::element::{DamageType::*, GuardianClass, SubclassElement};
use crate::status::{StatusKind::*, TriggerKind};

pub fn supers() -> Vec<AbilityDef> {
    vec![
        ability(
            "Fists of Havoc",
            Super,
            Arc,
            cd::SUPER,
            0.0,
            "Charge your fists with Arc energy and slam into enemies.",
            vec![
                Effect::SelfStatus(Amplified, 1),
                Effect::Roam(
                    RoamingSuper::new(
                        15.0,
                        SuperAttack::new(
                            "Havoc Slam",
                            1.0,
                            8.0,
                            vec![Effect::moves(MoveKind::Lunge, 8.0), Effect::blast(320.0, 5.0)],
                        ),
                    )
                    .heavy(SuperAttack::new(
                        "Ground Slam",
                        3.0,
                        0.0,
                        vec![Effect::blast_with(600.0, 7.0, &[(Jolt, 1)])],
                    )),
                ),
            ],
        ),
        ability(
            "Thundercrash",
            Super,
            Arc,
            cd::SUPER,
            30.0,
            "Launch into the air and crash down for massive damage.",
            vec![Effect::moves(MoveKind::Slam, 30.0), Effect::blast_with(2500.0, 7.0, &[(Jolt, 1)])],
        ),
        ability(
            "Hammer of Sol",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Summon a flaming hammer and throw it at enemies.",
            vec![Effect::Roam(RoamingSuper::new(
                15.0,
                SuperAttack::new("Throw Hammer", 0.6, 30.0, vec![Effect::blast_with(220.0, 3.0, &[(Scorch, 30)])]),
            ))],
        ),
        ability(
            "Burning Maul",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Wield a massive hammer that creates fiery tornadoes.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    16.0,
                    SuperAttack::new("Maul Swing", 0.8, 6.0, vec![Effect::blast_with(260.0, 4.0, &[(Scorch, 20)])]),
                )
                .heavy(SuperAttack::new(
                    "Fire Tornado",
                    3.0,
                    15.0,
                    vec![Effect::Zone(
                        ZoneDef::new(4.0, 3.0, Anchor::Point).pulse(0.3).damage(60.0).enemy_status(Scorch, 10),
                    )],
                )),
            )],
        ),
        ability(
            "Sentinel Shield",
            Super,
            Void,
            cd::SUPER,
            0.0,
            "Wield a shield of Void Light: bash, block and throw it.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    16.0,
                    SuperAttack::new(
                        "Shield Bash",
                        0.6,
                        5.0,
                        vec![Effect::moves(MoveKind::Lunge, 5.0), Effect::blast(280.0, 2.5)],
                    ),
                )
                .heavy(SuperAttack::new("Shield Throw", 2.0, 25.0, vec![Effect::chain(300.0, 3, 10.0, &[(Weaken, 1)])]))
                .resist(0.6),
            )],
        ),
        ability(
            "Ward of Dawn",
            Super,
            Void,
            cd::SUPER,
            0.0,
            "Create a protective bubble. Allies inside get an overshield and Weapons of Light.",
            vec![Effect::Zone(
                ZoneDef::new(7.0, 20.0, Anchor::Point).cover().overshield(15.0).ally_modifier(Modifier::empowering(
                    "Weapons of Light",
                    ModifierScope::Weapons,
                    0.35,
                )),
            )],
        ),
        ability(
            "Twilight Arsenal",
            Super,
            Void,
            cd::SUPER,
            30.0,
            "Throw three Void axes that weaken everything they hit.",
            vec![Effect::seekers(3, 900.0, 25.0, &[(Weaken, 1)]), Effect::blast_with(200.0, 5.0, &[(Weaken, 1)])],
        ),
        ability(
            "Glacial Quake",
            Super,
            Stasis,
            cd::SUPER,
            0.0,
            "Wield a Stasis gauntlet and slam the ground to freeze enemies.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    15.0,
                    SuperAttack::new(
                        "Quake",
                        0.8,
                        8.0,
                        vec![Effect::moves(MoveKind::Lunge, 8.0), Effect::blast_with(300.0, 4.0, &[(Slow, 50)])],
                    ),
                )
                .heavy(SuperAttack::new(
                    "Shatter Slam",
                    3.0,
                    0.0,
                    vec![Effect::blast_with(500.0, 7.0, &[(Slow, 100)])],
                )),
            )],
        ),
        ability(
            "Bladefury",
            Super,
            Strand,
            cd::SUPER,
            0.0,
            "Wield Strand blades that sever, and hurl blades that suspend.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    14.0,
                    SuperAttack::new(
                        "Slash",
                        0.5,
                        6.0,
                        vec![Effect::moves(MoveKind::Lunge, 6.0), Effect::blast_with(250.0, 3.0, &[(Sever, 1)])],
                    ),
                )
                .heavy(SuperAttack::new(
                    "Blade Throw",
                    2.0,
                    20.0,
                    vec![Effect::seekers(2, 300.0, 20.0, &[(Suspend, 1)])],
                )),
            )],
        ),
    ]
}

pub fn melees() -> Vec<AbilityDef> {
    vec![
        ability(
            "Seismic Strike",
            Melee,
            Arc,
            cd::MELEE,
            8.0,
            "Shoulder charge that blinds enemies around the impact.",
            vec![Effect::moves(MoveKind::Lunge, 8.0), Effect::blast_with(200.0, 4.0, &[(Blind, 1)])],
        ),
        ability(
            "Ballistic Slam",
            Melee,
            Arc,
            cd::MELEE,
            10.0,
            "Slam down from the air, damaging enemies around you.",
            vec![Effect::moves(MoveKind::Slam, 10.0), Effect::blast(250.0, 5.0)],
        ),
        ability(
            "Thunderclap",
            Melee,
            Arc,
            cd::MELEE,
            4.0,
            "Charge a devastating punch that erupts in a shockwave.",
            vec![Effect::blast_with(450.0, 5.0, &[(Jolt, 1)])],
        ),
        ability(
            "Hammer Strike",
            Melee,
            Solar,
            cd::MELEE,
            4.0,
            "A hammer blow that scorches and leaves you with Restoration.",
            vec![Effect::blast_with(250.0, 0.0, &[(Scorch, 40)]), Effect::SelfStatus(Restoration, 1)],
        ),
        ability(
            "Throwing Hammer",
            Melee,
            Solar,
            cd::MELEE,
            20.0,
            "Throw a hammer. Picking it up heals you.",
            vec![Effect::blast_with(200.0, 0.0, &[(Scorch, 20)]), Effect::Heal(50.0)],
        ),
        ability(
            "Shield Bash",
            Melee,
            Void,
            cd::MELEE,
            6.0,
            "Shoulder charge that suppresses.",
            vec![Effect::moves(MoveKind::Lunge, 6.0), Effect::blast_with(220.0, 2.0, &[(Suppress, 1)])],
        ),
        ability(
            "Shield Throw",
            Melee,
            Void,
            cd::MELEE,
            20.0,
            "Throw a ricocheting shield that weakens. Grants an overshield.",
            vec![Effect::chain(180.0, 2, 10.0, &[(Weaken, 1)]), Effect::Overshield(40.0)],
        ),
        ability(
            "Shiver Strike",
            Melee,
            Stasis,
            cd::MELEE,
            8.0,
            "Lunge into a Stasis strike that slows.",
            vec![Effect::moves(MoveKind::Lunge, 8.0), Effect::blast_with(200.0, 3.0, &[(Slow, 40)])],
        ),
        ability(
            "Frenzied Blade",
            Melee,
            Strand,
            cd::MELEE,
            6.0,
            "Three charges. Lunge with a Strand blade that severs.",
            vec![Effect::moves(MoveKind::Lunge, 6.0), Effect::blast_with(160.0, 2.0, &[(Sever, 1)])],
        )
        .charges(3),
    ]
}

pub fn class_abilities() -> Vec<AbilityDef> {
    vec![
        ability(
            "Towering Barricade",
            ClassAbility,
            Kinetic,
            cd::CLASS + 15.0,
            4.0,
            "A large barrier that blocks incoming fire.",
            vec![Effect::Zone(ZoneDef::new(3.5, 10.0, Anchor::Point).cover().ally_modifier(Modifier::resist(
                "Barricade Cover",
                ModifierScope::All,
                0.5,
            )))],
        ),
        ability(
            "Rally Barricade",
            ClassAbility,
            Kinetic,
            cd::CLASS,
            3.0,
            "A small barrier that reloads your weapon and speeds up regeneration.",
            vec![Effect::Reload, Effect::Zone(ZoneDef::new(3.0, 6.0, Anchor::Point).cover().heal(5.0))],
        ),
        ability(
            "Thruster",
            ClassAbility,
            Kinetic,
            cd::CLASS - 15.0,
            0.0,
            "A quick evasive burst.",
            vec![Effect::moves(MoveKind::Dash, 8.0)],
        ),
    ]
}

pub fn aspects() -> Vec<AspectDef> {
    vec![
        // Arc
        AspectDef::new("Juggernaut", Arc, "Using your class ability grants a frontal shield.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::Overshield(60.0)],
        ),
        AspectDef::new("Knockout", Arc, "Precision kills heal you and boost melee damage.").on(
            On::Kill(KillFilter::Precision),
            5.0,
            vec![
                Effect::Heal(40.0),
                Effect::Buff(Modifier::multiplicative("Knockout", ModifierScope::Melee, 0.5).timed(5.0)),
            ],
        ),
        AspectDef::new("Touch of Thunder", Arc, "Arc grenades hit harder and make you Amplified.")
            .modifier(Modifier::multiplicative("Touch of Thunder", ModifierScope::Grenade, 0.2))
            .on(On::Cast(Grenade), 0.0, vec![Effect::SelfStatus(Amplified, 1)]),
        // Solar
        AspectDef::new("Roaring Flames", Solar, "Ability kills increase ability damage.").on(
            On::Kill(KillFilter::AnyAbility),
            0.0,
            vec![Effect::Buff(Modifier::multiplicative("Roaring Flames", ModifierScope::Abilities, 0.3).timed(10.0))],
        ),
        AspectDef::new("Sol Invictus", Solar, "Ability kills create Sunspots that burn enemies and heal you.").on(
            On::Kill(KillFilter::AnyAbility),
            2.0,
            vec![Effect::Zone(ZoneDef::new(3.0, 6.0, Anchor::Point).damage(30.0).enemy_status(Scorch, 10).heal(5.0))],
        ),
        AspectDef::new("Consecration", Solar, "Your melee also releases a wave of Solar flame.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::blast_with(250.0, 6.0, &[(Scorch, 40)])],
        ),
        // Void
        AspectDef::new("Bastion", Void, "Using your class ability grants you an overshield.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::Overshield(60.0)],
        ),
        AspectDef::new("Controlled Demolition", Void, "Volatile explosions you cause heal you.").on(
            On::Causes(TriggerKind::VolatileExplosion),
            1.0,
            vec![Effect::Heal(40.0)],
        ),
        AspectDef::new("Offensive Bulwark", Void, "Melee damage is increased; ability kills grant an overshield.")
            .modifier(Modifier::multiplicative("Offensive Bulwark", ModifierScope::Melee, 0.5))
            .on(On::Kill(KillFilter::AnyAbility), 3.0, vec![Effect::Overshield(30.0)]),
        // Stasis
        AspectDef::new("Diamond Lance", Stasis, "Killing frozen enemies creates a lance that freezes others.").on(
            On::Kill(KillFilter::VictimHad(Frozen)),
            6.0,
            vec![Effect::blast_with(0.0, 5.0, &[(Slow, 100)])],
        ),
        AspectDef::new("Howl of the Storm", Stasis, "Your melee sends out a wave of crystals that freezes.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::blast_with(80.0, 6.0, &[(Slow, 100)])],
        ),
        AspectDef::new("Tectonic Harvest", Stasis, "Shattering crystals gives melee energy and an overshield.").on(
            On::Causes(TriggerKind::Shatter),
            0.5,
            vec![Effect::energy(Melee, 0.15), Effect::Overshield(20.0)],
        ),
        // Strand
        AspectDef::new("Drengr's Lash", Strand, "Your class ability sends out threads that suspend enemies.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::blast_with(50.0, 8.0, &[(Suspend, 1)])],
        ),
        AspectDef::new("Banner of War", Strand, "Melee kills heal nearby allies and boost melee damage.").on(
            On::Kill(KillFilter::Ability(Melee)),
            0.0,
            vec![
                Effect::HealAllies { amount: 50.0, radius: 10.0 },
                Effect::Buff(Modifier::multiplicative("Banner of War", ModifierScope::Melee, 0.3).timed(10.0)),
            ],
        ),
        AspectDef::new("Flechette Storm", Strand, "Your melee also hurls severing flechettes.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::seekers(5, 60.0, 8.0, &[(Sever, 1)])],
        ),
    ]
}

pub fn kit(element: SubclassElement) -> SubclassKit {
    let all_supers = supers();
    let all_melees = melees();
    let all_classes = class_abilities();
    let all_aspects = aspects();
    let (s, g, m, c, a): KitChoice = match element {
        SubclassElement::Arc => (
            &["Fists of Havoc", "Thundercrash"],
            grenades::arc(),
            &["Seismic Strike", "Ballistic Slam", "Thunderclap"],
            &["Towering Barricade", "Rally Barricade", "Thruster"],
            &["Juggernaut", "Knockout", "Touch of Thunder"],
        ),
        SubclassElement::Solar => (
            &["Hammer of Sol", "Burning Maul"],
            grenades::solar(),
            &["Hammer Strike", "Throwing Hammer"],
            &["Towering Barricade", "Rally Barricade"],
            &["Roaring Flames", "Sol Invictus", "Consecration"],
        ),
        SubclassElement::Void => (
            &["Sentinel Shield", "Ward of Dawn", "Twilight Arsenal"],
            grenades::void(),
            &["Shield Bash", "Shield Throw"],
            &["Towering Barricade", "Rally Barricade"],
            &["Bastion", "Controlled Demolition", "Offensive Bulwark"],
        ),
        SubclassElement::Stasis => (
            &["Glacial Quake"],
            grenades::stasis(),
            &["Shiver Strike"],
            &["Towering Barricade", "Rally Barricade"],
            &["Diamond Lance", "Howl of the Storm", "Tectonic Harvest"],
        ),
        SubclassElement::Strand => (
            &["Bladefury"],
            grenades::strand(),
            &["Frenzied Blade"],
            &["Towering Barricade", "Rally Barricade"],
            &["Drengr's Lash", "Banner of War", "Flechette Storm"],
        ),
        SubclassElement::Prismatic => (
            &["Thundercrash", "Hammer of Sol", "Twilight Arsenal", "Glacial Quake", "Bladefury"],
            pick_abilities(
                &grenades::all(),
                &["Pulse Grenade", "Thermite Grenade", "Suppressor Grenade", "Glacier Grenade", "Shackle Grenade"],
            ),
            &["Thunderclap", "Hammer Strike", "Shield Throw", "Frenzied Blade"],
            &["Towering Barricade", "Rally Barricade", "Thruster"],
            &["Knockout", "Consecration", "Drengr's Lash"],
        ),
    };
    SubclassKit {
        class: GuardianClass::Titan,
        element,
        supers: pick_abilities(&all_supers, s),
        grenades: g,
        melees: pick_abilities(&all_melees, m),
        class_abilities: pick_abilities(&all_classes, c),
        aspects: pick_aspects(&all_aspects, a),
    }
}
