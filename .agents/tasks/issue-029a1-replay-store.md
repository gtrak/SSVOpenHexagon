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
