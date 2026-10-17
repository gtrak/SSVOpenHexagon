//! Port of `ssvs::Input::Trigger` / `ssvs::Input::Combo` / `ssvs::Input`
//! enums + the key/button name vocabulary from `extlibs/SSVStart`:
//!
//! - `include/SSVStart/Input/Trigger.hpp`, `Input/Combo.hpp`, `Input/Enums.hpp`;
//! - `include/SSVStart/Utils/Input.hpp` (name maps, validity checks,
//!   hardcoded-key set);
//! - `include/SSVStart/Global/Typedefs.hpp` (`KKey = sf::Keyboard::Key`,
//!   `MBtn = sf::Mouse::Button`, `KeyBitset = bitset<KeyCount + 1>` — the
//!   `+ 1` makes the bit index `int(key) + 1`, so bit 0 is the
//!   "Unknown/unbound" marker for `KKey::Unknown == -1`).
//!
//! The enum integers are the SFML fork's (`vittorioromeo/SFML@dacceddb`,
//! `include/SFML/Window/Keyboard.hpp` / `Mouse.hpp`) — see
//! `.agents/plans/001-rust-port/mapping/oracle-vectors.md` and
//! `.agents/skills/lib-binds.md`. They are part of the in-memory bind model
//! even though `config.json` stores bind *names*.

use oh_data::json::{self, JsonError, Obj};
use oh_data::linked_value::JsonConvertible;
use serde_json::Value as SerdeValue;

/// `sf::Keyboard::Key` (`ssvs::KKey`). `Unknown = -1`; `A = 0 .. Pause = 100`;
/// `KeyCount = 101`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Key(pub i32);

impl Key {
    pub const UNKNOWN: Key = Key(-1);
    pub const A: Key = Key(0);
    pub const B: Key = Key(1);
    pub const C: Key = Key(2);
    pub const D: Key = Key(3);
    pub const E: Key = Key(4);
    pub const F: Key = Key(5);
    pub const G: Key = Key(6);
    pub const H: Key = Key(7);
    pub const I: Key = Key(8);
    pub const J: Key = Key(9);
    pub const K: Key = Key(10);
    pub const L: Key = Key(11);
    pub const M: Key = Key(12);
    pub const N: Key = Key(13);
    pub const O: Key = Key(14);
    pub const P: Key = Key(15);
    pub const Q: Key = Key(16);
    pub const R: Key = Key(17);
    pub const S: Key = Key(18);
    pub const T: Key = Key(19);
    pub const U: Key = Key(20);
    pub const V: Key = Key(21);
    pub const W: Key = Key(22);
    pub const X: Key = Key(23);
    pub const Y: Key = Key(24);
    pub const Z: Key = Key(25);
    pub const NUM0: Key = Key(26);
    pub const NUM1: Key = Key(27);
    pub const NUM2: Key = Key(28);
    pub const NUM3: Key = Key(29);
    pub const NUM4: Key = Key(30);
    pub const NUM5: Key = Key(31);
    pub const NUM6: Key = Key(32);
    pub const NUM7: Key = Key(33);
    pub const NUM8: Key = Key(34);
    pub const NUM9: Key = Key(35);
    pub const ESCAPE: Key = Key(36);
    pub const L_CONTROL: Key = Key(37);
    pub const L_SHIFT: Key = Key(38);
    pub const L_ALT: Key = Key(39);
    pub const L_SYSTEM: Key = Key(40);
    pub const R_CONTROL: Key = Key(41);
    pub const R_SHIFT: Key = Key(42);
    pub const R_ALT: Key = Key(43);
    pub const R_SYSTEM: Key = Key(44);
    pub const MENU: Key = Key(45);
    pub const L_BRACKET: Key = Key(46);
    pub const R_BRACKET: Key = Key(47);
    pub const SEMICOLON: Key = Key(48);
    pub const COMMA: Key = Key(49);
    pub const PERIOD: Key = Key(50);
    pub const APOSTROPHE: Key = Key(51);
    pub const SLASH: Key = Key(52);
    pub const BACKSLASH: Key = Key(53);
    pub const GRAVE: Key = Key(54);
    pub const EQUAL: Key = Key(55);
    pub const HYPHEN: Key = Key(56);
    pub const SPACE: Key = Key(57);
    pub const ENTER: Key = Key(58);
    pub const BACKSPACE: Key = Key(59);
    pub const TAB: Key = Key(60);
    pub const PAGE_UP: Key = Key(61);
    pub const PAGE_DOWN: Key = Key(62);
    pub const END: Key = Key(63);
    pub const HOME: Key = Key(64);
    pub const INSERT: Key = Key(65);
    pub const DELETE: Key = Key(66);
    pub const ADD: Key = Key(67);
    pub const SUBTRACT: Key = Key(68);
    pub const MULTIPLY: Key = Key(69);
    pub const DIVIDE: Key = Key(70);
    pub const LEFT: Key = Key(71);
    pub const RIGHT: Key = Key(72);
    pub const UP: Key = Key(73);
    pub const DOWN: Key = Key(74);
    pub const NUMPAD0: Key = Key(75);
    pub const NUMPAD1: Key = Key(76);
    pub const NUMPAD2: Key = Key(77);
    pub const NUMPAD3: Key = Key(78);
    pub const NUMPAD4: Key = Key(79);
    pub const NUMPAD5: Key = Key(80);
    pub const NUMPAD6: Key = Key(81);
    pub const NUMPAD7: Key = Key(82);
    pub const NUMPAD8: Key = Key(83);
    pub const NUMPAD9: Key = Key(84);
    pub const F1: Key = Key(85);
    pub const F2: Key = Key(86);
    pub const F3: Key = Key(87);
    pub const F4: Key = Key(88);
    pub const F5: Key = Key(89);
    pub const F6: Key = Key(90);
    pub const F7: Key = Key(91);
    pub const F8: Key = Key(92);
    pub const F9: Key = Key(93);
    pub const F10: Key = Key(94);
    pub const F11: Key = Key(95);
    pub const F12: Key = Key(96);
    pub const F13: Key = Key(97);
    pub const F14: Key = Key(98);
    pub const F15: Key = Key(99);
    pub const PAUSE: Key = Key(100);

