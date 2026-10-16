//! The `Core/LeaderboardCache` port (76 lines + the header).
//!
//! The client-side cache of the server's top-scores / own-score answers,
//! keyed by level validator. What is cached, and for how long:
//!
//! - `receivedScores` replaces the whole ranked-scores vector for the
//!   level and refreshes the cache time;
//! - `receivedOwnScore` stores the single own-score entry and refreshes
//!   the cache time;
//! - `requestedScores` marks "a request was sent/answered just now" for
//!   the level (creating the entry if absent) so the UI doesn't
//!   re-request immediately;
//! - `shouldRequestScores` is true when there is no entry **or** the
//!   cache is older than 6 seconds (`LeaderboardCache.cpp:39-51`);
//! - `getScores` / `getOwnScore` / `hasInformation` read the entry.
//!
//! The staleness clock is the C++ `HRClock`
//! (`Utils/Clock.hpp:14` = `std::chrono::high_resolution_clock` —
//! monotonic). The port funnels every `HRClock::now()` call through an
//! injectable provider (default `std::time::Instant::now`, the same
//! monotonic clock) so the 6-second window is testable without a real
//! sleep — a documented equivalent (the C++ calls the clock inline,
//! `LeaderboardCache.cpp:20, 29, 36, 48`).
//!
//! The C++ header also declares `getSupported`
//! (`LeaderboardCache.hpp:50`) but defines it nowhere (grep-verified
//! dead declaration — never called, never linked); the port omits it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use crate::db_records::ProcessedScore;

/// The C++ `CachedScores` (`LeaderboardCache.hpp:24-28`): the ranked
/// scores, the optional own score, and the cache timestamp.
#[derive(Debug)]
struct CachedScores {
    /// C++ `_scores`.
    scores: Vec<ProcessedScore>,
    /// C++ `_ownScore` (`std::optional`).
    own_score: Option<ProcessedScore>,
    /// C++ `_cacheTime` (`HRTimePoint`).
    cache_time: Instant,
}

impl Default for CachedScores {
    fn default() -> Self {
        Self {
            scores: Vec::new(),
            own_score: None,
            cache_time: Instant::now(),
        }
    }
}

/// C++ `hg::LeaderboardCache` (`Core/LeaderboardCache.hpp`).
pub struct LeaderboardCache {
    /// C++ `_levelValidatorToScores` (`std::unordered_map`).
    level_validator_to_scores: HashMap<String, CachedScores>,
    /// The C++ `HRClock::now()` calls (default: the same monotonic clock).
    now: RefCell<Box<dyn Fn() -> Instant>>,
}

impl LeaderboardCache {
    /// The default cache — the C++ constructor (no members; the map starts
    /// empty and the clock is `HRClock::now()`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            level_validator_to_scores: HashMap::new(),
            now: RefCell::new(Box::new(Instant::now)),
        }
    }

    /// Test seam: a cache whose `HRClock::now()` is a controlled clock.
    pub fn with_clock(now: impl Fn() -> Instant + 'static) -> Self {
        Self {
            level_validator_to_scores: HashMap::new(),
            now: RefCell::new(Box::new(now)),
        }
    }

    fn now(&self) -> Instant {
        (self.now.borrow())()
    }

    /// C++ `receivedScores` (`LeaderboardCache.cpp:18-24`): replace the
    /// level's scores and refresh the cache time (creating the entry if
    /// absent — the C++ `operator[]`).
    pub fn received_scores(&mut self, level_validator: &str, scores: Vec<ProcessedScore>) {
        let now = self.now();
        let cs = self
            .level_validator_to_scores
            .entry(level_validator.to_string())
            .or_default();
        cs.scores = scores;
        cs.cache_time = now;
    }

    /// C++ `receivedOwnScore` (`LeaderboardCache.cpp:26-32`): store the
    /// own-score entry and refresh the cache time.
    pub fn received_own_score(&mut self, level_validator: &str, score: ProcessedScore) {
        let now = self.now();
        let cs = self
            .level_validator_to_scores
            .entry(level_validator.to_string())
            .or_default();
        cs.own_score = Some(score);
        cs.cache_time = now;
    }

    /// C++ `requestedScores` (`LeaderboardCache.cpp:34-38`): refresh the
    /// cache time only (creating the entry if absent) — the caller is
    /// about to send (or just sent) a top-scores request, so the 6-second
    /// window restarts.
    pub fn requested_scores(&mut self, level_validator: &str) {
        let now = self.now();
        let cs = self
            .level_validator_to_scores
            .entry(level_validator.to_string())
            .or_default();
        cs.cache_time = now;
    }

    /// C++ `shouldRequestScores` (`LeaderboardCache.cpp:39-51`): no entry
    /// → `true`; entry age `> 6 s` → `true`; else `false`.
    pub fn should_request_scores(&self, level_validator: &str) -> bool {
        match self.level_validator_to_scores.get(level_validator) {
            None => true,
            Some(cs) => self.now() - cs.cache_time > std::time::Duration::from_secs(6),
        }
    }

    /// C++ `getScores` (`LeaderboardCache.cpp:53-57`): the C++ asserts
    /// `hasInformation` (debug) and then `_levelValidatorToScores.at(...)`
    /// (release: `std::out_of_range` when absent) — the port returns
    /// `None` for the absent entry instead (the callers — `MenuGame` —
    /// always check `has_information` first, `MenuGame.cpp:5608`).
    #[must_use]
    pub fn get_scores(&self, level_validator: &str) -> Option<&Vec<ProcessedScore>> {
        self.level_validator_to_scores
            .get(level_validator)
            .map(|cs| &cs.scores)
    }

    /// C++ `getOwnScore` (`LeaderboardCache.cpp:59-67`): `None` when the
    /// entry is absent (the C++ assert/`.at` fault — see
    /// [`Self::get_scores`]); `Some(None)` when the entry exists but has
    /// no own score (the C++ returns `nullptr`); `Some(Some(&score))`
    /// when set (the C++ returns the pointer).
    #[must_use]
    pub fn get_own_score(&self, level_validator: &str) -> Option<Option<&ProcessedScore>> {
        self.level_validator_to_scores
            .get(level_validator)
            .map(|cs| cs.own_score.as_ref())
    }

    /// C++ `hasInformation` (`LeaderboardCache.cpp:69-73`).
    #[must_use]
    pub fn has_information(&self, level_validator: &str) -> bool {
        self.level_validator_to_scores.contains_key(level_validator)
    }
}

