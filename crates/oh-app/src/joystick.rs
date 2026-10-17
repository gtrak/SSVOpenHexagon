//! 29j1 — the desktop controller backend (the C++ `sf::Joystick`
//! controller-0 poll's replacement): `RealJoystick` reads the FIRST
//! connected gamepad from `gilrs` (the C++'s hardcoded `joyId = 0`,
//! `Core/Joystick.cpp:131`), snapshots it into an `oh_config::
//! JoystickPoll` (the SFML-scale seam, `oh-config` `joystick.rs`'s
//! module docs), and feeds the shared `oh_config::joystick` machine —
//! the same state the engine's `Joystick` seam (`oh_game::seams::
//! Joystick`) and the menu's `MenuJoystick` seam (`oh_menu::seams::
//! MenuJoystick`) query. The composition (the windowed client's
//! `build`) shares ONE `RealJoystick` between both seams.
//!
//! # Desktop only (cfg-gate)
//!
//! The gilrs backend is desktop-only (`Cargo.toml`'s
//! `not(any(target_os = "android", target_family = "wasm"))` target
//! table): the Android / wasm compositions keep the no-op
//! `NoJoystick` / `NoMenuJoystick` seams (the C++'s
//! `isConnected(0) == false` state — every query false, vendor id 0,
//! the poll a no-op).
//!
//! # gilrs system requirements (reported, 29j1)
//!
//! * **Linux build**: the `libudev` system library — the transitive
//!   `libudev-sys` build links it (`pkg-config --libs libudev`), so
//!   `libudev-dev` must be installed at build time (`.github/
//!   workflows/rust.yml`'s portability notes carry this).
//! * **Linux runtime**: hidraw device access — non-root users need a
//!   udev rule granting `/dev/hidraw*` (gilrs ships `60-gilrs.rules`).
//!   A failed open degrades to the no-controller state (logged once,
//!   never retried) — the C++ SFML no-pad behavior (every query
//!   false, axes 0), NOT a crash.
//!
//! # SFML parity notes
//!
//! * `GilrsBuilder::with_default_filters(false)`: the C++ applies the
//!   `Config::getJoystickDeadzone()` deadzone ONCE, in the machine's
//!   `axisPressed` (the SFML values are raw, `-100..100` sticks /
//!   `-127..127` POV); gilrs's default filters (a per-device deadzone
//!   renormalization + jitter + dpad-axis-to-button) would double-
//!   filter. With the filters off, the dpad buttons come from the
//!   mapping database (the standard `+h0left:DpadLeft`-style hat
//!   entries — the C++'s SFML POV axes read the same kernel hat).
//! * `with_force_feedback(false)`: the C++ never uses it; FF needs
//!   extra hidraw permissions some kernels deny.
//! * Sticks: gilrs `Axis::LeftStickX/Y` are `-1..1` (Y positive UP);
//!   SFML's `X`/`Y` are `-100..100` (Y positive DOWN) — the snapshot
//!   rescales to the SFML units the machine expects and inverts Y.
//! * SFML's `isButtonPressed(0, i)` (i in `0..8`) is the XBOX 360
//!   button order (the `Core/Joystick.cpp:10-40` mapping comment: A0
//!   B1 X2 Y3 LB4 RB5 back6 start7); the snapshot reads the matching
//!   gilrs buttons (`South/East/North/West/LeftTrigger2/RightTrigger2/
//!   Select/Start` — the standard mapping database's names for those
//!   positions). A pad whose mapping database entry names a position
//!   differently is the documented backend divergence (the C++'s
//!   kernel-order index would read it; the C++ comment pins XBOX 360
//!   as the reference layout).
//!
//! # Single-thread contract (the `unsafe impl Send + Sync` below)
//!
//! `gilrs::Gilrs` is `!Send + !Sync` (the C++'s SFML poll is likewise
//! a single-thread read). `RealJoystick` is shared through
//! `Arc<dyn Joystick>` (the engine) and `Rc<dyn MenuJoystick>` (the
//! menu) — both seams are driven ONLY from the composition's main
//! thread (the winit event-loop thread desktop / the activity thread
//! on Android; the C++'s game-loop thread), exactly the
//! `oh_lua::api::state::SharedLua` raw-pointer precedent in
//! `compose.rs`'s module docs ("only dereferenced on the single
//! thread that owns it"). The `Mutex` guards the `&mut Gilrs` access
//! itself; the `unsafe impl` asserts the thread contract the
//! composition upholds.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};

use gilrs::{Axis, Button, Gilrs, GilrsBuilder};
use oh_config::joystick as machine;
use oh_config::{Jdir, Jid, JoystickPoll};
use oh_game::seams::Joystick;
use oh_menu::seams::MenuJoystick;

