//! Warlock supers, melees, rifts and aspects.

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
            "Stormtrance",
            Super,
            Arc,
            cd::SUPER,
            0.0,
            "Fire chain lightning from your hands and blink across the battlefield.",
            vec![
                Effect::SelfStatus(Amplified, 1),
                Effect::Roam(
                    RoamingSuper::new(
                        15.0,
                        SuperAttack::new("Chain Lightning", 0.15, 15.0, vec![Effect::chain(120.0, 4, 10.0, &[])]),
                    )
                    .heavy(SuperAttack::new(
                        "Blink",
                        1.5,
                        10.0,
                        vec![Effect::moves(MoveKind::Blink, 10.0)],
                    )),
                ),
            ],
        ),
        ability(
            "Chaos Reach",
            Super,
            Arc,
            cd::SUPER,
            0.0,
            "Channel a long, devastating beam of Arc Light.",
            vec![Effect::Roam(RoamingSuper::new(
                6.0,
                SuperAttack::new("Beam", 0.2, 30.0, vec![Effect::blast(450.0, 1.5)]),
            ))],
        ),
        ability(
            "Daybreak",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Fly on Solar wings and hurl explosive swords.",
            vec![Effect::Roam(RoamingSuper::new(
                15.0,
                SuperAttack::new("Sword of Light", 0.5, 30.0, vec![Effect::blast_with(350.0, 4.0, &[(Scorch, 20)])]),
            ))],
        ),
        ability(
            "Well of Radiance",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Plant a sword that heals allies and empowers their weapons.",
            vec![
                Effect::blast_with(400.0, 5.0, &[(Scorch, 30)]),
                Effect::Zone(
                    ZoneDef::new(7.0, 20.0, Anchor::Point)
                        .heal(15.0)
                        .ally_status(Restoration, 1)
                        .ally_modifier(Modifier::empowering("Well of Radiance", ModifierScope::All, 0.25)),
                ),
            ],
        ),
        ability(
            "Song of Flame",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Make yourself and nearby allies Radiant and cast scorching flames.",
            vec![
                Effect::AllyStatus { kind: Radiant, stacks: 1, radius: 15.0 },
                Effect::AllyStatus { kind: Restoration, stacks: 1, radius: 15.0 },
                Effect::Roam(RoamingSuper::new(
                    15.0,
                    SuperAttack::new("Flame", 0.5, 25.0, vec![Effect::blast_with(250.0, 3.0, &[(Scorch, 30)])]),
                )),
            ],
        ),
        ability(
            "Nova Bomb: Cataclysm",
            Super,
            Void,
            cd::SUPER,
            30.0,
            "Hurl a slow Void bomb that splits into seekers.",
            vec![Effect::blast(1500.0, 6.0), Effect::seekers(6, 200.0, 12.0, &[])],
        ),
        ability(
            "Nova Bomb: Vortex",
            Super,
            Void,
            cd::SUPER,
            30.0,
            "Hurl a Void bomb that leaves a damaging vortex.",
            vec![
                Effect::blast(1200.0, 6.0),
                Effect::Zone(ZoneDef::new(6.0, 4.0, Anchor::Point).pulse(0.4).damage(120.0)),
            ],
        ),
        ability(
            "Nova Warp",
            Super,
            Void,
            cd::SUPER,
            0.0,
            "Teleport around the battlefield and unleash Void bursts.",
            vec![Effect::Roam(
                RoamingSuper::new(12.0, SuperAttack::new("Warp Burst", 0.6, 0.0, vec![Effect::blast(260.0, 4.0)]))
                    .heavy(SuperAttack::new(
                        "Charged Burst",
                        2.0,
                        10.0,
                        vec![Effect::moves(MoveKind::Blink, 10.0), Effect::blast(600.0, 6.0)],
                    )),
            )],
        ),
        ability(
            "Winter's Wrath",
            Super,
            Stasis,
            cd::SUPER,
            0.0,
            "Fire freezing Stasis bolts and shatter enemies with a shockwave.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    12.0,
                    SuperAttack::new("Frost Bolt", 0.4, 25.0, vec![Effect::seekers(1, 150.0, 25.0, &[(Slow, 50)])]),
                )
                .heavy(SuperAttack::new(
                    "Shockwave",
                    2.0,
                    0.0,
                    vec![Effect::blast_with(400.0, 8.0, &[(Slow, 100)])],
                )),
            )],
        ),
        ability(
            "Needlestorm",
            Super,
            Strand,
            cd::SUPER,
            25.0,
            "Launch a volley of Strand needles that unravel enemies.",
            vec![Effect::seekers(6, 400.0, 20.0, &[(Unravel, 1)]), Effect::blast_with(200.0, 6.0, &[(Unravel, 1)])],
        ),
    ]
}

