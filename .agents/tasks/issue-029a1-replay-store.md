# Task 029a1 — replays are never written (the audit's top finding)

Lane 029a1 from the issue-29 module-parity audit (`game` pair). Master green
(gate 4/4, 750 tests) at `142aa16f`. **No worktree** (D-035): main checkout;
commit every 3-5 tool calls.

## The finding (audit M2, orchestrator-verified)

`HexagonGame::death_saveReplay` (`HexagonGame.cpp:1067-1097`) writes the
compressed replay to disk:

```cpp
create_directories(p.parent_path());          // :1086
crf.serialize_to_file(p);                     // :1089  -> Replays/<levelId>/<diff>x/<name>.z
ssvu::lo("Replay") << "Successfully saved new compressed replay file ...";  // :1093
```

The port delegates this to the `ReplayStore` seam
(`crates/oh-game/src/seams.rs:120-138`), but the ONLY implementation is
`NullReplayStore`, whose `save` returns `true` and writes **nothing** — and
the composition wires exactly that (`crates/oh-app/src/compose.rs:526`).

**Consequence: no run ever persists a replay file.** The replay *format* is
byte-exact and oracle-pinned (issue 02), the *writer* exists
(`CompressedReplayFile` serialization), and the *call site* exists — but the
filesystem half is a stub, so the game's flagship feature silently produces
nothing. This is a `data-format` risk: a user finishes a run, is told the
replay was saved, and there is no file.

Verify all of the above yourself first (grep the impls, read `compose.rs`,
read `death_save_replay`/`death_send_and_save_replay` in
`hexagon_game.rs:1288-1313`).

## What to do

