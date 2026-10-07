//! The match: owns the `guardian_combat` sandbox, keeps it in sync with the
//! Bevy world, and hands this frame's combat events to everyone else.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use guardian_combat::ability::AbilitySlot;
use guardian_combat::catalog;
use guardian_combat::combatant::{ChampionKind, Combatant, CombatantId, Team};
use guardian_combat::config::SandboxConfig;
use guardian_combat::element::{DamageType, GuardianClass};
use guardian_combat::sandbox::Sandbox;
use guardian_combat::stats::{Curve, StatBlock};
use guardian_combat::weapon::{presets, AmmoType};

use crate::common::{a3, v3, AppState, FrameEvents, LoadoutChoice, MatchEntity, PendingLoadout, Phase, Rng};
use crate::models::{ModelKind, Models, Rig};
use crate::player::{spawn_player, Player};

pub const PLAYER_TEAM: Team = Team(0);
pub const ENEMY_TEAM: Team = Team(1);

#[derive(Resource)]
pub struct Combat {
    pub sb: Sandbox,
    pub player: CombatantId,
    /// Sandbox id → enemy entity.
    pub enemies: HashMap<CombatantId, Entity>,
    pub choice: LoadoutChoice,
    pub wave: u32,
    pub wave_delay: f32,
    pub spawn_queue: VecDeque<crate::enemies::EnemyKind>,
    pub spawn_timer: f32,
    pub kills: u32,
    pub over: bool,
    pub time: f32,
    pub hint: Option<(String, f32)>,
    pub banner: Option<(String, f32)>,
    /// Recent casts, for the debug snapshot.
    pub cast_log: VecDeque<String>,
}

impl Combat {
    pub fn hint(&mut self, text: impl Into<String>) {
        self.hint = Some((text.into(), 1.8));
    }

    pub fn banner(&mut self, text: impl Into<String>) {
        self.banner = Some((text.into(), 2.2));
    }

    pub fn player_alive(&self) -> bool {
        self.sb.get(self.player).is_some_and(|c| c.is_alive())
    }
}

/// Set when a match (re)starts, so the HUD rebuilds.
#[derive(Resource)]
pub struct NewMatch;

/// Ammo pickups dropped by enemies.
#[derive(Component)]
pub struct AmmoBrick {
    pub ammo: AmmoType,
    pub life: f32,
}

/// Visual for an Orb of Power, keyed by its sandbox id.
#[derive(Component)]
pub struct OrbVisual(pub guardian_combat::sandbox::OrbId);

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameEvents>()
            .init_resource::<PendingLoadout>()
            .init_resource::<Rng>()
            .add_systems(Update, begin_match.run_if(not(in_state(AppState::Loading))))
            .add_systems(
                Update,
                (sync_and_tick, pickups).chain().in_set(Phase::Sim).run_if(in_state(AppState::Playing)),
            );
    }
}

fn demo_config() -> SandboxConfig {
    let mut cfg = SandboxConfig::pve();
    // Faster cooldowns than the real game so a short session shows everything.
    cfg.stats.ability_regen = Curve::linear(0.0, 1.0, 100.0, 6.0);
    cfg.energy.super_per_damage = 1.0 / 5000.0;
    cfg.energy.super_per_kill = 0.02;
    cfg.orbs.lifetime = 25.0;
    cfg
}

fn model_for(class: GuardianClass) -> ModelKind {
    match class {
        GuardianClass::Titan => ModelKind::Knight,
        GuardianClass::Hunter => ModelKind::Rogue,
        GuardianClass::Warlock => ModelKind::Mage,
    }
}

/// Builds the player's combatant from a loadout.
fn make_guardian(choice: &LoadoutChoice, cfg: &SandboxConfig) -> Result<Combatant, String> {
    let kit = catalog::kit(choice.class, choice.element);
    let find = |slot: AbilitySlot, name: &str| {
        kit.options(slot)
            .iter()
            .find(|a| a.name == name)
            .cloned()
            .ok_or_else(|| format!("{name} isn't available on this subclass"))
    };
    let element = choice.element.damage_type().unwrap_or(DamageType::Void);
    let mut primary = presets::hand_cannon_140(DamageType::Kinetic);
    primary.name = "Hand Cannon".into();
    primary.anti_champion = Some(ChampionKind::Barrier);
    primary.falloff.start = 40.0;
    primary.falloff.end = 60.0;
    let mut special = presets::shotgun_55(element);
    special.name = "Shotgun".into();
    special.anti_champion = Some(ChampionKind::Overload);
    let mut heavy = presets::rocket_launcher(DamageType::Solar);
    heavy.name = "Rocket Launcher".into();
    heavy.anti_champion = Some(ChampionKind::Unstoppable);

    let mut g = Combatant::guardian(
        "Guardian",
        PLAYER_TEAM,
        choice.class,
        StatBlock::new(100, 100, 100, 100, 100, 100),
        &cfg.guardian,
        &cfg.stats,
    )
    .with_weapon(primary)
    .with_weapon(special)
    .with_weapon(heavy)
    .with_ability(find(AbilitySlot::Super, &choice.super_)?)
    .with_ability(find(AbilitySlot::Grenade, &choice.grenade)?)
    .with_ability(find(AbilitySlot::Melee, &choice.melee)?)
    .with_ability(find(AbilitySlot::ClassAbility, &choice.class_ability)?);
    for name in choice.aspects.iter().take(2) {
        let a = kit.aspects.iter().find(|a| &a.name == name).ok_or_else(|| format!("{name} isn't an aspect here"))?;
        g = g.with_aspect(a.clone());
    }
    if let Some(s) = g.loadout.super_.as_mut() {
        s.add_energy(0.6);
    }
    Ok(g)
}

