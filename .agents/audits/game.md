# 1:1 Port Audit — `HexagonGame.cpp` → `crates/oh-game/src/hexagon_game.rs`

Repo: `/home/gary/dev/SSVOpenHexagon`
C++ module (audited completely): `src/SSVOpenHexagon/Core/HexagonGame.cpp` (1676 lines)
Rust counterpart (audited completely): `crates/oh-game/src/hexagon_game.rs` (2738 lines)
Header: `include/SSVOpenHexagon/Core/HexagonGame.hpp` (563 lines)

Audit focus: death sequence (`death_*` family), scoring/PB paths, replay save/send,
rich-presence updates, status/color getters, message queue.

> Cross-file note: the Rust `hexagon_game.rs` is a **superset by C++ file** — it also
> implements `start()` (C++ `HGUpdate.cpp:505`), the periodic Steam rich-presence
> update (C++ `HGUpdate.cpp:147-149`), `update_rich_presence_callbacks` (C++
> `updateRichPresenceCallbacks`, defined in this file but the Rust copy lives in
> `update.rs`), `draw_scene_batches` + `init_flash_effect` + `format_time` (C++
> `HGGraphics.cpp`). Those are other module pairs; they are NOT flagged below. The
> audit direction is: for each C++ behavior in `HexagonGame.cpp`, find its Rust
> counterpart anywhere in the crate.

## BEHAVIOR INVENTORY (C++ `HexagonGame.cpp`)

File-local helpers:
- `getReplayScore(status)` :49-53 — `customScore != 0 ? customScore : playedAccumulatedFrametime`.
- `initializeRng()` :58-66 — fresh `random_number_generator` from a thread-local `pcg32_fast` seeded by `seed_seq_from<std::random_device>`.
- `initText(...)` :296-301 — build `sf::Text` with `characterSize / getZoomFactor()` (render).

