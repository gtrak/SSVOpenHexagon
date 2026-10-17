//! Port of `src/SSVOpenHexagon/Core/HGUpdate.cpp` — the `hg::HexagonGame`
//! per-frame update loop (plan 001 issue 09; mapping `gameplay.md` §14,
//! transcribed verbatim, C++ `float` → `f32`, `double` → `f64`, no
//! floating-point reordering).
//!
//! The C++ `HexagonGame` is a single object: `this` is both the simulation
//! and the `GameApi` backend the 007 Lua lambdas call. Here every method is
//! `&self` on the shared engine handle, mutating through two reentrant
//! locks (the shared [`SharedState`] store and the crate-local
//! [`EngineState`]); C++ statements are transcribed into short-lived guard
//! scopes so that no `&mut` borrow into a guard's contents ever outlives a
//! reentrant call (stacked-borrows discipline — D-013).
//!
//! Windowed presentation (particles, cameras, key icons, level info, text)
//! is gated on the constructor's `window: bool` exactly like the C++
//! `window != nullptr` checks; the C++ `ReplayExecution` tests run with a
//! null window, and this issue's tests do the same.

use std::time::SystemTime;

use oh_config::{Jdir, Jid, TIME_STEP};
use oh_math::ssvs::{get_vec_from_rad, Vec2};
use oh_math::ssvu::{get_clamped, get_ft_to_seconds, get_sign, PI};
use oh_math::{
    hg::{
        color::Color, concat::format_ostream_f32, easing::get_smoother_step,
        move_towards::get_move_towards_zero, string::to_uppercase,
    },
    FT,
};
use oh_replay::InputBits;

use crate::hexagon_game::{
    EngineState, HexagonGame, Particle, PreShakeCenters, SpriteState, SwapParticle,
    SwapParticleSpawnInfo, TrailParticle,
};
use crate::lua_state::SharedStore;
use crate::timeline::Outcome;
use crate::{StateChange, Timeline2Runner};

/// C++ `HexagonGame::centerPos` (`const sf::Vector2f{0.f, 0.f}` member).
const CENTER_POS: Vec2 = Vec2::new(0.0, 0.0);

/// C++ file-local `setInputImplIfFalse`.
fn set_input_impl_if_false(var: &mut bool, x: bool) {
    if !*var {
        *var = x;
    }
}

impl<S: SharedStore> HexagonGame<S> {
    /// C++ `fastForwardTo(target)` (verbatim).
    pub(crate) fn fast_forward_to(&self, target: f64) {
        let tp_begin = SystemTime::now();
        let exceeded_processing_time = || {
            SystemTime::now()
                .duration_since(tp_begin)
                .map(|d| d.as_secs_f64() > 3.0)
                .unwrap_or(false)
        };
        while !self.st().status.has_died
            && self.st().status.get_time_seconds() < target
            && !exceeded_processing_time()
        {
            self.update(TIME_STEP, 1.0_f32);
            self.post_update();
        }
    }

    /// C++ `advanceByTicks(nTicks)` (verbatim).
    pub(crate) fn advance_by_ticks(&self, n_ticks: i32) {
        for _ in 0..n_ticks {
            self.update(TIME_STEP, 1.0_f32);
            self.post_update();
        }
    }

