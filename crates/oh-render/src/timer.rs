//! C++ `ssvs::TimerStatic` — the fixed-timestep accumulation timer
//! (`extlibs/SSVStart/include/SSVStart/GameSystem/Timers/TimerStatic.hpp`),
//! ported for the windowed game loop (issue 15 step (c)).
//!
//! Fidelity notes (D-014 — verified against the header line by line):
//!
//! * `runUpdate()` (`TimerStatic.hpp:36-47`) is `loops = 0; time +=
//!   frameTime; while(time >= timeSlice && loops < maxLoops) {
//!   updateFromTimer(step); time -= timeSlice; ++loops; }` — the loop
//!   RESTARTS `loops` every call (it counts "ticks this frame", not a
//!   running total), and `time` is NOT reset between calls (the leftover
//!   sub-slice carries over).
//! * All time values are C++ `ssvu::FT` = floats in *frametime units*,
//!   60 per second (`Frametime.hpp:27` `secondsFTRatio{60.f}`);
//!   `run_frame_time` converts wall-clock seconds to FT with
//!   `getSecondsToFT` (`Frametime.hpp:38-41`), exactly as
//!   `TimerBase::runFrameTime` does (`TimerBase.hpp:40-43`, from its
//!   `sf::Clock` member `TimerBase.hpp:21`).
//! * `max_loops` is a C++ `float` (`TimerStatic.hpp:19`) defaulting to
//!   50 (`:24`); `loops` is float-valued (`:19`).
//! * The C++ timer holds a `GameEngine&` and calls
//!   `gameEngine.updateFromTimer(step)` directly (`TimerStatic.hpp:20,41`).
//!   The port takes that engine call as a per-call `FnMut(f32)` (the
//!   engine's `updateFromTimer`, which routes to the CURRENT game state's
//!   `update(ft)` — `GameEngine.hpp:40-50`): same call, same argument,
//!   same loop; the timer stays a pure accumulator and the game-state
//!   switching lives in the composition (D-013/D-020: `oh-render` must
//!   not depend on the game/menu crates).
//! * The C++ `GameTimer::refresh` timer-swap (`GameTimer.hpp:26-37`) is
//!   NOT ported: `mainClient` installs the timer exactly once before
//!   `run()` (`main.cpp:330-331`), so no next-impl swap can ever happen.
//! * The C++ `TimerBase::runDraw` (`TimerBase.inl:10-13` →
//!   `GameEngine::drawFromTimer` `GameEngine.hpp:52-56`) is the loop's
//!   per-frame draw — the composition's state routing performs it; the
//!   timer itself only accumulates.

use std::time::Instant;

/// C++ `ssvs::TimerStatic` (TimerStatic.hpp:14-89).
#[derive(Debug)]
pub struct TimerStatic {
    step: f32,
    time_slice: f32,
    time: f32,
    max_loops: f32,
    loops: f32,
    /// C++ `frameTime` (TimerBase.hpp:22) — set by `run_frame_time` from
    /// the monotonic clock (the C++ `sf::Clock` member).
    frame_time: f32,
    /// C++ `fps` (TimerBase.hpp:23) — set by `run_fps`.
    fps: f32,
    /// C++ `sf::Clock clock` (TimerBase.hpp:21) — default-constructed with
    /// the timer (the C++ member's lifetime starts at `TimerStatic`
    /// construction, so the first `run_frame_time` measures from there).
    last: Instant,
}

impl TimerStatic {
    /// C++ `TimerStatic(mGameEngine, mStep = 1.f, mTimeSlice = 1.f,
    /// mMaxLoops = 50.f)` (TimerStatic.hpp:24-29).
    #[must_use]
    pub fn new(step: f32, time_slice: f32, max_loops: f32) -> Self {
        Self {
            step,
            time_slice,
            time: 0.0,
            max_loops,
            loops: 0.0,
            frame_time: 0.0,
            fps: 0.0,
            last: Instant::now(),
        }
    }

    /// C++ `TimerBase::runFrameTime` (TimerBase.hpp:40-43):
    /// `frameTime = getSecondsToFT(clock.restart().asSeconds())` — seconds
    /// → FT (`× 60`, Frametime.hpp:27,40).
    pub fn run_frame_time(&mut self) {
        let now = Instant::now();
        let secs = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        self.frame_time = secs * 60.0; // ssvu::getSecondsToFT
    }

    /// C++ `TimerStatic::runUpdate` (TimerStatic.hpp:36-47).
    /// `update_from_timer` is the C++ `gameEngine.updateFromTimer` call
    /// (GameEngine.hpp:40-50) — it routes to the current game state's
    /// `update(mFT)`.
    pub fn run_update(&mut self, mut update_from_timer: impl FnMut(f32)) {
        self.loops = 0.0;
        self.time += self.frame_time;

        while self.time >= self.time_slice && self.loops < self.max_loops {
            update_from_timer(self.step);
            self.time -= self.time_slice;
            self.loops += 1.0;
        }
    }

