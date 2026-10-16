//! `oh-menu` — port of `SSVOpenHexagon/Core/MenuGame` (plan 001, issue 013;
//! ground rules D-000/D-002).
//!
//! `MenuGame` is the entire front-end: all ten states, every menu, every
//! item, all transitions, the layout constants and font-sizing math, the
//! cameras, the scrollbar, online status bar, text-entry, pack/level
//! browsing, and `HexagonDialogBox`.
//!
//! Modules (mapping `rendering-menu.md` §11):
//! - [`states`] — §11.1 the `States` enum (+ `PackChange` / `DialogInputState`),
//!   `is_entering_text`
//! - [`seams`] — the `fnHG*` hexagon-game callback seams, the asset/config/
//!   network/audio seam traits (the `fnHGNewGame`/`fnHGTriggerRefresh`/
//!   `fnHGUpdateRichPresenceCallbacks` closures, `MenuAssets`, `MenuConfig`,
//!   the issue-017 `NetworkClient` seam, `AudioSink`)
//! - [`layout`] — §11.11/11.12/11.13 layout constants, font-sizing math,
//!   `refresh_camera`, `calc_menu_offset` / `calc_menu_item_offset`
//! - [`menu`] — §11.6 every menu + every item (verbatim names + callbacks)
//! - [`menu_game`] — §11.2/11.3/11.7/11.8/11.9/11.10 the `MenuGame` state
//!   machine: constructor, `change_state_to`, navigation actions, `update`,
//!   `set_index`, profile editing, dialog-input flow
//! - [`dialog_box`] — §12 `HexagonDialogBox`
//!
//! Fidelity: user-visible strings are verbatim; layout constants and
//! font-sizing math match the C++ (they depend on the current resolution).
//!
//! Online/leaderboard flows depend on issue 017 (`NetworkClient` is a
//! documented seam, no fake network behavior). Per-state pixel rendering is
//! slice 13b (see STATUS); the layout *math* lives in [`layout`] and is
//! unit-tested.

pub mod dialog_box;
pub mod layout;
pub mod menu;
pub mod menu_game;
pub mod real;
pub mod seams;
pub mod states;

pub use menu_game::{LevelDrawer, MenuGame, Shared};
pub use oh_menu_system::{Category, Item, Menu};
pub use real::RealMenuAssets;
pub use seams::{
    AudioSink, ConfigValues, ConnectionState, FakeAssets, FakeConfig, LevelInfo, MenuAssets,
    MenuConfig, NetEvent, NetworkClient, NewGameCallback, NoopNetwork, PackDataInfo,
    RecordingAudio, RichPresenceCallback, ScoreEntry, StyleBackground, TriggerRefreshCallback,
};
pub use states::{DialogInputState, PackChange, States};
