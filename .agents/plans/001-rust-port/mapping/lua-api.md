# Lua API Mapping (SSVOpenHexagon → Rust port)

Mechanical transcription of the complete Lua API surface exposed by the C++ engine,
for use in the Rust port. All C++ source references are paths relative to the repo
root, with 1-indexed line numbers as of the current tree.

Sources transcribed:

- `src/SSVOpenHexagon/Core/HGScripting.cpp` (level-game functions, 1185 lines, fully read)
- `src/SSVOpenHexagon/Core/LuaScripting.cpp` (shared game+menu functions, 1689 lines, fully read)
- `include/SSVOpenHexagon/Utils/LuaWrapper.hpp` + `src/SSVOpenHexagon/Utils/LuaWrapper.cpp` (conversion layer, fully read)
- `include/SSVOpenHexagon/Utils/LuaMetadata.hpp`, `LuaMetadataProxy.hpp` (+ `src/.../LuaMetadata.cpp`)
- `src/SSVOpenHexagon/Utils/Utils.cpp` (script loading / dependency resolution)
- `src/SSVOpenHexagon/Core/HexagonGame.cpp`, `HGUpdate.cpp`, `HGGraphics.cpp`, `MenuGame.cpp` (callback call sites)
- Lua corpus: `find _RELEASE/Packs -name '*.lua'` → 42 files (6 base Scripts, 36 level scripts)

Registration mechanism: every function is registered via `addLuaFn(lua, name, lambda)`,
which calls `lua.writeVariable(name, lambda)` (a `Lua::LuaContext` from the vendored
LuaWrapper) and records a metadata entry (`Utils::LuaMetadataProxy`) for docs.
Registration order: `LuaScripting::init(...)` (random, io.open, math.random redefinition,
malicious-function removal, utils, custom walls, level control, style control,
exec-script, shaders, config) → then `HexagonGame::initLua()` calls the
`initLua_Utils/AudioControl/MainTimeline/EventTimeline/CustomTimelines/LevelControl/
StyleControl/WallCreation/Steam/Deprecated` groups.

Type key: `int` = C++ `int` (pushed as 64-bit `lua_Integer`); `float`/`double` =
floating (pushed as `lua_Number`, 64-bit double); `bool`; `string` = `std::string`;
`uint` = `unsigned int`; `ull` = `unsigned long long` (64-bit); `handle` = `int`
(`CCustomWallHandle` and `CustomTimelineHandle` are both `using ... = int;`);
`FT` = `ssvu::FT` = `float` (frametime, 1.0 = 1/60 s); `size_t` = `std::size_t`.

---

## 1. Complete function table

### 1.1 `u_` — utility functions (35)

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `u_rndReal` | `()` | `float` | `rng.get_real<float>(0, 1)` — uniform real in `[0, 1)`. See §4. | `LuaScripting.cpp:89` |
| `u_rndIntUpper` | `(int upper)` | `float` (declared `-> float`; value is an `int` in `[1; upper]`) | `rng.get_int<int>(1, upper)`. | `LuaScripting.cpp:92` |
| `u_rndInt` | `(int lower, int upper)` | `float` (declared `-> float`; value is an `int` in `[lower; upper]`) | `rng.get_int<int>(lower, upper)`. | `LuaScripting.cpp:96` |
| `u_rndSwitch` | `(int mode, int lower, int upper)` | `float` | `mode==0` → `u_rndReal()`; `mode==1` → `u_rndIntUpper(upper)`; `mode==2` → `u_rndInt(lower, upper)`; any other mode → `SSVOH_ASSERT(false)` then returns `0` (consumes no RNG). Internal replacement for `math.random`. | `LuaScripting.cpp:101` |
| `u_getAttemptRandomSeed` | `()` | `ull` | `rng.seed()` — the current `pcg32_fast` seed (64-bit). Pushed as a 64-bit integer. | `LuaScripting.cpp:132` |
| `u_inMenu` | `()` | `bool` | Captured `inMenu` flag: `false` for level scripts (`HexagonGame::initLua`), `true` for menu scripts (`MenuGame::initLua`). | `LuaScripting.cpp:224` |
| `u_isHeadless` | `()` | `bool` | Captured `headless` flag (`window == nullptr`). | `LuaScripting.cpp:230` |
| `u_getVersionMajor` | `()` | `int` | `GAME_VERSION.major`. | `LuaScripting.cpp:237` |
| `u_getVersionMinor` | `()` | `int` | `GAME_VERSION.minor`. | `LuaScripting.cpp:241` |
| `u_getVersionMicro` | `()` | `int` | `GAME_VERSION.micro`. | `LuaScripting.cpp:245` |
| `u_getVersionString` | `()` | `string` | `GAME_VERSION_STR`. | `LuaScripting.cpp:249` |
| `u_execScript` | `(const string& scriptFilename)` | `void` | Runs `<context>/Scripts/<scriptFilename>`; see §6. In the game context the callback is `HexagonGame::runLuaFile` (cached path when headless or `Config::getUseLuaFileCache()`). | `LuaScripting.cpp:1200` |
| `u_execDependencyScript` | `(const string& packDisambiguator, const string& packName, const string& packAuthor, const string& scriptFilename)` | `void` | Validates `(disambiguator, name, author)` against the current pack's `dependencies` (+ min version), resolves the dependee pack folder, runs `<dependeeFolder>/Scripts/<scriptFilename>`, and temporarily swaps the exec-script context to the dependee folder. See §6. | `LuaScripting.cpp:1210` |
| `u_getWidth` | `()` | `uint` | `Config::getWidth()` (window width in pixels). | `LuaScripting.cpp:1570` |
| `u_getHeight` | `()` | `uint` | `Config::getHeight()`. | `LuaScripting.cpp:1574` |
| `u_impl_addTrackedResult` | `(const string& result)` | `void` | **Internal, not an addLuaFn**: registered via raw `lua.writeVariable` (no metadata/docs entry). Appends `result` to `ilcLuaTrackedResults` (in-level console value tracking). | `HGScripting.cpp:49-52` |
| `u_setFlashEffect` | `(float value)` | `void` | `status.flashEffect = value`. | `HGScripting.cpp:62` |
| `u_setFlashColor` | `(int r, int g, int b)` | `void` | `initFlashEffect(r, g, b)`. | `HGScripting.cpp:67` |
| `u_log` | `(const string& message)` | `void` | Prints to console log; also appends `"[lua]: <message>\n"` to `ilcCmdLog`. In headless mode (`window == nullptr`) it is a **no-op**. Menu overrides this with a different implementation (`MenuGame.cpp:1078`). | `HGScripting.cpp:75` |
| `u_isKeyPressed` | `(int keyCode)` | `bool` | **DEPRECATED** (emits `raiseWarning`). `window != nullptr && window->getInputState()[ssvs::KKey(keyCode)]`. | `HGScripting.cpp:89` |
| `u_haltTime` | `(double duration)` | `void` | `status.pauseTime(ssvu::getFTToSeconds(duration))` — i.e. pauses the timer for `duration / 60` **seconds** (duration is FT frametime, not seconds — the docstring saying "seconds" is wrong). | `HGScripting.cpp:107` |
| `u_clearWalls` | `()` | `void` | `walls.clear()`. | `HGScripting.cpp:113` |
| `u_getPlayerAngle` | `()` | `float` | `player.getPlayerAngle()` (radians). Menu stub returns `0`. | `HGScripting.cpp:117` |
| `u_setPlayerAngle` | `(float angle)` | `void` | `player.setPlayerAngle(angle)`. | `HGScripting.cpp:121` |
| `u_isMouseButtonPressed` | `(int buttonCode)` | `bool` | **DEPRECATED** (emits `raiseWarning`). `window != nullptr && window->getInputState()[ssvs::MBtn(buttonCode)]`. | `HGScripting.cpp:126` |
| `u_isFastSpinning` | `()` | `bool` | `status.fastSpin > 0`. | `HGScripting.cpp:145` |
| `u_forceIncrement` | `()` | `void` | `incrementDifficulty()` (also fires `onIncrement`). | `HGScripting.cpp:151` |
| `u_getDifficultyMult` | `()` | `float` | `difficultyMult` (level `HexagonGame` member, `float`). Menu stub returns `1`. | `HGScripting.cpp:157` |
| `u_getSpeedMultDM` | `()` | `float` | `getSpeedMultDM()`. Menu stub returns `1`. | `HGScripting.cpp:161` |
| `u_getDelayMultDM` | `()` | `float` | `getDelayMultDM()`. Menu stub returns `1`. | `HGScripting.cpp:167` |
| `u_swapPlayer` | `(bool playSound)` | `void` | `performPlayerSwap(playSound)` — swaps player 180°, fires the `onCursorSwap` callback, plays `levelStatus.swapSound` if `playSound`. | `HGScripting.cpp:173` |
| `u_kill` | `()` | `void` | **DEPRECATED** alias of `t_kill`: `raiseWarning` + `timeline.append_do(death(true))`. | `HGScripting.cpp:862` |
| `u_eventKill` | `()` | `void` | **DEPRECATED** alias of `e_kill`: `raiseWarning` + `eventTimeline.append_do(death(true))`. | `HGScripting.cpp:876` |
| `u_playSound` | `(const string& soundId)` | `void` | **DEPRECATED** alias of `a_playSound`: `raiseWarning` + `playSoundOverride(id)`. | `HGScripting.cpp:890` |
| `u_playPackSound` | `(const string& fileName)` | `void` | **DEPRECATED** alias of `a_playPackSound`: `raiseWarning` + `playPackSoundOverride(getPackId(), fileName)`. | `HGScripting.cpp:906` |

