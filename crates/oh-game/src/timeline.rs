//! Port of `include/SSVOpenHexagon/Utils/Timeline2.hpp` +
//! `src/SSVOpenHexagon/Utils/Timeline2.cpp` — `hg::Utils::timeline2` and
//! `timeline2_runner`.
//!
//! The `clock` is `std::chrono::high_resolution_clock`; a single `wait_for`
//! may be "in flight" at a time (tracked by the runner's `_wait_start_tp`).
//! The runner processes actions sequentially and halts at the first `waiting`
//! action.

use crate::clock::HRTimePoint;

/// `std::chrono::milliseconds` analogue.
type Duration = std::time::Duration;

/// `Utils::action` (the `std::variant` of the four action kinds).
pub enum Action {
    /// `action_do { std::function<void()> }`.
    Do(Box<dyn Fn() + Send + 'static>),
    /// `action_wait_for { duration }`.
    WaitFor(Duration),
    /// `action_wait_until { time_point }`.
    WaitUntil(HRTimePoint),
    /// `action_wait_until_fn { std::function<time_point()> }`.
    WaitUntilFn(Box<dyn Fn() -> HRTimePoint + Send + 'static>),
}

/// `class timeline2` — an ordered list of actions.
#[derive(Default)]
pub struct Timeline2 {
    actions: Vec<Action>,
}

impl Timeline2 {
    /// Empty timeline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// `clear()` — `_actions.clear()`.
    pub fn clear(&mut self) {
        self.actions.clear();
    }

    /// `append_do(func)`.
    pub fn append_do(&mut self, func: impl Fn() + Send + 'static) {
        self.actions.push(Action::Do(Box::new(func)));
    }

    /// `append_wait_for(duration)`.
    pub fn append_wait_for(&mut self, d: Duration) {
        self.actions.push(Action::WaitFor(d));
    }

    /// `append_wait_for_seconds(s)` — `append_wait_for(milliseconds((int)(s *
    /// 1000.0)))`; the `(int)` cast TRUNCATES the millisecond count.
    pub fn append_wait_for_seconds(&mut self, s: f64) {
        let ms = (s * 1000.0) as i64;
        self.append_wait_for(Duration::from_millis(ms as u64));
    }

    /// `append_wait_for_sixths(s)` — `append_wait_for_seconds(s / 60.0)`.
    pub fn append_wait_for_sixths(&mut self, s: f64) {
        self.append_wait_for_seconds(s / 60.0);
    }

    /// `append_wait_until(time_point)`.
    pub fn append_wait_until(&mut self, tp: HRTimePoint) {
        self.actions.push(Action::WaitUntil(tp));
    }

    /// `append_wait_until_fn(fn)`.
    pub fn append_wait_until_fn(&mut self, tp_fn: impl Fn() -> HRTimePoint + Send + 'static) {
        self.actions.push(Action::WaitUntilFn(Box::new(tp_fn)));
    }

    /// `size()`.
    #[must_use]
    pub fn size(&self) -> usize {
        self.actions.len()
    }

    /// `action_at(i)` — `assert(i < size); _actions[i]`.
    pub fn action_at(&mut self, i: usize) -> &mut Action {
        debug_assert!(i < self.actions.len());
        &mut self.actions[i]
    }
}

/// `timeline2_runner::outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Proceed,
    Waiting,
    Finished,
}

/// `class timeline2_runner` — tracks one active `wait_for` at a time.
#[derive(Debug, Default)]
pub struct Timeline2Runner {
    current_idx: usize,
    wait_start_tp: Option<HRTimePoint>,
}

impl Timeline2Runner {
    /// Fresh runner (C++ value-init / `runner = {}`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_idx: 0,
            wait_start_tp: None,
        }
    }

    /// C++ `outcome update(timeline2& timeline, const time_point tp)`.
    pub fn update(&mut self, timeline: &mut Timeline2, tp: HRTimePoint) -> Outcome {
        if self.current_idx >= timeline.size() {
            return Outcome::Finished;
        }

        while self.current_idx < timeline.size() {
            let o = match timeline.action_at(self.current_idx) {
                Action::Do(f) => {
                    f();
                    Outcome::Proceed
                }
                Action::WaitFor(d) => {
                    let start = match self.wait_start_tp {
                        None => {
                            self.wait_start_tp = Some(tp);
                            tp
                        }
                        Some(s) => s,
                    };
                    let elapsed = tp.saturating_sub(start);
                    if elapsed < *d {
                        return Outcome::Waiting;
                    }
                    self.wait_start_tp = None;
                    Outcome::Proceed
                }
                Action::WaitUntil(t) => {
                    if tp < *t {
                        return Outcome::Waiting;
                    }
                    Outcome::Proceed
                }
                Action::WaitUntilFn(f) => {
                    if tp < f() {
                        return Outcome::Waiting;
                    }
                    Outcome::Proceed
                }
            };

            if o == Outcome::Proceed {
                self.current_idx += 1;
                continue;
            }
            if o == Outcome::Waiting {
                return o;
            }
            // Only Proceed / Waiting are produced above.
            unreachable!()
        }

        Outcome::Finished
    }
}
