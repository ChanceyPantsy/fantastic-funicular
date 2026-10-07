use guardian_combat::buffs::ModifierScope;
use guardian_combat::effect::{Anchor, KillFilter, PassiveTrigger, RoamingSuper, SuperAttack, ZoneDef};
use guardian_combat::prelude::*;

const PLAYERS: Team = Team(0);
const ENEMIES: Team = Team(1);

fn player(sb: &Sandbox) -> Combatant {
    Combatant::guardian(
        "P",
        PLAYERS,
        GuardianClass::Warlock,
        StatBlock::default(),
        &sb.config.guardian,
        &sb.config.stats,
    )
    .at([0.0, 0.0, 0.0])
}

fn enemy_at(x: f32, hp: f32) -> Combatant {
    Combatant::enemy("E", ENEMIES, Rank::Minor, hp).at([x, 0.0, 0.0])
}

fn hp(sb: &Sandbox, id: CombatantId) -> f32 {
    sb.get(id).unwrap().health.health
}

fn grenade(effects: Vec<Effect>) -> AbilityDef {
    let mut a = AbilityDef::new("Test", AbilitySlot::Grenade, DamageType::Void, 10.0);
    a.effects = effects;
    a
}

#[test]
fn blast_at_a_point_hits_only_enemies_in_radius() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::blast(100.0, 3.0)])));
    let near = sb.spawn(enemy_at(10.0, 500.0));
    let far = sb.spawn(enemy_at(20.0, 500.0));
    let ally = sb.spawn(player(&sb).at([10.0, 1.0, 0.0]));
    sb.use_ability(p, AbilitySlot::Grenade, [10.0, 0.0, 0.0]).unwrap();
    assert_eq!(hp(&sb, near), 400.0);
    assert_eq!(hp(&sb, far), 500.0);
    assert_eq!(sb.get(ally).unwrap().health.total(), 200.0);
}

#[test]
fn zones_pulse_until_they_expire() {
    let mut sb = Sandbox::default();
    let zone = ZoneDef::new(4.0, 2.0, Anchor::Point).pulse(0.5).damage(10.0);
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::Zone(zone)])));
    let e = sb.spawn(enemy_at(10.0, 500.0));
    sb.use_ability(p, AbilitySlot::Grenade, e).unwrap();
    assert_eq!(hp(&sb, e), 490.0, "first pulse is immediate");
    for _ in 0..40 {
        sb.tick(0.1);
    }
    // Pulses at 0, 0.5, 1.0, 1.5 within the 2s lifetime.
    assert_eq!(hp(&sb, e), 460.0);
    assert_eq!(sb.zones().count(), 0);
}

#[test]
fn zones_buff_allies_inside() {
    let mut sb = Sandbox::default();
    let well = ZoneDef::new(5.0, 10.0, Anchor::Point).heal(20.0).ally_modifier(Modifier::empowering(
        "Well",
        ModifierScope::All,
        0.25,
    ));
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::Zone(well)])));
    sb.deal_damage(p, DamageInstance::new(160.0, DamageType::Kinetic, SourceKind::Environment));
    sb.use_ability(p, AbilitySlot::Grenade, None).unwrap();
    let c = sb.get(p).unwrap();
    assert_eq!(c.health.health, 60.0);
    assert!(c.modifiers.iter().any(|m| m.name == "Well"));

    // Walk out: the buff lapses shortly after.
    sb.set_position(p, [50.0, 0.0, 0.0]);
    for _ in 0..10 {
        sb.tick(0.1);
    }
    assert!(!sb.get(p).unwrap().modifiers.iter().any(|m| m.name == "Well"));
}

#[test]
fn seekers_hit_the_nearest_targets() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::seekers(2, 50.0, 10.0, &[])])));
    let a = sb.spawn(enemy_at(5.0, 500.0));
    let b = sb.spawn(enemy_at(6.0, 500.0));
    let c = sb.spawn(enemy_at(9.0, 500.0));
    sb.use_ability(p, AbilitySlot::Grenade, [5.0, 0.0, 0.0]).unwrap();
    assert_eq!((hp(&sb, a), hp(&sb, b), hp(&sb, c)), (450.0, 450.0, 500.0));
    let beams = sb.events().iter().filter(|e| matches!(e, CombatEvent::Beam { .. })).count();
    assert_eq!(beams, 2);
}