    /// C++ `update(mFT, timescale)` — the MAIN UPDATE (verbatim).
    pub fn update(&self, mut m_ft: FT, timescale: f32) {
        // --------------------------------------------------------------------
        // Fast-forwarding for level testing
        if let Some(target) = self.est().fast_forward_target.take() {
            self.fast_forward_to(target);

            if let Some(a) = self.0.audio.as_ref() {
                // C++ `float + double -> float` narrowing:
                let new_offset = (a.get_music_playing_offset_seconds() as f64
                    + self.st().status.get_time_seconds()) as f32;
                a.set_music_playing_offset_seconds(new_offset);
            }

            return;
        }

        // --------------------------------------------------------------------
        // Advance by ticks for level testing
        if let Some(n_ticks) = self.est().advance_tick_count.take() {
            let was_paused = {
                let mut e = self.est();
                let p = e.debug_pause;
                e.debug_pause = false;
                p
            };
            self.advance_by_ticks(n_ticks);
            self.est().debug_pause = was_paused;

            return;
        }

        // --------------------------------------------------------------------
        // Update client (hexagonClient == nullptr in this port).

        // --------------------------------------------------------------------
        // Scale simulation delta frame time
        m_ft *= timescale;

        // --------------------------------------------------------------------
        // Update Discord and Steam "rich presence" (window-gated) — the
        // accounting is the C++ one, verbatim. The Steam half is the
        // engine's `steam_manager` seam (issue 020); the Discord
        // `run_callbacks` retry is issue 021; the in-game Discord activity
        // itself is set in `start` / `death`, per the C++.
        if self.0.window {
            let (name_str, diff_str, time_str) = {
                let g = self.st();
                (
                    self.name_format(&g.level_name),
                    self.diff_format(g.difficulty_mult),
                    self.time_format(g.status.get_time_seconds() as f32),
                )
            };
            {
                let mut e = self.est();
                e.time_until_rich_presence_update -= get_ft_to_seconds(m_ft);

                if e.time_until_rich_presence_update <= 0.0_f32 {
                    // C++ `steamManager->set_rich_presence_in_game(nameStr,
                    // diffStr, timeStr)` (HGUpdate.cpp:147-149) — the C++'s
                    // `steamManager != nullptr` gate is the sink's
                    // null-equivalent (`NoneSteamSink`).
                    self.0
                        .steam_manager
                        .set_rich_presence_in_game(&name_str, &diff_str, &time_str);
                    e.time_until_rich_presence_update = 5.0_f32;
                }
            }

            self.update_rich_presence_callbacks();
        }

        // --------------------------------------------------------------------
        if self.est().must_start {
            self.est().must_start = false;
            self.start();
        }

        if !self.est().debug_pause {
            self.update_flash(m_ft);
            {
                let mut e = self.est();
                e.effect_timeline_manager.update(m_ft);
            }

            if !self.must_replay_input() {
                self.update_input();
            } else {
                // C++ `SSVOH_ASSERT(activeReplay.has_value())`.
                if !self.st().status.started {
                    // C++ `window->getTimerBase().reset()` — the port has no
                    // window timer (windowed replay viewing is issue 10).
                    self.est().must_start = true;
                } else {
                    let ib: InputBits = self
                        .est()
                        .active_replay
                        .as_mut()
                        .expect("activeReplay.has_value() (SSVOH_ASSERT)")
                        .get_current_and_move_forward();

                    {
                        let mut e = self.est();
                        e.input_movement = if ib.left() {
                            -1
                        } else if ib.right() {
                            1
                        } else {
                            0
                        };
                        e.input_swap = ib.swap();
                        e.input_focused = ib.focus();
                    }
                }
            }

            // --------------------------------------------------------------------
            // Update key icons.
            if self.config().get_show_key_icons() || self.must_show_replay_ui() {
                self.update_key_icons();
            }

            // --------------------------------------------------------------------
            // Update level info.
            if self.config().get_show_level_info() || self.must_show_replay_ui() {
                self.update_level_info();
            }

            // --------------------------------------------------------------------
            // Update input leniency time after death to avoid accidental
            // restart.
            {
                let mut e = self.est();
                if e.death_input_ignore > 0.0_f32 {
                    e.death_input_ignore -= m_ft;
                }
            }

            // --------------------------------------------------------------------
            if self.st().status.started {
                self.st().style_data.compute_colors();

                {
                    let p = &mut self.est().player;
                    p.update(
                        self.get_input_focused(),
                        self.get_level_status().swap_enabled,
                        m_ft,
                    );
                }

                if !self.st().status.has_died {
                    // C++ `const std::optional<bool> preventPlayerInput =
                    // runLuaFunctionIfExists<bool, float, int, bool, bool>(
                    // "onInput", mFT, getInputMovement(), getInputFocused(),
                    // getInputSwap())`. The C++ rethrows out of `update` on
                    // error (Lippincott handler: print, goToMenu unless
                    // debug); the port applies the same handler and then
                    // proceeds with no veto (documented deviation on the
                    // error path only).
                    let prevent_player_input = match self.0.script_host.run_on_input(
                        m_ft,
                        self.get_input_movement(),
                        self.get_input_focused(),
                        self.get_input_swap(),
                    ) {
                        Ok(v) => v,
                        Err(msg) => {
                            self.host_error("onInput", &msg, true);
                            None
                        }
                    };

                    if !matches!(prevent_player_input, Some(true)) {
                        {
                            let p = &mut self.est().player;
                            p.update_input_movement(
                                self.get_input_movement() as f32,
                                self.get_player_speed_mult(),
                                self.get_input_focused(),
                                m_ft,
                            );
                        }

                        // Play "swap ready blip" sound and create particles
                        if !self.est().player_now_ready_to_swap
                            && self.est().player.is_ready_to_swap()
                        {
                            self.est().player_now_ready_to_swap = true;

                            if self.config().get_play_swap_ready_sound() {
                                self.play_sound_override("swapBlip.ogg");
                            }

                            {
                                let e = &mut self.est();
                                e.swap_particles_spawn_info = Some(SwapParticleSpawnInfo {
                                    ready: true,
                                    position: e.player.get_position(),
                                    angle: e.player.get_player_angle(),
                                });
                            }
                        }

                        // Create particles after swap
                        if self.get_level_status().swap_enabled
                            && self.get_input_swap()
                            && self.est().player.is_ready_to_swap()
                        {
                            {
                                let e = &mut self.est();
                                e.swap_particles_spawn_info = Some(SwapParticleSpawnInfo {
                                    ready: false,
                                    position: e.player.get_position(),
                                    angle: e.player.get_player_angle(),
                                });
                            }

                            self.perform_player_swap(true /* mPlaySound */);
                            {
                                let p = &mut self.est().player;
                                p.reset_swap(self.get_swap_cooldown());
                                p.set_just_swapped(true);
                            }
                            self.est().player_now_ready_to_swap = false;
                        } else {
                            self.est().player.set_just_swapped(false);
                        }
                    }

                    self.st().status.accumulate_frametime(m_ft as f64);
                    {
                        let g = self.st();
                        if g.level_status.score_overridden {
                            let name = g.level_status.score_override.clone();
                            // C++ `lua.readVariable<float>(levelStatus.scoreOverride)`
                            // — a PLAIN read (not `runLuaFunctionIfExists`):
                            // a missing/`nil` variable throws
                            // `VariableDoesntExistException` OUT of
                            // `update()`, and the SSVStart timer chain has no
                            // try/catch (`GameEngine::updateFromTimer` →
                            // `GameState::update`), so the C++ terminates the
                            // process on a broken level. The port keeps its
                            // documented, safer deviation instead (deviations.md
                            // K5 / 029m 29s1 — matching the C++ would mean
                            // crashing on a level-authoring bug): `host_error`
                            // (print + `go_to_menu(false, true)` unless
                            // debug) + skip the custom-score update for this
                            // frame; the frame continues.
                            match self.0.script_host.read_variable_f32(&name) {
                                Ok(v) => self.st().status.update_custom_score(v),
                                Err(msg) => self.host_error("readVariable", &msg, false),
                            }
                        }
                    }

                    self.update_events(m_ft);
                    self.update_increment();

                    if self.est().must_change_sides && self.st().walls.is_empty() {
                        let sides = {
                            let mut g = self.st();
                            let (min, max) = (g.level_status.sides_min, g.level_status.sides_max);
                            g.rng.get_int(min, max)
                        };
                        self.side_change(sides);
                    }

                    self.update_level(m_ft);
                    self.update_custom_timelines();

                    if self.config().get_beat_pulse() {
                        self.update_beat_pulse(m_ft);
                    }

                    self.update_pulse(m_ft);

                    if !self.config().get_black_and_white() {
                        let mut g = self.st();
                        let dm = g.difficulty_mult;
                        g.style_data.update(m_ft, dm.powf(0.8_f32));
                    }

                    self.update_player_position(self.get_radius());

                    self.update_walls(m_ft);
                    {
                        let mut g = self.st();
                        g.walls.retain(|w| !w.is_dead());
                    }

                    self.update_custom_walls(m_ft);
                } else {
                    self.st().level_status.rotation_speed *= 0.99_f32;
                }

                // This is done even with 3D disabled as it's used to affect the RNG
                // state for replay validation.
                self.update_pulse_3d(m_ft);

                if !self.config().get_no_rotation() {
                    self.update_rotation(m_ft);
                }

                self.update_camera_shake(m_ft);

                if !self.st().status.has_died {
                    // C++ `const auto fixup = [](const float x)
                    // -> random_number_generator::state_type { return x < 0.f ? -x : x; };`
                    let fixup = |x: f32| -> u64 {
                        // C++ float -> uint64 narrowing (truncation toward
                        // zero; all call sites pass small non-negative
                        // values after the abs).
                        if x < 0.0_f32 {
                            -x as u64
                        } else {
                            x as u64
                        }
                    };

                    let mut g = self.st();
                    let (pulse, pulse_3d, fast_spin, flash_effect, rotation_speed) = (
                        g.status.pulse,
                        g.status.pulse_3d,
                        g.status.fast_spin,
                        g.status.flash_effect,
                        g.level_status.rotation_speed,
                    );
                    g.rng.advance(fixup(pulse));
                    g.rng.advance(fixup(pulse_3d));
                    g.rng.advance(fixup(fast_spin));
                    g.rng.advance(fixup(flash_effect));
                    g.rng.advance(fixup(rotation_speed));
                    // TODO (P1): stuff from style?
                }
            }

            if self.0.window {
                // C++ `SSVOH_ASSERT(overlayCamera/backgroundCamera.has_value())`.
                self.update_particles(m_ft);

                if self.config().get_show_player_trail() && self.st().status.show_player_trail {
                    self.update_trail_particles(m_ft);
                }

                if self.config().get_show_swap_particles() {
                    self.update_swap_particles(m_ft);
                }

                {
                    let mut e = self.est();
                    e.overlay_camera
                        .as_mut()
                        .expect("overlayCamera (SSVOH_ASSERT)")
                        .update(m_ft);
                    e.background_camera
                        .as_mut()
                        .expect("backgroundCamera (SSVOH_ASSERT)")
                        .update(m_ft);
                }
            }
        }

        self.update_text(m_ft);

        if self.st().status.started {
            if self.st().status.must_state_change != StateChange::None {
                let execute_last_replay =
                    self.st().status.must_state_change == StateChange::MustReplay;

                if !execute_last_replay && !self.0.profiles.any_local_profile_active() {
                    // If playing a replay from file, there is no local profile
                    // active, so just go to the menu when attempting to restart
                    // the level.
                    // C++ `goToMenu()` — the default-argument call
                    // `goToMenu(true /*mSendScores*/, false /*mError*/)`
                    // (HexagonGame.hpp:417).
                    self.go_to_menu(true, false);
                    return;
                }

                let restart_id = self.est().restart_id.clone();
                let (pack_id, first_time, dm) = (
                    self.get_pack_id(),
                    self.est().restart_first_time,
                    self.st().difficulty_mult,
                );
                self.new_game(&pack_id, &restart_id, first_time, dm, execute_last_replay);
            }

            // TODO (P2): score invalidation due to performance
            #[allow(unreachable_code)]
            {
                // C++ `#if 0` performance-invalidation block (dead in C++):
                // if(!status.scoreInvalid && Config::getOfficial())
                // { invalidateScore("PERFORMANCE ISSUES"); }
                //
                if false {
                    self.invalidate_score("PERFORMANCE ISSUES");
                }
            }

            if !self.config().get_3d() && self.st().level_status._3d_required {
                self.invalidate_score("3D REQUIRED");
            }

            if !self.config().get_shaders() && self.st().level_status.shaders_required {
                self.invalidate_score("SHADERS REQUIRED");
            }
        }

        // 030b fingerprint (OFF BY DEFAULT, additive-only): the end-of-update
        // line `<frame> <hash> <draws>` — see .agents/plans/001-rust-port/
        // parity/cpp-fingerprint-definition.md (the C++ twin is the same
        // call at the end of `HexagonGame::update`, HGUpdate.cpp).
        self.maybe_emit_fingerprint();
    }

