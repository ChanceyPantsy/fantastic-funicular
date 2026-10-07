//! A scripted PvE fight: a Solar warlock against a pack of shielded
//! enemies and an Unstoppable champion. Prints the event log.
//!
//! cargo run --example encounter

use guardian_combat::prelude::*;
use guardian_combat::weapon::{presets, FireError};

fn main() {
    let mut sb = Sandbox::new(SandboxConfig::pve());

    let mut hc = presets::hand_cannon_140(DamageType::Solar);
    hc.anti_champion = Some(ChampionKind::Unstoppable);
    hc.on_hit = vec![(StatusKind::Scorch, 15)];

    let player = sb.spawn(
        Combatant::guardian(
            "Warlock",
            Team(0),
            GuardianClass::Warlock,
            StatBlock::new(120, 60, 50, 150, 70, 30),
            &sb.config.guardian,
            &sb.config.stats,
        )
        .with_weapon(hc)
        .with_ability(ability::presets::incendiary_grenade())
        .with_ability(ability::presets::healing_rift())
        .with_ability(ability::presets::nova_bomb())
        .at([0.0, 0.0, 0.0]),
    );

    let mut pack = Vec::new();
    for i in 0..4 {
        let c = Combatant::enemy(format!("Acolyte {i}"), Team(1), Rank::Minor, 220.0)
            .with_shield(if i % 2 == 0 { 80.0 } else { 0.0 }, Some(DamageType::Solar))
            .at([20.0 + i as f32 * 1.5, 0.0, 0.0]);
        pack.push(sb.spawn(c));
    }
    let ogre = sb.spawn(
        Combatant::enemy("Unstoppable Ogre", Team(1), Rank::Miniboss, 6000.0)
            .as_champion(ChampionKind::Unstoppable)
            .at([25.0, 0.0, 0.0]),
    );

    let name = |sb: &Sandbox, id: CombatantId| sb.get(id).map_or("?".to_string(), |c| c.name.clone());
    let dt = 1.0 / 30.0;
    let mut t = 0.0f32;

    // Open with a grenade on the pack, then work through it with the hand cannon.
    sb.use_ability(player, AbilitySlot::ClassAbility, None).unwrap();
    sb.use_ability(player, AbilitySlot::Grenade, Some(pack[1])).unwrap();

    while t < 30.0 {
        let alive_pack: Vec<_> = pack.iter().copied().filter(|&id| sb.get(id).unwrap().is_alive()).collect();
        let target = alive_pack.first().copied().or(sb.get(ogre).unwrap().is_alive().then_some(ogre));
        let Some(target) = target else { break };

        let fired = sb.fire_weapon(player, Some(target), Shot::precision());
        if fired == Err(ActionError::Weapon(FireError::EmptyMagazine)) {
            sb.reload(player);
        }
        if sb.get(player).unwrap().loadout.super_.as_ref().unwrap().is_ready() {
            sb.use_ability(player, AbilitySlot::Super, Some(target)).unwrap();
        }
        sb.collect_orbs_near(player, 100.0);
        sb.tick(dt);
        t += dt;

        for e in sb.events() {
            match e {
                CombatEvent::Killed { victim, kind, .. } => {
                    println!("[{t:5.2}s] {} killed by {kind:?}", name(&sb, victim))
                }
                CombatEvent::ShieldBroken { target, element, .. } => {
                    println!("[{t:5.2}s] {}'s {element:?} shield broke", name(&sb, target))
                }
                CombatEvent::Triggered { kind, origin, .. } => {
                    println!("[{t:5.2}s] {kind:?} on {}", name(&sb, origin))
                }
                CombatEvent::ChampionStunned { target, kind } => {
                    println!("[{t:5.2}s] {} ({kind:?}) stunned", name(&sb, target))
                }
                CombatEvent::AbilityUsed { slot, .. } => println!("[{t:5.2}s] used {slot:?}"),
                CombatEvent::AbilityReady { slot, .. } => println!("[{t:5.2}s] {slot:?} ready"),
                CombatEvent::OrbCollected { .. } => println!("[{t:5.2}s] picked up an Orb of Power"),
                CombatEvent::ReloadFinished { .. } => println!("[{t:5.2}s] reloaded"),
                _ => {}
            }
        }
    }

    let o = sb.get(ogre).unwrap();
    println!("\nAfter {t:.1}s: ogre at {:.0}/{:.0} HP", o.health.health, o.health.max_health);
}
