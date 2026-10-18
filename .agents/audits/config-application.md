# Audit — do the configs actually APPLY? (per-platform, Android + wasm in focus)

**Provenance.** Produced by lane **047** on the operator's directive *"generally check for configs that
don't apply on android and wasm builds"*, and transcribed here from that lane's report because the
table is the entire value of an audit and it had no durable home (the lane's `## Report` carried the fix
and the method, not the table). The citations are the lane's; the **K2 correction** in section A is the
orchestrator's, applied after the register and the audit were found to disagree — see the note beneath
the table. Anything re-verified by the orchestrator says so.

## Method

- **Denominator: the C++'s own config table** — `Config.cpp`'s `X(name, type, json_key, default)` rows.
  Enumerated completely: **101 rows** = 73 non-bind + 12 joystick binds + 16 keyboard/mouse triggers.
  Cross-checked against the port's `oh-config` `Config` struct: **101 `LinkedValue` fields — exact
  parity**, no C++ row without a port field.
- **Detection: grep the port for READS of each field** (including indirect reads through a getter, and
  internal reads inside `oh-config` that feed other applied values). A field that is only declared,
  loaded and synced is **inert** — that is the grep that found the fullscreen gap.
- **Verdicts**: **APPLIED** (the port reads the value and produces the C++'s observable effect on that
  target) · **INERT-BY-DESIGN** (the platform has no equivalent *and* the C++'s own path is equally
  inert there) · **MISSING** (the port is inert where the C++ is not).
- **Key platform fact**: the C++ has **no android or wasm build** (SFML is desktop-only), so for the
  platform-window settings "the C++ is equally inert" means *there is no C++ path on that target at
  all*. That is what makes those rows parity rather than gaps.

## A. Target-sensitive rows