    /// 030b fingerprint (OFF BY DEFAULT, additive-only) — the port twin of
    /// the C++ `HexagonGame::maybeEmitFingerprint` (HGUpdate.cpp). The field
    /// set, ORDER, hash and the RNG trap are the definition note's; a drift
    /// in either implementation is a fingerprint-definition break.
    fn maybe_emit_fingerprint(&self) {
        let mut e = self.est();
        if e.fingerprint_file.is_none() {
            return; // OFF by default: no path, no behaviour change.
        }
        let g = self.st();

        let mut b = FingerprintBuilder::new();

        // S1/S2: the standard walls, in vector (spawn) order. The four
        // vertices carry position AND orientation (no derived trig).
        b.u32(g.walls.len() as u32);
        for w in &g.walls {
            for p in w.get_vertex_positions() {
                b.f32(p.x);
                b.f32(p.y);
            }

            let s = w.get_speed();
            b.f32(s.speed);
            b.f32(s.accel);
            b.f32(s.min());
            b.f32(s.max());
            b.f32(s.ping_pong());

            let c = w.get_curve();
            b.f32(c.speed);
            b.f32(c.accel);
            b.f32(c.min());
            b.f32(c.max());
            b.f32(c.ping_pong());

            b.f32(w.get_hue_mod());
            b.u8(u8::from(w.is_dead()));
        }

        // S3/S4: the custom walls, in ascending ALIVE-handle order.
        let alive = g.cw_manager.alive_handles();
        b.u32(alive.len() as u32);
        for &h in &alive {
            for p in g.cw_manager.get_vertex_pos4(h) {
                b.f32(p.x);
                b.f32(p.y);
            }

            b.u8(g.cw_manager.get_killing_side(h));

            let mut flags: u8 = 0;
            if !g.cw_manager.get_can_collide(h) {
                flags |= 1; // bit 0 = NoCollision (CWFlags / port bitfield)
            }
            if g.cw_manager.get_deadly(h) {
                flags |= 2; // bit 1 = Deadly
            }
            b.u8(flags);
        }

        // S5: the player (position + rotation angle).
        let p_pos = e.player.get_position();
        b.f32(p_pos.x);
        b.f32(p_pos.y);
        b.f32(e.player.get_player_angle());

        // S6: the status scalars.
        b.f32(g.status.pulse);
        b.f32(g.status.pulse_direction);
        b.f32(g.status.pulse_delay);
        b.f32(g.status.beat_pulse);
        b.f32(g.status.beat_pulse_delay);
        b.f32(g.status.pulse_3d);
        b.f32(g.status.pulse_3d_direction);
        b.f32(g.status.flash_effect);
        b.f32(g.status.radius);
        b.f32(g.status.fast_spin);
        b.f32(g.status.camera_shake);

        // S7: the status booleans.
        b.u8(u8::from(g.status.has_died));
        b.u8(match g.status.must_state_change {
            StateChange::None => 0,
            StateChange::MustRestart => 1,
            StateChange::MustReplay => 2,
        });
        b.u8(u8::from(g.status.score_invalid));
        b.u8(u8::from(g.status.started));
        b.u8(u8::from(g.status.show_player_trail));

        // S8/S9/S10: the custom score + the timekeeping (the `Final time`
        // print is exactly field 9, at the final frame).
        b.f32(g.status.get_custom_score());
        b.f64(g.status.get_time_seconds());
        b.f64(g.status.get_increment_time_seconds());

        // S11: the level-status timers.
        b.u64(g.level_status.current_increments);
        b.u32(g.level_status.sides);
        b.f32(g.level_status.speed_mult);
        b.f32(g.level_status.delay_mult);
        b.f32(g.level_status.rotation_speed);

        // S12/S13: the difficulty and the replay's recorded score.
        b.f32(g.difficulty_mult);
        b.f64(e.last_played_score);

        // S14: the entity counts.
        b.u32(e.particles.len() as u32);
        b.u32(e.trail_particles.len() as u32);
        b.u32(e.swap_particles.len() as u32);

        // S15: the game-RNG canonical state label.
        b.u64(g.rng.state_position());

        let draws_now = g.rng.draws();
        let line = format!(
            "{} {:016x} {}\n",
            e.fingerprint_frame,
            b.hash(),
            draws_now - e.fingerprint_last_draws
        );

        if let Some(file) = e.fingerprint_file.as_mut() {
            // The hook is observation-only; a write failure must not mutate
            // gameplay (the C++ twin's `fputs` failure is likewise inert).
            use std::io::Write as _;
            let _ = file.write_all(line.as_bytes());
        }

        e.fingerprint_frame += 1;
        e.fingerprint_last_draws = draws_now;
    }

    /// C++ `postUpdate()` — `postUpdate_ImguiLuaConsole()` only
    /// (`HGUpdate.cpp:1648-1651`). Ports the C++ function's headless
    /// prefix (`HGUpdate.cpp:1287-1309`):
    ///
    /// * C++ `if(window == nullptr) return;` — the port's engine has no
    ///   window (the constructor `window` flag gates the draw lane only), so
    ///   in C++ the rest of the function never runs headless. The port
    ///   instead runs the two gates the C++ applies AFTER the window check
    ///   (spec-mandated headless-testable prefix); the whole ImGui tail
    ///   (`HGUpdate.cpp:1314-1643`) is step (b).
    /// * C++ `if(!Config::getDebug() || Config::getOfficial())` → force-hide
    ///   the console and clear the pending toggle.
    /// * C++ `if(ilcShowConsoleNext)` → toggle `ilcShowConsole`, clear
    ///   `ilcShowConsoleNext` (the C++ also fires
    ///   `ImGui::SFML::ProcessEvent(GainedFocus)` — step (b)).
    /// * C++ `if(!ilcShowConsole) return;` — the console-shown tail is UI.
    pub fn post_update(&self) {
        // (the C++ `window == nullptr` return — see docs)
        if !self.config().get_debug() || self.config().get_official() {
            let mut e = self.est();
            e.ilc_show_console = false;
            e.ilc_show_console_next = false;
            return;
        }

        if self.est().ilc_show_console_next {
            let mut e = self.est();
            e.ilc_show_console = !e.ilc_show_console;
            e.ilc_show_console_next = false;
            // C++ `ImGui::SFML::ProcessEvent(sf::Event{sf::Event::GainedFocus})`
            // — step (b).
        }

        // C++ `if(!ilcShowConsole) return;` — the console-shown tail is UI
        // (step (b)); expressed as the positive condition since the tail is
        // empty here.
        if self.est().ilc_show_console {
            // C++ `ImGui::SFML::Update(*window, ilcDeltaClock.restart())` …
            // `ImGui::End()` (`HGUpdate.cpp:1314-1643`) — the console UI,
            // incl. the `InputText("Command")` Enter branch (port headlessly
            // as [`Self::post_update_imgui_lua_console_command`]) — step (b).
        }
    }