#[allow(clippy::too_many_arguments)]
pub fn begin_match(
    mut commands: Commands,
    mut pending: ResMut<PendingLoadout>,
    old: Query<Entity, With<MatchEntity>>,
    models: Res<Models>,
    mut next: ResMut<NextState<AppState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(choice) = pending.0.take() else { return };
    let cfg = demo_config();
    let guardian = match make_guardian(&choice, &cfg) {
        Ok(g) => g,
        Err(e) => {
            error!("bad loadout: {e}");
            crate::web::report_error(&e);
            return;
        }
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let mut sb = Sandbox::new(cfg);
    let start = Vec3::new(0.0, 0.0, 18.0);
    let player_id = sb.spawn(guardian.at(a3(start)));

    let body = models.spawn(&mut commands, Rig::new(model_for(choice.class)), Transform::from_scale(Vec3::splat(0.85)));
    commands.entity(body).insert(MatchEntity);
    spawn_player(&mut commands, &mut meshes, &mut materials, start, body, &choice);

    let mut combat = Combat {
        sb,
        player: player_id,
        enemies: HashMap::new(),
        choice: choice.clone(),
        wave: 0,
        wave_delay: 2.5,
        spawn_queue: VecDeque::new(),
        spawn_timer: 0.0,
        kills: 0,
        over: false,
        time: 0.0,
        hint: None,
        banner: None,
        cast_log: VecDeque::new(),
    };
    combat.hint("Click to fight. WASD to move, mouse to aim.");
    commands.insert_resource(combat);
    commands.insert_resource(FrameEvents::default());
    commands.insert_resource(NewMatch);
    next.set(AppState::Playing);
}

/// Pushes positions into the sandbox, advances it, and collects its events.
fn sync_and_tick(
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    mut frame: ResMut<FrameEvents>,
    player: Query<&Transform, With<Player>>,
    enemies: Query<(&crate::enemies::Enemy, &Transform)>,
) {
    let dt = time.delta_secs().min(0.1);
    let c = &mut *combat;
    c.time += dt;
    for t in [&mut c.hint, &mut c.banner] {
        if let Some((_, s)) = t {
            *s -= dt;
            if *s <= 0.0 {
                *t = None;
            }
        }
    }
    if let Ok(t) = player.single() {
        c.sb.set_position(c.player, a3(t.translation + Vec3::Y * 0.9));
    }
    for (e, t) in &enemies {
        c.sb.set_position(e.id, a3(t.translation + Vec3::Y * 0.9 * e.scale));
    }
    if !c.over {
        c.sb.tick(dt);
    }
    frame.0 = c.sb.events();
    if !c.over && !c.player_alive() {
        c.over = true;
        c.banner = Some(("GUARDIAN DOWN".into(), f32::MAX));
    }
}

/// Orb and ammo pickups, and orb visuals.
#[allow(clippy::too_many_arguments)]
fn pickups(
    mut commands: Commands,
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    player: Query<&Transform, With<Player>>,
    mut bricks: Query<(Entity, &mut AmmoBrick, &mut Transform), Without<Player>>,
    orbs: Query<(Entity, &OrbVisual)>,
    mut fx: ResMut<crate::vfx::FxAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(pt) = player.single() else { return };
    let me = pt.translation;
    let c = &mut *combat;
    if c.over {
        return;
    }
    c.sb.collect_orbs_near(c.player, 1.8);

    // Orb visuals follow the sandbox's orb list.
    let live: HashMap<_, _> = c.sb.orbs().map(|o| (o.id, o.position)).collect();
    let mut shown = Vec::new();
    for (e, o) in &orbs {
        if live.contains_key(&o.0) {
            shown.push(o.0);
        } else {
            commands.entity(e).despawn();
        }
    }
    for (id, pos) in live {
        if shown.contains(&id) {
            continue;
        }
        let Some(p) = pos else { continue };
        let mesh = fx.sphere(&mut meshes);
        commands.spawn((
            OrbVisual(id),
            MatchEntity,
            Mesh3d(mesh),
            MeshMaterial3d(fx.orb.clone()),
            Transform::from_translation(v3(p) + Vec3::Y * 0.2).with_scale(Vec3::splat(0.35)),
            PointLight { color: Color::srgb(1.0, 0.85, 0.4), intensity: 40_000.0, range: 5.0, ..default() },
        ));
    }

    let dt = time.delta_secs();
    for (e, mut b, mut t) in &mut bricks {
        b.life -= dt;
        t.rotate_y(dt * 2.0);
        if t.translation.distance(me) < 1.8 {
            if let Some(g) = c.sb.get_mut(c.player) {
                for w in &mut g.weapons {
                    if w.def.ammo == b.ammo {
                        let (amount, cap) = if b.ammo == AmmoType::Heavy { (2, 12) } else { (4, 30) };
                        w.add_reserves(amount, cap);
                    }
                }
            }
            c.hint(format!("+ {:?} ammo", b.ammo));
            commands.entity(e).despawn();
        } else if b.life <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}
