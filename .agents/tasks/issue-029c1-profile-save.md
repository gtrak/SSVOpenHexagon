# Task 029c1 — the profile/save system is a no-op in the shipping binary

The audit's deepest finding (**12 pairs audited, 10 FAIL**; this one confirmed
independently by BOTH the `game` and `assets` pairs). Master green: gate 4/4,
753 tests. **No worktree** (D-035): main checkout, commit every 3-5 tool calls.

## The finding (orchestrator-verified, citations re-derived from the C++)

The C++ persists the player's data. The port does not:

| C++ | Rust today |
|---|---|
| `saveCurrentLocalProfile()` `Assets.cpp:1001-1025` — writes version/name/scores/favorites to `getCurrentLocalProfileFilePath()` | **absent** |
| `saveAllProfiles()` `Assets.cpp:1027-1058` — rewrites every profile | **absent** |
| `pSaveAll()` → called at exit, `Core/main.cpp:382-388` (scope guard, logs *"Saving all local profiles..."* / *"Done saving all local profiles"*) | **absent** |
| `pSetCurrent` `:1537-1550`, `pCreate` `:1552-1568`, `pRemove` `:1563-1566` | **absent** |
| `getLocalScore`/`setLocalScore` `:1446-1454`, `anyLocalProfileActive` `:1459` | `NullProfiles` no-ops (`oh-game/src/seams.rs:105-118`), **wired into the shipping binary** (`compose.rs:454-456`: *"the binary passes NullProfiles — the C++ `HGAssets` profile surface's mutation API is not ported yet"*) |
| `MenuAssets::profile_create/remove/set_current` | no-ops (`oh-menu/src/real.rs:170-171`) |
| `is_level_favorite` | hardcoded `false` (`real.rs:200`) |

`oh-assets` has the **read** half (`assets.rs:464 load_all_local_profiles`,
`:482 add_local_profile`, `:675 get_profile_data`, `:680/687` counts/names) and
defers the write half (module doc `assets.rs:16`). Consequence: **no profile can
be created, no score is ever recorded or persisted, no favorite can change, and
every personal best reads 0** — the whole save system is inert.

## What to implement

### 1. `oh-assets` — the write/state surface (read the C++, match it exactly)
- `save_current_local_profile()` — `Assets.cpp:1001-1025`: early-return when no
  current profile; archive `version{major,minor,micro}` (from the port's
  `GAME_VERSION`), `name`, `scores`, `favorites` (the **set** → array); write to
  `getCurrentLocalProfileFilePath()`. Find the C++'s path helper and mirror it.
- `save_all_profiles()` — `:1027-1058`: for every profile in the map (C++
  `std::map` = name-sorted; the port already uses `BTreeMap`, so this matches)
  write the same four keys to that profile's file.
- `p_create(name)` — `:1552-1568`: write `Profiles/<name>.json`, then
  `profile_data_map.clear()` + `load_all_local_profiles()` (the C++ does NOT
  terminate on failure — check the code and match; note the C++ writes **no
  `version` key** here, and its empty `scores`/`favorites` values are
  `ssvuj::Obj{}` — determine what that emits for `jsoncpp` and match it
  *exactly*, because these files are re-read by the C++: this is an on-disk
  compatibility artifact).
- `p_remove(name)` — `:1563-1566`: map erase ONLY. **The file deletion is the
  MENU's job** (`MenuGame.cpp:2419-2431`: `std::remove("Profiles/"+name+".json")`
  and only on success `assets.pRemove(name)`), with two guards before it
  (`:2398-2417`): refuse when it is the only remaining profile, and refuse when
  it is the currently-used one (both `error.ogg` + a dialog + `setIgnoreAllInputs(2)`).
- `p_set_current(name)` — `:1537-1550`: locate (the C++ `SSVOH_ASSERT`s it
  exists) and set the current profile.