1. **Implement the real store** — a `RealReplayStore` (name it as you like)
   in the crate that owns the filesystem half (`oh-game`'s seam is the
   interface; the impl may live in `oh-app`/a small module — justify the
   placement and follow how the other real seams are structured, e.g. the
   profiles/asset stores in `oh-app`).
   - Mirror the C++ EXACTLY: `create_directories(parent)` (note: the C++
     uses `std::filesystem::create_directories`, which succeeds if it
     already exists), the path construction
     (`Replays/<levelId>/<difficultyMult>x/<name>.z` — read the C++ for the
     exact naming rules, including the `difficultyMult` formatting; the
     oracle-pinned replay corpus and `mapping/` docs may already state it),
     the serialized bytes (`CompressedReplayFile` → the same zlib-level-9
     path the C++ uses, per D-006), and the success/failure return.
   - Handle the same failures the C++ does (a write failure must produce
     the C++'s "Failure saving replay" log, not a panic) and keep the
     engine's existing log lines' SEMANTICS (and add the two missing ones
     M6/M7 only if trivial — they are separate, low-risk; do not expand
     scope beyond a correct, C++-faithful save path).
2. **Wire it in `compose.rs`** in place of `NullReplayStore`, keeping the
   seam (headless/server paths that must not touch the disk stay on a
   no-op — check what the C++ does in those modes and match it; the
   existing headless determinism tests must not start writing files: run
   them and confirm, and if a test needs an isolated dir, use the existing
   tempfile pattern rather than a global).
3. **Test it** — host tests that prove a real file appears with the
   expected path + bytes: (a) the store writes a file whose CONTENTS
   round-trip through the existing replay parse path, (b) the directory
   creation is idempotent (a second save to the same dir succeeds), (c) a
   failure path returns false without panicking.
4. **Verify on the device** (this is the point): play a level on the phone
   (adb `192.168.10.138:38721`; wake with `KEYCODE_WAKEUP`; `pm clear` for
   a fresh install), die, return to the menu, and show the replay EXISTS
   on disk — `adb shell run-as info.vittorioromeo.openhexagon ls -R files/Replays`
   (the release build is not debuggable; if `run-as` is unavailable, copy
   the file out via a path you CAN read, or enable `debuggable = true` in
   metadata TEMPORARILY and revert it before the final commit — say which
   you did). Then confirm the file's size/bytes look sane and, if the menu
   offers replay playback, that it plays (the C++ "REPLAYS" flow).
5. **Register row** (issue 28's rule): the fixed deviation gets its row in
   the same commit; update the audit's `game` pair note if appropriate
   (`.agents/audits/game.md` is the audit lane's file — you may add a
   one-line STATUS marker at the top, do not rewrite it).
6. **Report** (short): the C++ citations you implemented, the impl's
   placement + why, the tests, the DEVICE evidence (the `ls`/bytes), and
   residual risk. Note explicitly whether headless/server stay
   disk-silent.

## Hygiene

- Commit every 3-5 tool calls. No worktrees. No emulator. Never touch
  non-phone processes.
- The issue-29 audit workflow is running READ-ONLY in the background; it
  writes only `.agents/audits/*.md` — do not edit that directory beyond
  the one-line status marker, and do not commit its files.
- Keep the host gate green (`tools/gate.sh`, 750 tests).

## Report (completed 2026-10-17 — on-device: moto g power 2021 / SD439 / Adreno 610, Android 11, 1600×720, adb `192.168.10.138:38721`)

**Implementation** (commits `14167fc6` + `7196f522`, spec'd "seam in `oh-app`"): `crates/oh-app/src/replay_store.rs` — `RealReplayStore { base_dir: PathBuf }`, the `ReplayStore` impl: `save` = `std::fs::create_dir_all` (the C++ `std::filesystem::create_directories`, HexagonGame.cpp:1086 — the return unchecked in the C++, idempotent equivalent) → `oh_replay::CompressedReplayFile::serialize_to_file` (the same raw compressed-bytes write as C++ Replay.cpp:437-445; the compression itself — zlib level 9, the D-006 system-zlib pin, oracle-pinned by the CZHEX vectors — already ran in the engine's `death_send_and_save_replay`) → the `Replay`-channel log lines (the C++'s HexagonGame.cpp:1093 "Successfully saved new compressed replay file" — missing per audit M7 — and the :1087 failure line). Placement: the `ReplayStore` TRAIT lives in `oh-game` beside the death flow that calls it (`crates/oh-game/src/seams.rs`); the IMPL lives in `oh-app` because the composition owns the app's `base_dir` — the C++'s CWD at save time. The C++ writes `Replays/...` relative to the process CWD; both windowed entry points chdir to the game-data root before composing (the desktop `dispatch::run`'s main.cpp:653 port + Android's `chdir_to_base_dir`), so the CWD-relative pieces are byte-identical to the C++ in production.

**Wiring**: the windowed composition (`windowed.rs` `WindowedClient::build`) — the only composition that runs a windowed game — gets `RealReplayStore::new(base_dir)` in place of `NullReplayStore`. The headless client / server / print-lua-docs compositions keep `NullReplayStore`: the C++ headless modes never write a replay file (`inReplay()` gates the death-save at HexagonGame.cpp:970; `shouldSaveScore` rejects at :1178-1183/:1220-1227; the server's executor is the same `runReplayUntilDeathAndGetScore` path, and the headless determinism tests must not start writing files).

**Host tests** (3 new, `replay_store.rs`; gate 753 total): `save_writes_the_file_and_round_trips` (the file lands at the C++-shaped `Replays/<levelId>/<diff>x/<name>.z` path and its CONTENTS round-trip through the existing `deserialize_from_file` + `decompress_replay_file` parse path — i.e. a real, valid compressed replay, not just a file), `save_into_an_existing_dir_succeeds` (idempotent dir creation), `save_reports_failure_without_panicking` (the C++ failure branch returns false). fmt + clippy clean.

**On-device proof** (text-only per the 029a1-completion spec — zero screenshots; `debuggable=true` re-added TEMPORARILY for `run-as` only — reverted in commit `2ca6659d`, the shipped config stays non-debuggable). Release APK, `pm clear` clean slate, shipped default config (windowed, offline, vsync off / 200 fps cap). Flow: fresh boot → profile name entered (the profile editor is the documented issue-005 no-op — in-memory menu state only) → Level Selection → the default tutorial pack was UNPLAYABLE on this device (the `babysteps` LuaJIT hang below — K3) → pack-folded to the cube pack via the bottom-band touch → started `euclideanpc` → death at 1.85 s → game-over screen.

Evidence, verbatim:

1. logcat (tag `RustStdoutStderr` = the port's stdout→logcat pipe; app pid 11483, 22:29:19 device time — the device clock runs ~5 h behind UTC, which is why logcat stamps read 10-16):
```
10-16 22:29:19.939 11483 11510 I RustStdoutStderr: [Replay] Attempting to send and save replay...
10-16 22:29:19.939 11483 11510 I RustStdoutStderr: [Replay] Failure sending replay
10-16 22:29:19.942 11483 11510 I RustStdoutStderr: [Replay] Successfully saved new compressed replay file 'Replays/ohvrvanilla_vittorio_romeo_cube_1_euclideanpc/1x/0_20261017_022919_no_profile_ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_1x_1.85417s.ohr.z'
```
("Failure sending replay" = offline, no client — the C++ `death_sendReplay`'s no-client branch, HexagonGame.cpp:1034-1043 path; the save proceeds regardless, C++-faithful. The same death also logged the C++-identical `[hg::HexagonGame::shouldSaveScore()] No local profile active, rejecting`.)

2. `run-as info.vittorioromeo.openhexagon find /data/user/0/info.vittorioromeo.openhexagon/files -name '*.z'`:
```
/data/user/0/info.vittorioromeo.openhexagon/files/Replays/ohvrvanilla_vittorio_romeo_cube_1_euclideanpc/1x/0_20261017_022919_no_profile_ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_1x_1.85417s.ohr.z
```

3. `run-as ... ls -lR /data/user/0/info.vittorioromeo.openhexagon/files/Replays`:
```
Replays/ohvrvanilla_vittorio_romeo_cube_1_euclideanpc/1x:
-rw------- 1 u0_a324 u0_a324 98 2026-10-16 22:29 0_20261017_022919_no_profile_ohvrvanilla_vittorio_romeo_cube_1_euclideanpc_1x_1.85417s.ohr.z
```

Path-shape audit against the C++ `death_sendAndSaveReplay` pieces (HexagonGame.cpp:1034-1043, 256-261): `Replays/` + full level id `ohvrvanilla_vittorio_romeo_cube_1_euclideanpc` + `/` + `diffFormat(difficultyMult)` = `1` → `1x/` (the menu's `current_level` cache was None after the pack fold → the C++-faithful `map_or(1.0, ...)` default, `menu_game.rs:699-703`) + `<name>.z` with `<name>` = `0_20261017_022919_no_profile_<levelId>_<diff>x_<played_seconds>s.ohr` — `no_profile` (no local profile active — the documented no-op profile system; the C++ default name) and `1.85417s` (played score / 60).

**Register**: `.agents/plans/001-rust-port/deviations.md` — **F5** filed (this fix + citations + this evidence). Also **K3** filed (NEW deviation diagnosed in this session, not fixed): the `babysteps` LuaJIT trace-compilation hang (see residual risk).

**Probes / temp changes**: `debuggable = true` in `crates/oh-android/Cargo.toml` (`[package.metadata.android.application]`) — reverted in commit `2ca6659d` (the block itself is pre-existing from the issue-25 lane; only the value toggled). No other temp changes; no `.agents/audits/` files touched or committed (the issue-29 audit's untracked outputs stay untracked).

**Host gate**: fmt clean; 753/753 (750 baseline + the 3 `replay_store` tests); clippy clean; `cargo check --target aarch64-linux-android` clean (the on-device APK was built from this same source with the temporary debuggable flag — code paths otherwise identical).

**Residual risk**:
- **K3 (new)**: on this device, the tutorial pack's `babysteps` level (the menu's default first level) pegs `android_main` ~100% user CPU in LuaJIT trace compilation (`lj_asm_trace` → `__clear_cache` in `call_on_update`) for 85+ s — three independent runs 2026-10-17, two ANRs (dropbox 22:08:57 / 22:19:23, "Waited 5004 ms for MotionEvent"). Cube-pack levels (incl. `euclideanpc`, empty-`onUpdate` `commando`/`labyrinth`) play fine the same day, and the 025j 35-min session had zero ANRs — so it is level-specific or ASLR-dependent, and it is NOT a replay-store defect (the save fired and wrote the file before any input was needed: `euclideanpc`'s start is lethal at 1×, the player died with zero input at 1.85 s). Root cause open (port LuaJIT × device interaction vs a level-script property; the C++ on this device is untested) — needs its own lane (trace capture, mitigations incl. the K2 fps cap, which feeds `onUpdate` at 2-3× the C++'s paced rate).
- The on-device file's BYTES were not pulled to the host: the device's adb-over-TCP listener went silent at the end of the session (phone still answers ping; both adb ports closed; `adb tcpip` not re-armable without the device screen). Existence + size (98 B) + name are proven; byte-level validity is covered by the host round-trip test exercising the same `serialize_to_file` code path. If the device regains adb, `run-as ... cat` + a host zlib-inflate is the closing check.
- Replay PLAYBACK: untested on-device this session. The port has a headless replay-execution path that consumes these files (`windowed.rs` "Playing compressed replay file", the server's `runReplayUntilDeathAndGetScore` equivalent), and the file format round-trips in host tests — the on-device file came from the same code path, so playback of it is expected to work; not demonstrated on-device.
