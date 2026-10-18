//! §11.2 / 11.3 / 11.7 / 11.8 / 11.9 / 11.10 — the `MenuGame` state machine.
//!
//! Two types:
//!
//! - [`Shared`] — all the mutable `MenuGame` state plus the seam handles
//!   (assets/config/network/audio/callbacks/font). Every state transition,
//!   the color derivation in `set_index`, the dialog-box helpers, the
//!   profile-editing logic, and the per-frame `update` step live here. It is
//!   held behind `Rc<RefCell<Shared>>` so the menu-item closures (built in
//!   [`crate::menu`]) can call back into it without a physical dependency on
//!   `MenuGame`.
//! - [`MenuGame`] — owns the five `Rc<Menu>`s and coordinates navigation
//!   (`up_action`/`down_action`/`left_action`/`right_action`/`ok_action`/
//!   `erase_action`/`exit_action` + the pack-change actions) between the
//!   `oh-menu-system` menus and the [`Shared`] state.
//!
//! Re-entrancy discipline: a menu `exec()` may call back into `Shared`
//! (through the item closures), so navigation methods never hold a
//! `RefMut<Shared>` across a `menu.exec()` call — they read what they need,
//! drop the borrow, `exec()`, then re-borrow.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use oh_rng::Pcg32Fast;

use oh_math::hg::color::Color;
use oh_menu_system::menu::Menu;

use crate::dialog_box::{DBoxDraw, DialogKey, HexagonDialogBox};
use crate::layout::{
    char_sizes, refresh_view, CharSizes, DIFFICULTY_BUMP_EFFECT_MAX, MAX_ERRORS_ON_SCREEN,
    MAX_OFFSET, MAX_PROFILES_ON_SCREEN,
};
use crate::menu;
use crate::seams::{
    AudioSink, IgnoreInputsCallback, LevelInfo, MenuAssets, MenuConfig, MenuJoystick, MenuTouch,
    MenuWindow, NetEvent, NetworkClient, NewGameCallback, RichPresenceCallback,
    TriggerRefreshCallback, UnlockAchievementCallback,
};
use crate::states::{DialogInputState, PackChange, States};

/// C++ `MenuGame` constructor's random tips (MenuGame.cpp:758-767) — the
/// verbatim four two-line pairs. The tip is chosen at construction with
/// `randomTip = tips[ssvu::getRndI(0, tips.size())]` (29b8a).
const RANDOM_TIPS: [[&str; 2]; 4] = [
    [
        "HOLDING FOCUS WHILE CHANGING PACK",
        "SKIPS THE SWITCH ANIMATION",
    ],
    ["REMEMBER TO TAKE BREAKS", "OPEN HEXAGON IS AN INTENSE GAME"],
    ["EXPERIMENT USING SWAP", "IT MAY SAVE YOUR LIFE"],
    [
        "IF A LEVEL IS TOO CHALLENGING",
        "PRACTICE IT AT A LOWER DIFFICULTY",
    ],
];

/// Process entropy seed — the same convention as oh-game's `initialize_rng`
/// (HexagonGame.cpp:55 port, hexagon_game.rs `entropy_seed`): SystemTime
/// nanos ^ pid rotated 32 bits. The C++ instead seeds its thread-local
/// `ssvu::pcg32_fast` with `std::random_device` (Rnd.hpp:21-22) — both
/// non-deterministic by design.
fn entropy_seed() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    now ^ (u64::from(std::process::id())).rotate_left(32)
}

/// C++ `randomTip = tips[ssvu::getRndI(0, tips.size())]` (MenuGame.cpp:
/// 758-767). The C++ `getRndI(min, max)` is
/// `uniform_int_distribution(min, max − 1)` (Rnd.hpp:44-50, inclusive) on
/// the thread-local `pcg32_fast`; the port runs the same draw through
/// `oh_rng` (the oracle-pinned `pcg32_fast` + `std::uniform_int`
/// sampling) in a thread-local engine.
fn random_tip_pair() -> [String; 2] {
    thread_local! {
        static TIP_RNG: std::cell::RefCell<Pcg32Fast> =
            std::cell::RefCell::new(Pcg32Fast::from_seed(entropy_seed()));
    }
    TIP_RNG.with(|c| {
        let mut rng = c.borrow_mut();
        let i = oh_rng::uniform_int(&mut rng, 0, RANDOM_TIPS.len() as u32 - 1) as usize;
        [RANDOM_TIPS[i][0].to_string(), RANDOM_TIPS[i][1].to_string()]
    })
}

/// C++ `MenuGame.hpp:135-137` — the credits-bar's cycling texture ids
/// (the `creditsBar2` sprite's three frames, MenuGame.cpp:2848-2850 —
/// 29b8b).
pub const CREDITS_BAR_IDS: [&str; 3] = ["creditsBar2.png", "creditsBar2b.png", "creditsBar2c.png"];

/// C++ `MenuGame::LevelDrawer` — the selection state for one of the two
/// level lists (regular / favorites).
#[derive(Debug, Clone)]
pub struct LevelDrawer {
    pub pack_idx: i32,
    pub current_index: i32,
    /// C++ `const std::vector<std::string>* levelDataIds` (owned copy).
    pub level_data_ids: Vec<String>,
    pub x_offset: f32,
    pub y_offset: f32,
    pub y_scroll_to: f32,
    pub lvl_offsets: Vec<f32>,
    pub is_favorites: bool,
}

impl Default for LevelDrawer {
    fn default() -> Self {
        Self {
            pack_idx: 0,
            current_index: 0,
            level_data_ids: Vec::new(),
            x_offset: 0.0,
            y_offset: 0.0,
            y_scroll_to: 0.0,
            lvl_offsets: Vec::new(),
            is_favorites: false,
        }
    }
}

/// All mutable `MenuGame` state + the seam handles.
pub struct Shared {
    // ---- state + navigation ----
    pub state: States,
    pub ignore_inputs: u32,
    pub focus_held: bool,
    pub was_focus_held: bool,
    /// C++ `wheelProgress` (MenuGame.hpp:172; ctor `wheelProgress{0.f}`
    /// MenuGame.cpp:187) — the wheel accumulator the `MouseWheelScrolled`
    /// handler adds to on every non-LevelSelection event (MenuGame.cpp:
    /// 344-354: a full line's worth of progress fires `upAction` /
    /// `downAction` and resets to 0).
    pub wheel_progress: f32,
    pub touch_delay: f32,
    /// C++ `shiftHeld`'s port equivalent (29b8c) — the app layer feeds the
    /// per-frame `LShift` / `RShift` state (the C++ `leftRightActionImpl`
    /// reads `sf::Keyboard::isKeyPressed(LShift/RShift)` live, MenuGame.cpp:
    /// 1877-1878; the menu has no window access, so the app layer's input
    /// snapshot feeds it — `MenuSlotAdapter::refresh_input`).
    pub shift_held: bool,
    // ---- level selection ----
    pub lvl_slct: LevelDrawer,
    pub fav_slct: LevelDrawer,
    pub favorites_active: bool,
    pub is_level_favorite: bool,
    pub first_level_selection: bool,
    pub diff_mult_idx: i32,
    pub pack_change_state: PackChange,
    pub pack_change_direction: i32,
    pub pack_change_offset: f32,
    pub scroll_speed: f32,
    // ---- loading screen ----
    pub scrollbar_offset: i32,
    pub hexagon_rotation: f32,
    pub random_tip: [String; 2],
    /// C++ `currentCreditsId{0}` (MenuGame.hpp:527) — the credits-bar's
    /// accumulated frametime (the `+= mFT` per frame, MenuGame.cpp:2848;
    /// the texture id is `credits_bar2_id()`, 29b8b).
    pub current_credits_id: f32,
    // ---- entering text ----
    pub entered_chars: Vec<char>,
    pub entered_str: String,
    // ---- misc ----
    pub exit_timer: f32,
    pub difficulty_bump_effect: f32,
    pub current_level: Option<LevelInfo>,
    // ---- first-time tips ----
    pub show_first_time_tips: bool,
    pub must_show_ftt_main_menu: bool,
    pub must_show_ftt_level_select: bool,
    pub must_show_ftt_death_tips: bool,
    pub must_show_login_at_startup: bool,
    pub dialog_box_delay: f32,
    // ---- layout ----
    pub w: f32,
    pub h: f32,
    pub four_by_three: bool,
    pub bg_skew_y: f32,
    pub menu_half_height: f32,
    pub entering_text_offset: f32,
    /// C++ `levelDetailsOffset` (MenuGame.hpp:421) — the level-selection
    /// left-panel slide offset; mutated by `drawLevelSelectionLeftSide`
    /// (`calcMenuOffset`, MenuGame.cpp:5321) and reset by
    /// `updateLevelSelectionDrawingParameters` (MenuGame.cpp:4298).
    pub level_details_offset: f32,
    pub char_sizes: CharSizes,
    pub rotation_speed: f32,
    // ---- colors ----
    pub menu_text_color: Color,
    pub menu_quad_color: Color,
    pub menu_selection_color: Color,
    pub dialog_box_text_color: Color,
    // ---- dialog ----
    pub dialog: HexagonDialogBox,
    pub dialog_input_state: DialogInputState,
    pub registration_username: String,
    pub registration_password: String,
    pub registration_password_confirm: String,
    pub login_username: String,
    pub login_password: String,
    pub delete_account_password: String,
    // ---- mouse-driven menu exec ----
    pub must_use_menu_item: Option<i32>,
    /// `window.stop()` (the main-menu / welcome "EXIT" items).
    pub quit_requested: bool,
    // ---- seams ----
    assets: Rc<dyn MenuAssets>,
    config: Rc<dyn MenuConfig>,
    network: Rc<dyn NetworkClient>,
    audio: Rc<dyn AudioSink>,
    /// issue 025f — the C++ ctor's `window` reference's touch half
    /// (the 9-zone grid's `getFingerDownCount` / `getFingerDownPositions`
    /// data, MenuGame.cpp:2802-2835).
    touch: Rc<dyn MenuTouch>,
    /// 29j1 — the C++ `Joystick` global's menu-side surface (the
    /// `MenuGame::update` joystick block, MenuGame.cpp:2643-2784, + the
    /// `setIgnoreAllInputs` `ignoreAllPresses` half, :3402); the
    /// composition backs it with the shared `oh_config::joystick`
    /// machine (seams::MenuJoystick's docs).
    joystick: Rc<dyn MenuJoystick>,
    /// issue 025g — the C++ ctor's `window` reference's size half
    /// (MenuGame.hpp:111 — the C++ `MenuGame::getWindowWidth()` /
    /// `getWindowHeight()`, MenuGame.cpp:3350-3357, read
    /// `window.getRenderWindow().getSize()` — the ACTUAL current
    /// render-target size, NOT the config's resolution; the 9-zone
    /// grid's thirds, `refresh_camera`'s view math, and the
    /// `drawOnlineStatus` identity view run in this space —
    /// `seams::MenuWindow`'s docs).
    window: Rc<dyn MenuWindow>,
    new_game: Option<NewGameCallback>,
    trigger_refresh: Option<TriggerRefreshCallback>,
    rich_presence: Option<RichPresenceCallback>,
    /// C++ `steamManager` (the ctor's first argument, main.cpp:427-434)
    /// — the `unlock_achievement` half (the C++'s two menu-side call
    /// sites; `seams::UnlockAchievementCallback`'s docs).
    unlock_achievement: Option<UnlockAchievementCallback>,
    ignore_inputs_cb: Option<IgnoreInputsCallback>,
    font: Rc<dyn crate::layout::FontHeightOracle>,
}

/// The C++ okAction's `ETLPNewBoot` / `ETLPNew` profile-creation block's
/// three exits (MenuGame.cpp:2211-2256): `Skipped` (the empty-name
/// `break` — the C++'s `if(!enteredStr.empty())` guard: it falls to the
/// okAction tail, which plays select.ogg), `Rejected` (the duplicate-name
/// `return` — the C++ exits okAction WITHOUT the select.ogg tail),
/// `Created` (the name — the caller adds the "profile selection" menu
/// item, the C++ `profiles.create` half, 29b6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileCreateResult {
    Skipped,
    Rejected,
    Created(String),
}