#[test]
fn chain_jumps_between_enemies_in_range() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::chain(50.0, 2, 4.0, &[])])));
    let ids: Vec<_> = [5.0, 8.0, 11.0, 14.0].iter().map(|&x| sb.spawn(enemy_at(x, 500.0))).collect();
    let gap = sb.spawn(enemy_at(30.0, 500.0));
    sb.use_ability(p, AbilitySlot::Grenade, ids[0]).unwrap();
    let hit: Vec<bool> = ids.iter().map(|&id| hp(&sb, id) < 500.0).collect();
    assert_eq!(hit, vec![true, true, true, false], "first target plus two jumps");
    assert_eq!(hp(&sb, gap), 500.0);
}

#[test]
fn roaming_super_replaces_weapons_until_it_ends() {
    let mut sb = Sandbox::default();
    let roam = RoamingSuper::new(5.0, SuperAttack::new("Shot", 0.5, 30.0, vec![Effect::blast(100.0, 0.0)])).uses(2);
    let mut sup = AbilityDef::new("Gun", AbilitySlot::Super, DamageType::Solar, 1.0);
    sup.effects = vec![Effect::Roam(roam)];
    let p = sb.spawn(player(&sb).with_ability(sup).with_weapon(weapon::presets::hand_cannon_140(DamageType::Kinetic)));
    let e = sb.spawn(enemy_at(10.0, 1000.0));
    sb.get_mut(p).unwrap().loadout.super_.as_mut().unwrap().add_energy(1.0);

    sb.use_ability(p, AbilitySlot::Super, None).unwrap();
    assert!(sb.get(p).unwrap().in_super());
    assert_eq!(sb.fire_weapon(p, Some(e), Shot::body()), Err(ActionError::InSuper));
    assert_eq!(sb.super_attack(p, true, e), Err(ActionError::NoHeavyAttack));

    sb.super_attack(p, false, e).unwrap();
    assert_eq!(sb.super_attack(p, false, e), Err(ActionError::NotReady), "cooldown");
    sb.tick(0.5);
    sb.super_attack(p, false, e).unwrap();
    assert!(!sb.get(p).unwrap().in_super(), "ends after its last shot");
    assert_eq!(hp(&sb, e), 800.0);
    assert!(sb.fire_weapon(p, Some(e), Shot::body()).is_ok());
}

#[test]
fn roaming_super_grants_damage_resistance() {
    let mut sb = Sandbox::default();
    let roam = RoamingSuper::new(5.0, SuperAttack::new("Hit", 0.5, 5.0, vec![])).resist(0.5);
    let mut sup = AbilityDef::new("Roam", AbilitySlot::Super, DamageType::Arc, 1.0);
    sup.effects = vec![Effect::Roam(roam)];
    let p = sb.spawn(player(&sb).with_ability(sup));
    sb.get_mut(p).unwrap().loadout.super_.as_mut().unwrap().add_energy(1.0);
    sb.use_ability(p, AbilitySlot::Super, None).unwrap();
    sb.deal_damage(p, DamageInstance::new(100.0, DamageType::Kinetic, SourceKind::Environment));
    assert_eq!(sb.get(p).unwrap().health.total(), 150.0);
    for _ in 0..60 {
        sb.tick(0.1);
    }
    assert!(!sb.get(p).unwrap().in_super(), "times out");
}

#[test]
fn aspect_passives_fire_on_kills_casts_and_triggers() {
    let mut sb = Sandbox::default();
    let aspect = AspectDef::new("Test Aspect", DamageType::Solar, "")
        .on(PassiveTrigger::Kill(KillFilter::Any), 0.0, vec![Effect::Overshield(25.0)])
        .on(PassiveTrigger::Cast(AbilitySlot::Grenade), 0.0, vec![Effect::SelfStatus(StatusKind::Radiant, 1)]);
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::blast(100.0, 0.0)])).with_aspect(aspect));
    let e = sb.spawn(enemy_at(5.0, 50.0));
    sb.use_ability(p, AbilitySlot::Grenade, e).unwrap();
    let c = sb.get(p).unwrap();
    assert!(c.statuses.has(StatusKind::Radiant));
    assert_eq!(c.health.overshield, 25.0);
}

