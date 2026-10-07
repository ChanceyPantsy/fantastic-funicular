//! The HUD, laid out like Destiny's: radar top-left, health top-centre,
//! abilities bottom-left, super bottom-centre, weapons bottom-right.

use std::collections::BTreeMap;

use bevy::prelude::*;
use guardian_combat::ability::AbilitySlot;
use guardian_combat::combatant::{CombatantId, Rank};
use guardian_combat::element::DamageType;
use guardian_combat::sandbox::CombatEvent;
use guardian_combat::weapon::AmmoType;

use crate::actions::Hitmarker;
use crate::combat::Combat;
use crate::common::{
    element_color, subclass_color, v3, AppState, Fonts, FrameEvents, MatchEntity, PendingLoadout, Phase,
};
use crate::enemies::Enemy;
use crate::player::{Grab, Player, PlayerCamera};

const RADAR: f32 = 150.0;
const RADAR_RANGE: f32 = 35.0;
const DOTS: usize = 40;
const NUMBERS: usize = 40;

#[derive(Resource)]
struct Hud {
    health: Entity,
    shield: Entity,
    overshield: Entity,
    boss: Entity,
    boss_name: Entity,
    boss_fill: Entity,
    boss_shield: Entity,
    wave: Entity,
    abilities: [(Entity, Entity, Entity, Entity); 3],
    super_fill: Entity,
    super_text: Entity,
    weapons: [(Entity, Entity, Entity); 3],
    statuses: Entity,
    dots: Vec<Entity>,
    numbers: Vec<Entity>,
    crosshair: Entity,
    marker: Entity,
    hint: Entity,
    banner: Entity,
    vignette: Entity,
    overlay: Entity,
    overlay_text: Entity,
}

struct Number {
    at: Vec3,
    age: f32,
    text: String,
    color: Color,
    size: f32,
}

#[derive(Resource, Default)]
struct Numbers(Vec<Number>);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Numbers>()
            .add_systems(
                Update,
                build_hud.after(crate::combat::begin_match).run_if(resource_exists::<crate::combat::NewMatch>),
            )
            .add_systems(
                Update,
                (update_hud, damage_numbers, restart_keys)
                    .in_set(Phase::React)
                    .run_if(in_state(AppState::Playing).and_then(resource_exists::<Hud>)),
            );
    }
}

fn text(font: &Handle<Font>, s: &str, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, px),
        right: right.map_or(Val::Auto, px),
        top: top.map_or(Val::Auto, px),
        bottom: bottom.map_or(Val::Auto, px),
        ..default()
    }
}

fn bar(commands: &mut Commands, parent: Entity, w: f32, h: f32, color: Color) -> Entity {
    let track = commands
        .spawn((Node { width: px(w), height: px(h), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1))))
        .id();
    let fill = commands
        .spawn((Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(color)))
        .id();
    commands.entity(track).add_child(fill);
    commands.entity(parent).add_child(track);
    fill
}

