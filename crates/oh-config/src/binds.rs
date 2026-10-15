//! Port of `include/SSVOpenHexagon/Core/BindControl.hpp` +
//! `src/SSVOpenHexagon/Core/BindControl.cpp` — the menu-independent bind
//! editing logic. The C++ classes derive from `ssvms::ItemBase` (a menu
//! item; issue 11); here the menu plumbing is dropped and the `Config`
//! struct is passed explicitly where the C++ reached for its globals. The
//! C++ "callback" template parameters become per-call closures.

use crate::config::{clear_trigger_bind, rebind_trigger, Tid};
use crate::joystick::Jid;
use crate::triggers::{Button, Combo, Key, Trigger};
use crate::Config;

/// C++ `hg::KeyboardBindControl` (without the `ssvms::ItemBase` base).
#[derive(Clone, Debug)]
pub struct KeyboardBindControl {
    name: String,
    trigger_id: Tid,
    waiting_for_bind: bool,
    /// A few actions have hardcoded keys; the user may not bind the
    /// hardcoded key a second time.
    hardcoded_key: Key,
}

impl KeyboardBindControl {
    /// C++ `KeyboardBindControl(mMenu, mCategory, mName, get, set, clear,
    /// callback, mTriggerID, mHardcodedKey)`. Performs the constructor's
    /// sanitization: if the user manually put `hardcoded_key` into any
    /// combo, that slot is cleared. (C++ copies the combos first — "Cannot
    /// use a reference here because `triggerGetter()` returns by value" —
    /// the port does the same.)
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        cfg: &mut Config,
        trigger_id: Tid,
        hardcoded_key: Key,
    ) -> Self {
        let combos = cfg.trigger_ref(trigger_id).get_combos().to_vec();
        for (i, c) in combos.iter().enumerate() {
            if c.get_keys()[Combo::bit_index(hardcoded_key.0)] {
                clear_trigger_bind(cfg.get_trigger(trigger_id), i as i32);
            }
        }
        Self {
            name: name.into(),
            trigger_id,
            waiting_for_bind: false,
            hardcoded_key,
        }
    }

    /// C++ `getRealSize(combos)`: count of leading non-unbound combos.
    fn get_real_size(cfg: &Config, trigger_id: Tid) -> i32 {
        let mut i = 0;
        for c in cfg.trigger_ref(trigger_id).get_combos() {
            if c.is_unbound() {
                break;
            }
            i += 1;
        }
        i
    }

    /// C++ `exec()` — toggles `waitingForBind`.
    pub fn exec(&mut self) {
        self.waiting_for_bind = !self.waiting_for_bind;
    }

    /// C++ `isWaitingForBind()`.
    #[must_use]
    pub fn is_waiting_for_bind(&self) -> bool {
        self.waiting_for_bind
    }

    /// C++ `erase()`: clear the LAST active slot + callback; `false` when
    /// there is nothing to clear.
    pub fn erase(&mut self, cfg: &mut Config, callback: impl Fn(&Trigger, i32)) -> bool {
        let size = Self::get_real_size(cfg, self.trigger_id);
        if size == 0 {
            return false;
        }
        clear_trigger_bind(cfg.get_trigger(self.trigger_id), size - 1);
        callback(&cfg.trigger_copy(self.trigger_id), self.trigger_id as i32);
        true
    }

    /// C++ `newKeyboardBind(KKey)` (the key overload).
    pub fn new_keyboard_bind_key(
        &mut self,
        cfg: &mut Config,
        key: Key,
        callback: impl Fn(&Trigger, i32),
    ) -> bool {
        if key == self.hardcoded_key {
            self.waiting_for_bind = false;
            return false;
        }

        // stop if the pressed key is already assigned to this bind
        let size = Self::get_real_size(cfg, self.trigger_id);
        let trigger = cfg.trigger_copy(self.trigger_id);
        let combos = trigger.get_combos();
        for i in 0..size {
            if combos[i as usize].get_keys()[key.0 as usize + 1] {
                self.waiting_for_bind = false;
                return true;
            }
        }

        // assign the pressed key to the config value
        rebind_trigger(
            cfg.get_trigger(self.trigger_id),
            key.0,
            Button::LEFT.0,
            size,
        );
        callback(&cfg.trigger_copy(self.trigger_id), self.trigger_id as i32);
        self.waiting_for_bind = false;
        true
    }

    /// C++ `newKeyboardBind(MBtn)` (the button overload).
    pub fn new_keyboard_bind_button(
        &mut self,
        cfg: &mut Config,
        btn: Button,
        callback: impl Fn(&Trigger, i32),
    ) -> bool {
        // stop if the pressed button is already assigned to this bind
        let size = Self::get_real_size(cfg, self.trigger_id);
        let trigger = cfg.trigger_copy(self.trigger_id);
        let combos = trigger.get_combos();
        for i in 0..size {
            if combos[i as usize].get_btns()[btn.0 as usize + 1] {
                self.waiting_for_bind = false;
                return true;
            }
        }

        rebind_trigger(
            cfg.get_trigger(self.trigger_id),
            Key::UNKNOWN.0,
            btn.0,
            size,
        );
        callback(&cfg.trigger_copy(self.trigger_id), self.trigger_id as i32);
        self.waiting_for_bind = false;
        true
    }

    /// C++ `getName()`: `name + ": " + getKeyboardBindNames(ID)` (plus a
    /// trailing `"_"` while waiting for a bind).
    #[must_use]
    pub fn get_name(&self, cfg: &Config) -> String {
        let mut bind_names = cfg.get_keyboard_bind_names(self.trigger_id);
        if self.waiting_for_bind {
            bind_names.push('_');
        }
        format!("{}: {}", self.name, bind_names)
    }
}