### 1.2 `a_` — audio functions (11)

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `a_setMusic` | `(const string& musicId)` | `void` | `musicData = assets.getMusicData(levelData->packId, id)`; `musicData.firstPlay = true`; `stopLevelMusic(); playLevelMusic()`. | `HGScripting.cpp:183` |
| `a_setMusicSegment` | `(const string& musicId, int segment)` | `void` | As above, then `playLevelMusicAtTime(musicData.getSegment(segment).time)`. | `HGScripting.cpp:196` |
| `a_setMusicSeconds` | `(const string& musicId, float time)` | `void` | As above, then `playLevelMusicAtTime(time)`. | `HGScripting.cpp:210` |
| `a_playSound` | `(const string& soundId)` | `void` | `playSoundOverride(id)` (id from `assets.json` `soundBuffers`). | `HGScripting.cpp:223` |
| `a_playPackSound` | `(const string& fileName)` | `void` | `playPackSoundOverride(getPackId(), fileName)` — plays `<pack>/Sounds/<fileName>`. | `HGScripting.cpp:230` |
| `a_syncMusicToDM` | `(bool value)` | `void` | `levelStatus.syncMusicToDM = value; refreshMusicPitch()`. | `HGScripting.cpp:238` |
| `a_setMusicPitch` | `(float pitch)` | `void` | `levelStatus.musicPitch = pitch; refreshMusicPitch()`. | `HGScripting.cpp:250` |
| `a_overrideBeepSound` | `(const string& fileName)` | `void` | `levelStatus.beepSound = getPackId() + "_" + fileName` (level-scoped override). | `HGScripting.cpp:263` |
| `a_overrideIncrementSound` | `(const string& fileName)` | `void` | `levelStatus.levelUpSound = getPackId() + "_" + fileName`. | `HGScripting.cpp:272` |
| `a_overrideSwapSound` | `(const string& fileName)` | `void` | `levelStatus.swapSound = getPackId() + "_" + fileName`. | `HGScripting.cpp:282` |
| `a_overrideDeathSound` | `(const string& fileName)` | `void` | `levelStatus.deathSound = getPackId() + "_" + fileName`. | `HGScripting.cpp:291` |

### 1.3 `t_` — main timeline functions (6)

All `t_*` schedule into `HexagonGame::timeline` (a `Utils::timeline2`); they do not execute immediately.

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `t_eval` | `(const string& code)` | `void` | `timeline.append_do(runLuaCode(lua, code))` — deferred inline Lua evaluation. | `HGScripting.cpp:315` |
| `t_clear` | `()` | `void` | `timeline.clear()`. | `HGScripting.cpp:323` |
| `t_kill` | `()` | `void` | `timeline.append_do(death(true))`. | `HGScripting.cpp:326` |
| `t_wait` | `(double duration)` | `void` | `timeline.append_wait_for_sixths(duration)` — waits `duration`/60 s (frames at 60 FPS). | `HGScripting.cpp:330` |
| `t_waitS` | `(double duration)` | `void` | `timeline.append_wait_for_seconds(duration)`. | `HGScripting.cpp:338` |
| `t_waitUntilS` | `(double duration)` | `void` | `waitUntilSImpl`: `timeline.append_wait_until_fn(levelStartTP + duration*1000 ms)` (int-millisecond truncation: `static_cast<int>(mDuration * 1000.0)`). | `HGScripting.cpp:344` (impl `HGScripting.cpp:301-311`) |

### 1.4 `e_` — event timeline functions (11 + 5 deprecated)

All schedule into `eventTimeline` (the "1.92 events" timeline).

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `e_eval` | `(const string& code)` | `void` | `eventTimeline.append_do(runLuaCode(lua, code))`. | `HGScripting.cpp:355` |
| `e_kill` | `()` | `void` | `eventTimeline.append_do(death(true))`. | `HGScripting.cpp:365` |
| `e_stopTime` | `(double duration)` | `void` | `eventTimeline.append_do(status.pauseTime(getFTToSeconds(duration)))` — pauses `duration/60` s. | `HGScripting.cpp:369` |
| `e_stopTimeS` | `(double duration)` | `void` | `eventTimeline.append_do(status.pauseTime(duration))` — pauses `duration` seconds. | `HGScripting.cpp:380` |
| `e_wait` | `(double duration)` | `void` | `eventTimeline.append_wait_for_sixths(duration)`. | `HGScripting.cpp:389` |
| `e_waitS` | `(double duration)` | `void` | `eventTimeline.append_wait_for_seconds(duration)`. | `HGScripting.cpp:397` |
| `e_waitUntilS` | `(double duration)` | `void` | `eventTimeline.append_wait_until_fn(levelStartTP + duration*1000 ms)`. | `HGScripting.cpp:403` |
| `e_messageAdd` | `(const string& message, double duration)` | `void` | `eventTimeline.append_do`: **only if `firstPlay`** → `addMessage(msg, duration, soundToggle=true)`. | `HGScripting.cpp:411` |
| `e_messageAddImportant` | `(const string& message, double duration)` | `void` | `eventTimeline.append_do(addMessage(msg, duration, true))` — every run. | `HGScripting.cpp:430` |
| `e_messageAddImportantSilent` | `(const string& message, double duration)` | `void` | `eventTimeline.append_do(addMessage(msg, duration, /*soundToggle*/ false))` — every run, no sound. | `HGScripting.cpp:443` |
| `e_clearMessages` | `()` | `void` | `clearMessages()`. | `HGScripting.cpp:456` |
| `e_eventStopTime` | `(double duration)` | `void` | **DEPRECATED** (= `e_stopTime`): `raiseWarning` + same as `e_stopTime`. | `HGScripting.cpp:922` |
| `e_eventStopTimeS` | `(double duration)` | `void` | **DEPRECATED** (= `e_stopTimeS`). | `HGScripting.cpp:939` |
| `e_eventWait` | `(double duration)` | `void` | **DEPRECATED** (= `e_wait`). | `HGScripting.cpp:955` |
| `e_eventWaitS` | `(double duration)` | `void` | **DEPRECATED** (= `e_waitS`). | `HGScripting.cpp:971` |
| `e_eventWaitUntilS` | `(double duration)` | `void` | **DEPRECATED** (= `e_waitUntilS`). | `HGScripting.cpp:986` |

### 1.5 `l_` — level control functions (110)

#### 1.5.1 Direct `addLuaFn` registrations in `HGScripting.cpp`

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `l_overrideScore` | `(const string& variable)` | `void` | Sets `levelStatus.scoreOverride = variable; scoreOverridden = true`, then executes Lua code `if (type(<variable>) ~= "number") then error("Score override must be a number value") end` (the variable name is string-concatenated into the code; must be a **global**). On `runtime_error`: prints; if not `Config::getDebug()` → `goToMenu(false, true /*error*/)`. | `HGScripting.cpp:627` |
| `l_setRotation` | `(float angle)` | `void` | If `backgroundCamera` has value: `setRotation(angle)` (degrees). | `HGScripting.cpp:660` |
| `l_getRotation` | `()` | `float` | `backgroundCamera->getRotation()`, or `0.f` if no background camera. | `HGScripting.cpp:673` |
| `l_getOfficial` | `()` | `bool` | `Config::getOfficial()`. | `HGScripting.cpp:684` |
| `l_forceSetPulse` | `(const float value)` | `void` | (in `initLua_Deprecated`; **no** `raiseWarning` call, despite living there.) `status.pulse = value; refreshPulse()`. | `HGScripting.cpp:1086` |
| `l_forceSetBeatPulse` | `(const float value)` | `void` | (in `initLua_Deprecated`; **no** `raiseWarning` call.) `status.beatPulse = value; refreshBeatPulse(); player.updatePosition(status.radius)`. | `HGScripting.cpp:1094` |

> Note: `HexagonGame::makeLuaAccessor` (`HGScripting.cpp:604`, declared `HexagonGame.hpp:424`)
> contains two `addLuaFn` calls (`<prefix>_get<Name>` / `<prefix>_set<Name>`), but it is **never
> called** anywhere in the codebase — dead code. All real accessor generation comes from the
> free function `hg::LuaScripting::makeLuaAccessor` (`LuaScripting.cpp:57-79`).

#### 1.5.2 Accessor-generated `LevelStatus` getters/setters (43 pairs, 86 functions)

Generated by `makeLuaAccessor(lua, levelStatus, "l")` (`LuaScripting.cpp:511`): for each
member-pointer `pmd`, registers `l_get<Name>` = `() -> Type { return obj.*pmd; }` and
`l_set<Name>` = `(Type value) { obj.*pmd = value; }`, where `Type = std::decay_t<decltype(member)>`.
`Type` below is the C++ member type (from `include/SSVOpenHexagon/Data/LevelStatus.hpp`).

