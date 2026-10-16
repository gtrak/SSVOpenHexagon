//! SSVStart's input runtime — `Type` / `Mode` / `Bind` / `Manager` — over
//! the `oh_config::Trigger` / `oh_config::Combo` data model (issue 04) and
//! the `oh_render::input::InputState` (issue 15c).
//!
//! Oracle (D-014, SSVStart @ `6cc7cb8`,
//! `extlibs/SSVStart/include/SSVStart/Input/`):
//!
//! * `Enums.hpp:12-21` — `Type { Always, Once }`, `Mode { Overlap,
//!   Exclusive }`;
//! * `Combo.inl:12-39` — `Combo::isDown` ([`combo_is_down`] here; the
//!   `Combo` value type is `oh_config::Combo`);
//! * `Bind.hpp:17-115` + `Bind.inl:12-16` — [`Bind`] (the trigger + the
//!   type/mode + the on/off callbacks + the `released` latch + the
//!   combo/user priority);
//! * per-frame order — `GameEngine::runUpdate` (GameEngine.hpp:80-86)
//!   calls `GameState::refreshInput` (GameState.hpp:54-57) ONCE per frame
//!   before the timer burst; each timer tick's `GameEngine::updateFromTimer`
//!   (GameEngine.hpp:40-50) calls `GameState::updateInput`
//!   (GameState.hpp:49-52) BEFORE the state's `update`.
//!
//! The C++ `Bind` stores a `Manager&` and `Combo::isDown` writes
//! `manager.processedInput` (Combo.inl:26-29,35-36); the port threads the
//! manager's `processedInput` (`&mut InputState`) through the same call
//! chain — same reads, same writes, same order. `processed` and `input`
//! must be distinct objects (the manager's `processedInput` vs the
//! window's `InputState`, `GameWindow::inputState`, GameWindow.hpp:30).

use oh_config::{Combo, Trigger};

use crate::input::InputState;

/// C++ `ssvs::Input::Type` (`Input/Enums.hpp:12-16`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// `Type::Always` — active on every frame the combo is down.
    Always,
    /// `Type::Once` — active only on the frame the combo goes down (the
    /// `released` latch, `Bind.hpp:94-103`).
    Once,
}

/// C++ `ssvs::Input::Mode` (`Input/Enums.hpp:17-21`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `Mode::Overlap` — down regardless of this frame's already-processed
    /// input.
    Overlap,
    /// `Mode::Exclusive` — down only if none of the combo's keys/btns were
    /// processed earlier in this frame (`Combo.inl:24-30`).
    Exclusive,
}

/// C++ `Combo::isDown(mManager, mInputState, mMode)` (`Combo.inl:12-39`).
///
/// `processed` is the manager's `processedInput`: on a positive match the
/// combo's keys/btns are added to it (`Combo.inl:35-36`), and an
/// [`Mode::Exclusive`] match is rejected when any of the combo's keys/btns
/// are already there (`Combo.inl:26-29`).
#[must_use]
pub fn combo_is_down(
    combo: &Combo,
    processed: &mut InputState,
    input: &InputState,
    mode: Mode,
) -> bool {
    // "If this combo is unbound, it is not valid" (Combo.inl:15-16).
    if combo.is_unbound() {
        return false;
    }

    // "Check if the combo's keys/btns are currently pressed"
    // (Combo.inl:18-20): `(mInputState.getKeys() & keys) != keys`.
    let keys = combo.get_keys();
    if !keys.iter().zip(input.get_keys()).all(|(c, i)| !c || *i) {
        return false;
    }
    let btns = combo.get_btns();
    if !btns.iter().zip(input.get_btns()).all(|(c, i)| !c || *i) {
        return false;
    }

    if mode == Mode::Exclusive {
        // Combo.inl:24-30.
        if keys.iter().zip(processed.get_keys()).any(|(c, p)| *c && *p) {
            return false;
        }
        if btns.iter().zip(processed.get_btns()).any(|(c, p)| *c && *p) {
            return false;
        }
    }

    // "Combo will return true: add its keys/btns to this frame's processed
    // inputs" (Combo.inl:32-36).
    {
        let pkeys = processed.get_keys_mut();
        for (i, c) in keys.iter().enumerate() {
            if *c {
                pkeys[i] = true;
            }
        }
        let pbtns = processed.get_btns_mut();
        for (i, c) in btns.iter().enumerate() {
            if *c {
                pbtns[i] = true;
            }
        }
    }

    true
}