    /// `sf::Keyboard::Key::KeyCount`.
    pub const KEY_COUNT: i32 = 101;
}

/// `sf::Mouse::Button` (`ssvs::MBtn`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Button(pub i32);

impl Button {
    pub const LEFT: Button = Button(0);
    pub const RIGHT: Button = Button(1);
    pub const MIDDLE: Button = Button(2);
    pub const X_BUTTON1: Button = Button(3);
    pub const X_BUTTON2: Button = Button(4);

    /// `sf::Mouse::Button::ButtonCount`.
    pub const BUTTON_COUNT: i32 = 5;
}

// `extlibs/SSVStart/Utils/Input.hpp` `getKKeyStrArray()` /
// `getMBtnStrArray()` — `"k" + <SFML enum name>` / `"b" + <SFML enum name>`.
const KEY_NAMES: [&str; 101] = [
    "kA",
    "kB",
    "kC",
    "kD",
    "kE",
    "kF",
    "kG",
    "kH",
    "kI",
    "kJ",
    "kK",
    "kL",
    "kM",
    "kN",
    "kO",
    "kP",
    "kQ",
    "kR",
    "kS",
    "kT",
    "kU",
    "kV",
    "kW",
    "kX",
    "kY",
    "kZ",
    "kNum0",
    "kNum1",
    "kNum2",
    "kNum3",
    "kNum4",
    "kNum5",
    "kNum6",
    "kNum7",
    "kNum8",
    "kNum9",
    "kEscape",
    "kLControl",
    "kLShift",
    "kLAlt",
    "kLSystem",
    "kRControl",
    "kRShift",
    "kRAlt",
    "kRSystem",
    "kMenu",
    "kLBracket",
    "kRBracket",
    "kSemicolon",
    "kComma",
    "kPeriod",
    "kApostrophe",
    "kSlash",
    "kBackslash",
    "kGrave",
    "kEqual",
    "kHyphen",
    "kSpace",
    "kEnter",
    "kBackspace",
    "kTab",
    "kPageUp",
    "kPageDown",
    "kEnd",
    "kHome",
    "kInsert",
    "kDelete",
    "kAdd",
    "kSubtract",
    "kMultiply",
    "kDivide",
    "kLeft",
    "kRight",
    "kUp",
    "kDown",
    "kNumpad0",
    "kNumpad1",
    "kNumpad2",
    "kNumpad3",
    "kNumpad4",
    "kNumpad5",
    "kNumpad6",
    "kNumpad7",
    "kNumpad8",
    "kNumpad9",
    "kF1",
    "kF2",
    "kF3",
    "kF4",
    "kF5",
    "kF6",
    "kF7",
    "kF8",
    "kF9",
    "kF10",
    "kF11",
    "kF12",
    "kF13",
    "kF14",
    "kF15",
    "kPause",
];