pub fn melees() -> Vec<AbilityDef> {
    vec![
        ability(
            "Chain Lightning",
            Melee,
            Arc,
            cd::MELEE,
            6.0,
            "A burst of lightning that chains to nearby enemies.",
            vec![Effect::chain(180.0, 3, 10.0, &[])],
        ),
        ability(
            "Ball Lightning",
            Melee,
            Arc,
            cd::MELEE,
            20.0,
            "Launch a ball of lightning that explodes.",
            vec![Effect::blast_with(250.0, 4.0, &[(Jolt, 1)])],
        ),
        ability(
            "Incinerator Snap",
            Melee,
            Solar,
            cd::MELEE,
            8.0,
            "Snap your fingers to create a burst of scorching explosions.",
            vec![Effect::blast_with(180.0, 5.0, &[(Scorch, 40)])],
        ),
        ability(
            "Celestial Fire",
            Melee,
            Solar,
            cd::MELEE,
            20.0,
            "Launch a spiral of three scorching fireballs.",
            vec![Effect::seekers(3, 80.0, 20.0, &[(Scorch, 30)])],
        ),
        ability(
            "Pocket Singularity",
            Melee,
            Void,
            cd::MELEE,
            20.0,
            "Launch a ball of Void that makes enemies volatile.",
            vec![Effect::blast_with(150.0, 4.0, &[(Volatile, 1)])],
        ),
        ability(
            "Penumbral Blast",
            Melee,
            Stasis,
            cd::MELEE,
            20.0,
            "Launch a ball of Stasis that slows.",
            vec![Effect::blast_with(120.0, 2.0, &[(Slow, 80)])],
        ),
        ability(
            "Arcane Needle",
            Melee,
            Strand,
            cd::MELEE,
            20.0,
            "Two charges. Fire seeking needles that unravel.",
            vec![Effect::seekers(3, 60.0, 20.0, &[(Unravel, 1)])],
        )
        .charges(2),
    ]
}

pub fn class_abilities() -> Vec<AbilityDef> {
    vec![
        ability(
            "Healing Rift",
            ClassAbility,
            Kinetic,
            cd::CLASS + 25.0,
            0.0,
            "Create a rift that heals allies inside.",
            vec![Effect::Zone(ZoneDef::new(4.0, 10.0, Anchor::Point).heal(12.0))],
        ),
        ability(
            "Empowering Rift",
            ClassAbility,
            Kinetic,
            cd::CLASS + 35.0,
            0.0,
            "Create a rift that boosts allies' weapon damage.",
            vec![Effect::Zone(ZoneDef::new(4.0, 10.0, Anchor::Point).ally_modifier(Modifier::empowering(
                "Empowering Rift",
                ModifierScope::Weapons,
                0.2,
            )))],
        ),
        ability(
            "Phoenix Dive",
            ClassAbility,
            Kinetic,
            cd::CLASS,
            0.0,
            "Dive down, curing yourself and allies nearby.",
            vec![Effect::moves(MoveKind::Hop, 4.0), Effect::HealAllies { amount: 80.0, radius: 6.0 }],
        ),
    ]
}