impl Default for LeaderboardCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ps(position: u32, user: &str, value: f64) -> ProcessedScore {
        ProcessedScore {
            position,
            user_name: user.to_string(),
            score_timestamp: 0,
            score_value: value,
        }
    }

    /// A controllable clock: tests advance a shared base `Instant`.
    struct CtrlClock {
        base: std::rc::Rc<std::cell::RefCell<Instant>>,
    }

    impl CtrlClock {
        fn new() -> Self {
            Self {
                base: std::rc::Rc::new(std::cell::RefCell::new(Instant::now())),
            }
        }
        fn advance_secs(&self, secs: u64) {
            *self.base.borrow_mut() += std::time::Duration::from_secs(secs);
        }
        fn now_fn(&self) -> impl Fn() -> Instant {
            let base = std::rc::Rc::clone(&self.base);
            move || *base.borrow()
        }
    }

    #[test]
    fn fresh_cache_wants_a_request() {
        // C++: no entry → shouldRequestScores true (LeaderboardCache.cpp:43-46).
        let cache = LeaderboardCache::new();
        assert!(cache.should_request_scores("lv"));
        assert!(!cache.has_information("lv"));
        assert!(cache.get_scores("lv").is_none());
        assert!(cache.get_own_score("lv").is_none());
    }

    #[test]
    fn requested_scores_resets_the_window() {
        let clock = CtrlClock::new();
        let now_fn = clock.now_fn();
        let mut cache = LeaderboardCache::with_clock(now_fn);

        cache.requested_scores("lv");
        assert!(cache.has_information("lv"));
        assert!(cache.get_scores("lv").is_some(), "entry exists…");
        assert!(
            cache.get_scores("lv").unwrap().is_empty(),
            "…with no scores yet"
        );
        assert!(!cache.should_request_scores("lv"), "just requested");

        // 5 s is still inside the 6-second window.
        clock.advance_secs(5);
        assert!(!cache.should_request_scores("lv"));

        // 7 s is outside it.
        clock.advance_secs(2);
        assert!(cache.should_request_scores("lv"));
    }

    #[test]
    fn boundary_exactly_six_seconds_still_fresh() {
        // C++: `(now - _cacheTime) > std::chrono::seconds(6)` — exactly
        // 6 s is NOT `>`, so at the boundary the cache is still fresh;
        // the next whole-second tick is stale.
        let clock = CtrlClock::new();
        let now_fn = clock.now_fn();
        let mut cache = LeaderboardCache::with_clock(now_fn);
        cache.requested_scores("lv");

        clock.advance_secs(6);
        assert!(!cache.should_request_scores("lv"), "exactly 6 s: not stale");

        clock.advance_secs(1);
        assert!(cache.should_request_scores("lv"), "7 s: stale");
    }

    #[test]
    fn received_scores_replaces_and_refreshes() {
        let clock = CtrlClock::new();
        let now_fn = clock.now_fn();
        let mut cache = LeaderboardCache::with_clock(now_fn);

        cache.received_scores("lv", vec![ps(0, "a", 1.0), ps(1, "b", 0.5)]);
        assert_eq!(cache.get_scores("lv").unwrap().len(), 2);
        assert!(!cache.should_request_scores("lv"));

        // A fresh answer replaces the whole list (the C++ assigns the
        // vector, LeaderboardCache.cpp:21) and refreshes the time.
        clock.advance_secs(5);
        cache.received_scores("lv", vec![ps(0, "c", 9.0)]);
        assert_eq!(cache.get_scores("lv").unwrap(), &vec![ps(0, "c", 9.0)]);
        assert!(
            !cache.should_request_scores("lv"),
            "the answer refreshed the window"
        );

        clock.advance_secs(7);
        assert!(cache.should_request_scores("lv"));
    }

    #[test]
    fn received_own_score_is_optional_per_level() {
        let clock = CtrlClock::new();
        let now_fn = clock.now_fn();
        let mut cache = LeaderboardCache::with_clock(now_fn);

        // Entry without an own score: Some(None) (the C++ nullptr path).
        cache.requested_scores("lv");
        assert_eq!(cache.get_own_score("lv"), Some(None));

        cache.received_own_score("lv", ps(1, "me", 3.0));
        assert_eq!(cache.get_own_score("lv"), Some(Some(&ps(1, "me", 3.0))));

        // A different level is independent.
        assert!(cache.get_own_score("other").is_none());
    }

    #[test]
    fn separate_levels_are_independent() {
        let mut cache = LeaderboardCache::new();
        cache.received_scores("lv1", vec![ps(0, "a", 1.0)]);
        assert!(!cache.should_request_scores("lv1"));
        assert!(cache.should_request_scores("lv2"));
        assert_eq!(cache.get_scores("lv1").unwrap().len(), 1);
        assert!(cache.get_scores("lv2").is_none());
        assert!(cache.has_information("lv1"));
        assert!(!cache.has_information("lv2"));
    }
}
