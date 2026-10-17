# Audit: `src/SSVOpenHexagon/Core/HGUpdate.cpp` ↔ `crates/oh-game/src/update.rs`

Module pair for the 1:1 Rust port. Read COMPLETELY:
- C++: `src/SSVOpenHexagon/Core/HGUpdate.cpp` (1652 lines, all 27 member fns + 3 file-statics).
- Rust: `crates/oh-game/src/update.rs` (1608 lines, all fns), plus cross-referenced
  `crates/oh-game/src/lua_console.rs` (2264 lines), `crates/oh-game/src/hexagon_game.rs`,
  `crates/oh-game/src/seams.rs`, `crates/oh-game/src/renderer.rs`,
  `crates/oh-app/src/lua_console_ui.rs` (the "step (b)" egui console UI), and the C++
  oracle files `HexagonGame.cpp/.hpp`, `HGGraphics.cpp`, `HGScripting.cpp`, `CWall.cpp`,
  `CPlayer.cpp`, `Utils.hpp`, `LuaWrapper.hpp`, SSVUtils `Rnd.hpp`, SSVStart
  `GameState.hpp`/`GameEngine.hpp`.

Method: (1) behavior inventory of the C++ module; (2) implemented list of the Rust side;
(3) adversarial diff; every suspected absence verified by grep (commands in each row).

---

## C++ BEHAVIOR INVENTORY (HGUpdate.cpp)

