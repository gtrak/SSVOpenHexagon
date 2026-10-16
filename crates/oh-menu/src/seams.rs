//! Seam traits for `MenuGame` (§11, and the C++ constructor signature).
//!
//! `MenuGame` in C++ takes raw references to `HGAssets`, `Config`,
//! `Audio`, `HexagonClient`, the window, and three `std::function`
//! "hexagon game callbacks (to avoid physical dependency)". This module
//! turns each of those into an explicit, testable seam:
//!
//! - [`NewGameCallback`] / [`TriggerRefreshCallback`] /
//!   [`RichPresenceCallback`] — the `fnHG*` callbacks (the `HexagonGame`
//!   boundary; the *game* supplies these closures).
//! - [`MenuAssets`] — the asset/pack/level/style/profile surface `MenuGame`
//!   reads (`RealMenuAssets` wraps `oh_assets::Assets`; the profile-editor
//!   ops are a seam because `oh-assets` explicitly defers them).
//! - [`MenuConfig`] — the config getters/setters the menus and layout use.
//! - [`NetworkClient`] — the `HexagonClient` online flow. **Issue 017
//!   seam: no fake network behavior lives here.** The default
//!   [`NoopNetwork`] only ever reports `Disconnected` and no events.
//! - [`AudioSink`] — sound playback (`playSoundOverride`).
//!
//! The `fakes` submodule provides `FakeAssets` / `FakeConfig` / `NoopNetwork`
//! / `NoopAudio` so the crate's own tests (and issue 013's state-machine
//! test) can drive `MenuGame` without a display or a real `_RELEASE`.

use std::cell::RefCell;
use std::rc::Rc;

use oh_math::hg::color::Color;

/// `fnHGNewGame(pack, level, firstPlay, difficultyMult, executeLastReplay)` —
/// C++ `std::function<void(const std::string&, const std::string&, bool,
/// float, bool)>`.
pub type NewGameCallback = Box<dyn Fn(&str, &str, bool, f32, bool)>;

/// `fnHGTriggerRefresh(Trigger, int bindID)` — C++
/// `std::function<void(const ssvs::Input::Trigger&, int)>`. The `Trigger` is
/// passed by name (the `oh-config` `Trigger` type) to avoid a hard
/// dependency on the SSVStart trigger model here.
pub type TriggerRefreshCallback = Box<dyn Fn(&str, i32)>;

/// `fnHGUpdateRichPresenceCallbacks()` — C++ `std::function<void()>`.
pub type RichPresenceCallback = Box<dyn Fn()>;

/// `MenuGame::setIgnoreAllInputs`'s `game.ignoreAllInputs(bool)` half
/// (MenuGame.cpp:3389-3402 → `ssvs::Input::Manager::ignoreAllInputs`,
/// Manager.hpp:78-81) — the windowed composition wires it to the menu
/// state's input manager. The C++ `Joystick::ignoreAllPresses` half is the
/// unported joystick (NoJoystick seam).
pub type IgnoreInputsCallback = Box<dyn Fn(bool)>;

/// A level as `MenuGame` consumes it (name/author/style/difficulty for the
/// selection screens and `set_index`).
#[derive(Debug, Clone)]
pub struct LevelInfo {
    pub id: String,
    pub pack_id: String,
    pub name: String,
    pub author: String,
    /// C++ `LevelData.description` (the level-selection left panel).
    pub description: String,
    pub style_id: String,
    pub difficulty_mults: Vec<f32>,
    pub lua_script_path: String,
    /// C++ `LevelData.unscored` (the leaderboard / RANKED gating).
    pub unscored: bool,
    /// C++ `LevelData.validators` / `validatorsWithoutPackId` — keyed by the
    /// EXACT bit pattern of the difficulty mult (floats are not `Eq`/`Hash`
    /// in Rust; the C++ hash likewise depends only on the bits, and every
    /// lookup uses the very float that was inserted, so `to_bits()` keys
    /// are faithful).
    pub validators: std::collections::BTreeMap<u32, String>,
    pub validators_without_pack_id: std::collections::BTreeMap<u32, String>,
    pub music_name: String,
    pub music_author: String,
    pub music_album: String,
}

impl LevelInfo {
    /// C++ `LevelData::getNthDiffMult` (clamped index).
    #[must_use]
    pub fn get_nth_diff_mult(&self, mut index: i32) -> f32 {
        let n = self.difficulty_mults.len() as i32;
        if index < 0 {
            index = 0;
        } else if index >= n {
            index = n - 1;
        }
        self.difficulty_mults[index as usize]
    }

    /// C++ `LevelData::getValidator(diffMult)` (`validators.at(diffMult)`
    /// — the C++ asserts the key exists; the port `expect`s: the map is
    /// built from the same `difficulty_mults` vector the lookup keys on).
    #[must_use]
    pub fn get_validator(&self, diff_mult: f32) -> &str {
        self.validators
            .get(&diff_mult.to_bits())
            .expect("validator for an inserted difficulty mult")
    }

    /// C++ `LevelData::getValidatorWithoutPackId(diffMult)`.
    #[must_use]
    pub fn get_validator_without_pack_id(&self, diff_mult: f32) -> &str {
        self.validators_without_pack_id
            .get(&diff_mult.to_bits())
            .expect("validator (without pack id) for an inserted difficulty mult")
    }
}

/// C++ `hg::PackData` fields the menu draw reads (`name` / `author` /
/// `version` — the level-selection left panel's "PACK" block).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackDataInfo {
    pub name: String,
    pub author: String,
    pub version: i32,
}

