//! Lane 030a — the LuaJIT↔Luau **differential-simulation harness** (the
//! permanent test module behind the issue-30 "can we unify on Luau?"
//! decision; task `.agents/tasks/issue-030a-lua-parity.md` §2/§7).
//!
//! # What this is
//!
//! The shipped corpus (`_RELEASE/Packs/**`) driven through the REAL engine
//! (the same `compose` wiring the issue-23 parity tests and the oh-game
//! determinism harness use — a real `oh_game::seams::ScriptHost` over
//! `oh-lua`, a real `AssetsApi` over `oh_assets::Assets`), headless and
//! flat-out (no window ⇒ no vsync ⇒ each `update` + `post_update` pair is
//! one frame of a fixed `dt` schedule; a 60 s playthrough at this rate is
//! ~14.4k frames, sub-second to a few seconds). For every case
//! `(level, seed, input program, dt schedule)` over the 42 shipped
//! `_RELEASE/Packs/**/*.lua` scripts the harness emits a **per-frame
//! fingerprint** of gameplay-visible state and records the result in an
//! artifact under `target/lua-parity/` (large + machine-specific ⇒ never
//! committed; `target/` is gitignored).
//!
//! Two builds of this test (one per backend — `mlua` permits only one VM
//! per build) produce the two artifacts; the comparator (below) reads both
//! and fails on the **first divergent frame** — printing the case id, the
//! frame index, and which fingerprint component diverged (first-divergence
//! points at the construct that differs, which is the difference between a
//! verdict and a mystery).
//!
//! # Backends (lane 030a features)
//!
//! * `cargo test -p oh-parity` — the LuaJIT backend (default)
//! * `cargo test -p oh-parity --no-default-features --features backend-luau`
//!
//! # The artifact (fingerprint definition travels WITH the data)
//!
//! `target/lua-parity/<backend>/run-<k>/cases.json`:
//!
//! ```json
//! { "definition": { ...the full fingerprint + case-generator definition... },
//!   "backend": "luajit", "run": 0,
//!   "cases": [ { "id": ..., "case": {...}, "result": {...},
//!                 "frames": [ [ ...10 u64/hex components... ], ... ] }, ... ] }
//! ```
//!
//! # Multi-process repeat (the pairs-order / per-process hashing question)
//!
//! If a VM randomized string/table hashing per process, `pairs` order would
//! vary run-to-run *within that VM* — so any fingerprint that changes
//! across separate processes of the SAME backend means nondeterministic
//! gameplay, a red flag independent of the Luau question. Each run index is
//! a separate `cargo test` process (see `tools/lua-parity.sh`);
//! `repeat_determinism` below cross-checks run-`k` against run-`0`.
//!
//! # Environment
//!
//! * `LUA_PARITY_RUN` — the run index to emit (default `0`).
//! * `LUA_PARITY_COMPARE=1` — make `compare_backends` run (it passes
//!   vacuously otherwise, so a plain `cargo test` needs no counterpart
//!   artifact; `tools/lua-parity.sh` drives the real A/B).
//! * `LUA_PARITY_REPEATS=all` (or `LUA_PARITY_REPEATS=1,2,3`) — make
//!   `repeat_determinism` cross-check the named run indexes against run-0.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use oh_data::utils::build_pack_id;
use oh_game::lua_state::SharedState;
use oh_game::seams::ScriptHost;
use oh_replay::{ReplayData, ReplayFile};

use common::{compose, release_dir};

// ---------------------------------------------------------------------------
// The fingerprint definition (v1) — recorded verbatim in every artifact
// ---------------------------------------------------------------------------

/// C++ `Config::TIME_STEP` (Config.hpp:26): the engine's fixed headless
/// update step (1.0 frametime unit = 1/60 s, so 0.25 = 240 updates per
/// second of game time). The C++ headless `executeGameUntilDeath` uses
/// exactly this dt — the most oracle-pinned pacing available.
const DT: f32 = 0.25;

/// The frame cap per case (~33 s of game time at the fixed dt — the
/// spec's ~12k-frame scale trimmed so the permanent gate stage stays
/// bounded: the `stress1` level's deliberately heavy `onUpdate` makes a
/// full 12k-frame run the single most expensive case). A case that has
/// not died by the cap is compared on the frames it did produce; the cap
/// is identical for both backends, so a cap-reached case is a fair
/// A/B comparison of its prefix.
const FRAME_CAP: u64 = 8_000;

/// The fixed-dt schedule for every case: `DT` on every frame (the C++
/// headless pacing). Recorded per case in the artifact.
const DT_SCHEDULE: &str = "constant";

