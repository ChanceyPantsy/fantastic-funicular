#![cfg(feature = "serde")]

use guardian_combat::prelude::*;

#[test]
fn config_round_trips_through_json() {
    let cfg = SandboxConfig::pvp();
    let json = serde_json::to_string_pretty(&cfg).unwrap();
    let back: SandboxConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn weapons_load_from_json() {
    let json = r#"{
        "name": "Ace of Spades-ish",
        "archetype": "HandCannon",
        "ammo": "Primary",
        "damage_type": "Kinetic",
        "rpm": 140.0,
        "damage": 45.0,
        "precision_mult": 1.6,
        "pellets": 1,
        "falloff": { "start": 30.0, "end": 50.0, "min_mult": 0.5 },
        "magazine": 13,
        "reserves": null,
        "reload_time": 2.3,
        "charge_time": 0.0,
        "blast": null,
        "anti_champion": "Unstoppable",
        "on_hit": [["Scorch", 20]]
    }"#;
    let def: WeaponDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.anti_champion, Some(ChampionKind::Unstoppable));
    assert_eq!(def.on_hit, vec![(StatusKind::Scorch, 20)]);
}