Member functions:
- `ActiveReplay::ActiveReplay` :70-72 — wrap `replay_file`, build `replayPlayer`.
- `createWall` :74-80 — `walls.emplace_back(...centerPos...)`.
- `setMustStart` :82-85.
- `initKeyIcons` :87-107 — set key-icon sprite textures + call `updateKeyIcons` (window-gated; render).
- `updateKeyIcons` :109-157 — key-icon geometry (window-gated; render).
- `updateLevelInfo` :159-249 — level-info rectangle + 5 texts (window-gated; render).
- `nameFormat` :251-254, `diffFormat` :256-261 (`snprintf %g`), `timeFormat` :263-268 (`snprintf %.3f`).
- `imguiLuaConsoleHasInput` :270-273 — `ilcShowConsole && (Imgui::wantCaptureKeyboard() || Imgui::wantCaptureMouse())`.
- `HexagonGame(...)` ctor :283-455 — emplace cameras (window), grab star/circle textures, subscribe `game.onUpdate/onPostUpdate/onDraw/onAnyEvent`, `window->onRecreation += initKeyIcons`, `Config::keyboardBindsSanityCheck()` (:345), register keyboard binds (Escape/Exit/ForceRestart/Restart/Replay/Screenshot/LuaConsole/Pause + 2-state CCW/CW/Focus/Swap) :350-445, `Config::loadAllJoystickBinds()` (:448), `initKeyIcons()` (:452).
- `~HexagonGame` :456-459 — log.
- `refreshTrigger` :461-464 — forward to `game.refreshTrigger`.
- `setLastReplay` :467-475.
- `updateRichPresenceCallbacks` :477-508 — Steam `run_callbacks` retry (`steamAttempt`>20 → `steamHung`) + Discord `run_callbacks` retry (`discordAttempt`>20 → `discordHung`).
- `shouldPlaySounds` :510-513, `shouldPlayMusic` :515-518.
- `playSoundOverride` :520-526, `playSoundAbort` :528-534, `playPackSoundOverride` :536-543.
- `saveReplay` :545-556 — if `shouldSaveScore && !hasDied`: save score, `death_createReplayFile`, log "Attempting to send and save replay...", `death_sendAndSaveReplay`.
- `newGame` :558-821 — saveReplay if !firstPlay; set pack/level id; firstPlay logic; `setLevelData`; difficultyMult; reset `status`; new rng + last-seed bookkeeping OR activeReplay build/reset + `replayPackName`/`replayLevelName` + rng from replay seed; audio cleanup; clear timelines/particles/manager; `initFlashEffect(255,255,255)`; camera reset (window); Lua onPreUnload/reset/initLua/runLuaFile/onUnload|select + onInit; `restartId`/`restartFirstTime`; `setSides`; pulse/beatPulse initial delay; `timeUntilRichPresenceUpdate = -1`; **window-gated build of `status.restartInput`/`status.replayInput` from `Config::getKeyboardBindNames` + `Config::getJoystickBindName`** :758-821.
- `death_shakeCamera` :822-839 (window-gated; `cameraShake = 45 * getCameraShakeMultiplier`).
- `death_flashEffect` :841-845 — `initFlashEffect(255,255,255)`; `flashEffect = 255`.
- `death_updateRichPresence` :847-888 (window-gated; inReplay-gated; Discord `set_rich_presence_in_game(name [xdiff], "Survived ts s", true)`).
- `death_saveScoreIfNeeded` :889-917 (window→NoWindow; !shouldSaveScore→ShouldNotSave; `score = getTimeSeconds`; PB iff `score > getLocalScore(validatorWithoutPackId)`; `setLocalScore`).
- `death_saveScoreIfNeededAndShowPBEffects` :919-941 — NoWindow→return; ShouldNotSave/NotPersonalBest→`playSoundAbort("gameOver.ogg")`; else `pbText="NEW PERSONAL BEST!"`, `mustSpawnPBParticles=true`, `playSoundAbort("personalBest.ogg")`.
- `death(mForce)` :943-989 — hasDied→return; `deathInputIgnore=10`; `playSoundAbort(deathSound)`; onPreDeath; (!force && (invincible||tutorial))→return; onDeath; shake; richPresence; stopMusic; flash; `hasDied=true`; if !inReplay: `death_createReplayFile` + `onDeathReplayCreated` + log "Attempting to send and save replay..." (:980) + `death_sendAndSaveReplay`; `death_saveScoreIfNeededAndShowPBEffects`; (window && autoRestart)→`mustStateChange=MustRestart`.
- `death_createReplayFile` :992-1010 — `rfName = anyLocalProfileActive ? currentProfileName : "no_profile"`; build `replay_file`.
- `death_sendAndSaveReplay` :1012-1043 — `compress_replay_file`; (fail→log+return); `death_sendReplay(validator, crf)` (fail→log "Failure sending replay"); `death_saveReplay(filename+".z", crf)` (fail→log "Failure saving replay").
- `death_sendReplay` :1045-1065 — gate `hexagonClient!=null && state==LoggedIn_Ready && getOfficial`; `trySendCompressedReplay`; on success `steamManager->unlock_achievement("a24_onlinescore")` (:1063).
- `death_saveReplay` :1067-1097 — `dirPath="Replays/"+levelId+"/"+diffFormat+x/"`; sanitize `: ? * ? " < > |` → `_`; `create_directories`; `crf.serialize_to_file(p)`; (fail→log "Failed to save..."; success→log "Successfully saved..." :1093).
- `executeGameUntilDeath` :1099-1125 — loop `update(TIME_STEP,timescale)+postUpdate` until `hasDied` or time exceeded; return `GameExecutionResult`.
- `runReplayUntilDeathAndGetScore` :1127-1141 — `setLastReplay`; `newGame(..., executeLastReplay=true)`; `executeGameUntilDeath`.
- `incrementDifficulty` :1143-1159.
- `sideChange` :1161-1175.
- `shouldSaveScore` :1177-1229 — reject: no profile / !isEligibleForScore / scoreInvalid / tutorialMode / unscored / inReplay.
- `goToMenu(mSendScores,mError)` :1239-1281 — (no window→log+return); `stopSounds`; clear ilcLuaTracked*; (!error)→`beep.ogg`; (sendScores && !error)→`saveReplay`; clear calledDeprecatedFunctions; (!error)→onUnload; `fnGoToMenu(mError)`.
- `raiseWarning` :1283-1301 — once-per-function; console print + `ilcCmdLog`.
- `addMessage` :1303-1326 — (getShowMessages gate); uppercasify; timeline append_do(beep+set message) / append_wait_for_sixths(duration) / append_do(clear).
- `clearMessages` :1328-1331.
- `setLevelData` :1333-1346 — levelData ptr; fresh `levelStatus`; `styleData`+`computeColors`; `musicData`+firstPlay.
- Pack getters: `getPackId` :1348, `getPackData` :1353, `getPackDisambiguator` :1359, `getPackAuthor` :1364, `getPackName` :1369, `getPackVersion` :1374.
- `playLevelMusic` :1379-1391 — random/first segment via `getRndI` (cosmetic rng); `beatPulseDelay += seg.offset`.
- `playLevelMusicAtTime` :1393-1399, `stopLevelMusic` :1401-1407.
- `invalidateScore` :1409-1421 — set `scoreInvalid`/`invalidReason` once; log.
- Color getters: `getColorMain` :1423, `getColorPlayer` :1433, `getColorPlayerAdjustedForSwap` :1443, `getColorPlayerTrail` :1458, `getColorText` :1465, `getColorCap` :1475, `getColorWall` :1485.
- `getMusicDMSyncFactor` :1495 (`pow(diff,0.12)`), `getOptionalMusicDMSyncFactor` :1500.
- `refreshMusicPitch` :1505-1513 (audio-null gate).
- `setSides` :1515-1525 — `beep`; clamp ≥3.
- `getGame` :1527, `getRadius` :1532, `getColor(mIdx)` :1537 (`styleData.getColor(mIdx)`), `getSpeedMultDM` :1542, `getDelayMultDM` :1554, `getRotationSpeed` :1566, `getSides` :1571, `getWallSkewLeft/Right` :1576/1581, `getWallAngleLeft/Right` :1586/1591, `getStatus` :1596/1601, `getLevelStatus` :1606, `getAssets` :1611, `getInputFocused` :1616, `getPlayerSpeedMult` :1621, `getInputSwap` :1626, `getInputMovement` :1631, `inReplay` :1636, `mustReplayInput` :1641, `mustShowReplayUI` :1646, `getSwapCooldown` :1651, `performPlayerSwap` :1656, `performPlayerKill` :1667.