    /// C++ `updateWalls(mFT)` (verbatim).
    ///
    /// The C++ interleaves `w.update(...)` and the collision checks inside
    /// ONE loop over `walls`; the checks only READ the wall (against the
    /// pre-loop `pPos` snapshot) and only mutate the player/status, never
    /// the walls — so the port runs the complete update pass first and
    /// then the (identically ordered) collision decisions, keeping every
    /// C++ statement's effect and order.
    fn update_walls(&self, m_ft: FT) {
        let radius = self.get_radius();
        // C++ `const float radiusSquared{status.radius * status.radius + 8.f};`
        let radius_squared = radius * radius + 8.0_f32;
        // C++ `const sf::Vector2f& pPos{player.getPosition()};` — a fixed
        // snapshot taken before the loop.
        let p_pos = self.get_player_position();
        let spawn_distance = self.get_level_status().wall_spawn_distance;

        // C++ `for(CWall& w : walls) { w.update(...); ... }` update pass:
        {
            let mut g = self.st();
            for w in g.walls.iter_mut() {
                w.update(spawn_distance, radius, &CENTER_POS, m_ft);
            }
        }

        // First round (C++ loop body after `w.update`):
        let mut collided = false;
        for i in 0..self.st().walls.len() {
            let overlapping = {
                let g = self.st();
                g.walls[i].is_overlapping(&p_pos)
            };
            // If there is no collision skip to the next wall.
            if !overlapping {
                continue;
            }

            // Kill after a swap or if player could not be pushed out to safety.
            let just_swapped = self.est().player.get_just_swapped();
            // C++ `else if (player.pushCWall(...))` — the C++ only calls
            // pushCWall on the non-swap branch; the port hoists the guard
            // plumbing, not the call.
            let pushed_out = if just_swapped {
                false
            } else {
                let g = self.st();
                let wall = &g.walls[i];
                let p = &mut self.est().player;
                p.push_cwall(
                    self.get_input_movement(),
                    self.get_radius(),
                    wall,
                    &CENTER_POS,
                    radius_squared,
                    m_ft,
                )
            };
            if just_swapped {
                self.perform_player_kill();

                // C++ `steamManager->unlock_achievement("a22_swapdeath")`
                // (HGUpdate.cpp:448-451).
                self.0.steam_manager.unlock_achievement("a22_swapdeath");
            } else if pushed_out {
                self.perform_player_kill();
            }

            collided = true;
        }

        // There was no collision, so we can stop here.
        if !collided {
            return;
        }

        // Second round, always deadly...
        for i in 0..self.st().walls.len() {
            let overlapping = {
                let g = self.st();
                g.walls[i].is_overlapping(&p_pos)
            };
            if !overlapping {
                continue;
            }

            if self.est().player.get_just_swapped() {
                // C++ `steamManager->unlock_achievement("a22_swapdeath")`
                // (HGUpdate.cpp:478-481).
                self.0.steam_manager.unlock_achievement("a22_swapdeath");
            }

            self.perform_player_kill();
        }
    }

    /// C++ `updateCustomWalls(mFT)` (verbatim).
    fn update_custom_walls(&self, m_ft: FT) {
        let hit = {
            let mut g = self.st();
            let mut e = self.est();
            g.cw_manager.handle_collision(
                self.get_input_movement(),
                self.get_radius(),
                &mut e.player,
                m_ft,
            )
        };
        if hit {
            self.perform_player_kill();

            if self.est().player.get_just_swapped() {
                // C++ `steamManager->unlock_achievement("a22_swapdeath")`
                // (HGUpdate.cpp:496-499).
                self.0.steam_manager.unlock_achievement("a22_swapdeath");
            }
        }
    }

    /// C++ `updateInput()` (verbatim).
    fn update_input(&self) {
        if self.imgui_lua_console_has_input() {
            return;
        }

        {
            let e = &mut self.est();
            let started = self.st().status.started;
            let rotate_to_start = self.config().get_rotate_to_start();
            if !started && (!rotate_to_start || e.input_impl_ccw || e.input_impl_cw) {
                e.must_start = true;
            }
        }

        if self.est().always_spin_right {
            let mut e = self.est();
            e.input_impl_ccw = false;
            e.input_impl_cw = true;
            e.input_swap = false;
            e.input_focused = false;
        } else if self.est().execute_random_inputs {
            // C++ `static std::random_device rd; static std::mt19937 en(rd());`
            // + four `uniform_int_distribution<int>{0, 1}(en)` draws (test-
            // only, non-deterministic). The port draws from its per-engine
            // `pcg32_fast` (documented: the streams are not comparable).
            let mut e = self.est();
            e.input_impl_ccw = Self::draw_random_bool(&mut e.random_input_rng);
            e.input_impl_cw = Self::draw_random_bool(&mut e.random_input_rng);
            e.input_swap = Self::draw_random_bool(&mut e.random_input_rng);
            e.input_focused = Self::draw_random_bool(&mut e.random_input_rng);
        } else {
            // Keyboard and mouse state is handled by callbacks set in the
            // constructor.
            self.update_input_update_joystick_controls(); // Joystick state.
            self.update_input_update_touch_controls(); // Touchscreen state.
        }

        self.update_input_resolve_input_impl_to_input_movement();
        self.update_input_record_current_input_to_last_replay_data();
    }

    /// C++ `updateInput_UpdateJoystickControls()` (verbatim; the windowed
    /// half). The `Joystick` seam is the shared `oh_config::joystick`
    /// machine: the desktop composition feeds it from the controller
    /// backend (`oh_app::joystick::RealJoystick`, 29j1); the headless /
    /// Android / test compositions keep `NoJoystick` (every query false —
    /// the C++'s `isConnected(0) == false` state).
    fn update_input_update_joystick_controls(&self) {
        if !self.0.window {
            return;
        }

        self.0
            .joystick
            .update(self.config().get_joystick_deadzone());

        {
            let mut e = self.est();
            set_input_impl_if_false(
                &mut e.input_impl_ccw,
                self.0.joystick.pressed_dir(Jdir::Left),
            );
            set_input_impl_if_false(
                &mut e.input_impl_cw,
                self.0.joystick.pressed_dir(Jdir::Right),
            );
            set_input_impl_if_false(&mut e.input_swap, self.0.joystick.pressed_id(Jid::Swap));
            set_input_impl_if_false(&mut e.input_focused, self.0.joystick.pressed_id(Jid::Focus));
        }

        if self.0.joystick.rising_edge_id(Jid::Exit) {
            // C++ `goToMenu()` — the default-argument call
            // `goToMenu(true /*mSendScores*/, false /*mError*/)`
            // (HexagonGame.hpp:417).
            self.go_to_menu(true, false);
        } else if self.0.joystick.rising_edge_id(Jid::ForceRestart)
            || (self.st().status.has_died && self.0.joystick.rising_edge_id(Jid::Restart))
        {
            self.st().status.must_state_change = StateChange::MustRestart;
        } else if self.st().status.has_died && self.0.joystick.rising_edge_id(Jid::Replay) {
            self.st().status.must_state_change = StateChange::MustReplay;
        }
    }

    /// C++ `updateInput_UpdateTouchControls()` (verbatim; the finger
    /// positions come from the `Renderer` seam in this port).
    fn update_input_update_touch_controls(&self) {
        if !self.0.window {
            return;
        }

        let width_half = self.config().get_width() as f32 / 2.0_f32;
        let positions = self
            .0
            .renderer
            .as_ref()
            .map(|r| r.finger_down_positions())
            .unwrap_or_default();
        for p in positions {
            {
                let mut e = self.est();
                if p.x < width_half {
                    set_input_impl_if_false(&mut e.input_impl_ccw, true);
                } else {
                    set_input_impl_if_false(&mut e.input_impl_cw, true);
                }
            }
        }
    }