#[test]
fn passive_cooldowns_limit_activations() {
    let mut sb = Sandbox::default();
    let aspect = AspectDef::new("Slow Aspect", DamageType::Solar, "").on(
        PassiveTrigger::Kill(KillFilter::Any),
        5.0,
        vec![Effect::Heal(10.0)],
    );
    let p = sb.spawn(player(&sb).with_ability(grenade(vec![Effect::blast(100.0, 10.0)])).with_aspect(aspect));
    for i in 0..3 {
        sb.spawn(enemy_at(5.0 + i as f32, 50.0));
    }
    sb.use_ability(p, AbilitySlot::Grenade, [5.0, 0.0, 0.0]).unwrap();
    let triggered = sb.events().iter().filter(|e| matches!(e, CombatEvent::AspectTriggered { .. })).count();
    assert_eq!(triggered, 1);
}

#[test]
fn movement_is_reported_to_the_host() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(player(&sb).with_ability(catalog::ability_named("Grapple").unwrap()));
    sb.use_ability(p, AbilitySlot::Grenade, [10.0, 5.0, 0.0]).unwrap();
    let moved = sb.events().into_iter().find_map(|e| match e {
        CombatEvent::Move { kind, toward, .. } => Some((kind, toward)),
        _ => None,
    });
    assert_eq!(moved, Some((MoveKind::Grapple, Some([10.0, 5.0, 0.0]))));
}

#[test]
fn signature_abilities_do_their_thing() {
    let mut sb = Sandbox::default();
    let well = catalog::ability_named("Well of Radiance").unwrap();
    let p = sb.spawn(player(&sb).with_ability(well).with_weapon(weapon::presets::hand_cannon_140(DamageType::Kinetic)));
    let e = sb.spawn(enemy_at(10.0, 10_000.0));
    sb.get_mut(p).unwrap().loadout.super_.as_mut().unwrap().add_energy(1.0);
    sb.use_ability(p, AbilitySlot::Super, None).unwrap();
    sb.tick(0.1);
    let before = hp(&sb, e);
    sb.fire_weapon(p, Some(e), Shot::body()).unwrap();
    let dealt = before - hp(&sb, e);
    assert!((dealt - 45.0 * 1.25).abs() < 0.01, "Well empowers weapons: {dealt}");
}

/// Casts every ability of every kit (and every super's attacks) into a crowd
/// of enemies, then lets the fight play out. Catches panics, runaway chain
/// reactions and abilities that silently do nothing.
#[test]
fn every_kit_casts_cleanly() {
    for kit in catalog::all_kits() {
        let mut sb = Sandbox::default();
        let mut p = Combatant::guardian(
            "P",
            PLAYERS,
            kit.class,
            StatBlock::new(100, 100, 100, 100, 100, 100),
            &sb.config.guardian,
            &sb.config.stats,
        )
        .at([0.0, 0.0, 0.0])
        .with_weapon(weapon::presets::auto_rifle_600(DamageType::Kinetic));
        for a in kit.aspects.iter().cloned() {
            p = p.with_aspect(a);
        }
        let p = sb.spawn(p);
        let enemies: Vec<_> = (0..8)
            .map(|i| sb.spawn(enemy_at(4.0 + (i % 4) as f32 * 2.0, 400.0).at([4.0 + i as f32, (i % 3) as f32, 0.0])))
            .collect();

        for a in kit.abilities() {
            sb.get_mut(p).unwrap().loadout.equip(a.clone());
            let state = sb.get_mut(p).unwrap().loadout.get_mut(a.slot).unwrap();
            state.charges = 1;
            let target = enemies.iter().copied().find(|&e| sb.get(e).unwrap().is_alive());
            let result = sb.use_ability(p, a.slot, target);
            assert!(
                result.is_ok() || result == Err(ActionError::Restricted),
                "{:?} {:?}: {} failed: {result:?}",
                kit.class,
                kit.element,
                a.name
            );
            let did_something = !sb.events().is_empty();
            assert!(did_something, "{} produced no events", a.name);

            if let Some(r) = a.roaming() {
                for heavy in [false, true] {
                    if heavy && r.heavy.is_none() {
                        continue;
                    }
                    let target = enemies.iter().copied().find(|&e| sb.get(e).unwrap().is_alive());
                    let res = sb.super_attack(p, heavy, target);
                    assert!(res.is_ok() || !sb.get(p).unwrap().in_super(), "{}: {res:?}", a.name);
                }
                sb.end_super(p);
            }
            for _ in 0..30 {
                sb.tick(0.1);
            }
        }
    }
}