/// The `StyleData` fields `MenuGame::draw`'s background pass reads
/// (`MenuGame.cpp:5774-5787`; the C++ reads them from its `styleData`
/// member, which `setIndex` copied from the current level's style,
/// `MenuGame.cpp:2957-2958`).
#[derive(Debug, Clone, PartialEq)]
pub struct StyleBackground {
    /// C++ `styleData.getColors()` (after `computeColors`).
    pub colors: Vec<Color>,
    /// C++ `styleData.bgTileRadius`.
    pub bg_tile_radius: f32,
    /// C++ `styleData.BGRotOff` (degrees).
    pub bg_rot_off: f32,
    /// C++ `styleData.getMainColor()`.
    pub main_color: Color,
    /// C++ `styleData.getCapColorResult()` — `None` when the cap colour
    /// indexes a missing colour (the C++ `drawBackgroundMenu` would
    /// null-deref there; the port draws the cap ring in black, the
    /// `draw_background_menu` default for a missing cap).
    pub cap_color: Option<Color>,
}

/// The asset load results shown on the [`crate::states::States::LoadingScreen`]
/// (§11.16).
#[derive(Debug, Clone, Default)]
pub struct LoadResults {
    pub packs: u32,
    pub levels: u32,
    pub assets: u32,
    pub error_messages: Vec<String>,
}

/// The asset / pack / level / style / profile surface `MenuGame` reads.
pub trait MenuAssets {
    /// Selectable pack ids, in C++ `getSelectablePackInfos()` order.
    fn selectable_pack_ids(&self) -> Vec<String>;
    /// `getLevelIdsByPack` (empty if the pack has no levels).
    fn level_ids_for_pack(&self, pack_id: &str) -> Vec<String>;
    /// `getLevelData` (by level id).
    fn level(&self, level_id: &str) -> Option<LevelInfo>;
    /// `styleData.getColors()` for the level's style, after
    /// `computeColors()` (the C++ `setIndex` copies + recomputes the style).
    fn style_colors(&self, level_id: &str) -> Vec<Color>;
    /// `styleData.getTextColor()` for the level's style.
    fn style_text_color(&self, level_id: &str) -> Color;
    /// C++ `styleData` (the level's style after `computeColors`) — the
    /// fields `MenuGame::draw`'s background pass reads
    /// (`MenuGame.cpp:5774-5787` → `StyleData::drawBackgroundMenu`,
    /// `HGGraphics.cpp:122-141`). `None` when the level or its style is
    /// unknown (the C++ would be a null-deref; the port then draws no
    /// background sectors — the `colors.is_empty()` guard in
    /// `oh_render::graphics::draw_background_menu`).
    fn style_background(&self, level_id: &str) -> Option<StyleBackground>;
    /// C++ `assets.getPackData(packId)` — the `name` / `author` / `version`
    /// the level-selection left panel shows (empty `PackDataInfo` when the
    /// pack id is unknown).
    fn pack_data_info(&self, pack_id: &str) -> PackDataInfo;
    /// `loadInfo` (§11.16 counters + error list).
    fn load_results(&self) -> LoadResults;
    /// `getLocalProfilesSize()` / `getLocalProfileNames()`.
    fn profile_count(&self) -> usize;
    fn profile_names(&self) -> Vec<String>;
    /// `pIsValidLocalProfile()` / `pGetName()`.
    fn has_current_profile(&self) -> bool;
    fn current_profile_name(&self) -> Option<String>;
    /// `getCurrentLocalProfile().isLevelFavorite(levelID)`.
    fn is_level_favorite(&self, level_id: &str) -> bool;
    // ---- profile editor (C++ `p*`; not yet in `oh-assets`, see module note)
    fn profile_set_current(&self, name: &str);
    fn profile_create(&self, name: &str);
    fn profile_remove(&self, name: &str);
    /// `reloadAssets(reloadEntirePack)` (F3/F4, debug only).
    fn reload_assets(&self, entire_pack: bool, current_level_id: &str);
    // ---- command-line boot (C++ `loadCommandLineLevel`, MenuGame.cpp:1688+)
    /// The C++ iterates `assets.getPackDatas()` (an `unordered_map`) and
    /// takes the FIRST entry whose `name` equals `mPack` (MenuGame.cpp:
    /// 1696-1703) — the port mirrors that scan; when two packs share a
    /// name the winner is iteration-order-dependent in BOTH (the C++
    /// `unordered_map` and the port's `HashMap`).
    fn find_pack_id_by_name(&self, name: &str) -> Option<String>;
    /// C++ `assets.packHasLevels(packID)` (MenuGame.cpp:1709).
    fn pack_has_levels(&self, pack_id: &str) -> bool;
    // ---- menu-draw surface (slice 13b)
    /// C++ `assets.getPackIdsWithMissingDependencies()` — the "CURRENT
    /// PROFILE" warning block (`MenuGame.cpp:5804-5816`).
    fn pack_ids_with_missing_dependencies(&self) -> Vec<String>;
    /// C++ `assets.getLocalProfileByName(name)` != nullptr → the total
    /// survival time in the C++ `formatSurvivalTime` semantics (`int` sum
    /// of the per-level `int`-truncated scores, `MenuGame.cpp:3861-3883`);
    /// `None` when the profile does not exist (the C++ `nullptr` — the
    /// survival-time line is then not drawn).
    fn profile_total_survival_time(&self, name: &str) -> Option<i32>;
    /// C++ `assets.getCurrentLocalProfile().getScore(validator)` — the
    /// local personal best (0 when unset, `MenuGame.cpp:5489-5500`).
    fn local_profile_score(&self, validator: &str) -> f32;
}

/// The config surface `MenuGame` reads/writes. Every key maps 1:1 to a C++
/// `Config::` getter/setter (names in comments); defaults match `oh-config`.
pub trait MenuConfig {
    // --- layout / sizing
    /// `Config::getSizeX()` (window px width).
    fn size_x(&self) -> f32;
    /// `Config::getSizeY()` (window px height).
    fn size_y(&self) -> f32;
    /// `Config::getZoomFactor()`.
    fn zoom_factor(&self) -> f32;
    /// `Config::getFirstTimePlaying()` / `setFirstTimePlaying`.
    fn first_time_playing(&self) -> bool;
    fn set_first_time_playing(&self, value: bool);
    /// `Config::getShowLoginAtStartup()` / setter.
    fn show_login_at_startup(&self) -> bool;
    fn set_show_login_at_startup(&self, value: bool);
    /// `Config::getBlackAndWhite()` / setter.
    fn black_and_white(&self) -> bool;
    fn set_black_and_white(&self, value: bool);
    /// `Config::getOfficial()` / setter.
    fn official(&self) -> bool;
    fn set_official(&self, value: bool);
    /// `Config::getDebug()` / setter.
    fn debug(&self) -> bool;
    fn set_debug(&self, value: bool);
    /// `Config::getJoystickDeadzone()` / setter (0..100).
    fn joystick_deadzone(&self) -> f32;
    fn set_joystick_deadzone(&self, value: f32);

