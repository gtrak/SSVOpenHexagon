//! Port of `include/SSVOpenHexagon/Core/Clock.hpp` — the high-resolution
//! clock / time-point aliases the game uses for game-time markers.
//!
//! C++:
//! ```cpp
//! using HRClock = std::chrono::high_resolution_clock;
//! using HRTimePoint = std::chrono::time_point<HRClock>;
//! ```
//!
//! In the game these *time points* are only ever constructed from a
//! millisecond duration (`HRTimePoint{std::chrono::milliseconds{N}}`),
//! compared with `<`, and subtracted from one another. A `std::time::Duration`
//! (a millis-since-epoch marker) carries exactly the same values and supports
//! both operations, so it stands in for `HRTimePoint`.

/// `HRTimePoint` analogue: a millisecond-since-epoch marker.
pub type HRTimePoint = std::time::Duration;