    /// C++ `TimerBase::runFPS` (TimerBase.hpp:45-48):
    /// `fps = getFTToFPS(frameTime)` = 60 / frameTime (Frametime.hpp:45-48).
    /// `frameTime == 0` yields `+inf`, exactly like the C++ float division.
    pub fn run_fps(&mut self) {
        self.fps = 60.0 / self.frame_time;
    }

    /// C++ `TimerStatic::reset` (TimerStatic.hpp:31-34): zeroes both
    /// `time` and `loops` (NOT `frameTime` — the C++ touches only the two).
    pub fn reset(&mut self) {
        self.time = 0.0;
        self.loops = 0.0;
    }

    /// C++ `setStep` (TimerStatic.hpp:49-52).
    pub fn set_step(&mut self, step: f32) {
        self.step = step;
    }

    /// C++ `setTimeSlice` (TimerStatic.hpp:52-55).
    pub fn set_time_slice(&mut self, time_slice: f32) {
        self.time_slice = time_slice;
    }

    /// C++ `setMaxLoops` (TimerStatic.hpp:57-60).
    pub fn set_max_loops(&mut self, max_loops: f32) {
        self.max_loops = max_loops;
    }

    /// C++ `getStep` (TimerStatic.hpp:62-65).
    #[must_use]
    pub fn get_step(&self) -> f32 {
        self.step
    }

    /// C++ `getTimeSlice` (TimerStatic.hpp:67-70).
    #[must_use]
    pub fn get_time_slice(&self) -> f32 {
        self.time_slice
    }

    /// C++ `getTime` (TimerStatic.hpp:72-75).
    #[must_use]
    pub fn get_time(&self) -> f32 {
        self.time
    }

    /// C++ `getMaxLoops` (TimerStatic.hpp:77-80).
    #[must_use]
    pub fn get_max_loops(&self) -> f32 {
        self.max_loops
    }

    /// C++ `getLoops` (TimerStatic.hpp:82-85) — the ticks fired by the
    /// LAST `run_update` call.
    #[must_use]
    pub fn get_loops(&self) -> f32 {
        self.loops
    }

    /// C++ `getFrameTime` (TimerBase.hpp:53-56).
    #[must_use]
    pub fn get_frame_time(&self) -> f32 {
        self.frame_time
    }

    /// C++ `getFPS` (TimerBase.hpp:58-61).
    #[must_use]
    pub fn get_fps(&self) -> f32 {
        self.fps
    }

    /// Test seam: the C++ tests write `frameTime` (a protected `TimerBase`
    /// member) directly; production frames use `run_frame_time` (the real
    /// monotonic clock — no fixed frame time is invented).
    pub fn set_frame_time(&mut self, frame_time: f32) {
        self.frame_time = frame_time;
    }
}