    // --- gameplay
    fn auto_restart(&self) -> bool;
    fn set_auto_restart(&self, value: bool);
    fn rotate_to_start(&self) -> bool;
    fn set_rotate_to_start(&self, value: bool);
    fn invincible(&self) -> bool;
    fn set_invincible(&self, value: bool);
    fn timescale(&self) -> f32;
    fn set_timescale(&self, value: f32);
    fn save_last_username(&self) -> bool;
    fn set_save_last_username(&self, value: bool);
    /// `Config::getLastLoginUsername()` — the default login username.
    fn last_login_username(&self) -> String;
    /// `Config::setLastLoginUsername(x)`.
    fn set_last_login_username(&self, value: &str);
    /// `Config::resetConfigToDefaults()`.
    fn reset_config_to_defaults(&self);

    // --- controls / binds
    /// `Config::resetBindsToDefaults()`.
    fn reset_binds_to_defaults(&self);
    /// `refreshBinds()` — re-apply every trigger + `fnHGTriggerRefresh` and
    /// reload joystick binds (a seam; the default re-applies nothing).
    fn refresh_binds(&self);

    // --- resolution
    /// `window->getFullscreen()` (the LAlt+Enter toggle's read half,
    /// MenuGame.cpp:1006-1007; the ported value lives in the config).
    fn fullscreen(&self) -> bool;
    /// `Config::setFullscreen(window, b)`.
    fn set_fullscreen(&self, value: bool);
    /// `Config::setCurrentResolution(w, h)` (the `"<w>x<h>"` items).
    fn set_resolution(&self, w: u32, h: u32);
    /// `Config::setCurrentResolutionAuto(window)` (no-op seam default).
    fn set_resolution_auto(&self);
    /// `sf::VideoMode::getFullscreenModes()` filtered to 32-bit; the C++
    /// groups them by `10.f * w / h`. Empty by default (window-agnostic).
    fn resolution_modes(&self) -> Vec<(u32, u32)>;

    // --- graphics
    fn draw_text_outlines(&self) -> bool;
    fn set_draw_text_outlines(&self, value: bool);
    fn text_padding(&self) -> i32;
    fn set_text_padding(&self, value: i32);
    fn text_scaling(&self) -> f32;
    fn set_text_scaling(&self, value: f32);
    /// Antialiasing level (0 = off, `n` = `n/2` sample multiplier); the C++
    /// item shows `"<n>x"` and adjusts by the `<< 1` / `>> 1` bit rules.
    fn antialiasing(&self) -> u32;
    fn set_antialiasing(&self, value: u32);
    fn darken_uneven_background_chunk(&self) -> bool;
    fn set_darken_uneven_background_chunk(&self, value: bool);
    fn show_key_icons(&self) -> bool;
    fn set_show_key_icons(&self, value: bool);
    fn key_icons_scale(&self) -> f32;
    fn set_key_icons_scale(&self, value: f32);
    fn show_level_info(&self) -> bool;
    fn set_show_level_info(&self, value: bool);
    fn show_timer(&self) -> bool;
    fn set_show_timer(&self, value: bool);
    fn show_status_text(&self) -> bool;
    fn set_show_status_text(&self, value: bool);

    // --- visual fxs
    /// `Config::get3D()` / setter (the "3D effects" toggle).
    fn three_d(&self) -> bool;
    fn set_three_d(&self, value: bool);
    fn shaders(&self) -> bool;
    fn set_shaders(&self, value: bool);
    fn no_pulse(&self) -> bool;
    fn set_no_pulse(&self, value: bool);
    fn no_rotation(&self) -> bool;
    fn set_no_rotation(&self, value: bool);
    fn no_background(&self) -> bool;
    fn set_no_background(&self, value: bool);
    fn flash(&self) -> bool;
    fn set_flash(&self, value: bool);
    fn camera_shake_multiplier(&self) -> f32;
    fn set_camera_shake_multiplier(&self, value: f32);

    // --- player visual fxs
    fn angle_tilt_intensity(&self) -> f32;
    fn set_angle_tilt_intensity(&self, value: f32);
    fn show_player_trail(&self) -> bool;
    fn set_show_player_trail(&self, value: bool);
    fn player_trail_alpha(&self) -> u32;
    fn set_player_trail_alpha(&self, value: u32);
    fn player_trail_scale(&self) -> f32;
    fn set_player_trail_scale(&self, value: f32);
    fn player_trail_decay(&self) -> f32;
    fn set_player_trail_decay(&self, value: f32);
    fn player_trail_has_swap_color(&self) -> bool;
    fn set_player_trail_has_swap_color(&self, value: bool);
    fn show_swap_particles(&self) -> bool;
    fn set_show_swap_particles(&self, value: bool);
    fn show_swap_blinking_effect(&self) -> bool;
    fn set_show_swap_blinking_effect(&self, value: bool);

    // --- fps settings
    fn vsync(&self) -> bool;
    fn set_vsync(&self, value: bool);
    fn limit_fps(&self) -> bool;
    fn set_limit_fps(&self, value: bool);
    fn max_fps(&self) -> u32;
    fn set_max_fps(&self, value: u32);
    fn show_fps(&self) -> bool;
    fn set_show_fps(&self, value: bool);

