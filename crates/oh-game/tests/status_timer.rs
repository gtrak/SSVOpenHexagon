//! Status-timer pause/resume/accumulate tests (task spec verification item 4).

use oh_game::HexagonGameStatus;

#[test]
fn fresh_status_has_initial_pause() {
    let s = HexagonGameStatus::new();
    // currentPause is re-primed to 0.1 * 60 = 6.0 FT (a 0.1 s initial pause).
    assert!(s.is_time_paused());
    assert!((s.get_total_accumulated_frametime() - 0.0).abs() < 1e-12);
}

#[test]
fn pause_resume_and_accumulate() {
    let mut s = HexagonGameStatus::new(); // currentPause = 6.0

    s.accumulate_frametime(1.0); // total 1.0; pause 6 -> 5 (no played time)
    assert!((s.get_total_accumulated_frametime() - 1.0).abs() < 1e-9);
    assert!((s.get_played_accumulated_frametime() - 0.0).abs() < 1e-9);
    assert!(s.is_time_paused());

    s.accumulate_frametime(7.0); // total 8.0; pause 5 -> -2 (over-run DROPPED)
    assert!((s.get_total_accumulated_frametime() - 8.0).abs() < 1e-9);
    assert!((s.get_played_accumulated_frametime() - 0.0).abs() < 1e-9);
    assert!(!s.is_time_paused());

    s.accumulate_frametime(1.0); // total 9.0; now playing: played = 1.0
    assert!((s.get_total_accumulated_frametime() - 9.0).abs() < 1e-9);
    assert!((s.get_played_accumulated_frametime() - 1.0).abs() < 1e-9);
    // time seconds = played FT / 60.
    assert!((s.get_time_seconds() - 1.0 / 60.0).abs() < 1e-12);
    // current TP reflects the TOTAL accumulated frametime (includes pause).
    assert_eq!(
        s.get_current_tp(),
        std::time::Duration::from_millis((9.0 / 60.0 * 1000.0) as i64 as u64)
    );
}

#[test]
fn pause_time_adds_frametime_pause() {
    let mut s = HexagonGameStatus::new();
    s.reset_time(); // currentPause = 6.0
    s.pause_time(0.5); // currentPause += 0.5 * 60.0 = 30.0 -> 36.0

    // 36 FT of pause: each 1.0 frame decrements currentPause, no played time.
    for _ in 0..36 {
        s.accumulate_frametime(1.0);
    }
    assert!((s.get_played_accumulated_frametime() - 0.0).abs() < 1e-9);

    // One more frame now counts as played.
    s.accumulate_frametime(1.0);
    assert!((s.get_played_accumulated_frametime() - 1.0).abs() < 1e-9);
}

#[test]
fn increment_time_and_reset() {
    let mut s = HexagonGameStatus::new();
    s.reset_time(); // currentPause = 6.0
    for _ in 0..6 {
        s.accumulate_frametime(1.0); // consume the initial 6 FT pause
    }
    s.accumulate_frametime(60.0); // now playing: increment time = 60.0
    assert!((s.get_increment_time_seconds() - 1.0).abs() < 1e-9);

    s.reset_increment_time();
    assert!((s.get_increment_time_seconds() - 0.0).abs() < 1e-9);
}

#[test]
fn start_resets_time_and_marks_started() {
    let mut s = HexagonGameStatus::new();
    s.accumulate_frametime(100.0);
    assert!(!s.started);

    s.start();
    assert!(s.started);
    assert!((s.get_total_accumulated_frametime() - 0.0).abs() < 1e-12);
    assert!(s.is_time_paused()); // re-primed to the 0.1 s initial pause
}
