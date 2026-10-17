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

## Report

**Status: host work complete, gate green (795 tests = 777 baseline + 18 new); on-device verification PENDING (ADB down for the whole lane — see below). Commits `39ce69fb` (oh-assets + oh-data), `acfe3524` (oh-menu), `e1661891` (oh-app + oh-game).**

### The design decision (the crux) + why

The C++ has ONE `hg::HGAssets` (main.cpp:381/:434): the same object backs both the engine (`HexagonGame`) and the menu (`MenuGame`), so a score the engine's death path writes (`setLocalScore`, HexagonGame.cpp:871,908,915) is visible in the menu immediately, and the `mainClient` exit guard's `pSaveAll` persists it. The port had the opposite topology — the menu mirrored `current_profile` in a local `RefCell` (`RealMenuAssets`), the engine talked to `NullProfiles`, and the two could never agree.

**Decision: the profile state lives on `oh_assets::Assets` as a `Mutex<ProfileStore>` (the C++'s `profileDataMap` `BTreeMap` + `currentProfilePtr` as `current: Option<String>`), behind the EXISTING `Arc<Assets>`** — interior mutability, not a second shared store. Both the engine's `Profiles` seam (new `compose::RealProfiles`) and the menu's `RealMenuAssets` wrap the SAME `Arc<Assets>` and delegate every profile op to that one store. This is the direct translation of the C++'s single-object semantics: one object, one state, two observers.

Why not the alternatives:
- A *dedicated* shared `ProfileStore` held by both seams would duplicate the `Assets` constructor's `loadAllLocalProfiles` step AND the `addLocalProfile` invalid-favorite filter (which needs `levelDatas` — only `Assets` has it), so the store would have to be built into `Assets::new` anyway and then re-shared — strictly more plumbing for the same object.
- A per-consumer mirror (the pre-fix `RefCell`) is exactly the "disagree after a run" failed lane the spec forbids.
- `Mutex` (not `RefCell`) because `Arc<Assets>` must stay `Send + Sync` (the engine stores `Arc<dyn Profiles>` and is itself `Send + Sync`); the C++'s unsynchronized single-thread access generalizes to a short lock. The menu is single-threaded `Rc`; the engine's death path runs on the same windowed thread; headless is one thread — contention is a non-issue, the lock is there for `Sync`.

### The JSON shape determination (on-disk compatibility)

Verified against the vendored SSVUtils `ssvuj` (the C++ writes through it) and the shipped `_RELEASE/config.json`:

- **`pCreate` file is EXACTLY** `"\n{\n\t\"favorites\" : {},\n\t\"name\" : \"<n>\",\n\t\"scores\" : {}\n}\n"` — byte-for-byte pinned by a test. The C++ `pCreate` (Assets.cpp:1545-1561) archives `name` + `scores`/`favorites` = `ssvuj::Obj{}` and **no `version` key**. `ssvuj::Obj{}` is the fork's `ObjImpl` = `VecMap` (a sorted map) default-constructed **empty** — NOT a null — so the writer emits `{}`. Top-level members come out byte-sorted (`favorites` < `name` < `scores` — the fork's `std::map` storage), and the fork's `StyledStreamWriter` puts a comma after every member **but the last** (the shipped `config.json`'s visible shape; the port's fork-faithful `write_to_string` reproduces it).
- **`saveCurrentLocalProfile` / `saveAllProfiles` write the four-key shape** with `version` = the CURRENT `GAME_VERSION` (2.1.7 — the C++ archives the compile-time constant, Assets.cpp:1011-1014, **not** the loaded profile's version, which the C++'s `isObj("version")` load bug — LoadFromJson.cpp:40-44 — pins to -1.0.0 for every file it reads; the pre-existing `oh_data::profile_to_obj` is the *stored*-version builder and was deliberately NOT reused for the save path). `scores` is the `BTreeMap` (byte-sorted), `favorites` is the set as an array (the port `BTreeSet`'s byte-sorted order — the C++ `unordered_set` order is unspecified, so this is a documented, deterministic resolution).
- The `pCreate` file (empty `{}` scores/favorites, no version) is re-read by the C++'s own `loadProfileFromJson` (`scores`/`favorites` extract as empty; version → -1.0.0 by the bug) — so the port's byte-exact `pCreate` output is a genuine on-disk compatibility artifact, proven by the reload test.

### What was implemented

- **oh-assets** (`assets.rs`): `ProfileStore` + the full C++ write/state surface — `any_local_profile_active`, `current_local_profile_name`, `get_local_score` / `set_local_score`, `is_level_favorite`, `profile_save_current` (the no-current early return), `profile_save_all`, `profile_create` (write → clear → reload; the C++ `std::terminate()` on failed reload is the port's clean `Err`), `profile_remove` (map erase ONLY), `profile_set_current` (the C++ assert-on-missing is a no-op). The `::loadAssets` / `HGAssets::HGAssets` console lines are ported to the port's `[channel] message` convention. `get_profile_data` now clones (the guard is scope-local).
- **oh-data** (`json.rs`): `empty_obj()` (the C++ `ssvuj::Obj{}`).
- **oh-menu** (`seams.rs` / `real.rs` / `menu_game.rs`): `RealMenuAssets`'s local `current_profile` `RefCell` is **gone** — every profile op delegates to the shared store. `play_locally` now runs `profile_save_current()` FIRST (the old `profile_set_current("")` workaround — a divergence — is removed). The erase action now does the C++'s `std::remove` FIRST (new `MenuAssets::profile_remove_file`) and only on success runs `profile_remove` + item removal + beep; the failure branch logs the C++ `eraseAction()` line and returns. `FakeAssets` gained `profile_save_log` + `profile_remove_file` for the call-site tests.
- **oh-app** (`compose.rs` / `lifecycle.rs` / `client.rs` / `windowed.rs` / `dispatch.rs` / `print_lua_docs.rs` / `android.rs`): new `RealProfiles` (the engine seam over the shared `Arc<Assets>`); **every** shipping composition now passes it (the C++ builds the real `HGAssets` in all four entrypoints — windowed, headless client, server, print-lua-docs — `NullProfiles` remains only the test stand-in for the C++'s never-`pSetCurrent`ed state). New `ProfileSaveGuard` (main.cpp:382-388 verbatim: the two `::main` log lines + `profile_save_all`), installed in `main_client` (headless **and** windowed — the C++ `mainClient` guard is branch-agnostic) and in the Android `run` (the C++'s `mainClient` path); LIFO-before the config-save guard, exactly the C++ drop order. Server / print-lua-docs get the real store but NO guard (the C++ gives them none).
- **oh-game** (`seams.rs`): the `Profiles` / `NullProfiles` docs updated (the shipping seam is now `RealProfiles`).

### Deviations (documented in-code)

1. `pCreate`'s C++ `std::terminate()` on a failed reload → the port's clean `Err` (the `FatalLoadError` class; the menu logs the C++'s line and keeps running where the C++ process dies).
2. `pSetCurrent`'s C++ `SSVOH_ASSERT` on a missing name (release: UB) → a no-op (every C++ call site passes an existing name, so the assert never fires in the C++ either).
3. `pCreate`'s reload parse-errors are not appended to `LoadInfo` (the C++'s `MenuGame` copies `getLoadResults()` at boot, MenuGame.cpp:232 — unobservable there; only the `read_file_with_errors` `ssvuj::logReadError` console line fires on both, as in the C++).
4. The favorites array order is the port `BTreeSet`'s byte-sorted order (the C++ `unordered_set` order is unspecified).
5. `get_profile_data` clones the `ProfileData` (the `Mutex` guard is scope-local; the C++ returns a reference).

### Tests (18 new, all host)

- `oh-assets/tests/profiles.rs` (9): the `pCreate` on-disk shape **byte-for-byte** + reload through the port's own `load_all_local_profiles` (on-disk compatibility); the fresh-store `currentProfilePtr == nullptr` state (+ the -1.0.0 loaded-version bug); set-score → `pSaveCurrent` → reload round-trip (the 2.1.7 four-key shape); the favorites round-trip (a minimal fixture pack, since the `addLocalProfile` filter needs valid level ids); the no-current early return (file untouched); the missing-`Profiles/` `pCreate` `Err`; `pSaveAll`'s every-file rewrite; `pRemove`'s map-erase-only (the file survives); the `pSetCurrent` missing-name no-op.
- `oh-menu/tests/profiles.rs` (6): `play_locally`'s first-save over the current profile + the no-current early-return arm (both the `SLPSelectBoot` and `ETLPNewBoot` state choices); the erase guards (only-remaining / in-use) with the `error.ogg` / dialog / `ignore_inputs == 2` / nothing-removed assertions; the file→store→item→beep success order; the missing-file seam arm.
- `oh-app/tests/profile_store.rs` (2) — **the design-crux proof**: a score set through the engine's `Profiles` seam (`RealProfiles`) is read back through the menu's `MenuAssets` (`RealMenuAssets`) on the SAME `Arc<Assets>` with no sync step, and reaches disk through `profile_save_all`; + the both-sides `currentProfilePtr == nullptr` state.
- `oh-app` lifecycle `profile_save_guard_rewrites_profiles_at_drop`: the exit guard's persistence side effect (a post-load `set_local_score` reaches disk through the guard) + the disk-silent no-profiles arm (the headless determinism tests' guarantee).

Existing headless determinism tests remain disk-silent: they carry no `Profiles/` content, so the new `ProfileSaveGuard` (which only fires in `main_client` / Android `run`, never in the `oh-game` test composition) writes nothing, and `profile_save_all` over an empty store is a no-op.

### Device `cat` output

**PENDING — not run.** The phone's ADB listener was down for the whole lane: `adb connect 192.168.10.138:38721` → connection refused (the host still pings it). Per the orchestrator's instruction the device check is deferred to the batch when the phone is re-armed; **no temporary `debuggable = true` flag was added** (there was nothing to revert — the `run-as` step never ran). The on-disk proof will be the C++'s own shape: `run-as <pkg> cat files/Profiles/<name>.json` after creating a profile + a death, expected to show the four-key `version`/`name`/`scores`/`favorites` object with the death's `scores` entry.

### Residual risk

- The on-device save round-trip (create profile on the phone → play → die → `cat` the JSON) is UNVERIFIED on hardware; the host tests prove the byte-exact shape and the reload, but the Android internal-dir rooting (`base_dir.join("Profiles")`) is exercised only by the host temp-dir tests. Low risk — the same `Assets::root()` path the C++ uses CWD-relative, and the folder is created by the existing `create_folder_if_non_existant` lifecycle call.
- `pCreate`'s `std::terminate()` → clean-`Err` deviation: in practice the reload only fails when `Profiles/` is absent (which the lifecycle guard creates), so the `Err` arm is unreachable in the shipped binary.

### What I did NOT do (out of scope, as instructed)

- The **new-profile menu item's** dynamic add/remove into the selection category (the C++ `profiles.create<Single>(...)` at MenuGame.cpp:2238-2245) — that is **29b6**. `create_profile_from_entered` calls the real `profile_create`/`profile_set_current` (so the store is correct), but the menu's item list still reflects boot-time `profile_names()`.
- The **scroll machinery** (29b5) — untouched.
- The **F1 favorite TOGGLE** (`getCurrentLocalProfile().addFavoriteLevel`/`removeFavoriteLevel`, MenuGame.cpp:4843-4895) — the *read* half (`isLevelFavorite`, now backed by the real store) is done; the *write* half (the toggle) is a menu-lane item, so no test can currently drive a non-empty favorites set through the public API (the favorites round-trip test instead seeds a C++-shaped file with valid level ids and proves save→reload).
- **On-device verification** (above) — PENDING.
