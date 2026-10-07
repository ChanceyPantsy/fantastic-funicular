//! Talking to the web page: it hands us a loadout, and we ask it to show
//! its loadout picker. On native builds a loadout comes from the command line.

use bevy::prelude::*;

use crate::common::{AppState, LoadoutChoice, PendingLoadout};

pub struct WebPlugin;

impl Plugin for WebPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), on_menu)
            .add_systems(Update, (poll, debug_snapshot, debug_aim, debug_charge, debug_wave));
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::sync::Mutex;

    use wasm_bindgen::prelude::*;

    pub static PENDING: Mutex<Option<String>> = Mutex::new(None);
    pub static PAUSED: Mutex<bool> = Mutex::new(false);
    pub static LOCKED: Mutex<bool> = Mutex::new(false);

    /// The page reports pointer-lock changes here.
    #[wasm_bindgen]
    pub fn set_pointer_locked(locked: bool) {
        *LOCKED.lock().unwrap() = locked;
    }

    /// Pauses the game while the page shows its own UI.
    #[wasm_bindgen]
    pub fn set_paused(paused: bool) {
        *PAUSED.lock().unwrap() = paused;
    }

    pub static DEBUG: Mutex<String> = Mutex::new(String::new());
    pub static AIM: Mutex<bool> = Mutex::new(false);
    pub static CHARGE: Mutex<bool> = Mutex::new(false);
    pub static WAVE: Mutex<u32> = Mutex::new(0);

    /// Test hook: clear the field and start wave `n` now.
    #[wasm_bindgen]
    pub fn debug_wave(n: u32) {
        *WAVE.lock().unwrap() = n;
    }

    /// Test hook: fill every ability and the super.
    #[wasm_bindgen]
    pub fn debug_charge() {
        *CHARGE.lock().unwrap() = true;
    }

    /// Test hook: turn the player to face the nearest enemy's head.
    #[wasm_bindgen]
    pub fn debug_aim() {
        *AIM.lock().unwrap() = true;
    }

    /// A snapshot of the match for automated tests.
    #[wasm_bindgen]
    pub fn debug_json() -> String {
        DEBUG.lock().unwrap().clone()
    }

    /// Every class/subclass kit as JSON, for the loadout picker.
    #[wasm_bindgen]
    pub fn catalog_json() -> String {
        super::catalog_json()
    }

    /// Called by the page with a loadout as JSON.
    #[wasm_bindgen]
    pub fn start_match(json: String) {
        *PENDING.lock().unwrap() = Some(json);
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = window, js_name = guardianReady)]
        pub fn ready();
        #[wasm_bindgen(js_namespace = window, js_name = guardianShowLoadout)]
        pub fn show_loadout();
        #[wasm_bindgen(js_namespace = window, js_name = guardianError)]
        pub fn error(msg: &str);
        #[wasm_bindgen(js_namespace = window, js_name = guardianUnlock)]
        pub fn unlock();
    }
}

fn on_menu(mut pending: ResMut<PendingLoadout>) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = &mut pending;
        imp::ready();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // `guardian_strike [class] [subclass]`, e.g. `guardian_strike titan arc`.
        use guardian_combat::element::{GuardianClass, SubclassElement};
        let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
        let class = match args.first().map(String::as_str) {
            Some("titan") => GuardianClass::Titan,
            Some("hunter") => GuardianClass::Hunter,
            _ => GuardianClass::Warlock,
        };
        let element = match args.get(1).map(String::as_str) {
            Some("arc") => SubclassElement::Arc,
            Some("void") => SubclassElement::Void,
            Some("stasis") => SubclassElement::Stasis,
            Some("strand") => SubclassElement::Strand,
            Some("prismatic") => SubclassElement::Prismatic,
            _ => SubclassElement::Solar,
        };
        pending.0 = Some(LoadoutChoice::default_for(class, element));
    }
}

fn poll(mut pending: ResMut<PendingLoadout>) {
    #[cfg(target_arch = "wasm32")]
    if let Some(json) = imp::PENDING.lock().unwrap().take() {
        match serde_json::from_str::<LoadoutChoice>(&json) {
            Ok(choice) => pending.0 = Some(choice),
            Err(e) => report_error(&e.to_string()),
        }
    }
    let _ = &mut pending;
}

#[derive(serde::Serialize)]
struct OptionOut {
    name: String,
    description: String,
    el: guardian_combat::element::DamageType,
}