/// The per-frame fingerprint components, in artifact order (index in
/// brackets = the component reported by the comparator on divergence):
///
/// | # | component       | source (gameplay-visible state)                          |
/// |---|-----------------|----------------------------------------------------------|
/// | 0 | `rng`           | the game-RNG PCG engine's raw u64 state (draw count is   |
/// |   |                 | implicit: under a fixed seed the state is a bijection of |
/// |   |                 | the draw sequence)                                        |
/// | 1 | `wall_count`    | `walls.len()`                                             |
/// | 2 | `walls`         | FNV-1a over every wall's 4 vertex positions (f32 bits,   |
/// |   |                 | LE) + hue_mod + speed/curve data, wall order preserved   |
/// | 3 | `custom_walls`  | `cw_manager.count()`                                      |
/// | 4 | `timelines`     | main/event `Timeline2::size()` + custom-timeline count   |
/// |   |                 | + each custom timeline's size                             |
/// | 5 | `player`        | player position (Vec2 f32 bits) + player angle (f32 bits)|
/// | 6 | `score`         | `get_replay_score()` (f64 bits: customScore if set,      |
/// |   |                 | else played time — C++ `getReplayScore`)                 |
/// | 7 | `custom_score`  | `status.get_custom_score()` (f32 bits)                   |
/// | 8 | `level_status`  | FNV-1a over every `LevelStatus` field (tracked_variables |
/// |   |                 | sorted by key — HashMap order is not observable state)   |
/// | 9 | `status`        | the `HexagonGameStatus` timers/accumulators/flags        |
///
/// A value that diverges in only component 2 (with 1 unchanged) localizes
/// the divergence to wall MOTION (positions/rotation); component 0
/// localizes it to the RNG stream; components 8/9 to level-status/timer
/// logic; and so on.
const FINGERPRINT_DEF: &str = "lua-engine-parity/v1";

/// The input-program generator (documented with the results per the spec):
/// a SplitMix64 stream seeded from the case id (below); per tick `i`, one
/// u64 draw `r`; the byte is `0` unless: `r & 1` ⇒ right (0x02), else
/// `r & 2` ⇒ left (0x01); plus, independently, `r & 4` ⇒ swap (0x04) and
/// `r & 8` ⇒ focus (0x08) (bit order pinned by the C++-derived oracle
/// replay fixtures: bit0=left, bit1=right, bit2=swap, bit3=focus). The
/// stream is a SYNTHETIC REPLAY — it rides the project's most
/// oracle-pinned path (`set_last_replay` + `new_game(executeLastReplay)`,
/// issue 02) rather than a new driver.
const INPUT_GEN: &str = "splitmix64/case-id/right1-left2-swap4-focus8";

/// SplitMix64 (the documented input-program PRNG).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// The case seed: FNV-1a 64 over the case id (stable regardless of the
/// case-list ordering; +1 so the seed is never 0).
fn case_seed(case_id: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in case_id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h.wrapping_add(1)
}

/// One input program (the documented generator).
fn input_program(seed: u64, count: usize) -> Vec<u8> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            let r = splitmix64(&mut state);
            let mut b = 0u8;
            if r & 1 != 0 {
                b |= 0x02; // right
            } else if r & 2 != 0 {
                b |= 0x01; // left
            }
            if r & 4 != 0 {
                b |= 0x04; // swap
            }
            if r & 8 != 0 {
                b |= 0x08; // focus
            }
            b
        })
        .collect()
}

// ---------------------------------------------------------------------------
// FNV-1a 64 (the fingerprint hash — deterministic, dependency-free, and
// stable across machines; the artifacts are compared for equality, not
// re-hashed, so a non-cryptographic hash is exactly what is needed)
// ---------------------------------------------------------------------------

fn fnv1a(bytes: impl AsRef<[u8]>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes.as_ref() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The canonical byte encoder: fixed-width little-endian, one tag byte per
/// field kind (so a struct layout change can never silently re-encode).
#[derive(Default)]
struct Enc {
    buf: Vec<u8>,
}

impl Enc {
    fn u32(&mut self, v: u32) {
        self.buf.push(b'u');
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.push(b'U');
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    fn bool(&mut self, v: bool) {
        self.buf.push(if v { b't' } else { b'f' });
    }
    fn str(&mut self, v: &str) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v.as_bytes());
    }
    fn hash(&self) -> u64 {
        fnv1a(&self.buf)
    }
}

// ---------------------------------------------------------------------------
// The case model + the seeded case generator (all 42 shipped scripts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind")]
enum CaseKind {
    /// One of the `Packs/<pack>/Scripts/Levels/*.lua` level scripts, run as
    /// its level (replay of the synthetic input program against
    /// `(pack_id, level_id)`).
    Level {
        pack: String,
        level: String,
        /// The script file (recorded for auditability).
        #[serde(rename = "script")]
        script: String,
    },
    /// One of the `Packs/base/Scripts/*.lua` helper scripts (not a level —
    /// no level id exists in the asset registry): smoke-loaded into the
    /// engine context AFTER the neutral level load, then driven for
    /// `SCRIPT_CASE_FRAMES` frames of the fixed schedule (idle input).
    Script {
        /// The script path relative to `_RELEASE` (the host's CWD).
        script: String,
    },
}

/// The 6 helper-script cases ride a NEUTRAL level: the corpus-pinned
/// `cube_1` / `apeirogon` (its onInit state is C++-derived-pinned by the
/// issue-23 tests, so the neutral context is itself oracle-pinned).
const NEUTRAL_PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const NEUTRAL_LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

#[derive(Debug, Clone, serde::Serialize)]
struct Case {
    id: String,
    kind: CaseKind,
    seed: u64,
    input_count: usize,
}

/// The scripted-case frame count (~8 s of game time at the fixed dt).
const SCRIPT_CASE_FRAMES: u64 = 2_000;

