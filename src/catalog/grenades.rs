//! Grenades. Every class shares the grenades of an element.

use super::{ability, cd};
use crate::ability::{AbilityDef, AbilitySlot::Grenade};
use crate::effect::{Anchor, Effect, MoveKind, ZoneDef};
use crate::element::DamageType::{self, *};
use crate::status::StatusKind::*;

fn zone(radius: f32, duration: f32) -> ZoneDef {
    ZoneDef::new(radius, duration, Anchor::Point)
}

pub fn arc() -> Vec<AbilityDef> {
    vec![
        ability(
            "Arcbolt Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Chains lightning between nearby enemies.",
            vec![Effect::chain(110.0, 3, 10.0, &[(Jolt, 1)])],
        ),
        ability(
            "Flashbang Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Explodes and blinds enemies.",
            vec![Effect::blast_with(60.0, 5.0, &[(Blind, 1)])],
        ),
        ability(
            "Flux Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Sticks to a target and deals heavy damage in a small blast.",
            vec![Effect::blast(260.0, 2.5)],
        ),
        ability(
            "Lightning Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Two charges. Sticks to a surface and pulses lightning.",
            vec![Effect::Zone(zone(4.0, 5.0).pulse(1.0).damage(55.0).enemy_status(Jolt, 1))],
        )
        .charges(2),
        ability(
            "Pulse Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Pulses damage and jolts enemies inside.",
            vec![Effect::Zone(zone(4.0, 4.0).pulse(0.8).damage(50.0).enemy_status(Jolt, 1))],
        ),
        ability(
            "Skip Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Splits into tracking projectiles.",
            vec![Effect::seekers(4, 60.0, 12.0, &[])],
        ),
        ability(
            "Storm Grenade",
            Grenade,
            Arc,
            cd::GRENADE,
            20.0,
            "Calls down a lightning storm.",
            vec![Effect::Zone(zone(5.0, 3.0).pulse(0.3).damage(30.0))],
        ),
    ]
}

pub fn solar() -> Vec<AbilityDef> {
    vec![
        ability(
            "Firebolt Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Fires Solar bolts at nearby enemies.",
            vec![Effect::seekers(3, 50.0, 15.0, &[(Scorch, 30)])],
        ),
        ability(
            "Fusion Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Sticks to a target and explodes.",
            vec![Effect::blast_with(280.0, 3.0, &[(Scorch, 30)])],
        ),
        ability(
            "Healing Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Heals and grants Restoration to you and nearby allies.",
            vec![
                Effect::blast(40.0, 3.0),
                Effect::HealAllies { amount: 100.0, radius: 8.0 },
                Effect::AllyStatus { kind: Restoration, stacks: 1, radius: 8.0 },
            ],
        ),
        ability(
            "Incendiary Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Explodes and scorches enemies.",
            vec![Effect::blast_with(150.0, 4.0, &[(Scorch, 60)])],
        ),
        ability(
            "Solar Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Creates a burning flare that scorches enemies inside.",
            vec![Effect::Zone(zone(4.0, 4.0).damage(30.0).enemy_status(Scorch, 10))],
        ),
        ability(
            "Swarm Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Releases drones that seek enemies.",
            vec![Effect::seekers(5, 40.0, 12.0, &[(Scorch, 15)])],
        ),
        ability(
            "Thermite Grenade",
            Grenade,
            Solar,
            cd::GRENADE,
            20.0,
            "Sends a line of fire forward.",
            vec![
                Effect::blast_with(120.0, 2.0, &[(Scorch, 20)]),
                Effect::Zone(zone(3.0, 3.0).damage(25.0).enemy_status(Scorch, 15)),
            ],
        ),
    ]
}

pub fn void() -> Vec<AbilityDef> {
    vec![
        ability(
            "Axion Bolt",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Fires two seeking Void bolts.",
            vec![Effect::seekers(2, 90.0, 15.0, &[])],
        ),
        ability(
            "Magnetic Grenade",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Sticks and explodes twice.",
            vec![Effect::blast(150.0, 3.0), Effect::blast(150.0, 3.0)],
        ),
        ability(
            "Scatter Grenade",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Splits into submunitions that track enemies.",
            vec![Effect::blast(50.0, 6.0), Effect::seekers(6, 40.0, 6.0, &[])],
        ),
        ability(
            "Suppressor Grenade",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Explodes and suppresses enemies' abilities.",
            vec![Effect::blast_with(50.0, 5.0, &[(Suppress, 1)])],
        ),
        ability(
            "Vortex Grenade",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Creates a vortex that damages enemies inside.",
            vec![Effect::Zone(zone(4.0, 4.0).pulse(0.4).damage(35.0))],
        ),
        ability(
            "Voidwall Grenade",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Creates a wall of Void flame.",
            vec![Effect::Zone(zone(4.0, 4.0).damage(40.0))],
        ),
        ability(
            "Void Spike",
            Grenade,
            Void,
            cd::GRENADE,
            20.0,
            "Plants a spike that damages enemies around it.",
            vec![Effect::Zone(zone(3.0, 5.0).damage(45.0))],
        ),
    ]
}

pub fn stasis() -> Vec<AbilityDef> {
    vec![
        ability(
            "Coldsnap Grenade",
            Grenade,
            Stasis,
            cd::GRENADE,
            20.0,
            "A seeker that freezes the first enemy it reaches, then seeks again.",
            vec![Effect::seekers(2, 40.0, 15.0, &[(Slow, 100)])],
        ),
        ability(
            "Duskfield Grenade",
            Grenade,
            Stasis,
            cd::GRENADE,
            20.0,
            "Creates a field that slows enemies inside.",
            vec![Effect::Zone(zone(5.0, 5.0).enemy_status(Slow, 20))],
        ),
        ability(
            "Glacier Grenade",
            Grenade,
            Stasis,
            cd::GRENADE,
            20.0,
            "Raises a wall of Stasis crystals that slows enemies.",
            vec![Effect::blast_with(30.0, 4.0, &[(Slow, 60)])],
        ),
    ]
}

pub fn strand() -> Vec<AbilityDef> {
    vec![
        ability(
            "Grapple",
            Grenade,
            Strand,
            60.0,
            20.0,
            "Two charges. Pulls you to the target point.",
            vec![Effect::moves(MoveKind::Grapple, 20.0)],
        )
        .charges(2),
        ability(
            "Shackle Grenade",
            Grenade,
            Strand,
            cd::GRENADE,
            20.0,
            "Explodes and suspends enemies.",
            vec![Effect::blast_with(60.0, 5.0, &[(Suspend, 1)])],
        ),
        ability(
            "Threadling Grenade",
            Grenade,
            Strand,
            cd::GRENADE,
            20.0,
            "Releases threadlings that chase enemies.",
            vec![Effect::seekers(3, 70.0, 15.0, &[])],
        ),
    ]
}

/// The grenades available to a damage type.
pub fn for_element(element: DamageType) -> Vec<AbilityDef> {
    match element {
        Arc => arc(),
        Solar => solar(),
        Void => void(),
        Stasis => stasis(),
        Strand => strand(),
        Kinetic => Vec::new(),
    }
}

/// Every grenade in the game, for Prismatic picks.
pub fn all() -> Vec<AbilityDef> {
    [arc(), solar(), void(), stasis(), strand()].concat()
}
