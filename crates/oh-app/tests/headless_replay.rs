//! Issue 15 step (b): the headless client run (C++ `mainClient`'s headless
//! branch, main.cpp:540-581) — pinned by running the REAL `open-hexagon`
//! binary against a sandbox data root. (The step-(c) windowed wiring is
//! pinned at the library level in `tests/windowed.rs` — the real GL +
//! winit run is MANUAL, spec §5: running the binary's windowed path in a
//! unit test would hang on a machine WITH a display.)
//!
//! # The cross-check (spec §6, corrected — D-014)
//!
//! The spec's "cross-check against `tests/determinism.rs`'s established
//! numbers, e.g. the cube 1.3875 fixture" does not exist as written:
//! `determinism.rs` pins NO absolute numbers (its module docs:
//! "Self-consistent, NOT C++-verified … NO expected numbers" — D-018),
//! and the live run's score is not absolutely pinnable anyway (the game
//! RNG is entropy-seeded per `newGame` — C++-faithful,
//! `HexagonGame.cpp:306` + `initializeRng()` `:57-65`; observed
//! run-to-run: 1.75417, 1.725, …). The `1.3875` figure appears only in
//! DECISIONS.md D-019 as a scratch discrimination value, not as a fixture.
//! So the established cross-check IS the D-018 property `determinism.rs`
//! establishes: the binary's printed `Final time:` (the replay run's
//! `playedTimeSeconds`, C++ `std::ostream` float format) must equal the
//! harness's own live-run score for that replay. Distinct reachable
//! scores (multiples of `TIME_STEP = 1/240 s`) are ≥ ~4e-3 s apart while
//! the 6-significant-digit resolution at ~1–2 s is ~1e-6 s, so comparing
//! through the shared `format_ostream_f64` is injective over reachable
//! values.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use common::release_dir;

// ---------------------------------------------------------------------
// Sandbox data root
// ---------------------------------------------------------------------

/// A disposable data root: the `_RELEASE` packs/assets/overrides linked
/// in + a COPY of the shipped `config.json` (the config-save guard
/// overwrites it — it must never write through a symlink into the tracked
/// `_RELEASE` file).
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "oh-app-headless-{}-{}-{tag}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace('-', "_")
        ));
        std::fs::create_dir_all(&dir).expect("sandbox dir");
        let release = release_dir();
        for name in ["Packs", "Assets", "ConfigOverrides"] {
            sandbox_link(&release.join(name), &dir.join(name));
        }
        std::fs::copy(release.join("config.json"), dir.join("config.json"))
            .expect("sandbox config copy");
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The sandbox's read-only views of the tracked `_RELEASE` subtrees:
/// symlinks on Unix (cheap) and Windows (`symlink_dir` — the
/// GitHub-hosted runner runs with the privilege; on a local machine
/// without symlink rights — non-admin without Developer Mode — fall
/// back to a recursive copy). The copy fallback keeps the sandbox
/// invariant ("never write through into the tracked `_RELEASE`")
/// strictly stronger.
fn sandbox_link(src: &Path, dst: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst).expect("sandbox link");
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(src, dst).is_err() {
            copy_dir_recursive(src, dst).expect("sandbox copy fallback");
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        copy_dir_recursive(src, dst).expect("sandbox copy");
    }
}

#[cfg_attr(unix, allow(dead_code))] // the Windows/other-platform fallbacks
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------
// The binary
// ---------------------------------------------------------------------

fn run_binary(sandbox: &Path, args: &[&str]) -> Output {
    let exe = std::env::var("CARGO_BIN_EXE_open-hexagon").expect("open-hexagon is built");
    Command::new(exe)
        .current_dir(sandbox)
        .args(args)
        .output()
        .expect("spawn open-hexagon")
}

// ---------------------------------------------------------------------
// The live-run fixture (the determinism harness's live phase)
// ---------------------------------------------------------------------