    /// C++ `updateInput_ResolveInputImplToInputMovement()` (verbatim).
    fn update_input_resolve_input_impl_to_input_movement(&self) {
        let mut e = self.est();

        if e.input_impl_cw && !e.input_impl_ccw {
            e.input_impl_last_movement = 1;
            e.input_movement = e.input_impl_last_movement;
            return;
        }

        if !e.input_impl_cw && e.input_impl_ccw {
            e.input_impl_last_movement = -1;
            e.input_movement = e.input_impl_last_movement;
            return;
        }

        if e.input_impl_cw && e.input_impl_ccw {
            e.input_movement = -e.input_impl_last_movement;
            return;
        }

        e.input_impl_last_movement = 0;
        e.input_movement = e.input_impl_last_movement;
    }

    /// C++ `updateInput_RecordCurrentInputToLastReplayData()` (verbatim).
    fn update_input_record_current_input_to_last_replay_data(&self) {
        let (started, has_died, left, right, swap, focus) = {
            let g = self.st();
            let e = self.est();
            (
                g.status.started,
                g.status.has_died,
                e.input_movement == -1,
                e.input_movement == 1,
                e.input_swap,
                e.input_focused,
            )
        };

        if !started || has_died {
            return;
        }

        self.est()
            .last_replay_data
            .record_input(left, right, swap, focus);
    }

    /// C++ `updateEvents(mFT)` (verbatim).
    fn update_events(&self, _m_ft: FT) {
        {
            // C++ `eventTimelineRunner.update(eventTimeline, status.getTimeTP())`
            let finished = {
                let mut g = self.st();
                let tp = g.status.get_time_tp();
                let o = {
                    let mut e = self.est();
                    e.event_timeline_runner.update(&mut g.event_timeline, tp)
                };
                matches!(o, Outcome::Finished)
            };
            if finished {
                self.st().event_timeline.clear();
                self.est().event_timeline_runner = Timeline2Runner::new();
            }
        }

        {
            // C++ `messageTimelineRunner.update(messageTimeline,
            // status.getCurrentTP())` — ABSOLUTE time (pauses included).
            let finished = {
                let mut e = self.est();
                let tp = self.st().status.get_current_tp();
                let e: &mut EngineState = &mut e;
                let (runner, timeline) = (&mut e.message_timeline_runner, &mut e.message_timeline);
                let o = runner.update(timeline, tp);
                matches!(o, Outcome::Finished)
            };
            if finished {
                self.est().message_timeline.clear();
                self.est().message_timeline_runner = Timeline2Runner::new();
            }
        }
    }

    /// C++ `updateCustomTimelines()` (verbatim).
    fn update_custom_timelines(&self) {
        let tp = self.st().status.get_current_tp();
        self.st().custom_timelines.update_all_timelines(tp);
    }

    /// C++ `updateIncrement()` (verbatim).
    fn update_increment(&self) {
        {
            let mut g = self.st();
            if !g.level_status.inc_enabled {
                return;
            }

            if g.status.get_increment_time_seconds() < f64::from(g.level_status.inc_time) {
                return;
            }

            g.level_status.current_increments += 1;
        }

        self.increment_difficulty();

        self.st().status.reset_increment_time();
        self.est().must_change_sides = true;
    }

    /// C++ `updateLevel(mFT)` (verbatim).
    fn update_level(&self, m_ft: FT) {
        if self.st().status.is_time_paused() {
            return;
        }

        // C++ `runLuaFunctionIfExists<float>("onUpdate", mFT)` (the float
        // return is discarded by the C++ too).
        if let Err(msg) = self.0.script_host.run_on_update(m_ft) {
            self.host_error("onUpdate", &msg, true);
        }

        let (finished, must_change) = {
            let mut g = self.st();
            let tp = g.status.get_time_tp();
            let o = {
                let mut e = self.est();
                e.main_timeline_runner.update(&mut g.main_timeline, tp)
            };
            (matches!(o, Outcome::Finished), self.est().must_change_sides)
        };

        if finished && !must_change {
            self.st().main_timeline.clear();
            // C++ `runLuaFunctionIfExists<void>("onStep")` — AFTER the clear,
            // BEFORE the runner reset (verbatim order).
            if let Err(msg) = self.0.script_host.run_on_step() {
                self.host_error("onStep", &msg, true);
            }
            // C++ `timelineRunner = {};`
            self.est().main_timeline_runner = Timeline2Runner::new();
        }
    }

    /// C++ `updatePulse(mFT)` (verbatim).
    fn update_pulse(&self, m_ft: FT) {
        {
            let mut g = self.st();
            if !g.level_status.manual_pulse_control {
                if g.status.pulse_delay <= 0.0_f32 {
                    let pulse_add = if g.status.pulse_direction > 0.0_f32 {
                        g.level_status.pulse_speed
                    } else {
                        -g.level_status.pulse_speed_r
                    };

                    let pulse_limit = if g.status.pulse_direction > 0.0_f32 {
                        g.level_status.pulse_max
                    } else {
                        g.level_status.pulse_min
                    };

                    g.status.pulse += pulse_add * m_ft * self.get_music_dm_sync_factor();

                    if (g.status.pulse_direction > 0.0_f32 && g.status.pulse >= pulse_limit)
                        || (g.status.pulse_direction < 0.0_f32 && g.status.pulse <= pulse_limit)
                    {
                        g.status.pulse = pulse_limit;
                        g.status.pulse_direction *= -1.0_f32;

                        if g.status.pulse_direction < 0.0_f32 {
                            g.status.pulse_delay = g.level_status.pulse_delay_max;
                        }
                    }
                }

                g.status.pulse_delay -= m_ft * self.get_music_dm_sync_factor();
            }
        }
        self.refresh_pulse();
    }

    /// C++ `refreshPulse()` (verbatim).
    pub fn refresh_pulse(&self) {
        if !self.0.window {
            return;
        }

        let p = if self.config().get_no_pulse() {
            1.0_f32
        } else {
            let g = self.st();
            g.status.pulse / g.level_status.pulse_min
        };

        {
            let mut e = self.est();
            let c = e
                .background_camera
                .as_mut()
                .expect("backgroundCamera (SSVOH_ASSERT)");
            let rotation = c.get_rotation();

            let w = self.config().get_width() as f32 * self.config().get_zoom_factor();
            let h = self.config().get_height() as f32 * self.config().get_zoom_factor();
            c.set_view(Vec2::new(0.0, 0.0), Vec2::new(w * p, h * p));

            c.set_rotation(rotation);
        }
    }

    /// C++ `updateBeatPulse(mFT)` (verbatim).
    fn update_beat_pulse(&self, m_ft: FT) {
        {
            let mut g = self.st();
            if !g.level_status.manual_beat_pulse_control {
                if g.status.beat_pulse_delay <= 0.0_f32 {
                    g.status.beat_pulse = g.level_status.beat_pulse_max;
                    g.status.beat_pulse_delay = g.level_status.beat_pulse_delay_max;
                } else {
                    g.status.beat_pulse_delay -= m_ft * self.get_music_dm_sync_factor();
                }

                if g.status.beat_pulse > 0.0_f32 {
                    g.status.beat_pulse -= (2.0_f32 * m_ft * self.get_music_dm_sync_factor())
                        * g.level_status.beat_pulse_speed_mult;
                }
            }
        }
        self.refresh_beat_pulse();
    }

    /// C++ `refreshBeatPulse()` (verbatim).
    pub fn refresh_beat_pulse(&self) {
        let radius_min = if self.config().get_beat_pulse() {
            self.st().level_status.radius_min
        } else {
            75.0_f32
        };
        let mut g = self.st();
        g.status.radius =
            radius_min * (g.status.pulse / g.level_status.pulse_min) + g.status.beat_pulse;
    }