| Name (Lua: `l_get<Name>` / `l_set<Name>`) | Type | Semantics | Source:line |
|---|---|---|---|
| `SpeedMult` | `float` | Wall speed multiplier. | `LuaScripting.cpp:513` |
| `PlayerSpeedMult` | `float` | Player speed multiplier. | `:523` |
| `SpeedInc` | `float` | Additive per-increment speed change. | `:528` |
| `SpeedMax` | `float` | Max wall speed. | `:534` |
| `RotationSpeed` | `float` | Camera rotation speed. | `:542` |
| `RotationSpeedInc` | `float` | Per-increment rotation speed change. | `:550` |
| `RotationSpeedMax` | `float` | Max rotation speed. | `:558` |
| `DelayMult` | `float` | Delay multiplier. | `:568` |
| `DelayInc` | `float` | Per-increment delay change. | `:579` |
| `DelayMin` | `float` | Min delay. | `:585` |
| `DelayMax` | `float` | Max delay. | `:593` |
| `FastSpin` | `float` | Fast-spin intensity on increment. | `:601` |
| `IncTime` | `float` | Seconds per level increment. | `:611` |
| `PulseMin` | `float` | Pulse min. | `:621` |
| `PulseMax` | `float` | Pulse max. | `:628` |
| `PulseSpeed` | `float` | Pulse ramp-up speed. | `:635` |
| `PulseSpeedR` | `float` | Pulse ramp-down speed. | `:642` |
| `PulseDelayMax` | `float` | Max delay between pulse cycles. | `:648` |
| `PulseInitialDelay` | `float` | Initial pulse-cycle delay. | `:655` |
| `SwapCooldownMult` | `float` | 180°-swap cooldown multiplier. | `:662` |
| `BeatPulseMax` | `float` | Polygon beat-pulse max size. | `:669` |
| `BeatPulseDelayMax` | `float` | Beat-pulse cycle delay (FT). | `:678` |
| `BeatPulseInitialDelay` | `float` | Initial beat-pulse delay. | `:689` |
| `BeatPulseSpeedMult` | `float` | Beat-pulse speed multiplier. | `:700` |
| `RadiusMin` | `float` | Polygon min radius. | `:707` |
| `WallSkewLeft` | `float` | Y offset of top-left wall vertex. | `:714` |
| `WallSkewRight` | `float` | Y offset of top-right wall vertex. | `:723` |
| `WallAngleLeft` | `float` | X offset of top-left wall vertex. | `:732` |
| `WallAngleRight` | `float` | X offset of top-right wall vertex. | `:741` |
| `WallSpawnDistance` | `float` | Wall spawn distance. | `:750` |
| `3dRequired` | `bool` | Member `_3DRequired`. | `:757` |
| `ShadersRequired` | `bool` | | `:766` |
| `CameraShake` | `float` | | `:775` |
| `Sides` | `uint` (`unsigned int`) | | `:782` |
| `SidesMax` | `uint` | | `:790` |
| `SidesMin` | `uint` | | `:799` |
| `SwapEnabled` | `bool` | | `:808` |
| `TutorialMode` | `bool` | | `:815` |
| `IncEnabled` | `bool` | | `:833` |
| `DarkenUnevenBackgroundChunk` | `bool` | | `:844` |
| `ManualPulseControl` | `bool` | | `:853` |
| `ManualBeatPulseControl` | `bool` | | `:860` |
| `CurrentIncrements` | `unsigned long` (64-bit) | Member `currentIncrements` (`unsigned long`). | `:867` |

#### 1.5.3 Other `l_` functions in `LuaScripting.cpp` (6)

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `l_enableRndSideChanges` | `(bool enabled)` | `void` | `levelStatus.rndSideChangesEnabled = enabled` (allows sides to randomize between `SidesMin`/`SidesMax` each increment). | `LuaScripting.cpp:877` |
| `l_addTracked` | `(const string& variable, const string& name)` | `void` | `levelStatus.trackedVariables[variable] = name` (displayed in-game below the timer; variable must be global). | `:887` |
| `l_removeTracked` | `(const string& variable)` | `void` | `levelStatus.trackedVariables.erase(variable)`. | `:898` |
| `l_clearTracked` | `()` | `void` | `levelStatus.trackedVariables.clear()`. | `:904` |
| `l_getLevelTime` | `()` | `double` | `status.getTimeSeconds()`. | `:908` |
| `l_resetTime` | `()` | `void` | `status.resetTime()` (resets level timer, increment time, pause time). | `:912` |

#### 1.5.4 Accessor-generated `HexagonGameStatus` getters/setters (6 pairs, 12 functions)

Generated by `makeLuaAccessor(lua, status, "l")` (`LuaScripting.cpp:918`; all values from
`include/SSVOpenHexagon/Core/HGStatus.hpp`):

| Name (Lua: `l_get<Name>` / `l_set<Name>`) | Type | Semantics | Source:line |
|---|---|---|---|
| `Pulse` | `float` | Current pulse value (member `HexagonGameStatus::pulse`). | `LuaScripting.cpp:920` |
| `PulseDirection` | `float` | Current pulse direction (−1 or 1). | `:926` |
| `PulseDelay` | `float` | Current pulse delay. | `:933` |
| `BeatPulse` | `float` | Current beat-pulse value. | `:939` |
| `BeatPulseDelay` | `float` | Current beat-pulse delay. | `:945` |
| `ShowPlayerTrail` | `bool` | Whether player trails show. | `:951` |

### 1.6 `s_` — style control functions (56)

#### 1.6.1 Direct registration

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `s_setStyle` | `(const string& styleId)` | `void` | `styleData = assets.getStyleData(levelData->packId, id); styleData.computeColors()`. | `HGScripting.cpp:693` |
| `s_setCapColorMain` | `()` | `void` | `styleData.setCapColor(CapColor{CapColorMode::Main{}})`. | `LuaScripting.cpp:1132` |
| `s_setCapColorMainDarkened` | `()` | `void` | `styleData.setCapColor(CapColor{CapColorMode::MainDarkened{}})`. | `:1138` |
| `s_setCapColorByIndex` | `(int index)` | `void` | `styleData.setCapColor(CapColor{CapColorMode::ByIndex{index}})`. | `:1145` |
| `s_getMainColor` | `()` | `(int, int, int, int)` tuple | `colorToTuple(styleData.getMainColor())` → (r, g, b, a) of the computed main color (via `sdColorGetter`, `:1160`). | `:1162` |
| `s_getPlayerColor` | `()` | `(int, int, int, int)` tuple | Computed player color. | `:1165` |
| `s_getTextColor` | `()` | `(int, int, int, int)` tuple | Computed text color. | `:1168` |
| `s_get3DOverrideColor` | `()` | `(int, int, int, int)` tuple | Computed 3D-override color. | `:1170` |
| `s_getCapColorResult` | `()` | `(int, int, int, int)` tuple | Computed cap color result. | `:1173` |
| `s_getColor` | `(int index)` | `(int, int, int, int)` tuple | If `styleData.getColors()` empty → `(0, 0, 0, 0)`; else `colorToTuple(styleData.getColor(index))`. | `:1178` |

#### 1.6.2 Accessor-generated `StyleData` getters/setters (23 pairs, 46 functions)

Generated by `makeLuaAccessor(lua, styleData, "s")` (`LuaScripting.cpp:961`).
Member types from `include/SSVOpenHexagon/Data/StyleData.hpp`.

| Name (Lua: `s_get<Name>` / `s_set<Name>`) | Type | Notes | Source:line |
|---|---|---|---|
| `HueMin` | `float` | | `LuaScripting.cpp:962` |
| `HueMax` | `float` | | `:970` |
| `HueInc` | `float` | **Backwards-compat alias**: member is `hueIncrement`. | `:979` |
| `HueIncrement` | `float` | Same member `hueIncrement` as `HueInc` (both pairs registered). | `:984` |
| `PulseMin` | `float` | Style's pulse multiplier min (member `StyleData::pulseMin`). | `:994` |
| `PulseMax` | `float` | Style's pulse multiplier max. | `:1002` |
| `PulseInc` | `float` | **Backwards-compat alias**: member is `pulseIncrement`. | `:1012` |
| `PulseIncrement` | `float` | Same member as `PulseInc`. | `:1017` |
| `HuePingPong` | `bool` | | `:1027` |
| `MaxSwapTime` | `float` | | `:1035` |
| `3dDepth` | `float` | Member `_3dDepth`. | `:1044` |
| `3dSkew` | `float` | Member `_3dSkew`. | `:1049` |
| `3dSpacing` | `float` | Member `_3dSpacing`. | `:1057` |
| `3dDarkenMult` | `float` | Member `_3dDarkenMult`. | `:1063` |
| `3dAlphaMult` | `float` | Member `_3dAlphaMult`. | `:1069` |
| `3dAlphaFalloff` | `float` | Member `_3dAlphaFalloff`. | `:1077` |
| `3dPulseMax` | `float` | Member `_3dPulseMax`. | `:1086` |
| `3dPulseMin` | `float` | Member `_3dPulseMin`. | `:1091` |
| `3dPulseSpeed` | `float` | Member `_3dPulseSpeed`. | `:1096` |
| `3dPerspectiveMult` | `float` | Member `_3dPerspectiveMult`. | `:1103` |
| `BGTileRadius` | `float` | Member `bgTileRadius`. | `:1110` |
| `BGColorOffset` | `uint` (`unsigned int`) | Member `BGColorOffset` (0 or 1). | `:1118` |
| `BGRotationOffset` | `float` | Member is `BGRotOff` (degrees). | `:1125` |

### 1.7 `w_` — standard wall creation (5)

All `w_*` schedule into the **main timeline** (`timeline.append_do(createWall(...))`);
walls are created when the timeline reaches that point, not at call time.

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `w_wall` | `(int side, float thickness)` | `void` | `createWall(side, thickness, SpeedData{getSpeedMultDM()}, SpeedData{} /*curve*/, 0.f /*hueMod*/)`. | `HGScripting.cpp:707` |
| `w_wallAdj` | `(int side, float thickness, float speedMult)` | `void` | `createWall(side, thickness, SpeedData{speedMult * getSpeedMultDM()}, ...)` — speed adj multiplied by DM-adjusted speed. | `:724` |
| `w_wallAcc` | `(int side, float thickness, float speedMult, float acceleration, float minSpeed, float maxSpeed)` | `void` | Speed: `SpeedData{speedMult * getSpeedMultDM(), acceleration / pow(difficultyMult, 0.65f), minSpeed * getSpeedMultDM(), maxSpeed * getSpeedMultDM()}`. | `:744` |
| `w_wallHModSpeedData` | `(float hueModifier, int side, float thickness, float speedMult, float acceleration, float minSpeed, float maxSpeed, bool pingPong)` | `void` | Full `SpeedData{speedMult*getSpeedMultDM(), acceleration, minSpeed, maxSpeed, pingPong}` (no DM division on accel here), hue offset `hueModifier`. | `:773` |
| `w_wallHModCurveData` | `(float hueModifier, int side, float thickness, float curveSpeedMult, float curveAcceleration, float curveMinSpeed, float curveMaxSpeed, bool pingPong)` | `void` | Straight speed = `SpeedData{getSpeedMultDM()}`; curve `SpeedData{curveSpeedMult, curveAcceleration, curveMinSpeed, curveMaxSpeed, pingPong}` (curve values are NOT DM-adjusted); hue offset. | `:805` |

