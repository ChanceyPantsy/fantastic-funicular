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
| `effect` | What abilities do, as data: blasts, lingering zones (rifts, wells, bubbles, barricades, storms), seekers, chain lightning, buffs, heals, energy refunds, movement requests, and roaming supers with light/heavy attacks. Aspects are passives that fire on kills, casts, applied statuses and triggers |
| `catalog` | All 3 classes × 6 subclasses (Arc, Solar, Void, Stasis, Strand, Prismatic): 29 supers, 27 grenades, 25 melees, 9 class abilities and 46 aspects |
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
  layer over `Sandbox`. `demo/src/lib.rs` is a working example of one.
* **Data-driven mods:** enable `serde` and ship weapons, abilities and a
  `SandboxConfig` as JSON so they can be retuned without recompiling (see
  `tests/serde.rs`).

Area effects (Ignite, Jolt chains, Volatile, Shatter, splash, shield breaks)
only reach other combatants that have a `position`. Without one, an effect
hits only its primary target.

## Playable demo

`demo/` is **Guardian Arena**, a small top-down wave shooter that runs the
crate in the browser through WebAssembly. Pick any class, subclass, super,
grenade, melee, class ability and two aspects, then fight waves of Hive,
shielded Knights, all three champion types and an Ogre boss.

```sh
./demo/build.sh                          # builds demo/web/guardian_arena.wasm
python3 -m http.server -d demo/web 8080  # then open http://localhost:8080
```

The demo talks to Rust through a plain C ABI (`demo/src/lib.rs`): no
wasm-bindgen, just `extern "C"` functions plus JSON in and out. A Unity,
Unreal or Godot plugin can use the same interface.

## Examples

```sh
cargo run --example ttk_table   # shots/time to kill for the preset weapons
cargo run --example encounter   # a scripted PvE fight with an event log
```

## Development

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features
cargo fmt
```

## Not modelled yet

Movement itself (jump types, sprint, slide: the sandbox only reports
movement requests), fragments, exotic armor and weapon-perk catalogues,
Prismatic's Transcendence, verbs that leave pickups (Firesprites, Ionic
Traces, Stasis shards and Tangles are modelled as direct effects), intrinsic
burst fire for pulse rifles, and PvP flinch and aim assist. The modifier,
status and effect systems are data-driven, so most perks and fragments can
be written as a `Modifier`, `StatusDef` or aspect passive without code
changes.