    // --- audio
    fn no_sound(&self) -> bool;
    fn set_no_sound(&self, value: bool);
    fn no_music(&self) -> bool;
    fn set_no_music(&self, value: bool);
    fn sound_volume(&self) -> u32;
    fn set_sound_volume(&self, value: u32);
    fn music_volume(&self) -> u32;
    fn set_music_volume(&self, value: u32);
    fn music_speed_dm_sync(&self) -> bool;
    fn set_music_speed_dm_sync(&self, value: bool);
    fn music_speed_multiplier(&self) -> f32;
    fn set_music_speed_multiplier(&self, value: f32);
    fn play_swap_ready_sound(&self) -> bool;
    fn set_play_swap_ready_sound(&self, value: bool);

    // --- advanced
    fn use_lua_file_cache(&self) -> bool;
    fn set_use_lua_file_cache(&self, value: bool);
    /// `assets.getLuaFileCache().clear()` (the C++ item is a Single action).
    fn clear_lua_file_cache(&self);
    fn disable_game_rendering(&self) -> bool;
    fn set_disable_game_rendering(&self, value: bool);
}

/// C++ `HexagonClient::State` (the online-menu visibility predicates use
/// these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // `LoggedIn_Ready` mirrors C++ `HexagonClient::State`
pub enum ConnectionState {
    Disconnected,
    Connecting,
    ConnectionError,
    Connected,
    LoggedIn,
    LoggedIn_Ready,
}

/// C++ `HexagonClient::Event` — the events polled in `MenuGame::update`.
#[derive(Debug, Clone)]
pub enum NetEvent {
    ConnectionSuccess,
    ConnectionFailure(String),
    Kicked,
    RegistrationSuccess,
    RegistrationFailure(String),
    LoginSuccess,
    LoginFailure(String),
    LogoutSuccess,
    LogoutFailure(String),
    DeleteAccountSuccess,
    DeleteAccountFailure(String),
    GameVersionMismatch,
    ProtocolVersionMismatch,
}

/// The `HexagonClient` online flow. **Issue 017 seam** — no fake network
/// behavior; the concrete client is supplied by the app layer. The online
/// menu items and `update()` event handling operate purely through this
/// trait.
/// C++ `Database::ProcessedScore` (plus `OwnScore.position`) — the
/// leaderboard entries `drawLevelSelectionLeftSide` renders
/// (`MenuGame.cpp:5581-5676`).
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreEntry {
    /// C++ `ProcessedScore.userName`.
    pub user_name: String,
    /// C++ `ProcessedScore.scoreTimestamp` (unix seconds).
    pub score_timestamp: i64,
    /// C++ `ProcessedScore.scoreValue`.
    pub score_value: f64,
    /// C++ `OwnScore.position` (1-based rank; the top-scores loop passes the
    /// loop index — only `getOwnScore` supplies a stored position).
    pub position: i32,
}

pub trait NetworkClient {
    fn state(&self) -> ConnectionState;
    /// Whether the client has valid RT keys (login/register/delete).
    fn has_rt_keys(&self) -> bool;
    /// C++ `HexagonClient::isLevelSupportedByServer` (HexagonClient.cpp:
    /// 986-990) — membership in the server-synced validator set; the set is
    /// EMPTY while offline (the issue-017 client fills it after
    /// `CONNECT`), so the offline default is `false` — the RANKED badge and
    /// the online leaderboard paths render exactly as the C++ does when
    /// disconnected. Issue 017 overrides this with the real set.
    fn is_level_supported_by_server(&self, _validator: &str) -> bool {
        false
    }
    /// C++ `HexagonClient::getLoginName` (HexagonClient.cpp:953-956) —
    /// `std::optional<std::string>`; `None` when not logged in.
    fn login_name(&self) -> Option<String> {
        None
    }
    // ---- leaderboard cache (C++ `leaderboardCache`, MenuGame.cpp:341;
    // the C++ `Database::LeaderboardCache` lives in `Database.cpp` — issue
    // 017 territory; offline it always reports "no information", which is
    // the default below, so the left panel renders the C++ offline
    // fallbacks: "..." / "PLEASE LOG IN TO LOAD LEADERBOARD") ----
    /// C++ `leaderboardCache->hasInformation(validator)`.
    fn has_score_information(&self, _validator: &str) -> bool {
        false
    }
    /// C++ `leaderboardCache->getScores(validator)` (empty offline).
    fn scores(&self, _validator: &str) -> Vec<ScoreEntry> {
        Vec::new()
    }
    /// C++ `leaderboardCache->getOwnScore(validator)` (`nullptr` offline).
    fn own_score(&self, _validator: &str) -> Option<ScoreEntry> {
        None
    }
    /// C++ `hexagonClient.tryRequestTopScoresAndOwnScore(validator)` +
    /// `leaderboardCache->requestedScores(validator)` — the draw-side
    /// request kick (MenuGame.cpp:5571-5575); a no-op seam offline.
    fn request_scores(&self, _validator: &str) {}
    fn connect(&self);
    fn disconnect(&self);
    fn try_login(&self, username: &str, password: &str);
    fn try_register(&self, username: &str, password: &str);
    fn try_logout(&self);
    fn try_delete_account(&self, password: &str);
    fn poll_event(&self) -> Option<NetEvent>;
    fn update(&self);
}

/// `Audio` seam — `playSoundOverride` / `stopMusic` / `stopSounds`.
pub trait AudioSink {
    fn play_sound(&self, asset_id: &str);
    fn stop_all(&self);
}

// ---------------------------------------------------------------------------
// Test fakes (public so integration tests can construct a `MenuGame`).
// ---------------------------------------------------------------------------

/// A `MenuAssets` backed by in-memory vectors.
pub struct FakeAssets {
    inner: RefCell<FakeAssetsInner>,
}

pub struct FakeAssetsInner {
    pub packs: Vec<String>,
    /// pack id → pack `name` (the C++ `PackData.name` — the `-p` CLI
    /// argument matches on THIS, not the id).
    pub pack_names: std::collections::BTreeMap<String, String>,
    pub levels_by_pack: std::collections::BTreeMap<String, Vec<String>>,
    pub levels: std::collections::BTreeMap<String, LevelInfo>,
    pub colors: std::collections::BTreeMap<String, Vec<Color>>,
    pub text_color: std::collections::BTreeMap<String, Color>,
    pub load: LoadResults,
    pub profiles: Vec<String>,
    pub current_profile: Option<String>,
    pub favorites: std::collections::BTreeSet<String>,
    pub reload_log: Vec<(bool, String)>,
}

