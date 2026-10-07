//! Guardian Strike: a first-person Destiny-style arena built on Bevy and the
//! `guardian_combat` crate. Characters are KayKit's CC0 models.

mod actions;
mod combat;
mod common;
mod enemies;
mod hud;
mod level;
mod models;
mod player;
mod vfx;
mod viewmodel;
mod web;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;

use common::{AppState, Fonts, Phase};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.01, 0.015, 0.03)))
        .insert_resource(GlobalAmbientLight { color: Color::srgb(0.7, 0.75, 1.0), brightness: 350.0, ..default() })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Guardian Strike".into(),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin { meta_check: AssetMetaCheck::Never, ..default() }),
        )
        .init_state::<AppState>()
        .configure_sets(Update, (Phase::Input, Phase::Act, Phase::Sim, Phase::React).chain())
        .add_plugins((
            models::ModelsPlugin,
            combat::CombatPlugin,
            player::PlayerPlugin,
            actions::ActionsPlugin,
            viewmodel::ViewmodelPlugin,
            enemies::EnemyPlugin,
            vfx::VfxPlugin,
            hud::HudPlugin,
            web::WebPlugin,
        ))
        .add_systems(Startup, (level::spawn_level, load_fonts, spawn_menu_camera))
        .add_systems(Update, (menu_camera, pause, ui_scale))
        .run();
}

/// A slow fly-around of the arena behind the loadout picker.
#[derive(Component)]
struct MenuCamera;

fn spawn_menu_camera(mut commands: Commands) {
    commands.spawn((
        MenuCamera,
        Camera3d::default(),
        bevy::camera::Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::post_process::bloom::Bloom::NATURAL,
        bevy::pbr::DistanceFog {
            color: Color::srgb(0.03, 0.04, 0.07),
            falloff: bevy::pbr::FogFalloff::Linear { start: 35.0, end: 170.0 },
            ..default()
        },
        Transform::from_xyz(0.0, 14.0, 40.0).looking_at(Vec3::new(0.0, 3.0, 0.0), Vec3::Y),
    ));
}

fn menu_camera(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    mut cams: Query<(&mut Camera, &mut Transform), With<MenuCamera>>,
) {
    let menu = *state.get() != AppState::Playing;
    for (mut cam, mut t) in &mut cams {
        cam.is_active = menu;
        if menu {
            let a = time.elapsed_secs() * 0.08;
            *t =
                Transform::from_xyz(a.sin() * 42.0, 13.0, a.cos() * 42.0).looking_at(Vec3::new(0.0, 3.0, 0.0), Vec3::Y);
        }
    }
}

/// Scales the HUD with the window so it fits small and large screens.
fn ui_scale(windows: Query<&Window>, mut scale: ResMut<UiScale>) {
    let Ok(w) = windows.single() else { return };
    let s = (w.height() / 900.0).min(w.width() / 1500.0).clamp(0.45, 1.6);
    if (scale.0 - s).abs() > 0.01 {
        scale.0 = s;
    }
}

/// Freezes the match while the mouse is released or the page has a menu open.
fn pause(
    mut time: ResMut<Time<Virtual>>,
    grab: Res<player::Grab>,
    combat: Option<Res<combat::Combat>>,
    state: Res<State<AppState>>,
) {
    let playing = *state.get() == AppState::Playing;
    let over = combat.is_some_and(|c| c.over);
    let paused = web::page_paused() || (playing && !grab.0 && !over);
    if paused && !time.is_paused() {
        time.pause();
    } else if !paused && time.is_paused() {
        time.unpause();
    }
}

fn load_fonts(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Fonts { hud: assets.load("fonts/ChakraPetch-SemiBold.ttf") });
}