| # | Behavior | C++ file:line |
|---|---|---|
| 1 | `fastForwardTo(target)` — tick-loop `update(TIME_STEP,1)`+`postUpdate()` until `hasDied`/time reached/3 s wall-clock cap | 54-70 |
| 2 | `advanceByTicks(nTicks)` — N× `update`+`postUpdate` | 72-79 |
| 3 | `update(mFT, timescale)` — main loop: fast-forward consume (85-103, incl. music-offset `float+double→float`), advance-ticks consume w/ `debugPause` save-restore (106-119), `hexagonClient->update()` (121-123), `mFT *= timescale` (127), window-gated rich-presence accounting: `nameFormat`/`diffFormat`/`timeFormat`, 5 s `timeUntilRichPresenceUpdate`, `steamManager->set_rich_presence_in_game`, `updateRichPresenceCallbacks()` (130-159) | 81-426 |
| 4 | `update` — `mustStart → start()` (160-166) | 160-166 |
| 5 | `update` — `!debugPause`: `updateFlash`, `effectTimelineManager.update` (168-169); input: `updateInput()` or replay branch: assert `activeReplay`, `!started → window->getTimerBase().reset()` + `mustStart` (179-188) else `get_current_and_move_forward()` → movement/swap/focus (190-206); key icons gate (213-216); level info gate (219-222); `deathInputIgnore` decay (225-228) | 168-228 |
| 6 | `update` — `status.started`: `styleData.computeColors()` (237); `player.update` (239); `!hasDied`: Lua `onInput` veto via `runLuaFunctionIfExists<bool,...>` (241-246) → `player.updateInputMovement` (249-250); swap-ready blip (`Config::getPlaySwapReadySound`, `swapBlip.ogg`, `SwapParticleSpawnInfo{ready:true}`) (258-269); swap (`swapEnabled && inputSwap && isReadyToSwap` → `SwapParticleSpawnInfo{ready:false}`, `performPlayerSwap(true)`, `resetSwap(getSwapCooldown())`, `setJustSwapped(true)`, else `setJustSwapped(false)`) (272-290); `status.accumulateFrametime` (294); `scoreOverridden → lua.readVariable<float>(scoreOverride)` → `updateCustomScore` (295-299); `updateEvents` (301); `updateIncrement` (302); `mustChangeSides && walls.empty() → sideChange(rng.get_int(sidesMin,sidesMax))` (304-308); `updateLevel` (310); `updateCustomTimelines` (311); `Config::getBeatPulse → updateBeatPulse` (313-315); `updatePulse` (317); `!getBlackAndWhite → styleData.update(mFT, pow(difficultyMult,0.8))` (319-321); `player.updatePosition(getRadius())` (323); `updateWalls` (325); `eraseRemoveIf walls isDead` (326-328); `updateCustomWalls` (330); else `levelStatus.rotationSpeed *= 0.99f` (334) | 235-335 |
| 7 | `update` — `updatePulse3D` (always, 347-region; "affects RNG state for replay validation" comment at 337-338); `!getNoRotation → updateRotation` (340-342); `updateCameraShake` (344); `!hasDied → rng.advance(fixup(pulse|pulse3D|fastSpin|flashEffect|rotationSpeed))` with abs-fixup (347-368, `TODO (P1): stuff from style?` at 368) | 337-368 |
| 8 | `update` — window-gated: `updateParticles`, `getShowPlayerTrail && showPlayerTrail → updateTrailParticles`, `getShowSwapParticles → updateSwapParticles`, overlay/background `camera->update` (371-388); `updateText(mFT)` (390); `started && mustStateChange`: `MustReplay` flag, `!executeLastReplay && !anyLocalProfileActive → goToMenu()` (392-403), `newGame(packId, restartId, restartFirstTime, difficultyMult, executeLastReplay)` (407-411); `#if 0` performance invalidation (415-420); `!get3D && _3DRequired → invalidateScore("3D REQUIRED")` (422-424); `!getShaders && shadersRequired → invalidateScore("SHADERS REQUIRED")` (425-427) | 371-427 |
| 9 | `updateWalls` — `collided`, `radiusSquared = radius²+8`, `pPos` pre-loop snapshot; per wall: `w.update(wallSpawnDistance, getRadius(), centerPos, mFT)` then `isOverlapping(pPos)`; justSwapped → `performPlayerKill()` + steam `a22_swapdeath` (448-451); else `player.push(...)` true → `performPlayerKill()` (453-457); early out if `!collided` (462-466); second round always deadly, re-unlocks `a22_swapdeath` if justSwapped (468-485) | 427-486 |
| 10 | `updateCustomWalls` — `cwManager.handleCollision(...)` true → `performPlayerKill()` + `a22_swapdeath` if justSwapped (496-499) | 488-502 |
| 11 | `start()` — `status.start()`, `messageText=""`, `go.ogg`; non-replay: nameFormat/pack nameFormat/diffFormat + Discord `set_rich_presence_in_game(name " [x"+diff+"]", pack)` (512-524); client: `trySendStartedGame(validator)` when `LoggedIn_Ready && getOfficial && !unscored && isLevelSupportedByServer` (526-532); replay: Discord `set_rich_presence_on_replay()` (534-541); `audio->resumeMusic()` when `!getNoMusic` (543-547); Lua `onLoad` (549) | 504-550 |
| 12 | file-static `setInputImplIfFalse(var, x)` | 552-558 |
| 13 | `updateInput_UpdateJoystickControls` — window-gated; `Joystick::update(deadzone)`; `setInputImplIfFalse` for Left/Right/`Swap`/`Focus`; `risingEdge(Exit) → goToMenu()` (576); `risingEdge(ForceRestart) \|\| (hasDied && risingEdge(Restart)) → MustRestart`; `hasDied && risingEdge(Replay) → MustReplay` (574-587) | 560-587 |
| 14 | `updateInput_UpdateTouchControls` — window-gated; per finger down position: `p.x < windowSize.x/2 → inputImplCCW` else `inputImplCW` | 589-606 |
| 15 | `updateInput_ResolveInputImplToInputMovement` — CW-only → +1; CCW-only → -1; both → `-inputImplLastMovement`; neither → 0 (updates `inputImplLastMovement`) | 609-629 |
| 16 | `updateInput_RecordCurrentInputToLastReplayData` — skip unless `started && !hasDied`; `lastReplayData.record_input(left,right,swap,focus)` | 632-645 |
| 17 | `updateInput` — `imguiLuaConsoleHasInput() → return` (648-650); `!started && (!getRotateToStart \|\| ccw \|\| cw) → mustStart` (652-657); `alwaysSpinRight` force (660-665); `executeRandomInputs` → 4 bools from **file-static** `mt19937(random_device)` (667-677); else joystick + touch; resolve; record | 647-687 |
| 18 | `updateEvents` — event timeline @ `getTimeTP()`; message timeline @ `getCurrentTP()`; on `finished`: clear + reset runner | 689-706 |
| 19 | `updateCustomTimelines` — `_customTimelineManager.updateAllTimelines(getCurrentTP())` | 708-710 |
| 20 | `updateIncrement` — `!incEnabled`/time-not-reached early-outs; `++currentIncrements`; `incrementDifficulty()`; `resetIncrementTime()`; `mustChangeSides = true` | 713-729 |
| 21 | `updateLevel` — `isTimePaused → return`; Lua `onUpdate(mFT)`; main timeline @ `getTimeTP()`; on finished && `!mustChangeSides`: clear, Lua `onStep`, reset runner | 731-748 |
| 22 | `updatePulse` — non-manual: pulseDelay gate, direction-signed speed/limit, `pulse += add*mFT*getMusicDMSyncFactor()`, clamp + direction flip + `pulseDelayMax` when going negative; `pulseDelay -= mFT*factor`; `refreshPulse()` | 750-782 |
| 23 | `refreshPulse` — window-gated; `p = getNoPulse ? 1 : pulse/pulseMin`; set background view `zeroVec2f × (width*zoom*p, height*zoom*p)`, preserve rotation | 784-800 |
| 24 | `updateBeatPulse` — non-manual: delay gate → `beatPulse = beatPulseMax`, `delay = delayMax`; else `delay -= mFT*factor`; `beatPulse -= 2*mFT*factor*beatPulseSpeedMult` while > 0; `refreshBeatPulse()` | 802-823 |
| 25 | `refreshBeatPulse` — `radiusMin = getBeatPulse ? levelStatus.radiusMin : 75`; `status.radius = radiusMin*(pulse/pulseMin) + beatPulse` | 825-830 |
| 26 | `updateRotation` — `nextRotation = getRotationSpeed()*10`; `fastSpin>0`: add `abs(getSmootherStep(0, levelStatus.fastSpin, status.fastSpin)/3.5*17)*getSign(nextRotation)` (latent div-by-zero preserved), `fastSpin -= mFT`; window-gated `backgroundCamera->turn` | 832-851 |
| 27 | `updateCameraShake` — cameras-none guard; `cameraShake<=0`: restore `preShakeCenters` + reset; else decrement, capture centers if absent, TWO `rng.get_real(-i,i)` draws (post-decrement `i`), offset both centers | 853-893 |
| 28 | `updateFlash` — `flashEffect -= 3*mFT` if > 0, clamp [0,255], set every `flashPolygon` vertex `color.a` | 895-908 |
| 29 | `updatePulse3D` — `pulse3D += _3dPulseSpeed*_3dPulseDirection*mFT`; bounce at `_3dPulseMax`/`_3dPulseMin` (direction flip) | 910-921 |
| 30 | `updateParticles` — drop out-of-bounds (±256 padding); move/rotate; `mustSpawnPBParticles`: `nextPBParticleSpawn -= mFT`, spawn when ≤ 0: PB particle from **global** pcg32_fast in order **x(−64..w+64), rot(0..360), scale(0.75..1.35), alpha int(90..145 excl.), vx(−12..12), vy(4..18), angVel(−6..6)** (944-955), `nextPBParticleSpawn = 2.75` | 923-978 |
| 31 | `updateTrailParticles` — drop `a<=3`; per particle `moveTowardsZero(a, decay*mFT)`, `scale *= 0.98`, `pos = vecFromRad(angle, radius+2.4)`; spawn on `player.hasChangedAngle()` (trail color/alpha/scale from Config) | 980-1031 |
| 32 | `updateSwapParticles` — drop `a<=3`; per particle alpha decay `3.5*mFT`, `scale *= 0.98`, `pos += velocity*mFT`; consume `swapParticlesSpawnInfo`: not-ready → 20×(expand 0.45, speed 1.0, scale 1.0, alpha 45) + 10×(3.14, 0.45, 0.75, 35); ready → 14×(3.14, 1.3, 0.4, 140); per-particle RNG order: **scale, angle-offset, speed** | 1033-1116 |
| 33 | file-statics `ilcTextEditCallbackStub` (1118), `Stricmp` (1124), `Strnicmp` (1135) — `#ifndef SSVOH_ANDROID` | 1118-1146 |
| 34 | `ilcTextEditCallback` — `#ifndef SSVOH_ANDROID`: Completion: word scan (delims space/tab/comma/semicolon, skip leading `?`), candidates vs `LuaScripting::getAllFunctionNames()` by case-insensitive prefix; 0 → log `No match…`; 1 → replace word + trailing space; >1 → longest-common-prefix completion + `Possible matches` list (1152-1237). History: Up/Down over `ilcHistory` with −1 sentinel, whole-buffer replace (1247-1280). Android build: return 0 | 1148-1285 |
| 35 | `postUpdate_ImguiLuaConsole` — `#ifndef SSVOH_ANDROID`: window-null → return (1289-1292); `!getDebug \|\| getOfficial → force-hide console + clear pending toggle` (1294-1300); `ilcShowConsoleNext → toggle + clear + ImGui ProcessEvent(GainedFocus)` (1301-1308); `!ilcShowConsole → return` (1309-1312); else full ImGui console: `SFML::Update(window, ilcDeltaClock.restart())` (1314), window 600×700 first-use, help line, colored log rendering (`[error]`/`[warning]`/`[lua]`/`# ` prefix/`[?]`), 80-char line wrap, autoscroll; `InputText("Command")` Enter (1425-1545): echo `# cmd`, history dedupe+push, split, `!CLEAR`/`!HELP` (verbatim block 1454-1461)/`!ff` (`stod`, sets `fastForwardTarget`, ostream-`%g` log, `invalid_argument`/`out_of_range` error strings)/`!advt` (`stoi`, `ticks>=0?ticks:0`, reuses `<seconds>` wording 1499/1503)/`?fn` (`getRTrim`, `getDocsForFunction`)/fallback (`u_log(cmd)` then bare `cmd`, nested-catch `[error]:`/`[error]: unknown`); default-focus calls; `update ms`/`draw ms` footer; `static simSpeed/invincible` + `DragFloat("Timescale")`/`Checkbox("Invincible")` with **per-frame `Config::setTimescale`/`setInvincible` write-back** (1549-1557); Track `InputText` Enter (LR-trim, `u_impl_addTrackedResult` snippet + name, 1562-1572); `Untrack All` button (1577-1581); tracked-results loop (execute each, first error → exact `[error]: …` line, erase pair, stop; asserts 1623-1624; `TrackedResults` table 1627-1641) | 1287-1646 |
| 36 | `postUpdate()` = `postUpdate_ImguiLuaConsole()` only | 1648-1651 |

