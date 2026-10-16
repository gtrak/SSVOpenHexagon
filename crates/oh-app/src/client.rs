//! C++ `mainClient` (main.cpp:244-597) — the pieces that exist in the
//! step-(b) tree, in the C++ order. Issue 15 step (b): the headless replay
//! run (`main.cpp:540-581`) is the ported core; the windowed path
//! (step (c)) lives in [`crate::windowed`] — this function routes to it
//! when `!headless`.

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
/// Steam (`:251-258` — the manager + the initialized-gated ticket/stats/
/// callbacks calls; issue 020) → folders (`:275-276`) → config load +
/// reapply (`:279-283`) → [config-save scope guard `:285-291`] → assets
/// (`:380`) → [pSaveAll guard: issue 17] → (audio: absent — issue 17 owns
/// the port; the engine null-guards every C++ `audio->` call site) →
/// (HexagonClient: issue 17) → replay scan (`:468-470`) → the branch:
/// windowed (`!headless`: `crate::windowed::WindowedClient` — window +
/// timer `:297-332`, engine + MenuGame + the four callback links
/// `:413-463`, boot dispatch `:471-537`, `window->run()` `:588-592`) /
/// headless (engine `:413-421` window-less + the replay run `:537-582`)
/// → Finished (`:594-595`).
///
/// `base_dir` = the game-data root the binary's `dispatch::run` resolves
/// and chdirs to before dispatching (the C++'s chdir to `argv[0]`'s
/// parent, `main.cpp:653`, ported as
/// `oh_app::dispatch::resolve_game_root` followed by `set_current_dir`):
/// the executable's directory if it contains `Assets/` (the C++
/// behavior), else the CWD if it does (the port's CWD-based test
/// harnesses), else the nearest ancestor with `_RELEASE/Assets`, else
/// the CWD. The C++ paths — `Profiles/`, `Replays/`, `config.json`, the
/// asset roots — are CWD-relative, and the Rust test idiom
/// (oh-config/oh-assets) resolves against an explicit base dir instead.
/// Documented deviation: the C++ chdirs UNCONDITIONALLY to the exe dir;
/// the port's fallback branches are reached only in layouts where the
/// C++ would fail outright (`resolve_game_root`'s docs).
///
/// `cli_level_name` / `cli_level_pack` are the C++ `-l` / `-p` arguments:
/// the C++ headless branch IGNORES them (they are consumed by the windowed
/// MenuGame path only, `main.cpp:507-511` — D-012 contract correction), so
/// the ported headless path leaves them unused; the windowed path
/// (`crate::windowed::WindowedClient::boot`) uses them.
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
    // --------------------------------------------------------------------
    // C++ main.cpp:251-258 — Steam integration: the manager (construct +
    // `SteamAPI_Init`) and, iff initialized, the encrypted-app-ticket
    // request (the no-`SecretSteamKey.hpp` build — gap 2 in
    // `.agents/skills/lib-steamworks.md`: logs + false, no flags set),
    // the stats/achievements request, and the first callback pump.
    // Held until the end of `mainClient` (the C++'s scope), shared with
    // the engine + the menu composition.
    let steam = crate::steam::steam_manager();
    if steam.is_initialized() {
        steam.request_encrypted_app_ticket();
        steam.request_stats_and_achievements();
        steam.run_callbacks();
    }

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

    // (C++ main.cpp:305-332: window + config setters + timer — the
    // windowed branch builds it in `crate::windowed::WindowedClient::build`
    // AFTER the shared assets load; the headless branch has no window.)

    // Initialize assets (C++ main.cpp:380: `HGAssets{&steamManager,
    // headless}`).
    let assets = match oh_assets::Assets::new(Arc::new(oh_vfs::RealFs::new(base_dir)), headless, false) {
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

    // Load drag & drop replay, if any -- otherwise run game as normal
    // (C++ main.cpp:468-470): the first leftover arg containing `.ohr.z`.
    let compressed_replay_filename = get_first_compressed_replay_filename_from_args(args);
    if !headless {
        // C++ main.cpp:297-332 (window + config setters + timer),
        // :413-463 (engine `window` = true + MenuGame + the four
        // fnHG*/fnGoToMenu links), :471-537 (the boot dispatch —
        // `gotoMenu` / `-p -l` / compressed-replay drag&drop), :588-592
        // (`window->run()`) — issue 15 step (c). The windowed composition
        // builds its OWN engine (with `window` = true) sharing the `Arc`
        // assets with the menu (the C++ shares one `HGAssets`);
        // `cli_level_pack` / `cli_level_name` feed the C++ `mg->init`
        // (`main.cpp:507-511`).
        match crate::windowed::WindowedClient::build(
            crate::windowed::create_window(&config.read().expect("config poisoned"), base_dir),
            config.clone(),
            assets,
            base_dir,
            steam.clone(),
            // C++ main.cpp:264-269 — `discordManager.emplace()` (the windowed
            // path is always non-headless): the real IPC manager.
            Some(Arc::new(oh_discord::DiscordManager::new(
                oh_discord::IpcDiscordCore::new(oh_discord::DISCORD_APP_ID),
            )) as Arc<dyn oh_discord::Discord>),
        ) {
            Ok(client) => {
                // C++ main.cpp:334-350 — the SIGINT handler (the safe
                // port: `signal-hook`'s flag register over the client's
                // `interrupt` flag; the loop head performs the handler's
                // actions — `windowed.rs` module docs, the SIGINT bullet).
                client.install_sigint();
                client.boot(
                    cli_level_pack,
                    cli_level_name,
                    compressed_replay_filename.as_deref(),
                );
                client.run();
                // C++ main.cpp:594-595.
                println!("[::mainClient] Finished");
                0
            }
            Err(e) => {
                // The C++ would `std::terminate()` on the font-load assert;
                // the port exits non-zero (documented deviation, same class
                // as the asset-load fatal above).
                eprintln!("[FATAL ERROR] Windowed boot failed: {e}");
                1
            }
        }
    } else {
        // Initialize hexagon game (C++ main.cpp:413-421: null steam/discord/
        // audio/window/client; the step-(b) engine is window-less). The C++
        // headless path has NO Discord manager (main.cpp:264-269 `if(!headless)`
        // is false), so `discord = None`.
        let composed = compose_with(
            assets,
            base_dir.to_path_buf(),
            false, /* window */
            Arc::new(NullProfiles),
            steam,
            None, // C++ headless: no Discord manager (main.cpp:264-269)
        );

        // (C++ main.cpp:426-463: MenuGame + the fnHG* links — windowed
        // only; the headless branch has no menu.)

        // C++ main.cpp:537-582 — the headless branch
        // (SSVOH_ASSERT(headless) at :539).
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

        // (C++ main.cpp:590-593: `window->run()` — windowed only; the
        // headless branch skips it.)

        // C++ main.cpp:594-595.
        println!("[::mainClient] Finished");
        0
    }
}