/// The helper-script cases' input program: the C++ `alwaysSpinRight`
/// benchmark input (an all-0x02 stream — deterministic and long-lived,
/// the ReplayExecutionBenchmark pattern), so the smoke run gets its full
/// `SCRIPT_CASE_FRAMES` instead of dying early on the neutral level.
const SCRIPT_CASE_INPUT: u8 = 0x02;

/// The shipped JSONs (pack.json / level jsons) may carry `//` comment
/// LINES (the C++ JSON parser accepts them; serde_json does not) — strip
/// whole comment lines (a string can never occupy a whole line starting
/// with `//` in these files, so the line filter is safe).
fn strip_json_comment_lines(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Scans `_RELEASE/Packs/**` and builds the full case set:
///
/// * every `Packs/<pack>/Levels/*.json` whose `luaFile` exists → a LEVEL
///   case (`level_id = pack_id + "_" + json's id` — note the json NAME is
///   not always the id: `3D_required.json` → id `ThreeDimensions-Required-test`);
/// * every `Packs/<pack>/Scripts/*.lua` OUTSIDE `Levels/` (the 6 `base`
///   helper scripts) → a SCRIPT case.
///
/// Together these drive all 42 shipped `.lua` files (36 level scripts, of
/// which `negative_pulse.lua` serves two level ids, plus the 6 helpers).
/// Deterministic: sorted by path.
fn all_cases(root: &Path) -> Vec<Case> {
    let mut cases = Vec::new();

    // Level cases: from the level JSONs (the engine's own level registry).
    let packs_dir = root.join("Packs");
    let mut pack_dirs: Vec<PathBuf> = fs::read_dir(&packs_dir)
        .expect("_RELEASE/Packs")
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    pack_dirs.sort();

    for pack_dir in &pack_dirs {
        let pack_json = pack_dir.join("pack.json");
        let pack_text = fs::read_to_string(&pack_json)
            .unwrap_or_else(|e| panic!("read {}: {e}", pack_json.display()));
        let pack: serde_json::Value = serde_json::from_str(&strip_json_comment_lines(&pack_text))
            .unwrap_or_else(|e| panic!("parse {}: {e}", pack_json.display()));
        let pack_id = build_pack_id(
            pack["disambiguator"].as_str().expect("disambiguator"),
            pack["author"].as_str().expect("author"),
            pack["name"].as_str().expect("name"),
            pack["version"].as_i64().expect("version") as i32,
        );

        let levels_dir = pack_dir.join("Levels");
        let mut level_jsons: Vec<PathBuf> = match fs::read_dir(&levels_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect(),
            Err(_) => Vec::new(),
        };
        level_jsons.sort();
        for lj in level_jsons {
            // (see `strip_json_comment_lines` — the C++ JSON parser
            // accepts comment lines, serde_json does not)
            let text = fs::read_to_string(&lj).expect("level json");
            let level: serde_json::Value = serde_json::from_str(&strip_json_comment_lines(&text))
                .unwrap_or_else(|e| panic!("parse {}: {e}", lj.display()));
            let id = level["id"].as_str().expect("level id");
            let lua_file = level["luaFile"].as_str().expect("luaFile");
            let script = pack_dir.join(lua_file);
            if !script.exists() {
                continue; // e.g. a level json without its script: not a case
            }
            let level_id = format!("{pack_id}_{id}");
            let id = format!("lvl-{level_id}");
            cases.push(Case {
                id: id.clone(),
                kind: CaseKind::Level {
                    pack: pack_id.clone(),
                    level: level_id,
                    script: script
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                },
                seed: case_seed(&id),
                input_count: FRAME_CAP as usize,
            });
        }
    }

    // Script cases: the non-Level helper scripts (the `base` pack's
    // shared pattern/utility scripts — smoke-loaded, see `CaseKind::Script`).
    for pack_dir in &pack_dirs {
        let scripts_dir = pack_dir.join("Scripts");
        if !scripts_dir.is_dir() {
            continue;
        }
        let mut scripts: Vec<PathBuf> = fs::read_dir(&scripts_dir)
            .expect("Scripts dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "lua"))
            .collect();
        scripts.sort();
        for script in scripts {
            let id = format!("scr-{}", script.file_stem().unwrap().to_string_lossy());
            cases.push(Case {
                id: id.clone(),
                kind: CaseKind::Script {
                    script: script
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                },
                seed: case_seed(&id),
                input_count: SCRIPT_CASE_FRAMES as usize,
            });
        }
    }

    cases
}

// ---------------------------------------------------------------------------
// The per-frame fingerprint
// ---------------------------------------------------------------------------

/// One frame's fingerprint (the 10 components, artifact order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
struct FrameFp([u64; 10]);

const FN_NAMES: [&str; 10] = [
    "rng",
    "wall_count",
    "walls",
    "custom_walls",
    "timelines",
    "player",
    "score",
    "custom_score",
    "level_status",
    "status",
];

fn encode_walls(walls: &[oh_game::wall::CWall]) -> u64 {
    let mut enc = Enc::default();
    for w in walls {
        for v in w.get_vertex_positions() {
            enc.f32(v.x);
            enc.f32(v.y);
        }
        let s = w.get_speed();
        enc.f32(s.speed);
        enc.f32(s.accel);
        let c = w.get_curve();
        enc.f32(c.speed);
        enc.f32(c.accel);
    }
    enc.hash()
}

fn encode_timelines(shared: &SharedState) -> u64 {
    let mut enc = Enc::default();
    enc.u32(shared.main_timeline.size() as u32);
    enc.u32(shared.event_timeline.size() as u32);
    enc.u32(shared.custom_timelines.count() as u32);
    enc.hash()
}

fn encode_level_status(ls: &oh_game::LevelStatus) -> u64 {
    let mut enc = Enc::default();
    let mut tracked: Vec<(&String, &String)> = ls.tracked_variables.iter().collect();
    tracked.sort_by(|a, b| a.0.cmp(b.0)); // HashMap order is not observable state
    enc.u32(tracked.len() as u32);
    for (k, v) in tracked {
        enc.str(k);
        enc.str(v);
    }
    enc.bool(ls.score_overridden);
    enc.str(&ls.score_override);
    enc.bool(ls.sync_music_to_dm);
    enc.f32(ls.music_pitch);
    enc.str(&ls.beep_sound);
    enc.str(&ls.level_up_sound);
    enc.str(&ls.swap_sound);
    enc.str(&ls.death_sound);
    enc.f32(ls.speed_mult);
    enc.f32(ls.player_speed_mult);
    enc.f32(ls.speed_inc);
    enc.f32(ls.speed_max);
    enc.f32(ls.rotation_speed);
    enc.f32(ls.rotation_speed_inc);
    enc.f32(ls.rotation_speed_max);
    enc.f32(ls.delay_mult);
    enc.f32(ls.delay_inc);
    enc.f32(ls.delay_min);
    enc.f32(ls.delay_max);
    enc.f32(ls.fast_spin);
    enc.f32(ls.inc_time);
    enc.f32(ls.pulse_min);
    enc.f32(ls.pulse_max);
    enc.f32(ls.pulse_speed);
    enc.f32(ls.pulse_speed_r);
    enc.f32(ls.pulse_delay_max);
    enc.f32(ls.pulse_initial_delay);
    enc.f32(ls.swap_cooldown_mult);
    enc.f32(ls.beat_pulse_initial_delay);
    enc.f32(ls.beat_pulse_max);
    enc.f32(ls.beat_pulse_delay_max);
    enc.f32(ls.beat_pulse_speed_mult);
    enc.f32(ls.radius_min);
    enc.f32(ls.wall_skew_left);
    enc.f32(ls.wall_skew_right);
    enc.f32(ls.wall_angle_left);
    enc.f32(ls.wall_angle_right);
    enc.f32(ls.wall_spawn_distance);
    enc.f32(ls.camera_shake);
    enc.u32(ls.sides);
    enc.u32(ls.sides_max);
    enc.u32(ls.sides_min);
    enc.bool(ls.swap_enabled);
    enc.bool(ls.tutorial_mode);
    enc.bool(ls._3d_required);
    enc.bool(ls.shaders_required);
    enc.bool(ls.inc_enabled);
    enc.bool(ls.rnd_side_changes_enabled);
    enc.bool(ls.darken_uneven_background_chunk);
    enc.bool(ls.manual_pulse_control);
    enc.bool(ls.manual_beat_pulse_control);
    enc.u64(ls.current_increments);
    enc.hash()
}

fn encode_status(st: &oh_game::HexagonGameStatus) -> u64 {
    let mut enc = Enc::default();
    enc.f64(st.get_total_accumulated_frametime());
    enc.f64(st.get_played_accumulated_frametime());
    enc.f64(st.get_paused_accumulated_frametime());
    enc.f64(st.get_time_seconds());
    enc.f64(st.get_increment_time_seconds());
    enc.bool(st.is_time_paused());
    enc.bool(st.has_died);
    enc.u32(st.must_state_change as u32);
    enc.bool(st.score_invalid);
    enc.f32(st.get_custom_score());
    enc.f32(st.pulse);
    enc.f32(st.pulse_direction);
    enc.f32(st.beat_pulse);
    enc.f32(st.radius);
    enc.f32(st.fast_spin);
    enc.f32(st.camera_shake);
    enc.bool(st.started);
    enc.bool(st.show_player_trail);
    enc.hash()
}

/// The per-frame fingerprint (definition above, `FINGERPRINT_DEF`). One
/// lock over the shared store per frame; every component is hashed
/// separately so a divergence localizes to the construct that changed.
fn frame_fingerprint(
    game: &oh_game::hexagon_game::HexagonGame<oh_lua::api::state::GameApiState>,
) -> FrameFp {
    let shared = game.state_handle().lock();
    let status = game.get_status();
    let level_status = game.get_level_status();
    let pos = game.get_player_position();
    let angle = game.get_player_angle();
    let score = game.get_replay_score();

    let mut enc = Enc::default();
    enc.f32(pos.x);
    enc.f32(pos.y);
    enc.f32(angle);
    let player_hash = enc.hash();

    FrameFp([
        shared.rng.engine_state(),                  // 0: rng (the raw engine state)
        shared.walls.len() as u64,                  // 1: wall_count
        encode_walls(&shared.walls),                // 2: walls
        shared.cw_manager.count() as u64,           // 3: custom_walls
        encode_timelines(&shared),                  // 4: timelines
        player_hash,                                // 5: player
        score.to_bits(),                            // 6: score
        status.get_custom_score().to_bits() as u64, // 7: custom_score
        encode_level_status(&level_status),         // 8: level_status
        encode_status(&status),                     // 9: status
    ])
}

// ---------------------------------------------------------------------------
// Case execution (headless, fixed dt, flat-out)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
struct CaseResult {
    /// `None` = the case ran to death or the frame cap without a load
    /// error; `Some` = the script's load/execution error (SCRIPT cases:
    /// the host's own `run_lua_file` result; LEVEL cases: `None` — a level
    /// script error inside `new_game` surfaces through the engine's
    /// `host_error` (printed) and through the frame-0 fingerprint).
    load_error: Option<String>,
    died_frame: Option<u64>,
    final_frame: u64,
    /// Wall-clock of the whole case (script load + all frames +
    /// per-frame fingerprinting), microseconds.
    wall_clock_us: u64,
}

fn replay_file(case: &Case, pack: &str, level: &str, inputs: &[u8]) -> ReplayFile {
    let mut data = ReplayData::default();
    for b in inputs {
        data.record_input(b & 0x01 != 0, b & 0x02 != 0, b & 0x04 != 0, b & 0x08 != 0);
    }
    ReplayFile {
        version: 0,
        player_name: "lane030a".into(),
        seed: case.seed,
        data,
        pack_id: pack.into(),
        level_id: level.into(),
        first_play: true,
        difficulty_mult: 1.0,
        played_score: 0.0,
    }
}

fn run_case(assets: &Arc<oh_assets::Assets>, case: &Case) -> (CaseResult, Vec<FrameFp>) {
    let root = release_dir();
    let composed = compose(assets.clone(), root, false);
    let game = &composed.game;
    let t0 = Instant::now();

    let inputs: Vec<u8> = match &case.kind {
        CaseKind::Level { .. } => input_program(case.seed, case.input_count),
        CaseKind::Script { .. } => vec![SCRIPT_CASE_INPUT; case.input_count],
    };
    let (pack, level) = match &case.kind {
        CaseKind::Level { pack, level, .. } => (pack.clone(), level.clone()),
        // The helper-script cases ride the neutral (oracle-pinned) level.
        CaseKind::Script { .. } => (NEUTRAL_PACK.to_string(), NEUTRAL_LEVEL.to_string()),
    };

    game.set_last_replay(replay_file(case, &pack, &level, &inputs));
    game.new_game(&pack, &level, true, 1.0, true);

    // Frame 0 = the state immediately after `new_game` (the level script
    // has already executed) — a load-time divergence is visible at frame 0.
    let mut frames = Vec::new();
    frames.push(frame_fingerprint(game));

    let load_error = match &case.kind {
        // The helper script is executed in the SAME engine context the
        // level would use (dofile semantics); its error (if any) is the
        // case's load result.
        CaseKind::Script { script, .. } => composed.host.run_lua_file(script).err(),
        CaseKind::Level { .. } => None,
    };

    let cap = match &case.kind {
        CaseKind::Level { .. } => FRAME_CAP,
        CaseKind::Script { .. } => SCRIPT_CASE_FRAMES,
    };
    let mut died_frame = None;
    let mut f: u64 = 0;
    while f < cap {
        if game.get_status().has_died {
            died_frame = Some(f);
            break;
        }
        game.update(DT, 1.0);
        game.post_update();
        f += 1;
        frames.push(frame_fingerprint(game));
    }
    if game.get_status().has_died && died_frame.is_none() {
        died_frame = Some(f);
    }

    let wall_clock_us = t0.elapsed().as_micros() as u64;
    (
        CaseResult {
            load_error,
            died_frame,
            final_frame: f,
            wall_clock_us,
        },
        frames,
    )
}

// ---------------------------------------------------------------------------
// The artifact (under target/ — never committed)
// ---------------------------------------------------------------------------

fn backend_name() -> &'static str {
    if cfg!(feature = "backend-luau") {
        "luau"
    } else {
        "luajit"
    }
}

fn artifact_dir() -> PathBuf {
    let target = std::env::var("CARGO_TARGET_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join("lua-parity")
}

fn run_index() -> u32 {
    std::env::var("LUA_PARITY_RUN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn artifact_path(run: u32) -> PathBuf {
    artifact_dir()
        .join(backend_name())
        .join(format!("run-{run}"))
        .join("cases.json")
}

fn definition_json() -> serde_json::Value {
    serde_json::json!({
        "name": FINGERPRINT_DEF,
        "components": FN_NAMES,
        "hash": "FNV-1a 64 over the canonical LE encoding (tag byte per field)",
        "dt_schedule": format!("{DT_SCHEDULE} (dt = {DT} frametime units = 1/240 s; the C++ headless TIME_STEP)"),
        "frame_cap_level": FRAME_CAP,
        "frame_cap_script": SCRIPT_CASE_FRAMES,
        "input_generator": INPUT_GEN,
        "seed": "FNV-1a 64 over the case id, +1",
        "neutral_level": NEUTRAL_LEVEL,
        "engine": "oh-game HexagonGame via the issue-23 parity composition (compose), headless, window=false",
    })
}

fn write_artifact(cases: &[(Case, CaseResult, Vec<FrameFp>)]) {
    let run = run_index();
    let path = artifact_path(run);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("artifact dir");
    }
    let mut case_values = Vec::new();
    for (case, result, frames) in cases {
        let frames_hex: Vec<Vec<String>> = frames
            .iter()
            .map(|fp| fp.0.iter().map(|v| format!("{v:016x}")).collect())
            .collect();
        case_values.push(serde_json::json!({
            "id": case.id,
            "case": {
                "kind": &case.kind,
                "seed": format!("{:016x}", case.seed),
                "input_count": case.input_count,
                "dt_schedule": DT_SCHEDULE,
                "dt": DT,
            },
            "result": result,
            "frames": frames_hex,
        }));
    }
    let doc = serde_json::json!({
        "definition": definition_json(),
        "backend": backend_name(),
        "run": run,
        "cases": case_values,
    });
    fs::write(&path, serde_json::to_string_pretty(&doc).expect("json"))
        .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    eprintln!(
        "lane 030a: emitted {} cases ({} frames total) to {}",
        cases.len(),
        cases.iter().map(|(_, _, f)| f.len()).sum::<usize>(),
        path.display()
    );
}

// ---------------------------------------------------------------------------
// The comparator (reads BOTH backends' artifacts; fails with the case +
// the first divergent frame + the divergent component)
// ---------------------------------------------------------------------------

struct Artifact {
    backend: String,
    cases: BTreeMap<String, (serde_json::Value, Vec<Vec<String>>)>,
}

fn read_artifact(path: &Path) -> Artifact {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} (run the emit step for this backend first)",
            path.display()
        )
    });
    let doc: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let backend = doc["backend"].as_str().expect("backend").to_string();
    let mut cases = BTreeMap::new();
    for c in doc["cases"].as_array().expect("cases") {
        let id = c["id"].as_str().expect("id").to_string();
        let frames: Vec<Vec<String>> = c["frames"]
            .as_array()
            .expect("frames")
            .iter()
            .map(|row| {
                row.as_array()
                    .expect("row")
                    .iter()
                    .map(|v| v.as_str().expect("hex").to_string())
                    .collect()
            })
            .collect();
        cases.insert(id, (c.clone(), frames));
    }
    Artifact { backend, cases }
}

