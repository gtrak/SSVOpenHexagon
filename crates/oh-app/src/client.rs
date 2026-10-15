//! C++ `mainClient` (main.cpp:244-597) — the pieces that exist in the
//! step-(b) tree, in the C++ order. Issue 15 step (b): the headless replay
//! run (`main.cpp:540-581`) is the ported core; the windowed path is an
//! unmistakable non-zero-exit stub for step (c).

use std::path::Path;
use std::sync::{Arc, RwLock};

use oh_config::Config;
use oh_game::seams::NullProfiles;

use crate::args::get_first_compressed_replay_filename_from_args;
use crate::compose::compose_with;
use crate::lifecycle::{create_folder_if_non_existant, load_and_reapply, ConfigSaveGuard};

/// C++ `mainClient(headless, args, cliLevelName, cliLevelPack)`
/// (main.cpp:244-597), the ported pieces in the C++ order:
///
/// (Steam issue 20 / Discord issue 21 absent — the C++ Discord manager
/// exists only when `!headless`) → folders (`:275-276`) → config load +
/// reapply (`:279-283`) → [config-save scope guard `:285-291`] → (window +
/// ImGui: step (c)) → assets (`:380`) → [pSaveAll guard: issue 17] →
/// (audio: absent — issue 17 owns the port; the engine null-guards every
/// C++ `audio->` call site) → (HexagonClient: issue 17) → engine
/// (`:410-417`) → (MenuGame + links: step (c)) → replay scan
/// (`:468-470`) → headless branch (`:540-581`) / windowed stub → (window
/// run: step (c)) → Finished (`:594-595`).
///
/// `base_dir` = the process CWD (the binary passes
/// `std::env::current_dir()`; the C++ paths — `Profiles/`, `Replays/`,
/// `config.json`, the asset roots — are CWD-relative, and the C++'s
/// `main()` chdir-to-`argv[0]`-parent (`main.cpp:650`) is NOT ported: it is
/// a release-packaging convenience outside this issue's dispatch scope,
/// and the Rust test idiom (oh-config/oh-assets) resolves against an
/// explicit base dir instead).
///
/// `cli_level_name` / `cli_level_pack` are accepted for the verbatim
/// signature: the C++ headless branch IGNORES them (they are consumed by
/// the windowed MenuGame path only, `main.cpp:495-505` — D-012 contract
/// correction), so the ported headless path leaves them unused; step (c)
/// uses them.
///
/// Documented deviations (every missing C++ piece, all out of step-(b)
/// scope): the C++ fatal `std::terminate()` on asset-load failure
/// (`Assets.cpp:323-352`) → `[FATAL ERROR]` line + exit 1; a
/// `syncAllFromObj` failure (the C++ throws uncaught out of
/// `Config::loadConfig`) → error line + exit 1; the C++ `std::cout <<
/// x << '\n'` float print → the port's C++-ostream-format port
/// (`oh_math::hg::concat::format_ostream_f64`, 6 significant digits).
pub fn main_client(
    headless: bool,
    args: &[String],
    cli_level_name: Option<&str>,
    cli_level_pack: Option<&str>,
    base_dir: &Path,
) -> i32 {
    // C++ signature parity: the ported (headless) branch ignores the
    // `-p`/`-l` pair — the C++ consumes it in the windowed MenuGame path
    // only (`main.cpp:495-505`; D-012). Step (c) uses it.
    let _ = (cli_level_name, cli_level_pack);

    // Create game folders if needed (C++ main.cpp:275-276).
    create_folder_if_non_existant(base_dir, "Profiles/");
    create_folder_if_non_existant(base_dir, "Replays/");

    // Load configuration (and overrides) (C++ main.cpp:279-283).
    let mut config = Config::default();
    if let Err(e) = load_and_reapply(&mut config, base_dir, args) {
        // C++ would throw out of `syncAllFromObj` (uncaught in mainClient);
        // the port exits non-zero (documented deviation).
        eprintln!("[::mainClient] config load failed: {e}");
        return 1;
    }
    let config = Arc::new(RwLock::new(config));

    // C++ main.cpp:285-291 scope guard — fires on EVERY exit below,
    // including the headless branch's early returns (LIFO-last of the
    // three mainClient guards; the pSaveAll/ImGui pieces are issues 17 /
    // step (c)).
    let _config_save = ConfigSaveGuard::new(config.clone(), base_dir.join("config.json"));

    // (C++ main.cpp:305-377: window + ImGui — step (c).)

    // Initialize assets (C++ main.cpp:380: `HGAssets{&steamManager,
    // headless}`).
    let assets = match oh_assets::Assets::new(base_dir, headless, false) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            // C++ would `std::terminate()` (Assets.cpp:323-352).
            eprintln!("[FATAL ERROR] Asset loading failed: {}", e.reason);
            return 1;
        }
    };

    // (C++ main.cpp:381-387 pSaveAll scope guard — issue 17: the profile
    // mutation API is unported; headless replay runs never create replays
    // (`in_replay` guard, `hexagon_game.rs:1118`), so nothing is saved.)

    // (C++ main.cpp:390-407: audio + HexagonClient — issue 17; the engine
    // takes the None/no-op equivalents its seams already accept.)

    // Initialize hexagon game (C++ main.cpp:410-417: null steam/discord/
    // audio/window/client; the step-(b) engine is window-less).
    let composed = compose_with(
        assets,
        base_dir.to_path_buf(),
        false, /* window */
        Arc::new(NullProfiles),
    );

    // (C++ main.cpp:420-466: MenuGame + the fnHG* links — step (c).)

    // Load drag & drop replay, if any -- otherwise run game as normal
    // (C++ main.cpp:468-470): the first leftover arg containing `.ohr.z`.
    let compressed_replay_filename = get_first_compressed_replay_filename_from_args(args);

    if !headless {
        // Step (c) replaces this with the C++ `:305-377` window creation +
        // `:420-466` MenuGame + the windowed replay/menu branch
        // (`:471-537`) + `:590-593` `window->run()`. Documented deviation:
        // the C++ RUNS THE GAME here; the stub must be unmistakable.
        // (`cli_level_pack` / `cli_level_name` feed the C++ `mg->init`
        // `:495-505` — step (c).)
        eprintln!("Windowed mode is not yet ported (issue 15 step c)");
        return 1;
    }

    // C++ main.cpp:540-581 — the headless branch (SSVOH_ASSERT(headless)).
    match &compressed_replay_filename {
        None => {
            // C++ main.cpp:542-545: `std::cout` + `return 1` (the config
            // save guard fires on the way out — C++ scope-guard semantics).
            println!("Running in headless mode without replay...?");
            return 1;
        }
        Some(filename) => {
            let mut crf = oh_replay::CompressedReplayFile::default();
            // C++ main.cpp:549: `crf.deserialize_from_file(*filename)`.
            if crf.deserialize_from_file(filename) {
                match oh_replay::decompress_replay_file(&crf) {
                    // C++ main.cpp:554-558: `std::cerr` + `return 1`.
                    None => {
                        eprintln!("Could not decompress replay file");
                        return 1;
                    }
                    Some(replay_file) => {
                        // C++ main.cpp:561-564.
                        println!(
                            "[Replay] Playing compressed replay file in headless mode '{filename}'"
                        );

                        // TODO (P2): check level validity — the C++ comment,
                        // verbatim (`main.cpp:566`).

                        // C++ main.cpp:568-573: `runReplayUntilDeathAndGetScore
                        // (replayFile, 1 /* maxProcessingSeconds */, 1.f
                        // /* timescale */).value().playedTimeSeconds`,
                        // printed with the C++ `std::ostream` float format
                        // (6 significant digits — the port's
                        // `format_ostream_f64`). The `.value()` would abort
                        // the C++ if the 1-second budget is exceeded; the
                        // port's `expect` is the equivalent (unreachable for
                        // the D-018 self-consistent replays: a ~1.4 s game
                        // run takes milliseconds of processing).
                        let result = composed
                            .game
                            .run_replay_until_death_and_get_score(&replay_file, 1, 1.0)
                            .expect("replay playback exceeded the C++ 1-second processing budget (the C++ .value() would abort)");
                        println!(
                            "Player died.\nFinal time: {}",
                            oh_math::hg::concat::format_ostream_f64(result.played_time_seconds)
                        );
                    }
                }
            } else {
                // C++ main.cpp:577-581: log and FALL THROUGH to Finished +
                // `return 0` — the C++ does NOT `return 1` here (verified
                // against the source, the D-014 double-check): a
                // non-openable file is logged, the client exits 0.
                println!(
                    "[Replay] Failed to read compressed replay file in headless mode '{filename}'"
                );
            }
        }
    }

    // (C++ main.cpp:590-593: `window->run()` — step (c); headless skips it.)

    // C++ main.cpp:594-595.
    println!("[::mainClient] Finished");
    0
}
