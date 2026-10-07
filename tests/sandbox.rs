use guardian_combat::prelude::*;
use guardian_combat::sandbox::ActionError;
use guardian_combat::weapon::FireError;

const PLAYERS: Team = Team(0);
const ENEMIES: Team = Team(1);

fn guardian(sb: &Sandbox, stats: StatBlock) -> Combatant {
    Combatant::guardian("Player", PLAYERS, GuardianClass::Hunter, stats, &sb.config.guardian, &sb.config.stats)
        .at([0.0, 0.0, 0.0])
}

fn enemy(hp: f32, x: f32) -> Combatant {
    Combatant::enemy("Thrall", ENEMIES, Rank::Minor, hp).at([x, 0.0, 0.0])
}

fn count(events: &[CombatEvent], f: impl Fn(&CombatEvent) -> bool) -> usize {
    events.iter().filter(|e| f(e)).count()
}

fn killed(events: &[CombatEvent], id: CombatantId) -> bool {
    events.iter().any(|e| matches!(e, CombatEvent::Killed { victim, .. } if *victim == id))
}

#[test]
fn pvp_hand_cannon_three_crits_kills_a_guardian() {
    let mut sb = Sandbox::new(SandboxConfig::pvp());
    let a = sb
        .spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::hand_cannon_140(DamageType::Kinetic)));
    let mut b = guardian(&sb, StatBlock::default());
    b.team = ENEMIES;
    let b = sb.spawn(b.at([15.0, 0.0, 0.0]));

    for i in 0..3 {
        sb.fire_weapon(a, Some(b), Shot::precision()).unwrap();
        assert_eq!(sb.get(b).unwrap().is_alive(), i < 2);
        sb.tick(60.0 / 140.0);
    }
    assert!(killed(&sb.events(), b));
}

#[test]
fn friendly_fire_is_off_by_default() {
    let mut sb = Sandbox::default();
    let a = sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::sniper_72(DamageType::Kinetic)));
    let ally = sb.spawn(guardian(&sb, StatBlock::default()));
    sb.fire_weapon(a, Some(ally), Shot::precision()).unwrap();
    assert_eq!(sb.get(ally).unwrap().health.total(), 200.0);
}

#[test]
fn scorch_builds_to_ignite_which_hits_nearby_enemies() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()));
    let a = sb.spawn(enemy(2000.0, 10.0));
    let b = sb.spawn(enemy(2000.0, 12.0));
    let far = sb.spawn(enemy(2000.0, 40.0));

    sb.apply_status(a, StatusKind::Scorch, 100, Some(p));
    let ev = sb.events();
    assert_eq!(count(&ev, |e| matches!(e, CombatEvent::Triggered { kind: TriggerKind::Ignite, .. })), 1);
    let hp = |sb: &Sandbox, id| sb.get(id).unwrap().health.health;
    assert!(hp(&sb, a) < 2000.0, "ignite hits its origin");
    assert!(hp(&sb, b) < 2000.0, "and enemies in the radius");
    assert_eq!(hp(&sb, far), 2000.0);
    assert_eq!(hp(&sb, p), sb.get(p).unwrap().health.max_health, "never the player who caused it");
}

#[test]
fn volatile_explodes_once_when_damaged() {
    let mut sb = Sandbox::default();
    let p =
        sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::auto_rifle_600(DamageType::Kinetic)));
    let a = sb.spawn(enemy(5000.0, 10.0));
    sb.apply_status(a, StatusKind::Volatile, 1, Some(p));
    sb.events();

    sb.fire_weapon(p, Some(a), Shot::body()).unwrap();
    sb.tick(0.1);
    sb.fire_weapon(p, Some(a), Shot::body()).unwrap();
    let ev = sb.events();
    assert_eq!(count(&ev, |e| matches!(e, CombatEvent::Triggered { kind: TriggerKind::VolatileExplosion, .. })), 1);
    assert!(!sb.get(a).unwrap().statuses.has(StatusKind::Volatile));
}

#[test]
fn jolt_chains_to_at_most_three_others() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()));
    let origin = sb.spawn(enemy(5000.0, 10.0));
    let others: Vec<_> = (0..5).map(|i| sb.spawn(enemy(5000.0, 11.0 + i as f32))).collect();
    sb.apply_status(origin, StatusKind::Jolt, 1, Some(p));
    sb.deal_damage(origin, DamageInstance::new(10.0, DamageType::Kinetic, SourceKind::Environment).from(p));

    let hit = others.iter().filter(|&&id| sb.get(id).unwrap().health.health < 5000.0).count();
    assert_eq!(hit, 3);
    // The three nearest get hit.
    assert!(sb.get(others[0]).unwrap().health.health < 5000.0);
    assert_eq!(sb.get(others[4]).unwrap().health.health, 5000.0);
}