/// C++ `ssvs::Input::Bind` (`Input/Bind.hpp:17-115`).
///
/// One registered trigger: its `Trigger` (up to `Config::MAX_BINDS = 4`
/// combos, ANY of which activates), the on/off callbacks, the `Type` /
/// `Mode`, the `released` latch (the `Type::Once` edge detector), and the
/// sort priority.
pub struct Bind {
    trigger: Trigger,
    on: Box<dyn FnMut(f32)>,
    off: Box<dyn FnMut(f32)>,
    rtype: Type,
    mode: Mode,
    released: bool,
    trigger_id: i32,
    /// C++ `priorityCombo` (`Bind.hpp:25` — "calculated automatically").
    priority_combo: usize,
    /// C++ `priorityUser` (`Bind.hpp:26` — "set by user";
    /// `setPriorityUser`, `Bind.inl:12-16`).
    priority_user: u64,
    /// The C++ LAlt+Enter bind's action calls `game.ignoreNextInputs()`
    /// (MenuGame.cpp:994-1002 → `Manager::isIgnoringNext`, Manager.hpp:74-77)
    /// from INSIDE its `on` callback; the port expresses the same effect as
    /// a per-bind flag (the port's callback runs inside the manager's own
    /// `update` and cannot re-enter it) — [`Manager::update`] sets the
    /// manager's `isIgnoringNext` when this bind's `on` fires.
    ignore_next_on_fire: bool,
}

impl Bind {
    /// `Manager::emplace`'s construction (`Manager.hpp:59-66`) +
    /// `Bind::Bind` (`Bind.hpp:48-61`).
    #[must_use]
    pub fn new(
        trigger: Trigger,
        rtype: Type,
        mode: Mode,
        trigger_id: i32,
        on: impl FnMut(f32) + 'static,
        off: impl FnMut(f32) + 'static,
    ) -> Self {
        let mut this = Self {
            trigger,
            on: Box::new(on),
            off: Box::new(off),
            rtype,
            mode,
            released: true,
            trigger_id,
            priority_combo: 0,
            priority_user: 0,
            ignore_next_on_fire: false,
        };
        this.recalculate_priority_combo();
        this
    }

    /// C++ `Bind::isDown` (`Bind.hpp:32-37`): the first combo down (its
    /// positive `isDown` marks `processed`).
    fn is_down(&self, processed: &mut InputState, input: &InputState) -> bool {
        for c in self.trigger.get_combos() {
            if combo_is_down(c, processed, input, self.mode) {
                return true;
            }
        }
        false
    }

    /// C++ `Bind::update` (`Bind.hpp:63-66`): `isActive ? on(mFT) :
    /// off(mFT)`. Returns whether the `on` callback fired (the C++ is
    /// `void`; the manager's `isIgnoringNext` hook needs the distinction).
    pub fn update(&mut self, ft: f32, input: &InputState, processed: &mut InputState) -> bool {
        if self.is_active(input, processed) {
            (self.on)(ft);
            true
        } else {
            (self.off)(ft);
            false
        }
    }

    /// C++ `Bind::refresh` (`Bind.hpp:67-70`): `!released && !isDown`
    /// re-arms the latch.
    pub fn refresh(&mut self, input: &InputState, processed: &mut InputState) {
        if !self.released && !self.is_down(processed, input) {
            self.released = true;
        }
    }

    /// C++ `Bind::refreshTrigger` (`Bind.hpp:71-75`).
    pub fn refresh_trigger(&mut self, trigger: &Trigger) {
        self.trigger = trigger.clone();
        self.recalculate_priority_combo();
    }

    /// C++ `Bind::getTriggerID` (`Bind.hpp:76-79`).
    #[must_use]
    pub fn get_trigger_id(&self) -> i32 {
        self.trigger_id
    }

    /// C++ `Bind::isActive` (`Bind.hpp:94-103`).
    pub fn is_active(&mut self, input: &InputState, processed: &mut InputState) -> bool {
        if self.rtype == Type::Always {
            return self.is_down(processed, input);
        }
        // `Type::Once` (Bind.hpp:97-101): fire on the down frame only —
        // the latch is set BEFORE the `on` callback runs (the manager then
        // cannot re-fire the same press within the frame's ticks).
        if self.released && self.is_down(processed, input) {
            self.released = false;
            return true;
        }
        false
    }

