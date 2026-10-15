//! Port of `include/SSVOpenHexagon/Utils/Ticker.hpp` +
//! `src/SSVOpenHexagon/Utils/Ticker.cpp` — `hg::Ticker`.
//!
//! All fields are `float` (`f32`); `update` semantics copied verbatim (D-000):
//! when a tick reaches the target, the overflow beyond the target is DROPPED
//! (`current` resets to 0, no carry) and `running` is set to `loop`.

use oh_math::ssvu::frametime::get_ft_to_seconds;
use oh_math::FT;

/// `hg::Ticker`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ticker {
    target: f32,
    current: f32,
    total: f32,
    running: bool,
    loop_: bool,
    ticks: usize,
}

impl Ticker {
    /// C++ `Ticker(ssvu::FT mTarget, bool mRunning = true)` — `current`/`total`
    /// are value-initialized to 0, `loop` defaults to `true`.
    #[must_use]
    pub fn new(m_target: FT) -> Self {
        Self::new_running(m_target, true)
    }

    /// `Ticker(mTarget, mRunning)` with an explicit `running` flag
    /// (e.g. `Ticker{80.f, false}`).
    #[must_use]
    pub fn new_running(m_target: FT, m_running: bool) -> Self {
        Self {
            target: m_target,
            current: 0.0,
            total: 0.0,
            running: m_running,
            loop_: true,
            ticks: 0,
        }
    }

    /// C++ `update(ssvu::FT mFT)`.
    ///
    /// `increment = mFT * static_cast<float>(running)`; paused tickers never
    /// advance and never fire. On fire `current` resets to 0 (overflow dropped)
    /// and `running = loop`.
    pub fn update(&mut self, m_ft: FT) -> bool {
        // `increment = mFT * static_cast<float>(running)` (running: bool -> 1.f/0.f).
        let increment = m_ft * if self.running { 1.0f32 } else { 0.0f32 };

        self.current += increment;
        self.total += increment;

        if self.current < self.target {
            return false;
        }

        self.ticks += 1;
        self.reset_current();
        self.running = self.loop_;
        true
    }

    /// C++ `update(ssvu::FT mFT, ssvu::FT mTarget)`.
    pub fn update_target(&mut self, m_ft: FT, m_target: FT) -> bool {
        self.target = m_target;
        self.update(m_ft)
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    pub fn resume(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.reset_current();
        self.pause();
    }

    pub fn restart(&mut self) {
        self.reset_current();
        self.resume();
    }

    pub fn restart_target(&mut self, m_target: FT) {
        self.target = m_target;
        self.restart();
    }

    pub fn reset_current(&mut self) {
        self.current = 0.0;
    }

    pub fn reset_ticks(&mut self) {
        self.ticks = 0;
    }

    pub fn reset_total(&mut self) {
        self.total = 0.0;
    }

    pub fn reset_all(&mut self) {
        self.reset_current();
        self.reset_ticks();
        self.reset_total();
    }

    pub fn set_loop(&mut self, m_x: bool) {
        self.loop_ = m_x;
    }

    #[must_use]
    pub fn get_loop(&self) -> bool {
        self.loop_
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    #[must_use]
    pub fn get_target(&self) -> f32 {
        self.target
    }

    #[must_use]
    pub fn get_current(&self) -> f32 {
        self.current
    }

    #[must_use]
    pub fn get_total(&self) -> f32 {
        self.total
    }

    #[must_use]
    pub fn get_ticks(&self) -> usize {
        self.ticks
    }

    /// C++ `getTotalSecs<FT>()` — `getFTToSeconds(total)` = `total / 60.f`.
    #[must_use]
    pub fn get_total_secs(&self) -> f32 {
        get_ft_to_seconds(self.total)
    }

    /// C++ `getCurrentSecs<FT>()` — `getFTToSeconds(current)`.
    #[must_use]
    pub fn get_current_secs(&self) -> f32 {
        get_ft_to_seconds(self.current)
    }
}