impl Shared {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        assets: Rc<dyn MenuAssets>,
        config: Rc<dyn MenuConfig>,
        network: Rc<dyn NetworkClient>,
        audio: Rc<dyn AudioSink>,
        touch: Rc<dyn MenuTouch>,
        joystick: Rc<dyn MenuJoystick>,
        window: Rc<dyn MenuWindow>,
        new_game: Option<NewGameCallback>,
        trigger_refresh: Option<TriggerRefreshCallback>,
        rich_presence: Option<RichPresenceCallback>,
        unlock_achievement: Option<UnlockAchievementCallback>,
        ignore_inputs_cb: Option<IgnoreInputsCallback>,
        font: Rc<dyn crate::layout::FontHeightOracle>,
    ) -> Self {
        // C++ ctor default state is `LoadingScreen`.
        Self {
            state: States::LoadingScreen,
            // C++ ctor BODY `setIgnoreAllInputs(1)` (MenuGame.cpp:358 —
            // the "To close the load results with any key" comment at
            // :357; the member-init `ignoreInputs{0}` at MenuGame.cpp:192
            // is overwritten by the ctor body before the `onEvent`
            // boot-screen handlers register at :519/:616/:673). The boot
            // screens hold this gate until
            // `checkCloseBootScreens` decrements it back to 0.
            ignore_inputs: 1,
            focus_held: false,
            was_focus_held: false,
            // C++ ctor `wheelProgress{0.f}` (MenuGame.cpp:187).
            wheel_progress: 0.0,
            touch_delay: 0.0,
            shift_held: false,
            lvl_slct: LevelDrawer::default(),
            fav_slct: LevelDrawer {
                is_favorites: true,
                ..LevelDrawer::default()
            },
            favorites_active: false,
            is_level_favorite: false,
            first_level_selection: true,
            diff_mult_idx: 0,
            pack_change_state: PackChange::Rest,
            pack_change_direction: 0,
            pack_change_offset: 0.0,
            scroll_speed: crate::layout::BASE_SCROLL_SPEED,
            scrollbar_offset: 0,
            hexagon_rotation: 0.0,
            random_tip: random_tip_pair(),
            current_credits_id: 0.0,
            entered_chars: Vec::new(),
            entered_str: String::new(),
            exit_timer: 0.0,
            difficulty_bump_effect: 0.0,
            current_level: None,
            show_first_time_tips: false,
            must_show_ftt_main_menu: true,
            must_show_ftt_level_select: true,
            must_show_ftt_death_tips: true,
            must_show_login_at_startup: true,
            dialog_box_delay: 0.0,
            w: 1024.0,
            h: 768.0,
            four_by_three: false,
            bg_skew_y: 0.6,
            menu_half_height: 0.0,
            entering_text_offset: 0.0,
            level_details_offset: 0.0,
            char_sizes: char_sizes(false),
            rotation_speed: 0.0,
            menu_text_color: Color::WHITE,
            menu_quad_color: Color::BLACK,
            menu_selection_color: Color::WHITE,
            dialog_box_text_color: Color::BLACK,
            dialog: HexagonDialogBox::new(),
            dialog_input_state: DialogInputState::Nothing,
            registration_username: String::new(),
            registration_password: String::new(),
            registration_password_confirm: String::new(),
            login_username: String::new(),
            login_password: String::new(),
            delete_account_password: String::new(),
            must_use_menu_item: None,
            quit_requested: false,
            assets,
            config,
            network,
            audio,
            touch,
            joystick,
            window,
            new_game,
            trigger_refresh,
            rich_presence,
            unlock_achievement,
            ignore_inputs_cb,
            font,
        }
    }

    // ---- seam accessors ----
    #[must_use]
    pub fn assets(&self) -> &Rc<dyn MenuAssets> {
        &self.assets
    }
    #[must_use]
    pub fn config(&self) -> &Rc<dyn MenuConfig> {
        &self.config
    }
    #[must_use]
    pub fn network(&self) -> &Rc<dyn NetworkClient> {
        &self.network
    }
    #[must_use]
    pub fn audio(&self) -> &Rc<dyn AudioSink> {
        &self.audio
    }

    #[must_use]
    pub fn touch(&self) -> &Rc<dyn MenuTouch> {
        &self.touch
    }

    /// 29j1 — the C++ `Joystick` global's menu-side seam.
    #[must_use]
    pub fn joystick(&self) -> &Rc<dyn MenuJoystick> {
        &self.joystick
    }

    /// The C++ ctor's `window` reference's size half (`seams::MenuWindow`'s
    /// docs — issue 025g): the actual current window size the C++ reads
    /// live from `window.getRenderWindow().getSize()`.
    #[must_use]
    pub fn window(&self) -> &Rc<dyn MenuWindow> {
        &self.window
    }

    /// C++ `window.stop()` (EXIT items).
    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// C++ `steamManager.unlock_achievement(id)` (the `seams::
    /// UnlockAchievementCallback` docs) — `None` = the C++'s
    /// not-initialized manager (no-op).
    pub fn unlock_achievement(&self, id: &str) {
        if let Some(cb) = &self.unlock_achievement {
            cb(id);
        }
    }

    /// Fire the `fnHGTriggerRefresh` seam (keyboard/joystick bind controls).
    pub fn fire_trigger_refresh(&self, name: &str, id: i32) {
        if let Some(cb) = &self.trigger_refresh {
            cb(name, id);
        }
    }

    /// C++ `playSoundOverride(assetId)` → `audio.playSound` override.
    pub fn play_sound_override(&mut self, asset_id: &str) {
        self.audio.play_sound(asset_id);
    }

    /// C++ `setIgnoreAllInputs(presses)` — `ignoreInputs = presses` (0
    /// clears the input-ignore on game + joystick; else holds it).
    pub fn set_ignore_all_inputs(&mut self, presses: u32) {
        self.ignore_inputs = presses;
        // C++ `game.ignoreAllInputs(presses != 0)` (MenuGame.cpp:3393-3400 →
        // Manager.hpp:78-81) — the keyboard half via the composition's
        // callback; the `Joystick::ignoreAllPresses(presses != 0)` half
        // (MenuGame.cpp:3397/:3402, 29j1) latches the shared machine so
        // no rising edge fires while the boot screens / dialogs hold the
        // input-ignore gate (C++ order: game half first).
        if let Some(cb) = &self.ignore_inputs_cb {
            cb(presses != 0);
        }
        self.joystick.ignore_all_presses(presses != 0);
    }

    /// C++ `isEnteringText()`.
    #[must_use]
    pub fn is_entering_text(&self) -> bool {
        self.state.is_entering_text()
    }

    // ---- state → menu (used by `MenuGame` and the online visibility) ----

    /// C++ `getNthSelectablePackInfo`.
    #[must_use]
    pub fn get_nth_selectable_pack_id(&self, i: i32) -> Option<String> {
        let ids = self.assets.selectable_pack_ids();
        ids.get(i as usize).cloned()
    }

    /// C++ `getSelectablePackInfosSize()`.
    #[must_use]
    pub fn get_selectable_pack_infos_size(&self) -> usize {
        // C++ :4981-4983 — the favorites list counts as a single pack.
        if self.favorites_active {
            return 1;
        }
        self.assets.selectable_pack_ids().len()
    }

    // The active level drawer is chosen by `favorites_active`; the methods
    // below access `lvl_slct` / `fav_slct` through targeted field borrows so
    // they can also read `self.assets` in the same call.

    /// C++ `isFavoriteLevels()`.
    #[must_use]
    pub fn is_favorite_levels(&self) -> bool {
        self.favorites_active
    }

    /// C++ `addRemoveFavoriteLevel()` (MenuGame.cpp:4835-4931) — the F1 /
    /// `(un)favorite` toggle: the level under the active drawer's index is
    /// added to / removed from the current profile's favorites (the 029c1
    /// store) and the favorites list / offsets / drawer switch follow the
    /// C++ arm-by-arm.
    pub fn add_remove_favorite_level(&mut self) {
        // C++ :4836-4838 — the level under the active drawer's index (the
        // port's drawer ids are the C++'s `packId + "_" + id` full ids, so
        // the :4840 reconstruction is the entry itself).
        let level_id = {
            let d = if self.favorites_active {
                &self.fav_slct
            } else {
                &self.lvl_slct
            };
            d.level_data_ids.get(d.current_index as usize).cloned()
        };
        let Some(level_id) = level_id else {
            return;
        };

        // Level is a favorite so remove it.
        if self.is_level_favorite {
            self.assets.remove_favorite_level(&level_id);
            // C++ :4845-4847.
            self.fav_slct.level_data_ids.retain(|id| id != &level_id);
            // C++ :4848.
            self.fav_slct.lvl_offsets.pop();

            // C++ :4850-4860 — the favorites index within bounds.
            if !self.fav_slct.level_data_ids.is_empty() {
                let n = self.fav_slct.level_data_ids.len() as i32;
                self.fav_slct.current_index = self.fav_slct.current_index.clamp(0, n - 1);
            } else {
                self.fav_slct.current_index = 0;
            }

            // C++ :4862-4865.
            if self.is_favorite_levels() {
                self.adjust_levels_offset();
            }

            // C++ :4867-4888 — an empty list forces the exit from the
            // favorites view; else the no-empty-space scroll test.
            if self.fav_slct.level_data_ids.is_empty() {
                // this way the menu is not drawn anymore.
                self.fav_slct.x_offset = 0.0;
                self.favorites_active = false;
            } else {
                let layout = self.level_selection_layout();
                let scroll = self.h
                    - (layout.pack_label_height
                        + 2.0 * layout.slct_frame_size
                        + layout.level_label_height * self.fav_slct.level_data_ids.len() as f32);
                if scroll > self.fav_slct.y_offset {
                    self.fav_slct.y_offset = scroll;
                    self.fav_slct.y_scroll_to = scroll;
                }
            }

            // C++ :4890-4892 — update the looks (on the possibly-switched
            // active drawer; `set_index` re-derives `is_level_favorite`
            // — the C++'s `setIndex` re-reads the profile there).
            self.set_index(if self.favorites_active {
                self.fav_slct.current_index
            } else {
                self.lvl_slct.current_index
            });
        } else {
            // C++ :4893-4896.
            self.assets.add_favorite_level(&level_id);
            self.fav_slct.lvl_offsets.push(0.0);
            self.is_level_favorite = true;

            // C++ :4897-4924 — add the level to the favorites list keeping
            // it sorted in (lowercased-name) alphabetical order.
            let tweaked_fav_name = self
                .assets
                .level(&level_id)
                .map(|l| l.name.to_lowercase())
                .unwrap_or_default();
            let pos = self.fav_slct.level_data_ids.iter().position(|id| {
                self.assets
                    .level(id)
                    .map(|l| l.name.to_lowercase())
                    .unwrap_or_default()
                    > tweaked_fav_name
            });
            match pos {
                Some(p) => self.fav_slct.level_data_ids.insert(p, level_id),
                None => self.fav_slct.level_data_ids.push(level_id),
            }

            // C++ :4926-4927 — "Just in case."
            let n = self.fav_slct.level_data_ids.len() as i32;
            self.fav_slct.current_index = self.fav_slct.current_index.clamp(0, n - 1);
        }

        self.play_sound_override("select.ogg");
    }

    /// C++ `switchToFromFavoriteLevels()` (MenuGame.cpp:4933-4955) — the
    /// F2 favorites-view switch (no-op without a favorites list or outside
    /// the level selection).
    pub fn switch_to_from_favorite_levels(&mut self) {
        // C++ :4934-4937.
        if self.state != States::LevelSelection || self.fav_slct.level_data_ids.is_empty() {
            return;
        }
        // C++ :4940-4943 — quickly finish any ongoing pack changes.
        if self.pack_change_state == PackChange::Folding {
            self.change_pack();
        }
        self.pack_change_state = PackChange::Rest;
        // C++ :4945-4946 — stop the outgoing drawer's scroll (the C++
        // reads the pre-switch `lvlDrawer`).
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.y_scroll_to = d.y_offset;
        self.pack_change_offset = 0.0;

        // C++ :4950 — the switch.
        self.favorites_active = !self.favorites_active;
        // C++ :4951 — update the looks (on the NEW drawer).
        self.set_index(if self.favorites_active {
            self.fav_slct.current_index
        } else {
            self.lvl_slct.current_index
        });
        // C++ :4952-4954.
        self.adjust_levels_offset();
        self.reset_level_names_scrolls();
        self.play_sound_override("select.ogg");
    }

    // ---- reloadAssets (§11.x) ----

    /// C++ `MenuGame::reloadAssets(reloadEntirePack)` (MenuGame.cpp:3092-3127)
    /// — the hardcoded F3 (`reloadAssets(false)` — reload the LEVEL's
    /// assets) / F4 (`reloadAssets(true)` — reload the PACK's assets) debug
    /// handler.
    ///
    /// Guard (C++ :3094-3099): `state == LevelSelection &&
    /// dialogBox.empty() && Config::getDebug()` — the whole thing is
    /// debug-gated. Then: `reloadAllShaders` + `reloadPack` / `reloadLevel`
    /// (the current level's `packId` / `packPath` / `id` — the C++'s
    /// `levelData` member; the port's active drawer's current entry, the
    /// full asset id the `MenuAssets` seam resolves), `setIndex(
    /// lvlDrawer->currentIndex)` to load the new level
    /// data, the UPPERCASE result dialog + `select.ogg` +
    /// `setIgnoreAllInputs(2)` (the C++'s "needs to be two" comment:
    /// the dialog box reacts to key releases — the first release is the
    /// key that popped the box, the second closes it).
    pub fn reload_assets(&mut self, reload_entire_pack: bool) {
        // C++ :3094-3099 — the guard.
        if self.state != States::LevelSelection || !self.dialog.empty() || !self.config.debug() {
            return;
        }
        // C++ :3101 — `assets.reloadAllShaders()`.
        self.assets.reload_all_shaders();
        // C++ :3104-3114 — the current level's data. The C++'s `levelData`
        // member (the `setIndex`-loaded entry) — the port's active
        // drawer's current entry: the full asset id the `MenuAssets` seam
        // resolves into the C++'s `packId` / `packPath` / `id` triplet.
        let level_id = {
            let d = if self.favorites_active {
                &self.fav_slct
            } else {
                &self.lvl_slct
            };
            d.level_data_ids.get(d.current_index as usize).cloned()
        };
        let Some(level_id) = level_id else {
            // C++: `levelData` would be the null-deref; the port's
            // defensive no-op (the handler only fires with a drawer
            // entry — the boot's `setIndex` half).
            return;
        };
        let reload_output = self.assets.reload_assets(reload_entire_pack, &level_id);
        // C++ :3116 — `setIndex(lvlDrawer->currentIndex)` — loads the new
        // level data (the active drawer's index — the C++'s `lvlDrawer`).
        let index = if self.favorites_active {
            self.fav_slct.current_index
        } else {
            self.lvl_slct.current_index
        };
        self.set_index(index);
        // C++ :3118-3120 — the uppercase result dialog + the close line
        // (`Utils::uppercasify` = per-char `std::toupper` → the ASCII
        // half; the line itself is already uppercase).
        let mut reload_output = reload_output.to_ascii_uppercase();
        reload_output.push_str("\nPRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n");
        // C++ :3121-3126 — the "Needs to be two" comment, the sound,
        // the dialog, the two-release gate.
        self.play_sound_override("select.ogg");
        self.show_dialog_box(&reload_output);
        self.set_ignore_all_inputs(2);
    }

    // ---- setIndex (§11.10) ----

    /// C++ `setIndex(mIdx)` — resolve the level at `mIdx` in the active
    /// drawer, derive the menu colors, set `diffMultIdx` to the 1.0× index.
    /// The menu Lua execution (`runLuaFile` + `onInit` + `onLoad`) is a
    /// documented seam (menu Lua is wired through `oh-lua` by a later issue);
    /// it only affects the background style and never the state machine.
    pub fn set_index(&mut self, m_idx: i32) {
        let level_id = {
            let d = if self.favorites_active {
                &self.fav_slct
            } else {
                &self.lvl_slct
            };
            d.level_data_ids.get(m_idx as usize).cloned()
        };
        let Some(level_id) = level_id else {
            return;
        };

        {
            let d = if self.favorites_active {
                &mut self.fav_slct
            } else {
                &mut self.lvl_slct
            };
            d.current_index = m_idx;
        }

        let level = match self.assets.level(&level_id) {
            Some(l) => l,
            None => return,
        };

        // Favorites: re-derive the pack index from the level's pack.
        if self.favorites_active {
            self.is_level_favorite = true;
            let packs = self.assets.selectable_pack_ids();
            for (i, p) in packs.iter().enumerate() {
                if p == &level.pack_id {
                    self.fav_slct.pack_idx = i as i32;
                    break;
                }
            }
        } else if !self.first_level_selection {
            self.is_level_favorite = self.assets.is_level_favorite(&level_id);
        }

        let colors = self.assets.style_colors(&level_id);
        let bw = self.config.black_and_white();

        let mut quad = if bw {
            Color::new(20, 20, 20, 255)
        } else {
            self.assets.style_text_color(&level_id)
        };
        if quad.a == 0 && !bw {
            for c in &colors {
                if c.a != 0 {
                    quad = *c;
                    break;
                }
            }
        }
        let mut text = if bw {
            Color::WHITE
        } else if let Some(c0) = colors.first() {
            *c0
        } else {
            Color::WHITE
        };

        let mut dialog = quad;
        dialog.a = 255;

        let mut selection;
        if colors.len() == 1 {
            text.a = 255;
            selection = quad;
            selection.a = 75;
        } else {
            if text.a == 0 || text == quad {
                for c in &colors {
                    if c.a != 0 && c != &quad && !bw {
                        text = *c;
                        break;
                    }
                }
            }
            selection = if bw {
                Color::WHITE
            } else {
                colors.get(1).copied().unwrap_or(Color::WHITE)
            };
            if selection.a == 0 || selection == quad || selection == text {
                for c in &colors {
                    if c.a != 0 && c != &quad && c != &text && !bw {
                        selection = *c;
                        break;
                    }
                }
            }
            selection.a = 175;
        }

        self.menu_quad_color = quad;
        self.menu_text_color = text;
        self.menu_selection_color = selection;
        self.dialog_box_text_color = dialog;

        // `diffMultIdx` = index of the first 1.0× multiplier.
        self.diff_mult_idx = 0;
        while (self.diff_mult_idx as usize) < level.difficulty_mults.len()
            && level.difficulty_mults[self.diff_mult_idx as usize] != 1.0
        {
            self.diff_mult_idx += 1;
        }

        // Menu Lua: `runLuaFile(level.lua) + onInit + onLoad`. Seam — not
        // wired to `oh-lua` yet (only affects the background style/pulse).
        self.current_level = Some(level);
    }

    // ---- dialog box helpers (§11.6 / HexagonDialogBox call sites) ----

    /// C++ `showDialogBox(msg)` — `create(msg, 22, 12.f, center)`.
    pub fn show_dialog_box(&mut self, msg: &str) {
        let line_h = self.font.height(22.0);
        let width = 60.0 * msg.len() as f32; // coarse width probe (slice 13b)
        self.dialog.create_with_key(
            msg,
            22,
            12.0,
            DBoxDraw::Center,
            DialogKey::Unknown,
            0.0,
            0.0,
            line_h,
            width,
        );
    }

    /// C++ `showInputDialogBox(msg)` — `createInput(msg, 22, 12.f, center)`.
    pub fn show_input_dialog_box(&mut self, msg: &str) {
        let line_h = self.font.height(22.0);
        let width = 60.0 * msg.len() as f32;
        self.dialog
            .create_input(msg, 22, 12.0, DBoxDraw::Center, line_h, width);
    }

    /// C++ `showInputDialogBoxNice(title, inputType, extra)`.
    pub fn show_input_dialog_box_nice(&mut self, title: &str, input_type: &str, extra: &str) {
        let mut str_buf = String::new();
        if extra.is_empty() {
            str_buf.push_str(title);
            str_buf.push_str("\n\nPLEASE INSERT ");
            str_buf.push_str(input_type);
            str_buf.push_str("\n\nCONFIRM WITH [ENTER]\nCANCEL WITH [ESCAPE]\n");
        } else {
            str_buf.push_str(title);
            str_buf.push_str("\n\nPLEASE INSERT ");
            str_buf.push_str(input_type);
            str_buf.push_str("\n\n");
            str_buf.push_str(extra);
            str_buf.push_str("\n\nCONFIRM WITH [ENTER]\nCANCEL WITH [ESCAPE]\n");
        }
        self.show_input_dialog_box(&str_buf);
    }

    /// C++ `openLoginDialogBoxAndStartLoginProcess()`.
    pub fn open_login_dialog_box_and_start_login_process(&mut self) {
        self.dialog_input_state = DialogInputState::Login_EnteringUsername;
        // C++ `MenuGame.cpp:6152-6154`:
        //   defaultLoginUsername = Config::getSaveLastLoginUsername()
        //       ? Config::getLastLoginUsername() : "".
        let default_username = if self.config.save_last_username() {
            self.config.last_login_username()
        } else {
            String::new()
        };
        self.show_input_dialog_box_nice_with_default("LOGIN", "USERNAME", &default_username);
    }

    fn show_input_dialog_box_nice_with_default(
        &mut self,
        title: &str,
        input_type: &str,
        def: &str,
    ) {
        self.show_input_dialog_box_nice(title, input_type, "");
        self.dialog.input_mut().push_str(def);
    }

    /// C++ `ignoreInputsAfterMenuExec()` — `setIgnoreAllInputs(
    /// mustUseMenuItem.has_value() ? 1 : 2)`.
    pub fn ignore_inputs_after_menu_exec(&mut self) {
        let n = if self.must_use_menu_item.is_some() {
            1
        } else {
            2
        };
        self.set_ignore_all_inputs(n);
    }

    /// C++ `checkCloseDialogBox`'s `closeBox` lambda (MenuGame.cpp:
    /// 386-391) — select.ogg + `clearDialogBox` + `setIgnoreAllInputs(0)`.
    fn close_box(&mut self) {
        self.play_sound_override("select.ogg");
        self.dialog.clear();
        self.set_ignore_all_inputs(0);
    }

    /// The C++ release handlers' raw `--ignoreInputs` (the Key handler's
    /// close-key half, MenuGame.cpp:544-550; the mouse / joystick
    /// dialog halves, :638-641 / :695-698; the generic tails, :579-582 /
    /// :649-653 / :706-709). The C++'s raw decrement leaves the manager
    /// gate unsynced; the port re-syncs it on every decrement via
    /// `set_ignore_all_inputs` — observable-identical here (the gate is
    /// already latched while `ignore_inputs > 0`, and the close / the
    /// sequence arms re-sync at 0 / 1 within the same event).
    fn decrement_ignore_inputs(&mut self) {
        let n = self.ignore_inputs - 1;
        self.set_ignore_all_inputs(n);
    }

    /// C++ `checkCloseDialogBox` lambda (MenuGame.cpp:384-489) — the
    /// dialog input-sequence machine: on the close-key release, the
    /// `Nothing` state closes the box; the Registration / Login /
    /// DeleteAccount sequences capture the typed input, transition
    /// (the masked re-dialogs + `setIgnoreAllInputs(1)`), and fire the
    /// network seam (`try_register` / `try_login` / `try_delete_account`)
    /// — or the "PASSWORD MISMATCH" dialog on a confirm mismatch
    /// (29b3). The C++'s `transitionInputSequence` / `endInputSequence`
    /// scope-guard order: the state is set, the input is captured, THEN
    /// `closeBox` runs (the guard fires at scope exit, after the return
    /// value is evaluated).
    fn check_close_dialog_box(&mut self) {
        // C++ :408-410 — the counter gate: the box only reacts once the
        // caller's `--ignoreInputs` took the counter to 0.
        if self.ignore_inputs != 0 {
            return;
        }
        match self.dialog_input_state {
            DialogInputState::Nothing => {
                // C++ :412-416.
                self.close_box();
            }
            DialogInputState::Registration_EnteringUsername => {
                // C++ :419-427.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Registration_EnteringPassword;
                self.close_box();
                self.registration_username = typed;
                self.show_input_dialog_box_nice("REGISTRATION", "PASSWORD", "");
                self.dialog.set_input_box_password(true);
                self.set_ignore_all_inputs(1);
            }
            DialogInputState::Registration_EnteringPassword => {
                // C++ :430-438.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Registration_EnteringPasswordConfirm;
                self.close_box();
                self.registration_password = typed;
                self.show_input_dialog_box_nice("REGISTRATION", "CONFIRM PASSWORD", "");
                self.dialog.set_input_box_password(true);
                self.set_ignore_all_inputs(1);
            }
            DialogInputState::Registration_EnteringPasswordConfirm => {
                // C++ :440-459.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Nothing;
                self.close_box();
                self.registration_password_confirm = typed;
                if self.registration_password == self.registration_password_confirm {
                    let user = self.registration_username.clone();
                    let pass = self.registration_password.clone();
                    self.network.try_register(&user, &pass);
                } else {
                    self.play_sound_override("error.ogg");
                    self.show_dialog_box(
                        "PASSWORD MISMATCH\n\n\
                         PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
                    );
                    self.set_ignore_all_inputs(1);
                }
            }
            DialogInputState::Login_EnteringUsername => {
                // C++ :465-473.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Login_EnteringPassword;
                self.close_box();
                self.login_username = typed;
                self.show_input_dialog_box_nice("LOGIN", "PASSWORD", "");
                self.dialog.set_input_box_password(true);
                self.set_ignore_all_inputs(1);
            }
            DialogInputState::Login_EnteringPassword => {
                // C++ :476-480.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Nothing;
                self.close_box();
                self.login_password = typed;
                let user = self.login_username.clone();
                let pass = self.login_password.clone();
                self.network.try_login(&user, &pass);
            }
            DialogInputState::DeleteAccount_EnteringPassword => {
                // C++ :483-487.
                let typed = self.dialog.input().to_string();
                self.dialog_input_state = DialogInputState::Nothing;
                self.close_box();
                self.delete_account_password = typed;
                self.network
                    .try_delete_account(&self.delete_account_password);
            }
        }
    }

    // ---- changeStateTo (§11.3) ----

    /// C++ `changeStateTo(mState)` — full transition + first-time tips.
    pub fn change_state_to(&mut self, m_state: States) {
        let prev_state = self.state;
        self.state = m_state;

        if prev_state == self.state {
            return;
        }

        if self.state == States::SMain
            && std::mem::replace(&mut self.must_show_login_at_startup, false)
            && self.config.show_login_at_startup()
        {
            self.open_login_dialog_box_and_start_login_process();
            self.set_ignore_all_inputs(2);
        }

        if self.state == States::LevelSelection {
            self.first_level_selection = false;
        }

        if !self.show_first_time_tips {
            return;
        }

        let show_tip = |s: &mut Self, str: &str| {
            s.play_sound_override("select.ogg");
            s.show_dialog_box(str);
            s.set_ignore_all_inputs(1);
            s.dialog_box_delay = 64.0;
        };

        if self.state == States::SMain
            && std::mem::replace(&mut self.must_show_ftt_main_menu, false)
        {
            show_tip(
                self,
                "WELCOME TO OPEN HEXAGON!\n\n\
                 YOU CAN NAVIGATE THE MAIN MENU WITH THE UP/DOWN ARROW KEYS\n\
                 OR THE DPAD/THUMBSTICK ON YOUR CONTROLLER\n\n\
                 REMEMBER TO CHECK OUT THE OPTIONS MENU\n\n\
                 PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
            );
        } else if self.state == States::LevelSelection
            && std::mem::replace(&mut self.must_show_ftt_level_select, false)
        {
            show_tip(
                self,
                "THIS IS WHERE YOU CAN PICK A LEVEL TO PLAY\n\n\
                 LEVELS ARE ORGANIZED IN 'LEVEL PACKS'\n\
                 TO BROWSE LEVELS AND LEVEL PACKS, GO UP/DOWN\n\
                 HOLD SHIFT (FOCUS) TO QUICKLY JUMP BETWEEN PACKS\n\
                 TO CHANGE THE DIFFICULTY OF A LEVEL, GO LEFT/RIGHT\n\n\
                 AS A FIRST TIMER, PLAY THE 'CUBE' LEVELS IN ORDER\n\
                 THEN YOU CAN GET NEW LEVELS ON THE STEAM WORKSHOP\n\n\
                 HAVE FUN!\n\n\
                 PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
            );
        } else if self.state == States::LevelSelection
            && std::mem::replace(&mut self.must_show_ftt_death_tips, false)
        {
            show_tip(
                self,
                "IF YOU FEEL LIKE THE GAME IS TOO HARD, TRY THESE TIPS\n\n\
                 - DECREMENT THE DIFFICULTY BY PRESSING LEFT\n\
                 - PLAY AN EASIER LEVEL TO BUILD UP MUSCLE MEMORY\n\
                 - TURN ON 'INVINCIBILITY' IN 'GAMEPLAY' OPTIONS TO PRACTICE\n\n\
                 REMEMBER THAT PRACTICE IS THE SECRET TO SUCCESS!\n\n\
                 PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
            );
        }
    }

    // ---- playLocally (§11.2 / boot flow) ----

    /// C++ `playLocally()` — `assets.pSaveCurrent(); enteredStr = "";
    /// state = getLocalProfilesSize() == 0 ? ETLPNewBoot : SLPSelectBoot`
    /// (MenuGame.cpp:1783-1789): a run's scores reach disk FIRST — the
    /// C++'s `pSaveCurrent()` is the call (the no-current-profile early
    /// return keeps the call a no-op on a fresh boot).
    pub fn play_locally(&mut self) {
        self.assets.profile_save_current();
        self.entered_str.clear();
        self.state = if self.assets.profile_count() == 0 {
            States::ETLPNewBoot
        } else {
            States::SLPSelectBoot
        };
    }

    /// C++ `changeFavoriteLevelsToProfile()` (MenuGame.cpp:4781-4833) —
    /// rebuild the favorites drawer from the current profile's favorite
    /// level ids (each profile has its own favorites).
    pub fn change_favorite_levels_to_profile(&mut self) {
        // C++ :4786-4792 — copy the profile's favorites into the list.
        let ids = self.assets.favorite_level_ids();
        // C++ :4794-4798 — sort by the lowercased level name.
        let mut named: Vec<(String, String)> = ids
            .into_iter()
            .map(|id| {
                (
                    self.assets
                        .level(&id)
                        .map(|l| l.name.to_lowercase())
                        .unwrap_or_default(),
                    id,
                )
            })
            .collect();
        named.sort_by(|a, b| a.0.cmp(&b.0));
        let fav_ids: Vec<String> = named.into_iter().map(|(_, id)| id).collect();

        let sz = fav_ids.len() as i32;
        // C++ :4804-4808 — no favorites: force the regular drawer.
        if sz == 0 {
            self.favorites_active = false;
            return;
        }
        // C++ :4812 — the favorites index within boundaries.
        self.fav_slct.current_index = self.fav_slct.current_index.clamp(0, sz - 1);
        self.fav_slct.level_data_ids = fav_ids;
        // C++ :4815 — resize the level offsets (the C++ `resize` keeps
        // the existing values and value-initializes the new tail to
        // `0.f` — `Vec::resize(len, 0.0)` matches).
        self.fav_slct.lvl_offsets.resize(sz as usize, 0.0);
    }

    // ---- profile editing (okAction / eraseAction) ----

    /// C++ okAction's `ETLPNewBoot` / `ETLPNew` profile-creation block
    /// (the `pCreate` / `pSetCurrent` / `changeFavoriteLevelsToProfile`
    /// half — the `profiles.create` menu-item half is the caller's, 29b6).
    pub fn create_profile_from_entered(&mut self) -> ProfileCreateResult {
        if self.entered_str.is_empty() {
            // C++ :2214 `if(!enteredStr.empty())` — the empty guard is
            // faithful to the C++: it skips the `pCreate` call too (the
            // block's `break` → the okAction tail's select.ogg).
            return ProfileCreateResult::Skipped;
        }
        // Reject duplicate names (the C++ :2220-2233).
        for name in self.assets.profile_names() {
            if self.entered_str == name {
                self.play_sound_override("error.ogg");
                self.show_dialog_box(
                    "A PROFILE WITH THE SAME NAME ALREADY EXISTS\n\
                     PLEASE ENTER ANOTHER NAME\n\n\
                     PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
                );
                self.set_ignore_all_inputs(2);
                // C++ :2232 `return;` — out of okAction (no select.ogg).
                return ProfileCreateResult::Rejected;
            }
        }
        // C++ :2235-2237 — `pCreate` + `pSetCurrent` + the favorites
        // rebuild, then the menu item (the caller's half, 29b6).
        let name = self.entered_str.clone();
        self.assets.profile_create(&name);
        self.assets.profile_set_current(&name);
        self.change_favorite_levels_to_profile();
        self.entered_str.clear();
        ProfileCreateResult::Created(name)
    }

    // ---- playSelectedLevel (§11.8) ----

    /// C++ `playSelectedLevel()` — fires the `fnHGNewGame` seam.
    pub fn play_selected_level(&mut self) {
        let Some(cb) = &self.new_game else {
            return;
        };
        // C++ :2369-2383 — the ACTIVE drawer (`lvlDrawer`), not always
        // `lvlSlct`: with the favorites view on, the played level / pack
        // come from `favSlct` (the C++'s `levelData` lookup of
        // `lvlDrawer->levelDataIds->at(lvlDrawer->currentIndex)` — the
        // diff-mult half, `levelData->getNthDiffMult(diffMultIdx)`).
        let (pack_idx, level_id) = {
            let d = if self.favorites_active {
                &self.fav_slct
            } else {
                &self.lvl_slct
            };
            (
                d.pack_idx,
                d.level_data_ids
                    .get(d.current_index as usize)
                    .cloned()
                    .unwrap_or_default(),
            )
        };
        let pack_id = self
            .get_nth_selectable_pack_id(pack_idx)
            .unwrap_or_default();
        let diff_mult = self
            .assets
            .level(&level_id)
            .map_or(1.0, |l| l.get_nth_diff_mult(self.diff_mult_idx));
        // C++ :2373 — `setMouseCursorVisible(false)`: the cursor is hidden
        // when the level starts (inside the `if(fnHGNewGame)` guard, the
        // `let Some(cb)` above — the port's `play_selected_level` only
        // reaches the callback when a `new_game` seam is wired, exactly
        // as the C++ only hides when `fnHGNewGame` is set).
        self.window().set_mouse_cursor_visible(false);
        cb(&pack_id, &level_id, true, diff_mult, false);
    }

    // ---- entering-text accumulation (§11.9) ----

    pub(crate) fn process_entered_chars(&mut self) {
        const LIMIT: usize = 18;
        let chars: Vec<char> = self.entered_chars.clone();
        // C++ `update()` (MenuGame.cpp:2862-2878): the accumulation into
        // `enteredStr` is gated on `isEnteringText()` (the
        // ETLPNewBoot / ETLPNew states); the `enteredChars.clear()` is
        // UNCONDITIONAL (:2878, outside the gate).
        if self.is_entering_text() {
            for c in &chars {
                if self.entered_str.len() < LIMIT && (c.is_alphanumeric() || is_punctuation(c)) {
                    self.play_sound_override("beep.ogg");
                    self.entered_str.push(*c);
                }
            }
        }
        self.entered_chars.clear();
    }

    // ---- network event handling (§11.9) ----

    /// C++ `update()` `hexagonClient.pollEvent()` handling.
    pub(crate) fn handle_net_event(&mut self, event: NetEvent) {
        if !self.dialog.empty() {
            return;
        }
        let in_scope = matches!(
            self.state,
            States::SMain
                | States::MOnline
                | States::MOpts
                | States::SLPSelect
                | States::LevelSelection
        );
        if !in_scope {
            return;
        }

        let (error, msg, extra) = match &event {
            NetEvent::ConnectionSuccess => (false, "CONNECTION SUCCESS", ""),
            NetEvent::ConnectionFailure(e) => (true, "CONNECTION FAILURE", e.as_str()),
            NetEvent::Kicked => (true, "DISCONNECTED FROM SERVER", ""),
            NetEvent::RegistrationSuccess => (false, "REGISTRATION SUCCESS", ""),
            NetEvent::RegistrationFailure(e) => (true, "REGISTRATION FAILURE", e.as_str()),
            NetEvent::LoginSuccess => (false, "LOGIN SUCCESS", ""),
            NetEvent::LoginFailure(e) => (true, "LOGIN FAILURE", e.as_str()),
            NetEvent::LogoutSuccess => (false, "LOGOUT SUCCESS", ""),
            NetEvent::LogoutFailure(e) => (true, "LOGOUT FAILURE", e.as_str()),
            NetEvent::DeleteAccountSuccess => (false, "DELETE ACCOUNT SUCCESS", ""),
            NetEvent::DeleteAccountFailure(e) => (true, "DELETE ACCOUNT FAILURE", e.as_str()),
            NetEvent::GameVersionMismatch => return, // C++ logs only
            NetEvent::ProtocolVersionMismatch => {
                (true, "CLIENT/SERVER PROTOCOL VERSION MISMATCH", "")
            }
        };

        if error {
            self.play_sound_override("error.ogg");
        } else {
            self.play_sound_override("select.ogg");
        }

        let mut str_buf = String::from(msg);
        if !extra.is_empty() {
            str_buf.push_str("\n\n");
            str_buf.push_str(extra);
        }
        str_buf.push('\n');

        self.dialog_box_delay = 16.0;
        self.show_dialog_box(&str_buf);
        self.set_ignore_all_inputs(1);

        // C++ :2590-2593 — the `ELoginSuccess` arm: the `a23_login` Steam
        // achievement fires right after the "LOGIN SUCCESS" dialog box
        // (the C++ lambda order: show the dialog, then the unlock — 29b8d).
        if matches!(event, NetEvent::LoginSuccess) {
            self.unlock_achievement("a23_login");
        }
    }

    // ---- per-state animation step (§11.9) ----

    /// C++ `update()` `LevelSelection` pack-change fold/stretch machine
    /// (MenuGame.cpp:2882-2934). The `Rest` arm runs the level-list scroll
    /// easing (`scrollLevelListToTargetY`, the 29b5 port); `Folding` / `Stretching` carry the C++
    /// per-frame `calcPackChangeScrollFold` / `calcPackChangeScrollStretch` calls.
    pub(crate) fn step_pack_change(&mut self, ft: f32) {
        match self.pack_change_state {
            PackChange::Rest => {
                self.scroll_level_list_to_target_y(ft);
            }
            PackChange::Folding => {
                let list_height = self.get_level_list_height();
                // C++ :2887-2891 — fold the level list of the current pack.
                self.pack_change_offset =
                    (self.pack_change_offset + ft * self.scroll_speed).min(list_height);
                // C++ :2894-2895 — scroll the level list to show the current level.
                self.calc_pack_change_scroll_fold(list_height);
                if self.pack_change_offset < list_height {
                    return;
                }
                // C++ :2900-2906 — switch pack + set the stretch info.
                self.change_pack();
                self.pack_change_offset = self.get_level_list_height();
                self.calc_scroll_speed();
                self.pack_change_state = PackChange::Stretching;
            }
            PackChange::Stretching => {
                // C++ :2912-2914 — stretch the level list of the new pack.
                self.pack_change_offset =
                    (self.pack_change_offset - ft * self.scroll_speed).max(0.0);
                // C++ :2917-2918 — scroll to show the current level.
                self.calc_pack_change_scroll_stretch(self.get_level_list_height());
                if self.pack_change_offset > 0.0 {
                    return;
                }
                // C++ :2923-2926 — the fold is over.
                self.pack_change_offset = 0.0;
                self.adjust_levels_offset();
                self.pack_change_state = PackChange::Rest;
            }
        }
        // C++ :2930-2932 — make sure there is no empty space above the
        // first element of the level list.
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.y_offset = d.y_offset.min(0.0);
    }

    /// C++ `getLevelListHeight()` (MenuGame.cpp:4389-4395) — the level
    /// list's height in the selection layout: `levelLabelHeight * (
    /// focusHeld ? 1 : levelCount) + slctFrameSize` (the focused list
    /// shows ONE collapsed row — the draw side's `...` rows, :5018-5138).
    #[must_use]
    pub fn get_level_list_height(&self) -> f32 {
        let drawer = if self.favorites_active {
            &self.fav_slct
        } else {
            &self.lvl_slct
        };
        let layout = self.level_selection_layout();
        let rows = if self.focus_held {
            1.0
        } else {
            drawer.level_data_ids.len() as f32
        };
        layout.level_label_height * rows + layout.slct_frame_size
    }

    /// C++ `getLevelSelectionHeight()` (MenuGame.cpp:4315-4325) — the whole
    /// selection's height (pack labels + level list + the pack borders). No
    /// C++ call site (the method is declared and defined but never invoked
    /// in MenuGame.cpp — ported for API parity; the scroll machinery
    /// consumes `getLevelListHeight` instead).
    #[must_use]
    pub fn get_level_selection_height(&self) -> f32 {
        let layout = self.level_selection_layout();
        let pack_count = self.get_selectable_pack_infos_size() as f32;
        let pack_idx = if self.favorites_active {
            self.fav_slct.pack_idx
        } else {
            self.lvl_slct.pack_idx
        };
        let border = if pack_idx != self.get_selectable_pack_infos_size() as i32 - 1 {
            2.0
        } else {
            1.0
        };
        layout.pack_label_height * pack_count + self.get_level_list_height()
            - self.pack_change_offset
            + border * layout.slct_frame_size
    }

    /// C++ `updateLevelSelectionDrawingParameters` (MenuGame.cpp:4298-4312)
    /// as a pure function of the selection font's probed heights — the
    /// scroll machinery's layout constants (`LevelSelectionLayout::new`).
    /// The C++ recomputes them on `refreshCamera` (the port's
    /// `refresh_if_stale`); the heights are the `char_sizes`-keyed probes
    /// (the (b) menu-draw lane caches the same values per refresh — this
    /// computes them on demand in oh-menu, where the oracle is cheap).
    #[must_use]
    pub fn level_selection_layout(&self) -> crate::layout::LevelSelectionLayout {
        crate::layout::LevelSelectionLayout::new(
            self.font_height(19.0),
            self.font_height(self.char_sizes.selection_big),
            self.font_height(self.char_sizes.selection_small),
        )
    }

    /// C++ `calcScrollSpeed()` (MenuGame.cpp:4397-4402) — the level-count
    /// scaling: long lists (more than 16 levels) scroll faster.
    pub fn calc_scroll_speed(&mut self) {
        let drawer = if self.favorites_active {
            &self.fav_slct
        } else {
            &self.lvl_slct
        };
        self.scroll_speed =
            crate::layout::BASE_SCROLL_SPEED * (drawer.level_data_ids.len() as f32 / 16.0).max(1.0);
    }

    /// C++ `scrollLevelListToTargetY(ft)` (MenuGame.cpp:4578-4598) — the
    /// level-list scroll easing: `YOffset` approaches `YScrollTo` at
    /// `scrollSpeed` units/s, clamped to land exactly on the target, with
    /// the `Utils::epsilon` arrival gate (a sub-epsilon distance is treated
    /// as arrived — the C++ `std::abs(YOffset − YScrollTo) <= epsilon`).
    pub(crate) fn scroll_level_list_to_target_y(&mut self, ft: f32) {
        let (y_offset, y_scroll_to) = if self.favorites_active {
            (self.fav_slct.y_offset, self.fav_slct.y_scroll_to)
        } else {
            (self.lvl_slct.y_offset, self.lvl_slct.y_scroll_to)
        };
        if (y_offset - y_scroll_to).abs() <= crate::layout::EPSILON {
            return;
        }
        let speed = self.scroll_speed;
        let new_offset = if y_offset < y_scroll_to {
            // C++ :4585-4591 — `YOffset += ft * scrollSpeed; if (>= target)
            // YOffset = target;`
            (y_offset + ft * speed).min(y_scroll_to)
        } else {
            // C++ :4593-4597 — the negative-`ft`-direction twin.
            (y_offset - ft * speed).max(y_scroll_to)
        };
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.y_offset = new_offset;
    }

    /// C++ `checkWindowTopScroll` / `checkWindowTopScrollWithResult`
    /// (MenuGame.cpp:4604-4627) — one ported function: the action fires
    /// iff `-scroll` is STRICTLY greater than the active drawer's
    /// `YOffset`; the `WithResult` variant's return value is the `bool`
    /// (the callers' `if(!checkWindowTopScrollWithResult(...))` tails).
    /// `with_offset` picks the C++'s two lambdas: `YScrollTo = target`
    /// (`false`, the level-change scroll) or `YScrollTo = YOffset = target`
    /// (`true`, the pack-change scroll).
    fn check_window_top_scroll(&mut self, scroll: f32, with_offset: bool) -> bool {
        let target = -scroll;
        let fired = if self.favorites_active {
            target > self.fav_slct.y_offset
        } else {
            target > self.lvl_slct.y_offset
        };
        if !fired {
            return false;
        }
        if self.favorites_active {
            self.fav_slct.y_scroll_to = target;
            if with_offset {
                self.fav_slct.y_offset = target;
            }
        } else {
            self.lvl_slct.y_scroll_to = target;
            if with_offset {
                self.lvl_slct.y_offset = target;
            }
        }
        true
    }

    /// C++ `checkWindowBottomScroll` / `checkWindowBottomScrollWithResult`
    /// (MenuGame.cpp:4629-4652) — the bottom-boundary twin: the action
    /// fires iff `h − scroll` is STRICTLY less than `YOffset`.
    fn check_window_bottom_scroll(&mut self, scroll: f32, with_offset: bool) -> bool {
        let target = self.h - scroll;
        let fired = if self.favorites_active {
            target < self.fav_slct.y_offset
        } else {
            target < self.lvl_slct.y_offset
        };
        if !fired {
            return false;
        }
        if self.favorites_active {
            self.fav_slct.y_scroll_to = target;
            if with_offset {
                self.fav_slct.y_offset = target;
            }
        } else {
            self.lvl_slct.y_scroll_to = target;
            if with_offset {
                self.lvl_slct.y_offset = target;
            }
        }
        true
    }

    /// C++ `calcLevelChangeScroll(dir)` (MenuGame.cpp:4404-4461) — the
    /// level-up/down (`dir = ±2`) scroll target: keep the two neighbouring
    /// level labels (or the neighbouring pack labels at the pack's
    /// top/bottom) on screen. Runs on `YScrollTo` only (the C++ lambdas
    /// never touch `YOffset`) — the easing (`scrollLevelListToTargetY`) does
    /// the approach on the next frames.
    pub(crate) fn calc_level_change_scroll(&mut self, dir: i32) {
        // C++ :4406 — the level-change scroll runs at the base speed.
        self.scroll_speed = crate::layout::BASE_SCROLL_SPEED;
        let layout = self.level_selection_layout();
        let (current_index, pack_idx, size) = if self.favorites_active {
            (
                self.fav_slct.current_index,
                self.fav_slct.pack_idx,
                self.fav_slct.level_data_ids.len() as i32,
            )
        } else {
            (
                self.lvl_slct.current_index,
                self.lvl_slct.pack_idx,
                self.lvl_slct.level_data_ids.len() as i32,
            )
        };
        // C++ :4411-4417 — the favorites list pretends to be a single
        // pack at index 0.
        let actual_pack_idx = if self.is_favorite_levels() {
            0
        } else {
            pack_idx
        };

        if dir < 0 {
            let scroll = if current_index < 2 {
                // C++ :4419-4424 — approaching the top of the pack: show
                // the first level label + the next pack label, or two
                // previous pack labels.
                layout.pack_label_height * (actual_pack_idx + 1 - (2 - current_index)) as f32
            } else {
                // C++ :4425-4430 — the two previous level labels.
                layout.pack_label_height * (actual_pack_idx + 1) as f32
                    + layout.level_label_height * (current_index + dir) as f32
                    + layout.slct_frame_size
            };
            // C++ :4432-4434.
            self.check_window_top_scroll(scroll, false);
            return;
        }

        // C++ :4438-4449 — approaching the bottom of the pack: the last
        // level label + the next pack label, or two next pack labels.
        let scroll = if current_index >= size - 2
            && actual_pack_idx != self.get_selectable_pack_infos_size() as i32 - 1
        {
            layout.pack_label_height
                * (actual_pack_idx + 1 + (2 - (size - 1 - current_index))) as f32
                + layout.level_label_height * size as f32
                + 3.0 * layout.slct_frame_size
        } else {
            // C++ :4450-4456 — the two next level labels.
            layout.pack_label_height * (actual_pack_idx + 1) as f32
                + layout.level_label_height * (current_index + dir + 1).min(size) as f32
                + 2.0 * layout.slct_frame_size
        };
        // C++ :4458-4460.
        self.check_window_bottom_scroll(scroll, false);
    }

    /// C++ `calcPackChangeScrollFold(mLevelListHeight)` (MenuGame.cpp:
    /// 4463-4479) — the fold animation's per-frame scroll: as the bottom
    /// of the level list leaves the screen, the offset is set to the list's
    /// height (the fold point) and the drawer jumps to it (`YScrollTo =
    /// YOffset = target` + `packChangeOffset = mLevelListHeight`).
    pub(crate) fn calc_pack_change_scroll_fold(&mut self, list_height: f32) {
        // C++ :4464-4466 — `-2` (the "jump to the last level of the
        // previous pack" up-wrap) is handled by the target pack's own
        // stretch, not the fold.
        if self.pack_change_direction == -2 {
            return;
        }
        let layout = self.level_selection_layout();
        let (pack_idx, offset) = if self.favorites_active {
            (self.fav_slct.pack_idx, self.pack_change_offset)
        } else {
            (self.lvl_slct.pack_idx, self.pack_change_offset)
        };
        // C++ :4469-4472 — the top of the current pack label + the fold.
        let scroll = layout.pack_label_height * pack_idx as f32
            + layout.slct_frame_size
            + (layout.pack_label_height + layout.slct_frame_size + list_height - offset).max(0.0);
        if self.check_window_top_scroll(scroll, true) {
            // C++ :4474-4478 — the pack-change offset snaps to the list
            // height in the SAME action as the drawer jump.
            self.pack_change_offset = list_height;
        }
    }

    /// C++ `calcPackChangeScrollStretch(mLevelListHeight)` (MenuGame.cpp:
    /// 4481-4561) — the stretch animation's per-frame scroll: the new pack's
    /// label (and its levels, as they stretch in) must stay on screen;
    /// "the bottom must prevail" (the C++ comment) where both bounds
    /// disagree.
    pub(crate) fn calc_pack_change_scroll_stretch(&mut self, list_height: f32) {
        let layout = self.level_selection_layout();
        let pack_idx = if self.favorites_active {
            self.fav_slct.pack_idx
        } else {
            self.lvl_slct.pack_idx
        };
        let offset = self.pack_change_offset;
        let h = self.h;
        let pack_count = self.get_selectable_pack_infos_size() as i32;

        if self.pack_change_direction == -2 {
            // C++ :4489-4491 — the last pack has no "next pack": special
            // treatment after this branch.
            if pack_idx != pack_count - 1 {
                // C++ :4496-4499 — the top of the current pack label +
                // the bottom of the next pack label.
                let scroll_top = layout.pack_label_height * pack_idx as f32;
                let scroll_bottom = scroll_top
                    + 2.0 * (layout.pack_label_height + layout.slct_frame_size)
                    + (list_height - offset).max(0.0);

                // C++ :4506-4519 — the `specialAction`: once the pack label
                // is outside the draw window the standing-still list has no
                // reference point, so cut the animation short (reset the
                // offset + jump straight to the final position).
                let target = h - scroll_bottom;
                let fired = if self.favorites_active {
                    target < self.fav_slct.y_offset
                } else {
                    target < self.lvl_slct.y_offset
                };
                if fired {
                    let y_offset = if self.favorites_active {
                        self.fav_slct.y_offset
                    } else {
                        self.lvl_slct.y_offset
                    };
                    if scroll_top < -y_offset {
                        // C++ :4511-4517 — the short-cut.
                        self.pack_change_offset = 0.0;
                        let t = h
                            - (scroll_top
                                + 2.0 * (layout.pack_label_height + layout.slct_frame_size)
                                + list_height.max(0.0));
                        if self.favorites_active {
                            self.fav_slct.y_scroll_to = t;
                            self.fav_slct.y_offset = t;
                        } else {
                            self.lvl_slct.y_scroll_to = t;
                            self.lvl_slct.y_offset = t;
                        }
                    } else if self.favorites_active {
                        self.fav_slct.y_scroll_to = target;
                        self.fav_slct.y_offset = target;
                    } else {
                        self.lvl_slct.y_scroll_to = target;
                        self.lvl_slct.y_offset = target;
                    }
                } else {
                    // C++ :4527-4529 — the top must prevail.
                    self.check_window_top_scroll(scroll_top, true);
                }
                return;
            }

            // C++ :4534-4549 — the last pack: top of the (virtual) next
            // pack label + bottom of the level list.
            let scroll_top = layout.pack_label_height * pack_count as f32 + layout.slct_frame_size;
            let scroll_bottom = scroll_top + (list_height - offset).max(0.0);
            // C++ :4543-4546 — the bottom must prevail.
            if !self.check_window_bottom_scroll(scroll_bottom, true) {
                self.check_window_top_scroll(scroll_top, true);
            }
            return;
        }

        // C++ :4553-4560 — the list is shifted to fit all levels in the
        // pack (or the pack label + as many levels as fit).
        let scroll_top = layout.pack_label_height * (pack_idx - 1) as f32;
        let scroll_bottom = scroll_top
            + layout.pack_label_height
            + (layout.pack_label_height + layout.slct_frame_size + list_height - offset).min(h);
        self.check_window_bottom_scroll(scroll_bottom, true);
        self.check_window_top_scroll(scroll_top, true);
    }

    /// C++ `quickPackFoldStretch()` (MenuGame.cpp:4563-4576) — the focus
    /// rising/falling edge's instant re-layout: keep the previous pack
    /// label (top) and the next pack label (bottom) on screen, then reset
    /// the level offsets.
    pub fn quick_pack_fold_stretch(&mut self) {
        let layout = self.level_selection_layout();
        let pack_idx = if self.favorites_active {
            self.fav_slct.pack_idx
        } else {
            self.lvl_slct.pack_idx
        };
        // C++ :4565-4568 — the previous pack label's top + the next pack
        // label's bottom (the collapsed single-row list height —
        // `focusHeld` makes `getLevelListHeight` count one row).
        let scroll_top = layout.pack_label_height * (pack_idx - 1) as f32;
        let scroll_bottom = scroll_top
            + 2.0 * layout.pack_label_height
            + layout.slct_frame_size
            + self.get_level_list_height();
        // C++ :4570-4575.
        self.check_window_bottom_scroll(scroll_bottom, true);
        self.check_window_top_scroll(scroll_top, true);
        self.adjust_levels_offset();
    }

    /// C++ `adjustLevelsOffset()` — reset every level's x-offset, keep the
    /// selected one at `maxOffset`.
    pub fn adjust_levels_offset(&mut self) {
        let n = self.lvl_slct.level_data_ids.len();
        self.lvl_slct.lvl_offsets = vec![0.0; n];
        if let Some(o) = self
            .lvl_slct
            .lvl_offsets
            .get_mut(self.lvl_slct.current_index as usize)
        {
            *o = MAX_OFFSET;
        }
        if !self.fav_slct.lvl_offsets.is_empty() {
            for o in &mut self.fav_slct.lvl_offsets {
                *o = 0.0;
            }
            if let Some(o) = self
                .fav_slct
                .lvl_offsets
                .get_mut(self.fav_slct.current_index as usize)
            {
                *o = MAX_OFFSET;
            }
        }
    }

    // ---- pack-change actions (§11.8) ----

    /// C++ `changePackTo(idx)`.
    pub fn change_pack_to(&mut self, idx: i32) {
        let packs = self.assets.selectable_pack_ids();
        if packs.is_empty() {
            return;
        }
        let n = packs.len() as i32;
        let pidx = crate::layout::mod_range(idx, 0, n);
        let pack_id = packs.get(pidx as usize).cloned().unwrap_or_default();
        let ids = self.assets.level_ids_for_pack(&pack_id);
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.pack_idx = pidx;
        d.level_data_ids = ids;
        self.set_index(0);
        self.reset_level_names_scrolls();
    }

    /// C++ `changePack()`.
    pub fn change_pack(&mut self) {
        let packs = self.assets.selectable_pack_ids();
        if packs.is_empty() {
            return;
        }
        let n = packs.len() as i32;
        let step = if self.pack_change_direction > 0 {
            1
        } else {
            -1
        };
        let fav = self.favorites_active;
        let cur = if fav {
            self.fav_slct.pack_idx
        } else {
            self.lvl_slct.pack_idx
        };
        let pidx = crate::layout::mod_range(cur + step, 0, n);
        let pack_id = packs.get(pidx as usize).cloned().unwrap_or_default();
        let ids = self.assets.level_ids_for_pack(&pack_id);
        let idx = if self.pack_change_direction == -2 {
            ids.len() as i32 - 1
        } else {
            0
        };
        let d = if fav {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.pack_idx = pidx;
        d.level_data_ids = ids;
        self.set_index(idx);
        self.reset_level_names_scrolls();
    }

    /// C++ `changePackQuick(direction)` (MenuGame.cpp:2156-2181, focus-held
    /// pack jump: the wheel's `focusHeld` arm + the `Jid`'s pack-change
    /// binds). After the pack switch, the drawer must show the pack
    /// labels around the new pack: the previous pack label's top prevails,
    /// else the next pack label's bottom (the C++ "the top must prevail" /
    /// "the bottom is outside the boundaries" comments).
    pub fn change_pack_quick(&mut self, direction: i32) {
        // C++ :2157-2160 — the favorites list has no pack labels.
        if self.is_favorite_levels() {
            return;
        }
        self.pack_change_direction = direction;
        self.play_sound_override("beep.ogg");
        self.change_pack();
        self.adjust_levels_offset();

        let layout = self.level_selection_layout();
        let pack_idx = if self.favorites_active {
            self.fav_slct.pack_idx
        } else {
            self.lvl_slct.pack_idx
        };
        // C++ :2168-2170 — the top of the pack label one index before the
        // current one.
        let scroll = layout.pack_label_height * (pack_idx - 1) as f32;
        // C++ :2172-2176 — the top must prevail (`YScrollTo = YOffset =
        // target` — the drawer jumps, no easing).
        if !self.check_window_top_scroll(scroll, true) {
            // C++ :2178-2181 — the bottom of the pack label one index
            // AFTER the current one (clamped to the pack count), plus the
            // first level label + the borders.
            let pack_count = self.get_selectable_pack_infos_size() as i32;
            let scroll = layout.pack_label_height * (pack_idx + 2).min(pack_count) as f32
                + layout.level_label_height
                + 3.0 * layout.slct_frame_size;
            self.check_window_bottom_scroll(scroll, true);
        }
    }

    /// C++ `changePackAction(direction)` (boundary up/down triggers the fold).
    pub fn change_pack_action(&mut self, direction: i32) {
        if self.state != States::LevelSelection
            || self.get_selectable_pack_infos_size() == 1
            || self.pack_change_state != PackChange::Rest
        {
            return;
        }
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.y_scroll_to = d.y_offset;
        self.pack_change_state = PackChange::Folding;
        self.pack_change_direction = direction;
        self.calc_scroll_speed();

        self.touch_delay = 50.0;
        self.play_sound_override("beep.ogg");
    }

    fn reset_level_names_scrolls(&mut self) {
        // C++ `resetLevelNamesScrolls()` — clears the per-label scroll
        // positions (a 13b layout detail, no state effect).
    }

    // ---- difficulty (left/right in level selection) ----

    /// C++ `leftRightActionImpl`'s LevelSelection branch.
    pub fn change_difficulty(&mut self, left: bool) {
        if left {
            self.diff_mult_idx -= 1;
            self.play_sound_override("difficultyMultDown.ogg");
        } else {
            self.diff_mult_idx += 1;
            self.play_sound_override("difficultyMultUp.ogg");
        }
        self.difficulty_bump_effect = DIFFICULTY_BUMP_EFFECT_MAX;
        self.touch_delay = 50.0;
    }
}

