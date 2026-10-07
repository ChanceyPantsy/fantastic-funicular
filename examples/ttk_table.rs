//! Prints shots-to-kill and time-to-kill for the preset weapons against a
//! default guardian (200 HP).
//!
//! cargo run --example ttk_table

use guardian_combat::prelude::*;
use guardian_combat::weapon::presets;

fn main() {
    let hp = SandboxConfig::pvp().guardian.health + SandboxConfig::pvp().guardian.shield;
    let weapons = [
        presets::hand_cannon_140(DamageType::Kinetic),
        presets::auto_rifle_600(DamageType::Kinetic),
        presets::scout_rifle_260(DamageType::Kinetic),
        presets::shotgun_55(DamageType::Kinetic),
        presets::sniper_72(DamageType::Kinetic),
        presets::fusion_rifle(DamageType::Kinetic),
        presets::rocket_launcher(DamageType::Kinetic),
    ];

    println!("Target: {hp} HP, 10m\n");
    println!("{:<28} {:>6} {:>10} {:>10} {:>10}", "weapon", "rpm", "optimal", "crit-only", "body-only");
    for w in &weapons {
        let opt = w.optimal_kill(hp, 10.0).unwrap();
        let crit = w.time_to_kill(hp, true, 10.0).unwrap();
        let body = w.time_to_kill(hp, false, 10.0).unwrap();
        println!("{:<28} {:>6} {:>4}({}c) {:>9.2}s {:>9.2}s", w.name, w.rpm, opt.shots, opt.precision_hits, crit, body,);
    }

    let hc = presets::hand_cannon_140(DamageType::Kinetic);
    println!("\nHand cannon falloff (precision shots to kill):");
    for d in [10.0, 30.0, 40.0, 50.0, 60.0] {
        println!("  {d:>4}m: {}", hc.shots_to_kill(hp, true, d).unwrap());
    }
}
