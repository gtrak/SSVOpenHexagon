//! `ssvs::Input::InputState` — the per-frame input state the game reads.
//!
//! Ported from `extlibs/SSVStart/include/SSVStart/Input/InputState.hpp` and the
//! bitset/offset constants in `extlibs/SSVStart/include/SSVStart/Global/
//! Typedefs.hpp`:
//!
//! - `inputBitOffset = 1`, so the key/button bit index is `int(v) + 1`
//!   (`KKey::Unknown == -1` therefore lands on bit 0 — the "Unknown/unbound"
//!   marker; this is the same convention `oh-config::Combo` uses).
//! - `KeyBitset = bitset<KeyCount + 1>` (102 bits), `BtnBitset =
//!   bitset<ButtonCount + 1>` (6 bits), `FingerBitset = bitset<fingerCount>`
//!   with `fingerCount = 16` (fingers are 0-indexed, no offset).
//!
//! The `Key` / `Button` enums are the exact SFML `Keyboard::Key` /
//! `Mouse::Button` integers, reused from `oh-config` (DECISIONS.md D-004 SFML
//! entry; oracle-vectors.md). The game drives this state from
//! `GameWindow::runEvents()` (`extlibs/SSVStart/include/SSVStart/GameSystem/
//! GameWindow.hpp`): `KeyPressed`/`KeyReleased`, `MouseButtonPressed`/
//! `MouseButtonReleased`, `TouchBegan`/`TouchEnded`, and a full `reset()` on
//! `LostFocus`.

use oh_config::{Button, Key};

/// `ssvs::inputBitOffset`.
pub const BIT_OFFSET: i32 = 1;
/// `ssvs::fingerCount`.
pub const FINGER_COUNT: usize = 16;
/// `KeyCount + inputBitOffset` (`101 + 1`).
pub const KEY_BITS: usize = Key::KEY_COUNT as usize + 1;
/// `ButtonCount + inputBitOffset` (`5 + 1`).
pub const BTN_BITS: usize = Button::BUTTON_COUNT as usize + 1;

