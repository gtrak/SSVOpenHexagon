# Task spec — issue 004: `oh-config` (all keys, binds, joystick)

You are a worker. Implement exactly this issue. Do not redesign, do not widen scope.

## Context

Repo: `/home/gary/dev/SSVOpenHexagon`. Literal 1:1 Rust port. Issues 001
(workspace/`oh-math`/`oh-rng`), 002 (`oh-replay`), 003 (`oh-data`) are LANDED.

Read first:
1. `.agents/plans/001-rust-port/PLAN.md`, `DECISIONS.md`
2. `.agents/plans/001-rust-port/04-config.md` (the contract)
3. `.agents/plans/001-rust-port/mapping/data-json.md` §6 (the complete config-key
   table), §8 (Joystick + BindControl), §9 (on-disk layout), §10 (subtleties)
4. `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — the SFML key/button
   enum values (the bind model depends on them)
5. `include/SSVOpenHexagon/Global/Config.hpp` + `src/.../Config.cpp`,
   `Core/BindControl.*`, `Core/Joystick.*`
6. `.agents/skills/README.md`

## Working agreement

- **Skills are truth.** Write `.agents/skills/lib-binds.md` from the real SSVStart
   `Input/Trigger.hpp`, `Combo.hpp`, `Enums.hpp` + this repo's bind code.
- **Write first**, compile early.
- **Exactness**: `TICKS_PER_SECOND = 240.f`, `TIME_STEP = 60.f / 240.f`; every key's
   JSON name/type/default; the key/button enum integer values from the oracle doc.
- **Commit at the first green build**.

## What to build

1. The `Config` struct + every getter/setter pair (the full list is in `Config.hpp`;
   re-derive it from source — the mapping doc's table is a claim, confirm it).
2. `loadConfig(overrideIds)` (read `config.json`, apply `ConfigOverrides/<id>.json`
   shallowly on top), `saveConfig` (exact key names), `resetConfigToDefaults`,
   `resetBindsToDefaults`, `isEligibleForScore`, `recalculateSizes`,
   resolution/vsync/fps/antialiasing setters.
3. The keyboard bind table (`Tid`, trigger getters, `rebindTrigger`,
   `clearTriggerBind`, `getKeyboardBindNames`, `keyboardBindsSanityCheck`).
4. The joystick bind table (`Joystick::Jid`, getters/setters, `loadAllJoystickBinds`,
   `joystickBindsSanityCheck`, `getJoystickBindName`, deadzone).
5. `getTrigger(Tid)` + the trigger-refresh plumbing.

## Verification

1. `bash tools/gate.sh` — paste all `GATE_EXIT=` lines.
2. Load the real `_RELEASE/config.json`; assert exact values across types
   (`3D_enabled=true`, `max_fps=200`, `player_speed≈9.45`, the full
   `server_level_whitelist` length, `t_main[0][0]="kSpace"`-style binds).
3. Load each `_RELEASE/ConfigOverrides/*.json`; assert the override changes exactly
   the keys it names.
4. Save → reload round-trip: assert the re-parsed struct equals the loaded one.
5. Bind sanity checks pass on the real config.

## Report format

Module map, design summary, gate outputs, skill corrections, deviations, known gaps.