Globals/statics touched: `Config::*` (many), `window`, `steamManager`, `discordManager`,
`hexagonClient`, `audio`, `rng` (game), SSVUtils **global** pcg32_fast (`getRndR/getRndI`),
`status`, `levelData`/`levelStatus`, `styleData`, `player`, `walls`, `cwManager`,
`eventTimeline(+Runner)`, `messageTimeline(+Runner)`, `timeline(+Runner)`,
`_customTimelineManager`, `effectTimelineManager`, `flashPolygon`,
`particles/trailParticles/swapParticles`, `txStarParticle/txSmallCircle`,
`preShakeCenters`, `mustStart/debugPause/mustChangeSides/restartId/restartFirstTime/
deathInputIgnore/playerNowReadyToSwap/swapParticlesSpawnInfo/alwaysSpinRight/
executeRandomInputs/inputImplCCW/CW/inputImplLastMovement/inputMovement/inputSwap/
inputFocused/fastForwardTarget/advanceTickCount/timeUntilRichPresenceUpdate/
steamAttempt/steamHung/discordAttempt/discordHung/lastReplayData/activeReplay/
messageText/pbText/timeText/text/replayText/fpsText/pbTextGrowth/calledDeprecatedFunctions/
ilc* (ShowConsole, ShowConsoleNext, CmdLog, CmdBuffer, History, HistoryPos,
LuaTracked, LuaTrackedNames, LuaTrackedResults, TrackBuffer, DeltaClock)`.
Side effects: sounds (`swapBlip.ogg`, `go.ogg`), steam achievement `a22_swapdeath` (×3
sites), Discord/Steam rich presence, `hexagonClient->trySendStartedGame`, music resume,
Lua hooks (`onInput`, `onLoad`, `onUpdate`, `onStep`), ImGui console (Android-excluded).

