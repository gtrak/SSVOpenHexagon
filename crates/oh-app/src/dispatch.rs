//! C++ `main` (main.cpp:604-686) — the mode dispatch, plus the ported
//! server entrypoint (issue 18 milestone 4: the `mainServer` wiring over
//! the `oh_net::HexagonServer` port).

use std::collections::HashSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use oh_config::Config;
use oh_game::hexagon_game::HexagonGame;
use oh_game::seams::NullProfiles;
use oh_lua::api::state::GameApiState;
use oh_net::database::Database;
use oh_net::server::{GameExecutionResult, HexagonServer, ReplayExecutor};

use crate::args::parse_args;
use crate::client::main_client;
use crate::compose::compose_with_config;
use crate::lifecycle::{load_and_reapply, ConfigSaveGuard};
use crate::print_lua_docs::main_print_lua_docs;

/// C++ `HexagonGame& _hexagonGame` → the port's `oh_net::ReplayExecutor`
/// seam (the ONE method the server calls —
/// `HexagonServer.cpp:683-687`). The adapter lives here (the composition
/// crate — `oh-net` must not depend on `oh-game`). It OWNS a `HexagonGame`
/// clone (the engine is an `Arc<Inner>` — the clone is cheap): the
/// server's `Box<dyn ReplayExecutor>` is `'static`, and the cloned
/// engine's `SharedLua` handles point at the composing `LuaContext` (the
/// `compose` module docs' drop-order contract) — so the composing
/// `Composed` must outlive every replay run (the `main_server` layout).
struct GameReplayExecutor {
    game: HexagonGame<GameApiState>,
}

impl ReplayExecutor for GameReplayExecutor {
    fn run_replay_until_death_and_get_score(
        &self,
        rf: &oh_replay::ReplayFile,
        max_processing_seconds: i32,
        timescale: f32,
    ) -> Option<GameExecutionResult> {
        self.game
            .run_replay_until_death_and_get_score(rf, max_processing_seconds, timescale)
            .map(|r| GameExecutionResult {
                total_time_seconds: r.total_time_seconds,
                played_time_seconds: r.played_time_seconds,
                paused_time_seconds: r.paused_time_seconds,
                custom_score: r.custom_score,
            })
    }
}