/// The `test/ReplayExecution.t.cpp` `doTest` live half (the same shape as
/// `oh-game/tests/common::run_live_to_death`): the `alwaysSpinRight`
/// fixed policy on the real `cube_1`/`apeirogon` (the exact ids the C++
/// test names — D-018), to death. Returns the live run's
/// `playedTimeSeconds` + its replay file.
fn live_run_to_death() -> (f64, oh_replay::ReplayFile) {
    let assets = Arc::new(
        oh_assets::Assets::new(Arc::new(oh_vfs::RealFs::new(release_dir())), true, false).expect("_RELEASE loads headless"),
    );
    let c = oh_app::compose::compose_with(
        assets,
        release_dir(),
        false, /* window */
        Arc::new(oh_game::seams::NullProfiles),
        Arc::new(oh_game::seams::NoneSteamSink),
        None, // no Discord (headless replay test)
    );
    // C++ test: `hg.alwaysSpinRight = true;` (the fixed policy that removes
    // the only timing risk — determinism.rs module docs).
    c.game.set_always_spin_right(true);
    // C++ test: `hg.newGame(packs[0], levels[0], true, 1.f, false);`
    c.game.new_game(
        "ohvrvanilla_vittorio_romeo_cube_1",
        "ohvrvanilla_vittorio_romeo_cube_1_apeirogon",
        true,
        1.0,
        false,
    );
    let captured = Arc::new(Mutex::new(None));
    let cap = captured.clone();
    c.game
        .set_on_death_replay_created(Arc::new(move |rf: &oh_replay::ReplayFile| {
            *cap.lock().expect("replay capture poisoned") = Some(rf.clone());
        }));
    c.game.set_must_start(true);
    let result = c
        .game
        .execute_game_until_death(1, 1.0)
        .expect("live run exceeded the C++ 1-second processing budget");
    let rf = captured
        .lock()
        .expect("replay capture poisoned")
        .take()
        .expect("on_death_replay_created never fired");
    (result.played_time_seconds, rf)
}

// ---------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------