impl FakeAssets {
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            inner: RefCell::new(FakeAssetsInner {
                packs: Vec::new(),
                pack_names: std::collections::BTreeMap::new(),
                levels_by_pack: std::collections::BTreeMap::new(),
                levels: std::collections::BTreeMap::new(),
                colors: std::collections::BTreeMap::new(),
                text_color: std::collections::BTreeMap::new(),
                load: LoadResults::default(),
                profiles: Vec::new(),
                current_profile: None,
                favorites: std::collections::BTreeSet::new(),
                reload_log: Vec::new(),
            }),
        })
    }

    /// Registers a pack, its levels, and a default level/style per level
    /// (2-color style so `set_index`'s color derivation is observable).
    /// The pack's `name` defaults to its id (the C++ `PackData.name` is
    /// distinct from the id; override with [`Self::set_pack_name`]).
    pub fn add_pack(&self, pack_id: &str, level_names: &[&str]) {
        let mut inner = self.inner.borrow_mut();
        inner.packs.push(pack_id.to_string());
        inner
            .pack_names
            .insert(pack_id.to_string(), pack_id.to_string());
        let mut ids = Vec::new();
        for name in level_names {
            let id = format!("{pack_id}_{name}");
            let mut validators = std::collections::BTreeMap::new();
            let mut validators_without_pack_id = std::collections::BTreeMap::new();
            for dm in [0.5f32, 1.0, 1.5, 2.0] {
                validators.insert(dm.to_bits(), format!("validator-{id}-{dm}"));
                validators_without_pack_id
                    .insert(dm.to_bits(), format!("validator-nopack-{name}-{dm}"));
            }
            let lvl = LevelInfo {
                id: id.clone(),
                pack_id: pack_id.to_string(),
                name: name.to_string(),
                author: "author".to_string(),
                description: String::new(),
                style_id: "default".to_string(),
                difficulty_mults: vec![0.5, 1.0, 1.5, 2.0],
                lua_script_path: format!("{pack_id}/level.lua"),
                unscored: false,
                validators,
                validators_without_pack_id,
                music_name: String::new(),
                music_author: String::new(),
                music_album: String::new(),
            };
            inner.levels.insert(id.clone(), lvl);
            inner.colors.insert(
                id.clone(),
                vec![Color::new(30, 30, 255, 255), Color::new(255, 30, 30, 255)],
            );
            inner
                .text_color
                .insert(id.clone(), Color::new(20, 20, 255, 255));
            ids.push(id);
        }
        inner.levels_by_pack.insert(pack_id.to_string(), ids);
    }

    pub fn add_profile(&self, name: &str) {
        self.inner.borrow_mut().profiles.push(name.to_string());
    }

    /// Sets the pack's `name` (the C++ `PackData.name` the `-p` argument
    /// matches on; distinct from the pack id).
    pub fn set_pack_name(&self, pack_id: &str, name: &str) {
        self.inner
            .borrow_mut()
            .pack_names
            .insert(pack_id.to_string(), name.to_string());
    }
}

impl MenuAssets for FakeAssets {
    fn selectable_pack_ids(&self) -> Vec<String> {
        self.inner.borrow().packs.clone()
    }
    fn level_ids_for_pack(&self, pack_id: &str) -> Vec<String> {
        self.inner
            .borrow()
            .levels_by_pack
            .get(pack_id)
            .cloned()
            .unwrap_or_default()
    }
    fn level(&self, level_id: &str) -> Option<LevelInfo> {
        self.inner.borrow().levels.get(level_id).cloned()
    }
    fn style_colors(&self, level_id: &str) -> Vec<Color> {
        self.inner
            .borrow()
            .colors
            .get(level_id)
            .cloned()
            .unwrap_or_default()
    }
    fn style_text_color(&self, level_id: &str) -> Color {
        self.inner
            .borrow()
            .text_color
            .get(level_id)
            .copied()
            .unwrap_or_default()
    }
    fn style_background(&self, _level_id: &str) -> Option<StyleBackground> {
        // The fake carries no `StyleData`; the draw bridge then skips the
        // background sectors (the `colors.is_empty()` guard).
        None
    }
    fn pack_data_info(&self, pack_id: &str) -> PackDataInfo {
        let inner = self.inner.borrow();
        PackDataInfo {
            name: inner
                .pack_names
                .get(pack_id)
                .cloned()
                .unwrap_or_else(|| pack_id.to_string()),
            author: String::new(),
            version: 0,
        }
    }
    fn load_results(&self) -> LoadResults {
        self.inner.borrow().load.clone()
    }
    fn profile_count(&self) -> usize {
        self.inner.borrow().profiles.len()
    }
    fn profile_names(&self) -> Vec<String> {
        self.inner.borrow().profiles.clone()
    }
    fn has_current_profile(&self) -> bool {
        self.inner.borrow().current_profile.is_some()
    }
    fn current_profile_name(&self) -> Option<String> {
        self.inner.borrow().current_profile.clone()
    }
    fn is_level_favorite(&self, level_id: &str) -> bool {
        self.inner.borrow().favorites.contains(level_id)
    }
    fn profile_set_current(&self, name: &str) {
        self.inner.borrow_mut().current_profile = Some(name.to_string());
    }
    fn profile_create(&self, name: &str) {
        let mut inner = self.inner.borrow_mut();
        if !inner.profiles.contains(&name.to_string()) {
            inner.profiles.push(name.to_string());
        }
        inner.current_profile = Some(name.to_string());
    }
    fn profile_remove(&self, name: &str) {
        let mut inner = self.inner.borrow_mut();
        inner.profiles.retain(|p| p != name);
        if inner.current_profile.as_deref() == Some(name) {
            inner.current_profile = None;
        }
    }
    fn reload_assets(&self, entire_pack: bool, current_level_id: &str) {
        self.inner
            .borrow_mut()
            .reload_log
            .push((entire_pack, current_level_id.to_string()));
    }
    fn find_pack_id_by_name(&self, name: &str) -> Option<String> {
        let inner = self.inner.borrow();
        inner
            .packs
            .iter()
            .find(|id| inner.pack_names.get(*id).is_some_and(|n| n == name))
            .cloned()
    }
    fn pack_has_levels(&self, pack_id: &str) -> bool {
        self.inner
            .borrow()
            .levels_by_pack
            .get(pack_id)
            .is_some_and(|levels| !levels.is_empty())
    }
    fn pack_ids_with_missing_dependencies(&self) -> Vec<String> {
        Vec::new()
    }
    fn profile_total_survival_time(&self, name: &str) -> Option<i32> {
        // The fake's profiles carry no scores; the profile EXISTS when its
        // name is listed (the C++ `getLocalProfileByName` != nullptr).
        self.inner
            .borrow()
            .profiles
            .iter()
            .any(|p| p == name)
            .then_some(0)
    }
    fn local_profile_score(&self, _validator: &str) -> f32 {
        0.0
    }
}