    /// C++ `Bind::operator<` (`Bind.hpp:105-112`) — the `std::sort`
    /// comparator (`Manager::refresh`, `Manager.hpp:50-52`).
    pub fn less_than(&self, rhs: &Bind) -> bool {
        // "User-defined priority takes precedence" (Bind.hpp:107): a
        // differing `priorityUser` compares the COMBO priority instead
        // (descending); equal users compare the user priority (ascending).
        if self.priority_user != rhs.priority_user {
            return self.priority_combo > rhs.priority_combo;
        }
        self.priority_user < rhs.priority_user
    }

    /// C++ `Bind::setPriorityUser` (`Bind.inl:12-16`); the
    /// `manager.mustSort = true` half is the manager's
    /// (`Manager::set_priority_user`).
    pub fn set_priority_user(&mut self, value: u64) {
        self.priority_user = value;
    }

    /// Marks the bind's `on` callback as the C++
    /// `game.ignoreNextInputs()` caller (MenuGame.cpp:994-1002) — see the
    /// `ignore_next_on_fire` field docs.
    pub fn set_ignore_next_on_fire(&mut self) {
        self.ignore_next_on_fire = true;
    }

    /// C++ `Bind::recalculatePriorityCombo` (`Bind.hpp:39-45`): the max,
    /// over the combos, of `getKeys().count() + getBtns().count()` — the
    /// bitset counts include the `Unknown` bit (an unbound combo counts 1).
    fn recalculate_priority_combo(&mut self) {
        let mut max = 0;
        for c in self.trigger.get_combos() {
            max = max.max(
                c.get_keys().iter().filter(|b| **b).count()
                    + c.get_btns().iter().filter(|b| **b).count(),
            );
        }
        self.priority_combo = max;
    }
}

/// C++ `ssvs::Input::Manager` (`Input/Manager.hpp:21-82`) — one per
/// `ssvs::GameState` (the C++ `HexagonGame::game` and `MenuGame::game`
/// each own one, GameState.hpp:64-75).
#[derive(Default)]
pub struct Manager {
    /// C++ `processedInput` (`Manager.hpp:27`) — this frame's already
    /// consumed keys/btns (the `Mode::Exclusive` check, Combo.inl:26-29);
    /// reset by [`Self::refresh`] (Manager.hpp:56).
    processed_input: InputState,
    /// C++ `binds` (`Manager.hpp:28`).
    binds: Vec<Bind>,
    /// C++ `isIgnoringNext` (`Manager.hpp:29`).
    is_ignoring_next: bool,
    /// C++ `isIgnoringAll` (`Manager.hpp:29`).
    is_ignoring_all: bool,
    /// C++ `mustSort` (`Manager.hpp:29`).
    must_sort: bool,
}