const BTN_NAMES: [&str; 5] = ["bLeft", "bRight", "bMiddle", "bXButton1", "bXButton2"];

/// `getStrKKeyHardcodedMap()` — keys with hardcoded in-game behavior that
/// the config must not rebind.
const HARDCODED_KEY_NAMES: [&str; 15] = [
    "kJ",
    "kK",
    "kL",
    "kR",
    "kY",
    "kEscape",
    "kLAlt",
    "kEnter",
    "kBackspace",
    "kUp",
    "kDown",
    "kF1",
    "kF2",
    "kF3",
    "kF4",
];

/// C++ `getKKeyName(mKKey)` (asserts `mKKey != Unknown`; the port returns
/// `""` for `Unknown` instead of tripping the debug assert). `KEY_NAMES`
/// is 0-based: index `i` is the name of `Key(i)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_kkey_name(key: Key) -> &'static str {
    KEY_NAMES.get(key.0 as usize).copied().unwrap_or("")
}

/// C++ `getMBtnName(mMBtn)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_mbtn_name(btn: Button) -> &'static str {
    BTN_NAMES.get(btn.0 as usize).copied().unwrap_or("")
}

/// C++ `isKKeyNameValid(mId)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_kkey_name_valid(id: &str) -> bool {
    KEY_NAMES.contains(&id)
}

/// C++ `isKKeyHardcoded(mId)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_kkey_hardcoded(id: &str) -> bool {
    HARDCODED_KEY_NAMES.contains(&id)
}

/// C++ `isMBtnNameValid(mId)`.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn is_mbtn_name_valid(id: &str) -> bool {
    BTN_NAMES.contains(&id)
}

/// C++ `getKKey(mId)` (asserts validity; port: `None` when invalid).
#[must_use]
pub fn kkey_from_name(id: &str) -> Option<Key> {
    KEY_NAMES
        .iter()
        .position(|n| *n == id)
        .map(|i| Key(i as i32))
}

/// C++ `getMBtn(mId)` (asserts validity; port: `None` when invalid).
#[must_use]
pub fn mbtn_from_name(id: &str) -> Option<Button> {
    BTN_NAMES
        .iter()
        .position(|n| *n == id)
        .map(|i| Button(i as i32))
}

/// `ssvs::Input::Combo` — one slot of a trigger bind: a set of keys and a
/// set of buttons. Bit index is `int(key) + 1` (`inputBitOffset = 1`), so
/// bit 0 is the `KKey::Unknown` / "unbound" marker (mapping §1.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combo {
    // `KeyBitset` = `std::bitset<KeyCount + 1>`.
    keys: [bool; Key::KEY_COUNT as usize + 1],
    // `BtnBitset` = `std::bitset<ButtonCount + 1>`.
    btns: [bool; Button::BUTTON_COUNT as usize + 1],
}

impl Default for Combo {
    fn default() -> Self {
        Self {
            keys: [false; 102],
            btns: [false; 6],
        }
    }
}

impl Combo {
    /// C++ `Combo()` — all bits false (note: a default-constructed combo is
    /// NOT marked unbound; JSON `from_obj` clears the bind first, §1.8).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A combo in the cleared state: `KKey::Unknown` bit set, nothing else
    /// (C++ `Combo({KKey::Unknown})` / the `clearBind` of an empty combo).
    #[must_use]
    pub fn cleared() -> Self {
        let mut c = Self::default();
        c.clear_bind();
        c
    }

    /// C++ `getKeyBit` / `getBtnBit` index: `int(key) + 1` (`inputBitOffset
    /// = 1`), so `Key::Unknown (-1)` lands on bit 0 — the unbound marker.
    #[must_use]
    pub fn bit_index(v: i32) -> usize {
        (v + 1) as usize
    }

    /// C++ `addKey(mKey)`.
    pub fn add_key(&mut self, key: Key) {
        self.keys[Self::bit_index(key.0)] = true;
        if key != Key::UNKNOWN {
            self.keys[0] = false;
        }
    }

    /// C++ `addBtn(mBtn)`.
    pub fn add_btn(&mut self, btn: Button) {
        self.btns[Self::bit_index(btn.0)] = true;
        self.keys[0] = false;
    }

    /// C++ `clearBind()`: clear all 101 key bits + 5 button bits, then mark
    /// the `Unknown` bit (unbound marker).
    pub fn clear_bind(&mut self) {
        for i in 0..Key::KEY_COUNT as usize {
            self.keys[i + 1] = false;
        }
        for i in 0..Button::BUTTON_COUNT as usize {
            self.btns[i + 1] = false;
        }
        self.keys[0] = true;
    }

