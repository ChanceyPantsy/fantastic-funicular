//! Browser arena demo for `guardian_combat`.
//!
//! [`game`] is an ordinary Rust game loop. This file exposes it through a
//! plain C ABI (no wasm-bindgen), the same shape a Unity, Unreal or Godot
//! plugin would use:
//!
//! * strings go in as `(ptr, len)` pairs written into memory from [`alloc`];
//! * results come out as JSON in a shared buffer: call the function, then
//!   read [`out_len`] bytes starting at the returned pointer;
//! * a panic stores its message, readable with [`last_error`].

pub mod game;

use std::cell::RefCell;

use game::{Command, Game, Input, LoadoutChoice};
use guardian_combat::ability::AbilitySlot;
use guardian_combat::catalog;
use serde::Serialize;

thread_local! {
    static GAME: RefCell<Option<Game>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn output(bytes: Vec<u8>) -> *const u8 {
    OUT.with(|o| {
        *o.borrow_mut() = bytes;
        o.borrow().as_ptr()
    })
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        ERROR.with(|e| *e.borrow_mut() = info.to_string());
    }));
}

/// Reserves `len` bytes for the caller to write a string into.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Frees memory from [`alloc`].
///
/// # Safety
/// `ptr` must come from `alloc(len)` with the same `len`, and not be used again.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// Length of the last JSON result.
#[no_mangle]
pub extern "C" fn out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// The message of the last panic, as UTF-8 (length from [`out_len`]).
#[no_mangle]
pub extern "C" fn last_error() -> *const u8 {
    output(ERROR.with(|e| e.borrow().clone().into_bytes()))
}

#[derive(Serialize)]
struct OptionOut<'a> {
    name: &'a str,
    description: &'a str,
    el: guardian_combat::element::DamageType,
}

#[derive(Serialize)]
struct KitOut<'a> {
    class: guardian_combat::element::GuardianClass,
    element: guardian_combat::element::SubclassElement,
    supers: Vec<OptionOut<'a>>,
    grenades: Vec<OptionOut<'a>>,
    melees: Vec<OptionOut<'a>>,
    class_abilities: Vec<OptionOut<'a>>,
    aspects: Vec<OptionOut<'a>>,
}

/// Every class/subclass kit as JSON, for the loadout screen.
#[no_mangle]
pub extern "C" fn catalog_json() -> *const u8 {
    install_panic_hook();
    let kits = catalog::all_kits();
    let out: Vec<KitOut> = kits
        .iter()
        .map(|k| {
            let opts = |slot| {
                k.options(slot)
                    .iter()
                    .map(|a| OptionOut { name: &a.name, description: &a.description, el: a.damage_type })
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
                    .map(|a| OptionOut { name: &a.name, description: &a.description, el: a.damage_type })
                    .collect(),
            }
        })
        .collect();
    output(serde_json::to_vec(&out).expect("catalog serializes"))
}

/// Starts a game from a loadout JSON string (see [`LoadoutChoice`]).
/// Returns 1 on success, 0 on failure (message via [`last_error`]).
///
/// # Safety
/// `ptr..ptr+len` must be a readable UTF-8 buffer.
#[no_mangle]
pub unsafe extern "C" fn game_new(ptr: *const u8, len: usize) -> u32 {
    install_panic_hook();
    let bytes = std::slice::from_raw_parts(ptr, len);
    let result = serde_json::from_slice::<LoadoutChoice>(bytes).map_err(|e| e.to_string()).and_then(Game::new);
    match result {
        Ok(g) => {
            GAME.with(|cell| *cell.borrow_mut() = Some(g));
            1
        }
        Err(e) => {
            ERROR.with(|cell| *cell.borrow_mut() = e);
            0
        }
    }
}

fn with_game(f: impl FnOnce(&mut Game)) {
    GAME.with(|cell| {
        if let Some(g) = cell.borrow_mut().as_mut() {
            f(g);
        }
    });
}

/// Movement direction, aim point (arena metres) and fire flags:
/// bit 0 = fire held, bit 1 = auto-aim and auto-fire.
#[no_mangle]
pub extern "C" fn game_input(move_x: f32, move_y: f32, aim_x: f32, aim_y: f32, flags: u32) {
    with_game(|g| g.set_input(Input { move_x, move_y, aim_x, aim_y, fire: flags & 1 != 0, auto: flags & 2 != 0 }));
}

/// 1 grenade, 2 melee, 3 class ability, 4 super, 5 super heavy attack,
/// 6 reload, 7/8/9 switch to weapon 1/2/3.
#[no_mangle]
pub extern "C" fn game_command(code: u32) {
    let cmd = match code {
        1 => Command::Ability(AbilitySlot::Grenade),
        2 => Command::Ability(AbilitySlot::Melee),
        3 => Command::Ability(AbilitySlot::ClassAbility),
        4 => Command::Ability(AbilitySlot::Super),
        5 => Command::HeavyAttack,
        6 => Command::Reload,
        7..=9 => Command::Weapon((code - 7) as usize),
        _ => return,
    };
    with_game(|g| g.command(cmd));
}

#[no_mangle]
pub extern "C" fn game_tick(dt: f32) {
    with_game(|g| g.tick(dt));
}

/// The current frame's state as JSON.
#[no_mangle]
pub extern "C" fn game_state() -> *const u8 {
    let json =
        GAME.with(|cell| cell.borrow().as_ref().map(|g| serde_json::to_vec(&g.state()).expect("state serializes")));
    output(json.unwrap_or_else(|| b"null".to_vec()))
}