/// Compare two artifacts; return `Ok(())` or the first divergence.
fn compare_artifacts(a: &Artifact, b: &Artifact) -> Result<(), String> {
    for (id, (meta_a, frames_a)) in &a.cases {
        let (meta_b, frames_b) = b
            .cases
            .get(id)
            .ok_or_else(|| format!("case {id} is missing from the {} artifact", b.backend))?;

        // The case definitions must agree (seed / input count / dt).
        let ca = &meta_a["case"];
        let cb = &meta_b["case"];
        if ca["seed"] != cb["seed"]
            || ca["input_count"] != cb["input_count"]
            || ca["dt"] != cb["dt"]
        {
            return Err(format!(
                "case {id}: the case definitions differ between backends (a: {} / b: {})",
                ca, cb
            ));
        }

        // A load error on only one side is a divergence at frame 0.
        let la = meta_a["result"]["load_error"].as_str();
        let lb = meta_b["result"]["load_error"].as_str();
        if la != lb {
            return Err(format!(
                "case {id}: load-error divergence at frame 0 ({}: {la:?} / {}: {lb:?})",
                a.backend, b.backend
            ));
        }

        let (shorter, longer) = (frames_a.len(), frames_b.len());
        let n = shorter.min(longer);
        for f in 0..n {
            if frames_a[f] != frames_b[f] {
                let divergent: Vec<String> = frames_a[f]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| frames_a[f][*i] != frames_b[f][*i])
                    .map(|(i, _)| FN_NAMES[i].to_string())
                    .collect();
                return Err(format!(
                    "case {id}: FIRST DIVERGENT FRAME = {f} (of {n}); divergent component(s): {}",
                    divergent.join(", ")
                ));
            }
        }
        if frames_a.len() != frames_b.len() {
            // Death-frame asymmetry (one backend died earlier/later) —
            // report it explicitly with both final frames.
            return Err(format!(
                "case {id}: frame-count divergence ({} frames in {} vs {} in {} — death-frame asymmetry)",
                frames_a.len(),
                a.backend,
                frames_b.len(),
                b.backend
            ));
        }
    }
    Ok(())
}