    /// C++ `isUnbound()`: the `Unknown` bit is set.
    #[must_use]
    pub fn is_unbound(&self) -> bool {
        self.keys[0]
    }

    /// C++ `getKeys()` — the raw 1-based key bitset (bit 0 = Unknown).
    #[must_use]
    pub fn get_keys(&self) -> &[bool; 102] {
        &self.keys
    }

    /// C++ `getBtns()` — the raw 1-based button bitset.
    #[must_use]
    pub fn get_btns(&self) -> &[bool; 6] {
        &self.btns
    }
}

/// `ssvs::Input::Trigger` — a `std::vector<Combo>` (one trigger may have up
/// to `Config::MAX_BINDS = 4` active combos; multiple entries mean ANY of
/// the inputs triggers the action).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trigger {
    combos: Vec<Combo>,
}

impl Trigger {
    /// C++ `Trigger()` — empty combos vector (later fixed up to four
    /// unbound slots by `Config::fixup_missing_triggers`).
    #[must_use]
    pub fn new() -> Self {
        Self { combos: Vec::new() }
    }

    /// C++ `Trigger(initializer_list<Combo>)`.
    #[must_use]
    pub fn from_combos(combos: Vec<Combo>) -> Self {
        Self { combos }
    }

    /// One bound key, one combo — the C++ `addInput({{k::X}}, ...)`
    /// hardcoded-trigger shape (MenuGame.cpp:945,959,999; HexagonGame.cpp:
    /// 377-381).
    #[must_use]
    pub fn key(key: Key) -> Self {
        let mut c = Combo::new();
        c.add_key(key);
        Self { combos: vec![c] }
    }

    /// One combo of several keys (ALL must be pressed — `Combo::isDown`
    /// requires every combo key, Combo.inl:18-20) — the C++
    /// `{{k::LAlt, k::Enter}}` shape (MenuGame.cpp:993-994).
    #[must_use]
    pub fn keys(keys: &[Key]) -> Self {
        let mut c = Combo::new();
        for k in keys {
            c.add_key(*k);
        }
        Self { combos: vec![c] }
    }

    /// C++ `getCombos()` (const).
    #[must_use]
    pub fn get_combos(&self) -> &[Combo] {
        &self.combos
    }

    /// C++ `getCombos()` (mutable).
    pub fn get_combos_mut(&mut self) -> &mut Vec<Combo> {
        &mut self.combos
    }
}

// ---------------------------------------------------------------------------
// JSON conversion (C++ `Converter<ssvs::Input::Combo>` / `<Trigger>`,
// `Global/UtilsJson.cpp`; mapping §1.8)
// ---------------------------------------------------------------------------

impl JsonConvertible for Combo {
    /// C++ `Converter<Combo>::fromObj`: clear the bind, then per string
    /// element: `""` → `addKey(Unknown)`; valid key name → `addKey`; valid
    /// button name → `addBtn`; anything else logs a warning in C++ and puts
    /// an empty bind in its place (`addKey(Unknown)`). A non-array value
    /// has no member names → an unbound combo.
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        let mut combo = Combo::cleared();
        if let SerdeValue::Array(items) = obj {
            for item in items {
                // C++ `getExtr<std::string>(i)` — the fork's `asString`
                // coercion (a numeric element reads as its decimal string,
                // jsoncpp.inl:1177-1190), so a hand-edited `[123]` combo
                // item reaches the name checks as "123".
                let s = json::as_str(item)?;
                if s.is_empty() {
                    combo.add_key(Key::UNKNOWN);
                } else if let Some(key) = kkey_from_name(&s) {
                    combo.add_key(key);
                } else if let Some(btn) = mbtn_from_name(&s) {
                    combo.add_btn(btn);
                } else {
                    // C++ logs "<elem> is not a valid input name, an empty
                    // bind has been put in its place".
                    combo.add_key(Key::UNKNOWN);
                }
            }
        }
        Ok(combo)
    }

    /// C++ `Converter<Combo>::toObj`: unbound → `["" ]`; bound → all set
    /// keys first (ascending enum order, as names) then all set buttons.
    fn to_obj(&self) -> Obj {
        if self.is_unbound() {
            return SerdeValue::Array(vec![SerdeValue::String(String::new())]);
        }
        let mut items: Vec<Obj> = Vec::new();
        for i in 0..Key::KEY_COUNT {
            if self.keys[i as usize + 1] {
                items.push(SerdeValue::String(get_kkey_name(Key(i)).to_string()));
            }
        }
        for i in 0..Button::BUTTON_COUNT {
            if self.btns[i as usize + 1] {
                items.push(SerdeValue::String(get_mbtn_name(Button(i)).to_string()));
            }
        }
        SerdeValue::Array(items)
    }
}

