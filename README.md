# fantastic-funicular

Destiny 2's combat mechanics as a Rust library (`guardian_combat`) that other
games and mods can use.

The library contains only the rules: damage types, shields, buffs and debuffs,
status effects, weapons, abilities, stats, Orbs of Power and champions. It
does **no** rendering, physics or hit detection, and it has no engine
dependency. Your game decides what got hit; the library works out what that
hit does.

> **Numbers are approximations.** All tuning lives in `SandboxConfig` and is a
> starting point, not datamined values. Change it in code, or load it from
> JSON/RON/TOML with the `serde` feature.
>
> This is an unofficial fan project, not affiliated with Bungie. *Destiny* is a
> trademark of Bungie, Inc. The crate implements game *mechanics* and includes
> no Bungie assets.

## What's modelled

| Module | Mechanics |
|---|---|
| `element` | Kinetic, Arc, Solar, Void, Stasis, Strand; Light vs Darkness; Prismatic subclasses; classes |
| `stats` | The six 0–200 armor stats (Weapons, Health, Class, Grenade, Super, Melee): base benefit up to 100, enhanced benefit from 100 to 200, set by tunable curves |
| `buffs` | Damage buff stacking: **Empowering** and **Debuff** take only the strongest, **Surges** add up to a cap, **Multiplicative** perks multiply, **Resist** values multiply up to a cap |
| `status` | Scorch → **Ignite**, Jolt chain lightning, Blind, Amplified, Volatile explosions, Weaken, Suppress, Devour, Invisible, Slow → **Freeze** → **Shatter**, Frost Armor, Sever, Suspend, Unravel threads, Woven Mail, Radiant, Restoration. PvE and PvP tuning tables |
| `health` | Overshield → shield → health; elemental shields (a matching element breaks them faster and causes a break explosion); regeneration delay; health refills before shield |
| `weapon` | 17 archetypes and their ammo types; RPM, precision multiplier, pellets, charge time, falloff, magazine/reserves/reload, splash damage; shots-to-kill, TTK and best crit/body mix; preset weapons |
| `ability` | Grenade / melee / class / super energy, charges, stat-scaled cooldowns; preset abilities for each element |
| `combatant` | Guardians and enemies, ranks (Minor → Boss), teams, **champions** (Barrier, Overload, Unstoppable) with stuns and re-stun immunity |
| `damage` | The full damage pipeline, with every multiplier kept for debugging/UI |
| `sandbox` | The runtime: spawn, fire, use abilities, tick, chain reactions (with a depth limit), Orbs of Power, super energy from damage and kills, Devour healing, and a `CombatEvent` stream |

### Damage pipeline

```
base (precision × falloff × pellets, + splash)
  × attacker outgoing modifiers  (Radiant, perks, Sever...)
  × target incoming modifiers    (Weaken, Woven Mail, Frost Armor...)
  × stat bonus                   (Weapons/Grenade/Melee/Super stat, PvE only)
  × rank multiplier              (bosses take less)
  × champion multiplier          (Unstoppable resists until stunned)
  → overshield → elemental shield → health
```

## Usage

```toml
[dependencies]
guardian_combat = { git = "https://github.com/ChanceyPantsy/fantastic-funicular" }
# or, to load tuning/weapons/abilities from data files:
# guardian_combat = { git = "...", features = ["serde"] }
```

```rust
use guardian_combat::prelude::*;

let mut sb = Sandbox::new(SandboxConfig::pve());

let player = sb.spawn(
    Combatant::guardian("Player", Team(0), GuardianClass::Warlock,
        StatBlock::new(100, 50, 50, 100, 50, 50),
        &sb.config.guardian, &sb.config.stats)
    .with_weapon(weapon::presets::hand_cannon_140(DamageType::Solar))
    .with_ability(ability::presets::incendiary_grenade()),
);
let enemy = sb.spawn(
    Combatant::enemy("Acolyte", Team(1), Rank::Minor, 250.0)
        .with_shield(60.0, Some(DamageType::Solar))
        .at([20.0, 0.0, 0.0]),
);

// From your engine's hit detection:
sb.fire_weapon(player, Some(enemy), Shot::precision())?;
sb.use_ability(player, AbilitySlot::Grenade, Some(enemy))?;

// Every frame:
sb.set_position(player, [0.0, 0.0, 0.0]);
sb.tick(dt);
for event in sb.events() {
    match event {
        CombatEvent::Killed { victim, .. } => { /* play death anim, despawn */ }
        CombatEvent::Triggered { kind: TriggerKind::Ignite, origin, .. } => { /* VFX */ }
        _ => {}
    }
}
```

### Using it from a game engine

* **Bevy / Fyrox / custom Rust engine:** depend on the crate directly. Keep a
  `Sandbox` as a resource, map your entities to `CombatantId`s, sync positions
  and call `tick` in your update loop.
* **Godot:** wrap `Sandbox` in a `godot-rust` (gdext) node.
* **Unity / Unreal / C# / C++ mods:** build a `cdylib` with a small `extern "C"`
  layer over `Sandbox` (not included yet).
* **Data-driven mods:** enable `serde` and ship weapons, abilities and a
  `SandboxConfig` as JSON so they can be retuned without recompiling (see
  `tests/serde.rs`).

Area effects (Ignite, Jolt chains, Volatile, Shatter, splash, shield breaks)
only reach other combatants that have a `position`. Without one, an effect
hits only its primary target.

## Examples

```sh
cargo run --example ttk_table   # shots/time to kill for the preset weapons
cargo run --example encounter   # a scripted PvE fight with an event log
```

## Development

```sh
cargo test --all-features
cargo clippy --all-targets --all-features
cargo fmt
```

## Not modelled yet

Movement (jump types, sprint, slide), exotic and weapon-perk catalogues,
aspects/fragments, intrinsic burst fire for pulse rifles, PvP flinch and aim
assist, and a C FFI layer. The modifier and status systems are data-driven,
so most perks and fragments can be expressed as a `Modifier` or a
`StatusDef` without code changes.