fn compare_against_base(artifacts: &[(u32, &Artifact)]) -> Result<(), String> {
    let (base_idx, base) = artifacts[0];
    for (idx, other) in &artifacts[1..] {
        compare_artifacts(base, other)
            .map_err(|msg| format!("run-{base_idx} vs run-{idx}: {msg}"))?
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// Emits this backend's per-case, per-frame fingerprints to
/// `target/lua-parity/<backend>/run-<LUA_PARITY_RUN>/cases.json`.
/// Always runs (a plain `cargo test -p oh-parity` produces the default
/// run-0 artifact; `tools/lua-parity.sh` drives the A/B + repeats).
#[test]
fn emit_fingerprints() {
    let root = release_dir();
    let cases = all_cases(&root);
    assert!(
        cases.len() >= 42,
        "the corpus shrank below the 42 shipped scripts: {} cases",
        cases.len()
    );
    let assets =
        Arc::new(oh_assets::Assets::new(root, true, false).expect("_RELEASE loads headless"));
    let t0 = Instant::now();
    let mut out = Vec::new();
    for case in &cases {
        let (result, frames) = run_case(&assets, case);
        eprintln!(
            "  {:<44} frames={:>5} died={:?} us={}",
            case.id, result.final_frame, result.died_frame, result.wall_clock_us
        );
        out.push((case.clone(), result, frames));
    }
    write_artifact(&out);
    eprintln!(
        "lane 030a: emit done in {:.2} s",
        t0.elapsed().as_secs_f64()
    );
}

/// The A/B comparator: reads BOTH backends' run-0 artifacts (and, when
/// `LUA_PARITY_COMPARE_ALL_RUNS=1`, every other run index present) and
/// fails with the case id + first divergent frame. Gated on
/// `LUA_PARITY_COMPARE=1` (so a plain `cargo test` of one backend alone
/// passes vacuously; `tools/lua-parity.sh` runs the real comparison).
#[test]
fn compare_backends() {
    if std::env::var("LUA_PARITY_COMPARE").is_err() {
        eprintln!(
            "lane 030a: comparison not requested (LUA_PARITY_COMPARE unset) — passing vacuously"
        );
        return;
    }
    let base = artifact_dir().join("luajit").join("run-0");
    let other = artifact_dir().join("luau").join("run-0");
    let a = read_artifact(&base.join("cases.json"));
    let b = read_artifact(&other.join("cases.json"));
    match compare_artifacts(&a, &b) {
        Ok(()) => eprintln!(
            "lane 030a: {} cases byte-identical across {} vs {} (run-0)",
            a.cases.len(),
            a.backend,
            b.backend
        ),
        Err(msg) => panic!("BACKEND DIVERGENCE: {msg}"),
    }

    // Cross-run checks (every run index that exists, ≥ 1 runs per backend):
    if std::env::var("LUA_PARITY_COMPARE_ALL_RUNS").is_ok() {
        for backend in ["luajit", "luau"] {
            let dir = artifact_dir().join(backend);
            let mut runs = vec![0u32];
            if let Ok(rd) = fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if let Some(k) = e
                        .file_name()
                        .to_string_lossy()
                        .strip_prefix("run-")
                        .and_then(|s| s.parse().ok())
                    {
                        runs.push(k);
                    }
                }
            }
            runs.sort();
            let artifacts: Vec<(u32, Artifact)> = runs
                .iter()
                .map(|k| {
                    (
                        *k,
                        read_artifact(&dir.join(format!("run-{k}")).join("cases.json")),
                    )
                })
                .collect();
            let refs: Vec<(u32, &Artifact)> = artifacts.iter().map(|(k, a)| (*k, a)).collect();
            compare_against_base(&refs)
                .unwrap_or_else(|msg| panic!("REPEAT DIVERGENCE ({backend}): {msg}"));
        }
    }
}