/// C++ `mainServer()` (main.cpp:197-242), in the C++ order (the Steam
/// manager absent — issue 20; the process-global `hg::Config` is the
/// port's one `Config`):
///
/// 1. `loadConfig({})` + `setUseLuaFileCache(true)` (`:199-203`) + the
///    config-save scope guard (`:205-209`);
/// 2. the headless `HGAssets` (`:211-214`);
/// 3. the null-everything `HexagonGame` (`:216-223`) — the port's
///    headless composition behind the [`ReplayExecutor`] seam;
/// 4. the `ohdb.sqlite` DB — the C++ `Database::` process singleton's
///    file (`Database.cpp:44`, CWD-relative → the port's `base_dir`);
/// 5. the `sf::IpAddress::resolve(Config::getServerIp()).value()`
///    (`:225-231`) with its `TODO (P0): handle resolve errors` — the
///    port's documented deviation: an explicit failure (the C++'s
///    `.value()` would abort on an unresolvable name);
/// 6. the `HexagonServer` (`:225-231`) — the C++ RUNS the server inside
///    its constructor (the `run()` at the ctor's end, the ctor-installed
///    SIGINT); the port's `new` + `run` split is the equivalent: bind,
///    install the safe SIGINT flag (`windowed.rs`'s `signal-hook`
///    precedent), then `run` until the flag.
///
/// Returns the C++ `mainServer`'s value (0 — the C++ even returns 0 when
/// the server's own initialization failed: the constructor logged the
/// failure and its `run()` never ran, `main.cpp:240-241`).
pub fn main_server(base_dir: &Path) -> i32 {
    // C++ main.cpp:199-203.
    let mut config = Config::default();
    if let Err(e) = load_and_reapply(&mut config, base_dir, &[]) {
        // The C++ would throw out of `loadConfig` (uncaught in
        // mainServer); the port exits non-zero (documented deviation,
        // the `main_client` precedent).
        eprintln!("[::mainServer] config load failed: {e}");
        return 1;
    }
    config.set_use_lua_file_cache(true);
    let config = Arc::new(RwLock::new(config));

    // C++ main.cpp:205-209: the config-save scope guard.
    let _config_save = ConfigSaveGuard::new(config.clone(), base_dir.join("config.json"));

    // C++ main.cpp:211-214: the headless `HGAssets` (the C++'s
    // steam-manager argument is issue 20).
    let assets = match oh_assets::Assets::new(base_dir, true, false) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            // The C++ would `std::terminate()` (Assets.cpp:323-352).
            eprintln!("[FATAL ERROR] Asset loading failed: {}", e.reason);
            return 1;
        }
    };

    // C++ main.cpp:216-223: the null-everything `HexagonGame` — the
    // port's headless composition (the `ReplayExecutor` seam: the one
    // method the server calls). `composed` deliberately outlives the
    // `server.run` below: the executor's engine clone holds `SharedLua`
    // handles into `composed.lua` (the `compose` module docs).
    let composed = compose_with_config(
        assets.clone(),
        base_dir.to_path_buf(),
        false, /* window */
        Arc::new(NullProfiles),
        config.clone(),
        // C++ main.cpp:199 — `mainServer` also constructs a `steam_manager`
        // (the NoSteam/feature-split backend; the server path never
        // satisfies Steam requests without a client, exactly as the C++
        // headless server degrades).
        crate::steam::steam_manager(),
        None, // C++ server: `nullptr /* discordManager */` (main.cpp:217)
        None, // C++ server: no window → no renderer (the headless `nullptr`)
    );
    let executor = GameReplayExecutor {
        game: composed.game.clone(),
    };

    // The C++ `Database::` process singleton's file (`"ohdb.sqlite"`,
    // `Database.cpp:44` — CWD-relative → the port's `base_dir`).
    let db = match Database::open(&base_dir.join("ohdb.sqlite")) {
        Ok(db) => db,
        Err(e) => {
            // No C++ equivalent at this point (the C++ singleton is
            // lazy); the port's explicit failure is the same class of
            // documented deviation as the config-load failure above.
            eprintln!("[::mainServer] database open failed: {e}");
            return 1;
        }
    };

    // C++ main.cpp:225-231: the `sf::IpAddress::resolve(...).value()`
    // with its `TODO (P0): handle resolve errors`.
    let server_ip: IpAddr = match (
        config.read().expect("config poisoned").get_server_ip(),
        0u16,
    )
        .to_socket_addrs()
        .into_iter()
        .flatten()
        .next()
    {
        Some(addr) => addr.ip(),
        None => {
            eprintln!("[::mainServer] could not resolve the server IP");
            return 1;
        }
    };

    // C++ main.cpp:225-231 (the constructor's arguments, the C++ order:
    // assets, game, ip, port, control port, whitelist).
    let (server_port, server_control_port, server_level_whitelist) = {
        let c = config.read().expect("config poisoned");
        (
            c.get_server_port(),
            c.get_server_control_port(),
            HashSet::from_iter(c.get_server_level_whitelist().iter().cloned()),
        )
    };

    let mut server = match HexagonServer::new(
        assets,
        Box::new(executor),
        db,
        server_ip,
        server_port,
        server_control_port,
        server_level_whitelist,
    ) {
        Ok(server) => server,
        Err(_e) => {
            // C++ main.cpp:240-241 — the C++ STILL logs "Finished" and
            // returns 0 when the constructor's initialization failed
            // (the constructor logged the failure; its `run()` never ran).
            println!("[::mainServer] Finished");
            return 0;
        }
    };

    // C++ `HexagonServer.cpp:1386-1398` (the ctor-installed SIGINT: the
    // C++'s `globalRunning = false` + listener close, the `TODO (P2):
    // UB` its comment marks) — the safe port: `signal-hook`'s flag
    // register (the `windowed.rs` precedent); `server.run`'s loop head
    // observes the flag (the C++'s `_running` check position).
    let stop = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::signal::SIGINT, stop.clone());

    // C++ `HexagonServer::run` (`HexagonServer.cpp:483-...`) — the C++
    // calls it from the constructor; the port's `new` + `run` split.
    server.run(&stop);

    // C++ main.cpp:240-241.
    println!("[::mainServer] Finished");
    0
}