### 1.8 `cw_` — custom wall functions (20)

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `cw_create` | `()` | `handle` (`int`) | `cwManager.create([](CCustomWall&) {})` — new empty custom wall. | `LuaScripting.cpp:256` |
| `cw_createDeadly` | `()` | `handle` (`int`) | `create` + `cw.setDeadly(true)`. | `:261` |
| `cw_createNoCollision` | `()` | `handle` (`int`) | `create` + `cw.setCanCollide(false)`. | `:270` |
| `cw_destroy` | `(handle cwHandle)` | `void` | `cwManager.destroy(handle)`. | `:279` |
| `cw_setVertexPos` | `(handle, int vertexIndex, float x, float y)` | `void` | `cwManager.setVertexPos(handle, idx, sf::Vector2f{x, y})`. | `:285` |
| `cw_moveVertexPos` | `(handle, int vertexIndex, float offsetX, float offsetY)` | `void` | `moveVertexPos` — adds offset to vertex position. | `:298` |
| `cw_moveVertexPos4Same` | `(handle, float x, float y)` | `void` | Adds offset to all 4 vertices. | `:311` |
| `cw_setVertexColor` | `(handle, int vertexIndex, int r, int g, int b, int a)` | `void` | `setVertexColor(handle, idx, sf::Color(r, g, b, a))`. | `:322` |
| `cw_setVertexPos4` | `(handle, float x0, float y0, float x1, float y1, float x2, float y2, float x3, float y3)` | `void` | Sets all 4 vertices in one call. | `:338` |
| `cw_setVertexColor4` | `(handle, int r0, int g0, int b0, int a0, int r1, int g1, int b1, int a1, int r2, int g2, int b2, int a2, int r3, int g3, int b3, int a3)` | `void` | Sets all 4 vertex colors in one call. | `:366` |
| `cw_setVertexColor4Same` | `(handle, int r, int g, int b, int a)` | `void` | Sets one color on all 4 vertices. | `:403` |
| `cw_setCollision` | `(handle, bool collision)` | `void` | `setCanCollide` (false → player passes through, can't die). | `:417` |
| `cw_setDeadly` | `(handle, bool deadly)` | `void` | `setDeadly` (kill on touch). | `:428` |
| `cw_setKillingSide` | `(handle, uint side)` | `void` | `setKillingSide` (0–3; 0 = side closer to center; useless if deadly). | `:439` |
| `cw_getCollision` | `(handle)` | `bool` | `getCanCollide`. | `:451` |
| `cw_getDeadly` | `(handle)` | `bool` | `getDeadly`. | `:459` |
| `cw_getKillingSide` | `(handle)` | `uint` | `getKillingSide`. | `:467` |
| `cw_getVertexPos` | `(handle, int vertexIndex)` | `(float, float)` tuple | `getVertexPos` → (x, y). | `:475` |
| `cw_getVertexPos4` | `(handle)` | `(float ×8)` tuple | `getVertexPos4` → (x0, y0, x1, y1, x2, y2, x3, y3). | `:490` |
| `cw_clear` | `()` | `void` | `cwManager.clear()` — removes all custom walls. | `:503` |

### 1.9 `ct_` — custom timeline functions (8)

`ct_create` returns a `CustomTimelineHandle` (`int`). Every other call first validates the
handle via `_customTimelineManager.isHandleValid(cth)`; an invalid handle logs to the
`CustomTimelineManager` log channel and **no-ops** (no Lua error).

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `ct_create` | `()` | `handle` (`int`) | `_customTimelineManager.create()`. | `HGScripting.cpp:463` |
| `ct_eval` | `(handle, const string& code)` | `void` | On the handle's timeline: `append_do(runLuaCode(lua, code))`. | `:485` |
| `ct_kill` | `(handle)` | `void` | `append_do(death(true))`. | `:502` |
| `ct_stopTime` | `(handle, double duration)` | `void` | `append_do(status.pauseTime(getFTToSeconds(duration)))`. | `:516` |
| `ct_stopTimeS` | `(handle, double duration)` | `void` | `append_do(status.pauseTime(duration))`. | `:534` |
| `ct_wait` | `(handle, double duration)` | `void` | `append_wait_for_sixths(duration)`. | `:551` |
| `ct_waitS` | `(handle, double duration)` | `void` | `append_wait_for_seconds(duration)`. | `:568` |
| `ct_waitUntilS` | `(handle, double duration)` | `void` | `append_wait_until_fn(levelStartTP + duration*1000 ms)`. | `:585` |

### 1.10 `shdr_` — shader functions (13)

All uniform-setters validate the shader id via `withValidShaderId` (no-op + log if headless
or invalid id). Render-stage ids are validated against `status.fragmentShaderIds` size
(`RenderStage::Count == 9`).

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `shdr_getShaderId` | `(const string& shaderFilename)` | `size_t` | Headless → `size_t(-1)`. Else resolves `<ctx>/Shaders/<filename>` (extension included in filename, e.g. `text.frag`), `assets.getShaderIdByPath`; missing → log + `size_t(-1)`. | `LuaScripting.cpp:1238` |
| `shdr_getDependencyShaderId` | `(const string& disambiguator, const string& packName, const string& packAuthor, const string& shaderFilename)` | `size_t` | Same dependency-pack validation as `u_execDependencyScript`, resolves `<dependeeFolder>/Shaders/<filename>`. | `:1274` |
| `shdr_setUniformF` | `(size_t shaderId, const string& name, float a)` | `void` | `shader.setUniformUnsafe(name, a)`. | `:1377` |
| `shdr_setUniformFVec2` | `(size_t, const string&, float a, float b)` | `void` | `sf::Glsl::Vec2`. | `:1391` |
| `shdr_setUniformFVec3` | `(size_t, const string&, float a, float b, float c)` | `void` | `sf::Glsl::Vec3`. | `:1408` |
| `shdr_setUniformFVec4` | `(size_t, const string&, float a, float b, float c, float d)` | `void` | `sf::Glsl::Vec4`. | `:1426` |
| `shdr_setUniformI` | `(size_t, const string&, int a)` | `void` | integer uniform. | `:1448` |
| `shdr_setUniformIVec2` | `(size_t, const string&, int a, int b)` | `void` | `sf::Glsl::Ivec2`. | `:1462` |
| `shdr_setUniformIVec3` | `(size_t, const string&, int a, int b, int c)` | `void` | `sf::Glsl::Ivec3`. | `:1479` |
| `shdr_setUniformIVec4` | `(size_t, const string&, int a, int b, int c, int d)` | `void` | `sf::Glsl::Ivec4`. | `:1497` |
| `shdr_resetAllActiveFragmentShaders` | `()` | `void` | Sets all 9 `status.fragmentShaderIds[i] = std::nullopt`. | `:1519` |
| `shdr_resetActiveFragmentShader` | `(size_t renderStage)` | `void` | `ids[renderStage] = nullopt` (out of range → log + no-op). | `:1532` |
| `shdr_setActiveFragmentShader` | `(size_t renderStage, size_t shaderId)` | `void` | `ids[renderStage] = shaderId` (out of range → log + no-op). | `:1549` |

### 1.11 Miscellaneous (no prefix category) (5)

| Lua name | C++ lambda arg types (exact) | Return type (exact) | Semantics (transcribed) | Source:line |
|---|---|---|---|---|
| `steam_unlockAchievement` | `(const string& achievementId)` | `void` | No-op in replay (`inReplay()`). Else if `steamManager != nullptr && Config::getOfficial()`: `steamManager->unlock_achievement(id)`. | `HGScripting.cpp:839` |
| `m_messageAdd` | `(const string& message, double duration)` | `void` | **DEPRECATED** (= `e_messageAdd`, first-play-only) + `raiseWarning`. | `HGScripting.cpp:1008` |
| `m_messageAddImportant` | `(const string& message, double duration)` | `void` | **DEPRECATED** (= `e_messageAddImportant`) + `raiseWarning`. | `:1033` |
| `m_messageAddImportantSilent` | `(const string& message, double duration)` | `void` | **DEPRECATED** (= `e_messageAddImportantSilent`) + `raiseWarning`. | `:1053` |
| `m_clearMessages` | `()` | `void` | **DEPRECATED** (= `e_clearMessages`) + `raiseWarning`. | `:1072` |

**Function count: 285 Lua global function names** — u_: 35, a_: 11, t_: 6, e_: 16 (11 + 5 deprecated),
l_: 110 (6 + 86 + 6 + 12), s_: 56 (10 + 46), w_: 5, cw_: 20, ct_: 8, shdr_: 13, Misc: 5 (1 + 4 deprecated).
(Plus `u_impl_addTrackedResult` already counted in u_; registered without metadata.)

---

## 2. Exact spelling of every global name

All 285 names exactly as registered (any typo breaks the API). Full list:

```
u_rndReal u_rndIntUpper u_rndInt u_rndSwitch u_getAttemptRandomSeed
u_inMenu u_isHeadless u_getVersionMajor u_getVersionMinor u_getVersionMicro u_getVersionString
u_execScript u_execDependencyScript u_getWidth u_getHeight
u_impl_addTrackedResult
u_setFlashEffect u_setFlashColor u_log u_isKeyPressed u_haltTime u_clearWalls
u_getPlayerAngle u_setPlayerAngle u_isMouseButtonPressed u_isFastSpinning
u_forceIncrement u_getDifficultyMult u_getSpeedMultDM u_getDelayMultDM u_swapPlayer
u_kill u_eventKill u_playSound u_playPackSound
a_setMusic a_setMusicSegment a_setMusicSeconds a_playSound a_playPackSound
a_syncMusicToDM a_setMusicPitch a_overrideBeepSound a_overrideIncrementSound
a_overrideSwapSound a_overrideDeathSound
t_eval t_clear t_kill t_wait t_waitS t_waitUntilS
e_eval e_kill e_stopTime e_stopTimeS e_wait e_waitS e_waitUntilS
e_messageAdd e_messageAddImportant e_messageAddImportantSilent e_clearMessages
e_eventStopTime e_eventStopTimeS e_eventWait e_eventWaitS e_eventWaitUntilS
l_overrideScore l_setRotation l_getRotation l_getOfficial
l_forceSetPulse l_forceSetBeatPulse
l_getSpeedMult l_setSpeedMult l_getPlayerSpeedMult l_setPlayerSpeedMult
l_getSpeedInc l_setSpeedInc l_getSpeedMax l_setSpeedMax
l_getRotationSpeed l_setRotationSpeed l_getRotationSpeedInc l_setRotationSpeedInc
l_getRotationSpeedMax l_setRotationSpeedMax l_getDelayMult l_setDelayMult
l_getDelayInc l_setDelayInc l_getDelayMin l_setDelayMin l_getDelayMax l_setDelayMax
l_getFastSpin l_setFastSpin l_getIncTime l_setIncTime
l_getPulseMin l_setPulseMin l_getPulseMax l_setPulseMax
l_getPulseSpeed l_setPulseSpeed l_getPulseSpeedR l_setPulseSpeedR
l_getPulseDelayMax l_setPulseDelayMax l_getPulseInitialDelay l_setPulseInitialDelay
l_getSwapCooldownMult l_setSwapCooldownMult l_getBeatPulseMax l_setBeatPulseMax
l_getBeatPulseDelayMax l_setBeatPulseDelayMax
l_getBeatPulseInitialDelay l_setBeatPulseInitialDelay
l_getBeatPulseSpeedMult l_setBeatPulseSpeedMult l_getRadiusMin l_setRadiusMin
l_getWallSkewLeft l_setWallSkewLeft l_getWallSkewRight l_setWallSkewRight
l_getWallAngleLeft l_setWallAngleLeft l_getWallAngleRight l_setWallAngleRight
l_getWallSpawnDistance l_setWallSpawnDistance
l_get3dRequired l_set3dRequired l_getShadersRequired l_setShadersRequired
l_getCameraShake l_setCameraShake l_getSides l_setSides l_getSidesMax l_setSidesMax
l_getSidesMin l_setSidesMin l_getSwapEnabled l_setSwapEnabled
l_getTutorialMode l_setTutorialMode l_getIncEnabled l_setIncEnabled
l_getDarkenUnevenBackgroundChunk l_setDarkenUnevenBackgroundChunk
l_getManualPulseControl l_setManualPulseControl
l_getManualBeatPulseControl l_setManualBeatPulseControl
l_getCurrentIncrements l_setCurrentIncrements
l_enableRndSideChanges l_addTracked l_removeTracked l_clearTracked
l_getLevelTime l_resetTime
l_getPulse l_setPulse l_getPulseDirection l_setPulseDirection
l_getPulseDelay l_setPulseDelay l_getBeatPulse l_setBeatPulse
l_getBeatPulseDelay l_setBeatPulseDelay l_getShowPlayerTrail l_setShowPlayerTrail
s_setStyle
s_setCapColorMain s_setCapColorMainDarkened s_setCapColorByIndex
s_getMainColor s_getPlayerColor s_getTextColor s_get3DOverrideColor s_getCapColorResult s_getColor
s_getHueMin s_setHueMin s_getHueMax s_setHueMax s_getHueInc s_setHueInc
s_getHueIncrement s_setHueIncrement s_getPulseMin s_setPulseMin s_getPulseMax s_setPulseMax
s_getPulseInc s_setPulseInc s_getPulseIncrement s_setPulseIncrement
s_getHuePingPong s_setHuePingPong s_getMaxSwapTime s_setMaxSwapTime
s_get3dDepth s_set3dDepth s_get3dSkew s_set3dSkew s_get3dSpacing s_set3dSpacing
s_get3dDarkenMult s_set3dDarkenMult s_get3dAlphaMult s_set3dAlphaMult
s_get3dAlphaFalloff s_set3dAlphaFalloff s_get3dPulseMax s_set3dPulseMax
s_get3dPulseMin s_set3dPulseMin s_get3dPulseSpeed s_set3dPulseSpeed
s_get3dPerspectiveMult s_set3dPerspectiveMult
s_getBGTileRadius s_setBGTileRadius s_getBGColorOffset s_setBGColorOffset
s_getBGRotationOffset s_setBGRotationOffset
w_wall w_wallAdj w_wallAcc w_wallHModSpeedData w_wallHModCurveData
cw_create cw_createDeadly cw_createNoCollision cw_destroy
cw_setVertexPos cw_moveVertexPos cw_moveVertexPos4Same cw_setVertexColor
cw_setVertexPos4 cw_setVertexColor4 cw_setVertexColor4Same
cw_setCollision cw_setDeadly cw_setKillingSide
cw_getCollision cw_getDeadly cw_getKillingSide cw_getVertexPos cw_getVertexPos4 cw_clear
ct_create ct_eval ct_kill ct_stopTime ct_stopTimeS ct_wait ct_waitS ct_waitUntilS
shdr_getShaderId shdr_getDependencyShaderId
shdr_setUniformF shdr_setUniformFVec2 shdr_setUniformFVec3 shdr_setUniformFVec4
shdr_setUniformI shdr_setUniformIVec2 shdr_setUniformIVec3 shdr_setUniformIVec4
shdr_resetAllActiveFragmentShaders shdr_resetActiveFragmentShader shdr_setActiveFragmentShader
steam_unlockAchievement
m_messageAdd m_messageAddImportant m_messageAddImportantSilent m_clearMessages
```

Notes on spellings and categories:

- Categories (docs grouping, `LuaMetadata.cpp:13-36`): prefix up to first `_` —
  `u_ a_ t_ e_ l_ s_ w_ cw_ ct_ shdr_`, everything else = "Miscellaneous"
  (this puts `steam_unlockAchievement` and all `m_*` in Miscellaneous; `cw_` must be
  matched **before** a hypothetical `c_` rule).
- Deprecated spellings that must keep working: `u_kill`, `u_eventKill`, `u_playSound`,
  `u_playPackSound`, `e_eventStopTime`, `e_eventStopTimeS`, `e_eventWait`, `e_eventWaitS`,
  `e_eventWaitUntilS`, `m_messageAdd`, `m_messageAddImportant`,
  `m_messageAddImportantSilent`, `m_clearMessages`, `u_isKeyPressed`,
  `u_isMouseButtonPressed`. (`l_forceSetPulse` / `l_forceSetBeatPulse` live in
  `initLua_Deprecated` but do **not** call `raiseWarning`.)
- The menu stub list (`MenuGame.cpp:1089-1128`) mentions a historical name
  **`u_timelineWait`** (and `u_setFlashEffect` twice) that is not registered as a real
  function anywhere — it exists only as a menu no-op stub. It is a vestige of an older API.
- `s_get3DOverrideColor` has a capital `3D` while the `StyleData` accessors use lowercase
  `3d` (`s_get3dDepth`, etc.). The two families are **not** consistent by design.
- `RenderStage` (used by `shdr_*` callers in Lua, e.g. `RenderStage.TEXT`) is **not** a
  C++-registered global — it is a Lua table defined in `_RELEASE/Packs/base/Scripts/utils.lua:30`
  and must match the C++ enum `RenderStage` (`HGStatus.hpp:26-38`):
  `BACKGROUNDTRIS=0 WALLQUADS3D=1 PIVOTQUADS3D=2 PLAYERTRIS3D=3 WALLQUADS=4 CAPTRIS=5
  PIVOTQUADS=6 PLAYERTRIS=7 TEXT=8` (C++ `Count = 9`).
- Other Lua-side enumerator tables in `utils.lua` (not C++ globals): `Movement`
  (`LEFT=-1 NONE=0 RIGHT=1`), `MouseButton` (SFML `sf::Mouse::Button` values), `KeyCode`
  (SFML `sf::Keyboard::Key` values), `CURVE_ROTATION_MULTIPLIER = 10.471975`,
  `CURVE_ROTATION_FOCUS = 2.0436`.

---

## 3. Lua ↔ C++ conversion rules (from `LuaWrapper.hpp/.cpp`)

Lua runtime: stock Lua 5.x C API (`#include <lua.hpp>`); `lua_Integer`/`lua_Number` are
64-bit (Lua 5.3+ semantics: integers are 64-bit, `lua_Number` = `double`).

### 3.1 State creation

- `LuaContext(bool openDefaultLibs = true)` → `lua_newstate` with a custom
  `realloc`-based allocator (NOT `luaL_newstate`, to trace memory); `openDefaultLibs`
  → `luaL_openlibs` (all stdlibs: `base`, `coroutine`, `table`, `io`, `os`,
  `string`, `bit32`/`bitlib`, `utf8`, `math`, `package`, `debug`).
- Both the game (`HexagonGame::lua`) and menu (`MenuGame::lua`) contexts are created
  with the default `true` — then sandboxed (below). A Rust port must replicate the
  exact set of enabled stdlib functions after removals (§3.3).
- State is NOT thread-safe (single stack); no mutex in this vendored version.

### 3.2 Push rules (C++ value → Lua value)

| C++ type | Lua value | Rule |
|---|---|---|
| integral (`int`, `uint`, `ull`, `size_t`, `char`…) | integer | `lua_pushinteger` (64-bit) |
| floating (`float`, `double`) | number | `lua_pushnumber` (converted to `double`) |
| `bool` | boolean | `lua_pushboolean` |
| `std::string` / `const char*` / char arrays | string | `lua_pushstring` |
| `std::tuple<Ts...>` | N return values | each element pushed in order (this is how `cw_getVertexPos` → `x, y` and color getters → `r, g, b, a` become Lua multiple returns) |
| `std::shared_ptr<T>` | full userdata | metatable with `__gc`, `_typeid`, `__index` (registry table of member fns) — **unused by this API** (no object wrappers are exposed) |
| functor/lambda | full userdata | metatable with `__call`, `_typeid`, `__gc`; this is how every `addLuaFn` works |
| `std::map<K,V>` | table | keys/values pushed recursively (used internally only) |
| `Lua::Table` | table | insert-only helper (unused by this API) |

### 3.3 Sandboxing (`destroyMaliciousFunctions`, `LuaScripting.cpp:178-218`)

Executed **after** `redefineIoOpen` + `redefineRandom`, inside `LuaScripting::init`:

- `os` → cleared entirely (no `os.execute`, `os.remove`, `os.clock`, `os.time`…).
- `io.popen`, `io.flush`, `io.write`, `io.setvbuf` → cleared (read-only `io` remains:
  `io.open`/`io.lines`/`io.read`/`io.type`…).
- `debug` → cleared entirely (defeats `getregistry`-based restore of cleared modules).
- `math.randomseed` → cleared (pack scripts cannot reseed the RNG).
- `package.loadlib`, `package.searchpath`, `package.loaders`,
  `package.loaded.os`, `package.loaded.debug` → cleared (`package` itself survives so
  `require`/`package.path` keep working for Lua-side module loading; `package.loadlib`
  is cleared **twice** — lines 207 and 217).
- `io.open` redefined (`LuaScripting.cpp:141-154`): wraps the original; any mode other
  than `"rb"` is coerced to `"r"` — i.e. all file opens become **read-only text** mode.
- `math.random` redefined (`LuaScripting.cpp:156-175`, raw string, verbatim):

```lua
math.random = function(a, b)
    if a == nil and b == nil then
        return u_rndSwitch(0, 0, 0)
    elseif b == nil then
        return u_rndSwitch(1, 0, a)
    else
        return u_rndSwitch(2, a, b)
    end
end
```

  (Note: `math.random()` → `u_rndReal`; `math.random(n)` → `u_rndIntUpper(n)` i.e.
  `[1, n]`; `math.random(a, b)` → `u_rndInt(a, b)`. All other `math.*` functions
  (`math.abs`, `math.floor`, …) remain stock.)

### 3.4 Read rules (Lua value → C++ arg) — `FunctionToPush::operator()` + `_read`

- Argument count check: `if (lua_gettop(state) < TupledFunction::count)` → Lua error
  `"this function requires at least N parameter(s)"` (via `luaL_where` + `lua_error`).
  In a clean call frame `lua_gettop` equals the number of arguments passed, so a
  short call does raise this error; extra Lua args beyond N are always ignored.
  (The check compares against the whole stack top, so a dirty stack can mask or
  trip it — a known rough edge of the wrapper.)
- Per-type coercion (each `_read` first throws `WrongTypeException` if the value is
  **userdata**):
  - int ← `lua_tointeger` (nil → 0; a Lua float is truncated toward zero)
  - float/double ← `lua_tonumber` (nil → 0.0)
  - bool ← `lua_toboolean != 0` (nil → false; **any nonzero number → true**)
  - string ← `lua_tostring` (nil → `nullptr`; constructing `std::string(nullptr)` is
    UB — a missing/nil string arg is a latent crash, no guard exists)
- Explicit `nil` arguments (and any slot beyond the call's top, if the arity check
  above is bypassed by a dirty stack) are read as: integers 0, floats 0.0,
  booleans false, strings UB (see above).

### 3.5 Error semantics

- Script execution goes through `lua_pcall` inside `_call`:
  - `LUA_ERRMEM` → `std::bad_alloc`
  - `LUA_ERRRUN` (Lua `error()`, runtime crash, etc.) → `Lua::ExecutionErrorException(msg)`
  - other rc → `std::runtime_error("UNKNOWN RC: " + msg)`
- Load failures (`lua_load`/`luaL_loadbuffer`, chunk name is literally `"chunk"`):
  `LUA_ERRMEM` → `bad_alloc`; `LUA_ERRSYNTAX` → `Lua::SyntaxErrorException(msg)`.
- `Utils::runLuaCode` / `Utils::runLuaFile` (`Utils.cpp:14-112`): catch
  `runtime_error`, log `"Fatal Lua error"` + code/filename + message, rethrow.
  `HexagonGame::runLuaFile` (`HGScripting.cpp:1127-1146`) additionally calls
  `goToMenu(false, true /*mError*/) ` on any exception when not in debug mode.
- `luaExceptionLippincottHandler` (`HGScripting.cpp:1155-1185`): wraps callback
  invocations; prints `[runLuaFunctionIfExists] Runtime error on "<name>" with level
  "<level name>"`, then same goToMenu behavior.
- `raiseWarning(name, msg)` (deprecated fns): logs a warning and records in
  `calledDeprecatedFunctions` (used for first-run "please update your level" prompts).

### 3.6 Variable access helpers used by the engine

- `doesVariableExist(name)` (no-dots → `lua_getglobal` + `lua_isnil`; dotted `a.b.c`
  walks tables, `VariableDoesntExistException` on missing/non-table) — used by
  `runLuaFunctionIfExists` before every callback.
- `readVariable<T>` / `writeVariable(name, value)` — dotted paths supported;
  `writeVariable` of a functor is how all 285 functions + menu stubs get installed.
- `clearVariable` sets nil (used by the sandbox).
- `callLuaFunction<T>(name, args...)` — the callback invocation path; `T` may be
  `void`, a scalar, or a `std::tuple`; `Utils::runLuaFunctionIfExists` wraps the result
  in `std::optional<T>` (nil callback → `std::nullopt`; nil return value is
  **coerced**, e.g. `nil` → `false` for `bool`).

---

## 4. Exact RNG mapping

Engine: `hg::random_number_generator` (`include/SSVOpenHexagon/Core/RandomNumberGenerator.hpp`):

- `engine_type = pcg32_fast` (SSVUtils vendored PCG, `extlibs/SSVUtils` Internal/PCG).
- `seed_type = random_number_generator_seed_type = unsigned long long` (`RandomNumberGeneratorTypes.hpp:9`).
- `get_int<T>(min, max)` → `std::uniform_int_distribution<T>{min, max}(rng)` (**closed** interval `[min, max]`).
- `get_real<T>(min, max)` → `std::uniform_real_distribution<T>{min, max}(rng)` (`[min, max)`).
- `advance(delta)` → `rng.advance(delta)` (state-skipping, used by the engine each frame).
- `seed()` → the constructor seed (unchanged by draws).

Seeding:

- Fresh game: `initializeRng()` (`HexagonGame.cpp:59-66`) — a `thread_local pcg32_fast`
  seeded from `std::random_device` (via `seed_seq_from`); `HexagonGame::rng` is
  constructed from one draw of that thread-local.
- Replay: `rng` is reconstructed from `replayFile._seed` (`HexagonGame.cpp:639`);
  `lastSeed` tracks `rng.seed()` (`:598`) and is stored into new replay files (`:617`, `:1002`).
- Menu: `static random_number_generator rng{0}` (`MenuGame.cpp:1065`) — **fixed seed 0**.

Lua bindings (all consume the **same** engine as the C++ game logic — this is what makes
replays deterministic, and the draw order matters):

| Lua function | C++ call | Range |
|---|---|---|
| `u_rndReal()` | `rng.get_real<float>(0, 1)` | real in `[0, 1)` |
| `u_rndIntUpper(upper)` | `rng.get_int<int>(1, upper)` | int in `[1, upper]` |
| `u_rndInt(lower, upper)` | `rng.get_int<int>(lower, upper)` | int in `[lower, upper]` |
| `u_rndSwitch(mode, lower, upper)` | dispatch: 0 → `u_rndReal`; 1 → `u_rndIntUpper(upper)`; 2 → `u_rndInt(lower, upper)`; else `SSVOH_ASSERT(false)` + return 0, **no RNG consumption** | — |
| `u_getAttemptRandomSeed()` | `rng.seed()` | the 64-bit seed itself (read-only) |
| `math.random()` / `(n)` / `(a,b)` | Lua shim → `u_rndSwitch(0/1/2, …)` (see §3.3) | as above |

Consumption by the C++ engine from the SAME stream (must be replicated in the Rust port
to keep Lua `math.random` sequences identical):

- Every frame in `HexagonGame::update` (`HGUpdate.cpp:353-357`):
  `rng.advance(fixup(status.pulse))`, `rng.advance(fixup(status.pulse3D))`,
  `rng.advance(fixup(status.fastSpin))`, `rng.advance(fixup(status.flashEffect))`,
  `rng.advance(fixup(levelStatus.rotationSpeed))`.
- Random side changes (`HGUpdate.cpp:301`): `rng.get_int(...)` between `SidesMin`/`SidesMax`.

Doc discrepancy to note: the `u_getAttemptRandomSeed` docstring claims
"`math.randomseed` is automatically initialized with the result of this function at the
beginning of a level" — **the code never does this** (and `math.randomseed` is deleted);
packs instead call `u_getAttemptRandomSeed()` themselves if they need the seed.

---

## 5. Callbacks (functions the scripts define; the engine calls)

All are invoked with `Utils::runLuaFunctionIfExists<T>(lua, name, args...)`
(`Utils.hpp:58-79`), which first checks `doesVariableExist(name)` — if the callback is
not defined, the call is silently skipped (no error, no return).

| Callback | Exact name | C++ arg types | Expected Lua return | Call site(s) | Semantics |
|---|---|---|---|---|---|
| init | `onInit` | `()` | ignored (void) | `HexagonGame.cpp:746` (level, right after `runLuaFile(luaScriptPath)`); `MenuGame.cpp:3062` (menu, after `runLuaFile`) | Level/menu setup; runs once per load (also on restart). |
| load | `onLoad` | `()` | ignored (void) | `HGUpdate.cpp:549` (game, when the level actually starts playing); `MenuGame.cpp:3063` | "Level started" hook (after countdown / start input). |
| update | `onUpdate` | `(FT)` = `(float mFT)` | `float` (return value is **ignored** — C++ calls with `T=float` but discards it) | `HGUpdate.cpp:738` (`updateLevel`, skipped when `status.isTimePaused()`) | Per-frame update with frametime (1.0 = 1/60 s). Lua corpus convention: `function onUpdate(mFT)`. |
| step | `onStep` | `()` | ignored (void) | `HGUpdate.cpp:745` (in `updateLevel`, when the main `timeline` finishes and `!mustChangeSides`) | "Pattern step finished" — pattern authors schedule the next step here. |
| input | `onInput` | `(FT, int, bool, bool)` = `(float mFT, int movement, bool focused, bool swap)` | `bool` — returning `true` **prevents** the default player input for this frame | `HGUpdate.cpp:244` (only when `status.started && !status.hasDied`; `movement` = `getInputMovement()` ∈ {−1, 0, 1}; `focused` = `getInputFocused()` (A/D or left/right arrows active); `swap` = `getInputSwap()`) | Lua convention: `function onInput(mFrameTime, mMovement, mFocus, mSwap)` (see `babysteps.lua:92`, `arcadia.lua:438`). |
| pre-death | `onPreDeath` | `()` | ignored (void) | `HexagonGame.cpp:954` (inside `death()`, **before** the invincibility/tutorial-mode check) | Fired even when the death is ignored by invincibility; `deathInputIgnore = 10.f` set just before. |
| death | `onDeath` | `()` | ignored (void) | `HexagonGame.cpp:961` (inside `death()`, only when the death actually proceeds, i.e. not invincible/tutorial and not `mForce`-suppressed) | Followed by `death_shakeCamera()`, `stopLevelMusic()`, etc. |
| increment | `onIncrement` | `()` | ignored (void) | `HexagonGame.cpp:1174` (end of `incrementDifficulty()`, after `playSoundOverride(levelUpSound)`) | Fired on every level increment (also from `u_forceIncrement()`). |
| pre-unload | `onPreUnload` | `()` | ignored (void) | `HexagonGame.cpp:730` (only when `!firstPlay`, **before** the `lua` context is destroyed and re-inited) | Last chance to run with the old context on restart. Not used by the shipped corpus. |
| unload | `onUnload` | `()` | ignored (void) | `HexagonGame.cpp:738` (restart path, `!firstPlay`, after fresh `initLua()` + script re-run) and `HexagonGame.cpp:1274` (exit-to-menu via `goToMenu`, only when `!mError`) | Cleanup hook. |
| cursor swap | `onCursorSwap` | `()` | ignored (void) | `HexagonGame.cpp:1659` (inside `performPlayerSwap`, after `player.playerSwap()`, before the swap sound) | Fired on every 180° swap, including `u_swapPlayer()` calls from Lua. |
| render stage | `onRenderStage` | `(int renderStage, float ft)` where `renderStage` = `static_cast<int>(RenderStage)` (0–8) and `ft` = `60.f / window->getFPS()` | `int` (the C++ side **discards** the return value; the call exists for side effects — scripts use it to call `shdr_setActiveFragmentShader` per stage) | `HGGraphics.cpp:75` (inside `getRenderStates(rs)`, per render stage, only when `Config::getShaders()` and a fragment shader id is set for that stage) | Lua convention: `function onRenderStage(rs)` (see `shaders_required.lua`, `text_shader.lua` comment). |

Callbacks that do **NOT** exist in this codebase (present in older OpenHexagon / other
engines, asked for in the task): `onSwap`, `onEvent`, `onMessage`, `onKill` — none are
invoked anywhere in C++ and none appear in the 42-file Lua corpus. The closest
equivalents: `onCursorSwap` (swap), `onStep`/`e_eval` (events), `e_messageAdd*`
(messages), `onDeath`/`onPreDeath`/`t_kill` (kill).

---

## 6. Include mechanism (`u_execScript` / `u_execDependencyScript`)

### 6.1 Exact signatures

- `u_execScript(scriptFilename: string)` — `LuaScripting.cpp:1200-1207`.
  Body: `fRunLuaFile(Utils::getDependentScriptFilename(execScriptPackPathContext, fPackPathGetter(), mScriptName))`.
  In the game, `fRunLuaFile` is a lambda wrapping `HexagonGame::runLuaFile(filename)`
  (`HGScripting.cpp:1127`); in the menu, `MenuGame::runLuaFile`.
- `u_execDependencyScript(packDisambiguator, packName, packAuthor, scriptFilename: 4×string)` —
  `LuaScripting.cpp:1210-1231`. Body: `Utils::withDependencyScriptFilename(fRunLuaFile,
  execScriptPackPathContext, assets, fGetPackData(), disambiguator, name, author, script)`.

### 6.2 Pack-path resolution (`Utils.cpp`)

- `getDependentAssetFilename(subfolder, ctx, currentPackPath, assetName)`
  (`Utils.cpp:165-175`): `context = ctx.empty() ? currentPackPath : ctx.back()`;
  returns `concat(context, subfolder, '/', assetName)`.
  → **`u_execScript(x)` runs `<context>/Scripts/x`**, where context is the *innermost
  active dependency folder* (top of the stack) or the level's own pack path if no
  dependency context is active. `currentPackPath` comes from
  `levelData->packPath` (game) / `levelData->packPath` (menu).
- `withDependencyAssetFilename` (`Utils.cpp:177-207`):
  1. `findDependencyPackDataOrThrow(assets, currentPack, disambiguator, name, author)`
     (`Utils.cpp:116-163`): the triple must match an entry of
     `currentPack.dependencies` (fields `PackDependency{disambiguator, name, author, minVersion}`);
     then `assets.findPackData(disambiguator, name, author)` must find a loaded pack;
     then `dependencyData->version >= depIt->minVersion` must hold. Each failure throws
     `std::runtime_error` with a descriptive message (e.g. `"Pack with disambiguator '…'
     … is not a dependency of '<current pack name>'\n"`).
  2. Pushes `dependencyData.folderPath` onto `execScriptPackPathContext` (a
     `std::vector<std::string>` per `HexagonGame` / `MenuGame`, `HexagonGame.hpp:121`,
     `MenuGame.hpp:117`), scope-guard pops it — so nested `u_execScript` calls *inside*
     the dependency script resolve relative to the **dependency** pack, not the
     original one.
  3. Invokes `f(dependencyData.folderPath + "/Scripts/" + scriptName)`.
- Errors propagate to `HexagonGame::runLuaFile`'s catch → log + `goToMenu(error)`
  (non-debug).

### 6.3 File execution (`Utils.cpp:49-112`)

- `runLuaFileCached(assets, lua, fileName)` (`:49-78`): keeps an
  `std::unordered_map<string, string>` in `assets.getLuaFileCache()`; on miss, reads the
  whole file (binary) into the map, then `runLuaCode(lua, cachedString)`; returns
  whether it was a cache hit.
- `runLuaFile(lua, fileName)` (`:80-112`): `std::ifstream` — if the file cannot be
  opened: throws `runtime_error("Fatal Lua error\nCould not open file: <name>\n")`;
  else `lua.executeCode(stream)` (loads with chunk name `"chunk"`), rethrows on error.
- `HexagonGame::runLuaFile` (`HGScripting.cpp:1127-1146`): picks
  `runLuaFileCached` when **headless or `Config::getUseLuaFileCache()`**, else
  `runLuaFile`; on exception → `goToMenu(false, true)` unless debug.
- The level's own script (`levelData->luaScriptPath`) is executed the same way on load
  (`HexagonGame.cpp:733` / `MenuGame.cpp:3061`).
- There is no `require`-based module system involvement: `require` still works (package
  survives sandboxing) but the corpus uses `u_execScript` / `u_execDependencyScript`
  for everything (11 and 138 uses respectively).

---

## 7. Which functions the corpus actually uses

Corpus = all 42 `.lua` files under `_RELEASE/Packs` (6 base Scripts + 36 level scripts).
Counts = `name(` occurrences in `.lua` files (includes both calls and any string
matches of the same shape; verified representative).

### 7.1 C++ API functions used (ranked)

| Count | Function |
|---|---|
| 218 | `t_wait` |
| 138 | `u_execDependencyScript` |
| 124 | `u_getDifficultyMult` |
| 111 | `u_rndInt` |
| 70 | `e_messageAddImportant` |
| 52 | `l_getLevelTime` |
| 44 | `steam_unlockAchievement` |
| 25 | `w_wall`, `u_getSpeedMultDM` |
| 18 | `e_messageAdd` |
| 14 | `u_log` |
| 13 | `t_eval`, `e_wait` |
| 11 | `u_execScript` |
| 10 | `ct_eval` |
| 9 | `shdr_setActiveFragmentShader`, `a_playSound` |
| 8 | `u_isFastSpinning` |
| 7 | `e_eval` |
| 6 | `u_setFlashEffect`, `s_setStyle`, `cw_setVertexColor4Same`, `ct_wait` |
| 5 | `u_getDelayMultDM` |
| 4 | `u_rndReal`, `l_enableRndSideChanges`, `e_stopTimeS`, `cw_setVertexPos4`, `cw_createDeadly`, `ct_create` |
| 3 | `e_waitS`, `cw_destroy`, `shdr_setUniformF` |
| 2 | `w_wallHModCurveData`, `u_setFlashColor`, `t_clear`, `shdr_getShaderId`, `l_setRotation`, `l_resetTime`, `l_getRotation`, `l_clearTracked`, `e_waitUntilS`, `e_kill`, `cw_moveVertexPos4Same`, `cw_getVertexPos`, `a_playPackSound` |
| 1 | `w_wallHModSpeedData`, `w_wallAdj`, `w_wallAcc`, `u_rndIntUpper`, `u_inMenu`, `u_getWidth`, `u_getVersionMinor`, `u_getVersionMicro`, `u_getVersionMajor`, `u_getPlayerAngle`, `u_getHeight`, `u_clearWalls`, `shdr_setUniformFVec4`, `shdr_setUniformFVec2`, `s_setCapColorMain`, `s_setCapColorByIndex`, `s_getColor`, `e_clearMessages`, `cw_setVertexPos`, `cw_getVertexPos4`, `cw_clear` |

Plus accessor-generated calls (heavy use in `onInit` setups and pattern code):
top 30 by count: `l_getSides` 114, `l_setSides` 48, `l_setRotationSpeedInc` 47,
`l_setRotationSpeed` 47, `l_setSpeedMult` 45, `l_setSpeedInc` 43, `l_setSidesMin` 43,
`l_setIncTime` 42, `l_setDelayMult` 38, `l_setSidesMax` 37, `l_setFastSpin` 35,
`l_setRotationSpeedMax` 34, `l_setDelayInc` 34, `l_setBeatPulseMax` 34,
`l_setBeatPulseDelayMax` 34, `l_setPulseSpeedR` 33, `l_setPulseSpeed` 33,
`l_setPulseMin` 33, `l_setPulseMax` 33, `l_setPulseDelayMax` 33, `l_setSpeedMax` 21,
`l_setSwapEnabled` 18, `l_setSwapCooldownMult` 13, `l_getRotationSpeed` 12,
`s_getHueInc` 9, `l_setWallSkewLeft` 7, `l_setBeatPulseSpeedMult` 7, `l_getSpeedMult` 5,
`l_setTutorialMode` 5, `l_setPulseInitialDelay` 5, … (tail down to 1: `l_getPulse`,
`l_setWallSkewRight`, `l_setWallAngleLeft`, `l_setShadersRequired`, `l_setRadiusMin`,
`l_set3dRequired`, `l_getSwapEnabled`, `l_getSpeedInc`, `l_getRotationSpeedInc`,
`s_setHueInc`, `s_set3dSkew`, `s_setPulseInc`, `s_setMaxSwapTime`, `l_setDelayMax`,
`l_setDarkenUnevenBackgroundChunk`, `l_getSpeedMax`, `l_getSidesMin`, `l_setIncEnabled`,
`l_setDelayMin`, `l_getDelayMult`).

Also: `math.random` (the redefined shim) is used **80** times in the corpus.

**Not used at all in the shipped corpus (0 occurrences):** every deprecated function
(`u_kill`, `u_eventKill`, `u_playSound`, `u_playPackSound`, `e_eventStopTime`,
`e_eventStopTimeS`, `e_eventWait`, `e_eventWaitS`, `e_eventWaitUntilS`, `m_messageAdd`,
`m_messageAddImportant`, `m_messageAddImportantSilent`, `m_clearMessages`,
`u_isKeyPressed`, `u_isMouseButtonPressed`), `l_forceSetPulse`, `l_forceSetBeatPulse`,
`l_overrideScore`, `l_getOfficial`, `u_swapPlayer`, `u_haltTime`, `u_forceIncrement`,
`u_setPlayerAngle`, `u_getAttemptRandomSeed`, `u_isHeadless`, `a_setMusic`,
`a_setMusicSegment`, `a_setMusicSeconds`, `a_syncMusicToDM`, `a_setMusicPitch`,
`a_overrideBeepSound`, `a_overrideIncrementSound`, `a_overrideSwapSound`,
`a_overrideDeathSound`, `shdr_getDependencyShaderId`,
`shdr_resetAllActiveFragmentShaders`, `shdr_resetActiveFragmentShader`,
`shdr_setUniformFVec3`, `shdr_setUniformI`, `shdr_setUniformIVec2`,
`shdr_setUniformIVec3`, `shdr_setUniformIVec4`. (Workshop levels may still use any of
these — the full API must be preserved.)

### 7.2 Corpus conventions observed

- Level scripts open with `u_execDependencyScript("ohvrvanilla", "base",
  "vittorio romeo", "utils.lua" | "common.lua" | "commonpatterns.lua" |
  "nextpatterns.lua" | "evolutionpatterns.lua")` (the base pack's disambiguator is
  `ohvrvanilla`).
- Standard callback set in level scripts: `onInit` (all 36 levels), `onLoad`,
  `onIncrement`, `onStep`, `onUnload`, `onUpdate`; selectively `onInput`
  (`babysteps`, `arcadia`), `onCursorSwap` (`evotutorial`, `bipolarity`),
  `onDeath` (`curvetest2`), `onPreDeath` (used in one level), `onRenderStage`
  (shader levels).
- Base Scripts are pure-Lua libraries (they define helper functions + the
  `RenderStage`/`KeyCode`/`MouseButton`/`Movement` enumerator tables +
  `CURVE_ROTATION_*` constants); they do not register any C++ globals.
- Patterns are built from `w_wall`/`w_wallAdj`/`w_wallAcc`/`w_wallHMod*` + `t_wait` +
  `u_rndInt` + `l_getSides`/`l_setSides`.

---

## 8. Top 10 subtleties for the Rust port

1. **Integer returns pushed as 64-bit, int getters declared `float` for RNG fns.**
   `u_rndInt`/`u_rndIntUpper`/`u_rndSwitch` are declared `-> float` but always yield
   integer-valued results (Lua sees them as numbers; `math.random(5)` must return an
   *integer* to old pack code doing `math.random(5) % 3` etc.). `u_getAttemptRandomSeed`
   returns a full 64-bit `ull` — Lua 5.4 integers are 64-bit, so no truncation; a port
   using 32-bit ints or floats here silently breaks seed-dependent logic.
2. **One shared PCG32 stream between Lua and C++ engine.** The engine consumes the same
   `rng` as `math.random`: per-frame `advance(pulse)`, `advance(pulse3D)`,
   `advance(fastSpin)`, `advance(flashEffect)`, `advance(rotationSpeed)`
   (`HGUpdate.cpp:353-357`) plus side-change `get_int` (`:301`). Any reordering of these
   draws/advances relative to Lua calls changes every subsequent `math.random` result
   and breaks replay validation. Menu context uses fixed seed 0.
3. **Sandbox is exact and order-sensitive.** After `luaL_openlibs`: `io.open` redefined
   (force read-only text), `math.random` redefined, *then* clearing of `os`, `debug`,
   `io.popen/flush/write/setvbuf`, `math.randomseed`, `package.loadlib` (twice),
   `package.searchpath`, `package.loaded.os`, `package.loaded.debug`, `package.loaders`.
   `require`/`package.path` must keep working (base pack modules rely on it in spirit;
   the corpus itself uses `u_exec*` instead).
4. **Explicit `nil` Lua args coerce, they don't error.** nil → int 0, float 0.0,
   bool false, but string ← `lua_tostring` = `nullptr` → `std::string(nullptr)` is
   UB (latent crash, no guard). Short calls DO raise the Lua error "this function
   requires at least N parameter(s)" in a clean call frame (`FunctionToPush::operator()`
   checks `lua_gettop` — which counts the whole stack, so a dirty stack can mask the
   check and then missing slots read as nil, §3.4); extra args are always silently
   ignored. Port should decide deliberately: replicate the lenient nil coercion, or
   fail — pack code may rely on the lenient behavior.