#[derive(serde::Serialize)]
struct KitOut {
    class: guardian_combat::element::GuardianClass,
    element: guardian_combat::element::SubclassElement,
    supers: Vec<OptionOut>,
    grenades: Vec<OptionOut>,
    melees: Vec<OptionOut>,
    class_abilities: Vec<OptionOut>,
    aspects: Vec<OptionOut>,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn catalog_json() -> String {
    use guardian_combat::ability::AbilitySlot;
    let kits: Vec<KitOut> = guardian_combat::catalog::all_kits()
        .iter()
        .map(|k| {
            let opts = |slot| {
                k.options(slot)
                    .iter()
                    .map(|a| OptionOut { name: a.name.clone(), description: a.description.clone(), el: a.damage_type })
                    .collect()
            };
            KitOut {
                class: k.class,
                element: k.element,
                supers: opts(AbilitySlot::Super),
                grenades: opts(AbilitySlot::Grenade),
                melees: opts(AbilitySlot::Melee),
                class_abilities: opts(AbilitySlot::ClassAbility),
                aspects: k
                    .aspects
                    .iter()
                    .map(|a| OptionOut { name: a.name.clone(), description: a.description.clone(), el: a.damage_type })
                    .collect(),
            }
        })
        .collect();
    serde_json::to_string(&kits).unwrap_or_default()
}

/// Whether the page has paused the game.
pub fn page_paused() -> bool {
    #[cfg(target_arch = "wasm32")]
    return *imp::PAUSED.lock().unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    false
}

/// On the web, whether the page holds pointer lock (the page manages it).
pub fn pointer_locked() -> Option<bool> {
    #[cfg(target_arch = "wasm32")]
    return Some(*imp::LOCKED.lock().unwrap());
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// Asks the page to release the mouse.
pub fn unlock() {
    #[cfg(target_arch = "wasm32")]
    imp::unlock();
}

fn debug_snapshot(
    combat: Option<Res<crate::combat::Combat>>,
    enemies: Query<(&crate::enemies::Enemy, &Transform)>,
    player: Query<(&crate::player::Player, &Transform), Without<crate::enemies::Enemy>>,
    bolts: Query<(), With<crate::enemies::EnemyBolt>>,
) {
    let Some(c) = combat else { return };
    let me = c.sb.get(c.player);
    let snapshot = serde_json::json!({
        "wave": c.wave,
        "time": c.time,
        "kills": c.kills,
        "over": c.over,
        "enemies": enemies.iter().filter(|(e, _)| e.targetable()).count(),
        "ai": enemies.iter().map(|(e, t)| e.debug(t, player.iter().next().map_or(Vec3::ZERO, |p| p.1.translation))).collect::<Vec<_>>(),
        "bolts": bolts.iter().count(),
        "casts": c.cast_log.iter().cloned().collect::<Vec<_>>(),
        "health": me.map(|m| m.health.health),
        "shield": me.map(|m| m.health.shield),
        "in_super": me.is_some_and(|m| m.in_super()),
        "super": me.and_then(|m| m.loadout.super_.as_ref()).map(|s| if s.charges > 0 { 1.0 } else { s.energy }),
        "zones": c.sb.zones().map(|z| z.name.clone()).collect::<Vec<_>>(),
        "third": player.iter().next().map(|p| p.0.third),
        "pos": player.iter().next().map(|p| p.1.translation.to_array()),
        "look": player.iter().next().map(|p| [p.0.yaw, p.0.pitch, p.0.recoil]),
        "forced": player.iter().next().map(|p| p.0.forced.is_some()),
        "statuses": me.map(|m| m.statuses.iter().map(|s| format!("{:?}", s.kind)).collect::<Vec<_>>()),
    });
    #[cfg(target_arch = "wasm32")]
    {
        *imp::DEBUG.lock().unwrap() = snapshot.to_string();
    }
    let _ = snapshot;
}

fn debug_aim(
    mut player: Query<(&mut crate::player::Player, &Transform)>,
    enemies: Query<(&crate::enemies::Enemy, &Transform), Without<crate::player::Player>>,
) {
    #[cfg(target_arch = "wasm32")]
    let wanted = std::mem::take(&mut *imp::AIM.lock().unwrap());
    #[cfg(not(target_arch = "wasm32"))]
    let wanted = false;
    if !wanted {
        return;
    }
    let Ok((mut p, t)) = player.single_mut() else { return };
    let eye = t.translation + Vec3::Y * crate::player::EYE;
    let target = enemies
        .iter()
        .filter(|(e, _)| e.targetable())
        .map(|(e, et)| crate::enemies::hitboxes(e, et).2)
        .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
    if let Some(head) = target {
        let d = (head - eye).normalize();
        p.yaw = (-d.x).atan2(-d.z);
        p.pitch = d.y.asin();
    }
}

fn debug_charge(combat: Option<ResMut<crate::combat::Combat>>) {
    #[cfg(target_arch = "wasm32")]
    let wanted = std::mem::take(&mut *imp::CHARGE.lock().unwrap());
    #[cfg(not(target_arch = "wasm32"))]
    let wanted = false;
    let Some(mut c) = combat else { return };
    if !wanted {
        return;
    }
    let pid = c.player;
    if let Some(me) = c.sb.get_mut(pid) {
        for slot in guardian_combat::ability::AbilitySlot::ALL {
            if let Some(a) = me.loadout.get_mut(slot) {
                a.add_energy(10.0);
            }
        }
    }
}

fn debug_wave(
    mut commands: Commands,
    combat: Option<ResMut<crate::combat::Combat>>,
    enemies: Query<(Entity, &crate::enemies::Enemy)>,
) {
    #[cfg(target_arch = "wasm32")]
    let wave = std::mem::take(&mut *imp::WAVE.lock().unwrap());
    #[cfg(not(target_arch = "wasm32"))]
    let wave = 0;
    let Some(mut c) = combat else { return };
    if wave == 0 {
        return;
    }
    for (e, en) in &enemies {
        c.sb.despawn(en.id);
        commands.entity(e).despawn();
    }
    c.enemies.clear();
    c.spawn_queue.clear();
    c.wave = wave - 1;
    c.wave_delay = 0.0;
}

/// Asks the page to show its loadout picker.
pub fn show_loadout() {
    #[cfg(target_arch = "wasm32")]
    imp::show_loadout();
}

pub fn report_error(msg: &str) {
    #[cfg(target_arch = "wasm32")]
    imp::error(msg);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{msg}");
}