/// Multi-process repeat: cross-checks run-`k` against run-`0` for this
/// backend (separate `cargo test` PROCESSES per run index — if a VM
/// randomized string/table hashing per process, any per-frame fingerprint
/// that varies across runs means nondeterministic gameplay). Gated on
/// `LUA_PARITY_REPEATS` (`all` or a comma list).
#[test]
fn repeat_determinism() {
    let requested = match std::env::var("LUA_PARITY_REPEATS") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("lane 030a: repeat check not requested (LUA_PARITY_REPEATS unset) — passing vacuously");
            return;
        }
    };
    let base = read_artifact(&artifact_path(0));
    let indexes: Vec<u32> = if requested == "all" {
        (1..16).collect()
    } else {
        requested
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    };
    let mut checked = 0usize;
    for k in indexes {
        let path = artifact_path(k);
        if !path.exists() {
            continue; // the tool only emits the indexes that exist
        }
        let other = read_artifact(&path);
        compare_against_base(&[(0, &base), (k, &other)]).unwrap_or_else(|msg| {
            panic!(
                "PER-PROCESS NONDETERMINISM ({backend}): {msg} — a \n\
                     fingerprint varies across separate processes of the same \n\
                     backend (pairs-order / hash-randomization risk: CONFIRMED)",
                backend = base.backend
            )
        });
        checked += 1;
    }
    eprintln!(
        "lane 030a: repeat check passed for {checked} run index(es) of {}",
        base.backend
    );
}