/// The SFML stick-axis scale (the `JoystickPoll` docs: sticks
/// `[-100, 100]`) — gilrs reports `-1..1`.
const STICK_SCALE: f32 = 100.0;
/// The SFML POV-axis pressed magnitude (the `JoystickPoll` docs: POV
/// `[-127, 127]`; the C++ `axisPressed` only compares against the
/// deadzone, so any non-zero past it fires — the SFML value is the
/// faithful magnitude).
const POV_PRESSED: f32 = 127.0;

/// SFML `isButtonPressed(0, i)` button index → gilrs button (the XBOX
/// 360 mapping, `Core/Joystick.cpp:10-40`: A0 B1 X2 Y3 LB4 RB5 back6
/// start7; the mapping database's names for those positions).
const SFML_BUTTONS: [Button; 8] = [
    Button::South,
    Button::East,
    Button::North,
    Button::West,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
];

struct Backend {
    /// The controller backend; `None` until the first poll creates it
    /// (the C++ creates the SFML joystick state lazily on first read).
    gilrs: Option<Gilrs>,
    /// A failed backend open (logged once, never retried) — every query
    /// then reports the no-controller state.
    failed: bool,
}

impl Backend {
    /// C++ `Joystick::update`'s SFML poll half: drain the device events
    /// (SFML's implicit `sf::Joystick::update()` on every axis/button
    /// read) and snapshot the FIRST connected gamepad (the C++'s
    /// `joyId = 0`) into SFML-scale values. No connected pad → the
    /// C++'s disconnected read: axes 0, no button pressed.
    fn snapshot(&mut self) -> JoystickPoll {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return JoystickPoll::default();
        };
        while gilrs.next_event().is_some() {}
        let mut poll = JoystickPoll::default();
        let Some((_, gamepad)) = gilrs.gamepads().next() else {
            return poll;
        };
        // Left stick (the C++'s `sf::Joystick::Axis::X` / `Y`): SFML
        // X right-positive, Y DOWN-positive; gilrs Y up-positive.
        poll.x = gamepad.value(Axis::LeftStickX) * STICK_SCALE;
        poll.y = -gamepad.value(Axis::LeftStickY) * STICK_SCALE;
        // Dpad (the C++'s `PovX` / `PovY`): right +127 / left -127,
        // up +127 / down -127 (the `Core/Joystick.cpp:30-36` mapping).
        if gamepad.is_pressed(Button::DPadRight) {
            poll.pov_x = POV_PRESSED;
        } else if gamepad.is_pressed(Button::DPadLeft) {
            poll.pov_x = -POV_PRESSED;
        }
        if gamepad.is_pressed(Button::DPadUp) {
            poll.pov_y = POV_PRESSED;
        } else if gamepad.is_pressed(Button::DPadDown) {
            poll.pov_y = -POV_PRESSED;
        }
        for (i, btn) in SFML_BUTTONS.iter().enumerate() {
            poll.buttons[i] = gamepad.is_pressed(*btn);
        }
        poll
    }

    /// C++ `sf::Joystick::isConnected(0) ?
    /// sf::Joystick::getIdentification(0).vendorId : 0` (Config.cpp:
    /// 1577-1581 — the `getJoystickBindName` vendor source).
    fn vendor_id(&self) -> u32 {
        match self.gilrs.as_ref().and_then(|g| g.gamepads().next()) {
            Some((_, gp)) => u32::from(gp.vendor_id().unwrap_or(0)),
            None => 0,
        }
    }
}

/// The desktop controller poller — one shared object behind BOTH the
/// engine's `Joystick` seam and the menu's `MenuJoystick` seam (the
/// C++'s ONE global `Joystick` state; the machine it feeds is global
/// too, so the two seams observe one controller, exactly like the
/// C++).
pub struct RealJoystick {
    backend: Mutex<Backend>,
    /// The failure is logged ONCE (a per-frame re-log would spam; the
    /// C++ SFML silent-degrades — the port's single stderr line is the
    /// documented divergence, module docs).
    logged_failure: Cell<bool>,
}

// `Gilrs` is `!Send + !Sync` (single-thread device read, the C++'s
// single-thread SFML poll); `RealJoystick` is carried in `Arc<dyn
// Joystick + Send + Sync>` (the engine) and `Rc<dyn MenuJoystick>` (the
// menu). The composition drives both seams ONLY from its main thread
// (the winit event-loop / the Android activity thread) — the
// `SharedLua` raw-pointer precedent (`compose.rs` module docs).
// SAFETY: every `&self` entry point touches the `Gilrs` handle only
// through the `Mutex` (the `&mut` it needs), and the composition
// upholds the single-thread contract above (the handle is created and
// dropped on that thread; the `Mutex` makes a theoretical second-thread
// call still data-race-free, it just observes the device state the
// main thread last polled).
unsafe impl Send for RealJoystick {}
unsafe impl Sync for RealJoystick {}

