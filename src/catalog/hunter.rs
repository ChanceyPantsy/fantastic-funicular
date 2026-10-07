//! Hunter supers, melees, dodges and aspects.

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
            "Arc Staff",
            Super,
            Arc,
            cd::SUPER,
            0.0,
            "Wield a staff of Arc energy and strike with blinding speed.",
            vec![
                Effect::SelfStatus(Amplified, 1),
                Effect::Roam(
                    RoamingSuper::new(
                        15.0,
                        SuperAttack::new(
                            "Staff Strike",
                            0.35,
                            5.0,
                            vec![Effect::moves(MoveKind::Lunge, 5.0), Effect::blast(220.0, 2.5)],
                        ),
                    )
                    .speed(1.2),
                ),
            ],
        ),
        ability(
            "Gathering Storm",
            Super,
            Arc,
            cd::SUPER,
            25.0,
            "Throw a staff that calls down lightning and keeps striking around it.",
            vec![
                Effect::blast_with(2200.0, 6.0, &[(Jolt, 1)]),
                Effect::Zone(ZoneDef::new(6.0, 4.0, Anchor::Point).damage(80.0).enemy_status(Jolt, 1)),
            ],
        ),
        ability(
            "Golden Gun: Deadshot",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Six fast, scorching shots.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    10.0,
                    SuperAttack::new("Deadshot", 0.3, 40.0, vec![Effect::blast_with(400.0, 0.0, &[(Scorch, 50)])]),
                )
                .uses(6),
            )],
        ),
        ability(
            "Golden Gun: Marksman",
            Super,
            Solar,
            cd::SUPER,
            0.0,
            "Three devastating shots.",
            vec![Effect::Roam(
                RoamingSuper::new(6.0, SuperAttack::new("Marksman", 0.6, 50.0, vec![Effect::blast(1200.0, 0.0)]))
                    .uses(3),
            )],
        ),
        ability(
            "Blade Barrage",
            Super,
            Solar,
            cd::SUPER,
            25.0,
            "Launch a volley of explosive knives.",
            vec![Effect::seekers(10, 260.0, 18.0, &[(Scorch, 20)])],
        ),
        ability(
            "Shadowshot: Deadfall",
            Super,
            Void,
            cd::SUPER,
            30.0,
            "Fire a Void arrow that tethers enemies: weakened, suppressed and stuck.",
            vec![
                Effect::blast(400.0, 0.0),
                Effect::Zone(
                    ZoneDef::new(6.0, 10.0, Anchor::Point)
                        .damage(10.0)
                        .enemy_status(Weaken, 1)
                        .enemy_status(Suppress, 1),
                ),
            ],
        ),
        ability(
            "Shadowshot: Moebius Quiver",
            Super,
            Void,
            cd::SUPER,
            0.0,
            "Fire three rapid Void arrows that weaken.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    6.0,
                    SuperAttack::new("Quiver Shot", 0.6, 40.0, vec![Effect::blast_with(600.0, 5.0, &[(Weaken, 1)])]),
                )
                .uses(3),
            )],
        ),
        ability(
            "Spectral Blades",
            Super,
            Void,
            cd::SUPER,
            0.0,
            "Turn invisible and strike with Void blades.",
            vec![
                Effect::SelfStatus(Invisible, 1),
                Effect::Roam(
                    RoamingSuper::new(
                        15.0,
                        SuperAttack::new(
                            "Blade Strike",
                            0.4,
                            5.0,
                            vec![Effect::moves(MoveKind::Lunge, 5.0), Effect::blast(250.0, 2.5)],
                        ),
                    )
                    .heavy(SuperAttack::new(
                        "Blade Wave",
                        2.0,
                        15.0,
                        vec![Effect::seekers(3, 200.0, 15.0, &[])],
                    )),
                ),
            ],
        ),
        ability(
            "Silence and Squall",
            Super,
            Stasis,
            cd::SUPER,
            25.0,
            "Throw a kama that freezes, and one that becomes a slowing storm.",
            vec![
                Effect::seekers(1, 600.0, 25.0, &[(Slow, 100)]),
                Effect::Zone(ZoneDef::new(6.0, 8.0, Anchor::Point).damage(40.0).enemy_status(Slow, 15)),
            ],
        ),
        ability(
            "Silkstrike",
            Super,
            Strand,
            cd::SUPER,
            0.0,
            "Wield a Strand rope dart that severs, and grapple in to suspend.",
            vec![Effect::Roam(
                RoamingSuper::new(
                    15.0,
                    SuperAttack::new(
                        "Rope Dart",
                        0.45,
                        6.0,
                        vec![Effect::moves(MoveKind::Lunge, 6.0), Effect::blast_with(260.0, 3.0, &[(Sever, 1)])],
                    ),
                )
                .heavy(SuperAttack::new(
                    "Grapple Strike",
                    2.0,
                    20.0,
                    vec![Effect::moves(MoveKind::Grapple, 20.0), Effect::blast_with(300.0, 4.0, &[(Suspend, 1)])],
                )),
            )],
        ),
    ]
}