5. **Timeline fns are deferred, not immediate.** `t_*`, `e_*`, `ct_*`, and all `w_*`
   schedule lambdas onto timelines; `w_wall(...)` does NOT create a wall at call time —
   it captures `getSpeedMultDM()` (and `difficultyMult` for the `w_wallAcc` accel
   division `accel / DM^0.65`) **at execution time**, not at call time. `t_wait`/`e_wait`/
   `ct_wait` take *frames at 60 FPS* (1/60 s each), `*_waitS` take seconds,
   `*_waitUntilS` wait until `levelStartTP + int(duration*1000) ms` (int-ms truncation).
   `u_haltTime`/`e_stopTime` take FT (÷60 → seconds) despite docstrings saying "seconds".
6. **`firstPlay` gating on messages and death.** `e_messageAdd`/`m_messageAdd` only
   fire on the first run of a level; `e_messageAddImportant*`/`m_*` fire every run.
   `onPreUnload` only fires when `!firstPlay` (restart), and `onUnload` fires both on
   restart and on exit-to-menu (the latter only when not an error exit).
7. **`onInput` return value is a veto.** Returning `true` suppresses the default player
   movement/swap for that frame (`HGUpdate.cpp:244-251`); returning nil is fine
   (optional). Args are `(float ft, int movement ∈{-1,0,1}, bool focused, bool swap)`.
   `onRenderStage`'s `int` return is discarded — it is purely a side-effect hook.