/// C++ `hg::JoystickBindControl` (without the `ssvms::ItemBase` base).
/// `33` is the unbound sentinel button value.
#[derive(Clone, Debug)]
pub struct JoystickBindControl {
    name: String,
    button_id: Jid,
    waiting_for_bind: bool,
}

impl JoystickBindControl {
    #[must_use]
    pub fn new(name: impl Into<String>, button_id: Jid) -> Self {
        Self {
            name: name.into(),
            button_id,
            waiting_for_bind: false,
        }
    }

    /// C++ `exec()`.
    pub fn exec(&mut self) {
        self.waiting_for_bind = !self.waiting_for_bind;
    }

    /// C++ `isWaitingForBind()`.
    #[must_use]
    pub fn is_waiting_for_bind(&self) -> bool {
        self.waiting_for_bind
    }

    /// C++ `erase()`: `false` when already unbound; else set the config
    /// value to the 33 sentinel + callback.
    pub fn erase(&mut self, cfg: &mut Config, callback: impl Fn(u32, i32)) -> bool {
        if cfg.joystick_bind(self.button_id) == 33 {
            return false;
        }
        cfg.set_joystick_bind_value(self.button_id, 33);
        callback(33, self.button_id as i32);
        true
    }

    /// C++ `newJoystickBind(joy)`.
    pub fn new_joystick_bind(&mut self, cfg: &mut Config, joy: u32, callback: impl Fn(u32, i32)) {
        // stop if the pressed button is already assigned to this bind
        if joy == cfg.joystick_bind(self.button_id) {
            self.waiting_for_bind = false;
            return;
        }
        cfg.set_joystick_bind_value(self.button_id, joy);
        callback(joy, self.button_id as i32);
        self.waiting_for_bind = false;
    }

    /// C++ `getName()`.
    #[must_use]
    pub fn get_name(&self, cfg: &Config, vendor_id: u32) -> String {
        let mut bind_name = cfg.get_joystick_bind_name(self.button_id, vendor_id);
        if self.waiting_for_bind {
            bind_name.push('_');
        }
        format!("{}: {}", self.name, bind_name)
    }
}
