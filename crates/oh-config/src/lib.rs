//! `oh-config` — port of SSVOpenHexagon's `Global/Config`: every config key
//! (JSON name/type/default, mapping §6.2), `loadConfig(overrideIds)` /
//! `saveConfig`, the 28 keyboard/joystick bind values, and the SSVStart
//! Trigger/Combo model behind them.
//!
//! One Rust module per C++ file (plan 001, ground rule 1; D-000/D-002):
//!
//! - `config` — `Global/Config` (all 101 keys, getters/setters, score
//!   eligibility, sizes, keyboard + joystick bind tables);
//! - `triggers` — `ssvs::Input::Trigger` / `Combo` + the SFML
//!   `Keyboard::Key` / `Mouse::Button` enums and name vocabulary
//!   (`.agents/skills/lib-binds.md`);
//! - `joystick` — `Core/Joystick` state machine (controller 0);
//! - `binds` — `Core/BindControl` (menu-independent logic).
//!
//! Fidelity contract: `.agents/plans/001-rust-port/mapping/data-json.md`
//! §6/§8/§9/§10, `.agents/plans/001-rust-port/mapping/oracle-vectors.md`
//! (SFML key/button enum integers), `.agents/plans/001-rust-port/04-config.md`.

pub mod binds;
pub mod config;
pub mod joystick;
pub mod triggers;

pub use binds::{JoystickBindControl, KeyboardBindControl};
pub use config::{
    bind_to_human_readable_name, clear_trigger_bind, default_server_level_whitelist,
    rebind_trigger, resize_trigger, Config, Tid, MAX_BINDS, TICKS_PER_SECOND, TIME_SLICE,
    TIME_STEP,
};
pub use joystick::{Jdir, Jid, JoystickPoll};
pub use triggers::{Button, Combo, Key, Trigger};
