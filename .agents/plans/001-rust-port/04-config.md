# Issue 04 — `oh-config`: every config key, binds, joystick

## Objective

Port `hg::Config` (all keys, load/save/apply, resolution handling, overrides),
`BindControl`, and `Joystick` so the existing `config.json` and
`ConfigOverrides/*.json` drive identical settings and are written back byte-
compatibly (key names and value types).

## Key decisions

- Mirror `include/SSVOpenHexagon/Global/Config.hpp` + `src/.../Config.cpp`,
  `Core/BindControl.*`, `Core/Joystick.*`.
- Config is a global mutable singleton in C++ (`namespace hg::Config` with free
  functions). In Rust: a `Config` struct behind a `OnceLock`/`RwLock` with the same
  getter/setter surface, or a plain struct threaded explicitly — **the simplest
  1:1 option is a global accessor matching the C++ free-function names** (snake_case).
  Document the choice.
- `TICKS_PER_SECOND = 240.f`, `TIME_STEP = 60.f / 240.f`, `TIME_SLICE` — copy exactly.
- Trigger/bind model: mirror `ssvs::Input::Trigger` / `Combo` (list of combos;
  each combo a set of keys/buttons/mouse). Key/button enum values (`KKey`, `MBtn`)
  are **part of the serialized format** — copy the exact integer values from
  `SSVStart/Input/Enums.hpp`.

## Files

| File | Change |
|---|---|
| `crates/oh-config/Cargo.toml` | new — deps `oh-data`, `serde_json` |
| `crates/oh-config/src/{config,binds,joystick,triggers}.rs` | new |
| `crates/oh-config/tests/config_release.rs` | new — load the real `_RELEASE/config.json` |
| `.agents/skills/lib-binds.md` | new — the trigger/combo/key-enum model from real source |

## Steps

1. Port every getter/setter pair (the full list is in `Config.hpp`) and the
   backing fields with their defaults.
2. Port `loadConfig` (read `config.json`, apply `ConfigOverrides/<id>.json` on top),
   `saveConfig` (exact key names + formatting), `resetConfigToDefaults`,
   `resetBindsToDefaults`, `isEligibleForScore`, `recalculateSizes`,
   resolution/vsync/fps/antialiasing setters.
3. Port the keyboard bind table: `Tid` enum, `triggerGetters`, `rebindTrigger`,
   `clearTriggerBind`, `getKeyboardBindNames`, `keyboardBindsSanityCheck`.
4. Port the joystick bind table: `Joystick::Jid`, getters/setters, `loadAllJoystickBinds`,
   `joystickBindsSanityCheck`, `getJoystickBindName`, deadzone.
5. Port `getTrigger(Tid)` and trigger refresh plumbing used by menus/game.

## Verification

- Load `_RELEASE/config.json`; assert a sample of exact values across types
  (`3D_enabled=true`, `max_fps=200`, `player_speed≈9.45`, `t_main[0][0]="kSpace"`,
  the full `server_level_whitelist` length).
- Load each `_RELEASE/ConfigOverrides/*.json`; assert the override changes exactly
  the keys it names and nothing else.
- Save → reload round-trip: assert the re-parsed struct equals the loaded one, and
  diff the emitted JSON's key set against the C++-written file's key set.
- Bind sanity checks pass on the real config (no duplicate/empty triggers).
- `tools/gate.sh` green.

## Notes / risks

- The C++ `saveConfig` ordering/formatting is produced by a JSON writer; if exact
  byte-equality is impractical, pin **key-name + value + type** equality and record
  the deviation.
- `Config::getUneligibilityReason()` strings are user-visible; copy verbatim.
- Resolution helpers depend on the window backend (issue 10) — leave the seam
  (`applyResolution(&mut Window)`) and keep pure math here.