impl RealJoystick {
    #[must_use]
    pub fn new() -> Self {
        Self {
            backend: Mutex::new(Backend {
                gilrs: None,
                failed: false,
            }),
            logged_failure: Cell::new(false),
        }
    }

    /// Create the backend on first use (the C++'s lazy SFML state); a
    /// failed open degrades to the no-controller state (logged once).
    /// `GilrsBuilder::with_default_filters(false)`: raw values — the
    /// C++'s single machine-side deadzone is the ONLY deadzone (module
    /// docs' SFML-parity note); `with_force_feedback(false)`: the C++
    /// never uses it (module docs).
    fn backend(&self) -> MutexGuard<'_, Backend> {
        let mut b = self.backend.lock().expect("joystick backend poisoned");
        if b.gilrs.is_none() && !b.failed {
            match GilrsBuilder::new()
                .with_default_filters(false)
                .with_force_feedback(false)
                .build()
            {
                Ok(g) => b.gilrs = Some(g),
                Err(e) => {
                    b.failed = true;
                    if !self.logged_failure.get() {
                        self.logged_failure.set(true);
                        eprintln!("[joystick] controller backend unavailable: {e}");
                    }
                }
            }
        }
        b
    }

    /// C++ `Joystick::update(deadzone)` — poll the controller, feed the
    /// shared machine (the C++'s SFML poll inside `update` is this
    /// split: poll here, machine math in `oh_config::joystick::update`).
    fn poll_update(&self, deadzone: f32) {
        let poll = self.backend().snapshot();
        machine::update(deadzone, &poll);
    }

    /// C++ `Joystick::pressed(Jid::Focus)` (the menu's `focusHeld`
    /// OR-term, MenuGame.cpp:2650-2653).
    fn pressed_focus(&self) -> bool {
        machine::pressed(Jid::Focus)
    }

    /// C++ `Joystick::pressed(Jdir)`.
    fn pressed_jdir(&self, dir: Jdir) -> bool {
        machine::pressed_dir(dir)
    }

    /// C++ `Joystick::pressed(Jid)`.
    fn pressed_jid(&self, id: Jid) -> bool {
        machine::pressed(id)
    }

    /// C++ `Joystick::risingEdge(Jdir)`.
    fn rising_jdir(&self, dir: Jdir) -> bool {
        machine::rising_edge_dir(dir)
    }

    /// C++ `Joystick::risingEdge(Jid)`.
    fn rising_jid(&self, id: Jid) -> bool {
        machine::rising_edge(id)
    }

    /// C++ `sf::Joystick::isConnected(0) ?
    /// sf::Joystick::getIdentification(0).vendorId : 0`.
    fn controller_vendor_id(&self) -> u32 {
        self.backend().vendor_id()
    }
}

impl Default for RealJoystick {
    fn default() -> Self {
        Self::new()
    }
}

impl Joystick for RealJoystick {
    fn update(&self, deadzone: f32) {
        self.poll_update(deadzone)
    }
    fn pressed_dir(&self, dir: Jdir) -> bool {
        self.pressed_jdir(dir)
    }
    fn pressed_id(&self, id: Jid) -> bool {
        self.pressed_jid(id)
    }
    fn rising_edge_id(&self, id: Jid) -> bool {
        self.rising_jid(id)
    }
    fn vendor_id(&self) -> u32 {
        self.controller_vendor_id()
    }
}

/// The menu's `MenuJoystick` view of the SAME shared
/// [`RealJoystick`] (the composition holds one `RealJoystick` for the
/// engine's `Arc<dyn Joystick>` and this `Arc`-backed clone for the
/// menu's `Rc<dyn MenuJoystick>` — the C++'s one global, one poller;
/// `RealJoystick` is `Send + Sync` — the documented single-thread
/// contract above — so the `Arc` share is safe).
#[derive(Clone)]
pub struct RealMenuJoystick(std::sync::Arc<RealJoystick>);

impl MenuJoystick for RealMenuJoystick {
    fn update(&self, deadzone: f32) {
        self.0.poll_update(deadzone)
    }
    fn pressed_focus(&self) -> bool {
        self.0.pressed_focus()
    }
    fn rising_edge(&self, jid: Jid) -> bool {
        self.0.rising_jid(jid)
    }
    fn rising_edge_dir(&self, jdir: Jdir) -> bool {
        self.0.rising_jdir(jdir)
    }
    fn ignore_all_presses(&self, ignore: bool) {
        machine::ignore_all_presses(ignore)
    }
}

impl RealMenuJoystick {
    #[must_use]
    pub fn new(inner: std::sync::Arc<RealJoystick>) -> Self {
        Self(inner)
    }
}