    /// C++ `updateRotation(mFT)` (verbatim).
    fn update_rotation(&self, m_ft: FT) {
        let mut next_rotation = self.get_rotation_speed() * 10.0_f32;
        {
            let mut g = self.st();
            if g.status.fast_spin > 0.0_f32 {
                // C++ `std::abs((getSmootherStep(0, levelStatus.fastSpin,
                // status.fastSpin) / 3.5f) * 17.f) * ssvu::getSign(nextRotation)`
                // — the latent div-by-zero when `levelStatus.fastSpin == 0`
                // is preserved (mapping §16.13: replicate).
                next_rotation +=
                    ((get_smoother_step(0.0_f32, g.level_status.fast_spin, g.status.fast_spin)
                        / 3.5_f32)
                        * 17.0_f32)
                        .abs()
                        * get_sign(next_rotation) as f32;

                g.status.fast_spin -= m_ft;
            }
        }

        if self.0.window {
            // C++ `SSVOH_ASSERT(backgroundCamera.has_value())`.
            if let Some(c) = self.est().background_camera.as_mut() {
                c.turn(next_rotation);
            }
        }
    }

    /// C++ `updateCameraShake(mFT)` (verbatim).
    fn update_camera_shake(&self, m_ft: FT) {
        if self.est().background_camera.is_none() || self.est().overlay_camera.is_none() {
            return;
        }

        if self.st().status.camera_shake <= 0.0_f32 {
            let centers = self.est().pre_shake_centers.take();
            if let Some(centers) = centers {
                let mut e = self.est();
                e.background_camera
                    .as_mut()
                    .expect("backgroundCamera")
                    .set_center(centers.background);
                e.overlay_camera
                    .as_mut()
                    .expect("overlayCamera")
                    .set_center(centers.overlay);
            }

            return;
        }

        self.st().status.camera_shake -= m_ft;

        if self.est().pre_shake_centers.is_none() {
            let (background, overlay) = {
                let e = self.est();
                (
                    e.background_camera
                        .as_ref()
                        .expect("backgroundCamera")
                        .get_center(),
                    e.overlay_camera
                        .as_ref()
                        .expect("overlayCamera")
                        .get_center(),
                )
            };
            self.est().pre_shake_centers = Some(PreShakeCenters {
                background,
                overlay,
            });
        }

        // C++ `SSVOH_ASSERT(backgroundCamera/overlayCamera/preShakeCenters
        // .has_value())` — all true by construction at this point.
        //
        // C++ `makeShakeVec`: TWO `rng.get_real(-i, i)` draws (x then y),
        // `i = status.cameraShake` (post-decrement value).
        let i = self.st().status.camera_shake;
        let shake = {
            let mut g = self.st();
            Vec2::new(g.rng.get_real(-i, i), g.rng.get_real(-i, i))
        };
        let centers = self
            .est()
            .pre_shake_centers
            .expect("preShakeCenters (SSVOH_ASSERT)");
        {
            let mut e = self.est();
            e.background_camera
                .as_mut()
                .expect("backgroundCamera (SSVOH_ASSERT)")
                .set_center(centers.background + shake);
            e.overlay_camera
                .as_mut()
                .expect("overlayCamera (SSVOH_ASSERT)")
                .set_center(centers.overlay + shake);
        }
    }

    /// C++ `updateFlash(mFT)` (verbatim).
    fn update_flash(&self, m_ft: FT) {
        let a = {
            let mut g = self.st();
            if g.status.flash_effect > 0.0_f32 {
                g.status.flash_effect -= 3.0_f32 * m_ft;
            }

            g.status.flash_effect = get_clamped(g.status.flash_effect, 0.0_f32, 255.0_f32);

            // C++ `vertex.color.a = status.flashEffect` — float -> uint8
            // truncation (the value is clamped to [0, 255] first).
            g.status.flash_effect as u8
        };

        {
            let e = &mut self.est();
            for vertex in e.flash_polygon.as_mut_slice().iter_mut() {
                vertex.color.a = a;
            }
        }

        // The C++ presents the batch in `draw()` (issue 11); the port hands
        // the current batch to the attached renderer, if any.
        if let Some(r) = self.0.renderer.as_ref() {
            let flash = self.est().flash_polygon.clone();
            r.draw_flash_effect(&flash);
        }
    }

    /// C++ `updatePulse3D(mFT)` (verbatim).
    fn update_pulse_3d(&self, m_ft: FT) {
        let mut g = self.st();
        g.status.pulse_3d += g.style_data._3d_pulse_speed * g.status.pulse_3d_direction * m_ft;
        if g.status.pulse_3d > g.style_data._3d_pulse_max {
            g.status.pulse_3d_direction = -1.0_f32;
        } else if g.status.pulse_3d < g.style_data._3d_pulse_min {
            g.status.pulse_3d_direction = 1.0_f32;
        }
    }

    /// C++ `updateParticles(mFT)` (verbatim; windowed, cosmetic RNG only).
    fn update_particles(&self, m_ft: FT) {
        let width = self.config().get_width() as f32;
        let height = self.config().get_height() as f32;

        let mut e = self.est();

        // C++ `isOutOfBounds` (padding 256.f).
        e.particles.retain(|p| {
            let pos = p.sprite.position;
            !(pos.x < 0.0_f32 - 256.0_f32
                || pos.x > width + 256.0_f32
                || pos.y < 0.0_f32 - 256.0_f32
                || pos.y > height + 256.0_f32)
        });

        for p in e.particles.iter_mut() {
            p.sprite.position += p.velocity * m_ft;
            p.sprite.rotation_deg += p.angular_velocity * m_ft;
        }

        if e.must_spawn_pb_particles {
            e.next_pb_particle_spawn -= m_ft;
            if e.next_pb_particle_spawn <= 0.0_f32 {
                // C++ `makePBParticle` (HGUpdate.cpp:944-955) — ONE
                // pcg32_fast stream drawn in this exact order: position x,
                // rotation, scale, alpha (int), velocity x, velocity y,
                // angularVelocity. (y is the fixed `-64.f`; the color
                // comes from `getColorMain`, an RNG-free read.)
                let position_x = self.0.cosmetic_rng.get_rnd_r(-64.0_f32, width + 64.0_f32);
                let rotation_deg = self.0.cosmetic_rng.get_rnd_r(0.0_f32, 360.0_f32);
                let scale = self.0.cosmetic_rng.get_rnd_r(0.75_f32, 1.35_f32);
                let color = {
                    let mut c = self.get_color_main();
                    c.a = self.0.cosmetic_rng.get_rnd_i(90, 145) as u8;
                    c
                };
                let p = Particle {
                    sprite: SpriteState {
                        position: Vec2::new(position_x, -64.0_f32),
                        rotation_deg,
                        scale: Vec2::new(scale, scale),
                        color,
                        origin: Vec2::new(0.0, 0.0), // C++: the texture origin (issue 11).
                    },
                    velocity: Vec2::new(
                        self.0.cosmetic_rng.get_rnd_r(-12.0_f32, 12.0_f32),
                        self.0.cosmetic_rng.get_rnd_r(4.0_f32, 18.0_f32),
                    ),
                    angular_velocity: self.0.cosmetic_rng.get_rnd_r(-6.0_f32, 6.0_f32),
                };
                e.particles.push(p);
                e.next_pb_particle_spawn = 2.75_f32;
            }
        }
    }