Globals/statics read/written: `Config::*` (many), `assets` (HGAssets), `audio`, `window`,
`steamManager`, `discordManager`, `hexagonClient`, `Imgui::wantCaptureKeyboard/Mouse`,
`ssvu::getRndI` (global cosmetic rng in `playLevelMusic`), `ssvs::zeroVec2f`, `centerPos`.

---

## MISSING (highest risk first)

| # | item | C++ evidence (file:line) | absence proof (command + result) | risk |
|---|------|--------------------------|----------------------------------|------|
| M1 | `newGame` does **not build** `status.restartInput` / `status.replayInput` (the "PRESS X TO RESTART\n / PRESS Y TO REPLAY\n / NO … BUTTON SET\n" hints, incl. joystick "OR JOYSTICK …"). Rust `new_game` has only a comment, no code. `update_text` (update.rs:1395-1396) then reads the always-empty fields on death, so a windowed player gets no restart/replay hint after dying. | `HexagonGame.cpp:758-821` (`if(window!=nullptr)` block; `getKeyboardBindNames(Tid::Restart/Replay)` :763-764; `getJoystickBindName(Jid::Restart/Replay)` :781/:801; "PRESS … TO RESTART" :791-809; "PRESS … TO REPLAY" :813-820). Rust comment only: `hexagon_game.rs:950-953` ("windowed-only (and Config bind-name plumbing, issues 10/15); headless leaves … empty"). | `grep -rn 'TO RESTART\|TO REPLAY\|PRESS \|NO RESTART\|NO REPLAY\|OR JOYSTICK' crates/oh-game/src/` → **no matches**. (Bind-name plumbing exists: `oh-config/src/config.rs:2106 get_keyboard_bind_names`, `:1970 get_joystick_bind_name`; they are simply never called from `new_game`.) | uservisible-cosmetic |
| M2 | `death_saveReplay` **file-write half has no production implementation** — C++ writes the compressed replay to `Replays/<levelId>/<diff>x/<name>.z` (`create_directories` + `crf.serialize_to_file`). Rust delegates to `ReplayStore::save`, but the **only** impl is `NullReplayStore` (returns `true`, writes nothing) and the composition wires it. No Rust code actually persists a replay. | `HexagonGame.cpp:1067-1097` (`create_directories` :1086; `serialize_to_file` :1089; success log :1093). Rust seam: `seams.rs:120-129` (trait) + `:132-138` (`NullReplayStore` no-op); wiring: `oh-app/src/compose.rs:526` (`Arc::new(NullReplayStore)`). | `grep -rn 'impl ReplayStore for' crates/` → **only** `seams.rs:134 impl ReplayStore for NullReplayStore` (no-op). `grep -rn 'ReplayStore\|NullReplayStore' crates/oh-app/src/` → `compose.rs:526` wires `NullReplayStore`. | data-format |
| M3 | `death_sendReplay` **server-send half** (`trySendCompressedReplay`) + the on-success **Steam `a24_onlinescore` unlock** (gated by client state + `getOfficial`). Rust `death_send_and_save_replay` performs no send and unconditionally logs "Failure sending replay"; `hexagon_client` is absent (issue 17). | `HexagonGame.cpp:1045-1065` (gate :1047-1051; `trySendCompressedReplay` :1058; `unlock_achievement("a24_onlinescore")` :1063). Rust: `hexagon_game.rs:1288-1313` (computes `_level_validator` then logs "Failure sending replay" :1301, no send). | `grep -rn 'a24_onlinescore' crates/oh-game crates/oh-app crates/oh-steam crates/oh-discord` → **only** a doc comment at `seams.rs:454` (no code). `grep -rn 'trySendCompressedReplay\|try_send_compressed_replay' crates/` → none. | internal |
| M4 | Ctor side effect `Config::keyboardBindsSanityCheck()` (fixes/repairs invalid keyboard binds) is **never called** in the Rust engine or composition. | `HexagonGame.cpp:345`. Rust `new()` (`hexagon_game.rs:503-643`) does not call it. | `grep -rn 'keyboard_binds_sanity_check' crates/oh-game crates/oh-app` → **no matches** (method defined at `oh-config/src/config.rs:1947` + used only in `oh-config/tests`). | internal |
| M5 | Ctor side effect `Config::loadAllJoystickBinds()` (populates joystick binds) is **never called** in the Rust engine or composition. | `HexagonGame.cpp:448`. Rust `new()` does not call it. | `grep -rn 'load_all_joystick_binds' crates/oh-game crates/oh-app` → **no matches** (defined at `oh-config/src/config.rs:2015` + used only in tests). | internal |
| M6 | `death(mForce)` is missing its `ssvu::lo("Replay") << "Attempting to send and save replay...\n"` log (present in C++ `death` but absent in Rust `death`; Rust only logs it in `save_replay`). | `HexagonGame.cpp:980`. Rust `death()` `hexagon_game.rs:1159-1199` (no such log between the `on_death_replay_created` hook and `death_send_and_save_replay`). | `grep -rn 'Attempting to send and save replay' crates/oh-game/src/` → **single hit** `hexagon_game.rs:1654` (inside `save_replay` only). | internal |
| M7 | `death_saveReplay` success log `ssvu::lo("Replay") << "Successfully saved new compressed replay file …"` is absent in Rust (Rust logs only the failure path). | `HexagonGame.cpp:1093-1094`. Rust `death_send_and_save_replay` `hexagon_game.rs:1304-1313` (only "Failure saving replay" :1312). | `grep -rn 'Successfully saved' crates/oh-game/src/` → **no matches**. | internal |
| M8 | `refreshTrigger(trigger, bindID)` (dynamic keybind refresh) has no Rust counterpart — the input-trigger system was replaced by `trigger_*`/`set_input_impl_*` methods (issue 010b). | `HexagonGame.cpp:461-464`. | `grep -rniE 'refresh_trigger' crates/oh-game/src/` → **no matches**. | internal |
| M9 | `getGame()` (returns `ssvs::GameState&`, consumed by `main.cpp:448/461/479/502` `window->setGameState`) has no Rust method; the game-state handoff is performed by the composition's window adapter instead. | `HexagonGame.cpp:1527-1529`. | `grep -rniE 'fn get_game\b' crates/oh-game/src/` → **no matches** (documented at `hexagon_game.rs:471,1634` as the composition adapter's job). | internal |
| M10 | `getColor(int mIdx)` (`styleData.getColor(mIdx)`; only caller is render `drawText`, `HGGraphics.cpp:732`) has no `HexagonGame` method. The capability exists as `StyleData::get_color(idx)` (`oh-data/src/style_data.rs:350`) but is not surfaced on the engine. | `HexagonGame.cpp:1537-1539`. | `grep -rniE 'fn get_color\b' crates/oh-game/src/` → **no match** on `HexagonGame` (only `player.rs:145`, a different `CPlayer::get_color`). | none |
| M11 | Unused pack accessors `getPackDisambiguator` / `getPackAuthor` / `getPackVersion` have no Rust counterpart (no C++ call sites either — API-surface only). | `HexagonGame.cpp:1359-1361 / 1364-1366 / 1374-1376`. | `grep -rniE 'get_pack_disambiguator\|get_pack_author\|get_pack_version' crates/oh-game/src/` → **no matches**. (C++ callers: none beyond decl/def.) | none |
| M12 | `getAssets()` (`HGAssets&` accessor) has no Rust counterpart (no C++ call sites — API-surface only). | `HexagonGame.cpp:1611-1613`. | `grep -rniE 'fn get_assets' crates/oh-game/src/` → **no matches**. | none |