/// C++ `main`'s chdir (main.cpp:653, "Set working directory to current
/// executable location"): the game-data root (`Assets/`, `Packs/`,
/// `config.json`, `Profiles/`, `Replays/`, `steam_appid.txt`) the port
/// resolves before dispatching. Pure (only existence checks) so it is
/// unit-testable without spawning anything.
///
/// Precedence (the existence check is "`<dir>/Assets` is a directory"):
///
/// 1. `exe_dir` — if it contains `Assets/` (the C++ main.cpp:653
///    behavior: the packaged layout, `<game_root>/open-hexagon.exe`
///    beside `Assets/`, run from any CWD);
/// 2. `cwd` — if it contains `Assets/` (keeps the port's CWD-based test
///    harnesses / explicit roots working);
/// 3. the nearest ancestor `A` of `exe_dir` (including `exe_dir` itself)
///    for which `A/_RELEASE/Assets` exists — returns `A/_RELEASE` (the
///    dev layout: `cargo run` from the repo root —
///    `target/<profile>/open-hexagon` → `<repo>/_RELEASE`);
/// 4. the nearest ancestor `A` of `cwd` (including `cwd` itself) for
///    which `A/_RELEASE/Assets` exists — returns `A/_RELEASE`;
/// 5. otherwise `cwd` unchanged (so the existing
///    `[FATAL ERROR] Asset loading failed` still fires).
///
/// Documented deviation from the C++: the C++ chdirs UNCONDITIONALLY to
/// the executable's directory (main.cpp:653). The port only deviates in
/// the fallback branches (2)-(5), which are reached only in layouts where
/// the C++ would fail outright (no `Assets/` beside the exe); the
/// packaged layout (branch 1) is byte-identical to the C++.
fn resolve_game_root(cwd: &Path, exe_dir: &Path) -> PathBuf {
    if exe_dir.join("Assets").is_dir() {
        return exe_dir.to_path_buf();
    }
    if cwd.join("Assets").is_dir() {
        return cwd.to_path_buf();
    }
    for ancestor in exe_dir.ancestors() {
        let release = ancestor.join("_RELEASE");
        if release.join("Assets").is_dir() {
            return release;
        }
    }
    for ancestor in cwd.ancestors() {
        let release = ancestor.join("_RELEASE");
        if release.join("Assets").is_dir() {
            return release;
        }
    }
    cwd.to_path_buf()
}

