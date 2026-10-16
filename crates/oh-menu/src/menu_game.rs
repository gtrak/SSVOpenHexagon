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

use oh_math::hg::color::Color;
use oh_menu_system::menu::Menu;

use crate::dialog_box::{DBoxDraw, DialogKey, HexagonDialogBox};
use crate::layout::{
    char_sizes, refresh_view, CharSizes, DIFFICULTY_BUMP_EFFECT_MAX, MAX_ERRORS_ON_SCREEN,
    MAX_OFFSET, MAX_PROFILES_ON_SCREEN,
};
use crate::menu;
use crate::seams::{
    AudioSink, IgnoreInputsCallback, LevelInfo, MenuAssets, MenuConfig, MenuTouch, MenuWindow,
    NetEvent, NetworkClient, NewGameCallback, RichPresenceCallback, TriggerRefreshCallback,
};
use crate::states::{DialogInputState, PackChange, States};

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
    pub touch_delay: f32,
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
    ignore_inputs_cb: Option<IgnoreInputsCallback>,
    font: Rc<dyn crate::layout::FontHeightOracle>,
}

impl Shared {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        assets: Rc<dyn MenuAssets>,
        config: Rc<dyn MenuConfig>,
        network: Rc<dyn NetworkClient>,
        audio: Rc<dyn AudioSink>,
        touch: Rc<dyn MenuTouch>,
        window: Rc<dyn MenuWindow>,
        new_game: Option<NewGameCallback>,
        trigger_refresh: Option<TriggerRefreshCallback>,
        rich_presence: Option<RichPresenceCallback>,
        ignore_inputs_cb: Option<IgnoreInputsCallback>,
        font: Rc<dyn crate::layout::FontHeightOracle>,
    ) -> Self {
        // C++ ctor default state is `LoadingScreen`.
        Self {
            state: States::LoadingScreen,
            // C++ ctor BODY `setIgnoreAllInputs(1)` (MenuGame.cpp:357 —
            // "To close the load results with any key"; the member-init
            // `ignoreInputs{0}` at MenuGame.cpp:192 is overwritten by the
            // ctor body before the `onEvent` boot-screen handlers register
            // at :518/:617/:674). The boot screens hold this gate until
            // `checkCloseBootScreens` decrements it back to 0.
            ignore_inputs: 1,
            focus_held: false,
            was_focus_held: false,
            touch_delay: 0.0,
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
            random_tip: [String::new(), String::new()],
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
            window,
            new_game,
            trigger_refresh,
            rich_presence,
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
        // Manager.hpp:78-81). The `Joystick::ignoreAllPresses` half is the
        // unported joystick (NoJoystick seam).
        if let Some(cb) = &self.ignore_inputs_cb {
            cb(presses != 0);
        }
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
    /// state = getLocalProfilesSize() == 0 ? ETLPNewBoot : SLPSelectBoot`.
    pub fn play_locally(&mut self) {
        self.assets
            .profile_set_current(self.assets.current_profile_name().as_deref().unwrap_or(""));
        self.entered_str.clear();
        self.state = if self.assets.profile_count() == 0 {
            States::ETLPNewBoot
        } else {
            States::SLPSelectBoot
        };
    }

    /// C++ `changeFavoriteLevelsToProfile()` — rebuild the favorites drawer
    /// from the current profile's favorite level ids.
    pub fn change_favorite_levels_to_profile(&mut self) {
        let fav = self.assets.current_profile_name().unwrap_or_default();
        // The favorites id list is the current profile's favorite levels.
        // `MenuAssets` exposes favorites through the current profile; the
        // drawer's level list is recomputed on demand.
        let _ = fav;
        // Coarse: favorites list is derived from `assets` when entering the
        // favorites view (see `switchToFromFavoriteLevels`, a 13b detail).
    }

    // ---- profile editing (okAction / eraseAction) ----

    /// C++ okAction's `ETLPNewBoot` / `ETLPNew` profile-creation block.
    /// Returns `true` if a profile was created and the state advanced to
    /// SMain (so the caller should return early).
    pub fn create_profile_from_entered(&mut self) -> bool {
        if self.entered_str.is_empty() {
            return false;
        }
        // Reject duplicate names.
        for name in self.assets.profile_names() {
            if self.entered_str == name {
                self.play_sound_override("error.ogg");
                self.show_dialog_box(
                    "A PROFILE WITH THE SAME NAME ALREADY EXISTS\n\
                     PLEASE ENTER ANOTHER NAME\n\n\
                     PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
                );
                self.set_ignore_all_inputs(2);
                return false;
            }
        }
        let name = self.entered_str.clone();
        self.assets.profile_create(&name);
        self.assets.profile_set_current(&name);
        self.change_favorite_levels_to_profile();
        self.entered_str.clear();
        true
    }

    // ---- playSelectedLevel (§11.8) ----

    /// C++ `playSelectedLevel()` — fires the `fnHGNewGame` seam.
    pub fn play_selected_level(&mut self) {
        let Some(cb) = &self.new_game else {
            return;
        };
        let drawer = &self.lvl_slct;
        let pack_id = self
            .get_nth_selectable_pack_id(drawer.pack_idx)
            .unwrap_or_default();
        let level_id = drawer
            .level_data_ids
            .get(drawer.current_index as usize)
            .cloned()
            .unwrap_or_default();
        let diff_mult = self
            .current_level
            .as_ref()
            .map_or(1.0, |l| l.get_nth_diff_mult(self.diff_mult_idx));
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
    }

    // ---- per-state animation step (§11.9) ----

    /// C++ `update()` `LevelSelection` pack-change fold/stretch machine.
    pub(crate) fn step_pack_change(&mut self, ft: f32) {
        match self.pack_change_state {
            PackChange::Rest => {}
            PackChange::Folding => {
                let list_height = self.get_level_list_height();
                self.pack_change_offset =
                    (self.pack_change_offset + ft * self.scroll_speed).min(list_height);
                if self.pack_change_offset < list_height {
                    return;
                }
                self.change_pack();
                self.pack_change_offset = self.get_level_list_height();
                self.pack_change_state = PackChange::Stretching;
            }
            PackChange::Stretching => {
                self.pack_change_offset =
                    (self.pack_change_offset - ft * self.scroll_speed).max(0.0);
                if self.pack_change_offset > 0.0 {
                    return;
                }
                self.pack_change_offset = 0.0;
                self.adjust_levels_offset();
                self.pack_change_state = PackChange::Rest;
            }
        }
        let d = if self.favorites_active {
            &mut self.fav_slct
        } else {
            &mut self.lvl_slct
        };
        d.y_offset = d.y_offset.min(0.0);
    }

    /// C++ `getLevelListHeight()` (coarse — proportional to the level count;
    /// the exact value needs the level-selection layout constants, slice 13b).
    #[must_use]
    pub fn get_level_list_height(&self) -> f32 {
        let drawer = if self.favorites_active {
            &self.fav_slct
        } else {
            &self.lvl_slct
        };
        30.0 * drawer.level_data_ids.len() as f32
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

    /// C++ `changePackQuick(direction)` (focus-held pack jump).
    pub fn change_pack_quick(&mut self, direction: i32) {
        if self.is_favorite_levels() {
            return;
        }
        self.pack_change_direction = direction;
        self.play_sound_override("beep.ogg");
        self.change_pack();
        self.adjust_levels_offset();
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

    /// C++ `calcScrollSpeed()` (coarse — the base speed; the C++ scales it by
    /// the scroll distance, a 13b layout detail).
    pub fn calc_scroll_speed(&mut self) {
        self.scroll_speed = crate::layout::BASE_SCROLL_SPEED;
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
        window: Rc<dyn MenuWindow>,
        new_game: Option<NewGameCallback>,
        trigger_refresh: Option<TriggerRefreshCallback>,
        rich_presence: Option<RichPresenceCallback>,
        ignore_inputs: Option<IgnoreInputsCallback>,
        font: Rc<dyn crate::layout::FontHeightOracle>,
    ) -> Self {
        let shared = Shared::new(
            assets,
            config,
            network,
            audio,
            touch,
            window,
            new_game,
            trigger_refresh,
            rich_presence,
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

    /// C++ `isEnteringText()` (the input-gate half of the editable
    /// `Tid::Exit` bind, MenuGame.cpp:989-997).
    #[must_use]
    pub fn is_entering_text(&self) -> bool {
        self.shared.borrow().is_entering_text()
    }

    /// C++ `Tid::Escape` `Type::Always` bind body (MenuGame.cpp:965-971):
    /// `if(state != States::MOpts) exitTimer += mFT`. The
    /// `exitTimer > 20 → window.stop()` consumer (MenuGame.cpp:2852) is the
    /// unported `MenuGame::draw` lane (issue 13b) — the timer accumulates.
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
    /// `-p -l` boot (`main.cpp:510`).
    ///
    /// (Rust has no overloading: the C++'s `init(bool, string, string)`
    /// overload lands under this name.)
    pub fn init_command_line_level(&self, error: bool, pack: &str, level: &str) {
        self.init(error);
        self.load_command_line_level(pack, level);
    }

    /// C++ `loadCommandLineLevel(pack, level)` (MenuGame.cpp:1688-1775),
    /// ported over the `MenuAssets` seam. The C++ `lvlSlct.levelDataIds`
    /// is a POINTER into `assets.getLevelIdsByPack`; the port's drawer owns
    /// the copy (the seam returns an owned `Vec`), as in every other
    /// drawer port. The C++ channel name (sic `Menugame`) is kept.
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
                        s.set_index(level_count - 1);
                    } else {
                        s.set_index(prev_idx);
                    }
                } else if prev_idx < 0 {
                    s.change_pack_action(-2);
                } else {
                    s.set_index(prev_idx);
                }
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
                        s.set_index(0);
                    } else {
                        s.set_index(next_idx);
                    }
                } else if next_idx > level_count - 1 {
                    s.change_pack_action(1);
                } else {
                    s.set_index(next_idx);
                }
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
            s.focus_held || s.was_focus_held
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

    /// C++ `okAction()` (full switch).
    pub fn ok_action(&self) {
        {
            let mut s = self.shared.borrow_mut();
            s.touch_delay = 50.0;
        }
        let state = self.state();
        match state {
            States::ETLPNewBoot | States::ETLPNew => {
                let created = self.shared.borrow_mut().create_profile_from_entered();
                if created {
                    let mut s = self.shared.borrow_mut();
                    if state == States::ETLPNewBoot {
                        s.play_sound_override("openHexagon.ogg");
                    }
                    s.change_state_to(States::SMain);
                    return;
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

        {
            let mut s = self.shared.borrow_mut();
            while let Some(ev) = net.poll_event() {
                s.handle_net_event(ev);
            }

            if let Some(cb) = &s.rich_presence() {
                cb();
            }

            // Joystick / focus: `focusHeld` is driven by the app layer via
            // `set_focus_held`; mirror the previous-frame value.
            s.was_focus_held = s.focus_held;

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
        // 025j PROBE (temporary): periodic menu state + every zone dispatch.
        #[cfg(target_os = "android")]
        {
            use std::sync::atomic::{AtomicU64, Ordering};
            static MP: AtomicU64 = AtomicU64::new(0);
            let c = MP.fetch_add(1, Ordering::Relaxed) + 1;
            let (st, ign) = {
                let s = self.shared.borrow();
                (s.state, s.ignore_inputs)
            };
            if c % 30 == 0 {
                eprintln!(
                    "[025j-menu] state={st:?} ignore={ign} fingers={} pos={:?}",
                    touch.finger_down_count(),
                    touch.finger_down_positions()
                );
            }
            if touch.finger_down_count() > 0 && c % 30 != 0 {
                eprintln!(
                    "[025j-menu] TOUCH state={st:?} ignore={ign} pos={:?}",
                    touch.finger_down_positions()
                );
            }
        }
        if touch.finger_down_count() == 1 {
            let (w, h) = self.window_size();
            let w_third = w / 3.0;
            let w_lt = w - w_third;
            let h_third = h / 3.0;
            let h_lt = h - h_third;
            for p in touch.finger_down_positions() {
                if p.y > h_third && p.y < h_lt {
                    if p.x > 0.0 && p.x < w_third {
                        self.left_action();
                    } else if p.x < w && p.x > w_lt {
                        self.right_action();
                    } else if p.x > w_third && p.x < w_lt {
                        self.ok_action();
                    }
                } else {
                    if p.y < h_third {
                        self.up_action();
                    } else if p.y > h_lt {
                        self.down_action();
                    }
                }
            }
        }

        // C++ `backgroundCamera.turn(levelStatus.rotationSpeed * 10.f)` — the
        // menu background spins with the level rotation (a 13b render detail;
        // we advance a coarse accumulator so it is observable).
        let mut s = self.shared.borrow_mut();
        s.rotation_speed += 0.0; // rotationSpeed is set by the menu Lua seam
    }

    /// App-layer hook: the C++ `add2StateInput(Focus)` keeps `focusHeld` in
    /// sync with the focus trigger.
    pub fn set_focus_held(&self, held: bool) {
        self.shared.borrow_mut().focus_held = held;
    }

    /// C++ `game.onEvent(sf::Event::EventType::TextEntered)` handler
    /// (MenuGame.cpp:491-498) — `if(mEvent.text.unicode < 128)
    /// enteredChars.emplace_back(ssvu::toNum<char>(mEvent.text.unicode))`.
    /// The app layer routes the window's `TextEntered` fold through the
    /// shared entered-text sink; the menu state's `refresh_input` drain
    /// calls this per recorded char (the once-per-frame equivalent of the
    /// C++'s per-event `runEvents` dispatch — the released-edges drain's
    /// docs, `oh_app::windowed`).
    pub fn handle_text_entered(&self, c: char) {
        if (c as u32) < 128 {
            self.shared.borrow_mut().entered_chars.push(c);
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
    /// `JoystickButtonReleased` shared release tail
    /// (MenuGame.cpp:576-601 / :644-666 / :694-700) — issue 025g:
    /// while `ignoreInputs > 0` (inputs are special-cased: boot
    /// screens, dialog boxes, bind capture), a release that is NOT a
    /// boot-screen close (scenario 1 — the C++
    /// `checkCloseBootScreens`, MenuGame.cpp:360-380; the port's
    /// [`Self::advance_boot_screen`]) and NOT during a dialog delay and
    /// NOT a dialog-box close (scenario 3) DECREMENTS `ignoreInputs`
    /// (`--ignoreInputs`) — the mechanism that unblocks the menu's bind
    /// system after, e.g., `returnToLevelSelection`'s
    /// `setIgnoreAllInputs(1)` on `fnGoToMenu` (main.cpp:456-462): the
    /// C++'s first release after a game → menu drop takes the menu from
    /// 1 to 0, and subsequent keys work. Without this tail the port held
    /// `ignore_inputs >= 1` forever after such a drop (the on-device
    /// 025g finding: the menu's key binds stayed dead after a
    /// profile-creation → game → `goToMenu` round trip).
    ///
    /// The C++'s scenario-3 bind-capture half (the
    /// `KeyboardBindControl` key-assignment, MenuGame.cpp:586-601 /
    /// :655-666) is the documented oh-config `BindControl` seam (the
    /// port's options bind-capture UI lives there — `oh-menu`
    /// `menu.rs` module docs); the input-box login/registration
    /// `transitionInputSequence` half is the port's
    /// `DialogInputState` flow (not this tail).
    pub fn handle_release_edge(&self) {
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
        // Scenario 2 (MenuGame.cpp:530-532 / :633-635 / :683-685).
        if s.dialog_box_delay > 0.0 {
            return;
        }
        // Scenario 3 (MenuGame.cpp:534-573 / :637-643 / :687-693): a
        // dialog box with an unknown close key closes on any release —
        // the C++ `checkCloseDialogBox`'s `closeBox` (select sound +
        // `clearDialogBox` + `setIgnoreAllInputs(0)`; the C++'s
        // `--ignoreInputs;` before the `closeBox` is subsumed — the
        // close zeroes it).
        if !s.dialog.empty() {
            if s.dialog.key_to_close() != DialogKey::Unknown {
                return;
            }
            if s.dialog_input_state == DialogInputState::Nothing {
                s.play_sound_override("select.ogg");
                s.dialog.clear();
                s.set_ignore_all_inputs(0);
            } else {
                // The C++'s input-sequence `--ignoreInputs;` (the
                // sequence transition itself is the `DialogInputState`
                // flow's, not this tail's).
                let n = s.ignore_inputs - 1;
                s.set_ignore_all_inputs(n);
            }
            return;
        }
        // The generic `--ignoreInputs` tail (all three handlers); the
        // C++'s `if ((--ignoreInputs) == 0)` — `setIgnoreAllInputs(0)`
        // re-syncs the manager flag at 0 (the no-menu / not-MOpts
        // branch, MenuGame.cpp:579-582 / :647-650 / :697-700; the MOpts
        // bind-assignment branch is the documented `BindControl` seam).
        let n = s.ignore_inputs - 1;
        s.set_ignore_all_inputs(n);
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
}