8. **`l_overrideScore` executes concatenated Lua and requires a global variable name.**
   It string-splices the variable into `if (type(<var>) ~= "number") then error(...) end`
   — non-numeric → Lua error → `goToMenu(error)` (non-debug). Tracked variables
   (`l_addTracked`/`u_impl_addTrackedResult`) also read *global* Lua variables by name
   each frame (`readVariable`).
9. **Custom-wall and custom-timeline handles are opaque ints, invalid handles no-op.**
   `cw_*`/`ct_*` handle validation failures only log (no Lua error).
   `cw_getVertexPos`/`cw_getVertexPos4`/color getters return Lua multiple returns
   (tuples), so pack code writes `local x, y = cw_getVertexPos(h, 0)`.
10. **Two registration sites + menu stubs.** `LuaScripting::init` runs for BOTH menu
    and game; `MenuGame::initLua` then *overrides* `u_log`, stubs
    `u_getDifficultyMult`/`u_getSpeedMultDM`/`u_getDelayMultDM`/`u_getPlayerAngle` to
    constants, and no-ops ~45 "unused" functions (the stub list is an authoritative
    spelling checklist, and contains the legacy `u_timelineWait`). `u_inMenu()` is the
    way scripts distinguish the two. `steam_unlockAchievement` is a no-op in replays
    and only works with Steam + official mode.

---

## Validation checklist (for cross-checking the port)

- [ ] 285 registered function names match §2 list exactly (plus menu stubs §2 notes).
- [ ] Every accessor pair `l_get*/l_set*`, `s_get*/s_set*` uses the exact member
      spelling (§1.5.2 / §1.6.2), including `3dRequired`, `3dDepth`, `BGRotationOffset`,
      `HueInc`/`PulseInc` alias pairs (each alias registers BOTH spellings).
- [ ] `math.random(a,b)` ≡ `u_rndSwitch(2, a, b)` etc.; `math.randomseed` is nil.
- [ ] `io.open` read-only coercion; `os`/`debug` unavailable.
- [ ] Callback table §5: 12 callbacks, exact arg types; `onInput` veto semantics.
- [ ] `u_execScript` context-stack semantics (§6.2) incl. nested dependency resolution.
- [ ] RNG: pcg32_fast, closed-int `get_int`, `[0,1)` `get_real`, seed exposure,
      fixed menu seed 0, replay seed override, per-frame advances.
- [ ] Deferred-execution semantics of all `t_`/`e_`/`ct_`/`w_*` (§8.5).