/// `MenuConfig` backed by named fields (defaults match `oh-config`).
#[derive(Default)]
pub struct ConfigValues {
    // layout / sizing
    pub size_x: f32,
    pub size_y: f32,
    pub zoom_factor: f32,
    pub first_time_playing: bool,
    pub show_login_at_startup: bool,
    pub black_and_white: bool,
    pub official: bool,
    pub debug: bool,
    pub joystick_deadzone: f32,
    // gameplay
    pub auto_restart: bool,
    pub rotate_to_start: bool,
    pub invincible: bool,
    pub timescale: f32,
    pub save_last_username: bool,
    pub last_login_username: String,
    // controls
    // resolution
    pub fullscreen: bool,
    pub resolution_w: u32,
    pub resolution_h: u32,
    // graphics
    pub draw_text_outlines: bool,
    pub text_padding: i32,
    pub text_scaling: f32,
    pub antialiasing: u32,
    pub darken_uneven_background_chunk: bool,
    pub show_key_icons: bool,
    pub key_icons_scale: f32,
    pub show_level_info: bool,
    pub show_timer: bool,
    pub show_status_text: bool,
    // visual fxs
    pub three_d: bool,
    pub shaders: bool,
    pub no_pulse: bool,
    pub no_rotation: bool,
    pub no_background: bool,
    pub flash: bool,
    pub camera_shake_multiplier: f32,
    // player visual fxs
    pub angle_tilt_intensity: f32,
    pub show_player_trail: bool,
    pub player_trail_alpha: u32,
    pub player_trail_scale: f32,
    pub player_trail_decay: f32,
    pub player_trail_has_swap_color: bool,
    pub show_swap_particles: bool,
    pub show_swap_blinking_effect: bool,
    // fps
    pub vsync: bool,
    pub limit_fps: bool,
    pub max_fps: u32,
    pub show_fps: bool,
    // audio
    pub no_sound: bool,
    pub no_music: bool,
    pub sound_volume: u32,
    pub music_volume: u32,
    pub music_speed_dm_sync: bool,
    pub music_speed_multiplier: f32,
    pub play_swap_ready_sound: bool,
    // advanced
    pub use_lua_file_cache: bool,
    pub disable_game_rendering: bool,
    // action logs
    pub reset_config_log: u32,
    pub reset_binds_log: u32,
    pub clear_lua_cache_log: u32,
    pub refresh_binds_log: u32,
}

/// A `MenuConfig` over a mutable [`ConfigValues`] (test/inspection friendly).
#[derive(Default)]
pub struct FakeConfig {
    inner: RefCell<ConfigValues>,
}

impl FakeConfig {
    #[must_use]
    pub fn new() -> Rc<Self> {
        let cfg = Self::default();
        // match `oh-config` defaults the menus rely on
        {
            let mut v = cfg.inner.borrow_mut();
            v.size_x = 800.0;
            v.size_y = 450.0;
            v.zoom_factor = 1.0;
            v.timescale = 1.0;
            v.text_scaling = 1.0;
            v.key_icons_scale = 1.0;
            v.camera_shake_multiplier = 1.0;
            v.angle_tilt_intensity = 1.0;
            v.player_trail_alpha = 150;
            v.player_trail_scale = 0.5;
            v.player_trail_decay = 2.5;
            v.music_speed_multiplier = 1.0;
            v.max_fps = 60;
            v.sound_volume = 70;
            v.music_volume = 70;
        }
        Rc::new(cfg)
    }
}