/// ASCII punctuation check (C++ `ssvu::isPunctuation`).
fn is_punctuation(c: &char) -> bool {
    matches!(
        c,
        '!' | '"'
            | '#'
            | '%'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | '-'
            | '.'
            | '/'
            | ':'
            | ';'
            | '<'
            | '='
            | '>'
            | '?'
            | '@'
            | '['
            | ']'
            | '^'
            | '_'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
    )
}

// C++ `ssvu::getMod(val, lb, ub)` (3-arg wrap into `[lb, ub)`) is used
// through `crate::layout::mod_range` for the pack-index math.

// ===========================================================================
// MenuGame
// ===========================================================================

/// C++ `hg::MenuGame`.
pub struct MenuGame {
    shared: Rc<RefCell<Shared>>,
    pub welcome_menu: Rc<Menu>,
    pub main_menu: Rc<Menu>,
    pub options_menu: Rc<Menu>,
    pub online_menu: Rc<Menu>,
    pub profile_selection_menu: Rc<Menu>,
}

impl MenuGame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        assets: Rc<dyn MenuAssets>,
        config: Rc<dyn MenuConfig>,
        network: Rc<dyn NetworkClient>,
        audio: Rc<dyn AudioSink>,
        touch: Rc<dyn MenuTouch>,
        joystick: Rc<dyn MenuJoystick>,
        window: Rc<dyn MenuWindow>,
        new_game: Option<NewGameCallback>,
        trigger_refresh: Option<TriggerRefreshCallback>,
        rich_presence: Option<RichPresenceCallback>,
        unlock_achievement: Option<UnlockAchievementCallback>,
        ignore_inputs: Option<IgnoreInputsCallback>,
        font: Rc<dyn crate::layout::FontHeightOracle>,
    ) -> Self {
        let shared = Shared::new(
            assets,
            config,
            network,
            audio,
            touch,
            joystick,
            window,
            new_game,
            trigger_refresh,
            rich_presence,
            unlock_achievement,
            ignore_inputs,
            font.clone(),
        );
        let shared = Rc::new(RefCell::new(shared));

        // C++ ctor: first-time-tips flag.
        {
            let mut s = shared.borrow_mut();
            if s.config().first_time_playing() {
                s.show_first_time_tips = true;
                s.config().set_first_time_playing(false);
            }
        }

        // Build the five menus (closures capture `shared`).
        let menus = menu::build_menus(shared.clone());

        let this = MenuGame {
            shared: shared.clone(),
            welcome_menu: menus.welcome,
            main_menu: menus.main,
            options_menu: menus.options,
            online_menu: menus.online,
            profile_selection_menu: menus.profile_selection,
        };

        // C++ ctor: `refreshCamera()` then pick a background style + setIndex.
        this.refresh_camera();
        this.init_boot_level();

        this
    }

    /// C++ ctor step 8: pick the first selectable pack's first level as the
    /// menu background and `setIndex(0)`. (C++ uses
    /// `pickRandomMainMenuBackgroundStyle` + `setIndex(randomLevel)`; we use
    /// the deterministic first level so the boot is reproducible.)
    fn init_boot_level(&self) {
        let pack_id = {
            let s = self.shared.borrow();
            s.get_nth_selectable_pack_id(0)
        };
        let Some(pack_id) = pack_id else {
            return;
        };
        {
            let mut s = self.shared.borrow_mut();
            s.lvl_slct.pack_idx = 0;
            s.lvl_slct.level_data_ids = s.assets().level_ids_for_pack(&pack_id);
            // Resize lvlOffsets to the max level count across packs (C++ step
            // 10) — coarse: size to the boot pack's level count.
            let max_levels = s
                .assets()
                .selectable_pack_ids()
                .iter()
                .map(|p| s.assets().level_ids_for_pack(p).len())
                .max()
                .unwrap_or(0);
            s.lvl_slct.lvl_offsets = vec![0.0; max_levels];
            s.set_index(0);
        }
    }

    // ---- accessors ----

    #[must_use]
    pub fn state(&self) -> States {
        self.shared.borrow().state
    }

    /// C++ `getWindowWidth()` / `getWindowHeight()` (MenuGame.cpp:
    /// 3350-3357) — the ACTUAL current window size (the render-target /
    /// window size the C++ reads live from its `window` reference — the
    /// space the 9-zone touch grid's third-math and the C++'s
    /// `getFingerDownPositions()` coordinates run in; `seams::MenuWindow`
    /// for the port source + the config-vs-actual divergence this fixes,
    /// issue 025g). NOT the derived layout space (`Shared::w` / `h` —
    /// the C++ `refreshCamera`'s `w` / `h` member output):
    #[must_use]
    pub fn window_size(&self) -> (f32, f32) {
        let s = self.shared.borrow();
        let win = s.window();
        (win.width(), win.height())
    }

    /// The name of the currently selected item in the active menu
    /// (state-machine test assertion helper).
    #[must_use]
    pub fn selected_item_name(&self) -> Option<String> {
        self.current_menu()
            .map(|m| m.get_item().borrow().get_name())
    }

    #[must_use]
    pub fn selected_category(&self) -> Option<String> {
        self.current_menu()
            .map(|m| m.get_category().get_name().to_string())
    }

    #[must_use]
    pub fn shared(&self) -> &Rc<RefCell<Shared>> {
        &self.shared
    }

    #[must_use]
    pub fn config(&self) -> Rc<dyn MenuConfig> {
        self.shared.borrow().config().clone()
    }

    #[must_use]
    pub fn assets(&self) -> Rc<dyn MenuAssets> {
        self.shared.borrow().assets().clone()
    }

    /// C++ `changePackAction(direction)` (the `Tid::NextPack`/
    /// `Tid::PreviousPack` bind bodies, MenuGame.cpp:940-957) → the
    /// pack-selection logic in `Shared`.
    pub fn change_pack_action(&self, direction: i32) {
        self.shared.borrow_mut().change_pack_action(direction);
    }

    /// C++ hardcoded `{{k::F1}}` bind (MenuGame.cpp:1003-1005) + the
    /// `Jid::AddToFavorites` rising edge (MenuGame.cpp:2746-2749):
    /// `addRemoveFavoriteLevel()`.
    pub fn add_remove_favorite_level(&self) {
        self.shared.borrow_mut().add_remove_favorite_level();
    }

    /// C++ hardcoded `{{k::F2}}` bind (MenuGame.cpp:1007-1009) + the
    /// `Jid::FavoritesMenu` rising edge (MenuGame.cpp:2751-2754):
    /// `switchToFromFavoriteLevels()`.
    pub fn switch_to_from_favorite_levels(&self) {
        self.shared.borrow_mut().switch_to_from_favorite_levels();
    }

    /// C++ hardcoded `{{k::F3}}` / `{{k::F4}}` binds (MenuGame.cpp:
    /// 1011-1017): `reloadAssets(false)` — F3 reloads the LEVEL's assets;
    /// `reloadAssets(true)` — F4 reloads the PACK's assets (the handler's
    /// guard — LevelSelection + no dialog + debug mode — is inside,
    /// MenuGame.cpp:3092-3127).
    pub fn reload_assets(&self, reload_entire_pack: bool) {
        self.shared.borrow_mut().reload_assets(reload_entire_pack);
    }

    /// C++ `isEnteringText()` (the input-gate half of the editable
    /// `Tid::Exit` bind, MenuGame.cpp:975-983).
    #[must_use]
    pub fn is_entering_text(&self) -> bool {
        self.shared.borrow().is_entering_text()
    }

    /// C++ `Tid::Escape` `Type::Always` bind body (MenuGame.cpp:965-971):
    /// `if(state != States::MOpts) exitTimer += mFT`. The
    /// `exitTimer > 20 → window.stop()` consumer (MenuGame.cpp:2852-2854)
    /// runs at the `update()` tail (the port's `quit()` — the
    /// `quit_requested` request path the windowed loop consumes; 29b7):
    /// the timer accumulates here, the threshold fires there.
    pub fn add_exit_timer(&self, ft: f32) {
        if self.state() == States::MOpts {
            return;
        }
        self.shared.borrow_mut().exit_timer += ft;
    }

    /// C++ `Tid::Escape` `Type::Always` bind's off half (MenuGame.cpp:971):
    /// `exitTimer = 0`.
    pub fn reset_exit_timer(&self) {
        self.shared.borrow_mut().exit_timer = 0.0;
    }

    /// C++ `getCurrentMenu()` (state → menu).
    #[must_use]
    pub fn current_menu(&self) -> Option<Rc<Menu>> {
        let state = self.state();
        match state {
            States::SMain => Some(self.main_menu.clone()),
            States::MOpts => Some(self.options_menu.clone()),
            States::MOnline => Some(self.online_menu.clone()),
            States::SLPSelectBoot | States::SLPSelect => Some(self.profile_selection_menu.clone()),
            _ => None,
        }
    }

    /// C++ `isInMenu()`.
    #[must_use]
    pub fn is_in_menu(&self) -> bool {
        self.current_menu().is_some()
    }

    /// C++ `refreshCamera()` (§11.11) — recompute `w`/`h`, aspect flag,
    /// background skew and the per-font char sizes; reset all category
    /// offsets to 0.
    ///
    /// issue 025g — the C++'s view math runs on `getWindowWidth()` /
    /// `getWindowHeight()` (MenuGame.cpp:3131-3136 —
    /// `window.getRenderWindow().getSize()`, the ACTUAL current render
    /// target — the desktop's requested physical size, the Android
    /// native window's screen size), NOT on `Config::getSizeX()` /
    /// `getSizeY()`: on the phone the window is 1530×678 while the
    /// config resolution is 1024×768-ish, so the config-derived space
    /// the port used to read put the 9-zone grid's thirds in the wrong
    /// place (a center tap landed in the "right" band — the 025g device
    /// finding). The desktop is unchanged: the window matches the
    /// config resolution × pixelMult there, and the `refresh_view`
    /// math is scale-invariant under a uniform factor (the C++'s
    /// `backgroundCamera` / `overlayCamera` derivation, MenuGame.cpp:
    /// 3138-3159, is applied to these values by the draw bridge —
    /// `oh_app::menu_draw::refresh_if_stale`).
    pub fn refresh_camera(&self) {
        let (size_x, size_y) = {
            let s = self.shared.borrow();
            let win = s.window();
            (win.width(), win.height())
        };
        let (w, h, four_by_three, skew) = refresh_view(size_x, size_y);
        let sizes = char_sizes(four_by_three);
        for m in [
            &self.welcome_menu,
            &self.main_menu,
            &self.options_menu,
            &self.online_menu,
            &self.profile_selection_menu,
        ] {
            for c in m.get_categories() {
                *c.get_offset_mut() = 0.0;
            }
        }
        let mut s = self.shared.borrow_mut();
        s.w = w;
        s.h = h;
        s.four_by_three = four_by_three;
        s.bg_skew_y = skew;
        s.char_sizes = sizes;
    }

    // ---- boot (C++ `init` / `loadCommandLineLevel` / `returnToLevelSelection`) ----

    /// C++ `init(mError)` (MenuGame.cpp:791-811):
    ///
    /// ```cpp
    /// steamManager.set_rich_presence_in_menu();      // issue 20 (absent)
    /// steamManager.update_hardcoded_achievements();  // issue 20 (absent)
    /// discordManager.set_rich_presence_in_menu();    // issue 21 (absent)
    /// audio.stopMusic();
    /// audio.stopSounds();
    /// if(error) playSoundOverride("error.ogg"); else playSoundOverride("select.ogg");
    /// ```
    ///
    /// The Steam/Discord lines are their null equivalents (the managers do
    /// not exist in this port — issues 20/21). The C++'s commented-out
    /// `Online::setForceLeaderboardRefresh(true)` stays unported (dead in
    /// the C++).
    pub fn init(&self, error: bool) {
        let mut s = self.shared.borrow_mut();
        s.audio.stop_all(); // C++ `audio.stopMusic(); audio.stopSounds();`
        s.play_sound_override(if error { "error.ogg" } else { "select.ogg" });
    }

    /// C++ `init(mError, pack, level)` (MenuGame.cpp:813-819): runs
    /// `init(error)` then `loadCommandLineLevel(pack, level)` — the
    /// `-p -l` boot (`main.cpp:510`). Returns the C++ `loadCommandLineLevel`
    /// result (`true` = the level was found and `playSelectedLevel` fired
    /// — the boot STARTS the level; `false` = one of the C++'s early
    /// returns, no game state set).
    ///
    /// (Rust has no overloading: the C++'s `init(bool, string, string)`
    /// overload lands under this name.)
    pub fn init_command_line_level(&self, error: bool, pack: &str, level: &str) -> bool {
        self.init(error);
        self.load_command_line_level(pack, level)
    }

    /// C++ `loadCommandLineLevel(pack, level)` (MenuGame.cpp:1688-1780),
    /// ported over the `MenuAssets` seam. The C++ `lvlSlct.levelDataIds`
    /// is a POINTER into `assets.getLevelIdsByPack`; the port's drawer owns
    /// the copy (the seam returns an owned `Vec`), as in every other
    /// drawer port. The C++ channel name (sic `Menugame`) is kept.
    ///
    /// The C++'s LAST statement is `playSelectedLevel();` (:1778) — the
    /// `-p -l` boot STARTS the level immediately ("to immediately run
    /// level", the C++'s own `-p`/`-l` doc, main.cpp:96-97,:104-105);
    /// the port fires the same `new_game` seam (`play_selected_level`,
    /// the C++ `fnHGNewGame` — `hg.newGame(...)` + the game state switch,
    /// main.cpp:442-449). The failure paths (the early returns) never
    /// reach it, exactly as in the C++.
    pub fn load_command_line_level(&self, pack: &str, level: &str) -> bool {
        // C++ MenuGame.cpp:1695-1710: scan `getPackDatas()` for the pack
        // whose NAME matches the argument.
        let pack_id = self.shared.borrow().assets().find_pack_id_by_name(pack);
        let Some(pack_id) = pack_id else {
            println!(
                "[hg::Menugame::MenuGame()] Invalid pack name '{pack}' command line parameter, aborting boot level load"
            );
            return false;
        };

        // C++ MenuGame.cpp:1713-1719.
        if !self.shared.borrow().assets().pack_has_levels(&pack_id) {
            println!(
                "[hg::Menugame::MenuGame()] Pack '{pack}' has no levels, aborting boot level load"
            );
            return false;
        }

        // C++ MenuGame.cpp:1722-1755: find the menu pack index and the
        // level's index in the pack's level list.
        let level_id = format!("{pack_id}_{level}");
        let (packs, levels_list) = {
            let s = self.shared.borrow();
            (
                s.assets().selectable_pack_ids(),
                s.assets().level_ids_for_pack(&pack_id),
            )
        };
        // Both lookups are against the SAME pack's list, so `zip` (both
        // `Some`) is exactly the C++'s nested-loop break-on-match.
        let found = packs
            .iter()
            .position(|p| p == &pack_id)
            .zip(levels_list.iter().position(|l| l == &level_id));
        let Some((i, pos)) = found else {
            println!(
                "[hg::Menugame::MenuGame()] Invalid level name '{level}' command line parameter, aborting boot level load"
            );
            return false;
        };
        {
            let mut s = self.shared.borrow_mut();
            s.lvl_slct.pack_idx = i as i32;
            s.lvl_slct.level_data_ids = levels_list.clone();
            s.set_index(pos as i32);
        }

        // C++ MenuGame.cpp:1756-1773 — "Do the sequence of actions user
        // would have to do manually to get to the desired level".
        self.shared.borrow_mut().play_locally(); // go to profile selection screen
        if self.state() == States::ETLPNew {
            // C++ MenuGame.cpp:1760-1765. Verbatim: `playLocally` only ever
            // sets `ETLPNewBoot` / `SLPSelectBoot`, so this branch is
            // unreachable in the C++ too — kept for verbatimness.
            println!(
                "[hg::Menugame::MenuGame()] No player profiles exist, aborting boot level load"
            );
            return false;
        }
        // C++ MenuGame.cpp:1769-1770: `enteredStr =
        // assets.getLocalProfileNames()[0]; assets.pSetCurrent(enteredStr);`
        // — the C++ `[0]` is UB on an empty list (a shipped `_RELEASE` has
        // no `Profiles/` folder, so the empty case is the reachable one);
        // the port takes the first-or-empty instead of crashing.
        let first_profile = self
            .shared
            .borrow()
            .assets()
            .profile_names()
            .into_iter()
            .next()
            .unwrap_or_default();
        {
            let mut s = self.shared.borrow_mut();
            s.assets().profile_set_current(&first_profile);
            s.change_state_to(States::SMain);
            s.reset_level_names_scrolls(); // C++ `resetNamesScrolls()`
            s.change_state_to(States::LevelSelection);
        }
        // C++ MenuGame.cpp:1778 — the tail `playSelectedLevel();`: the
        // boot starts the level now (the C++'s `fnHGNewGame` — the
        // `new_game` seam's `hg.newGame(...)` + the game-state switch,
        // main.cpp:442-449).
        self.shared.borrow_mut().play_selected_level();
        true
    }

    /// C++ `returnToLevelSelection()` (MenuGame.cpp:3365-3370):
    ///
    /// ```cpp
    /// adjustLevelsOffset();
    /// lvlDrawer->XOffset = 0.f;
    /// setIgnoreAllInputs(1); // otherwise you go back to the main menu
    /// ```
    ///
    /// `lvlDrawer` is `&lvlSlct` (MenuGame.cpp:241 — the non-favorites
    /// drawer). Called by the C++ `main.cpp:458` `fnGoToMenu` before
    /// `mg->init(error)`.
    pub fn return_to_level_selection(&self) {
        let mut s = self.shared.borrow_mut();
        s.adjust_levels_offset();
        s.lvl_slct.x_offset = 0.0;
        s.set_ignore_all_inputs(1);
    }

    // ---- navigation (§11.8) ----

    /// C++ `upAction()`.
    pub fn up_action(&self) {
        let state = self.state();
        match state {
            States::LevelSelection => {
                let can_act = {
                    let s = self.shared.borrow();
                    s.pack_change_state == PackChange::Rest
                };
                if !can_act {
                    return;
                }
                let focus = self.shared.borrow().focus_held;
                if focus {
                    self.shared.borrow_mut().change_pack_quick(-1);
                    return;
                }
                let prev_idx = {
                    let s = self.shared.borrow();
                    let d = if s.favorites_active {
                        &s.fav_slct
                    } else {
                        &s.lvl_slct
                    };
                    d.current_index - 1
                };
                let one_pack = {
                    let s = self.shared.borrow();
                    s.get_selectable_pack_infos_size() == 1
                };
                let level_count = {
                    let s = self.shared.borrow();
                    let d = if s.favorites_active {
                        &s.fav_slct
                    } else {
                        &s.lvl_slct
                    };
                    d.level_data_ids.len() as i32
                };
                let mut s = self.shared.borrow_mut();
                if one_pack {
                    if prev_idx < 0 {
                        // C++ :1964-1981 — the single-pack top wrap: jump
                        // to the pack's end, recompute the speed, and
                        // scroll the list up to show it (`scroll > h −
                        // YOffset` → `YScrollTo = h − scroll`). The C++'
                        // `levelLabelHeight * (currentIndex + 1)` is the
                        // full list height after the `setIndex(size − 1)`.
                        s.set_index(level_count - 1);
                        s.calc_scroll_speed();
                        let layout = s.level_selection_layout();
                        let scroll = layout.pack_label_height
                            + layout.level_label_height * level_count as f32
                            + 2.0 * layout.slct_frame_size;
                        let h = s.h;
                        let d = if s.favorites_active {
                            &mut s.fav_slct
                        } else {
                            &mut s.lvl_slct
                        };
                        if scroll > h - d.y_offset {
                            d.y_scroll_to = h - scroll;
                        }
                    } else {
                        // C++ :1983-1987.
                        s.set_index(prev_idx);
                        s.calc_level_change_scroll(-2);
                    }
                } else if prev_idx < 0 {
                    // C++ :1990-1994 — `-2` = "go to the previous pack and
                    // skip to the last level of the list".
                    s.change_pack_action(-2);
                } else {
                    // C++ :1996-1999.
                    s.set_index(prev_idx);
                    s.calc_level_change_scroll(-2);
                }
                s.reset_level_names_scrolls();
                s.play_sound_override("beep.ogg");
                s.touch_delay = 50.0;
            }
            States::LoadingScreen => {
                let mut s = self.shared.borrow_mut();
                if s.scrollbar_offset != 0 {
                    s.scrollbar_offset -= 1;
                    s.play_sound_override("beep.ogg");
                    s.touch_delay = 50.0;
                }
            }
            _ => {
                let Some(menu) = self.current_menu() else {
                    return;
                };
                // profile-selection scrollbar math (§11.8 upAction tail).
                {
                    let mut s = self.shared.borrow_mut();
                    if matches!(s.state, States::SLPSelect | States::SLPSelectBoot) {
                        let idx = menu.get_idx();
                        let size = menu.get_items().len() as i32;
                        if idx - 1 < s.scrollbar_offset {
                            let index = crate::layout::mod_range(idx - 1, 0, size);
                            s.scrollbar_offset = (index - (MAX_PROFILES_ON_SCREEN - 1)).max(0);
                        }
                    }
                    if !any_item_enabled(&menu) {
                        return;
                    }
                }
                menu.previous();
                // C++ loops previous() until an enabled item; `menu.previous()`
                // alone is the single step. Re-check for disabled (matches the
                // `do { previous() } while(!enabled)` — apply the loop).
                let mut s = self.shared.borrow_mut();
                let mut guard = 0;
                while !s.is_item_enabled(&menu) && guard < 64 {
                    menu.previous();
                    guard += 1;
                }
                s.play_sound_override("beep.ogg");
                s.touch_delay = 50.0;
            }
        }
    }

    /// C++ `downAction()`.
    pub fn down_action(&self) {
        let state = self.state();
        match state {
            States::LevelSelection => {
                let can_act = {
                    let s = self.shared.borrow();
                    s.pack_change_state == PackChange::Rest
                };
                if !can_act {
                    return;
                }
                let focus = self.shared.borrow().focus_held;
                if focus {
                    self.shared.borrow_mut().change_pack_quick(1);
                    return;
                }
                let next_idx = {
                    let s = self.shared.borrow();
                    let d = if s.favorites_active {
                        &s.fav_slct
                    } else {
                        &s.lvl_slct
                    };
                    d.current_index + 1
                };
                let one_pack = {
                    let s = self.shared.borrow();
                    s.get_selectable_pack_infos_size() == 1
                };
                let level_count = {
                    let s = self.shared.borrow();
                    let d = if s.favorites_active {
                        &s.fav_slct
                    } else {
                        &s.lvl_slct
                    };
                    d.level_data_ids.len() as i32
                };
                let mut s = self.shared.borrow_mut();
                if one_pack {
                    if next_idx > level_count - 1 {
                        // C++ :2056-2070 — the single-pack bottom wrap: skip
                        // to the first level label (`YScrollTo = 0` after the
                        // speed recompute).
                        s.set_index(0);
                        s.calc_scroll_speed();
                        let d = if s.favorites_active {
                            &mut s.fav_slct
                        } else {
                            &mut s.lvl_slct
                        };
                        d.y_scroll_to = 0.0;
                    } else {
                        // C++ :2072-2076.
                        s.set_index(next_idx);
                        s.calc_level_change_scroll(2);
                    }
                } else if next_idx > level_count - 1 {
                    // C++ :2079-2081 — go to the next pack.
                    s.change_pack_action(1);
                } else {
                    // C++ :2083-2086.
                    s.set_index(next_idx);
                    s.calc_level_change_scroll(2);
                }
                s.reset_level_names_scrolls();
                s.play_sound_override("beep.ogg");
                s.touch_delay = 50.0;
            }
            States::LoadingScreen => {
                let mut s = self.shared.borrow_mut();
                let max =
                    s.assets().load_results().error_messages.len() as i32 - MAX_ERRORS_ON_SCREEN;
                if s.scrollbar_offset < max {
                    s.scrollbar_offset += 1;
                    s.play_sound_override("beep.ogg");
                    s.touch_delay = 50.0;
                }
            }
            _ => {
                let Some(menu) = self.current_menu() else {
                    return;
                };
                {
                    let mut s = self.shared.borrow_mut();
                    if matches!(s.state, States::SLPSelect | States::SLPSelectBoot) {
                        let idx = menu.get_idx();
                        let size = menu.get_items().len() as i32;
                        if idx + 1 > MAX_PROFILES_ON_SCREEN - 1 + s.scrollbar_offset {
                            let index = crate::layout::mod_range(idx + 1, 0, size);
                            s.scrollbar_offset = (index - (MAX_PROFILES_ON_SCREEN - 1)).max(0);
                        }
                    }
                    if !any_item_enabled(&menu) {
                        return;
                    }
                }
                menu.next();
                let mut s = self.shared.borrow_mut();
                let mut guard = 0;
                while !s.is_item_enabled(&menu) && guard < 64 {
                    menu.next();
                    guard += 1;
                }
                s.play_sound_override("beep.ogg");
                s.touch_delay = 50.0;
            }
        }
    }

    /// C++ `leftAction()`.
    pub fn left_action(&self) {
        self.left_right_action_impl(true);
    }

    /// C++ `rightAction()`.
    pub fn right_action(&self) {
        self.left_right_action_impl(false);
    }

    /// C++ `leftRightActionImpl(left)`.
    pub fn left_right_action_impl(&self, left: bool) {
        let state = self.state();
        if state == States::SLPSelectBoot {
            self.ok_action();
            return;
        }
        if state == States::LevelSelection {
            self.shared.borrow_mut().change_difficulty(left);
            return;
        }
        let Some(menu) = self.current_menu() else {
            return;
        };
        let can_increase = menu.get_item().borrow().can_increase();
        if !can_increase {
            return;
        }
        let modifier = {
            let s = self.shared.borrow();
            // C++ :1877-1879 — `isKeyPressed(LShift) || isKeyPressed(RShift)
            // || focusHeld || wasFocusHeld` (the Shift half is the app-
            // layer-fed `shift_held`, 29b8c) — two menu-item steps.
            s.shift_held || s.focus_held || s.was_focus_held
        };
        let steps = if modifier { 2 } else { 1 };
        for _ in 0..steps {
            if left {
                menu.decrease();
            } else {
                menu.increase();
            }
        }
        let mut s = self.shared.borrow_mut();
        s.play_sound_override("beep.ogg");
        s.touch_delay = 50.0;
    }

    /// C++ okAction's profile-creation menu-item half (MenuGame.cpp:
    /// 2241-2247): `profiles.create<i::Single>(enteredStr, [this, name]{
    /// assets.pSetCurrent(name); changeFavoriteLevelsToProfile(); })` +
    /// `profiles.sortByName()` — the label is the BARE name (the C++ has
    /// no prefix), the comparator is the category's `sort_by_name` (the
    /// plain byte-wise `<` on the item names — NOT the favorites list's
    /// lowercased-name sort), and the item is pushed AT ITS SORTED
    /// POSITION by the re-sort (the audit's row 6: the created profile
    /// now APPEARS in the menu, at the right spot).
    pub fn add_profile_selection_item(&self, name: &str) {
        let s = self.shared.clone();
        let name_c = name.to_string();
        let cat = self
            .profile_selection_menu
            .get_category_by_name("profile selection");
        let label = name.to_string();
        cat.create_single(&label, move || {
            let mut sh = s.borrow_mut();
            sh.assets().profile_set_current(&name_c);
            sh.change_favorite_levels_to_profile();
        });
        cat.sort_by_name();
    }

    /// C++ `okAction()` (full switch).
    pub fn ok_action(&self) {
        {
            let mut s = self.shared.borrow_mut();
            s.touch_delay = 50.0;
        }
        let state = self.state();
        match state {
            States::ETLPNewBoot | States::ETLPNew => {
                // Bind the result before the arms: the scrutinee's `RefMut`
                // temporary would otherwise stay alive across the arm bodies
                // (the re-entrancy discipline — the arms re-borrow `shared`).
                let created = self.shared.borrow_mut().create_profile_from_entered();
                match created {
                    // C++ :2235-2247 — `pCreate` + `pSetCurrent` + the
                    // favorites rebuild already ran in `Shared`; now the
                    // menu item (29b6) + the re-sort.
                    ProfileCreateResult::Created(name) => {
                        self.add_profile_selection_item(&name);
                        let mut s = self.shared.borrow_mut();
                        if state == States::ETLPNewBoot {
                            // C++ :2249-2252 — the boot arm RETURNS from
                            // okAction (no select.ogg tail).
                            s.play_sound_override("openHexagon.ogg");
                            s.change_state_to(States::SMain);
                            return;
                        }
                        // C++ :2253-2254 — the ETLPNew arm `break`s →
                        // the okAction tail's select.ogg.
                        s.change_state_to(States::SMain);
                    }
                    // C++ :2232 — the duplicate-name `return;` (out of
                    // okAction, WITHOUT the select.ogg tail).
                    ProfileCreateResult::Rejected => return,
                    // C++ :2214 `if(!enteredStr.empty())` guard — the
                    // `break` → the select.ogg tail (the port's guard is
                    // faithful: the C++ skips the whole block too).
                    ProfileCreateResult::Skipped => {}
                }
            }
            States::SLPSelectBoot => {
                let menu = self.current_menu().unwrap();
                let mut s = self.shared.borrow_mut();
                s.play_sound_override("openHexagon.ogg");
                drop(s);
                menu.exec();
                self.shared.borrow_mut().change_state_to(States::SMain);
                return;
            }
            States::SMain => {
                let menu = self.current_menu().unwrap();
                let category_before = menu.get_category().get_name().to_string();
                menu.exec();
                let mut s = self.shared.borrow_mut();
                if s.state == States::LevelSelection {
                    let cur = s.lvl_slct.current_index;
                    s.set_index(cur);
                    s.adjust_levels_offset();
                    return;
                }
                drop(s);
                let cur = self.current_menu();
                let Some(cur) = cur else {
                    return;
                };
                if !scroll_to_enabled_menu_item(&cur) {
                    return;
                }
                let changed = *cur.get_category().get_name() != category_before;
                let mut s = self.shared.borrow_mut();
                if changed {
                    s.menu_half_height = 0.0; // `adjustMenuOffset(true)` (13b)
                }
            }
            States::MOpts => {
                let menu = self.current_menu().unwrap();
                menu.exec();
                let cur = self.current_menu().unwrap();
                if !scroll_to_enabled_menu_item(&cur) {
                    return;
                }
            }
            States::MOnline => {
                let menu = self.current_menu().unwrap();
                menu.exec();
                let cur = self.current_menu().unwrap();
                let _ = scroll_to_enabled_menu_item(&cur);
            }
            States::LevelSelection => {
                let mut s = self.shared.borrow_mut();
                s.reset_level_names_scrolls();
                s.play_selected_level();
            }
            _ => {
                if let Some(menu) = self.current_menu() {
                    menu.exec();
                }
            }
        }
        self.shared.borrow_mut().play_sound_override("select.ogg");
    }

    /// C++ `eraseAction()` (Backspace).
    pub fn erase_action(&self) {
        let state = self.state();
        if self.shared.borrow().is_entering_text() {
            let changed = self.shared.borrow_mut().erase_entered_char();
            if changed {
                self.shared.borrow_mut().play_sound_override("beep.ogg");
            }
            return;
        }
        if state == States::SLPSelect {
            self.erase_profile();
            return;
        }
        if state == States::MOpts && self.is_in_menu() {
            // C++: if the item is a bind control and its `erase()` returns
            // true → beep. Bind controls are a seam (oh-config owns them);
            // the erase here is a no-op unless a bind control reports it.
            let menu = self.current_menu().unwrap();
            let _ = menu; // bind-control erase is a documented seam
        }
    }

    fn erase_profile(&self) {
        let Some(menu) = self.current_menu() else {
            return;
        };
        let name = menu.get_item().borrow().name().to_string();
        let count = menu.get_items().len();
        {
            let s = self.shared.borrow();
            if count <= 1 {
                drop(s);
                self.shared.borrow_mut().play_sound_override("error.ogg");
                self.shared.borrow_mut().show_dialog_box(
                    "YOU CANNOT ERASE THE ONLY REMAINING PROFILE\n\n\
                     PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
                );
                self.shared.borrow_mut().set_ignore_all_inputs(2);
                return;
            }
            if let Some(cur) = s.assets().current_profile_name() {
                if cur == name {
                    drop(s);
                    self.shared.borrow_mut().play_sound_override("error.ogg");
                    self.shared.borrow_mut().show_dialog_box(
                        "YOU CANNOT ERASE THE CURRENTLY IN USE PROFILE\n\n\
                         PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
                    );
                    self.shared.borrow_mut().set_ignore_all_inputs(2);
                    return;
                }
            }
        }
        // C++ `std::remove("Profiles/" + name + ".json")` (MenuGame.cpp:
        // 2421-2431) — the FILE deletion is the menu's job; the store
        // erase (`pRemove`) runs only on success. A missing file is the
        // C++'s `!= 0` branch: log + return (no store erase, no menu-item
        // removal, no beep).
        if !self.shared.borrow().assets().profile_remove_file(&name) {
            println!("[eraseAction()] Error: file Profiles/{name}.json does not exist");
            return;
        }
        self.shared.borrow_mut().assets().profile_remove(&name);
        // Remove the menu item (C++ removes the item from the category).
        menu.get_category().remove();
        self.shared.borrow_mut().play_sound_override("beep.ogg");
    }

    /// C++ `exitAction()`.
    pub fn exit_action(&self) {
        if self.is_in_menu() {
            let cat = self
                .current_menu()
                .unwrap()
                .get_category()
                .get_name()
                .to_string();
            if cat == "main" {
                return;
            }
        }
        self.shared.borrow_mut().play_sound_override("beep.ogg");

        let state = self.state();
        if state == States::SLPSelectBoot {
            self.shared
                .borrow_mut()
                .change_state_to(States::ETLPNewBoot);
            return;
        }
        if state == States::LevelSelection {
            self.shared.borrow_mut().change_state_to(States::SMain);
            return;
        }
        if self.shared.borrow().assets().has_current_profile() {
            if self.is_in_menu() {
                let menu = self.current_menu().unwrap();
                if menu.can_go_back() {
                    menu.go_back();
                } else {
                    self.shared.borrow_mut().change_state_to(States::SMain);
                }
            } else if self.shared.borrow().is_entering_text() {
                self.shared.borrow_mut().change_state_to(States::SMain);
            }
        }
    }

    /// C++ `update(mFT)` — the per-frame menu loop.
    pub fn update(&self, ft: f32) {
        let net = self.shared.borrow().network().clone();
        net.update();

        let menu = self.current_menu();

        // C++ `Joystick::update(Config::getJoystickDeadzone())`
        // (MenuGame.cpp:2643) — the per-frame controller poll through the
        // `MenuJoystick` seam (29j1 — the shared `oh_config::joystick`
        // machine; the game state's
        // `updateInput_UpdateJoystickControls` is the OTHER feeder, the
        // two states are mutually exclusive, so the machine is fed
        // exactly once per frame, exactly like the C++). Cloned out of
        // `shared` so the dispatch below can re-borrow freely.
        let joy = self.shared.borrow().joystick.clone();
        joy.update(self.config().joystick_deadzone());

        {
            let mut s = self.shared.borrow_mut();
            while let Some(ev) = net.poll_event() {
                s.handle_net_event(ev);
            }

            if let Some(cb) = &s.rich_presence() {
                cb();
            }

            // C++ `focusHeld` block (MenuGame.cpp:2645-2679), verbatim —
            // the app-layer keyboard trigger keeps feeding `focus_held`
            // through `set_focus_held` (the C++ `add2StateInput(Focus)`),
            // and the controller's focus button now ORs into it (the C++
            // `Joystick::pressed(Jid::Focus)`, MenuGame.cpp:2650-2653,
            // the `MenuJoystick` seam's `pressed_focus`, 29j1). The rising/
            // falling edge `quickPackFoldStretch()` arms (MenuGame.cpp:
            // 2663/:2669 → :4563) are live now (29b5): the focus-held
            // collapse to one row and the release's stretch both re-layout
            // the pack labels around the current pack.
            if s.state == States::LevelSelection
                && !s.is_favorite_levels()
                && s.pack_change_state == PackChange::Rest
            {
                if !s.focus_held {
                    s.focus_held = joy.pressed_focus();
                }
                // C++ :2657-2664 — focus rising: the list collapses to the
                // focused row (the C++'s `setIndex(0)` only on a non-zero
                // index) and the pack labels re-layout around the current
                // pack.
                if s.focus_held && !s.was_focus_held {
                    if s.lvl_slct.current_index != 0 {
                        s.set_index(0);
                    }
                    s.quick_pack_fold_stretch();
                } else if !s.focus_held && s.was_focus_held {
                    // C++ :2665-2670 — focus released: stretch the level
                    // list back and re-layout the pack labels.
                    s.quick_pack_fold_stretch();
                }
                s.was_focus_held = s.focus_held;
            } else {
                s.was_focus_held = false;
                s.focus_held = false;
            }

            s.touch_delay = (s.touch_delay - ft).max(0.0);
            s.dialog_box_delay = (s.dialog_box_delay - ft).max(0.0);
            s.difficulty_bump_effect = (s.difficulty_bump_effect - ft * 2.0).max(0.0);

            s.process_entered_chars();

            match s.state {
                States::LoadingScreen => s.hexagon_rotation += ft / 100.0,
                States::LevelSelection => s.step_pack_change(ft),
                _ => {}
            }
        }

        if let Some(m) = menu {
            m.update();
        }

        // C++ `MenuGame::update`'s joystick `risingEdge` dispatch
        // (MenuGame.cpp:2737-2784) — the controller's menu control
        // (29j1): pack change, the favorites toggle (29b1), the
        // stick/dpad menu navigation, select / exit. Outside the
        // `borrow_mut` block above: the action methods re-borrow
        // `shared` (the touch-grid note below). The `Jid::Screenshot`
        // arm keeps the C++'s structure with the same documented no-op
        // the keyboard path's `Tid::Screenshot` bind carries (the menu
        // screenshot's draw-lane consumer, menu_draw's
        // `mustTakeScreenshot`-always-false note).
        {
            if joy.rising_edge(oh_config::Jid::NextPack) {
                self.change_pack_action(1);
            } else if joy.rising_edge(oh_config::Jid::PreviousPack) {
                self.change_pack_action(-1);
            }

            // C++ :2746-2749 / :2751-2754 — the two favorites arms are
            // SEPARATE `if`s (both may fire in one frame, the C++'s
            // structure).
            if joy.rising_edge(oh_config::Jid::AddToFavorites) {
                self.add_remove_favorite_level();
            }
            if joy.rising_edge(oh_config::Jid::FavoritesMenu) {
                self.switch_to_from_favorite_levels();
            }

            // TODO(010b-menu-screenshot): C++ `mustTakeScreenshot = true`
            // (MenuGame.cpp:2782-2784) — the menu's screenshot consumer is the
            // unported `MenuGame::draw` lane (the `Tid::Screenshot`
            // keyboard bind's marker).

            if joy.rising_edge_dir(oh_config::Jdir::Left) {
                self.left_action();
            } else if joy.rising_edge_dir(oh_config::Jdir::Right) {
                self.right_action();
            } else if joy.rising_edge_dir(oh_config::Jdir::Up) {
                self.up_action();
            } else if joy.rising_edge_dir(oh_config::Jdir::Down) {
                self.down_action();
            }

            if joy.rising_edge(oh_config::Jid::Select) {
                self.ok_action();
            } else if joy.rising_edge(oh_config::Jid::Exit) {
                self.exit_action();
            }
        }

        // C++ `MenuGame::update` 9-zone touch grid (MenuGame.cpp:2802-2835)
        // — the phone's touch-only menu control (the port of the C++ block
        // verbatim): with exactly ONE finger down, the screen thirds
        // dispatch — the middle band's x-thirds to `leftAction` /
        // `okAction` / `rightAction` (the port's `ok_action` in the
        // entering-text states validates the entered name and creates the
        // profile — the touch submit the system IME's Done key can't
        // deliver, 025f), the top / bottom bands to `upAction` /
        // `downAction`. Finger data is the platform's `MenuTouch` seam
        // (the C++'s `window.getFingerDownCount()` /
        // `getFingerDownPositions()` — window coordinates, the C++'s
        // `getWindowWidth()` / `getWindowHeight()` space — the ACTUAL
        // current window size, issue 025g: the C++'s grid thirds run on
        // `getWindowWidth()` / `getWindowHeight()` (MenuGame.cpp:2804-
        // 2807), the live render-target size, NOT the derived `w` /
        // `h` layout space the port used to return from
        // `window_size()` (the config-vs-actual divergence — the 025g
        // device finding)); headless / desktop feed `NoopTouch` (never
        // fires — the C++ desktop's `Touch::isDown`-always-false
        // equivalent), and the desktop MOUSE is deliberately NOT wired to
        // the zones (the `MouseButtonReleased` exec path would
        // double-fire — the operator's later-step parity, seam note, 025f).
        // Un-gated by state, exactly as the C++ block. Each action sets
        // `touchDelay = 50` itself (the C++'s debounce, e.g.
        // MenuGame.cpp:2198). Outside the `borrow_mut` block above: the
        // action methods re-borrow `shared`.
        let touch = self.shared.borrow().touch().clone();
        // issue 039 s1 — the per-dispatch device diagnostic (the
        // `adb logcat -s OpenHexagon` half; the C++'s grid is per-frame
        // — ONE dispatch per frame while `getFingerDownCount() == 1`
        // holds — so a held finger's dispatch count IS the held-frame
        // count; the line is the count's instrument, issue 026's
        // evidence-path class, the s3 audio-log's twin).
        let grid_log = |zone: &str, p: &oh_math::ssvs::Vec2| {
            eprintln!(
                "[oh-menu] grid dispatch zone={zone} pos=({:.0},{:.0})",
                p.x, p.y
            );
        };
        if touch.finger_down_count() == 1 {
            let (w, h) = self.window_size();
            let w_third = w / 3.0;
            let w_lt = w - w_third;
            let h_third = h / 3.0;
            let h_lt = h - h_third;
            for p in touch.finger_down_positions() {
                if p.y > h_third && p.y < h_lt {
                    if p.x > 0.0 && p.x < w_third {
                        grid_log("left", &p);
                        self.left_action();
                    } else if p.x < w && p.x > w_lt {
                        grid_log("right", &p);
                        self.right_action();
                    } else if p.x > w_third && p.x < w_lt {
                        grid_log("ok", &p);
                        self.ok_action();
                    }
                } else {
                    if p.y < h_third {
                        grid_log("up", &p);
                        self.up_action();
                    } else if p.y > h_lt {
                        grid_log("down", &p);
                        self.down_action();
                    }
                }
            }
        }

        // C++ `backgroundCamera.turn(levelStatus.rotationSpeed * 10.f)` — the
        // menu background spins with the level rotation (a 13b render detail;
        // we advance a coarse accumulator so it is observable).
        // C++ `MenuGame::update`'s hold-Exit-to-quit (MenuGame.cpp:2852-2854):
        // `if(exitTimer > 20) window.stop();` — holding the Exit/Escape trigger
        // for 20 seconds quits the game (`add_exit_timer`'s accumulation is the
        // on-half; `reset_exit_timer` on release zeroes it, so the threshold
        // only fires on a continuous 20s hold). The port routes the C++'s
        // `window.stop()` through the existing quit-request path (`quit()` →
        // `quit_requested`, which the windowed loop consumes at its break
        // condition, windowed.rs `!window.is_open() || ... menu.quit_requested()`)
        // — the port's `window.stop()` equivalent. Strictly greater (the C++'s
        // `> 20`), so exactly 20.0 does not yet quit.
        let mut s = self.shared.borrow_mut();
        s.rotation_speed += 0.0; // rotationSpeed is set by the menu Lua seam
                                 // C++ :2848-2850 — the credits-bar's per-frame cycle:
                                 // `currentCreditsId += mFT` (the texture selection is
                                 // `credits_bar2_id()`, which the draw side consumes — 29b8b).
        s.current_credits_id += ft;
        if s.exit_timer > 20.0 {
            s.quit();
        }
    }

    /// App-layer hook: the C++ `add2StateInput(Focus)` keeps `focusHeld` in
    /// sync with the focus trigger.
    pub fn set_focus_held(&self, held: bool) {
        self.shared.borrow_mut().focus_held = held;
    }

    /// App-layer hook (29b8c): the C++ `leftRightActionImpl`'s Shift
    /// modifier reads the keyboard live (`sf::Keyboard::isKeyPressed(
    /// LShift) || isKeyPressed(RShift)`, MenuGame.cpp:1877-1878); the port
    /// has no window access in oh-menu, so the app layer's per-frame input
    /// snapshot feeds the equivalent (`MenuSlotAdapter::refresh_input`).
    pub fn set_shift_held(&self, held: bool) {
        self.shared.borrow_mut().shift_held = held;
    }

    /// C++ `MenuGame::onEvent(sf::Event::EventType::MouseWheelScrolled)`
    /// handler (MenuGame.cpp:299-356) — the app layer's
    /// `MenuSlotAdapter::refresh_input` drains the window's wheel-event
    /// sink and calls this PER RECORDED DELTA (the C++ `runEvents` runs
    /// the handler per event between polls — `wheelProgress` and the
    /// LevelSelection `YScrollTo` accumulate per event, so a frame's
    /// several wheel events each pass the clamp, the C++'s semantics).
    ///
    /// The C++'s two non-navigation halves are documented seams: the
    /// cursor re-enable (`setMouseCursorVisible(true)` while
    /// `window.hasFocus()`, MenuGame.cpp:302-305) is the S1 mouse-input
    /// seam (the port has no menu mouse state yet), and the bind-capture
    /// gate (MenuGame.cpp:308-316 — `state == MOpts` + the current item's
    /// `isWaitingForBind()`) is the S3 BindControl seam (the port's bind
    /// items never enter a waiting state; capture is owned by oh-config's
    /// `KeyboardBindControl` — `menu.rs` module docs).
    pub fn handle_mouse_wheel(&self, delta: f32) {
        // C++ :304-306 — `if(window.hasFocus()) setMouseCursorVisible(
        // true)`: the wheel re-shows the cursor (29u1). The C++'s
        // `hasFocus` gate is the documented S1 no-menu-mouse-state gap
        // (the handler is only reached in the menu state, so the re-show
        // is unconditional here).
        self.shared()
            .borrow()
            .window()
            .set_mouse_cursor_visible(true);
        if self.state() == States::LevelSelection {
            // C++ :319-321 — `if(focusHeld) changePackQuick(
            // mEvent.mouseWheelScroll.delta > 0 ? -1 : 1);`
            if self.shared().borrow().focus_held {
                self.shared()
                    .borrow_mut()
                    .change_pack_quick(if delta > 0.0 { -1 } else { 1 });
                return;
            }
            // C++ :322-341 — `lvlDrawer->YScrollTo +=
            // mEvent.mouseWheelScroll.delta * 48.f` + the two-sided
            // clamp. The lower bound is NOT a clamp: the C++ comment is
            // the verbatim "Why..." — the `a35_eagerformore` achievement
            // + `error.ogg`, then `YScrollTo = 0` (the jump back to the
            // list's top). A value cannot be both `> 0` and `< -4000`,
            // so the C++'s `if / else if` is this `if / else` with the
            // end-scroll test first (the C++'s call order inside the
            // branch: unlock, sound, then the reset).
            let mut s = self.shared().borrow_mut();
            let end_scroll = {
                let d = if s.favorites_active {
                    &mut s.fav_slct
                } else {
                    &mut s.lvl_slct
                };
                d.y_scroll_to += delta * 48.0;
                d.y_scroll_to < -4000.0
            };
            if end_scroll {
                s.unlock_achievement("a35_eagerformore");
                s.play_sound_override("error.ogg");
                let d = if s.favorites_active {
                    &mut s.fav_slct
                } else {
                    &mut s.lvl_slct
                };
                d.y_scroll_to = 0.0;
            } else {
                let d = if s.favorites_active {
                    &mut s.fav_slct
                } else {
                    &mut s.lvl_slct
                };
                if d.y_scroll_to > 0.0 {
                    d.y_scroll_to = 0.0;
                }
            }
            return;
        }
        // C++ :344-354 — the per-menu index navigation (every non-
        // LevelSelection state: the menus, the boot screens, the profile
        // select): `wheelProgress += delta`; a full line of progress
        // (past ±1) fires `upAction` / `downAction` and resets to 0.
        let mut s = self.shared().borrow_mut();
        s.wheel_progress += delta;
        if s.wheel_progress > 1.0 {
            s.wheel_progress = 0.0;
            drop(s);
            self.up_action();
        } else if s.wheel_progress < -1.0 {
            s.wheel_progress = 0.0;
            drop(s);
            self.down_action();
        }
    }

    /// C++ `game.onEvent(sf::Event::EventType::TextEntered)` handlers
    /// (MenuGame.cpp:265-272 + :491-517) — the FIRST handler feeds the
    /// profile-name entry (`if(mEvent.text.unicode < 128)
    /// enteredChars.emplace_back(…)`); the SECOND is the DIALOG INPUT-
    /// BOX ECHO (29b3): while a dialog box is open AND is an input box,
    /// the printable unicode range (32..126) appends to
    /// `dialogBox.getInput()` — capped at 32 chars — with `beep.ogg`, and
    /// Backspace (unicode 8) pops with `beep.ogg`. The two handlers are
    /// INDEPENDENT in the C++ (both fire on every `TextEntered` — a
    /// Backspace press reaches BOTH: `enteredChars` and the pop), so the
    /// port runs both halves per char. The app layer routes the
    /// window's `TextEntered` fold through the shared entered-text sink;
    /// the menu state's `refresh_input` drain calls this per recorded
    /// char (the once-per-frame equivalent of the C++'s per-event
    /// `runEvents` dispatch — the released-edges drain's docs,
    /// `oh_app::windowed`).
    pub fn handle_text_entered(&self, c: char) {
        let unicode = c as u32;
        let mut s = self.shared.borrow_mut();
        // The 1st `TextEntered` (MenuGame.cpp:265-272):
        // `unicode < 128 → enteredChars`.
        if unicode < 128 {
            s.entered_chars.push(c);
        }
        // The 2nd `TextEntered` (MenuGame.cpp:499-517) — the dialog
        // input-box echo (NOT gated on `ignoreInputs`: it fires exactly
        // while a dialog holds the gate).
        if !s.dialog.empty() && s.dialog.is_input_box() {
            if (32..127).contains(&unicode) {
                if s.dialog.input().len() < 32 {
                    s.play_sound_override("beep.ogg");
                    s.dialog.input_mut().push(c);
                }
            } else if unicode == 8 {
                // C++ :509-516 — Backspace.
                if !s.dialog.input().is_empty() {
                    s.play_sound_override("beep.ogg");
                    s.dialog.input_mut().pop();
                }
            }
        }
    }

    /// C++ `checkCloseBootScreens` (any key/mouse/joypad release on a boot
    /// screen) — LoadingScreen → EpilepsyWarning → `playLocally()`
    /// (→ SLPSelectBoot / ETLPNewBoot).
    pub fn advance_boot_screen(&self) {
        match self.state() {
            States::LoadingScreen => {
                let mut s = self.shared.borrow_mut();
                s.change_state_to(States::EpilepsyWarning);
                s.set_ignore_all_inputs(1);
                s.scrollbar_offset = 0;
            }
            States::EpilepsyWarning => {
                let fbt = self.shared.borrow().four_by_three;
                *self.main_menu.get_category().get_offset_mut() = if fbt { 280.0 } else { 400.0 };
                if let Some(first) = self.main_menu.get_items().into_iter().next() {
                    let mut item = first.borrow_mut();
                    *item.base_mut().offset_mut() = MAX_OFFSET;
                }
                let mut s = self.shared.borrow_mut();
                s.play_locally();
                s.set_ignore_all_inputs(0);
                s.play_sound_override("select.ogg");
            }
            _ => {}
        }
    }

    /// C++ `KeyReleased` / `MouseButtonReleased` /
    /// `JoystickButtonReleased` release dispatch
    /// (MenuGame.cpp:519-614 / :616-671 / :673-728) — issue 025g (the
    /// generic `--ignoreInputs` tail: while `ignoreInputs > 0`, a release
    /// that is NOT a boot-screen close (scenario 1 — the C++
    /// `checkCloseBootScreens`, MenuGame.cpp:360-380; the port's
    /// [`Self::advance_boot_screen`]) and NOT during a dialog delay and
    /// NOT a dialog-box close (scenario 3) DECREMENTS `ignoreInputs`
    /// (`--ignoreInputs`) — the mechanism that unblocks the menu's bind
    /// system after, e.g., `returnToLevelSelection`'s
    /// `setIgnoreAllInputs(1)` on `fnGoToMenu` (main.cpp:456-462)) + 29b3
    /// (the dialog halves, now wired for real): `released_key` carries
    /// the released KEYBOARD key's identity (`Some`) — the C++
    /// `KeyReleased` handler's `mEvent.key.code` — or `None` for a mouse
    /// button / joystick button / finger release (the C++
    /// `MouseButtonReleased` / `JoystickButtonReleased` handlers, which
    /// consult no key identity). The three dialog halves: (a) the Key
    /// handler's `--ignoreInputs` fires only on the close key (or the
    /// Unknown close key = any key, MenuGame.cpp:544-550); (b) the input
    /// box's Escape cancel — `setIgnoreAllInputs(0)` +
    /// `dialogInputState = Nothing` + select.ogg + `clearDialogBox` — the
    /// only way out of a started sequence besides completing it (:
    /// 552-561); (c) the `checkCloseDialogBox` sequence machine (the port's
    /// [`Shared::check_close_dialog_box`]) — the mouse / joystick halves
    /// ignore the input box entirely (`keyToClose != Unknown → return`,
    /// :636-645 / :693-702) and decrement + close an Unknown-key box.
    ///
    /// The C++'s scenario-3 bind-capture half (the
    /// `KeyboardBindControl` key-assignment, MenuGame.cpp:585-598 /
    /// :655-666) is the documented oh-config `BindControl` seam (the
    /// port's options bind-capture UI lives there — `oh-menu`
    /// `menu.rs` module docs).
    pub fn handle_release_edge(&self, released_key: Option<oh_config::Key>) {
        let mut s = self.shared.borrow_mut();
        // All three handlers' head: `if(ignoreInputs == 0) return;`
        if s.ignore_inputs == 0 {
            return;
        }
        // Scenario 1 (the boot screens) is the caller's
        // `advance_boot_screen` (the C++ `checkCloseBootScreens` — its
        // own `--ignoreInputs` gate, MenuGame.cpp:362-380).
        if matches!(s.state, States::LoadingScreen | States::EpilepsyWarning) {
            return;
        }
        // Scenario 2 (MenuGame.cpp:537-541 / :631-634 / :688-691).
        if s.dialog_box_delay > 0.0 {
            return;
        }
        // Scenario 3 — the open dialog box (29b3: the Key / mouse /
        // joystick dialog halves, MenuGame.cpp:544-565 / :636-645 /
        // :693-700).
        if !s.dialog.empty() {
            match released_key {
                Some(key) => {
                    // C++ :544-550 — the `--ignoreInputs` only when the
                    // close key is Unknown (any key) or the released key
                    // IS the close key (Enter on the input boxes).
                    if s.dialog.key_to_close() == DialogKey::Unknown
                        || (s.dialog.key_to_close() == DialogKey::Enter
                            && key == oh_config::Key::ENTER)
                    {
                        s.decrement_ignore_inputs();
                    }
                    // C++ :553-561 — the input box's Escape cancel (the
                    // only way out of a started sequence besides
                    // completing it).
                    if s.dialog.is_input_box() && key == oh_config::Key::ESCAPE {
                        s.set_ignore_all_inputs(0);
                        s.dialog_input_state = DialogInputState::Nothing;
                        s.play_sound_override("select.ogg");
                        s.dialog.clear();
                        return;
                    }
                    // C++ :563 — the sequence machine (the counter gate:
                    // the box only reacts once the counter is 0).
                    s.check_close_dialog_box();
                }
                None => {
                    // C++ :636-645 / :693-702 — the mouse / joystick
                    // halves: the input box (keyToClose == Enter) ignores
                    // pointer releases; an Unknown-key box decrements +
                    // closes.
                    if s.dialog.key_to_close() != DialogKey::Unknown {
                        return;
                    }
                    s.decrement_ignore_inputs();
                    s.check_close_dialog_box();
                }
            }
            return;
        }
        // The generic `--ignoreInputs` tail (all three handlers); the
        // C++'s `if ((--ignoreInputs) == 0)` — `setIgnoreAllInputs(0)`
        // re-syncs the manager flag at 0 (the no-menu / not-MOpts
        // branch, MenuGame.cpp:579-582 / :649-653 / :706-709; the MOpts
        // bind-assignment branch is the documented `BindControl` seam).
        s.decrement_ignore_inputs();
    }

    /// C++ `levelStatus`-level selection accessors (state-machine test helpers).
    #[must_use]
    pub fn diff_mult_idx(&self) -> i32 {
        self.shared.borrow().diff_mult_idx
    }

    #[must_use]
    pub fn selected_level_index(&self) -> i32 {
        self.shared.borrow().lvl_slct.current_index
    }

    /// `lvlSlct.XOffset` — exposed so [`Self::return_to_level_selection`]'s
    /// "force the drawer offset to 0" contract is assertable (the C++ member
    /// is private).
    #[must_use]
    pub fn lvl_slct_x_offset(&self) -> f32 {
        self.shared.borrow().lvl_slct.x_offset
    }

    /// C++ `window->stop()` request flag (the main-menu / welcome "EXIT"
    /// items set `quit_requested`; the C++ calls `window->stop()` directly,
    /// the ported flag is consumed by the windowed loop, issue 15 step
    /// (c)) — exposed so the loop can translate it to
    /// `GameEngine::stop()`.
    #[must_use]
    pub fn quit_requested(&self) -> bool {
        self.shared.borrow().quit_requested
    }

    #[must_use]
    pub fn selected_level_id(&self) -> String {
        let s = self.shared.borrow();
        s.lvl_slct
            .level_data_ids
            .get(s.lvl_slct.current_index as usize)
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn selectable_pack_count(&self) -> usize {
        self.shared.borrow().get_selectable_pack_infos_size()
    }

    #[must_use]
    pub fn current_level_count(&self) -> usize {
        self.shared.borrow().lvl_slct.level_data_ids.len()
    }

    #[must_use]
    pub fn current_pack_id(&self) -> String {
        let s = self.shared.borrow();
        s.get_nth_selectable_pack_id(s.lvl_slct.pack_idx)
            .unwrap_or_default()
    }

    /// C++ `adjustMenuOffset` (§11.13) — set the selected item's offset to
    /// `maxOffset` (coarse; the panel slide is a 13b render detail).
    pub fn adjust_menu_offset(&self, reset: bool) {
        let Some(menu) = self.current_menu() else {
            return;
        };
        if reset {
            *menu.get_category().get_offset_mut() = 0.0;
        }
        let idx = menu.get_idx() as usize;
        let items = menu.get_items();
        for i in &items {
            let mut item = i.borrow_mut();
            *item.base_mut().offset_mut() = 0.0;
        }
        if let Some(item) = items.get(idx) {
            let mut item = item.borrow_mut();
            *item.base_mut().offset_mut() = MAX_OFFSET;
        }
    }
}

