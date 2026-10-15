//! §11.6 — every menu and every item (verbatim names + exact callbacks).
//!
//! Builds the five `oh-menu-system` menus. Each item's callback is a closure
//! over the shared `MenuGame` state (a `Rc<RefCell<Shared>>`), the config
//! seam, the network seam, or the `fnHG*` callback seams — mirroring the C++
//! lambdas. The keyboard/joystick bind controls are modelled as `Single`
//! items whose `exec` fires the `fnHGTriggerRefresh` seam (the full
//! bind-capture UI is owned by `oh-config`'s `BindControl`, issue 004).
//!
//! The `cfg_toggle!` / `cfg_slider!` macros give each of the two config
//! functors its own `Rc` clone (a `move` closure owns its capture, so a
//! single `Rc` cannot be moved into both the getter and setter closures).

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu_system::category::Category;
use oh_menu_system::item::{Item, ItemExt};
use oh_menu_system::menu::Menu;

use crate::menu_game::Shared;
use crate::seams::{ConnectionState, MenuConfig};
use crate::states::States;

macro_rules! cfg_toggle {
    ($cat:expr, $cfg:expr, $name:expr, $get:ident, $set:ident) => {{
        let c = $cfg.clone();
        $cat.create_toggle_bool(
            $name,
            {
                let g = c.clone();
                move || g.$get()
            },
            {
                let s = c.clone();
                move |v| s.$set(v)
            },
        )
    }};
}

macro_rules! cfg_slider {
    ($cat:expr, $cfg:expr, $name:expr, $get:ident, $set:ident, $min:expr, $max:expr, $step:expr) => {{
        let c = $cfg.clone();
        $cat.create_slider_value(
            $name,
            {
                let g = c.clone();
                move || g.$get()
            },
            {
                let s = c.clone();
                move |v| s.$set(v)
            },
            $min,
            $max,
            $step,
        )
    }};
}

/// The five menus `MenuGame` owns.
pub struct Menus {
    pub welcome: Rc<Menu>,
    pub main: Rc<Menu>,
    pub options: Rc<Menu>,
    pub online: Rc<Menu>,
    pub profile_selection: Rc<Menu>,
}

/// C++ file-static `whenNotOfficial` (shared via `Rc` so it is cheaply
/// clonable into each gated item's predicate closure).
fn not_official(cfg: Rc<dyn MenuConfig>) -> Rc<dyn Fn() -> bool> {
    Rc::new(move || !cfg.official())
}