pub fn melees() -> Vec<AbilityDef> {
    vec![
        ability(
            "Combination Blow",
            Melee,
            Arc,
            cd::MELEE,
            4.0,
            "A powerful strike that heals you.",
            vec![Effect::moves(MoveKind::Lunge, 4.0), Effect::blast(250.0, 0.0), Effect::Heal(60.0)],
        ),
        ability(
            "Disorienting Blow",
            Melee,
            Arc,
            cd::MELEE,
            4.0,
            "A strike that blinds and jolts enemies around the target.",
            vec![Effect::blast_with(180.0, 4.0, &[(Blind, 1), (Jolt, 1)])],
        ),
        ability(
            "Knife Trick",
            Melee,
            Solar,
            cd::MELEE,
            15.0,
            "Throw a fan of knives that scorch.",
            vec![Effect::seekers(3, 80.0, 15.0, &[(Scorch, 30)])],
        ),
        ability(
            "Lightweight Knife",
            Melee,
            Solar,
            cd::MELEE,
            20.0,
            "Two charges. A quick thrown knife.",
            vec![Effect::blast(150.0, 0.0)],
        )
        .charges(2),
        ability(
            "Weighted Throwing Knife",
            Melee,
            Solar,
            cd::MELEE,
            20.0,
            "A heavy knife that scorches.",
            vec![Effect::blast_with(250.0, 0.0, &[(Scorch, 40)])],
        ),
        ability(
            "Proximity Explosive Knife",
            Melee,
            Solar,
            cd::MELEE,
            15.0,
            "A knife that explodes when enemies come near.",
            vec![Effect::blast_with(200.0, 4.0, &[(Scorch, 30)])],
        ),
        ability(
            "Snare Bomb",
            Melee,
            Void,
            cd::MELEE,
            20.0,
            "Throw a smoke bomb that weakens enemies.",
            vec![Effect::blast_with(120.0, 4.0, &[(Weaken, 1)])],
        ),
        ability(
            "Withering Blade",
            Melee,
            Stasis,
            cd::MELEE,
            20.0,
            "Two charges. A seeking Stasis blade that slows.",
            vec![Effect::seekers(1, 120.0, 20.0, &[(Slow, 50)])],
        )
        .charges(2),
        ability(
            "Threaded Spike",
            Melee,
            Strand,
            cd::MELEE,
            20.0,
            "Throw a spike that severs and returns.",
            vec![Effect::blast_with(200.0, 0.0, &[(Sever, 1)]), Effect::energy(Melee, 0.3)],
        ),
    ]
}

pub fn class_abilities() -> Vec<AbilityDef> {
    vec![
        ability(
            "Marksman's Dodge",
            ClassAbility,
            Kinetic,
            cd::CLASS - 10.0,
            0.0,
            "Dodge and reload your weapon.",
            vec![Effect::moves(MoveKind::Dash, 6.0), Effect::Reload],
        ),
        ability(
            "Gambler's Dodge",
            ClassAbility,
            Kinetic,
            cd::CLASS,
            0.0,
            "Dodge and recharge your melee.",
            vec![Effect::moves(MoveKind::Dash, 6.0), Effect::energy(Melee, 1.0)],
        ),
        ability(
            "Acrobat's Dodge",
            ClassAbility,
            Kinetic,
            cd::CLASS + 15.0,
            0.0,
            "Dodge and become Amplified.",
            vec![Effect::moves(MoveKind::Dash, 6.0), Effect::SelfStatus(Amplified, 1)],
        ),
    ]
}