#[test]
fn slow_freezes_and_frozen_shatters() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_ability(ability::presets::coldsnap_grenade()));
    let a = sb.spawn(enemy(5000.0, 10.0).with_weapon(weapon::presets::auto_rifle_600(DamageType::Kinetic)));
    sb.use_ability(p, AbilitySlot::Grenade, Some(a)).unwrap();
    assert!(sb.get(a).unwrap().statuses.has(StatusKind::Frozen));
    assert_eq!(sb.fire_weapon(a, Some(p), Shot::body()), Err(ActionError::Weapon(FireError::Restricted)));

    sb.events();
    sb.deal_damage(a, DamageInstance::new(1.0, DamageType::Kinetic, SourceKind::Melee).from(p));
    let ev = sb.events();
    assert_eq!(count(&ev, |e| matches!(e, CombatEvent::Triggered { kind: TriggerKind::Shatter, .. })), 1);
    assert!(!sb.get(a).unwrap().statuses.has(StatusKind::Frozen));
}

#[test]
fn matching_element_breaks_shield_and_explodes() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::sniper_72(DamageType::Arc)));
    let shielded = sb.spawn(enemy(500.0, 10.0).with_shield(150.0, Some(DamageType::Arc)));
    let bystander = sb.spawn(enemy(500.0, 11.0));
    sb.fire_weapon(p, Some(shielded), Shot::body()).unwrap();
    let ev = sb.events();
    assert_eq!(count(&ev, |e| matches!(e, CombatEvent::ShieldBroken { .. })), 1);
    assert!(sb.get(bystander).unwrap().health.health < 500.0, "shield break explosion");
}

#[test]
fn unstoppable_resists_until_stunned_by_anti_champion_hit() {
    let mut sb = Sandbox::default();
    let mut ar = weapon::presets::auto_rifle_600(DamageType::Kinetic);
    ar.anti_champion = Some(ChampionKind::Unstoppable);
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(ar));
    let champ = sb.spawn(
        Combatant::enemy("Ogre", ENEMIES, Rank::Miniboss, 10_000.0)
            .as_champion(ChampionKind::Unstoppable)
            .at([10.0, 0.0, 0.0]),
    );
    // A plain hit is halved.
    sb.deal_damage(champ, DamageInstance::new(100.0, DamageType::Kinetic, SourceKind::Environment).from(p));
    let before = sb.get(champ).unwrap().health.health;
    let mult = sb.config.ranks.mult(Rank::Miniboss);
    assert!((10_000.0 - before - 100.0 * 0.5 * mult).abs() < 1e-2);

    sb.fire_weapon(p, Some(champ), Shot::body()).unwrap();
    let ev = sb.events();
    assert_eq!(count(&ev, |e| matches!(e, CombatEvent::ChampionStunned { .. })), 1);
    assert!(sb.get(champ).unwrap().champion.as_ref().unwrap().is_stunned());
}

#[test]
fn barrier_champion_is_immune_until_broken() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()));
    let champ = sb.spawn(
        Combatant::enemy("Hobgoblin", ENEMIES, Rank::Major, 1000.0)
            .as_champion(ChampionKind::Barrier)
            .at([10.0, 0.0, 0.0]),
    );
    let hit = |amount| DamageInstance::new(amount, DamageType::Kinetic, SourceKind::Environment).from(p);
    sb.deal_damage(champ, hit(400.0));
    assert!(sb.get(champ).unwrap().champion.as_ref().unwrap().barrier_up);

    sb.deal_damage(champ, hit(100.0));
    assert_eq!(sb.get(champ).unwrap().health.health, 600.0, "immune behind barrier");

    sb.deal_damage(champ, hit(100.0).anti_champion(Some(ChampionKind::Barrier)));
    let c = sb.get(champ).unwrap();
    assert!(!c.champion.as_ref().unwrap().barrier_up);
    assert_eq!(c.health.health, 500.0, "the breaking hit lands");
}

#[test]
fn overload_regenerates_unless_stunned() {
    let mut sb = Sandbox::default();
    let champ = sb.spawn(Combatant::enemy("Captain", ENEMIES, Rank::Major, 1000.0).as_champion(ChampionKind::Overload));
    sb.deal_damage(champ, DamageInstance::new(500.0, DamageType::Kinetic, SourceKind::Environment));
    sb.tick(1.0);
    assert!(sb.get(champ).unwrap().health.health > 500.0);

    let hp = sb.get(champ).unwrap().health.health;
    sb.apply_status(champ, StatusKind::Suppress, 1, None);
    sb.tick(1.0);
    assert_eq!(sb.get(champ).unwrap().health.health, hp, "suppress stuns overload");
}