impl Manager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// C++ `Manager::update` (`Manager.hpp:32-45`) — once per timer tick,
    /// AFTER the frame's [`Self::refresh`] (GameEngine.hpp:40-50,80-86).
    pub fn update(&mut self, input: &InputState, ft: f32) {
        if self.is_ignoring_all {
            return;
        }

        for bind in self.binds.iter_mut() {
            // C++ `b->update(mFT, mInputState)` (Manager.hpp:38):
            // `isActive ? on : off` per bind.
            let fired = bind.update(ft, input, &mut self.processed_input);

            // The LAlt+Enter action's `game.ignoreNextInputs()`
            // (MenuGame.cpp:994-1002 → Manager.hpp:74-77), expressed as the
            // bind's `ignore_next_on_fire` flag (the C++ lambda could reach
            // the manager because the C++ `game` object owns it).
            if fired && bind.ignore_next_on_fire {
                self.is_ignoring_next = true;
            }

            // "Some special input combos, such as ALT+ENTER (fullscreen),
            // may work best if all the other inputs are then ignored."
            // (Manager.hpp:40-43)
            if self.is_ignoring_next {
                break;
            }
        }
    }

    /// C++ `Manager::refresh` (`Manager.hpp:46-58`) — once per frame,
    /// BEFORE the timer's update burst (GameEngine.hpp:84 →
    /// GameState.hpp:54-57): sort if dirty, reset the ignore-next flag +
    /// the `processedInput`, then the binds' release-latch pass.
    pub fn refresh(&mut self, input: &InputState) {
        if self.must_sort {
            // C++ `std::sort(begin, end, [](a, b) { return *a < *b; })`
            // (Manager.hpp:50-52) over `Bind::operator<` (Bind.hpp:105-112).
            self.binds.sort_unstable_by(|a, b| {
                if a.less_than(b) {
                    std::cmp::Ordering::Less
                } else if b.less_than(a) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            });
            self.must_sort = false;
        }

        self.is_ignoring_next = false;
        self.processed_input.reset();
        for bind in self.binds.iter_mut() {
            bind.refresh(input, &mut self.processed_input);
        }
    }

    /// C++ `Manager::emplace` (`Manager.hpp:59-66`) reached through
    /// `GameState::addInput` (`GameState.hpp:69-75`): append the bind +
    /// `mustSort = true`. Returns the bind's index (the C++ returns
    /// `Bind&`).
    pub fn add_input(
        &mut self,
        trigger: Trigger,
        on: impl FnMut(f32) + 'static,
        off: impl FnMut(f32) + 'static,
        rtype: Type,
        trigger_id: i32,
        mode: Mode,
    ) -> usize {
        let idx = self.binds.len();
        self.binds
            .push(Bind::new(trigger, rtype, mode, trigger_id, on, off));
        self.must_sort = true;
        idx
    }

    /// `GameState::addInput`'s no-off overload (`GameState.hpp:77-84`):
    /// `mFuncOff = [](ssvu::FT) {}`, `mMode = IMode::Overlap`.
    pub fn add_input_action(
        &mut self,
        trigger: Trigger,
        on: impl FnMut(f32) + 'static,
        rtype: Type,
        trigger_id: i32,
    ) -> usize {
        self.add_input(trigger, on, |_| {}, rtype, trigger_id, Mode::Overlap)
    }

    /// C++ `Bind::setPriorityUser`'s flag half (`Bind.inl:12-16`:
    /// `manager.mustSort = true`) for the bind at `bind`.
    pub fn set_priority_user(&mut self, bind: usize, value: u64) {
        self.binds[bind].set_priority_user(value);
        self.must_sort = true;
    }

    /// Marks the bind at `bind` as the C++
    /// `game.ignoreNextInputs()`-from-its-`on`-action shape
    /// (MenuGame.cpp:994-1002) — see `Bind::set_ignore_next_on_fire`.
    pub fn set_ignore_next_on_fire(&mut self, bind: usize) {
        self.binds[bind].set_ignore_next_on_fire();
    }

    /// C++ `Manager::refreshTriggers` (`Manager.hpp:68-72`) — the
    /// `refreshBinds` / rebind path (MenuGame.cpp:3373-3387, :1233-1240;
    /// HexagonGame.cpp:467-470): every bind registered with `bind_id`
    /// re-points at the new trigger + re-computes its combo priority.
    pub fn refresh_triggers(&mut self, trigger: &Trigger, bind_id: i32) {
        for bind in self.binds.iter_mut() {
            if bind.get_trigger_id() == bind_id {
                bind.refresh_trigger(trigger);
            }
        }
    }

    /// C++ `Manager::ignoreNextInputs` (`Manager.hpp:74-77`).
    pub fn ignore_next_inputs(&mut self) {
        self.is_ignoring_next = true;
    }

    /// C++ `Manager::ignoreAllInputs` (`Manager.hpp:78-81`).
    pub fn ignore_all_inputs(&mut self, ignore: bool) {
        self.is_ignoring_all = ignore;
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use oh_config::{Button, Key};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<String>>>;

    fn log() -> Log {
        Rc::new(RefCell::new(Vec::new()))
    }

    /// Recording on/off closures (the C++ `InputFunc` pair).
    fn recorder(log: Log, name: &'static str) -> (impl FnMut(f32), impl FnMut(f32)) {
        let on_log = log.clone();
        (
            move |_ft| {
                on_log.borrow_mut().push(format!("{name}:on"));
            },
            move |_ft| {
                log.borrow_mut().push(format!("{name}:off"));
            },
        )
    }

    fn single_key(k: Key) -> Trigger {
        Trigger::from_combos(vec![{
            let mut c = Combo::new();
            c.add_key(k);
            c
        }])
    }

    /// One C++ frame for a standalone bind: `refresh` then `update`
    /// (GameEngine.hpp:80-86 + :40-50), with a FRESH `processedInput`
    /// (the manager's `refresh` resets it, Manager.hpp:56).
    fn frame(bind: &mut Bind, input: &InputState) {
        let mut processed = InputState::new();
        bind.refresh(input, &mut processed);
        bind.update(1.0, input, &mut processed);
    }

    fn pressed(k: Key) -> InputState {
        let mut s = InputState::new();
        s.set_key_pressed(k, true);
        s
    }

    fn empty() -> InputState {
        InputState::new()
    }

    // -- combo matching -----------------------------------------------------

    #[test]
    fn combo_down_requires_every_key() {
        let mut combo = Combo::new();
        combo.add_key(Key::A);
        combo.add_key(Key::W);
        let mut processed = InputState::new();

        assert!(
            !combo_is_down(&combo, &mut processed, &pressed(Key::A), Mode::Overlap),
            "one of two keys"
        );
        assert!(
            !combo_is_down(&combo, &mut processed, &pressed(Key::W), Mode::Overlap),
            "the other of two keys"
        );

        let both = {
            let mut s = InputState::new();
            s.set_key_pressed(Key::A, true);
            s.set_key_pressed(Key::W, true);
            s
        };
        assert!(combo_is_down(&combo, &mut processed, &both, Mode::Overlap));
        // the match marks the combo's keys in `processed` (Combo.inl:35-36)
        assert!(processed.get_keys()[Key::A.0 as usize + 1]);
        assert!(processed.get_keys()[Key::W.0 as usize + 1]);
    }

    #[test]
    fn combo_button_match() {
        let mut combo = Combo::new();
        combo.add_btn(Button::LEFT);
        let mut processed = InputState::new();
        assert!(!combo_is_down(
            &combo,
            &mut processed,
            &empty(),
            Mode::Overlap
        ));

        let down = {
            let mut s = InputState::new();
            s.set_button_pressed(Button::LEFT, true);
            s
        };
        assert!(combo_is_down(&combo, &mut processed, &down, Mode::Overlap));
        assert!(processed.get_btns()[Button::LEFT.0 as usize + 1]);
    }

    #[test]
    fn unbound_combo_never_down() {
        let combo = Combo::cleared();
        let mut processed = InputState::new();
        assert!(!combo_is_down(
            &combo,
            &mut processed,
            &pressed(Key::A),
            Mode::Overlap
        ));
    }

    #[test]
    fn exclusive_rejects_already_processed_keys() {
        let mut processed = InputState::new();
        processed.set_key_pressed(Key::A, true); // an earlier bind consumed A
        assert!(
            !combo_is_down(
                &single_key(Key::A).get_combos()[0],
                &mut processed,
                &pressed(Key::A),
                Mode::Exclusive
            ),
            "exclusive: A already processed this frame"
        );
        assert!(
            combo_is_down(
                &single_key(Key::A).get_combos()[0],
                &mut processed,
                &pressed(Key::A),
                Mode::Overlap
            ),
            "overlap: blind to processedInput"
        );
    }

    // -- bind semantics -------------------------------------------------------

    #[test]
    fn once_fires_only_on_the_down_frame() {
        let l = log();
        let (on, off) = recorder(l.clone(), "once");
        let mut bind = Bind::new(single_key(Key::A), Type::Once, Mode::Overlap, -1, on, off);

        frame(&mut bind, &pressed(Key::A)); // down frame
        frame(&mut bind, &pressed(Key::A)); // held
        frame(&mut bind, &empty()); // released
        frame(&mut bind, &pressed(Key::A)); // down again
        assert_eq!(
            *l.borrow(),
            vec!["once:on", "once:off", "once:off", "once:on"]
        );
    }

    #[test]
    fn always_fires_every_down_frame_and_off_every_other() {
        let l = log();
        let (on, off) = recorder(l.clone(), "always");
        let mut bind = Bind::new(single_key(Key::A), Type::Always, Mode::Overlap, -1, on, off);

        frame(&mut bind, &empty());
        frame(&mut bind, &pressed(Key::A));
        frame(&mut bind, &pressed(Key::A));
        frame(&mut bind, &empty());
        assert_eq!(
            *l.borrow(),
            vec!["always:off", "always:on", "always:on", "always:off"]
        );
    }

    #[test]
    fn release_transition_rearms_the_once_latch() {
        // The C++ latch arms in `refresh` when the combo is no longer down
        // (Bind.hpp:67-70) — WITHOUT a release frame in between, a re-press
        // cannot fire.
        let l = log();
        let (on, off) = recorder(l.clone(), "once");
        let mut bind = Bind::new(single_key(Key::A), Type::Once, Mode::Overlap, -1, on, off);

        frame(&mut bind, &pressed(Key::A)); // down → on, latch off
                                            // held with the latch off: no re-fire
        frame(&mut bind, &pressed(Key::A));
        // released → `refresh` re-arms
        frame(&mut bind, &empty());
        // held again (a new press) → fires
        frame(&mut bind, &pressed(Key::A));
        assert_eq!(
            *l.borrow(),
            vec!["once:on", "once:off", "once:off", "once:on"]
        );
    }

    #[test]
    fn multi_combo_trigger_any_combo_activates() {
        let l = log();
        let (on, off) = recorder(l.clone(), "multi");
        let mut c1 = Combo::new();
        c1.add_key(Key::A);
        let mut c2 = Combo::new();
        c2.add_key(Key::D);
        let c3 = Combo::cleared(); // an unbound slot (the C++ four-slot shape)
        let mut bind = Bind::new(
            Trigger::from_combos(vec![c1, c2, c3]),
            Type::Once,
            Mode::Overlap,
            0,
            on,
            off,
        );

        frame(&mut bind, &pressed(Key::D)); // the 2nd combo
        assert_eq!(*l.borrow(), vec!["multi:on"]);
        assert_eq!(bind.get_trigger_id(), 0);
    }

    #[test]
    fn refresh_trigger_repoints_the_bind() {
        let l = log();
        let (on, off) = recorder(l.clone(), "repoint");
        let mut bind = Bind::new(single_key(Key::A), Type::Once, Mode::Overlap, 7, on, off);

        frame(&mut bind, &pressed(Key::A));
        bind.refresh_trigger(&single_key(Key::S));
        frame(&mut bind, &pressed(Key::A)); // old key: no fire
        frame(&mut bind, &empty());
        frame(&mut bind, &pressed(Key::S)); // new key: fires
        assert_eq!(
            *l.borrow(),
            vec!["repoint:on", "repoint:off", "repoint:off", "repoint:on"]
        );
    }

    #[test]
    fn priority_comparison_mirrors_cpp_operator_lt() {
        let big = {
            let mut c = Combo::new();
            c.add_key(Key::L_ALT);
            c.add_key(Key::ENTER);
            c
        };
        let (a_on, a_off) = recorder(log(), "w");
        let a = Bind::new(
            single_key(Key::W),
            Type::Always,
            Mode::Overlap,
            0,
            a_on,
            a_off,
        );
        let (b_on, b_off) = recorder(log(), "alt-enter");
        let mut b = Bind::new(
            Trigger::from_combos(vec![big]),
            Type::Once,
            Mode::Overlap,
            0,
            b_on,
            b_off,
        );
        // C++ `setPriorityUser(-1000)` (MenuGame.cpp:1002): an `int` into a
        // `std::size_t` wraps to `usize::MAX - 999`.
        b.set_priority_user((usize::MAX - 999) as u64);

        // differing priorityUser → the COMBO priority decides (descending):
        // B (2 keys) sorts before A (1 key).
        assert!(b.less_than(&a));
        assert!(!a.less_than(&b));

        // equal priorityUser (both 0) → `0 < 0` = false both ways.
        let (c_on, c_off) = recorder(log(), "x");
        let c = Bind::new(
            single_key(Key::X),
            Type::Always,
            Mode::Overlap,
            0,
            c_on,
            c_off,
        );
        assert!(!a.less_than(&c) && !c.less_than(&a));
    }

    // -- manager -------------------------------------------------------------

    /// One C++ frame through the manager: `refresh` (per-frame, before the
    /// burst) + N×`update` (per tick, GameEngine.hpp:40-50,80-86).
    fn manager_frame(mgr: &mut Manager, input: &InputState, ticks: u32) {
        mgr.refresh(input);
        for _ in 0..ticks {
            mgr.update(input, 0.25);
        }
    }

    #[test]
    fn manager_once_semantics_over_frames() {
        let l = log();
        let (on, off) = recorder(l.clone(), "once");
        let mut mgr = Manager::new();
        mgr.add_input(single_key(Key::A), on, off, Type::Once, -1, Mode::Overlap);

        manager_frame(&mut mgr, &pressed(Key::A), 2); // press frame: 2 ticks
        manager_frame(&mut mgr, &pressed(Key::A), 2); // held
        manager_frame(&mut mgr, &empty(), 1); // release
        manager_frame(&mut mgr, &pressed(Key::A), 1); // press again
        assert_eq!(
            *l.borrow(),
            vec![
                "once:on",  // the press frame's 1st tick
                "once:off", // 2nd tick: the latch is already off (same frame)
                "once:off", "once:off", "once:off", "once:on",
            ]
        );
    }

    #[test]
    fn manager_always_bind_tracks_the_held_key() {
        let l = log();
        let (on, off) = recorder(l.clone(), "always");
        let mut mgr = Manager::new();
        mgr.add_input(single_key(Key::S), on, off, Type::Always, -1, Mode::Overlap);

        manager_frame(&mut mgr, &empty(), 1);
        manager_frame(&mut mgr, &pressed(Key::S), 1);
        manager_frame(&mut mgr, &pressed(Key::S), 1);
        manager_frame(&mut mgr, &empty(), 1);
        assert_eq!(
            *l.borrow(),
            vec!["always:off", "always:on", "always:on", "always:off"]
        );
    }

    #[test]
    fn manager_exclusive_rejects_a_later_bind_on_the_same_key() {
        let l = log();
        let (a_on, a_off) = recorder(l.clone(), "overlap");
        let (b_on, b_off) = recorder(l.clone(), "exclusive");
        let mut mgr = Manager::new();
        mgr.add_input(
            single_key(Key::A),
            a_on,
            a_off,
            Type::Always,
            -1,
            Mode::Overlap,
        );
        mgr.add_input(
            single_key(Key::A),
            b_on,
            b_off,
            Type::Always,
            -1,
            Mode::Exclusive,
        );

        manager_frame(&mut mgr, &pressed(Key::A), 1);
        // The overlap bind processed A first → the exclusive bind is
        // suppressed this frame (Combo.inl:24-30).
        assert_eq!(*l.borrow(), vec!["overlap:on", "exclusive:off"]);

        // Reversed registration: the exclusive bind gets the key; the
        // overlap bind is blind to processedInput and fires too.
        let l2 = log();
        let (a_on, a_off) = recorder(l2.clone(), "exclusive");
        let (b_on, b_off) = recorder(l2.clone(), "overlap");
        let mut mgr = Manager::new();
        mgr.add_input(
            single_key(Key::A),
            a_on,
            a_off,
            Type::Always,
            -1,
            Mode::Exclusive,
        );
        mgr.add_input(
            single_key(Key::A),
            b_on,
            b_off,
            Type::Always,
            -1,
            Mode::Overlap,
        );
        manager_frame(&mut mgr, &pressed(Key::A), 1);
        assert_eq!(*l2.borrow(), vec!["exclusive:on", "overlap:on"]);
    }

    #[test]
    fn manager_refresh_triggers_repoints_binds_by_trigger_id() {
        let l = log();
        let (on, off) = recorder(l.clone(), "repoint");
        let mut mgr = Manager::new();
        mgr.add_input(single_key(Key::A), on, off, Type::Once, 5, Mode::Overlap);
        // a second bind with a DIFFERENT id must not move
        let l2 = log();
        let (on2, off2) = recorder(l2.clone(), "other");
        mgr.add_input(single_key(Key::A), on2, off2, Type::Once, 6, Mode::Overlap);

        manager_frame(&mut mgr, &pressed(Key::A), 1);
        mgr.refresh_triggers(&single_key(Key::Q), 5);
        manager_frame(&mut mgr, &empty(), 1); // re-arm both
        manager_frame(&mut mgr, &pressed(Key::A), 1); // old key: only bind 6 fires
        assert_eq!(
            *l.borrow(),
            vec!["repoint:on", "repoint:off", "repoint:off"]
        );
        assert_eq!(*l2.borrow(), vec!["other:on", "other:off", "other:on"]);

        manager_frame(&mut mgr, &empty(), 1);
        manager_frame(&mut mgr, &pressed(Key::Q), 1); // new key: bind 5 fires
        assert_eq!(
            *l.borrow(),
            vec![
                "repoint:on",
                "repoint:off",
                "repoint:off",
                "repoint:off",
                "repoint:on"
            ]
        );
    }

    #[test]
    fn manager_ignore_all_silences_every_bind() {
        let l = log();
        let (on, off) = recorder(l.clone(), "x");
        let mut mgr = Manager::new();
        mgr.add_input(single_key(Key::A), on, off, Type::Always, -1, Mode::Overlap);

        mgr.ignore_all_inputs(true);
        manager_frame(&mut mgr, &pressed(Key::A), 1);
        assert!(
            l.borrow().is_empty(),
            "ignored: no callback at all (Manager.hpp:34)"
        );

        mgr.ignore_all_inputs(false);
        manager_frame(&mut mgr, &pressed(Key::A), 1);
        assert_eq!(*l.borrow(), vec!["x:on"]);
    }

    #[test]
    fn manager_ignore_next_stops_after_the_firing_bind() {
        let l = log();
        let (alt_on, alt_off) = recorder(l.clone(), "alt-enter");
        let (other_on, other_off) = recorder(l.clone(), "other");
        let mut mgr = Manager::new();
        // C++ registration order (MenuGame.cpp:990-1002): the LAlt+Enter
        // bind fires first, then `game.ignoreNextInputs()` — everything
        // after it in the loop is skipped for the frame.
        let alt = {
            let mut c = Combo::new();
            c.add_key(Key::L_ALT);
            c.add_key(Key::ENTER);
            c
        };
        let idx = mgr.add_input(
            Trigger::from_combos(vec![alt]),
            alt_on,
            alt_off,
            Type::Once,
            -1,
            Mode::Overlap,
        );
        mgr.set_priority_user(idx, (usize::MAX - 999) as u64); // C++ -1000
        mgr.set_ignore_next_on_fire(idx);
        mgr.add_input(
            single_key(Key::A),
            other_on,
            other_off,
            Type::Always,
            -1,
            Mode::Overlap,
        );

        // Both keys down, 2 ticks in the frame: the `other` bind never
        // fires (the C++ `isIgnoringNext` persists until the frame's
        // `refresh` — Manager.hpp:55).
        let down = {
            let mut s = InputState::new();
            s.set_key_pressed(Key::L_ALT, true);
            s.set_key_pressed(Key::ENTER, true);
            s.set_key_pressed(Key::A, true);
            s
        };
        manager_frame(&mut mgr, &down, 2);
        assert_eq!(*l.borrow(), vec!["alt-enter:on", "alt-enter:off"]);

        // Next frame: the Once latch re-arms (the keys are released and
        // re-pressed) and the ignore flag resets (Manager.hpp:55); `other`
        // still never fires WHILE the LAlt+Enter bind is down.
        manager_frame(&mut mgr, &empty(), 1); // f2: everything released
        let mut down2 = down;
        down2.set_key_pressed(Key::A, true); // still held for `other`
        manager_frame(&mut mgr, &down2, 1); // f3: LAlt+Enter re-pressed
        assert_eq!(
            *l.borrow(),
            vec![
                "alt-enter:on",  // f1 t1: the press tick
                "alt-enter:off", // f1 t2: the latch is already off (same frame)
                "alt-enter:off", // f2: released
                "other:off",     // f2: `other` resumes (flag reset at refresh)
                "alt-enter:on",  // f3: re-pressed → ignore flag set again
            ]
        );

        // f4: release everything; f5: hold only A — `other` fires on its
        // own (the ignore flag is set only while the LAlt+Enter `on`
        // fires).
        manager_frame(&mut mgr, &empty(), 1);
        let other_held = {
            let mut s = InputState::new();
            s.set_key_pressed(Key::A, true);
            s
        };
        manager_frame(&mut mgr, &other_held, 1);
        assert_eq!(
            *l.borrow(),
            vec![
                "alt-enter:on",
                "alt-enter:off",
                "alt-enter:off",
                "other:off",
                "alt-enter:on",
                "alt-enter:off", // f4: released
                "other:off",     // f4
                "alt-enter:off", // f5: alt not pressed
                "other:on",      // f5: A held, no ignore flag
            ]
        );
    }

    #[test]
    fn manager_sort_orders_the_higher_combo_priority_first() {
        let l = log();
        let (single_on, single_off) = recorder(l.clone(), "single");
        let (pair_on, pair_off) = recorder(l.clone(), "pair");
        let mut mgr = Manager::new();
        mgr.add_input(
            single_key(Key::W),
            single_on,
            single_off,
            Type::Always,
            -1,
            Mode::Overlap,
        );
        let pair = {
            let mut c = Combo::new();
            c.add_key(Key::L_ALT);
            c.add_key(Key::ENTER);
            c
        };
        let idx = mgr.add_input(
            Trigger::from_combos(vec![pair]),
            pair_on,
            pair_off,
            Type::Always,
            -1,
            Mode::Overlap,
        );
        mgr.set_priority_user(idx, (usize::MAX - 999) as u64); // C++ -1000

        // `refresh` sorts on the first frame (mustSort from add_input);
        // the 2-key bind (priorityUser != 0) must run BEFORE the 1-key
        // bind (Bind.hpp:105-112 descending-combo branch).
        let down = {
            let mut s = InputState::new();
            s.set_key_pressed(Key::W, true);
            s.set_key_pressed(Key::L_ALT, true);
            s.set_key_pressed(Key::ENTER, true);
            s
        };
        manager_frame(&mut mgr, &down, 1);
        assert_eq!(*l.borrow(), vec!["pair:on", "single:on"]);
    }
}