#[allow(clippy::too_many_lines)]
fn build_hud(
    mut commands: Commands,
    fonts: Res<Fonts>,
    combat: Res<Combat>,
    camera: Option<Single<Entity, With<PlayerCamera>>>,
) {
    let Some(camera) = camera else { return };
    let camera = *camera;
    commands.remove_resource::<crate::combat::NewMatch>();
    let f = &fonts.hud;
    let white = Color::srgb(0.93, 0.92, 0.88);
    let dim = Color::srgba(0.93, 0.92, 0.88, 0.6);
    let root = commands
        .spawn((
            MatchEntity,
            UiTargetCamera(camera),
            Node { width: percent(100.0), height: percent(100.0), position_type: PositionType::Absolute, ..default() },
            GlobalZIndex(10),
        ))
        .id();

    // Damage vignette.
    let vignette = commands
        .spawn((
            Node { width: percent(100.0), height: percent(100.0), position_type: PositionType::Absolute, ..default() },
            vignette_gradient(0.0),
        ))
        .id();
    commands.entity(root).add_child(vignette);

    // Radar.
    let radar = commands
        .spawn((
            Node {
                width: px(RADAR),
                height: px(RADAR),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(percent(50.0)),
                position_type: PositionType::Absolute,
                left: px(24.0),
                top: px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.08, 0.12, 0.55)),
            BorderColor::all(Color::srgba(0.85, 0.75, 0.45, 0.6)),
        ))
        .id();
    commands.entity(root).add_child(radar);
    let me_dot = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(RADAR / 2.0 - 5.0),
                top: px(RADAR / 2.0 - 5.0),
                width: px(8.0),
                height: px(8.0),
                border_radius: BorderRadius::all(px(2.0)),
                ..default()
            },
            BackgroundColor(white),
        ))
        .id();
    commands.entity(radar).add_child(me_dot);
    let dots: Vec<Entity> = (0..DOTS)
        .map(|_| {
            let d = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(7.0),
                        height: px(7.0),
                        border_radius: BorderRadius::all(percent(50.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.95, 0.25, 0.2)),
                    Visibility::Hidden,
                ))
                .id();
            commands.entity(radar).add_child(d);
            d
        })
        .collect();
    let wave = commands.spawn((abs(Some(24.0), None, Some(RADAR + 30.0), None), text(f, "WAVE 1", 18.0, white))).id();
    commands.entity(root).add_child(wave);

    // Health (top centre).
    let top = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(22.0),
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(top);
    let overshield = bar(&mut commands, top, 300.0, 4.0, Color::srgb(0.75, 0.85, 1.0));
    let shield = bar(&mut commands, top, 300.0, 7.0, Color::srgb(0.92, 0.9, 0.82));
    let health = bar(&mut commands, top, 300.0, 7.0, Color::srgb(0.85, 0.85, 0.85));
    let boss = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(3.0),
            margin: UiRect::top(px(10.0)),
            ..default()
        })
        .id();
    commands.entity(top).add_child(boss);
    let boss_name = commands.spawn(text(f, "", 15.0, Color::srgb(0.95, 0.8, 0.4))).id();
    commands.entity(boss).add_child(boss_name);
    let boss_shield = bar(&mut commands, boss, 520.0, 5.0, Color::WHITE);
    let boss_fill = bar(&mut commands, boss, 520.0, 9.0, Color::srgb(0.9, 0.75, 0.25));

    // Abilities (bottom left).
    let abil_row = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: px(24.0),
            bottom: px(24.0),
            column_gap: px(10.0),
            align_items: AlignItems::FlexEnd,
            ..default()
        })
        .id();
    commands.entity(root).add_child(abil_row);
    let statuses = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: px(24.0),
            bottom: px(118.0),
            column_gap: px(6.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(statuses);
    let mut abilities = Vec::new();
    for (slot, key) in [(AbilitySlot::Grenade, "Q"), (AbilitySlot::Melee, "E"), (AbilitySlot::ClassAbility, "C")] {
        let el = combat
            .sb
            .get(combat.player)
            .and_then(|c| c.loadout.get(slot))
            .map_or(DamageType::Kinetic, |a| a.def.damage_type);
        let c = if el == DamageType::Kinetic { subclass_color(combat.choice.element) } else { element_color(el) };
        let col = commands
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                ..default()
            })
            .id();
        let boxe = commands
            .spawn((
                Node {
                    width: px(58.0),
                    height: px(58.0),
                    border: UiRect::all(px(2.0)),
                    border_radius: BorderRadius::all(px(6.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.1, 0.7)),
                BorderColor::all(c.with_alpha(0.5)),
            ))
            .id();
        let fill = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(0.0),
                    left: px(0.0),
                    width: percent(100.0),
                    height: percent(0.0),
                    ..default()
                },
                BackgroundColor(c.with_alpha(0.55)),
            ))
            .id();
        let label = commands.spawn(text(f, key, 20.0, white)).id();
        let name = commands.spawn((text(f, "", 11.0, dim), Node { max_width: px(84.0), ..default() })).id();
        let pips = commands.spawn(text(f, "", 11.0, c)).id();
        commands.entity(boxe).add_children(&[fill, label]);
        commands.entity(col).add_children(&[pips, boxe, name]);
        commands.entity(abil_row).add_child(col);
        abilities.push((fill, boxe, name, pips));
    }

    // Super (bottom centre).
    let sup = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(26.0),
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(5.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(sup);
    let super_text = commands.spawn(text(f, "", 14.0, white)).id();
    commands.entity(sup).add_child(super_text);
    let super_fill = bar(&mut commands, sup, 380.0, 8.0, subclass_color(combat.choice.element));

    // Weapons (bottom right).
    let wcol = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: px(24.0),
            bottom: px(24.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: px(6.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(wcol);
    let mut weapons = Vec::new();
    let names = combat
        .sb
        .get(combat.player)
        .map(|c| c.weapons.iter().map(|w| (w.def.name.clone(), w.def.ammo, w.def.anti_champion)).collect::<Vec<_>>())
        .unwrap_or_default();
    for (i, (name, ammo, anti)) in names.into_iter().enumerate().take(3) {
        let ammo_c = match ammo {
            AmmoType::Primary => white,
            AmmoType::Special => Color::srgb(0.4, 1.0, 0.55),
            AmmoType::Heavy => Color::srgb(0.75, 0.5, 1.0),
        };
        let row = commands
            .spawn((
                Node {
                    padding: UiRect::axes(px(12.0), px(6.0)),
                    column_gap: px(14.0),
                    align_items: AlignItems::Center,
                    border: UiRect::left(px(3.0)),
                    min_width: px(250.0),
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.1, 0.55)),
                BorderColor::all(ammo_c),
            ))
            .id();
        let label = format!("{}  {}{}", i + 1, name, anti.map_or(String::new(), |a| format!("  ·  anti-{a:?}")));
        let n = commands.spawn(text(f, &label, 14.0, white)).id();
        let count = commands.spawn(text(f, "", 18.0, ammo_c)).id();
        commands.entity(row).add_children(&[n, count]);
        commands.entity(wcol).add_child(row);
        weapons.push((row, n, count));
    }

    // Crosshair, hit marker, hints, banner.
    let crosshair = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50.0),
                top: percent(50.0),
                margin: UiRect { left: px(-11.0), top: px(-13.0), ..default() },
                ..default()
            },
            text(f, "+", 26.0, Color::srgba(1.0, 1.0, 1.0, 0.85)),
        ))
        .id();
    let marker = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50.0),
                top: percent(50.0),
                margin: UiRect { left: px(-14.0), top: px(-20.0), ..default() },
                ..default()
            },
            text(f, "×", 34.0, Color::NONE),
        ))
        .id();
    let hint = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: percent(64.0),
                width: percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            text(f, "", 18.0, white),
            TextLayout::justify(Justify::Center),
        ))
        .id();
    let banner = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: percent(26.0),
                width: percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            text(f, "", 44.0, Color::srgb(0.95, 0.8, 0.4)),
            TextLayout::justify(Justify::Center),
        ))
        .id();
    commands.entity(root).add_children(&[crosshair, marker, hint, banner]);

    let numbers = (0..NUMBERS)
        .map(|_| {
            let e = commands
                .spawn((
                    Node { position_type: PositionType::Absolute, ..default() },
                    text(f, "", 16.0, white),
                    Visibility::Hidden,
                ))
                .id();
            commands.entity(root).add_child(e);
            e
        })
        .collect();

    // Click-to-play / death overlay.
    let overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.55)),
        ))
        .id();
    let overlay_text = commands.spawn((text(f, "", 22.0, white), TextLayout::justify(Justify::Center))).id();
    commands.entity(overlay).add_child(overlay_text);
    commands.entity(root).add_child(overlay);

    let (h, s, o) = (health, shield, overshield);
    let abilities = [abilities[0], abilities[1], abilities[2]];
    let weapons: [(Entity, Entity, Entity); 3] = [weapons[0], weapons[1], weapons[2]];
    commands.insert_resource(Hud {
        health: h,
        shield: s,
        overshield: o,
        boss,
        boss_name,
        boss_fill,
        boss_shield,
        wave,
        abilities,
        super_fill,
        super_text,
        weapons,
        statuses,
        dots,
        numbers,
        crosshair,
        marker,
        hint,
        banner,
        vignette,
        overlay,
        overlay_text,
    });
}