impl JsonConvertible for Trigger {
    /// C++ vector converter: `resize(obj.size())` + per-index `extr` — a
    /// non-array / null value yields an empty trigger.
    fn from_obj(obj: &Obj) -> Result<Self, JsonError> {
        let mut combos = Vec::with_capacity(json::size(obj));
        if let SerdeValue::Array(items) = obj {
            for item in items {
                combos.push(Combo::from_obj(item)?);
            }
        }
        Ok(Self { combos })
    }

    fn to_obj(&self) -> Obj {
        SerdeValue::Array(
            self.combos
                .iter()
                .map(|c| c.to_obj())
                .collect::<Vec<_>>()
                .into_iter()
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn combos(obj: &str) -> Combo {
        let v = json::parse(obj).unwrap();
        Combo::from_obj(&v).unwrap()
    }

    #[test]
    fn combo_from_obj_key_and_button() {
        let c = combos(r#"["kSpace"]"#);
        assert!(!c.is_unbound());
        assert!(c.get_keys()[Key::SPACE.0 as usize + 1]);
        assert_eq!(
            c.to_obj(),
            Value::Array(vec![Value::String("kSpace".into())])
        );

        let c = combos(r#"["bMiddle"]"#);
        assert!(!c.is_unbound());
        assert!(c.get_btns()[Button::MIDDLE.0 as usize + 1]);
        assert_eq!(
            c.to_obj(),
            Value::Array(vec![Value::String("bMiddle".into())])
        );
    }

    #[test]
    fn combo_unbound_round_trips_to_empty_string() {
        let c = combos(r#"[""]"#);
        assert!(c.is_unbound());
        assert_eq!(c.to_obj(), Value::Array(vec![Value::String(String::new())]));

        // an invalid name is replaced by an empty bind (C++ warning path)
        let c = combos(r#"["notAKey"]"#);
        assert!(c.is_unbound());

        // a non-array value has no member names -> unbound combo
        let c = Combo::from_obj(&Value::String("kSpace".into())).unwrap();
        assert!(c.is_unbound());
    }

    #[test]
    fn combo_to_obj_keys_before_buttons_ascending() {
        let mut c = Combo::cleared();
        c.add_key(Key::W);
        c.add_key(Key::A);
        c.add_btn(Button::LEFT);
        assert_eq!(
            c.to_obj(),
            Value::Array(vec![
                Value::String("kA".into()),
                Value::String("kW".into()),
                Value::String("bLeft".into())
            ])
        );
    }

    #[test]
    fn trigger_json_round_trip() {
        let src = json::parse(r#"[["kA"],["kLeft"],["bLeft"],""]"#).unwrap();
        let t = Trigger::from_obj(&src).unwrap();
        assert_eq!(t.get_combos().len(), 4);
        assert!(!t.get_combos()[0].is_unbound());
        assert!(t.get_combos()[3].is_unbound());
        assert_eq!(
            t.to_obj(),
            Value::Array(vec![
                Value::Array(vec![Value::String("kA".into())]),
                Value::Array(vec![Value::String("kLeft".into())]),
                Value::Array(vec![Value::String("bLeft".into())]),
                Value::Array(vec![Value::String(String::new())])
            ])
        );
    }

    #[test]
    fn name_tables() {
        assert_eq!(get_kkey_name(Key::SPACE), "kSpace");
        assert_eq!(get_mbtn_name(Button::X_BUTTON2), "bXButton2");
        assert_eq!(kkey_from_name("kF12"), Some(Key::F12));
        assert_eq!(kkey_from_name("kNope"), None);
        assert!(is_kkey_hardcoded("kEscape") && !is_kkey_hardcoded("kSpace"));
        assert!(!is_kkey_name_valid("") && is_mbtn_name_valid("bXButton1"));
        // every named constant has the right integer (oracle-vectors.md)
        assert_eq!(Key::A.0, 0);
        assert_eq!(Key::Z.0, 25);
        assert_eq!(Key::PAUSE.0, 100);
        assert_eq!(Button::BUTTON_COUNT, 5);
    }
}