impl MenuConfig for FakeConfig {
    fn size_x(&self) -> f32 {
        self.inner.borrow().size_x
    }
    fn size_y(&self) -> f32 {
        self.inner.borrow().size_y
    }
    fn zoom_factor(&self) -> f32 {
        self.inner.borrow().zoom_factor
    }
    fn first_time_playing(&self) -> bool {
        self.inner.borrow().first_time_playing
    }
    fn set_first_time_playing(&self, value: bool) {
        self.inner.borrow_mut().first_time_playing = value;
    }
    fn show_login_at_startup(&self) -> bool {
        self.inner.borrow().show_login_at_startup
    }
    fn set_show_login_at_startup(&self, value: bool) {
        self.inner.borrow_mut().show_login_at_startup = value;
    }
    fn black_and_white(&self) -> bool {
        self.inner.borrow().black_and_white
    }
    fn set_black_and_white(&self, value: bool) {
        self.inner.borrow_mut().black_and_white = value;
    }
    fn official(&self) -> bool {
        self.inner.borrow().official
    }
    fn set_official(&self, value: bool) {
        self.inner.borrow_mut().official = value;
    }
    fn debug(&self) -> bool {
        self.inner.borrow().debug
    }
    fn set_debug(&self, value: bool) {
        self.inner.borrow_mut().debug = value;
    }
    fn joystick_deadzone(&self) -> f32 {
        self.inner.borrow().joystick_deadzone
    }
    fn set_joystick_deadzone(&self, value: f32) {
        self.inner.borrow_mut().joystick_deadzone = value;
    }
    fn auto_restart(&self) -> bool {
        self.inner.borrow().auto_restart
    }
    fn set_auto_restart(&self, value: bool) {
        self.inner.borrow_mut().auto_restart = value;
    }
    fn rotate_to_start(&self) -> bool {
        self.inner.borrow().rotate_to_start
    }
    fn set_rotate_to_start(&self, value: bool) {
        self.inner.borrow_mut().rotate_to_start = value;
    }
    fn invincible(&self) -> bool {
        self.inner.borrow().invincible
    }
    fn set_invincible(&self, value: bool) {
        self.inner.borrow_mut().invincible = value;
    }
    fn timescale(&self) -> f32 {
        self.inner.borrow().timescale
    }
    fn set_timescale(&self, value: f32) {
        self.inner.borrow_mut().timescale = value;
    }
    fn save_last_username(&self) -> bool {
        self.inner.borrow().save_last_username
    }
    fn set_save_last_username(&self, value: bool) {
        self.inner.borrow_mut().save_last_username = value;
    }
    fn last_login_username(&self) -> String {
        self.inner.borrow().last_login_username.clone()
    }
    fn set_last_login_username(&self, value: &str) {
        self.inner.borrow_mut().last_login_username = value.to_string();
    }
    fn reset_config_to_defaults(&self) {
        self.inner.borrow_mut().reset_config_log += 1;
    }
    fn reset_binds_to_defaults(&self) {
        self.inner.borrow_mut().reset_binds_log += 1;
    }
    fn refresh_binds(&self) {
        self.inner.borrow_mut().refresh_binds_log += 1;
    }
    fn fullscreen(&self) -> bool {
        self.inner.borrow().fullscreen
    }
    fn set_fullscreen(&self, value: bool) {
        self.inner.borrow_mut().fullscreen = value;
    }
    fn set_resolution(&self, w: u32, h: u32) {
        let mut v = self.inner.borrow_mut();
        v.resolution_w = w;
        v.resolution_h = h;
        v.size_x = w as f32;
        v.size_y = h as f32;
    }
    fn set_resolution_auto(&self) {}
    fn resolution_modes(&self) -> Vec<(u32, u32)> {
        Vec::new()
    }
    fn draw_text_outlines(&self) -> bool {
        self.inner.borrow().draw_text_outlines
    }
    fn set_draw_text_outlines(&self, value: bool) {
        self.inner.borrow_mut().draw_text_outlines = value;
    }
    fn text_padding(&self) -> i32 {
        self.inner.borrow().text_padding
    }
    fn set_text_padding(&self, value: i32) {
        self.inner.borrow_mut().text_padding = value;
    }
    fn text_scaling(&self) -> f32 {
        self.inner.borrow().text_scaling
    }
    fn set_text_scaling(&self, value: f32) {
        self.inner.borrow_mut().text_scaling = value;
    }
    fn antialiasing(&self) -> u32 {
        self.inner.borrow().antialiasing
    }
    fn set_antialiasing(&self, value: u32) {
        self.inner.borrow_mut().antialiasing = value;
    }
    fn darken_uneven_background_chunk(&self) -> bool {
        self.inner.borrow().darken_uneven_background_chunk
    }
    fn set_darken_uneven_background_chunk(&self, value: bool) {
        self.inner.borrow_mut().darken_uneven_background_chunk = value;
    }
    fn show_key_icons(&self) -> bool {
        self.inner.borrow().show_key_icons
    }
    fn set_show_key_icons(&self, value: bool) {
        self.inner.borrow_mut().show_key_icons = value;
    }
    fn key_icons_scale(&self) -> f32 {
        self.inner.borrow().key_icons_scale
    }
    fn set_key_icons_scale(&self, value: f32) {
        self.inner.borrow_mut().key_icons_scale = value;
    }
    fn show_level_info(&self) -> bool {
        self.inner.borrow().show_level_info
    }
    fn set_show_level_info(&self, value: bool) {
        self.inner.borrow_mut().show_level_info = value;
    }
    fn show_timer(&self) -> bool {
        self.inner.borrow().show_timer
    }
    fn set_show_timer(&self, value: bool) {
        self.inner.borrow_mut().show_timer = value;
    }
    fn show_status_text(&self) -> bool {
        self.inner.borrow().show_status_text
    }
    fn set_show_status_text(&self, value: bool) {
        self.inner.borrow_mut().show_status_text = value;
    }
    fn three_d(&self) -> bool {
        self.inner.borrow().three_d
    }
    fn set_three_d(&self, value: bool) {
        self.inner.borrow_mut().three_d = value;
    }
    fn shaders(&self) -> bool {
        self.inner.borrow().shaders
    }
    fn set_shaders(&self, value: bool) {
        self.inner.borrow_mut().shaders = value;
    }
    fn no_pulse(&self) -> bool {
        self.inner.borrow().no_pulse
    }
    fn set_no_pulse(&self, value: bool) {
        self.inner.borrow_mut().no_pulse = value;
    }
    fn no_rotation(&self) -> bool {
        self.inner.borrow().no_rotation
    }
    fn set_no_rotation(&self, value: bool) {
        self.inner.borrow_mut().no_rotation = value;
    }
    fn no_background(&self) -> bool {
        self.inner.borrow().no_background
    }
    fn set_no_background(&self, value: bool) {
        self.inner.borrow_mut().no_background = value;
    }
    fn flash(&self) -> bool {
        self.inner.borrow().flash
    }
    fn set_flash(&self, value: bool) {
        self.inner.borrow_mut().flash = value;
    }
    fn camera_shake_multiplier(&self) -> f32 {
        self.inner.borrow().camera_shake_multiplier
    }
    fn set_camera_shake_multiplier(&self, value: f32) {
        self.inner.borrow_mut().camera_shake_multiplier = value;
    }
    fn angle_tilt_intensity(&self) -> f32 {
        self.inner.borrow().angle_tilt_intensity
    }
    fn set_angle_tilt_intensity(&self, value: f32) {
        self.inner.borrow_mut().angle_tilt_intensity = value;
    }
    fn show_player_trail(&self) -> bool {
        self.inner.borrow().show_player_trail
    }
    fn set_show_player_trail(&self, value: bool) {
        self.inner.borrow_mut().show_player_trail = value;
    }
    fn player_trail_alpha(&self) -> u32 {
        self.inner.borrow().player_trail_alpha
    }
    fn set_player_trail_alpha(&self, value: u32) {
        self.inner.borrow_mut().player_trail_alpha = value;
    }
    fn player_trail_scale(&self) -> f32 {
        self.inner.borrow().player_trail_scale
    }
    fn set_player_trail_scale(&self, value: f32) {
        self.inner.borrow_mut().player_trail_scale = value;
    }
    fn player_trail_decay(&self) -> f32 {
        self.inner.borrow().player_trail_decay
    }
    fn set_player_trail_decay(&self, value: f32) {
        self.inner.borrow_mut().player_trail_decay = value;
    }
    fn player_trail_has_swap_color(&self) -> bool {
        self.inner.borrow().player_trail_has_swap_color
    }
    fn set_player_trail_has_swap_color(&self, value: bool) {
        self.inner.borrow_mut().player_trail_has_swap_color = value;
    }
    fn show_swap_particles(&self) -> bool {
        self.inner.borrow().show_swap_particles
    }
    fn set_show_swap_particles(&self, value: bool) {
        self.inner.borrow_mut().show_swap_particles = value;
    }
    fn show_swap_blinking_effect(&self) -> bool {
        self.inner.borrow().show_swap_blinking_effect
    }
    fn set_show_swap_blinking_effect(&self, value: bool) {
        self.inner.borrow_mut().show_swap_blinking_effect = value;
    }
    fn vsync(&self) -> bool {
        self.inner.borrow().vsync
    }
    fn set_vsync(&self, value: bool) {
        self.inner.borrow_mut().vsync = value;
    }
    fn limit_fps(&self) -> bool {
        self.inner.borrow().limit_fps
    }
    fn set_limit_fps(&self, value: bool) {
        self.inner.borrow_mut().limit_fps = value;
    }
    fn max_fps(&self) -> u32 {
        self.inner.borrow().max_fps
    }
    fn set_max_fps(&self, value: u32) {
        self.inner.borrow_mut().max_fps = value;
    }
    fn show_fps(&self) -> bool {
        self.inner.borrow().show_fps
    }
    fn set_show_fps(&self, value: bool) {
        self.inner.borrow_mut().show_fps = value;
    }
    fn no_sound(&self) -> bool {
        self.inner.borrow().no_sound
    }
    fn set_no_sound(&self, value: bool) {
        self.inner.borrow_mut().no_sound = value;
    }
    fn no_music(&self) -> bool {
        self.inner.borrow().no_music
    }
    fn set_no_music(&self, value: bool) {
        self.inner.borrow_mut().no_music = value;
    }
    fn sound_volume(&self) -> u32 {
        self.inner.borrow().sound_volume
    }
    fn set_sound_volume(&self, value: u32) {
        self.inner.borrow_mut().sound_volume = value;
    }
    fn music_volume(&self) -> u32 {
        self.inner.borrow().music_volume
    }
    fn set_music_volume(&self, value: u32) {
        self.inner.borrow_mut().music_volume = value;
    }
    fn music_speed_dm_sync(&self) -> bool {
        self.inner.borrow().music_speed_dm_sync
    }
    fn set_music_speed_dm_sync(&self, value: bool) {
        self.inner.borrow_mut().music_speed_dm_sync = value;
    }
    fn music_speed_multiplier(&self) -> f32 {
        self.inner.borrow().music_speed_multiplier
    }
    fn set_music_speed_multiplier(&self, value: f32) {
        self.inner.borrow_mut().music_speed_multiplier = value;
    }
    fn play_swap_ready_sound(&self) -> bool {
        self.inner.borrow().play_swap_ready_sound
    }
    fn set_play_swap_ready_sound(&self, value: bool) {
        self.inner.borrow_mut().play_swap_ready_sound = value;
    }
    fn use_lua_file_cache(&self) -> bool {
        self.inner.borrow().use_lua_file_cache
    }
    fn set_use_lua_file_cache(&self, value: bool) {
        self.inner.borrow_mut().use_lua_file_cache = value;
    }
    fn clear_lua_file_cache(&self) {
        self.inner.borrow_mut().clear_lua_cache_log += 1;
    }
    fn disable_game_rendering(&self) -> bool {
        self.inner.borrow().disable_game_rendering
    }
    fn set_disable_game_rendering(&self, value: bool) {
        self.inner.borrow_mut().disable_game_rendering = value;
    }
}