---

## RUST IMPLEMENTED LIST (update.rs + collaborators)

| # | Rust item | file:line | C++ counterpart |
|---|---|---|---|
| 1 | `set_input_impl_if_false` | update.rs:45-50 | HGUpdate.cpp:552 |
| 2 | `fast_forward_to` | update.rs:53-70 | #1 |
| 3 | `advance_by_ticks` | update.rs:71-78 | #2 |
| 4 | `update` (main) | update.rs:79-470 | #3-8 |
| 5 | `post_update` (console gate prefix only) | update.rs:493-527 | #35 (head) |
| 6 | `update_walls` (two-pass: update pass, then collision pass) | update.rs:529-615 | #9 |
| 7 | `update_custom_walls` | update.rs:617-638 | #10 |
| 8 | `update_input` | update.rs:640-681 | #17 |
| 9 | `update_input_update_joystick_controls` | update.rs:683-717 | #13 |
| 10 | `update_input_update_touch_configs… touch_controls` | update.rs:719-742 | #14 |
| 11 | `update_input_resolve_input_impl_to_input_movement` | update.rs:744-767 | #15 |
| 12 | `update_input_record_current_input_to_last_replay_data` | update.rs:769-791 | #16 |
| 13 | `update_events` | update.rs:793-828 | #18 |
| 14 | `update_custom_timelines` | update.rs:830-834 | #19 |
| 15 | `update_increment` | update.rs:836-855 | #20 |
| 16 | `update_level` | update.rs:857-889 | #21 |
| 17 | `update_pulse` | update.rs:891-927 | #22 |
| 18 | `refresh_pulse` | update.rs:929-956 | #23 |
| 19 | `update_beat_pulse` | update.rs:958-977 | #24 |
| 20 | `refresh_beat_pulse` | update.rs:979-989 | #25 |
| 21 | `update_rotation` | update.rs:991-1018 | #26 (div-by-zero preserved, update.rs:999-1001) |
| 22 | `update_camera_shake` | update.rs:1020-1090 | #27 (two `get_real` draws, post-decrement `i`) |
| 23 | `update_flash` (+ renderer push) | update.rs:1092-1120 | #28 |
| 24 | `update_pulse_3d` | update.rs:1122-1131 | #29 |
| 25 | `update_particles` | update.rs:1133-1188 | #30 (RNG order differs — see MISSING 2) |
| 26 | `update_trail_particles` | update.rs:1190-1236 | #31 |
| 27 | `update_swap_particles` | update.rs:1238-1316 | #32 (per-particle order scale/angle/speed ✓) |
| 28 | `update_key_icons` — **stub**: body is a comment only | update.rs:1318-1324 | #5 call site (def in C++ HexagonGame.cpp:109) |
| 29 | `update_level_info` — **stub**: body is a comment only | update.rs:1325-1329 | #5 call site (def in C++ HexagonGame.cpp:159) |
| 30 | `update_text` (ported from C++ **HGGraphics.cpp:421**, not HGUpdate.cpp) | update.rs:1333-1508 | HGGraphics.cpp:421-602 |
| 31 | `update_rich_presence_callbacks` (Steam arm → seam; Discord arm inline) | update.rs:1511-1536 | HexagonGame.cpp:477-505 |
| 32 | test accessors (`particles`, `trail_particles`, `swap_particles`, `must_spawn_pb_particles`, `pb_text`, `message_text`, `status_text`, `time_text`, `replay_text`, `fps_text`, `effect_timeline_pending`) | update.rs:1538-1600 | (no C++ counterpart — test hooks) |
| 33 | console state machine: `complete`, `history`, `stricmp_eq`, `strnicmp_prefix`, `parse_stod`, `parse_stoi`, `interpret_command` (!clear/!help/!ff/!advt/?/lua fallback), `track_code`, `untrack_all`, `update_tracked_results`, `HELP_TEXT` | lua_console.rs:144-725 | #33-35 |
| 34 | console engine methods: `post_update_imgui_lua_console_command`, `ilc_complete`, `ilc_history_key`, `ilc_track`, `ilc_set_track_buffer`, `ilc_set_cmd_buffer`, `ilc_untrack_all`, `ilc_update_tracked_results`, `init_lua_and_print_docs(_to)`, `set_ilc_show_console_next`, `set_ilc_capture`, state readers | lua_console.rs:716-937 | #35 |
| 35 | egui console UI (the "step (b)"): command/track inputs, completion/history wiring, timescale drag + invincible checkbox with per-frame config write-back, update/draw ms footer, tracked-results table | oh-app/src/lua_console_ui.rs:585-648, 841ff | #35 tail |
| 36 | `start()` — **ported to a different file** | hexagon_game.rs:1021-1064 | #11 |
| 37 | `go_to_menu`, `save_replay`, `invalidate_score`, `host_error` (Lippincott), `update_player_position`, `get_music_dm_sync_factor`, `get_rotation_speed`, `get_swap_cooldown`, `imgui_lua_console_has_input`, `draw_random_bool`, `perform_player_kill`, `death`, `init_flash_effect` | hexagon_game.rs:1603ff, 2462, 1981, 1825, 1901, 2191, 2549, 1143, 1159, 2396 | various |