/// A red edge that fades to clear in the middle of the screen.
fn vignette_gradient(alpha: f32) -> BackgroundGradient {
    RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::FarthestCorner,
        vec![
            ColorStop::new(Color::NONE, percent(45.0)),
            ColorStop::new(Color::srgba(0.7, 0.02, 0.02, alpha), percent(100.0)),
        ],
    )
    .into()
}

fn status_color(name: &str) -> Color {
    let el = match name {
        "Scorch" | "Radiant" | "Restoration" => DamageType::Solar,
        "Jolt" | "Blind" | "Amplified" => DamageType::Arc,
        "Volatile" | "Weaken" | "Suppress" | "Devour" | "Invisible" => DamageType::Void,
        "Slow" | "Frozen" | "FrostArmor" => DamageType::Stasis,
        "Sever" | "Suspend" | "Unravel" | "WovenMail" => DamageType::Strand,
        _ => DamageType::Kinetic,
    };
    element_color(el)
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    mut commands: Commands,
    hud: Res<Hud>,
    combat: Res<Combat>,
    grab: Res<Grab>,
    marker: Res<Hitmarker>,
    fonts: Res<Fonts>,
    time: Res<Time>,
    player: Single<(&Player, &Transform)>,
    enemies: Query<(&Enemy, &Transform)>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut colors: Query<&mut TextColor>,
    mut vis: Query<&mut Visibility>,
    mut borders: Query<&mut BorderColor, Without<Text>>,
    mut bgs: Query<&mut BackgroundColor>,
    children: Query<&Children>,
) {
    let Some(me) = combat.sb.get(combat.player) else { return };
    let (p, pt) = player.into_inner();
    let set_w = |nodes: &mut Query<&mut Node>, e: Entity, f: f32| {
        if let Ok(mut n) = nodes.get_mut(e) {
            n.width = percent((f * 100.0).clamp(0.0, 100.0));
        }
    };
    let set_text = |texts: &mut Query<&mut Text>, e: Entity, s: &str| {
        if let Ok(mut t) = texts.get_mut(e) {
            if t.0 != s {
                t.0 = s.to_string();
            }
        }
    };

    let h = &me.health;
    set_w(&mut nodes, hud.health, h.health / h.max_health.max(1.0));
    set_w(&mut nodes, hud.shield, h.shield / h.max_shield.max(1.0));
    set_w(&mut nodes, hud.overshield, h.overshield / 100.0);
    if let Ok(mut bg) = bgs.get_mut(hud.health) {
        bg.0 = if h.health < h.max_health * 0.4 { Color::srgb(0.9, 0.2, 0.15) } else { Color::srgb(0.85, 0.85, 0.85) };
    }
    let low = (1.0 - h.total() / h.max_total().max(1.0)).max(0.0);
    {
        let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 6.0).sin();
        let a = (p.hurt * 0.55 + if h.health < h.max_health * 0.5 { 0.3 * pulse * low } else { 0.0 }).min(0.7);
        commands.entity(hud.vignette).insert(vignette_gradient(a));
    }
    set_text(&mut texts, hud.wave, &format!("WAVE {}   ·   {} KILLS", combat.wave.max(1), combat.kills));

    // Boss / champion bar: the toughest living major.
    let boss = enemies
        .iter()
        .filter(|(e, _)| e.targetable())
        .filter_map(|(e, _)| combat.sb.get(e.id).map(|c| (e, c)))
        .filter(|(_, c)| c.champion.is_some() || matches!(c.rank, Rank::Boss))
        .max_by(|a, b| a.1.health.max_health.total_cmp(&b.1.health.max_health));
    if let Ok(mut v) = vis.get_mut(hud.boss) {
        *v = if boss.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Some((e, c)) = boss {
        let mut label = e.name.to_uppercase();
        if let Some(ch) = &c.champion {
            if ch.barrier_up {
                label.push_str("  ·  BARRIER");
            } else if ch.is_stunned() {
                label.push_str("  ·  STUNNED");
            }
        }
        set_text(&mut texts, hud.boss_name, &label);
        set_w(&mut nodes, hud.boss_fill, c.health.health / c.health.max_health.max(1.0));
        set_w(
            &mut nodes,
            hud.boss_shield,
            if c.health.max_shield > 0.0 { c.health.shield / c.health.max_shield } else { 0.0 },
        );
        if let (Ok(mut bg), Some(el)) = (bgs.get_mut(hud.boss_shield), c.health.shield_element) {
            bg.0 = element_color(el);
        }
    }

    // Abilities.
    for (i, slot) in [AbilitySlot::Grenade, AbilitySlot::Melee, AbilitySlot::ClassAbility].into_iter().enumerate() {
        let (fill, boxe, name, pips) = hud.abilities[i];
        let Some(a) = me.loadout.get(slot) else { continue };
        let ready = a.charges > 0;
        if let Ok(mut n) = nodes.get_mut(fill) {
            n.height = percent(if ready { 100.0 } else { a.energy * 100.0 });
        }
        if let Ok(mut b) = borders.get_mut(boxe) {
            let c = element_color(a.def.damage_type);
            *b = BorderColor::all(if ready { Color::WHITE } else { c.with_alpha(0.4) });
        }
        set_text(&mut texts, name, &a.def.name);
        let pip = if a.def.max_charges > 1 {
            "●".repeat(a.charges as usize) + &"○".repeat((a.def.max_charges - a.charges) as usize)
        } else {
            String::new()
        };
        set_text(&mut texts, pips, &pip);
    }

    // Super.
    if let Some(s) = me.loadout.get(AbilitySlot::Super) {
        match &me.super_mode {
            Some(m) => {
                set_w(&mut nodes, hud.super_fill, m.remaining / m.def.duration.max(0.1));
                let uses = m.light_uses_left.map_or(String::new(), |n| format!(" ×{n}"));
                let heavy =
                    m.def.heavy.as_ref().map_or(String::new(), |h| format!("   ·   RMB  {}", h.name.to_uppercase()));
                set_text(&mut texts, hud.super_text, &format!("LMB  {}{uses}{heavy}", m.def.light.name.to_uppercase()));
            }
            None => {
                let ready = s.charges > 0;
                set_w(&mut nodes, hud.super_fill, if ready { 1.0 } else { s.energy });
                let label = if ready {
                    format!("{}   ·   PRESS F", s.def.name.to_uppercase())
                } else {
                    format!("{}   {}%", s.def.name.to_uppercase(), (s.energy * 100.0) as u32)
                };
                set_text(&mut texts, hud.super_text, &label);
                if let Ok(mut c) = colors.get_mut(hud.super_text) {
                    let blink = ready && (time.elapsed_secs() * 3.0).sin() > 0.0;
                    c.0 = if blink { Color::srgb(1.0, 0.85, 0.4) } else { Color::srgb(0.93, 0.92, 0.88) };
                }
            }
        }
    }

    // Weapons.
    for (i, (row, _, count)) in hud.weapons.iter().enumerate() {
        let Some(w) = me.weapons.get(i) else { continue };
        let active = i == me.active_weapon;
        if let Ok(mut bg) = bgs.get_mut(*row) {
            bg.0 = if active { Color::srgba(0.18, 0.2, 0.26, 0.85) } else { Color::srgba(0.05, 0.07, 0.1, 0.5) };
        }
        let s = if w.is_reloading() {
            "RELOADING".to_string()
        } else {
            format!("{}  /  {}", w.magazine, w.reserves.map_or("∞".to_string(), |r| r.to_string()))
        };
        set_text(&mut texts, *count, &s);
    }

    // Player statuses and buffs.
    let mut tags: Vec<String> = me.statuses.iter().map(|s| format!("{:?}", s.kind)).collect();
    tags.extend(
        me.modifiers.iter().filter(|m| m.remaining.is_some() && !m.name.contains("Super")).map(|m| m.name.clone()),
    );
    let have: Vec<String> = children
        .get(hud.statuses)
        .map(|c| c.iter().filter_map(|e| texts.get(e).ok().map(|t| t.0.clone())).collect())
        .unwrap_or_default();
    if have != tags {
        commands.entity(hud.statuses).despawn_children();
        for tag in &tags {
            let c = status_color(tag);
            let e = commands
                .spawn((
                    Node {
                        padding: UiRect::axes(px(7.0), px(3.0)),
                        border_radius: BorderRadius::all(px(3.0)),
                        ..default()
                    },
                    BackgroundColor(c.with_alpha(0.22)),
                    text(&fonts.hud, tag, 12.0, c),
                ))
                .id();
            commands.entity(hud.statuses).add_child(e);
        }
    }

    // Radar: enemies relative to where you face.
    let me_pos = pt.translation;
    let inv = Quat::from_rotation_y(-p.yaw);
    let mut i = 0;
    for (e, t) in &enemies {
        if i >= hud.dots.len() || !e.targetable() {
            continue;
        }
        let rel = inv * (t.translation - me_pos);
        let d = Vec2::new(rel.x, rel.z);
        if d.length() > RADAR_RANGE {
            continue;
        }
        let pos = d / RADAR_RANGE * (RADAR / 2.0 - 8.0) + Vec2::splat(RADAR / 2.0 - 4.5);
        if let Ok(mut n) = nodes.get_mut(hud.dots[i]) {
            n.left = px(pos.x);
            n.top = px(pos.y);
            let s = if e.scale > 1.2 { 11.0 } else { 7.0 };
            n.width = px(s);
            n.height = px(s);
        }
        if let Ok(mut v) = vis.get_mut(hud.dots[i]) {
            *v = Visibility::Inherited;
        }
        i += 1;
    }
    for &d in &hud.dots[i..] {
        if let Ok(mut v) = vis.get_mut(d) {
            *v = Visibility::Hidden;
        }
    }

    // Crosshair and hit marker.
    if let Ok(mut v) = vis.get_mut(hud.crosshair) {
        *v = if p.third > 0.5 || p.ads > 0.8 { Visibility::Hidden } else { Visibility::Inherited };
    }
    if let Ok(mut c) = colors.get_mut(hud.marker) {
        c.0 = if marker.kill > 0.0 {
            Color::srgba(1.0, 0.3, 0.25, marker.kill / 0.35)
        } else if marker.t > 0.0 {
            if marker.crit {
                Color::srgba(1.0, 0.85, 0.3, marker.t / 0.15)
            } else {
                Color::srgba(1.0, 1.0, 1.0, marker.t / 0.15)
            }
        } else {
            Color::NONE
        };
    }
    set_text(&mut texts, hud.hint, combat.hint.as_ref().map_or("", |h| h.0.as_str()));
    set_text(&mut texts, hud.banner, combat.banner.as_ref().map_or("", |h| h.0.as_str()));

    // Overlays.
    let overlay = if combat.over {
        Some(format!(
            "\n\n\nWave {}  ·  {} kills\n\nEnter  try again        L  change loadout",
            combat.wave, combat.kills
        ))
    } else if !grab.0 {
        Some(
            "CLICK TO FIGHT\n\nWASD move  ·  Shift sprint  ·  Space jump (hold for class jump)\nMouse aim  ·  Left click fire  ·  Right click aim down sights\nQ grenade  ·  E melee  ·  C class ability  ·  F super\n1 2 3 weapons  ·  R reload  ·  Esc pause  ·  L loadout"
                .to_string(),
        )
    } else {
        None
    };
    if let Ok(mut v) = vis.get_mut(hud.overlay) {
        *v = if overlay.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Some(s) = overlay {
        set_text(&mut texts, hud.overlay_text, &s);
    }
}

/// Floating damage numbers above what you hit.
#[allow(clippy::too_many_arguments)]
fn damage_numbers(
    time: Res<Time>,
    hud: Res<Hud>,
    frame: Res<FrameEvents>,
    combat: Res<Combat>,
    mut numbers: ResMut<Numbers>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<(&mut Text, &mut TextColor, &mut TextFont)>,
    mut vis: Query<&mut Visibility>,
) {
    let dt = time.delta_secs();
    let mut dealt: BTreeMap<CombatantId, (f32, bool, DamageType)> = BTreeMap::new();
    for e in &frame.0 {
        if let CombatEvent::Damaged { target, attacker, absorbed, precision, damage_type, kind, .. } = e {
            if *attacker != Some(combat.player) || *target == combat.player {
                continue;
            }
            let entry = dealt.entry(*target).or_insert((
                0.0,
                false,
                if kind.is_weapon() { DamageType::Kinetic } else { *damage_type },
            ));
            entry.0 += absorbed.total();
            entry.1 |= *precision;
        }
    }
    for (id, (amount, crit, el)) in dealt {
        if amount < 0.5 {
            continue;
        }
        let Some(p) = combat.sb.get(id).and_then(|c| c.position) else { continue };
        let jitter = Vec3::new(((numbers.0.len() * 37) % 10) as f32 * 0.06 - 0.3, 0.9, 0.0);
        let color = if crit {
            Color::srgb(1.0, 0.85, 0.3)
        } else if el == DamageType::Kinetic {
            Color::srgb(0.95, 0.95, 0.92)
        } else {
            element_color(el)
        };
        numbers.0.push(Number {
            at: v3(p) + jitter,
            age: 0.0,
            text: format!("{}", amount.round() as i64),
            color,
            size: if crit { 22.0 } else { 17.0 },
        });
    }
    for n in &mut numbers.0 {
        n.age += dt;
        n.at.y += dt * 0.8;
    }
    numbers.0.retain(|n| n.age < 0.9);
    while numbers.0.len() > NUMBERS {
        numbers.0.remove(0);
    }
    let Ok((cam, cam_t)) = camera.single() else { return };
    for (i, &slot) in hud.numbers.iter().enumerate() {
        let shown = numbers.0.get(i).and_then(|n| cam.world_to_viewport(cam_t, n.at).ok().map(|p| (n, p)));
        if let Ok(mut v) = vis.get_mut(slot) {
            *v = if shown.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        }
        let Some((n, p)) = shown else { continue };
        if let Ok(mut node) = nodes.get_mut(slot) {
            node.left = px(p.x - 12.0);
            node.top = px(p.y - 10.0);
        }
        if let Ok((mut t, mut c, mut f)) = texts.get_mut(slot) {
            if t.0 != n.text {
                t.0 = n.text.clone();
            }
            c.0 = n.color.with_alpha((1.0 - n.age / 0.9) * 1.2);
            f.font_size = FontSize::Px(n.size);
        }
    }
}

fn restart_keys(keys: Res<ButtonInput<KeyCode>>, combat: Res<Combat>, mut pending: ResMut<PendingLoadout>) {
    if combat.over && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)) {
        pending.0 = Some(combat.choice.clone());
    }
    if keys.just_pressed(KeyCode::KeyL) {
        crate::web::show_loadout();
    }
}