pub fn aspects() -> Vec<AspectDef> {
    vec![
        // Arc
        AspectDef::new("Flow State", Arc, "Dodging reloads your weapon and makes you Amplified.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::Reload, Effect::SelfStatus(Amplified, 1)],
        ),
        AspectDef::new("Lethal Current", Arc, "Your melee leaves a lightning aftershock that jolts.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::blast_with(80.0, 4.0, &[(Jolt, 1)])],
        ),
        AspectDef::new("Tempest Strike", Arc, "Your melee sends a wave of lightning forward.").on(
            On::Cast(Melee),
            0.0,
            vec![Effect::seekers(3, 120.0, 10.0, &[(Jolt, 1)])],
        ),
        // Solar
        AspectDef::new("Gunpowder Gamble", Solar, "Kills charge an explosive that blows up where your victim fell.")
            .on(On::Kill(KillFilter::Any), 8.0, vec![Effect::blast_with(300.0, 5.0, &[(Scorch, 50)])]),
        AspectDef::new("Knock 'Em Down", Solar, "Melee damage is increased; super kills refund super energy.")
            .modifier(Modifier::multiplicative("Knock 'Em Down", ModifierScope::Melee, 0.5))
            .on(On::Kill(KillFilter::Super), 0.0, vec![Effect::energy(Super, 0.03)]),
        AspectDef::new("On Your Mark", Solar, "Dodging reloads and boosts weapon damage briefly.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![
                Effect::Reload,
                Effect::Buff(Modifier::multiplicative("On Your Mark", ModifierScope::Weapons, 0.15).timed(6.0)),
            ],
        ),
        // Void
        AspectDef::new("Stylish Executioner", Void, "Killing weakened enemies turns you invisible and heals you.").on(
            On::Kill(KillFilter::VictimHad(Weaken)),
            2.0,
            vec![Effect::SelfStatus(Invisible, 1), Effect::Heal(60.0)],
        ),
        AspectDef::new("Trapper's Ambush", Void, "Dodging releases a smoke burst that weakens enemies around you.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::blast_with(60.0, 6.0, &[(Weaken, 1)])],
        ),
        AspectDef::new("Vanishing Step", Void, "Dodging turns you invisible.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::SelfStatus(Invisible, 1)],
        ),
        // Stasis
        AspectDef::new("Grim Harvest", Stasis, "Shattering gives grenade and melee energy.").on(
            On::Causes(TriggerKind::Shatter),
            0.5,
            vec![Effect::energy(Grenade, 0.1), Effect::energy(Melee, 0.1)],
        ),
        AspectDef::new("Touch of Winter", Stasis, "Stasis grenades hit harder.").modifier(Modifier::multiplicative(
            "Touch of Winter",
            ModifierScope::Grenade,
            0.25,
        )),
        AspectDef::new("Winter's Shroud", Stasis, "Dodging slows enemies around you.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::blast_with(0.0, 6.0, &[(Slow, 40)])],
        ),
        // Strand
        AspectDef::new("Ensnaring Slam", Strand, "Your melee also suspends enemies around the target.").on(
            On::Cast(Melee),
            15.0,
            vec![Effect::blast_with(0.0, 6.0, &[(Suspend, 1)])],
        ),
        AspectDef::new("Whirling Maelstrom", Strand, "Your grenade also leaves a severing Strand vortex.").on(
            On::Cast(Grenade),
            0.0,
            vec![Effect::Zone(ZoneDef::new(3.0, 4.0, Anchor::Point).pulse(0.4).damage(30.0).enemy_status(Sever, 1))],
        ),
        AspectDef::new("Widow's Silk", Strand, "Kills recharge your grapple.").on(
            On::Kill(KillFilter::Any),
            1.0,
            vec![Effect::energy(Grenade, 0.1)],
        ),
        // Prismatic
        AspectDef::new("Ascension", Arc, "Dodging jolts and suspends enemies around you.").on(
            On::Cast(ClassAbility),
            0.0,
            vec![Effect::blast_with(100.0, 5.0, &[(Jolt, 1), (Suspend, 1)])],
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
            &["Arc Staff", "Gathering Storm"],
            grenades::arc(),
            &["Combination Blow", "Disorienting Blow"],
            &["Marksman's Dodge", "Gambler's Dodge", "Acrobat's Dodge"],
            &["Flow State", "Lethal Current", "Tempest Strike"],
        ),
        SubclassElement::Solar => (
            &["Golden Gun: Deadshot", "Golden Gun: Marksman", "Blade Barrage"],
            grenades::solar(),
            &["Knife Trick", "Lightweight Knife", "Weighted Throwing Knife", "Proximity Explosive Knife"],
            &["Marksman's Dodge", "Gambler's Dodge"],
            &["Gunpowder Gamble", "Knock 'Em Down", "On Your Mark"],
        ),
        SubclassElement::Void => (
            &["Shadowshot: Deadfall", "Shadowshot: Moebius Quiver", "Spectral Blades"],
            grenades::void(),
            &["Snare Bomb"],
            &["Marksman's Dodge", "Gambler's Dodge"],
            &["Stylish Executioner", "Trapper's Ambush", "Vanishing Step"],
        ),
        SubclassElement::Stasis => (
            &["Silence and Squall"],
            grenades::stasis(),
            &["Withering Blade"],
            &["Marksman's Dodge", "Gambler's Dodge"],
            &["Grim Harvest", "Touch of Winter", "Winter's Shroud"],
        ),
        SubclassElement::Strand => (
            &["Silkstrike"],
            grenades::strand(),
            &["Threaded Spike"],
            &["Marksman's Dodge", "Gambler's Dodge", "Acrobat's Dodge"],
            &["Ensnaring Slam", "Whirling Maelstrom", "Widow's Silk"],
        ),
        SubclassElement::Prismatic => (
            &["Golden Gun: Marksman", "Gathering Storm", "Silence and Squall", "Silkstrike"],
            pick_abilities(
                &grenades::all(),
                &["Arcbolt Grenade", "Swarm Grenade", "Magnetic Grenade", "Duskfield Grenade", "Grapple"],
            ),
            &["Combination Blow", "Knife Trick", "Withering Blade", "Threaded Spike"],
            &["Marksman's Dodge", "Gambler's Dodge"],
            &["Stylish Executioner", "Winter's Shroud", "Ascension"],
        ),
    };
    SubclassKit {
        class: GuardianClass::Hunter,
        element,
        supers: pick_abilities(&all_supers, s),
        grenades: g,
        melees: pick_abilities(&all_melees, m),
        class_abilities: pick_abilities(&all_classes, c),
        aspects: pick_aspects(&all_aspects, a),
    }
}