Explicit stubs/no-ops in update.rs (markers quoted):
- update.rs:1318-1324 `update_key_icons`: "a no-op here, exactly the C++ headless
  behavior (`window == nullptr`)" + "issue-11 render objects" / "mapping gameplay.md §15".
- update.rs:1325-1329 `update_level_info`: "(issue-11 render objects)".
- update.rs:111: "Update client (hexagonClient == nullptr in this port)."
- update.rs:170-176: "the port has no window timer (windowed replay viewing is issue 10)".
- update.rs:1073-1075 (flash): "the C++ presents the batch in `draw()` (issue 11); the
  port hands the current batch to the attached renderer, if any".

Rust-only additions: test accessors (32); `draw_flash_effect` push in `update_flash`;
`post_update` running headlessly (C++ never does — see SEAMS 2).

---

## MISSING

| # | Item | C++ evidence (file:line) | Absence proof (command + result) | Risk |
|---|---|---|---|---|
| 1 | **Score send/save dropped on menu return — both `goToMenu()` default-arg call sites.** C++ declares `void goToMenu(bool mSendScores = true, bool mError = false);` and the no-arg calls therefore run `if(mSendScores && !mError) saveReplay();` (submit local best + send/save replay). The Rust port's `go_to_menu` faithfully implements that branch (hexagon_game.rs:1622-1624 `if m_send_scores && !m_error { self.save_replay(); }`), but BOTH call sites transcribed from HGUpdate.cpp pass `false`, so the score is never submitted: (a) the death/restart-prompt → menu path, (b) the joystick-`Exit` rising edge. The port's own `trigger_go_to_menu` (hexagon_game.rs:2233-2234) uses `(true, false)`, so the author knew the semantics; only the update.rs sites are wrong. | `include/SSVOpenHexagon/Core/HexagonGame.hpp:417` (`mSendScores = true` default); `src/SSVOpenHexagon/Core/HGUpdate.cpp:399` and `:576` (`goToMenu();`); `src/SSVOpenHexagon/Core/HexagonGame.cpp:1263-1265` (`saveReplay()` gate) | `grep -n "goToMenu(bool" include/SSVOpenHexagon/Core/HexagonGame.hpp` → `417: void goToMenu(bool mSendScores = true, bool mError = false);` · `grep -n "goToMenu()" src/SSVOpenHexagon/Core/HGUpdate.cpp` → `399: goToMenu();` `576: goToMenu();` · `grep -n "go_to_menu(false, false)" crates/oh-game/src/update.rs` → `442:` `707:` (no `(true, false)` at either site) | **uservisible-gameplay** |
| 2 | **PB-particle cosmetic RNG draw order deviates.** C++ `makePBParticle` draws from ONE global pcg32_fast stream in the order x → rotation → scale → alpha → vx → vy → angVel, so each particle's x is stream value #1 and its alpha is #4. The Rust port draws alpha FIRST, then x, rotation, scale, vx, vy, angVel — every particle gets C++'s #4 as its x and #1 as its alpha (and the whole 7-draw block is shifted). The in-code comment claims the C++ order ("draw order: position, rotation, scale, alpha, …") while the code does not follow it — the comment is correct about C++ and wrong about the port. (No replay-determinism impact: `cosmetic_rng` is separate from the game `rng`; swap/trail paths verified in C++ order.) | `src/SSVOpenHexagon/Core/HGUpdate.cpp:944-955` (x @944, rot @945, scale @947, alpha @951, vel @954, angVel @955); engine = SSVUtils global pcg32_fast (`extlibs/SSVUtils/include/SSVUtils/Core/Utils/Rnd.hpp:27-33`) | `sed -n '1158,1175p' crates/oh-game/src/update.rs` → `1160: c.a = self.0.cosmetic_rng.get_rnd_i(90, 145) as u8;` precedes `1164: self.0.cosmetic_rng.get_rnd_r(-64.0_f32, width + 64.0_f32)` — i.e. alpha drawn before x; comment at update.rs:1158 documents the opposite order | **uservisible-cosmetic** |

