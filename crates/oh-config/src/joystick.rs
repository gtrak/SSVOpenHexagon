//! Port of `include/SSVOpenHexagon/Core/Joystick.hpp` +
//! `src/SSVOpenHexagon/Core/Joystick.cpp`.
//!
//! `hg::Joystick`: controller-0 state machine. The C++ polls SFML
//! (`sf::Joystick::getAxisPosition` / `isButtonPressed`) inside `update`;
//! the port takes an explicit `JoystickPoll` snapshot as the input seam
//! (issue 10 wires the window backend into it). Controller id is hardcoded
//! to 0 in the C++ as well.

use std::sync::{Mutex, MutexGuard};

/// C++ `hg::Joystick::Jdir`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Jdir {
    Unknown = -1,
    Left = 0,
    Right,
    Up,
    Down,
    JoystickDirectionsCount,
}

/// C++ `hg::Joystick::Jid` — the 12 joystick bind slots, in `config.json`
/// key order `j_select, j_exit, j_focus, j_swap, j_force_restart,
/// j_restart, j_replay, j_screenshot, j_next, j_previous,
/// j_add_favorite, j_favorite_menu`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Jid {
    Unknown = -1,
    Select = 0,
    Exit,
    Focus,
    Swap,
    ForceRestart,
    Restart,
    Replay,
    Screenshot,
    NextPack,
    PreviousPack,
    AddToFavorites,
    FavoritesMenu,
    JoystickBindsCount,
}

const JOY_COUNT: usize = 4; // `toSizeT(Jdir::JoystickDirectionsCount)`
const BIND_COUNT: usize = 12; // `toSizeT(Jid::JoystickBindsCount)`

/// The 12 bind slots in `Jid` value order (C++ `doButton(Jid::Select)`
/// .. `doButton(Jid::FavoritesMenu)`).
const JIDS: [Jid; BIND_COUNT] = [
    Jid::Select,
    Jid::Exit,
    Jid::Focus,
    Jid::Swap,
    Jid::ForceRestart,
    Jid::Restart,
    Jid::Replay,
    Jid::Screenshot,
    Jid::NextPack,
    Jid::PreviousPack,
    Jid::AddToFavorites,
    Jid::FavoritesMenu,
];

/// C++ `JoystickState` (function-local static, value-initialized).
#[derive(Clone, Debug, Default)]
pub struct JoystickState {
    ignore_all_presses: bool,
    dir_was_pressed: [bool; JOY_COUNT],
    dir_pressed: [bool; JOY_COUNT],
    was_pressed: [bool; BIND_COUNT],
    pressed: [bool; BIND_COUNT],
    /// `Jid` index -> SFML joystick button index from the `j_*` config keys.
    joystick_inputs: [u32; BIND_COUNT],
}

impl JoystickState {
    fn do_dir(&mut self, jdir: Jdir, check: bool) {
        self.dir_was_pressed[jdir as usize] =
            std::mem::replace(&mut self.dir_pressed[jdir as usize], check);
    }

    fn do_button(&mut self, jid: Jid, is_pressed: bool) {
        self.was_pressed[jid as usize] =
            std::mem::replace(&mut self.pressed[jid as usize], is_pressed);
    }
}

static JOYSTICK_STATE: Mutex<JoystickState> = Mutex::new(JoystickState {
    ignore_all_presses: false,
    dir_was_pressed: [false; JOY_COUNT],
    dir_pressed: [false; JOY_COUNT],
    was_pressed: [false; BIND_COUNT],
    pressed: [false; BIND_COUNT],
    joystick_inputs: [0; BIND_COUNT],
});

fn state() -> MutexGuard<'static, JoystickState> {
    JOYSTICK_STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// C++ `hg::Joystick::ignoreAllPresses(ignore)` — latch every was-pressed
/// flag and the `ignoreAllPresses` flag so no rising edge fires while it is
/// set; `update` then early-returns completely.
pub fn ignore_all_presses(ignore: bool) {
    let mut s = state();
    for b in s.dir_was_pressed.iter_mut() {
        *b = ignore;
    }
    for b in s.was_pressed.iter_mut() {
        *b = ignore;
    }
    s.ignore_all_presses = ignore;
}

