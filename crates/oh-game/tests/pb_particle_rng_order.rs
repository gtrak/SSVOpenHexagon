//! Issue 029m row 29d2: the PB (personal-best) particle's cosmetic-RNG draw
//! order must match the C++ `makePBParticle`
//! (`src/SSVOpenHexagon/Core/HGUpdate.cpp:944-955`): ONE `pcg32_fast`
//! stream, drawn in the order
//!
//!     x → rotation → scale → alpha (int) → vx → vy → angVel
//!
//! (y is the fixed `-64.f`; the color base is the RNG-free
//! `getColorMain()` read). The port originally drew alpha FIRST, so every
//! particle took the C++'s 4th stream value as its x and the 1st as its
//! alpha — while the in-code comment documented the CORRECT C++ order. The
//! code now matches the comment.
//!
//! No replay-determinism impact (`cosmetic_rng` is a separate stream from
//! the game `rng`); the test pins the stream-to-field mapping with a
//! call-indexing `CosmeticRng` double so a future re-shuffle fails.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use oh_config::TIME_STEP;
use oh_game::seams::{CosmeticRng, NoJoystick, NullReplayStore};

use common::{compose_with, release_dir, TestProfiles};

const PACK: &str = "ohvrvanilla_vittorio_romeo_cube_1";
const LEVEL: &str = "ohvrvanilla_vittorio_romeo_cube_1_apeirogon";

/// The call index (1, 2, 3, ...) as each draw's value: the C++ 7-draw block
/// must therefore land stream value #1 in position x, #2 in rotation, #3
/// in scale, #4 (the lone int draw) in alpha, and #5-#7 in velocity x / y
/// and angular velocity. Under the pre-fix order (alpha first) the fields
/// would be alpha=1, x=2, rotation=3, scale=4, ... — a different field for
/// every value, so any re-shuffle of the draws fails here.
#[derive(Debug)]
struct IndexRng {
    calls: AtomicUsize,
}

impl IndexRng {
    fn next(&self) -> usize {
        self.calls.fetch_add(1, Ordering::SeqCst) + 1
    }
}

impl CosmeticRng for IndexRng {
    fn get_rnd_r(&self, _min: f32, _max: f32) -> f32 {
        self.next() as f32
    }
    fn get_rnd_i(&self, _min: i32, _max: i32) -> i32 {
        self.next() as i32
    }
}

#[test]
fn pb_particle_draws_follow_the_cpp_stream_order() {
    let assets = Arc::new(
        oh_assets::Assets::new(release_dir(), true, false).expect("_RELEASE loads headless"),
    );
    let rng = Arc::new(IndexRng {
        calls: AtomicUsize::new(0),
    });
    // Windowed: `updateParticles` is window-gated (HGUpdate.cpp:362-384),
    // and the PB path (`death_saveScoreIfNeeded`) requires the C++'s
    // `window != nullptr` too.
    let c = compose_with(
        assets,
        release_dir(),
        true,
        Arc::new(TestProfiles),
        Arc::new(NullReplayStore),
        Arc::new(NoJoystick),
        rng,
    );

    c.game.new_game(PACK, LEVEL, true, 1.0, false);
    c.game.set_must_start(true);
    c.game.update(TIME_STEP, 1.0); // start

    // The C++ `resetTime()` seeds `currentPause = 0.1 * 60.0` (HGStatus.cpp)
    // — while the pause is live, each `accumulateFrametime` call is counted
    // ENTIRELY as paused (an over-run is dropped), so the played score (and
    // hence the personal-best) stays 0 until the pause drains. Advancing the
    // status through its public accumulator in frame-sized steps is the
    // test-set-up equivalent of "the player survived past the pause" (the
    // level's walls would kill an idle player long before).
    for _ in 0..40 {
        c.game
            .state_handle()
            .lock()
            .status
            .accumulate_frametime(0.25);
    }
    assert!(
        c.game.get_status().get_time_seconds() > 0.0,
        "the played score must be non-zero after the start-pause drains"
    );
    assert!(!c.game.must_spawn_pb_particles(), "no PB before the death");

    // C++ `death` → `death_saveScoreIfNeededAndShowPBEffects`: the score
    // beats the profile's local 0.0 → PersonalBest → the C++ arms
    // `mustSpawnPBParticles` (port: `hexagon_game.rs`
    // `death_save_score_if_needed_and_show_pb_effects`).
    c.game.perform_player_kill();
    assert!(
        c.game.must_spawn_pb_particles(),
        "the personal-best path must arm the PB-particle spawn"
    );

    // C++ `updateParticles` (window-gated, and OUTSIDE the `!hasDied`
    // block — HGUpdate.cpp:362-384): consumes the flag;
    // `nextPBParticleSpawn` starts at 0, so the C++ `makePBParticle`
    // lambda runs this frame.
    c.game.update(TIME_STEP, 1.0);

    let particles = c.game.particles();
    assert_eq!(particles.len(), 1, "exactly one PB particle this frame");
    let p = &particles[0];

    // The C++ draw order (HGUpdate.cpp:944-955) — stream value N goes to
    // field N:
    assert_eq!(
        p.sprite.position.x, 1.0_f32,
        "draw #1 (getRndR -64..width+64) -> position x"
    );
    assert_eq!(
        p.sprite.position.y, -64.0_f32,
        "y is the fixed -64.f (not a draw)"
    );
    assert_eq!(
        p.sprite.rotation_deg, 2.0_f32,
        "draw #2 (getRndR 0..360) -> rotation"
    );
    assert_eq!(
        p.sprite.scale.x, 3.0_f32,
        "draw #3 (getRndR 0.75..1.35) -> scale x"
    );
    assert_eq!(
        p.sprite.scale.y, 3.0_f32,
        "the C++ scales both axes by one draw"
    );
    assert_eq!(
        p.sprite.color.a, 4_u8,
        "draw #4 (the lone getRndI 90..145) -> alpha"
    );
    assert_eq!(
        p.velocity.x, 5.0_f32,
        "draw #5 (getRndR -12..12) -> velocity x"
    );
    assert_eq!(
        p.velocity.y, 6.0_f32,
        "draw #6 (getRndR 4..18) -> velocity y"
    );
    assert_eq!(
        p.angular_velocity, 7.0_f32,
        "draw #7 (getRndR -6..6) -> angularVelocity"
    );
}