/// C++ file-static `anyItemEnabled(menu)`.
#[must_use]
pub fn any_item_enabled(menu: &Rc<Menu>) -> bool {
    for i in menu.get_items() {
        if i.borrow().is_enabled() {
            return true;
        }
    }
    false
}

/// C++ file-static `scrollToEnabledMenuItem(menu)`.
fn scroll_to_enabled_menu_item(menu: &Rc<Menu>) -> bool {
    if !any_item_enabled(menu) {
        return false;
    }
    menu.update();
    let mut guard = 0;
    while !menu.get_item().borrow().is_enabled() && guard < 64 {
        menu.next();
        guard += 1;
    }
    true
}

// ---- small accessors used above that live on Shared ----
impl Shared {
    #[must_use]
    pub fn rich_presence(&self) -> &Option<RichPresenceCallback> {
        &self.rich_presence
    }

    /// Pop the last entered char (returns whether one was removed).
    pub fn erase_entered_char(&mut self) -> bool {
        if self.entered_str.is_empty() {
            return false;
        }
        self.entered_str.pop();
        true
    }

    /// The `isEnabled` probe over the menu's current item (used by the
    /// up/down navigation loops).
    pub fn is_item_enabled(&self, menu: &Rc<Menu>) -> bool {
        menu.get_item().borrow().is_enabled()
    }

    /// C++ `MenuFont::updateHeight` proxy (exposed for layout tests).
    #[must_use]
    pub fn font_height(&self, char_size: f32) -> f32 {
        self.font.height(char_size)
    }

    /// C++ `update()`'s credits-bar texture selection (MenuGame.cpp:
    /// 2848-2850): `getByModIdx(creditsIds, toInt(currentCreditsId / 100))`
    /// (29b8b) — the 0-based index into `CREDITS_BAR_IDS` the draw side's
    /// `creditsBar2` sprite should show this frame (the id changes every
    /// 100 accumulated seconds of frametime). `currentCreditsId` only
    /// accumulates (non-negative), but the `rem_euclid` is the C++
    /// `getMod` (Math.hpp:292) regardless. The port's `MenuDraw::draw`
    /// sets the sprite's frame from this id every frame (29w1: the real
    /// `Assets/creditsBar2{,b,c}.png` textures).
    #[must_use]
    pub fn credits_bar2_id(&self) -> usize {
        let idx = (self.current_credits_id / 100.0) as i32; // C++ toInt: trunc
        idx.rem_euclid(CREDITS_BAR_IDS.len() as i32) as usize
    }
}