/// Spec §6: headless + a REAL compressed replay fixture (built in-test
/// with `oh-replay`'s compressor from a live run) → exit 0 and the
/// printed final time equals the harness's own score for that replay
/// (module docs: the corrected cross-check).
#[test]
fn headless_real_compressed_replay_runs_to_death_and_prints_the_harness_score() {
    let sandbox = Sandbox::new("replay");
    let (live_score, rf) = live_run_to_death();
    let crf = oh_replay::compress_replay_file(&rf).expect("compress");
    let fixture = sandbox.path().join("cube_apeirogon.ohr.z");
    std::fs::write(&fixture, &crf.data).expect("write fixture");

    let fixture_arg = fixture.to_str().expect("utf-8 path");
    let out = run_binary(sandbox.path(), &["-headless", fixture_arg]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // C++ main.cpp:568-573 — `Player died.\nFinal time:
    // <playedTimeSeconds>\n` (the C++ `std::ostream` float format), equal
    // to the harness's live-run score (D-018, through the shared
    // `format_ostream_f64` — injective over reachable scores, module docs).
    let expected_final = format!(
        "Player died.\nFinal time: {}\n",
        oh_math::hg::concat::format_ostream_f64(live_score)
    );
    assert!(
        stdout.contains(&expected_final),
        "the printed final time must equal the harness's own score for that replay; stdout:\n{stdout}"
    );

    // C++ main.cpp:561-564 — the playing log (the arg string verbatim).
    assert!(
        stdout.contains(&format!(
            "[Replay] Playing compressed replay file in headless mode '{fixture_arg}'"
        )),
        "stdout:\n{stdout}"
    );

    // This milestone's lifecycle pieces, exercised end-to-end: folder
    // creation (main.cpp:65-79 + :275-276) and the config-save scope guard
    // (main.cpp:285-291) on the success path.
    assert!(
        stdout
            .contains("[::createFolderIfNonExistant] 'Profiles/' folder does not exist, creating"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("[::createFolderIfNonExistant] 'Replays/' folder does not exist, creating"),
        "stdout:\n{stdout}"
    );
    assert!(sandbox.path().join("Profiles").is_dir());
    assert!(sandbox.path().join("Replays").is_dir());
    assert!(
        stdout.contains("[::main] Saving config..."),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("[::main] Done saving config"),
        "stdout:\n{stdout}"
    );
    assert!(
        sandbox.path().join("config.json").is_file(),
        "the guard saved the config into the sandbox"
    );
    // C++ main.cpp:594 — the Finished line — and the config-save scope
    // guard (main.cpp:285-291) fires AFTER it, at scope exit: the C++
    // scope guard runs after the function body (including this log line)
    // completes, LIFO-last of the three mainClient guards. So the tail is
    // `Finished` → `Saving config...` → `Done saving config`.
    let finished_pos = stdout
        .find("[::mainClient] Finished\n")
        .expect("the Finished line; stdout:\n{stdout}");
    let saving_pos = stdout
        .find("[::main] Saving config...")
        .expect("the guard's first line; stdout:\n{stdout}");
    assert!(
        finished_pos < saving_pos,
        "the scope guard fires after the Finished line (C++ scope-end semantics)"
    );
    assert!(
        stdout.ends_with("[::main] Done saving config\n"),
        "the guard's second line is the very last output; stdout:\n{stdout}"
    );
}

/// Spec §6: headless WITHOUT a replay file → the exact C++ line
/// (main.cpp:544, `std::cout`) + `return 1` (main.cpp:545) — and the
/// config-save scope guard STILL fires on the early return (C++ scope-
/// guard semantics; the guard is registered before the branch).
#[test]
fn headless_without_replay_exits_1_with_the_exact_line() {
    let sandbox = Sandbox::new("noreplay");
    let out = run_binary(sandbox.path(), &["-headless"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "C++ main.cpp:545 `return 1`");
    assert!(
        stdout.contains("Running in headless mode without replay...?\n"),
        "the C++ main.cpp:544 exact line; stdout:\n{stdout}"
    );
    // The scope guard fires even on this early return (main.cpp:285-291).
    assert!(
        stdout.contains("[::main] Saving config..."),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("[::main] Done saving config"),
        "stdout:\n{stdout}"
    );
    assert!(sandbox.path().join("config.json").is_file());
    // The C++ `return 1` precedes the Finished line (main.cpp:594).
    assert!(
        !stdout.contains("[::mainClient] Finished"),
        "no Finished line on the early return; stdout:\n{stdout}"
    );
}

/// Spec §6: a CORRUPT replay file (readable, but not valid zlib —
/// `deserialize_from_file` succeeds, `decompress_replay_file` fails) →
/// the C++ `std::cerr` line (main.cpp:556) + `return 1` (main.cpp:557).
/// Discriminated from the missing-file fall-through below: NOT the
/// Failed-to-read log, NOT the Finished line.
#[test]
fn headless_corrupt_replay_exits_1_with_the_decompress_line() {
    let sandbox = Sandbox::new("corrupt");
    let fixture = sandbox.path().join("corrupt.ohr.z");
    std::fs::write(&fixture, b"definitely not zlib data").expect("write");

    let out = run_binary(sandbox.path(), &["-headless", "corrupt.ohr.z"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "C++ main.cpp:557 `return 1`");
    assert!(
        stderr.contains("Could not decompress replay file\n"),
        "the C++ main.cpp:556 cerr line; stderr:\n{stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("Failed to read compressed replay"),
        "the file WAS readable — this is the decompress-failure path; stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("[::mainClient] Finished"),
        "the C++ returns 1 before the Finished line; stdout:\n{stdout}"
    );
}

/// The D-014 double-check #1 (verified against the source): a replay arg
/// that NAME-SCANS (`contains ".ohr.z"`) but does not EXIST →
/// `deserialize_from_file` fails → the C++ logs `Failed to read
/// compressed replay file in headless mode '<name>'` (main.cpp:577-581)
/// and FALLS THROUGH to `return 0` — it does NOT `return 1` there.
#[test]
fn headless_missing_replay_file_logs_and_falls_through_to_exit_0() {
    let sandbox = Sandbox::new("missing");
    let out = run_binary(sandbox.path(), &["-headless", "missing.ohr.z"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the C++ fall-through: main.cpp:577-581 logs, then Finished + return 0"
    );
    assert!(
        stdout.contains(
            "[Replay] Failed to read compressed replay file in headless mode 'missing.ohr.z'"
        ),
        "stdout:\n{stdout}"
    );
    // The oh-replay port of the C++ cerr line for the unopenable file
    // (Replay.cpp:453).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Couldn't open compressed replay path 'missing.ohr.z'"),
        "stderr:\n{stderr}"
    );
    // The fall-through reaches the C++ Finished line — and, again, the
    // config-save scope guard fires AFTER the body (scope-end semantics),
    // closing the output.
    let finished_pos = stdout
        .find("[::mainClient] Finished\n")
        .expect("the fall-through reaches Finished; stdout:\n{stdout}");
    let saving_pos = stdout
        .find("[::main] Saving config...")
        .expect("the guard's first line; stdout:\n{stdout}");
    assert!(
        finished_pos < saving_pos,
        "the scope guard fires after the Finished line (C++ scope-end semantics)"
    );
    assert!(
        stdout.ends_with("[::main] Done saving config\n"),
        "the guard's second line is the very last output; stdout:\n{stdout}"
    );
}

// The step-(c) windowed stub this file used to pin (exit 1 +
// "Windowed mode is not yet ported") is gone with the wiring: the
// windowed path now boots for real (`oh_app::windowed::WindowedClient`).
// Its headless-first tests live in `tests/windowed.rs` (fake window;
// the real GL + winit run is MANUAL — spec §5: running the binary's
// windowed path in a unit test would hang on a machine WITH a display).