- `get_local_score`/`set_local_score` (current profile; `:1446-1454`),
  `any_local_profile_active`, `get_current_local_profile`.

### 2. ONE source of truth (this is the design crux — do not skip)
The C++ has a **single** `HGAssets`; the menu and the engine observe the same
profile state, so a score set during play is immediately visible in the menu.
In the port the menu talks to `MenuAssets` (over `RealMenuAssets`/`oh-assets`)
while the engine talks to the `Profiles` seam. **Both must be backed by the same
store.** Pick a design (e.g. interior mutability inside `oh-assets` behind the
existing `Arc`, or a dedicated shared profile store that both seams hold) and
justify it against the C++'s single-object semantics. A design where the two
disagree after a run is a failed lane.

### 3. Wire it up
- `RealMenuAssets`: implement `profile_create`/`profile_remove`/
  `profile_set_current`/`is_level_favorite`/`current_profile_name` over the real
  store (no more no-ops; `is_level_favorite` must consult the current profile's
  favorite set).
- The engine's `Profiles` seam: a real impl in the shipping composition instead
  of `NullProfiles` (`compose.rs:454-456` + the `windowed` path). Headless
  modes: **check what the C++ does** and match (the C++ still constructs
  `HGAssets` headless and still saves at exit — verify before you keep
  `NullProfiles` anywhere).
- Menu call sites: `playLocally` saves first (`MenuGame.cpp:2427-2432`:
  `assets.pSaveCurrent()` then the profile-select/new-boot state choice);
  `create_profile_from_entered` calls the real create; the erase action does the
  two guards + `std::remove` + `p_remove`.
- Exit: mirror `Core/main.cpp:382-388`'s scope guard (save all + the two log
  lines) on the port's shutdown path, including headless.
- The PB/death path already calls `set_local_score` through the seam
  (`HexagonGame.cpp:871,908,915`) — with a real impl those PBs persist. Verify
  the score actually reaches the file.

### 4. Tests (host)
- `p_create` then reload → the profile exists with the C++'s exact JSON shape
  (assert the bytes/keys, and that a profile created by the port parses back
  through the port's own `load_all_local_profiles`).
- set score → `save_current_local_profile` → reload → score present.
- favorites round-trip through save/reload.
- the "no current profile" early return; the missing-`Profiles/` dir case.
- the erase guards (only profile / in-use profile).
- Keep the existing headless determinism tests free of disk writes (or in a temp
  dir) — run them and prove it.

### 5. On-device verification (the point of the lane)
Create a profile on the phone, play a level, die, and show **text evidence**
that the score is on disk: `run-as ... cat files/Profiles/<name>.json` (the
release build is not debuggable — `run-as` needs a temporary `debuggable = true`
in `crates/oh-android/Cargo.toml`, which you MUST revert in a commit before you
finish, and say so — see how 029a1-completion did it). Device:
`adb -s 192.168.10.138:38721` (wake with `KEYCODE_WAKEUP` first).

**HARD RULE: at most ONE screenshot read for the whole task.** Prefer
`logcat -d | grep`, `find`, `ls`, `cat`. Two lanes have already died in the
image-read spiral. If a device session spins with no progress, STOP and report.

### 6. Register + report
- A row in `.agents/plans/001-rust-port/deviations.md` (issue 28's rule): the
  profile surface was a no-op seam (read half ported, write half deferred, seam
  wired to `NullProfiles`) — fixed, with the C++ citations.
- Append `## Report` to this task file: the design decision + why, the JSON
  shape determination, the tests, the device `cat` output, residual risk, and
  explicitly what you did NOT do (e.g. the new-profile menu item is 29b6; the
  scroll machinery is 29b5 — do not expand into those).

## Hygiene
- Keep the gate green: `bash tools/gate.sh` (753 tests + yours).
- No emulator; never touch non-phone processes.
- `.agents/audits/*.md` are committed audit reports — do not rewrite them.