impl Default for TimerStatic {
    /// C++ NSDMIs + defaulted args (TimerStatic.hpp:19,22-24): step 1.f,
    /// timeSlice 1.f, maxLoops 50.f.
    fn default() -> Self {
        Self::new(1.0, 1.0, 50.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header defaults (TimerStatic.hpp:22-24): step 1.f, timeSlice
    /// 1.f, maxLoops 50.f; fresh timer: time 0, loops 0.
    #[test]
    fn header_defaults() {
        let t = TimerStatic::default();
        assert_eq!(t.get_step(), 1.0);
        assert_eq!(t.get_time_slice(), 1.0);
        assert_eq!(t.get_max_loops(), 50.0);
        assert_eq!(t.get_loops(), 0.0);
        assert_eq!(t.get_time(), 0.0);
        assert_eq!(t.get_frame_time(), 0.0);
        assert_eq!(t.get_fps(), 0.0);
    }

    /// One frame of exactly one slice → exactly one update at `step`
    /// (TimerStatic.hpp:36-47 with the game's step/slice:
    /// TIME_STEP = TIME_SLICE = 60/240, Config.hpp:23-25).
    #[test]
    fn one_slice_one_update() {
        let mut t = TimerStatic::new(0.25, 0.25, 50.0);
        t.set_frame_time(0.25);
        let mut n = 0;
        let mut seen = 0.0;
        t.run_update(|ft| {
            n += 1;
            seen = ft;
        });
        assert_eq!(n, 1, "one slice → one update");
        assert_eq!(seen, 0.25, "the update gets `step`, verbatim");
        assert_eq!(t.get_loops(), 1.0);
        assert!(t.get_time() < f32::EPSILON, "the slice is fully consumed");
    }

    /// Less than one slice → NO update, and the leftover time carries into
    /// the next frame (TimerStatic.hpp:37-44: `time` accumulates, `time -=
    /// timeSlice` only inside the loop).
    #[test]
    fn sub_slice_frame_accumulates_without_updating() {
        let mut t = TimerStatic::new(0.25, 0.25, 50.0);
        t.set_frame_time(0.10);
        let mut n = 0;
        t.run_update(|_| n += 1);
        assert_eq!(n, 0);
        assert!((t.get_time() - 0.10).abs() < f32::EPSILON);

        // Next frame adds up to a full slice → exactly one update.
        t.set_frame_time(0.20);
        t.run_update(|_| n += 1);
        assert_eq!(n, 1);
        assert!((t.get_time() - 0.05).abs() < 1e-6, "0.1 + 0.2 − 0.25");
    }

    /// A frame larger than one slice fires several updates in order, each
    /// at `step` (the while-loop of TimerStatic.hpp:39-44).
    #[test]
    fn multi_slice_frame_fires_in_order() {
        let mut t = TimerStatic::new(0.25, 0.25, 50.0);
        t.set_frame_time(0.90); // 3.6 slices
        let mut fired = Vec::new();
        t.run_update(|ft| fired.push(ft));
        assert_eq!(fired, vec![0.25, 0.25, 0.25]);
        assert_eq!(t.get_loops(), 3.0);
        assert!((t.get_time() - 0.15).abs() < 1e-6, "0.9 - 3×0.25");
    }

    /// The maxLoops cap (TimerStatic.hpp:19,39): a huge backlog fires at
    /// most `maxLoops` updates and KEEPS the unprocessed remainder in
    /// `time` (C++ default `maxLoops = 50.f`).
    #[test]
    fn max_loops_caps_the_tick_burst_and_keeps_the_remainder() {
        let mut t = TimerStatic::new(1.0, 1.0, 50.0);
        t.set_frame_time(100.0);
        let mut n = 0;
        t.run_update(|_| n += 1);
        assert_eq!(n, 50, "the C++ default cap is 50");
        assert_eq!(t.get_loops(), 50.0);
        assert!(
            (t.get_time() - 50.0).abs() < f32::EPSILON,
            "50 slices consumed, 50 remain"
        );

        // The remainder drains on the next frame, still capped.
        t.set_frame_time(0.0);
        t.run_update(|_| n += 1);
        assert_eq!(n, 100);
        assert!(t.get_time() < f32::EPSILON);
    }

    /// `loops` RESTARTS every `runUpdate` call (TimerStatic.hpp:36) —
    /// `getLoops` reports the last frame's tick count, not a total.
    #[test]
    fn loops_restarts_each_frame() {
        let mut t = TimerStatic::new(1.0, 1.0, 50.0);
        t.set_frame_time(3.0);
        t.run_update(|_| ());
        assert_eq!(t.get_loops(), 3.0);
        t.set_frame_time(1.0);
        t.run_update(|_| ());
        assert_eq!(t.get_loops(), 1.0, "per-frame count, not cumulative");
    }

    /// `reset()` zeroes BOTH `time` and `loops` (TimerStatic.hpp:31-34) —
    /// and nothing else: step/slice/maxLoops/frameTime survive.
    #[test]
    fn reset_zeroes_time_and_loops_only() {
        let mut t = TimerStatic::new(0.5, 0.25, 7.0);
        t.set_frame_time(1.0);
        t.run_update(|_| ());
        assert_eq!(t.get_loops(), 4.0);
        t.reset();
        assert_eq!(t.get_time(), 0.0);
        assert_eq!(t.get_loops(), 0.0);
        assert_eq!(t.get_step(), 0.5);
        assert_eq!(t.get_time_slice(), 0.25);
        assert_eq!(t.get_max_loops(), 7.0);
        assert_eq!(t.get_frame_time(), 1.0, "reset does not touch frameTime");
    }

    /// `run_fps` = 60 / frameTime (TimerBase.hpp:45-48;
    /// Frametime.hpp:45-48 `getFTToFPS`).
    #[test]
    fn fps_is_60_over_frame_time() {
        let mut t = TimerStatic::default();
        t.set_frame_time(1.0); // 60 FT/s → 1 FT/frame = 60 fps
        t.run_fps();
        assert_eq!(t.get_fps(), 60.0);
        t.set_frame_time(0.5);
        t.run_fps();
        assert_eq!(t.get_fps(), 120.0);
    }

    /// The monotonic-clock path: `run_frame_time` measures real wall time
    /// (the C++ `clock.restart()`) in FT — 60 per second, so a 20 ms sleep
    /// is ≈ 1.2 FT. (Values are bounded, not pinned: wall-clock dependent.)
    #[test]
    fn run_frame_time_uses_the_monotonic_clock() {
        let mut t = TimerStatic::default();
        t.run_frame_time(); // since construction: ~0
        std::thread::sleep(std::time::Duration::from_millis(20));
        t.run_frame_time();
        assert!(
            t.get_frame_time() > 0.5,
            "20 ms ≈ 1.2 FT; got {}",
            t.get_frame_time()
        );
        // The clock restarts each call: an immediate second call sees ~0.
        t.run_frame_time();
        assert!(t.get_frame_time() < 1.0, "the interval restarted");
    }
}