/// C++ `hg::Joystick::setJoystickBind(button, buttonID)` — fed by
/// `Config::load_all_joystick_binds`.
pub fn set_joystick_bind(button: u32, button_id: i32) {
    state().joystick_inputs[button_id as usize] = button;
}

/// One poll of controller 0 (the SFML seam). Axis positions are the raw
/// SFML values: stick axes in `[-100, 100]`, POV axes in `[-127, 127]`
/// (0 when no direction). SFML has 8 buttons; out-of-range indices read as
/// "not pressed", like `sf::Joystick::isButtonPressed`.
#[derive(Clone, Copy, Debug, Default)]
pub struct JoystickPoll {
    pub pov_x: f32,
    pub pov_y: f32,
    pub x: f32,
    pub y: f32,
    pub buttons: [bool; 8],
}

impl JoystickPoll {
    #[must_use]
    pub fn button_pressed(&self, index: u32) -> bool {
        (index as usize) < self.buttons.len() && self.buttons[index as usize]
    }
}

#[derive(Clone, Copy)]
enum AxisDir {
    Left = -1,
    Dead = 0,
    Right = 1,
}

impl AxisDir {
    fn negate(self) -> AxisDir {
        match self {
            AxisDir::Left => AxisDir::Right,
            AxisDir::Dead => AxisDir::Dead,
            AxisDir::Right => AxisDir::Left,
        }
    }
}

impl PartialEq for AxisDir {
    fn eq(&self, other: &Self) -> bool {
        *self as i32 == *other as i32
    }
}

/// C++ `axisPressed(deadzone, joyId, axis)`.
fn axis_pressed(deadzone: f32, pos: f32) -> AxisDir {
    if pos < -deadzone {
        AxisDir::Left
    } else if pos > deadzone {
        AxisDir::Right
    } else {
        AxisDir::Dead
    }
}

/// C++ `hg::Joystick::update(deadzone)`, with the SFML poll made explicit.
///
/// XBOX 360 mapping comment from the source: buttons A0 B1 X2 Y3 LB4 RB5
/// back6 start7 L8 R9; triggers LT +Z, RT -Z; dpad left -povX, right +povX,
/// up +povY, down -povY.
pub fn update(deadzone: f32, poll: &JoystickPoll) {
    let mut s = state();

    // all presses are being ignored, try again later
    if s.ignore_all_presses {
        return;
    }

    let dpad_x = axis_pressed(deadzone, poll.pov_x);
    let dpad_y = axis_pressed(deadzone, poll.pov_y);
    let stick_x = axis_pressed(deadzone, poll.x);
    let stick_y = axis_pressed(deadzone, poll.y);

    let x_is = |dir: AxisDir| dpad_x == dir || stick_x == dir;
    let y_is = |dir: AxisDir| dpad_y == dir || stick_y == dir.negate();

    s.do_dir(Jdir::Left, x_is(AxisDir::Left));
    s.do_dir(Jdir::Right, x_is(AxisDir::Right));
    // Y is inverted: stick "up" (-Y) reads as `Dead`-negated `Right`.
    s.do_dir(Jdir::Up, y_is(AxisDir::Right));
    s.do_dir(Jdir::Down, y_is(AxisDir::Left));

    for (i, jid) in JIDS.iter().enumerate() {
        let button = s.joystick_inputs[i];
        s.do_button(*jid, poll.button_pressed(button));
    }
}

/// C++ `hg::Joystick::pressed(Jdir)`.
#[must_use]
pub fn pressed_dir(jdir: Jdir) -> bool {
    state().dir_pressed[jdir as usize]
}

/// C++ `hg::Joystick::risingEdge(Jdir)` — pressed && !wasPressed.
#[must_use]
pub fn rising_edge_dir(jdir: Jdir) -> bool {
    let s = state();
    s.dir_pressed[jdir as usize] && !s.dir_was_pressed[jdir as usize]
}

/// C++ `hg::Joystick::pressed(Jid)`.
#[must_use]
pub fn pressed(jid: Jid) -> bool {
    state().pressed[jid as usize]
}

/// C++ `hg::Joystick::risingEdge(Jid)`.
#[must_use]
pub fn rising_edge(jid: Jid) -> bool {
    let s = state();
    s.pressed[jid as usize] && !s.was_pressed[jid as usize]
}