/// `ssvs::Input::InputState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputState {
    // `FingerBitset` (16 bits, 0-indexed).
    fingers: [bool; FINGER_COUNT],
    // `KeyBitset` (bit `int(key) + 1`).
    keys: [bool; KEY_BITS],
    // `BtnBitset` (bit `int(btn) + 1`).
    btns: [bool; BTN_BITS],
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    /// All bits clear (C++ `InputState()` default-constructs three empty
    /// bitsets).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fingers: [false; FINGER_COUNT],
            keys: [false; KEY_BITS],
            btns: [false; BTN_BITS],
        }
    }

    /// `int(key) + 1` — the key bit index (`Unknown == -1` → bit 0).
    #[must_use]
    pub const fn key_bit_index(v: i32) -> usize {
        (v + BIT_OFFSET) as usize
    }

    /// `int(btn) + 1` — the button bit index.
    #[must_use]
    pub const fn btn_bit_index(v: i32) -> usize {
        (v + BIT_OFFSET) as usize
    }

    // ---- const accessors (C++ `operator[]` const overloads) ----------------

    /// C++ `operator[](KKey) const` → `getKeyBit(keys, mKey)`.
    #[must_use]
    pub fn is_key_pressed(&self, key: Key) -> bool {
        self.keys[Self::key_bit_index(key.0)]
    }

    /// The Lua-facing `u_isKeyPressed`'s read (HGScripting.cpp:92 —
    /// `window->getInputState()[ssvs::KKey(mKey)]`) over a raw `int`
    /// key code: the C++'s `Unknown(-1)` lands on bit 0 (always clear —
    /// the unbound marker); a code outside `Unknown(-1)`..`Pause(100)`
    /// is `false` (the C++'s `std::bitset::operator[]` throws
    /// `out_of_range` there; the port's defined behavior is `false` — a
    /// level script cannot make the host panic).
    #[must_use]
    pub fn is_key_pressed_code(&self, code: i32) -> bool {
        if !(-1..Key::KEY_COUNT).contains(&code) {
            return false;
        }
        self.keys[Self::key_bit_index(code)]
    }

    /// C++ `operator[](MBtn) const` → `getBtnBit(btns, mBtn)`.
    #[must_use]
    pub fn is_button_pressed(&self, btn: Button) -> bool {
        self.btns[Self::btn_bit_index(btn.0)]
    }

    /// The Lua-facing `u_isMouseButtonPressed`'s read (HGScripting.cpp:
    /// 138 — `window->getInputState()[ssvs::MBtn(mKey)]`) over a raw
    /// `int` button code: `false` outside `Unknown(-1)`..`XButton2(4)`
    /// (the out-of-range arm is the `is_key_pressed_code` note's).
    #[must_use]
    pub fn is_button_pressed_code(&self, code: i32) -> bool {
        if !(-1..Button::BUTTON_COUNT).contains(&code) {
            return false;
        }
        self.btns[Self::btn_bit_index(code)]
    }

    /// C++ `getFinger(FingerID) const` → `getFingerBit(fingers, mX)`.
    #[must_use]
    pub fn is_finger(&self, id: u32) -> bool {
        self.fingers.get(id as usize).copied().unwrap_or(false)
    }

    /// C++ `getKeys() const` — the raw 1-based key bitset (bit 0 = Unknown).
    #[must_use]
    pub fn get_keys(&self) -> &[bool; KEY_BITS] {
        &self.keys
    }

    /// C++ `getBtns() const` — the raw 1-based button bitset.
    #[must_use]
    pub fn get_btns(&self) -> &[bool; BTN_BITS] {
        &self.btns
    }

    /// C++ `getFingers() const` — the raw finger bitset.
    #[must_use]
    pub fn get_fingers(&self) -> &[bool; FINGER_COUNT] {
        &self.fingers
    }

    /// `GameWindow::getFingerDownCount()` — `fingers.count()`.
    #[must_use]
    pub fn get_finger_down_count(&self) -> usize {
        self.fingers.iter().filter(|b| **b).count()
    }

    // ---- mutable accessors (the C++ non-const `getFingers()` / `getKeys()`
    // / `getBtns()`, InputState.hpp:58-79) — the input manager's
    // per-frame `processedInput` writes through these (Combo.inl:35-36) ----

    /// C++ `getFingers()` (non-const overload).
    #[must_use]
    pub fn get_fingers_mut(&mut self) -> &mut [bool; FINGER_COUNT] {
        &mut self.fingers
    }

    /// C++ `getKeys()` (non-const overload).
    #[must_use]
    pub fn get_keys_mut(&mut self) -> &mut [bool; KEY_BITS] {
        &mut self.keys
    }

    /// C++ `getBtns()` (non-const overload).
    #[must_use]
    pub fn get_btns_mut(&mut self) -> &mut [bool; BTN_BITS] {
        &mut self.btns
    }

    // ---- mutators (C++ `operator[]` non-const overloads) --------------------

    /// C++ `inputState[mKey] = true/false` (from `KeyPressed` /
    /// `KeyReleased`).
    pub fn set_key_pressed(&mut self, key: Key, pressed: bool) {
        self.keys[Self::key_bit_index(key.0)] = pressed;
    }

    /// C++ `inputState[mBtn] = true/false` (from `MouseButtonPressed` /
    /// `MouseButtonReleased`).
    pub fn set_button_pressed(&mut self, btn: Button, pressed: bool) {
        self.btns[Self::btn_bit_index(btn.0)] = pressed;
    }

    /// C++ `inputState.getFinger(id) = true/false` (from `TouchBegan` /
    /// `TouchEnded`). Finger ids are 0-indexed (no offset).
    pub fn set_finger(&mut self, id: u32, pressed: bool) {
        if let Some(slot) = self.fingers.get_mut(id as usize) {
            *slot = pressed;
        }
    }

    /// C++ `reset()` — `fingers.reset(); keys.reset(); btns.reset();`
    /// (clears every bit; called on `LostFocus`).
    pub fn reset(&mut self) {
        self.fingers = [false; FINGER_COUNT];
        self.keys = [false; KEY_BITS];
        self.btns = [false; BTN_BITS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_bit_indexing_matches_sfvvoffset() {
        // int(key) + 1 : Unknown(-1)->0, A(0)->1, Z(25)->26, Pause(100)->101.
        assert_eq!(InputState::key_bit_index(-1), 0);
        assert_eq!(InputState::key_bit_index(0), 1);
        assert_eq!(InputState::key_bit_index(25), 26);
        assert_eq!(InputState::key_bit_index(100), 101);
        assert_eq!(InputState::btn_bit_index(0), 1);
        assert_eq!(InputState::btn_bit_index(4), 5);
        assert_eq!(KEY_BITS, 102);
        assert_eq!(BTN_BITS, 6);
    }

    #[test]
    fn press_release_round_trips() {
        let mut s = InputState::new();
        assert!(!s.is_key_pressed(Key::SPACE));
        s.set_key_pressed(Key::SPACE, true);
        assert!(s.is_key_pressed(Key::SPACE));
        // the raw bit is at index space.0 + 1
        assert!(s.get_keys()[Key::SPACE.0 as usize + 1]);
        s.set_key_pressed(Key::SPACE, false);
        assert!(!s.is_key_pressed(Key::SPACE));

        s.set_button_pressed(Button::LEFT, true);
        assert!(s.is_button_pressed(Button::LEFT));
        assert!(!s.is_button_pressed(Button::RIGHT));
    }

    #[test]
    fn finger_is_zero_indexed() {
        let mut s = InputState::new();
        s.set_finger(0, true);
        s.set_finger(1, true);
        assert!(s.is_finger(0));
        assert!(s.is_finger(1));
        assert!(!s.is_finger(2));
        assert_eq!(s.get_finger_down_count(), 2);
    }

    #[test]
    fn reset_clears_everything() {
        let mut s = InputState::new();
        s.set_key_pressed(Key::A, true);
        s.set_button_pressed(Button::MIDDLE, true);
        s.set_finger(3, true);
        s.reset();
        assert!(!s.is_key_pressed(Key::A));
        assert!(!s.is_button_pressed(Button::MIDDLE));
        assert!(!s.is_finger(3));
        assert_eq!(s.get_finger_down_count(), 0);
        assert_eq!(s, InputState::new());
    }

    #[test]
    fn unknown_key_lands_on_bit_zero() {
        let mut s = InputState::new();
        // Mirrors how C++ would index Unknown(-1) if it ever reached operator[].
        s.set_key_pressed(Key::UNKNOWN, true);
        assert!(s.get_keys()[0]);
    }

    #[test]
    fn raw_code_reads_are_the_lua_surface() {
        // The `u_isKeyPressed` / `u_isMouseButtonPressed` reads (HGScripting.
        // cpp:92/:138) over raw `int` codes: `int(code) + 1` indexing.
        let mut s = InputState::new();
        s.set_key_pressed(Key::L_SHIFT, true); // code 38 → bit 39
        s.set_button_pressed(Button::X_BUTTON1, true); // code 3 → bit 4
        assert!(s.is_key_pressed_code(38));
        assert!(!s.is_key_pressed_code(39));
        assert!(s.is_button_pressed_code(3));
        assert!(!s.is_button_pressed_code(2));
        // Unknown(-1) is the C++'s bit 0 (clear by default).
        assert!(!s.is_key_pressed_code(-1));
        assert!(!s.is_button_pressed_code(-1));
        // Out of range: the C++'s bitset subscript would throw; the port's
        // defined behavior is `false` (no panic from a level script).
        assert!(!s.is_key_pressed_code(101));
        assert!(!s.is_key_pressed_code(10_000));
        assert!(!s.is_key_pressed_code(-2));
        assert!(!s.is_key_pressed_code(i32::MAX));
        assert!(!s.is_button_pressed_code(5));
        assert!(!s.is_button_pressed_code(-2));
    }
}