pub fn aspects() -> Vec<AspectDef> {
    vec![
        // Arc
        AspectDef::new("Arc Soul", Arc, "Your rift summons an Arc Soul that fights beside you.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::Zone(ZoneDef::new(12.0, 15.0, Anchor::Caster).pulse(0.6).damage(15.0))],
        ),
        AspectDef::new("Electrostatic Mind", Arc, "Ability kills make you Amplified and give melee energy.").on(
            On::Kill(KillFilter::AnyAbility),
            0.0,
            vec![Effect::SelfStatus(Amplified, 1), Effect::energy(Melee, 0.1)],
        ),
        AspectDef::new("Lightning Surge", Arc, "Your melee teleports you forward, releasing lightning.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::moves(MoveKind::Blink, 8.0), Effect::blast_with(220.0, 4.0, &[(Jolt, 1), (Blind, 1)])],
        ),
        // Solar
        AspectDef::new("Heat Rises", Solar, "Casting your grenade fuels your melee and super.").on(
            On::Cast(Grenade),
            0.0,
            vec![Effect::energy(Melee, 0.25), Effect::energy(Super, 0.03)],
        ),
        AspectDef::new("Touch of Flame", Solar, "Solar grenades hit harder.").modifier(Modifier::multiplicative(
            "Touch of Flame",
            ModifierScope::Grenade,
            0.3,
        )),
        AspectDef::new("Hellion", Solar, "Your class ability summons a turret that scorches enemies.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::Zone(ZoneDef::new(10.0, 8.0, Anchor::Caster).damage(20.0).enemy_status(Scorch, 5))],
        ),
        // Void
        AspectDef::new("Chaos Accelerant", Void, "Void grenades hit harder.").modifier(Modifier::multiplicative(
            "Chaos Accelerant",
            ModifierScope::Grenade,
            0.35,
        )),
        AspectDef::new(
            "Child of the Old Gods",
            Void,
            "Your rift summons a Void child that weakens enemies and heals you.",
        )
        .on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::seekers(1, 30.0, 15.0, &[(Weaken, 1)]), Effect::HealAllies { amount: 40.0, radius: 15.0 }],
        ),
        AspectDef::new("Feed the Void", Void, "Ability kills grant Devour.").on(
            On::Kill(KillFilter::AnyAbility),
            0.0,
            vec![Effect::SelfStatus(Devour, 1)],
        ),
        // Stasis
        AspectDef::new("Bleak Watcher", Stasis, "Your grenade also plants a turret that slows enemies.").on(
            On::Cast(Grenade),
            0.0,
            vec![Effect::Zone(ZoneDef::new(6.0, 10.0, Anchor::Point).pulse(1.0).enemy_status(Slow, 30))],
        ),
        AspectDef::new("Glacial Harvest", Stasis, "Killing frozen enemies gives you an overshield and melee energy.")
            .on(
                On::Kill(KillFilter::VictimHad(Frozen)),
                0.5,
                vec![Effect::Overshield(30.0), Effect::energy(Melee, 0.1)],
            ),
        AspectDef::new("Iceflare Bolts", Stasis, "Shattering releases seekers that freeze.").on(
            On::Causes(TriggerKind::Shatter),
            0.5,
            vec![Effect::seekers(2, 40.0, 12.0, &[(Slow, 60)])],
        ),
        // Strand
        AspectDef::new("Mindspun Invocation", Strand, "Your grenade also releases threadlings.").on(
            On::Cast(Grenade),
            0.0,
            vec![Effect::seekers(3, 60.0, 15.0, &[])],
        ),
        AspectDef::new("The Wanderer", Strand, "Your grenade also suspends enemies around the target.").on(
            On::Cast(Grenade),
            0.0,
            vec![Effect::blast_with(0.0, 5.0, &[(Suspend, 1)])],
        ),
        AspectDef::new("Weaver's Call", Strand, "Your rift releases threadlings.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::seekers(3, 60.0, 15.0, &[])],
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
            &["Stormtrance", "Chaos Reach"],
            grenades::arc(),
            &["Chain Lightning", "Ball Lightning"],
            &["Healing Rift", "Empowering Rift"],
            &["Arc Soul", "Electrostatic Mind", "Lightning Surge"],
        ),
        SubclassElement::Solar => (
            &["Daybreak", "Well of Radiance", "Song of Flame"],
            grenades::solar(),
            &["Incinerator Snap", "Celestial Fire"],
            &["Healing Rift", "Empowering Rift", "Phoenix Dive"],
            &["Heat Rises", "Touch of Flame", "Hellion"],
        ),
        SubclassElement::Void => (
            &["Nova Bomb: Cataclysm", "Nova Bomb: Vortex", "Nova Warp"],
            grenades::void(),
            &["Pocket Singularity"],
            &["Healing Rift", "Empowering Rift"],
            &["Chaos Accelerant", "Child of the Old Gods", "Feed the Void"],
        ),
        SubclassElement::Stasis => (
            &["Winter's Wrath"],
            grenades::stasis(),
            &["Penumbral Blast"],
            &["Healing Rift", "Empowering Rift"],
            &["Bleak Watcher", "Glacial Harvest", "Iceflare Bolts"],
        ),
        SubclassElement::Strand => (
            &["Needlestorm"],
            grenades::strand(),
            &["Arcane Needle"],
            &["Healing Rift", "Empowering Rift"],
            &["Mindspun Invocation", "The Wanderer", "Weaver's Call"],
        ),
        SubclassElement::Prismatic => (
            &["Nova Bomb: Cataclysm", "Song of Flame", "Winter's Wrath", "Needlestorm", "Stormtrance"],
            pick_abilities(
                &grenades::all(),
                &["Storm Grenade", "Healing Grenade", "Vortex Grenade", "Coldsnap Grenade", "Threadling Grenade"],
            ),
            &["Ball Lightning", "Incinerator Snap", "Pocket Singularity", "Penumbral Blast", "Arcane Needle"],
            &["Healing Rift", "Empowering Rift", "Phoenix Dive"],
            &["Feed the Void", "Lightning Surge", "Hellion"],
        ),
    };
    SubclassKit {
        class: GuardianClass::Warlock,
        element,
        supers: pick_abilities(&all_supers, s),
        grenades: g,
        melees: pick_abilities(&all_melees, m),
        class_abilities: pick_abilities(&all_classes, c),
        aspects: pick_aspects(&all_aspects, a),
    }
}