## SEAMS (accepted / documented deviations)

| seam | C++ behavior (file:line) | Rust marker | note |
|------|--------------------------|-------------|------|
| Key-icon + level-info render objects | `initKeyIcons` :87-107; `updateKeyIcons` :109-157; `updateLevelInfo` :159-249; the `keyIcon*` / `levelInfoText*` / `levelInfoRectangle` sprites+texts | `update_key_icons`/`update_level_info` are no-ops: `update.rs:1315-1326` — "windowed sprite-geometry update … a no-op here, exactly the C++ headless behavior (`window == nullptr`)" / "(issue-11 render objects)". `initKeyIcons` texture-set half has no Rust counterpart (sprites don't exist). | Render-backend concern, **issue 11**. |
| Server-send half of replay | `death_sendReplay` :1045-1065 (client send) | `hexagon_game.rs:1288-1313` — "the server-send half always fails headless: `hexagonClient == nullptr`"; "issue 17". | Network/feature-gated. See also M3 (the `a24_onlinescore` unlock + send are the residual gap). |
| Replay file-write half | `death_saveReplay` :1067-1097 | `seams.rs:120-129` — "C++ `death_saveReplay`'s filesystem half … via `create_directories` and `crf.serialize_to_file`". | Documented seam; **but** see M2 — only `NullReplayStore` exists, so no production impl yet. |
| Steam manager | `steamManager->run_callbacks` / `unlock_achievement` / rich presence (:480-492, :1063) | `SteamSink` trait `seams.rs:445-460` ("issue 020"); counter lives in `oh_steam::SteamManager`. `update_rich_presence_callbacks` delegates Steam half: `update.rs:1511-1513`. | Feature-gated, **issue 020**. |
| Discord manager | `discordManager->run_callbacks` / `set_rich_presence_*` (:495-506) | `oh_discord::Discord` trait (`crates/oh-discord/src/manager.rs:39-52`) + `DiscordManager`; engine retry state `discord_hung`/`discord_attempt` (`hexagon_game.rs:363-366`), retry logic `update.rs:1519-1531`. | Feature-gated, **issue 21** (port present). |
| Input bind registration | ctor `game.addInput`/`add2StateInput` (Escape/Exit/ForceRestart/Restart/Replay/Screenshot/LuaConsole/Pause + CCW/CW/Focus/Swap) :350-445 | `trigger_force_restart/restart/replay/screenshot/go_to_menu/lua_console/pause` + `set_input_impl_ccw/cw/focused/swap` (`hexagon_game.rs:2153-2272`), invoked by the windowed input manager — "issue 010b". | Structural (composition-driven input). |
| Event subscription / game loop | ctor `game.onUpdate/onPostUpdate/onDraw/onAnyEvent += …` :315-320; `window->onRecreation += initKeyIcons` :324-326 | Composition drives `update`/`post_update`/`draw`; egui handles events — "issue 014b". `onRecreation`→key-icons is the issue-11 render seam above. | Structural (composition-driven loop). |
| `getGame()` / window game-state | `getGame()` :1527 (`main.cpp:448/461/479/502` `setGameState`) | `hexagon_game.rs:471,1634` — "performed by the composition's game state adapter". | Structural. |
| `initializeRng` seed source | `seed_seq_from<std::random_device>` :58-66 | `entropy_seed()` (process entropy) `hexagon_game.rs:369-376, 2568-2586` — "both are non-deterministic by design (this only seeds *live* games; replays always re-seed from the recorded `lastSeed`)". | RNG seed source; does **not** affect replay determinism (replay rng re-seeds from `lastSeed`). |
| Headless `window` gate | every `window != nullptr` branch | `window: bool` ctor arg models `mGameWindow != nullptr` (documented module-wide, e.g. `hexagon_game.rs:1236-1241`). | Overarching headless seam. |
| `getRndI` (cosmetic) in `playLevelMusic` | `ssvu::getRndI` :1385 | `cosmetic_rng.get_rnd_i` `hexagon_game.rs:1770-1777` — "the port draws from its `CosmeticRng` seam … seed streams are not comparable between C++ and the port". | Cosmetic RNG; not replay-relevant. |

## DIFFERENT (present but renamed / relocated / behaviorally different)

- `text` (C++ `sf::Text`, :265 in hpp) → `status_text: String` (Rust `status.rs`, set in `update.rs:1489`). Renamed; string-content only (fonts/layout deferred to issue 11).
- `getPackData()` (C++ live `const PackData&`, :1353) → `SharedState.pack_data: PackData` **clone/snapshot** taken in `set_level_data_inner` (`hexagon_game.rs:1001-1006`; `lua_state.rs:226-232`). Behaviorally equivalent because the registry is immutable after load (documented: "C++ reads them live through the pointer" vs "the port owns a clone").
- `getSpeedMultDM` / `getDelayMultDM` (:1542/:1554) → `SharedState::get_speed_mult_dm` / `get_delay_mult_dm` (`lua_state.rs:284/298`). Relocated (operate on shared state).
- `raiseWarning` (:1283) → `SharedState::raise_warning` (`lua_state.rs:314-331`). Relocated; C++ `std::cout` → `backend.log("lua", …)`; `ilcCmdLog` → `ilc_cmd_log`.
- `updateKeyIcons` / `updateLevelInfo` / `updateRichPresenceCallbacks` / `update_text` relocated to `update.rs` (C++ defines them in `HexagonGame.cpp`; Rust places the frame-driven ones with the update loop). Relocated, not missing (see SEAMS for the two no-ops).
- `get_color_cap` (`hexagon_game.rs:2065-2078`): C++ `getColorCap` returns `styleData.getCapColorResult()` (a `const sf::Color&`; asserts in debug on a bad index); Rust returns `get_cap_color_result().unwrap_or(Color::BLACK)` (defensive fallback). Minor defensive divergence.
- `death_sendAndSaveReplay` dirPath source: C++ uses `levelId`/`difficultyMult` (game state, :1072-1073); Rust uses `rf.level_id`/`rf.difficulty_mult` (replay file, `hexagon_game.rs:1305-1308`). Equivalent in practice (the replay file is built from the current state the same call).
- `getPackId()` C++ returns `levelData->packId` (live ref); Rust `get_pack_id` returns `self.level_data().pack_id.clone()`. Equivalent (owned clone).
- `executeGameUntilDeath`/`runReplayUntilDeathAndGetScore` return types map `double`/`float` → `f64`/`f32` identically (`GameExecutionResult`). `getReplayScore` C++ `double` → Rust `f64`. Types align (`status.rs:137/211/217/223/229/236/242/248`).
- `newGame` double-assert: C++ has `SSVOH_ASSERT(assets.isValidLevelId(mId))` twice (`HexagonGame.cpp:568, :582`); Rust has one `debug_assert!` (`hexagon_game.rs:706`) + `get_level_data(m_id).expect("SSVOH_ASSERT(assets.isValidLevelId(mId))")` (`hexagon_game.rs:730`). Same effect (panic on invalid id).
- Superset-by-C++-file: Rust `hexagon_game.rs` also carries `start()` (C++ `HGUpdate.cpp:505`), `draw_scene_batches` + `init_flash_effect` + `format_time` (C++ `HGGraphics.cpp`). These belong to other module pairs; behavior is present, not missing.

## VERDICT

FAILS 1:1 — the death/scoring/PB/replay/rich-presence/message core is a faithful, type-aligned port, but no Rust code performs the C++ **replay file-write** (only a no-op `NullReplayStore` is wired, `compose.rs:526`), so replays are silently never persisted (M2, data-format) — the single most important gap; the windowed **restart/replay input-hint strings** are also never built (M1, uservisible-cosmetic).
