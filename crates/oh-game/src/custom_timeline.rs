//! Port of `include/SSVOpenHexagon/Core/CustomTimeline.hpp`,
//! `CustomTimelineHandle.hpp`, `CustomTimelineManager.hpp` +
//! `src/SSVOpenHexagon/Core/CustomTimelineManager.cpp`.
//!
//! Handles are plain indices into a growing `Vec` (no free-list, unlike
//! `CCustomWallManager`). `create` appends and returns the new index. When a
//! timeline finishes, `updateAllTimelines` clears it and value-resets the
//! runner. Timelines run on the wall-clock `HRTimePoint` (from `Clock.hpp`).

use crate::clock::HRTimePoint;
use crate::timeline::{Outcome, Timeline2, Timeline2Runner};

/// `CustomTimelineHandle` == `int`.
pub type CustomTimelineHandle = i32;

/// `hg::CustomTimeline` — a timeline paired with its runner.
pub struct CustomTimeline {
    timeline: Timeline2,
    runner: Timeline2Runner,
}

impl CustomTimeline {
    /// Mutable access to the inner `Timeline2` (the Lua `ct_*` API appends
    /// deferred actions here — issue 007).
    pub fn timeline_mut(&mut self) -> &mut Timeline2 {
        &mut self.timeline
    }
}

/// `hg::CustomTimelineManager`.
#[derive(Default)]
pub struct CustomTimelineManager {
    timelines: Vec<CustomTimeline>,
}

impl CustomTimelineManager {
    /// `CustomTimelineManager()` (default).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `isHandleValid(h)` — `h >= 0 && h < timelines.size()`. Exposed for the
    /// Lua `ct_*` API handle validation (issue 007 logs + no-ops invalid
    /// handles exactly like the C++ `checkHandle` helper).
    #[must_use]
    pub fn is_handle_valid(&self, h: CustomTimelineHandle) -> bool {
        h >= 0 && h < self.timelines.len() as CustomTimelineHandle
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.timelines.clear();
    }

    /// `updateAllTimelines(tp)` — advances every timeline; a finished one is
    /// cleared and its runner reset.
    pub fn update_all_timelines(&mut self, tp: HRTimePoint) {
        for t in self.timelines.iter_mut() {
            if t.runner.update(&mut t.timeline, tp) == Outcome::Finished {
                t.timeline.clear();
                t.runner = Timeline2Runner::new();
            }
        }
    }

    /// `create()` — appends a fresh timeline and returns its index.
    pub fn create(&mut self) -> CustomTimelineHandle {
        self.timelines.push(CustomTimeline {
            timeline: Timeline2::new(),
            runner: Timeline2Runner::new(),
        });
        let h = self.timelines.len() as CustomTimelineHandle - 1;
        debug_assert!(self.is_handle_valid(h));
        h
    }

    /// `get(h)` (mutable).
    pub fn get(&mut self, h: CustomTimelineHandle) -> &mut CustomTimeline {
        debug_assert!(self.is_handle_valid(h));
        &mut self.timelines[h as usize]
    }

    /// Port-side test accessor (lane 030a's differential-simulation harness):
    /// `timelines.size()`, the live custom-timeline entity count.
    #[must_use]
    pub fn count(&self) -> usize {
        self.timelines.len()
    }
}