Nothing else in the C++ module lacks a Rust counterpart: every other behavior maps to
update.rs, lua_console.rs, hexagon_game.rs, oh-app/lua_console_ui.rs, or a documented
seam listed below.

---

## SEAMS

Accepted deviations, each with the marker that declares it:

| # | Seam | C++ | Rust marker (quoted) |
|---|---|---|---|
| S1 | **ImGui-console tail (UI)** — per-frame `ImGui::SFML::Update`/`ilcDeltaClock`, window render (log colors, 80-char wrap, autoscroll), focus calls, ms footer, widget plumbing. C++ itself excludes the whole tail on Android (`#ifndef SSVOH_ANDROID`, HGUpdate.cpp:39-43, 1118-1146, 1150, 1288). Ported as: headless state machine in oh-game (`interpret_command`, `complete`, `history`, `track_code`, `untrack_all`, `update_tracked_results`, engine methods — lua_console.rs:144-937) + egui UI in oh-app (oh-app/src/lua_console_ui.rs:585-648, incl. the per-frame `Config::setTimescale`/`setInvincible` write-back at :631-632 and the `TrackedResults` table at :841). | HGUpdate.cpp:1314-1643 | update.rs:485-527: "the whole ImGui tail (`HGUpdate.cpp:1314-1643`) is step (b)"; lua_console.rs:726: "the step (b) UI's `InputText("Command")` Enter will call this"; oh-app/lua_console_ui.rs:64-65: "Track field + `Untrack All`, the `Timescale` drag + `Invincible` checkbox (config write-back), and the `update ms` / `draw ms` footer." |
| S2 | **Headless `post_update` prefix.** C++ headless `postUpdate` is a no-op (`window == nullptr → return` before the gates); the port runs the debug/official force-hide and the `ilcShowConsoleNext` toggle headlessly. Unreachable state change in practice: `set_ilc_show_console_next` is only wired from the step-(b) input layer, and `ilc_capture` keeps its `true` default headless. | HGUpdate.cpp:1289-1309 | update.rs:479-484: "the port instead runs the two gates the C++ applies AFTER the window check (spec-mandated headless-testable prefix)". |
| S3 | **`hexagonClient`** — `update()` per-frame tick and `start()`'s `trySendStartedGame(validator)` (with `LoggedIn_Ready`/`getOfficial`/`unscored`/`isLevelSupportedByServer` gates). | HGUpdate.cpp:121-123, 526-532 | update.rs:111: "Update client (hexagonClient == nullptr in this port)."; hexagon_game.rs:1048: "the null client (issue 17; `HexagonClient::State::LoggedIn_Ready` gate)". |
| S4 | **Joystick backend** — `NoJoystick` test double (all queries false, `update` no-op) until issue 10 attaches a platform backend; the full rising-edge/pressed logic IS present in `update_input_update_joystick_controls`. | HGUpdate.cpp:560-587 + Core/Joystick.cpp | seams.rs:351-352: "The always-idle joystick (no platform backend attached yet)."; update.rs:682-683: "`NoJoystick` makes every query false until issue 10 feeds it". |
| S5 | **Touch positions** — finger-down positions come from the `Renderer` seam (`finger_down_positions()`, default empty) instead of `window->getFingerDownPositions()`; the half-width threshold uses `Config::get_width()` instead of the live `window->getRenderWindow().getSize().x` (identical in the C++ app unless resolution is re-applied mid-game). | HGUpdate.cpp:596-598 | update.rs:718-719: "the finger positions come from the `Renderer` seam in this port". |
| S6 | **Replay window-timer reset** — C++ resets `window->getTimerBase()` on replay start "to avoid initial speedup"; the port has no window timer at all (windowed replay viewing is issue 10). | HGUpdate.cpp:182-186 | update.rs:170-176: "the port has no window timer (windowed replay viewing is issue 10)". |
| S7 | **`executeRandomInputs` engine** — C++ uses a file-static `std::mt19937` seeded by `std::random_device`; the port draws 4 bools from its per-engine `pcg32_fast` `random_input_rng` (`draw_random_bool`, one u32 draw = top bit, matching `uniform_int{0,1}`). Test-only, non-deterministic in both. | HGUpdate.cpp:667-677 | update.rs:671-673: "The port draws from its per-engine `pcg32_fast` (documented: the streams are not comparable)". |
| S8 | **`updateKeyIcons` / `updateLevelInfo`** — C++ definitions (HexagonGame.cpp:109-158, :159-215) are window-gated SFML icon/rectangle layout (positions, scales, replay icon); headless C++ returns immediately. The Rust call sites (update.rs:197, :202) exist with the same gates, but the bodies are no-ops and **the windowed geometry update is not ported anywhere in the crate** (verified: only config keys `show_key_icons`/`key_icons_scale` and placeholder sprites in the oh-app draw bridge exist, e.g. oh-app/src/bridge.rs:185 `initKeyIcons` placeholder). Pending issue 11 (draw lane). | HexagonGame.cpp:109-215 (called from HGUpdate.cpp:213-222) | update.rs:1318-1328: "a no-op here, exactly the C++ headless behavior (`window == nullptr`)" + "the key-icon sprites are issue-11 render objects; mapping gameplay.md §15 … pins the geometry". |
| S9 | **`updateText` render properties** — C++ `setCharacterSize` (via `getZoomFactor`/`getTextScaling`) and `setOrigin` on `timeText`/`text`/`fpsText`/`messageText`/`pbText` are presentation-only; the port keeps the string content and defers sizing to the draw lane. | HGGraphics.cpp:536-557, 563-564, 567-571 | update.rs:1333-1337: "the SFML `setCharacterSize`/`setOrigin` calls are issue-11 render properties and don't affect game state. Window-gated, exactly like the C++". |
| S10 | **Sprite textures / origins** — `txStarParticle`/`txSmallCircle` texture refs and the `txSmallCircle->getSize()/2` particle origins are render-backend state; `SpriteState` carries geometry only, origins set to `(0,0)` with the concern punted to issue 11. | HGUpdate.cpp:941, 993-997, 1046-1048 | update.rs:1172: "C++: the texture origin (issue 11)"; :1222, :1268: "the texture half-size — a render-backend concern, issue 11". |
| S11 | **Flash-effect presentation** — C++ presents `flashPolygon` in `draw()`; the port pushes the current batch to the attached renderer inside `update_flash` (no-op when no renderer, i.e. headless). | HGUpdate.cpp:895-908 (update side) + HGGraphics.cpp draw | update.rs:1113-1119: "the C++ presents the batch in `draw()` (issue 11); the port hands the current batch to the attached renderer, if any". |
| S12 | **Steam rich-presence callback arm** — `steamManager->run_callbacks()` + `steamAttempt`/`steamHung` retry counter (HexagonGame.hpp:89-92) live in the `oh_steam::SteamManager` seam (issue 020); the Discord arm is ported inline on the engine (`discord_attempt`/`discord_hung`). | HexagonGame.cpp:478-493 | update.rs:1512-1513: "C++ HexagonGame.cpp:480-492 — the Steam arm." + `self.0.steam_manager.update_rich_presence_callbacks()` (impl: crates/oh-steam/src/manager.rs:546). |
| S13 | **`ilcDeltaClock` / ImGui context update** — per-frame ImGui timing and focus events (`ProcessEvent(GainedFocus)`) are owned by the egui layer in step (b); headless they are pure no-ops. | HGUpdate.cpp:1305-1306, 1314 | update.rs:517-519: "C++ `ImGui::SFML::ProcessEvent(sf::Event{sf::Event::GainedFocus})` — step (b)"; lua_console_ui.rs (egui owns frame clock). |
| S14 | **`#if 0` performance-invalidation block** — dead in C++; kept dead in Rust (behind `if false`) with a comment. Parity, not a gap. | HGUpdate.cpp:415-420 | update.rs:455-465: "C++ `#if 0` performance-invalidation block (dead in C++)". |