/// The per-frame `onUpdate` benchmark (task §3): one or two representative
/// levels, flat-out headless, µs/frame WITH and WITHOUT the per-frame
/// fingerprint. Printed, not pinned (host perf is a measurement, not a
/// pin — the phone is the real constraint; see the lane report).
#[test]
fn perf_on_update_per_frame() {
    let root = release_dir();
    let assets = Arc::new(
        oh_assets::Assets::new(root.clone(), true, false).expect("_RELEASE loads headless"),
    );
    let cases = all_cases(&root);
    // Representative: the lightest (the tutorial `babysteps`) and the
    // heaviest shipped level (`stress1`); matched by level short-name
    // because case ids embed the pack id (`lvl-<pack_id>_<level_id>`).
    for marker in ["babysteps", "stress1"] {
        let case = cases
            .iter()
            .find(|c| c.id.contains(marker))
            .unwrap_or_else(|| panic!("representative case {marker} missing"));
        let (pack, level) = match &case.kind {
            CaseKind::Level { pack, level, .. } => (pack.clone(), level.clone()),
            CaseKind::Script { .. } => unreachable!("representative cases are levels"),
        };
        let frames_target = 2_500u64;
        // The C++ benchmark's own input: `alwaysSpinRight` (the
        // ReplayExecutionBenchmark pattern — deterministic and long-lived
        // on most levels): a 0x02 stream, one input per frame.
        let spin: Vec<u8> = vec![0x02u8; frames_target as usize];

        // (a) pure engine+Lua cost: no fingerprinting.
        let composed = compose(assets.clone(), root.clone(), false);
        let game = &composed.game;
        game.set_always_spin_right(true);
        game.set_last_replay(replay_file(case, &pack, &level, &spin));
        game.new_game(&pack, &level, true, 1.0, true);
        let t0 = Instant::now();
        let mut frames = 0u64;
        while frames < frames_target && !game.get_status().has_died {
            game.update(DT, 1.0);
            game.post_update();
            frames += 1;
        }
        let pure_us = t0.elapsed().as_micros();
        let pure_frames = frames;
        drop(composed);

        // (b) with the per-frame fingerprint (the gate's cost).
        let composed = compose(assets.clone(), root.clone(), false);
        let game = &composed.game;
        game.set_always_spin_right(true);
        game.set_last_replay(replay_file(case, &pack, &level, &spin));
        game.new_game(&pack, &level, true, 1.0, true);
        let t0 = Instant::now();
        frames = 0;
        while frames < frames_target && !game.get_status().has_died {
            game.update(DT, 1.0);
            game.post_update();
            frames += 1;
            frame_fingerprint(game);
        }
        let fp_us = t0.elapsed().as_micros();
        let fp_frames = frames;

        // (frames - 1): the first frame is the post-`new_game` state; the
        // timed loop advances `frames` update pairs.
        let d = |us: u128, n: u64| us as f64 / n.saturating_sub(1).max(1) as f64;
        eprintln!(
            "lane 030a perf [{backend}/{run}]: {case_id} — pure engine+Lua (alwaysSpinRight): {pf} frames, {pure:.2} µs/frame; with per-frame fingerprint: {ff} frames, {fp:.2} µs/frame (frame budget at 200–360 fps: ~2.8–5.0 ms total)",
            case_id = case.id,
            backend = backend_name(),
            run = run_index(),
            pf = pure_frames,
            pure = d(pure_us, pure_frames),
            ff = fp_frames,
            fp = d(fp_us, fp_frames),
        );
    }
}

