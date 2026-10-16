# The Rust release — what ships, how to run it

Issue 24 (`.agents/plans/001-rust-port/24-packaging.md`): a distributable
build that runs against (or is dropped into) the existing C++ `_RELEASE`
assets, matching the C++ layout so players can keep their install.

Build it with `bash tools/package.sh` (Linux) or `bash tools/package.sh
--windows` (the `x86_64-pc-windows-gnu` cross build via
`tools/build-windows.sh --build --release`). The staged directory is
`target/package/<platform>/_RELEASE`.

## What ships

The staged `_RELEASE/` matches the C++ install layout — the BINARY
ALONGSIDE `Assets/`, `Packs/`, `ConfigOverrides/`, `config.json`:

```
_RELEASE/
├── open-hexagon        # the game (Linux; open-hexagon.exe on Windows)
├── Assets/             # copied from the tracked _RELEASE (fonts, sounds,
│                       # textures, assets.json — unchanged C++ data)
├── Packs/              # the level packs (all 8 tracked packs, incl. cube)
├── ConfigOverrides/    # the config override presets (debug/fullscreen/…)
└── config.json         # the game config the binary loads on boot
```

The C++ layout this mirrors, cited from this tree:

- The tracked `_RELEASE/` is the C++ install root: the game data
  (`Assets/`, `Packs/`, `ConfigOverrides/`, `config.json`), the
  Steam/Discord natives, the launch scripts, and — added by the C++ CI
  (`.github/workflows/ci_linux.yml`: `cp build/SSVOpenHexagon … _RELEASE`)
  — the binary itself, gitignored (`_RELEASE/SSVOpenHexagon*` in
  .gitignore).
- `prepare_release.sh` / `prepare_release_linux.sh` (the C++'s own
  release assemblers) copy exactly this data set — `Assets/`,
  `ConfigOverrides/`, `Packs/`, `config`-adjacent files — plus the
  binaries and the platform natives into one directory; the Linux
  launcher (`run_ssvopenhexagon_linux.sh`:
  `LD_LIBRARY_PATH="." ./SSVOpenHexagonLinux`) confirms the natives are
  meant to sit BESIDE the binary in `_RELEASE/`.
- `Profiles/` and `Replays/` are runtime-created by the binary on first
  run (main.cpp:275-276, ported in `oh-app/src/client.rs`) — the package
  does not pre-create them, and neither does a fresh C++ install.

Deliberate deviations from the C++ prepared releases:

- **No Steam/Discord natives in the default build.** The default Rust
  build never links them (below). `--with-steam` stages the Steam
  natives (`libsteam_api.so` / `steam_api64.dll` + `steam_appid.txt`,
  the same files the C++ links) because the linked binary cannot start
  without the loader stub (`.agents/skills/lib-steamworks.md` §4).
- No `.bat` launchers / launcher script (they wrap C++ binary args; the
  Rust binary takes the same CLI — below).
- No `scores.json` / `users.json` seeds (runtime user data; the C++
  prepared releases do not ship them either).
- All 8 tracked packs ship (the C++ assemblers curate 5 — a curation
  choice, not a layout one; the extras are additive).
- `oh-server-control` and `oh-workshop-uploader` (issues 18-19) are
  built by the same release build but are NOT part of the game-install
  layout; they live in `target/<...>/release/`.

## How to run

```
cd _RELEASE && ./open-hexagon            # Linux (Windows: open-hexagon.exe)
```

No `LD_LIBRARY_PATH` needed for the default build (it links no external
game natives). The `--with-steam` build needs the C++ launcher pattern
(`run_ssvopenhexagon_linux.sh`): `cd _RELEASE && LD_LIBRARY_PATH=. ./open-hexagon`
(the staged loader stub sits beside the binary; the plain NEEDED entry
does not search the binary's own directory). The CLI is the C++'s `parseArgs` (main.cpp:80-154), ported
verbatim in issue 15b:

| Flag | Meaning |
|---|---|
| (none) | windowed game (needs a display; a headless machine degrades to the null-target loop) |
| `-headless [replay.ohr.z]` | headless replay run: `Player died.` + `Final time: <score>`; exit 1 without a replay file |
| `-p <pack> -l <level>` | boot into the windowed level selection with that level selected |
| `-server` | the online server (issue 18) |
| `-printLuaDocs` | the Lua API docs (`produce-lua-metadata` feature — see below) |
| any arg containing `.ohr.z` | loaded as a drag-&-drop compressed replay (windowed) |

Replays: drop a `<name>.ohr.z` into `_RELEASE/` (or pass its path to
`-headless`). The committed smoke-test fixture
(`test/fixtures/cube_1_apeirogon.ohr.z` — a repo-level test input, NOT
shipped in the package) is a valid example:

```
cp test/fixtures/cube_1_apeirogon.ohr.z _RELEASE/ && cd _RELEASE
./open-hexagon -headless cube_1_apeirogon.ohr.z
# Player died.
# Final time: 1.725
```

## Feature-flag matrix

The packaging script's flags, and the cargo features behind them:

| package.sh flag | cargo feature | what it changes | what ships |
|---|---|---|---|
| (default) | — | the C++ no-SDK build analogue (issue 20): no Steam native linked; the in-game Steam surface degrades to logged no-ops | binary + data only |
| `--with-steam` (Linux only for now — the cross path's fixed cargo line is feature-OFF) | `oh-app/steam` | links the Steam loader stub via `steamworks-sys`; degrades at runtime when no Steam client is running (the C++ `SteamAPI_Init`-fails behavior) | + `libsteam_api.so` + `steam_appid.txt` (1358090) |
| (n/a) | — | **Discord is ALWAYS built in, and NEVER needs a native**: the Rust port (issue 21) speaks the local Discord client's IPC pipe (`discord-rich-presence`) instead of linking the C++'s official Game SDK — so there is no `discord_game_sdk.*` to bundle in any build. With no Discord client running, the manager is inert (the game keeps running). | nothing extra, ever |

(There is also `oh-game/produce-lua-metadata` — the C++'s
`SSVOH_PRODUCE_LUA_METADATA` docs-producing build; it is a dev/docs
flag, never a shipping concern, and is not wired into the packager.)

## Determinism note

The release profile mirrors the C++ release build exactly: `opt-level =
3` (the CMake GNU module's `-O3`) and `debug-assertions = false`
(`-DNDEBUG`) — see the `[profile.release]` block in the workspace
`Cargo.toml` for the full citation. LTO, `strip`, `codegen-units` and
`panic` are deliberately left at the Rust defaults: the C++ release
build enables none of them (its LTO block is commented out,
`CMakeLists.txt:353-366`), and the port's float math must stay identical
to the test-verified default profile (D-000: the replay determinism
harness — `oh-game`'s `ReplayExecution` /
`ReplayExecutionBenchmark` suites plus the D-019 regression guard — is
pinned on it).

Replayed runs are absolutely deterministic: a replay file carries its
RNG seed (`oh-game/src/hexagon_game.rs:2554` — "replays always re-seed
from the replay file's seed"; `:777-783`), so the packaged binary prints
the SAME `Final time:` for a given replay on every run — which is
exactly what the packager's smoke test pins (`1.725` for the committed
fixture, asserted byte-exact on every packaging run).