---

## DIFFERENT

Present but renamed or behaviorally different (incl. citation drift):

| # | Item | C++ (file:line) | Rust (file:line) | Assessment |
|---|---|---|---|---|
| D1 | `updateWalls` loop restructure: C++ interleaves `w.update` and the collision decision in ONE loop; Rust runs a complete update pass over all walls, then the collision decisions. | HGUpdate.cpp:433-460 | update.rs:529-615 (doc at 526-531) | **Verified behaviorally equivalent**: `CWall::update` reads only its own state + the passed args (CWall.cpp, `_speed.update/_curve.update/moveTowardsCenter/moveCurve`); the collision reads use the pre-loop `pPos` snapshot in both; `performPlayerKill`→`death`/`player.kill` (HexagonGame.cpp:822ff, CPlayer.cpp `kill`) never mutate `walls` or `status.radius`, so the per-iteration live `getRadius()` in C++ equals Rust's pre-loop snapshot. No observable difference. |
| D2 | Lua error **wording** on the `onInput`/`onUpdate`/`onStep`/`onLoad` paths: C++ Lippincott handler distinguishes `catch(std::runtime_error&)` ("Runtime error on …") from `catch(...)` ("Unknown runtime error on …"); the Rust `ScriptHost` collapses all errors to a `String` before `host_error`, which always prints the runtime-variant wording (and the `print_level` choice). Control flow (print → `goToMenu(false,true)` unless debug → continue with no veto) matches. | HGScripting.cpp:1155-1184; HexagonGame.hpp:312-321 | hexagon_game.rs:2462-2475; update.rs:239-252 | Minor: log-string divergence only for non-runtime errors (e.g. `std::bad_alloc` from `LUA_ERRMEM`, LuaWrapper.hpp `_call`). Risk: internal. |
| D3 | **`scoreOverride` `readVariable` error path** (and the analogous `readVariable<std::string>` in `updateText` for tracked vars / timer text): C++ calls `lua.readVariable` DIRECTLY (not via `runLuaFunctionIfExists`), so a missing/`nil` variable throws `VariableDoesntExistException` that is NOT caught by Lippincott and propagates out of `update()` — the `GameState::update` → `GameEngine::updateFromTimer` chain has no try/catch (SSVStart GameState.hpp:39-42, GameEngine.hpp:64-75), i.e. C++ terminates the process. The Rust port routes it through `host_error` (Lippincott wording + `go_to_menu(false,true)` unless debug) and SKIPS the update step, continuing the frame. The Rust comment claims "the port runs the same Lippincott behavior and skips" — inaccurate: Lippincott is never invoked on this C++ path; the C++ behavior is an uncaught-exception crash. | HGUpdate.cpp:296-299 (`lua.readVariable<float>(levelStatus.scoreOverride)`); HGGraphics.cpp:510-512 (tracked vars `readVariable<std::string>`), :542-544 (timer-text override `readVariable<std::string>`) | update.rs:316-321 (`Err(msg) => self.host_error("readVariable", &msg, false)` + skip) and update.rs:1413-1421 (tracked vars, `host_error` + `continue`), :1448-1455 (timer-text override, `host_error` + empty string) | Only reachable when a level sets `scoreOverridden`/tracks a variable that doesn't exist (broken level). C++: crash; port: clean skip + (in non-debug) menu. Documented deviation; the C++-behavior description in the comment is wrong. Risk: internal. |
| D4 | Touch threshold width source (see S5): live window pixel size vs `Config::get_width()`. | HGUpdate.cpp:598 | update.rs:722 | Internal; identical unless resolution changes mid-game. |
| D5 | `updateFlash` hands the batch to the renderer during update (see S11). | HGUpdate.cpp:895-908 | update.rs:1113-1119 | Presentation timing; headless parity. Risk: none/internal. |
| D6 | **Citation/comment drift (flagged per audit rules)**: (a) update.rs:1158 documents C++ draw order "position, rotation, scale, alpha, …" while the port's code draws alpha first (see MISSING 2). (b) update.rs:1497-1498: "the Discord half is issue 21 (absent)" — stale; the Discord arm IS implemented at update.rs:1519-1535 (and a second, correct doc block at 1499-1510 sits on the same fn, contradicting the first). (c) the three `a22_swapdeath` C++ citations in `update_walls`/`update_custom_walls` are each off by +1 line: Rust says "HGUpdate.cpp:449-452", "479-483", "497-500" (update.rs:589-590, 610-611, 633-634); the steam blocks actually span 448-451, 478-481, 496-499. | HGUpdate.cpp:448-451, 478-481, 496-499; 1497-1498 vs 1519-1535 | `grep -n "unlock_achievement" src/SSVOpenHexagon/Core/HGUpdate.cpp` → 450, 480, 498 (blocks 448-451/478-481/496-499) | Cosmetic documentation defects; the (b) line could mislead a future audit into "fixing" a working seam. |
| D7 | **File-mapping note (not a defect)**: C++ `start()` (HGUpdate.cpp:504-550) is ported to hexagon_game.rs:1021-1064, and C++ `updateText` (HGGraphics.cpp:421) / `updateRichPresenceCallbacks` (HexagonGame.cpp:477) are ported INTO update.rs — the port's file boundaries differ from the C++ module boundaries. All three verified present and faithful (client gate = S3; `resumeMusic`/`onLoad`/Discord arms present). | HGUpdate.cpp:504-550; HGGraphics.cpp:421-602; HexagonGame.cpp:477-505 | hexagon_game.rs:1021-1064; update.rs:1333-1508, 1511-1536 | Informational. |

---

## VERDICT

**FAIL — this pair does not pass 1:1.** The single most important gap: **both
`goToMenu()` call sites transcribed from HGUpdate.cpp (the death/restart-prompt → menu
path at update.rs:442 and the joystick-Exit path at update.rs:707) pass
`go_to_menu(false, false)`, dropping C++'s default `mSendScores = true` — so the
port never submits/saves the score (`saveReplay()`) when returning to the menu after
dying or on joystick exit, while C++ does.** Fix: `(true, false)` at both sites.
Secondary gap: the PB-particle cosmetic RNG draw order (alpha/x swap, update.rs:1160
vs HGUpdate.cpp:944-951) diverges from the C++ stream sequence.