    /// C++ `updateTrailParticles(mFT)` (verbatim; windowed).
    fn update_trail_particles(&self, m_ft: FT) {
        let trail_scale = self.config().get_player_trail_scale();
        let trail_alpha = self.config().get_player_trail_alpha() as u8;
        let trail_decay = self.config().get_player_trail_decay();
        let radius = self.get_radius();

        let mut e = self.est();

        // C++ `isDead`: `p.sprite.getColor().a <= 3`.
        e.trail_particles.retain(|p| p.sprite.color.a > 3);

        for p in e.trail_particles.iter_mut() {
            let new_alpha = get_move_towards_zero(p.sprite.color.a as f32, trail_decay * m_ft);
            p.sprite.color.a = new_alpha as u8;

            let s = p.sprite.scale.x * 0.98_f32;
            p.sprite.scale = Vec2::new(s, s);

            p.sprite.position = get_vec_from_rad(p.angle, radius + 2.4_f32);
        }

        if e.player.has_changed_angle() {
            // C++ `makeTrailParticle`.
            let color = {
                let mut c = self.get_color_player_trail();
                c.a = trail_alpha;
                c
            };
            let position = e.player.get_position();
            let angle = e.player.get_player_angle();
            e.trail_particles.push(TrailParticle {
                sprite: SpriteState {
                    position,
                    rotation_deg: 0.0,
                    scale: Vec2::new(trail_scale, trail_scale),
                    color,
                    // C++: `sf::Vector2f{txSmallCircle->getSize()} / 2.f`
                    // (the texture half-size — a render-backend concern,
                    // issue 11).
                    origin: Vec2::new(0.0, 0.0),
                },
                angle,
            });
        }
    }

    /// C++ `updateSwapParticles(mFT)` (verbatim; windowed, cosmetic RNG).
    #[allow(clippy::approx_constant)] // C++-verbatim `3.14f` expand values
    fn update_swap_particles(&self, m_ft: FT) {
        let mut e = self.est();

        // C++ `isDead`: `p.sprite.getColor().a <= 3`.
        e.swap_particles.retain(|p| p.sprite.color.a > 3);

        for p in e.swap_particles.iter_mut() {
            let new_alpha = get_move_towards_zero(p.sprite.color.a as f32, 3.5_f32 * m_ft);
            p.sprite.color.a = new_alpha as u8;

            let s = p.sprite.scale.x * 0.98_f32;
            p.sprite.scale = Vec2::new(s, s);
            p.sprite.position += p.velocity * m_ft;
        }

        if let Some(si) = e.swap_particles_spawn_info {
            // C++ `makeSwapParticle` (draw order per particle: scale,
            // angle-offset, speed).
            let make = |expand: f32, speed_mult: f32, scale_mult: f32, alpha: f32| {
                let scale = self.0.cosmetic_rng.get_rnd_r(0.65_f32, 1.35_f32) * scale_mult;
                let c = {
                    let mut c = self.get_color_player_trail();
                    c.a = alpha as u8;
                    c
                };
                let velocity = get_vec_from_rad(
                    si.angle + self.0.cosmetic_rng.get_rnd_r(-expand, expand),
                    self.0.cosmetic_rng.get_rnd_r(0.1_f32, 10.0_f32) * speed_mult,
                );
                SwapParticle {
                    sprite: SpriteState {
                        position: si.position,
                        rotation_deg: 0.0,
                        scale: Vec2::new(scale, scale),
                        color: c,
                        // C++: `sf::Vector2f{txSmallCircle->getSize()} / 2.f`.
                        origin: Vec2::new(0.0, 0.0),
                    },
                    velocity,
                }
            };

            if !si.ready {
                // after-swap burst:
                for _ in 0..20 {
                    e.swap_particles.push(make(
                        0.45_f32, /* expand */
                        1.0_f32,  /* speedMult */
                        1.0_f32,  /* scaleMult */
                        45.0_f32, /* alpha */
                    ));
                }

                for _ in 0..10 {
                    e.swap_particles.push(make(
                        3.14_f32, /* expand */
                        0.45_f32, /* speedMult */
                        0.75_f32, /* scaleMult */
                        35.0_f32, /* alpha */
                    ));
                }
            } else {
                // ready blip:
                for _ in 0..14 {
                    e.swap_particles.push(make(
                        3.14_f32,  /* expand */
                        1.3_f32,   /* speedMult */
                        0.4_f32,   /* scaleMult */
                        140.0_f32, /* alpha */
                    ));
                }
            }

            e.swap_particles_spawn_info = None;
        }
    }

    /// C++ `updateKeyIcons()` — windowed sprite-geometry update (icon
    /// positions/scales for the issue-11 draw pass); a no-op here, exactly
    /// the C++ headless behavior (`window == nullptr`).
    fn update_key_icons(&self) {
        // (the key-icon sprites are issue-11 render objects; mapping
        // gameplay.md §15 "UI update helpers" pins the geometry)
    }

    /// C++ `updateLevelInfo()` — windowed rectangle/text layout (see
    /// `update_key_icons`).
    fn update_level_info(&self) {
        // (issue-11 render objects)
    }

    /// C++ `updateText(mFT)` (HGGraphics.cpp) — the string-content half,
    /// verbatim; the SFML `setCharacterSize`/`setOrigin` calls are issue-11
    /// render properties and don't affect game state. Window-gated, exactly
    /// like the C++.
    pub fn update_text(&self, m_ft: FT) {
        if !self.0.window {
            return;
        }

        // --------------------------------------------------------------------
        // Update "personal best" text animation.
        {
            let mut e = self.est();
            e.pb_text_growth += 0.08_f32 * m_ft;
            if e.pb_text_growth > PI * 2.0_f32 {
                e.pb_text_growth = 0.0;
            }
        }

        // --------------------------------------------------------------------
        let mut os = String::new();
        {
            let g = self.st();
            let mut e = self.est();
            if e.debug_pause {
                os.push_str("(!) PAUSED (!)\n");
            }

            if g.level_status.tutorial_mode {
                os.push_str("TUTORIAL MODE\n");
            } else if self.config().get_official() {
                os.push_str("OFFICIAL MODE\n");
            }

            if self.config().get_debug() {
                os.push_str("DEBUG MODE\n");

                os.push_str(&format!(
                    "CUSTOM WALLS: {} / {}\n",
                    g.cw_manager.count(),
                    g.cw_manager.max_handles()
                ));
            }

            if g.status.started {
                if g.level_status.swap_enabled {
                    os.push_str("SWAP ENABLED\n");
                }

                if self.config().get_invincible() {
                    os.push_str("INVINCIBILITY ON\n");
                }

                let timescale = self.config().get_timescale();
                if timescale != 1.0_f32 {
                    os.push_str(&format!("TIMESCALE {}\n", format_ostream_f32(timescale)));
                }

                if g.status.score_invalid {
                    os.push_str(&format!(
                        "SCORE INVALIDATED ({})\n",
                        g.status.invalid_reason
                    ));
                }

                if g.status.has_died {
                    os.push_str(&g.status.restart_input);
                    os.push_str(&g.status.replay_input);
                }

                let n = g.called_deprecated_functions.len();
                if n > 1 {
                    os.push_str(&format!("{n} WARNINGS RAISED (CHECK CONSOLE)\n"));
                } else if n > 0 {
                    os.push_str("1 WARNING RAISED (CHECK CONSOLE)\n");
                }

                let tracked_variables = g.level_status.tracked_variables.clone();
                if self.config().get_show_tracked_variables() && !tracked_variables.is_empty() {
                    os.push('\n');
                    for (variable_name, display) in &tracked_variables {
                        if !self.0.script_host.variable_exists(variable_name) {
                            continue;
                        }

                        let value = match self.0.script_host.read_variable_string(variable_name) {
                            Ok(v) => v,
                            Err(msg) => {
                                self.host_error("readVariable", &msg, false);
                                continue;
                            }
                        };

                        os.push_str(&format!(
                            "{}: {}\n",
                            to_uppercase(display.to_string()),
                            to_uppercase(value)
                        ));
                    }
                }
            } else if self.config().get_rotate_to_start() {
                os.push_str("ROTATE TO START\n");
                e.message_text = "ROTATE TO START".to_string();
            }
        }

        // Set in game timer text
        {
            let g = self.st();
            let e = &mut self.est();
            if !g.level_status.score_overridden {
                // By default, use the timer for scoring
                e.time_text = if g.status.started {
                    self.format_time(g.status.get_time_seconds())
                } else {
                    "0".to_string()
                };
            } else {
                // Alternative scoring
                let name = g.level_status.score_override.clone();
                e.time_text = match self.0.script_host.read_variable_string(&name) {
                    Ok(v) => v,
                    Err(msg) => {
                        self.host_error("readVariable", &msg, false);
                        String::new()
                    }
                };
            }
        }

        // Set FPS Text, if option is enabled.
        if self.config().get_show_fps() {
            // C++ `ssvu::toStr(window->getFPS())` (HGGraphics.cpp:562) —
            // `ssvu::toStr(float)` (SSVUtils/Core/String/ToStr.hpp:63 →
            // the default stringifier, StringifierBase.hpp:24) is the
            // plain `std::ostream << float` (6 significant digits, no
            // trailing zeros — e.g. 120.0 → "120", 59.9342 → "59.9342"),
            // NOT `std::to_string`'s fixed 6 decimals (the graphics
            // audit's M3: the old `fps.to_string()` of the `u32` seam
            // value pinned the constant "0"). The `format_ostream_f32`
            // oracle-pinned port (the same formatting the TIMESCALE line
            // above uses). The Renderer seam stands in for the window's
            // FPS (the `float` — the C++'s `window->getFPS()` is the
            // previous frame's `runFPS` value, 0 on the first frame;
            // TimerBase.hpp:23/45-61, GameWindow.hpp:148).
            let fps = self.0.renderer.as_ref().map(|r| r.fps()).unwrap_or(0.0);
            self.est().fps_text = format_ostream_f32(fps);
        }

        // --------------------------------------------------------------------
        if self.must_show_replay_ui() {
            let rf = self
                .active_replay_file()
                .expect("activeReplay (mustShowReplayUI)");
            let mut ros = String::new();

            if !self.st().level_status.score_overridden {
                ros.push_str(&self.format_time(rf.played_seconds()));
                ros.push('s');
            } else {
                ros.push_str(&self.format_time(rf.played_score));
            }

            ros.push_str(&format!(" BY {}", rf.player_name));

            self.est().replay_text = ros;
        } else {
            self.est().replay_text.clear();
        }

        // C++ `text.setString(os.str())` — the status text content.
        self.est().status_text = os;
    }