| Setting (json key) | C++ application site | Port site | Desktop | Android | Wasm |
|---|---|---|---|---|---|
| `fullscreen` | `main.cpp:306` → `GameWindow.hpp:90` style | `windowed.rs:3972` → `Window::set_fullscreen` | **APPLIED** (047's fix `6405e893`) | **INERT** — activity window is already screen-sized; no C++ android build | **INERT** — browser-API button is lane 046; no C++ wasm build |
| `windowed_auto_resolution` | `Config.cpp:478,521` | `config.rs:1146` via `load_and_reapply` (real desktop mode) | **APPLIED** | **INERT** — zero mode passed + screen-sized; no C++ android build | **INERT** — zero mode + canvas-sized; no C++ wasm build |
| `fullscreen_auto_resolution` | `Config.cpp:485,521` | `config.rs:1150` | **APPLIED** | **INERT** (as above) | **INERT** (as above) |
| `fullscreen_width` | `main.cpp:304` via `getWidth()` | `get_width()` → `create_window` logical_w | **APPLIED** | **APPLIED** (logical size; physical is screen-sized) | **INERT** — canvas is CSS-sized; no C++ wasm build |
| `fullscreen_height` | `main.cpp:304` via `getHeight()` | `get_height()` | **APPLIED** | **APPLIED** (logical) | **INERT** (as above) |
| `windowed_width` | `main.cpp:304` via `getWidth()` | `get_width()` | **APPLIED** | **APPLIED** (logical) | **INERT** (as above) |
| `windowed_height` | `main.cpp:304` via `getHeight()` | `get_height()` | **APPLIED** | **APPLIED** (logical) | **INERT** (as above) |
| `vsync` | `main.cpp:308` → SFML `eglSwapInterval` | `Window::set_vsync` → `GlParts::set_swap_interval` (live interval) | **APPLIED** | **APPLIED** (25j's live EGL swap interval) | **INERT** — browser rAF is display-vsynced; no swap-interval API |
| `limit_fps` | `main.cpp:309` → `setFPSLimited` | `set_fps_limited` → `effective_framerate_limit` → `ControlFlow::WaitUntil` | **APPLIED** | **MISSING — see K2** (the value reaches the pacing logic, but winit's android event loop never blocks while a redraw is pending and 025i keeps one pending, so the cap is not enforced: menu ~500 fps, in-game ~360 fps against the C++'s paced 200, CPU 76–71 % vs the C++'s lower burn) | **INERT** — rAF paces frames |
| `max_fps` | `main.cpp:310` → `setMaxFPS` | `set_max_fps` → throttle | **APPLIED** | **MISSING — see K2** (as above) | **INERT** — rAF paces frames |
| `antialiasing_level` | `main.cpp:307` → SFML msaa context; `MenuGame.cpp:1447-1464` | level → context creation: the `pick_msaa_config` shared tail (gl.rs — smallest config with samples ≥ level; no config ≥ level → largest samples BELOW + the SFML-style downgrade warning, the pinned fork's `evaluateFormat`/post-creation-check shape, `GlContext.cpp:733-749`/`:974-988`); wasm `antialias = level > 0` (WebGL2's `antialias` is a BOOLEAN — the browser picks the sample count, the level stated as non-honourable-exactly at the create site + logged with the measured `GL_SAMPLES`) | **APPLIED** (050: the level now selects the MSAA context config; the scene renders to the DEFAULT framebuffer — no FBO anywhere in `oh-render` — so the context's samples ARE the effect, SFML's own model. MEASURED `GL_SAMPLES` (Mesa 4.5, Xvfb): 4→4, 8→8, 0→0, 2→4 (no 2-sample config; smallest ≥), 16/32→8 + warning) | **APPLIED** (050: the level goes into the Android EGL config pick through the SAME tail — the real risk handled: no matching EGL config → closest-available fallback + the named warning; `check-android.sh` green; the device's `GL_SAMPLES` is PENDING — adb unreachable) | **APPLIED** (050: the honest boolean mapping above — the level's exact value is NOT expressible in WebGL2, so "applied" means `antialias=true` iff level > 0 and the browser's own count (its `GL_SAMPLES`) is the observable; the emscripten check green in the emsdk container; the browser's actual count is PENDING — no browser on this box) |
| `pixel_multiplier` | `main.cpp:305` → `GameWindow.hpp:93` (×pixelMult) | physical = logical × mult at `create` (`windowed.rs:3946`, `android.rs:255`) | **APPLIED** | **APPLIED** | **INERT** — canvas is CSS-box × DPR, not config × mult |

### The correction applied to this table (orchestrator)

The audit's criterion was "the port reads the value **and produces the C++'s observable effect**", but
`limit_fps`/`max_fps` were verdicted **APPLIED on Android** — which the register's **K2** row contradicts
with a measured mechanism and on-device numbers (quoted above). K2 wins: it is measured, and by the
audit's own criterion the Android verdict must be MISSING. The two cells above are therefore corrected,
and **the audit was the wrong artifact, not K2** — recorded here rather than quietly amended. Lane 049
owns the repair (mirroring SFML's `Window::display` frame-limiter sleep on Android without undoing
025i's ANR fix) and the device re-measurement.

## B. Target-independent rows — APPLIED on all three targets

Read and applied through the shared `oh-game` / `oh-render` / `oh-app` / `oh-audio` code paths; audio
volume is set in the shared `WindowedClient::build` over the real `oh-audio` on every target (issue 041).

`official` (`hexagon_game.rs:2793`, `oh-lua`, `text_draw.rs:198`) · `no_pulse` (`update.rs:1102`,
`graphics.rs:511`) · `no_rotation` (`update.rs:379`) · `no_background` (`graphics.rs:434`) · `no_sound`
(`windowed.rs:207/1045`, `hexagon_game.rs:1872`) · `no_music` (`hexagon_game.rs:859/1174/1877`) ·
`black_and_white` (`graphics.rs:449/469/551`, `text_draw.rs:571`) · `3D_enabled` (`graphics.rs:500`,
`update.rs:481`) · `shaders_enabled` (`graphics.rs:409`, `update.rs:485`) · `3D_multiplier`
(`graphics.rs:516`) · `invincible` (`hexagon_game.rs:1265/1296`, `update.rs:1546`) · `auto_restart`
(`hexagon_game.rs:1329`) · `sound_volume` (`windowed.rs:2760`) · `music_volume` (`windowed.rs:2761`) ·
`flash_enabled` (`graphics.rs:699`) · `zoom_factor` (`graphics.rs:1272/1751/1946/2089`,
`update.rs:1117`) · `player_speed` (`hexagon_game.rs:560/897`) · `player_focus_speed`
(`hexagon_game.rs:564/901`) · `player_size` (`hexagon_game.rs:556/893`) · `auto_zoom_factor`
(`config.rs:1105/1163`, internal to `recalculate_sizes`) · `show_messages` (`hexagon_game.rs:1833`) ·
`debug` (`text_draw.rs:202`, `hexagon_game.rs`, `update.rs`, `lua_console.rs`) · `beatpulse_enabled`
(`update.rs:350/1148`) · `show_tracked_variables` (`update.rs:1575`, `text_draw.rs:247`) ·
`music_speed_dm_sync` (`hexagon_game.rs:1088`) · `show_fps` (`update.rs:1628`,
`text_draw.rs:287/354/444`) · `music_speed_mult` (`hexagon_game.rs:2034`) · `draw_text_outlines`
(`text_draw.rs:398/530/554`) · `darken_uneven_background_chunk` (`graphics.rs:448/468`) ·
`rotate_to_start` (`update.rs:803/1597`, `text_draw.rs:261`) · `joystick_deadzone` (`update.rs:849`) ·
`text_padding` (`text_draw.rs:416`) · `text_scaling` (`text_draw.rs:340/416`) · `timescale`
(`update.rs:1550`, `text_draw.rs:220`) · `show_key_icons` (`update.rs:197`, `graphics.rs:679`) ·
`key_icons_scale` (`graphics.rs:1138`, `text_draw.rs:465`) · `first_time_playing` (`windowed.rs:501`,
menu seam) · `show_level_info` (`update.rs:203`, `graphics.rs:693`) · `show_timer` (`text_draw.rs:418`) ·
`show_status_text` (`text_draw.rs:428`) · `server_ip` (`dispatch.rs:186`) · `server_port`
(`dispatch.rs:206`) · `server_control_port` (`dispatch.rs:207`) · `server_level_whitelist`
(`dispatch.rs:208`) · `save_last_login_username` (`windowed.rs:614`) · `last_login_username`
(`windowed.rs:626`) · `show_login_at_startup` (`windowed.rs:513`) · `camera_shake_multiplier`
(`hexagon_game.rs:1349`) · `angle_tilt_intensity` (`hexagon_game.rs:2530`) · `show_player_trail`
(`update.rs:420`, `graphics.rs:635`) · `player_trail_alpha` (`update.rs:1360`) · `player_trail_scale`
(`update.rs:1559`) · `player_trail_decay` (`update.rs:1361`) · `player_trail_has_swap_color`
(`hexagon_game.rs:2233`) · `show_swap_particles` (`update.rs:424`, `graphics.rs:639`) ·
`play_swap_ready_sound` (`update.rs:269`) · `show_swap_blinking_effect` (`hexagon_game.rs:2223/2531`) ·
`use_lua_file_cache` (`compose.rs:265`, `windowed.rs:1128`) · `disable_game_rendering`
(`graphics.rs:401`)

## C. INERT in the C++ too — parity, not bugs (§3: inert is parity)

- **`pulse_enabled`** — the C++ has **no getter** (`getPulseEnabled` does not exist); the value is
  synced and saved only. The port mirrors that. Inert in both.
- **`3D_max_depth`** — the C++'s `get3DMaxDepth()` is **defined and never called** (zero callers in
  `src`/`include`). The port reads it nowhere. Inert in both.

## D. The 28 binds

- **16 keyboard/mouse triggers** (`t_rotate_ccw` … `t_pause`): **APPLIED on all three targets** — the
  core game input, read via `get_trigger`/`trigger_ref` and driven by the shared input system (desktop
  keyboard/mouse; android/wasm touch + pointer map to the same actions).
- **12 joystick binds** (`j_select` … `j_favorite_menu`): desktop **APPLIED** (fed to the live gilrs
  `RealJoystick` via `load_all_joystick_binds`). Android/wasm **INERT-BY-DESIGN** — no controller
  hardware, and the C++'s `Joystick::isConnected(0) == false` state is equally inert without one.

## Filed but unfixed → FIXED by lane 050 (the record)

**`antialiasing_level` was MISSING on all three targets** (this section's original finding, since
repaired). The contexts were created with no msaa samples (verified by the orchestrator: `gl.rs:541`
`antialias: false`, and no `multisampling` anywhere in `oh-render`) while the C++ applies SFML msaa
(`main.cpp:307`, `MenuGame.cpp:1447-1464`). Filed **Size: MEDIUM-LARGE** — its own lane.

**The 050 shape diagnosis closed the size question the other way: it was the SMALL shape.** The
scene renders to the DEFAULT framebuffer (no `glBindFramebuffer` anywhere in `oh-render`; the
`RenderTexture` is a CPU-side analog; `render_states_for_stage`/the 029n1 pack shaders select
per-stage shader/blend state only — never a framebuffer target), so a multisampled context IS the
whole fix — SFML's own model (`recreateWindow`'s `ContextSettings{0, 0, antialiasingLevel, 0, 0}`,
GameWindow.hpp:91, applied before the window exists; the C++ has no offscreen render target in its
draw path either). No multisampled-FBO + resolve work was needed. What landed (the section-A row
above is the applied state): the level plumbed into all three context creations (glutin's config
pick — NOT a hard `with_multisampling` filter: it would return zero configs when the driver offers
only HIGHER sample counts, and it `debug_assert`s a power of two the C++ menu's reachable level 1
isn't; the pick mirrors the pinned SFML enumerate-and-score instead), the Android EGL fallback +
named warning (the "no matching config" risk), and the wasm boolean mapping stated honestly. The
acceptance is the MEASURED `GL_SAMPLES` (`glGetIntegerv` — available on all three targets), not the
intent: the desktop half is measured on this box (Mesa 4.5 over Xvfb — the table's numbers); the
Android device half and the browser half are PENDING (adb down / no browser here — the create-time
logs are the standing probe for both).