/// `NetworkClient` that never connects — the safe default for the issue-017
/// seam. It reports [`ConnectionState::Disconnected`] and yields no events,
/// so the online menu shows only CONNECT/DISCONNECT and login/register stay
/// disabled. No fake network behavior is implemented here.
#[derive(Default)]
pub struct NoopNetwork;

impl NetworkClient for NoopNetwork {
    fn state(&self) -> ConnectionState {
        ConnectionState::Disconnected
    }
    fn has_rt_keys(&self) -> bool {
        false
    }
    fn connect(&self) {}
    fn disconnect(&self) {}
    fn try_login(&self, _username: &str, _password: &str) {}
    fn try_register(&self, _username: &str, _password: &str) {}
    fn try_logout(&self) {}
    fn try_delete_account(&self, _password: &str) {}
    fn poll_event(&self) -> Option<NetEvent> {
        None
    }
    fn update(&self) {}
}

/// `AudioSink` that records the played sound ids (for assertions).
#[derive(Default)]
pub struct RecordingAudio {
    pub played: RefCell<Vec<String>>,
}

impl RecordingAudio {
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
}

impl AudioSink for RecordingAudio {
    fn play_sound(&self, asset_id: &str) {
        self.played.borrow_mut().push(asset_id.to_string());
    }
    fn stop_all(&self) {}
}