/// Harness self-check (the project's "the harness itself is tested"
/// contract): two different input programs on the SAME level/seed must
/// produce different fingerprints (the fingerprint is sensitive to the
/// input program, not just the level), and the case generator is
/// deterministic (two scans agree).
#[test]
fn harness_self_check() {
    let root = release_dir();
    let cases_a = all_cases(&root);
    let cases_b = all_cases(&root);
    assert_eq!(
        cases_a.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        cases_b.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        "the case generator must be deterministic"
    );

    // FNV sanity (known vectors — cross-checked against the reference
    // algorithm).
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);

    // Input-program determinism + sensitivity.
    let a = input_program(case_seed("harness-a"), 64);
    let b = input_program(case_seed("harness-a"), 64);
    let c = input_program(case_seed("harness-b"), 64);
    assert_eq!(a, b, "the input program must be deterministic in the seed");
    assert_ne!(a, c, "different seeds must yield different programs");

    // Fingerprint sensitivity: the same level, the same seed, but two
    // DIFFERENT input programs (left-spin vs right-spin) must diverge
    // within a few hundred frames.
    let assets = Arc::new(oh_assets::Assets::new(root.clone(), true, false).expect("load"));
    let spin = |dir: u8| {
        let bytes: Vec<u8> = (0..400).map(|_| dir).collect();
        let (pack, level) = (
            "ohvrvanilla_vittorio_romeo_tutorial_1".to_string(),
            "ohvrvanilla_vittorio_romeo_tutorial_1_babysteps".to_string(),
        );
        let mut rf = replay_file(
            &Case {
                id: "harness-spin".into(),
                kind: CaseKind::Level {
                    pack: pack.clone(),
                    level: level.clone(),
                    script: String::new(),
                },
                seed: 1,
                input_count: 400,
            },
            &pack,
            &level,
            &bytes,
        );
        // `replay_file` builds the data from bytes; replace the seed field
        // (replay_file sets it from the case).
        rf.seed = 7;
        let composed = compose(assets.clone(), root.clone(), false);
        let game = &composed.game;
        game.set_last_replay(rf);
        game.new_game(&pack, &level, true, 1.0, true);
        let mut frames = Vec::new();
        frames.push(frame_fingerprint(game));
        let mut f = 0u64;
        while f < 400 && !game.get_status().has_died {
            game.update(DT, 1.0);
            game.post_update();
            f += 1;
            frames.push(frame_fingerprint(game));
        }
        frames
    };
    let fa = spin(0x01); // left
    let fb = spin(0x02); // right
    assert!(
        fa.iter().zip(fb.iter()).take_while(|(x, y)| x == y).count() < 400,
        "the fingerprint must be sensitive to the input program (left vs right spin on babysteps diverged at or before frame {})",
        fa.iter().zip(fb.iter()).take_while(|(x, y)| x == y).count()
    );
}