#[test]
fn kills_drop_orbs_and_orbs_give_energy() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(
        guardian(&sb, StatBlock::new(0, 100, 0, 0, 0, 0))
            .with_weapon(weapon::presets::sniper_72(DamageType::Kinetic))
            .with_ability(ability::presets::vortex_grenade()),
    );
    let major = sb.spawn(Combatant::enemy("Knight", ENEMIES, Rank::Major, 100.0).at([5.0, 0.0, 0.0]));
    sb.use_ability(p, AbilitySlot::Grenade, None).unwrap();
    sb.fire_weapon(p, Some(major), Shot::precision()).unwrap();
    assert_eq!(sb.orbs().count(), 1);

    sb.deal_damage(p, DamageInstance::new(150.0, DamageType::Kinetic, SourceKind::Environment));
    let hp_before = sb.get(p).unwrap().health.health;
    sb.set_position(p, [5.0, 0.0, 0.5]);
    assert_eq!(sb.collect_orbs_near(p, 2.0), 1);
    let c = sb.get(p).unwrap();
    assert!((c.loadout.grenade.as_ref().unwrap().energy - sb.config.orbs.grenade_energy).abs() < 1e-5);
    assert!(c.health.health > hp_before, "Health stat makes orbs heal");
}

#[test]
fn devour_heals_on_kill() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::sniper_72(DamageType::Void)));
    let e = sb.spawn(enemy(50.0, 10.0));
    sb.deal_damage(p, DamageInstance::new(150.0, DamageType::Kinetic, SourceKind::Environment));
    sb.apply_status(p, StatusKind::Devour, 1, Some(p));
    sb.fire_weapon(p, Some(e), Shot::body()).unwrap();
    assert_eq!(sb.get(p).unwrap().health.health, 70.0);
}

#[test]
fn abilities_regenerate_faster_with_stats_and_respect_suppress() {
    let mut sb = Sandbox::default();
    let slow = sb.spawn(guardian(&sb, StatBlock::default()).with_ability(ability::presets::pulse_grenade()));
    let fast =
        sb.spawn(guardian(&sb, StatBlock::new(0, 0, 0, 100, 0, 0)).with_ability(ability::presets::pulse_grenade()));
    for id in [slow, fast] {
        sb.use_ability(id, AbilitySlot::Grenade, None).unwrap();
        assert_eq!(sb.use_ability(id, AbilitySlot::Grenade, None), Err(ActionError::NotReady));
    }
    for _ in 0..40 {
        sb.tick(1.0);
    }
    assert!(sb.get(fast).unwrap().loadout.grenade.as_ref().unwrap().is_ready());
    assert!(!sb.get(slow).unwrap().loadout.grenade.as_ref().unwrap().is_ready());

    sb.apply_status(fast, StatusKind::Suppress, 1, None);
    assert_eq!(sb.use_ability(fast, AbilitySlot::Grenade, None), Err(ActionError::Restricted));
}

#[test]
fn rocket_splash_hits_nearby_enemies() {
    let mut sb = Sandbox::default();
    let p =
        sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(weapon::presets::rocket_launcher(DamageType::Solar)));
    let a = sb.spawn(enemy(1000.0, 20.0));
    let b = sb.spawn(enemy(1000.0, 23.0));
    sb.fire_weapon(p, Some(a), Shot::body()).unwrap();
    assert_eq!(sb.get(a).unwrap().health.health, 700.0);
    assert_eq!(sb.get(b).unwrap().health.health, 750.0);
}

#[test]
fn damage_earns_super_energy() {
    let mut sb = Sandbox::default();
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_ability(ability::presets::nova_bomb()));
    let e = sb.spawn(enemy(1_000_000.0, 10.0));
    sb.deal_damage(e, DamageInstance::new(10_000.0, DamageType::Kinetic, SourceKind::Environment).from(p));
    let energy = sb.get(p).unwrap().loadout.super_.as_ref().unwrap().energy;
    assert!((energy - 0.5).abs() < 1e-4, "{energy}");
}

#[test]
fn champions_cannot_be_stun_locked() {
    let mut sb = Sandbox::default();
    let mut ar = weapon::presets::auto_rifle_600(DamageType::Kinetic);
    ar.anti_champion = Some(ChampionKind::Unstoppable);
    let p = sb.spawn(guardian(&sb, StatBlock::default()).with_weapon(ar));
    let champ = sb.spawn(
        Combatant::enemy("Ogre", ENEMIES, Rank::Miniboss, 1e6)
            .as_champion(ChampionKind::Unstoppable)
            .at([5.0, 0.0, 0.0]),
    );
    let stun = sb.config.champions.stun_duration;
    let immunity = sb.config.champions.stun_immunity;
    let mut stuns = 0;
    let mut t = 0.0;
    while t < stun + immunity + 1.0 {
        if sb.fire_weapon(p, Some(champ), Shot::body()) == Err(ActionError::Weapon(FireError::EmptyMagazine)) {
            sb.reload(p);
        }
        sb.tick(0.1);
        t += 0.1;
        stuns += count(&sb.events(), |e| matches!(e, CombatEvent::ChampionStunned { .. }));
    }
    assert_eq!(stuns, 2, "one stun, then immunity, then a second stun");
}