/// Attach an enable-when predicate, upcasting the typed item handle to the
/// `Rc<RefCell<dyn Item>>` that [`ItemExt::when`] is implemented for.
fn when<T>(item: Rc<RefCell<T>>, pred: impl Fn() -> bool + 'static)
where
    T: Item + 'static,
{
    let up: Rc<RefCell<dyn Item>> = item;
    up.when(pred);
}

/// Same as [`when`] for an item handle that is already dynamic.
fn when_dyn(item: Rc<RefCell<dyn Item>>, pred: impl Fn() -> bool + 'static) {
    item.when(pred);
}

/// C++ `MenuGame::initMenus()`.
pub(crate) fn build_menus(shared: Rc<RefCell<Shared>>) -> Menus {
    let cfg: Rc<dyn MenuConfig> = shared.borrow().config().clone();
    let net: Rc<dyn crate::seams::NetworkClient> = shared.borrow().network().clone();

    let welcome_menu = Menu::new();
    let main_menu = Menu::new();
    let options_menu = Menu::new();
    let online_menu = Menu::new();
    let profile_selection_menu = Menu::new();

    // ------------------------------------------------------------------
    // Welcome menu
    // ------------------------------------------------------------------
    let wlcm = welcome_menu.create_category("welcome");
    {
        let s = shared.clone();
        wlcm.create_single("play locally", move || s.borrow_mut().play_locally());
    }
    {
        let s = shared.clone();
        wlcm.create_single("exit game", move || s.borrow_mut().quit());
    }

    // ------------------------------------------------------------------
    // Options menu
    // ------------------------------------------------------------------
    let options = options_menu.create_category("options");
    let play = options_menu.create_category("gameplay");
    let controls = options_menu.create_category("controls");
    let keyboard = options_menu.create_category("keyboard");
    let joystick = options_menu.create_category("joystick");
    let resolution = options_menu.create_category("resolution");
    let gfx = options_menu.create_category("graphics");
    let sfx = options_menu.create_category("audio");
    let advanced = options_menu.create_category("advanced");
    let visfx = options_menu.create_category("visual fxs");
    let playervisfx = options_menu.create_category("player visual fxs");
    let fps = options_menu.create_category("fps settings");
    let six_by_nine = options_menu.create_category("16x9 resolutions");
    let four_by_three = options_menu.create_category("4x3 resolutions");
    let six_by_ten = options_menu.create_category("16x10 resolutions");

    options.create_goto("GAMEPLAY", &play);
    options.create_goto("CONTROLS", &controls);
    options.create_goto("RESOLUTION", &resolution);
    options.create_goto("GRAPHICS", &gfx);
    options.create_goto("AUDIO", &sfx);
    options.create_goto("ADVANCED", &advanced);
    {
        let c = cfg.clone();
        options.create_single("RESET CONFIG", move || {
            c.reset_config_to_defaults();
            c.refresh_binds();
        });
    }

    // ---- gameplay ----
    cfg_toggle!(play, cfg, "autorestart", auto_restart, set_auto_restart);
    cfg_toggle!(
        play,
        cfg,
        "rotate to start",
        rotate_to_start,
        set_rotate_to_start
    );
    cfg_toggle!(play, cfg, "OFFICIAL MODE", official, set_official);
    when(cfg_toggle!(play, cfg, "debug mode", debug, set_debug), {
        let p = not_official(cfg.clone());
        move || p()
    });
    when(
        cfg_toggle!(play, cfg, "invincible", invincible, set_invincible),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    when(
        cfg_slider!(
            play,
            cfg,
            "timescale",
            timescale,
            set_timescale,
            0.1f32,
            2.0f32,
            0.05f32
        ),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    cfg_toggle!(
        play,
        cfg,
        "save last username",
        save_last_username,
        set_save_last_username
    );
    cfg_toggle!(
        play,
        cfg,
        "show login at startup",
        show_login_at_startup,
        set_show_login_at_startup
    );
    play.create_go_back("back");

    // ---- controls ----
    controls.create_goto("keyboard", &keyboard);
    controls.create_goto("joystick", &joystick);
    cfg_slider!(
        controls,
        cfg,
        "joystick deadzone",
        joystick_deadzone,
        set_joystick_deadzone,
        0.0f32,
        100.0f32,
        1.0f32
    );
    {
        let c = cfg.clone();
        controls.create_single("reset binds", move || {
            c.reset_binds_to_defaults();
            c.refresh_binds();
        });
    }
    {
        let s = shared.clone();
        controls.create_single("hardcoded keys reference", move || {
            let mut sh = s.borrow_mut();
            sh.show_dialog_box(
                "UP ARROW - UP\n\
                 DOWN ARROW - DOWN\n\
                 RETURN - ENTER\n\
                 BACKSPACE - REMOVE BIND\n\
                 F1 - ADD LEVEL TO FAVORITES\n\
                 F2 - SWITCH TO/FROM FAVORITE LEVELS\n\
                 F3 - RELOAD LEVEL ASSETS (DEBUG MODE ONLY)\n\
                 F4 - RELOAD PACK ASSETS (DEBUG MODE ONLY)\n\n\
                 PRESS ANY KEY OR BUTTON TO CLOSE THIS MESSAGE\n",
            );
            sh.ignore_inputs_after_menu_exec();
        });
    }
    controls.create_go_back("back");

    // ---- keyboard binds (each fires the fnHGTriggerRefresh seam) ----
    let kb_names = [
        "rotate ccw",
        "rotate cw",
        "focus",
        "exit",
        "force restart",
        "restart",
        "replay",
        "screenshot",
        "swap",
        "up",
        "down",
        "next pack",
        "previous pack",
        "lua console (debug only)",
        "pause game (debug only)",
    ];
    for (i, name) in kb_names.into_iter().enumerate() {
        let s = shared.clone();
        let id = i as i32;
        keyboard.create_single(name, move || s.borrow().fire_trigger_refresh(name, id));
    }
    keyboard.create_go_back("back");

    // ---- joystick binds ----
    let joy_names = [
        "select",
        "exit",
        "focus",
        "swap",
        "force restart",
        "restart",
        "replay",
        "screenshot",
        "next pack",
        "previous pack",
        "(un)favorite level",
        "favorites menu",
    ];
    for (i, name) in joy_names.into_iter().enumerate() {
        let s = shared.clone();
        let id = i as i32;
        joystick.create_single(name, move || s.borrow().fire_trigger_refresh(name, id));
    }
    joystick.create_go_back("back");

    // ---- resolution ----
    {
        let c = cfg.clone();
        resolution.create_single("automatically set resolution", move || {
            c.set_resolution_auto()
        });
    }
    resolution.create_goto("16x9 resolutions", &six_by_nine);
    resolution.create_goto("4x3 resolutions", &four_by_three);
    resolution.create_goto("16x10 - uncommon resolutions", &six_by_ten);
    // C++: one Single per 32-bit fullscreen mode, grouped by aspect.
    for (w, h) in cfg.resolution_modes() {
        let label = format!("{w}x{h}");
        // C++ groups by `10.f * w / h` (`case 17` = 16:9, `case 13` = 4:3,
        // default = 16:10/uncommon).
        let ratio = 10.0 * (w as f32) / (h as f32);
        let cat: &Rc<Category> = if ratio >= 17.0 {
            &six_by_nine
        } else if ratio < 14.0 {
            &four_by_three
        } else {
            &six_by_ten
        };
        let c = cfg.clone();
        cat.create_single(&label, move || c.set_resolution(w, h));
    }
    {
        six_by_nine.create_go_back("back");
        four_by_three.create_go_back("back");
        six_by_ten.create_go_back("back");
        let cw = cfg.clone();
        resolution.create_single("go windowed", move || cw.set_fullscreen(false));
        let cf = cfg.clone();
        resolution.create_single("go fullscreen", move || cf.set_fullscreen(true));
    }
    resolution.create_go_back("back");

    // ---- graphics ----
    gfx.create_goto("visual fxs", &visfx);
    gfx.create_goto("player visual fxs", &playervisfx);
    gfx.create_goto("fps settings", &fps);
    cfg_toggle!(
        gfx,
        cfg,
        "text outlines",
        draw_text_outlines,
        set_draw_text_outlines
    );
    cfg_slider!(
        gfx,
        cfg,
        "text padding",
        text_padding,
        set_text_padding,
        0i32,
        64i32,
        1i32
    );
    cfg_slider!(
        gfx,
        cfg,
        "text scaling",
        text_scaling,
        set_text_scaling,
        0.1f32,
        4.0f32,
        0.05f32
    );
    {
        // C++ custom antialiasing slider: get "<n>x"; right: 0→1 else
        // clamp(n<<1,0,16); left: clamp(n>>1,0,16).
        let c = cfg.clone();
        gfx.create_slider(
            "antialiasing",
            {
                let g = c.clone();
                Box::new(move || format!("{}x", g.antialiasing())) as Box<dyn Fn() -> String>
            },
            {
                let s = c.clone();
                Box::new(move || {
                    let n = s.antialiasing();
                    s.set_antialiasing(if n == 0 { 1 } else { (n << 1).min(16) });
                }) as Box<dyn Fn()>
            },
            {
                let s = c.clone();
                Box::new(move || s.set_antialiasing((s.antialiasing() >> 1).min(16)))
                    as Box<dyn Fn()>
            },
        );
    }
    cfg_toggle!(
        gfx,
        cfg,
        "darken background chunk",
        darken_uneven_background_chunk,
        set_darken_uneven_background_chunk
    );
    cfg_toggle!(
        gfx,
        cfg,
        "show key icons",
        show_key_icons,
        set_show_key_icons
    );
    cfg_slider!(
        gfx,
        cfg,
        "key icons scaling",
        key_icons_scale,
        set_key_icons_scale,
        0.1f32,
        4.0f32,
        0.05f32
    );
    cfg_toggle!(
        gfx,
        cfg,
        "show level info",
        show_level_info,
        set_show_level_info
    );
    when(
        cfg_toggle!(gfx, cfg, "show timer", show_timer, set_show_timer),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    when(
        cfg_toggle!(
            gfx,
            cfg,
            "show status text",
            show_status_text,
            set_show_status_text
        ),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    gfx.create_go_back("back");

    // ---- visual fxs ----
    cfg_toggle!(visfx, cfg, "3D effects", three_d, set_three_d);
    cfg_toggle!(visfx, cfg, "shader effects", shaders, set_shaders);
    when(
        cfg_toggle!(visfx, cfg, "no pulse", no_pulse, set_no_pulse),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    when(
        cfg_toggle!(visfx, cfg, "no rotation", no_rotation, set_no_rotation),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    when(
        cfg_toggle!(
            visfx,
            cfg,
            "no background",
            no_background,
            set_no_background
        ),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    when(
        cfg_toggle!(
            visfx,
            cfg,
            "b&w colors",
            black_and_white,
            set_black_and_white
        ),
        {
            let p = not_official(cfg.clone());
            move || p()
        },
    );
    cfg_toggle!(visfx, cfg, "flash", flash, set_flash);
    cfg_slider!(
        visfx,
        cfg,
        "shake mult.",
        camera_shake_multiplier,
        set_camera_shake_multiplier,
        0.0f32,
        5.0f32,
        0.1f32
    );

    // ---- player visual fxs ----
    cfg_slider!(
        playervisfx,
        cfg,
        "angle tilt mult.",
        angle_tilt_intensity,
        set_angle_tilt_intensity,
        0.0f32,
        5.0f32,
        0.1f32
    );
    cfg_toggle!(
        playervisfx,
        cfg,
        "show trail",
        show_player_trail,
        set_show_player_trail
    );
    cfg_slider!(
        playervisfx,
        cfg,
        "trail alpha",
        player_trail_alpha,
        set_player_trail_alpha,
        0u32,
        255u32,
        5u32
    );
    cfg_slider!(
        playervisfx,
        cfg,
        "trail scale",
        player_trail_scale,
        set_player_trail_scale,
        0.05f32,
        1.0f32,
        0.05f32
    );
    cfg_slider!(
        playervisfx,
        cfg,
        "trail decay",
        player_trail_decay,
        set_player_trail_decay,
        0.5f32,
        50.0f32,
        2.5f32
    );
    cfg_toggle!(
        playervisfx,
        cfg,
        "trail has swap color",
        player_trail_has_swap_color,
        set_player_trail_has_swap_color
    );
    cfg_toggle!(
        playervisfx,
        cfg,
        "show swap particles",
        show_swap_particles,
        set_show_swap_particles
    );
    cfg_toggle!(
        playervisfx,
        cfg,
        "swap blinking effect",
        show_swap_blinking_effect,
        set_show_swap_blinking_effect
    );
    playervisfx.create_go_back("back");

    // ---- fps settings ----
    cfg_toggle!(fps, cfg, "vsync", vsync, set_vsync);
    cfg_toggle!(fps, cfg, "limit fps", limit_fps, set_limit_fps);
    cfg_slider!(
        fps,
        cfg,
        "max fps",
        max_fps,
        set_max_fps,
        30u32,
        1000u32,
        5u32
    );
    cfg_toggle!(fps, cfg, "show fps", show_fps, set_show_fps);
    fps.create_go_back("back");

    // ---- audio ----
    cfg_toggle!(sfx, cfg, "no sound", no_sound, set_no_sound);
    cfg_toggle!(sfx, cfg, "no music", no_music, set_no_music);
    cfg_slider!(
        sfx,
        cfg,
        "sound volume",
        sound_volume,
        set_sound_volume,
        0u32,
        100u32,
        5u32
    );
    cfg_slider!(
        sfx,
        cfg,
        "music volume",
        music_volume,
        set_music_volume,
        0u32,
        100u32,
        5u32
    );
    cfg_toggle!(
        sfx,
        cfg,
        "sync music with difficulty",
        music_speed_dm_sync,
        set_music_speed_dm_sync
    );
    cfg_slider!(
        sfx,
        cfg,
        "music speed multiplier",
        music_speed_multiplier,
        set_music_speed_multiplier,
        0.7f32,
        1.3f32,
        0.05f32
    );
    cfg_toggle!(
        sfx,
        cfg,
        "play swap ready blip sound",
        play_swap_ready_sound,
        set_play_swap_ready_sound
    );
    sfx.create_go_back("back");

    // ---- advanced ----
    cfg_toggle!(
        advanced,
        cfg,
        "cache lua files",
        use_lua_file_cache,
        set_use_lua_file_cache
    );
    {
        let c = cfg.clone();
        advanced.create_single("clear lua file cache", move || c.clear_lua_file_cache());
    }
    cfg_toggle!(
        advanced,
        cfg,
        "disable game rendering",
        disable_game_rendering,
        set_disable_game_rendering
    );

    // ------------------------------------------------------------------
    // Main menu
    // ------------------------------------------------------------------
    let main = main_menu.create_category("main");
    let local_profiles = main_menu.create_category("local profiles");

    main.create_single("LEVEL SELECT", {
        let s = shared.clone();
        move || {
            let mut sh = s.borrow_mut();
            if sh.first_level_selection {
                sh.lvl_slct.pack_idx = 0;
                sh.diff_mult_idx = 0;
                if let Some(pack) = sh.get_nth_selectable_pack_id(0) {
                    sh.lvl_slct.level_data_ids = sh.assets().level_ids_for_pack(&pack);
                }
                sh.set_index(0);
            }
            sh.change_state_to(States::LevelSelection);
            sh.play_sound_override("select.ogg");
        }
    });
    main.create_goto("LOCAL PROFILES", &local_profiles);
    main.create_single("ONLINE", {
        let s = shared.clone();
        move || s.borrow_mut().change_state_to(States::MOnline)
    });
    main.create_single("OPTIONS", {
        let s = shared.clone();
        move || s.borrow_mut().change_state_to(States::MOpts)
    });
    main.create_single("EXIT", {
        let s = shared.clone();
        move || s.borrow_mut().quit()
    });

    // ------------------------------------------------------------------
    // Online menu (category named "options", mirroring the C++)
    // ------------------------------------------------------------------
    let online = online_menu.create_category("options");

    let must_connect = {
        let n = net.clone();
        move || {
            matches!(
                n.state(),
                ConnectionState::Disconnected | ConnectionState::ConnectionError
            )
        }
    };
    let must_login = {
        let n = net.clone();
        move || n.state() == ConnectionState::Connected && n.has_rt_keys()
    };
    let must_register = {
        let n = net.clone();
        move || n.state() == ConnectionState::Connected && n.has_rt_keys()
    };
    let is_connected = {
        let n = net.clone();
        move || n.state() == ConnectionState::Connected
    };
    let logged_in = {
        let n = net.clone();
        move || {
            matches!(
                n.state(),
                ConnectionState::LoggedIn | ConnectionState::LoggedIn_Ready
            ) && n.has_rt_keys()
        }
    };
    let must_delete = {
        let n = net.clone();
        move || n.state() == ConnectionState::Connected && n.has_rt_keys()
    };

    online.create_single("CONNECT", {
        let n = net.clone();
        move || n.connect()
    });
    online.create_single("LOGIN", {
        let s = shared.clone();
        move || {
            let mut sh = s.borrow_mut();
            if sh.dialog_input_state != crate::states::DialogInputState::Nothing {
                return;
            }
            sh.open_login_dialog_box_and_start_login_process();
            sh.ignore_inputs_after_menu_exec();
        }
    });
    online.create_single("REGISTER", {
        let s = shared.clone();
        move || {
            let mut sh = s.borrow_mut();
            if sh.dialog_input_state != crate::states::DialogInputState::Nothing {
                return;
            }
            sh.dialog_input_state = crate::states::DialogInputState::Registration_EnteringUsername;
            sh.show_input_dialog_box_nice("REGISTRATION", "USERNAME", "");
            sh.ignore_inputs_after_menu_exec();
        }
    });
    online.create_single("LOGOUT", {
        let n = net.clone();
        move || n.try_logout()
    });
    online.create_single("DISCONNECT", {
        let n = net.clone();
        move || n.disconnect()
    });
    online.create_single("DELETE ACCOUNT", {
        let s = shared.clone();
        move || {
            let mut sh = s.borrow_mut();
            if sh.dialog_input_state != crate::states::DialogInputState::Nothing {
                return;
            }
            sh.dialog_input_state = crate::states::DialogInputState::DeleteAccount_EnteringPassword;
            sh.show_input_dialog_box_nice(
                "DELETE ACCOUNT",
                "PASSWORD",
                "WARNING: THIS WILL DELETE ALL YOUR SCORES",
            );
            sh.dialog.set_input_box_password(true);
            sh.ignore_inputs_after_menu_exec();
        }
    });

    attach_online_predicates(
        &online,
        must_connect,
        must_login,
        must_register,
        logged_in,
        is_connected,
        must_delete,
    );

    // ------------------------------------------------------------------
    // Local profiles (a category of the main menu)
    // ------------------------------------------------------------------
    local_profiles.create_single("CHOOSE PROFILE", {
        let s = shared.clone();
        move || s.borrow_mut().change_state_to(States::SLPSelect)
    });
    local_profiles.create_single("NEW PROFILE", {
        let s = shared.clone();
        move || {
            let mut sh = s.borrow_mut();
            sh.change_state_to(States::ETLPNew);
            sh.entered_str.clear();
            sh.play_sound_override("select.ogg");
        }
    });
    local_profiles.create_go_back("BACK");

    // ------------------------------------------------------------------
    // Profile selection
    // ------------------------------------------------------------------
    let profile_selection = profile_selection_menu.create_category("profile selection");
    {
        let names = shared.borrow().assets().profile_names();
        for name in names {
            let s = shared.clone();
            let name_c = name.clone();
            profile_selection.create_single(&name, move || {
                let mut sh = s.borrow_mut();
                sh.assets().profile_set_current(&name_c);
                sh.change_favorite_levels_to_profile();
            });
        }
        profile_selection.sort_by_name();
    }

    Menus {
        welcome: welcome_menu,
        main: main_menu,
        options: options_menu,
        online: online_menu,
        profile_selection: profile_selection_menu,
    }
}

/// Attach the C++ `operator|` visibility predicates to the online menu items
/// (index order matches the creation order above).
fn attach_online_predicates(
    online: &Rc<Category>,
    must_connect: impl Fn() -> bool + 'static,
    must_login: impl Fn() -> bool + 'static,
    must_register: impl Fn() -> bool + 'static,
    logged_in: impl Fn() -> bool + 'static,
    is_connected: impl Fn() -> bool + 'static,
    must_delete: impl Fn() -> bool + 'static,
) {
    let items = online.get_items();
    // items: 0 CONNECT, 1 LOGIN, 2 REGISTER, 3 LOGOUT, 4 DISCONNECT, 5 DELETE ACCOUNT
    if let Some(it) = items.first() {
        when_dyn(it.clone(), must_connect);
    }
    if let Some(it) = items.get(1) {
        when_dyn(it.clone(), must_login);
    }
    if let Some(it) = items.get(2) {
        when_dyn(it.clone(), must_register);
    }
    if let Some(it) = items.get(3) {
        when_dyn(it.clone(), logged_in);
    }
    if let Some(it) = items.get(4) {
        when_dyn(it.clone(), is_connected);
    }
    if let Some(it) = items.get(5) {
        when_dyn(it.clone(), must_delete);
    }
}