/// C++ `main(argc, argv)` (main.cpp:604-686) — the ported pieces, in the
/// C++ order:
///
/// 1. `argc < 1` fatal (`:605-609`) — Rust `std::env::args()` always
///    yields `argv[0]`, so this is unreachable but ported for verbatimness
///    (the C++ `return -1` → POSIX exit status 255);
/// 2. (libsodium init `:615-621` — issue 16, not ported yet: no sodium use
///    exists outside the oh-net codec, which inits on demand);
/// 3. (the SIGINT handler `:627-637` — step (c), with the window; the
///    server installs its own, `HexagonServer.cpp:1386-1398`);
/// 4. (the `log.txt` save scope guard `:643-650` — the port's logging is
///    stdout-`[channel]` lines, there is no log file to save; documented);
/// 5. the chdir to `argv[0]`'s parent (`:653`, "Set working directory to
///    current executable location") — the port's [`resolve_game_root`] +
///    `set_current_dir`, called here BEFORE `parseArgs` (the C++'s
///    position); documented deviation: the C++ chdirs UNCONDITIONALLY to
///    the exe dir, the port's fallback branches (2)-(5) are reached only
///    in layouts where the C++ would fail outright (`resolve_game_root`'s
///    docs);
/// 6. `parseArgs` (`:660-661`);
/// 7. the dispatch order (`:666-686`): `printLuaDocs` → `server` →
///    `mainClient` (the C++ `SSVOH_ASSERT(!printLuaDocs)` /
///    `SSVOH_ASSERT(!server)` at `:684-685` hold by construction here).
///
/// Returns the C++ `main`'s return value (`-1`, `0`, or `1`); the binary
/// entrypoint converts it to the POSIX exit status (low byte, so `-1` →
/// 255 exactly as the C++).
pub fn run(argv: &[String]) -> i32 {
    if argv.is_empty() {
        // C++ main.cpp:605-609 (unreachable in Rust — `std::env::args()`
        // always yields `argv[0]`; kept for the verbatim port).
        eprintln!("Fatal error: no executable specified");
        return -1;
    }

    // C++ main.cpp:653 — "Set working directory to current executable
    // location" (BEFORE `parseArgs`, the C++'s chdir position). Branch 1
    // of `resolve_game_root` is the C++ behavior; branches 2-5 are its
    // documented deviation (the resolver's docs).
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .or_else(|| Path::new(&argv[0]).parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let base_dir = resolve_game_root(&cwd, &exe_dir);
    if let Err(e) = std::env::set_current_dir(&base_dir) {
        // The C++'s `std::filesystem::current_path` would throw on
        // failure; the port logs and proceeds with `base_dir` (every
        // CWD-relative path below is anchored on it, not the process
        // CWD).
        eprintln!(
            "[::main] could not set the working directory to {}: {e}",
            base_dir.display()
        );
    }

    let parsed = parse_args(argv);

    // Print Lua docs mode (C++ main.cpp:666-669).
    if parsed.print_lua_docs {
        return main_print_lua_docs(&base_dir);
    }

    // Server mode (C++ main.cpp:674-677).
    if parsed.server {
        return main_server(&base_dir);
    }

    // Client mode (C++ main.cpp:682-686).
    main_client(
        parsed.headless,
        &parsed.args,
        parsed.cli_level_name.as_deref(),
        parsed.cli_level_pack.as_deref(),
        &base_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C++ `main`'s `argc < 1` branch (main.cpp:605-609): the C++
    /// `return -1` (POSIX exit status 255 via the binary's low-byte
    /// conversion). Unreachable in Rust, but the ported check pins it.
    #[test]
    fn empty_argv_is_the_c_fatal_error() {
        assert_eq!(run(&[]), -1);
    }

    /// The game-root resolver (the ported main.cpp:653 chdir) — one test
    /// per precedence branch.
    #[test]
    fn resolve_game_root_prefers_the_exe_dir_when_it_has_assets() {
        let exe = tempfile::tempdir().expect("exe dir");
        std::fs::create_dir_all(exe.path().join("Assets")).expect("exe Assets");
        let cwd = tempfile::tempdir().expect("cwd");
        assert_eq!(
            resolve_game_root(cwd.path(), exe.path()),
            exe.path().to_path_buf(),
            "branch 1: the exe dir wins even when the cwd has no Assets/"
        );
    }

    #[test]
    fn resolve_game_root_falls_back_to_the_cwd_when_the_exe_dir_lacks_assets() {
        let exe = tempfile::tempdir().expect("exe dir");
        let cwd = tempfile::tempdir().expect("cwd");
        std::fs::create_dir_all(cwd.path().join("Assets")).expect("cwd Assets");
        assert_eq!(
            resolve_game_root(cwd.path(), exe.path()),
            cwd.path().to_path_buf(),
            "branch 2: the CWD sandbox (the port's test harnesses)"
        );
    }

    #[test]
    fn resolve_game_root_walks_up_from_the_exe_dir_for_release() {
        let root = tempfile::tempdir().expect("root");
        let exe = root.path().join("target").join("debug");
        std::fs::create_dir_all(&exe).expect("exe dir");
        std::fs::create_dir_all(root.path().join("_RELEASE").join("Assets")).expect("_RELEASE/Assets");
        let cwd = tempfile::tempdir().expect("cwd");
        assert_eq!(
            resolve_game_root(cwd.path(), &exe),
            root.path().join("_RELEASE"),
            "branch 3: the dev layout (target/<profile>/open-hexagon -> <repo>/_RELEASE)"
        );
    }

    #[test]
    fn resolve_game_root_walks_up_from_the_cwd_for_release() {
        let root = tempfile::tempdir().expect("root");
        let cwd = root.path().join("run").join("here");
        std::fs::create_dir_all(&cwd).expect("cwd");
        std::fs::create_dir_all(root.path().join("_RELEASE").join("Assets")).expect("_RELEASE/Assets");
        let exe = tempfile::tempdir().expect("exe dir");
        assert_eq!(
            resolve_game_root(&cwd, exe.path()),
            root.path().join("_RELEASE"),
            "branch 4: the cwd's _RELEASE ancestor"
        );
    }

    #[test]
    fn resolve_game_root_falls_back_to_the_cwd_when_nothing_exists() {
        let exe = tempfile::tempdir().expect("exe dir");
        let cwd = tempfile::tempdir().expect("cwd");
        assert_eq!(
            resolve_game_root(cwd.path(), exe.path()),
            cwd.path().to_path_buf(),
            "branch 5: the existing [FATAL ERROR] Asset loading failed still fires"
        );
    }

    /// The ported `main_server` (the C++ `mainServer`, main.cpp:197-242)
    /// in an EMPTY base dir: the config loads (defaults), the asset load
    /// fails (no `_RELEASE` under the tempdir) → the documented
    /// not-`std::terminate` exit 1, BEFORE any socket bind. (The full
    /// socket path is pinned by `oh-net`'s loopback tests — the
    /// `oh_net::server` test module.)
    #[test]
    fn main_server_in_an_empty_base_dir_fails_on_the_asset_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(main_server(dir.path()), 1);
    }
}