    /// C++ `updateRichPresenceCallbacks()` (HexagonGame.cpp:477-508) — the
    /// Steam arm (the `steamAttempt`/`steamHung` hung counter,
    /// HexagonGame.hpp:90,92 — delegated to the engine's `steam_manager`
    /// seam, which owns the counter in the port — issue 020) + the Discord
    /// `run_callbacks` + the `discordAttempt` / `discordHung` retry.
    /// Driven every frame
    /// from BOTH states: the in-game `update` (inside the C++'s
    /// `window != nullptr` block, HGUpdate.cpp:155) and the menu's
    /// `fnHGUpdateRichPresenceCallbacks` (main.cpp:451-454). The retry state
    /// is a `HexagonGame` member (HexagonGame.hpp:89,91), NOT a
    /// `discord_manager` one — so it lives on the engine here. `pub` because
    /// the menu's per-frame callback (the app's `RichPresenceCallback`) calls
    /// it through the engine handle.
    pub fn update_rich_presence_callbacks(&self) {
        // C++ HexagonGame.cpp:480-492 — the Steam arm.
        self.0.steam_manager.update_rich_presence_callbacks();
        // C++ `if(discordManager != nullptr && !discordHung)`
        // (HexagonGame.cpp:495).
        let Some(discord) = self.0.discord.as_ref() else {
            return;
        };
        let mut e = self.est();
        if e.discord_hung {
            return;
        }
        // C++ `if(!discordManager->run_callbacks())` (HexagonGame.cpp:497).
        if !discord.run_callbacks() {
            e.discord_attempt += 1;
            // C++ `if(discordAttempt > 20)` (HexagonGame.cpp:500).
            if e.discord_attempt > 20 {
                e.discord_hung = true;
                // C++ `ssvu::lo("Discord") << "Too many failed callbacks.
                // Stopping Discord callbacks.\n"` (HexagonGame.cpp:503-504).
                println!("[Discord] Too many failed callbacks. Stopping Discord callbacks.");
            }
        }
    }

    /// The `particles` vector (test access; the C++ member).
    #[must_use]
    pub fn particles(&self) -> Vec<Particle> {
        self.est().particles.clone()
    }

    /// The `trailParticles` vector (test access).
    #[must_use]
    pub fn trail_particles(&self) -> Vec<TrailParticle> {
        self.est().trail_particles.clone()
    }

    /// The `swapParticles` vector (test access).
    #[must_use]
    pub fn swap_particles(&self) -> Vec<SwapParticle> {
        self.est().swap_particles.clone()
    }

    /// The C++ `mustSpawnPBParticles` flag (test access).
    #[must_use]
    pub fn must_spawn_pb_particles(&self) -> bool {
        self.est().must_spawn_pb_particles
    }

    /// The C++ `pbText` content (test access).
    #[must_use]
    pub fn pb_text(&self) -> String {
        self.est().pb_text.clone()
    }

    /// The C++ `messageText` content (test access).
    #[must_use]
    pub fn message_text(&self) -> String {
        self.est().message_text.clone()
    }

    /// The C++ `os`/`text` status-text content (test access).
    #[must_use]
    pub fn status_text(&self) -> String {
        self.est().status_text.clone()
    }

    /// The C++ `timeText` content (test access).
    #[must_use]
    pub fn time_text(&self) -> String {
        self.est().time_text.clone()
    }

    /// The C++ `replayText` content (test access).
    #[must_use]
    pub fn replay_text(&self) -> String {
        self.est().replay_text.clone()
    }

    /// The C++ `fpsText` content (test access).
    #[must_use]
    pub fn fps_text(&self) -> String {
        self.est().fps_text.clone()
    }

    /// The C++ `effectTimelineManager` (test access via a count of pending
    /// entries; the manager never stores data the tests need beyond firing).
    #[must_use]
    pub fn effect_timeline_pending(&self) -> usize {
        self.est().effect_timeline_manager.entries_count()
    }
}

// 030b fingerprint — the FNV-1a 64 field builder (the definition note:
// .agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md, v1).
// Floats enter as RAW IEEE-754 bit patterns (never an epsilon compare: a
// 1-ULP divergence must change the hash). The byte order and field order
// must stay bit-identical to the C++ twin (HGUpdate.cpp).
struct FingerprintBuilder {
    hash: u64,
}

impl FingerprintBuilder {
    #[must_use]
    const fn new() -> Self {
        Self {
            hash: 0xcbf29ce484222325,
        }
    }

    fn mix_byte(&mut self, byte: u8) {
        self.hash ^= u64::from(byte);
        self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV-1a 64 prime (0x100000001b3)
    }

    fn u8(&mut self, v: u8) {
        self.mix_byte(v);
    }

    fn u32(&mut self, v: u32) {
        self.u8(v as u8);
        self.u8((v >> 8) as u8);
        self.u8((v >> 16) as u8);
        self.u8((v >> 24) as u8);
    }

    fn u64(&mut self, v: u64) {
        self.u32(v as u32);
        self.u32((v >> 32) as u32);
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    #[must_use]
    const fn hash(&self) -> u64 {
        self.hash
    }
}

// Color import re-check: `Color` is used by `SpriteState` callers below the
// engine tests; re-exported through `hexagon_game::SpriteState`.
#[allow(unused_imports)]
use Color as _Color;
