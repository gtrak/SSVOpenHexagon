//! C++ `ssvms::Items` — the concrete item types
//! (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Items/{Single,Slider,
//! Toggle,GoBack,Goto}.hpp`).
//!
//! Construction goes through [`crate::category::Category::create_*`]; the
//! `new` / `from_*` constructors here are crate-internal because they take
//! the `Weak` menu/category handles that only a `Category` can hand out
//! (the C++ constructors take `Menu&` / `Category&`).

use std::ops::{Add, Sub};
use std::rc::{Rc, Weak};

use oh_math::ssvu::math::get_clamped;

use crate::category::Category;
use crate::item::{Item, ItemBase};
use crate::menu::Menu;

/// C++ `ssvms::Items::Single` (`Items/Single.hpp`) — `exec` is `action()`.
pub struct Single {
    base: ItemBase,
    action: Box<dyn Fn()>,
}

impl Single {
    pub(crate) fn new(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        action: Box<dyn Fn()>,
    ) -> Self {
        Single {
            base: ItemBase::new(menu, category, name),
            action,
        }
    }
}

impl Item for Single {
    fn base(&self) -> &ItemBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ItemBase {
        &mut self.base
    }
    fn exec(&mut self) {
        (self.action)();
    }
}

/// C++ `ssvms::Items::Slider` (`Items/Slider.hpp`) — `increasable = true`;
/// `increase`/`decrease` call their actions; `getName` is
/// `name + ": " + valueGetter()`.
pub struct Slider {
    base: ItemBase,
    value_getter: Box<dyn Fn() -> String>,
    increase_action: Box<dyn Fn()>,
    decrease_action: Box<dyn Fn()>,
}

impl Slider {
    /// C++ `Slider(Menu&, Category&, const std::string&, ValueGetter,
    /// Action, Action)`
    pub(crate) fn new(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        value_getter: Box<dyn Fn() -> String>,
        increase_action: Box<dyn Fn()>,
        decrease_action: Box<dyn Fn()>,
    ) -> Self {
        let mut base = ItemBase::new(menu, category, name);
        base.mark_increasable();
        Slider {
            base,
            value_getter,
            increase_action,
            decrease_action,
        }
    }

    /// C++ templated `Slider(..., TFuncGet mFuncGet, TFuncSet mFuncSet,
    /// T mMin, T mMax, T mIncrement)` — the C++ lambdas capture the functors
    /// by value (`[=]`), so the Rust `TGet`/`TSet` must be `Clone` (closures
    /// over `Rc`-based game state are, as in the C++ game).
    ///
    /// C++ semantics:
    /// - valueGetter: `ssvu::toStr(mFuncGet())`
    /// - increase: `mFuncSet(ssvu::getClamped(mFuncGet() + mIncrement, mMin, mMax))`
    /// - decrease: `mFuncSet(ssvu::getClamped(mFuncGet() - mIncrement, mMin, mMax))`
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_value<T, TGet, TSet>(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        m_func_get: TGet,
        m_func_set: TSet,
        m_min: T,
        m_max: T,
        m_increment: T,
    ) -> Self
    where
        T: Copy + PartialOrd + Add<Output = T> + Sub<Output = T> + std::fmt::Display + 'static,
        TGet: Fn() -> T + Clone + 'static,
        TSet: Fn(T) + Clone + 'static,
    {
        // C++ `[=]` captures the functors by value: each Rust closure
        // gets its own copy.
        let get_display = m_func_get.clone();
        let get_inc = m_func_get.clone();
        let set_inc = m_func_set.clone();
        let value_getter = Box::new(move || get_display().to_string());
        let increase_action =
            Box::new(move || set_inc(get_clamped(get_inc() + m_increment, m_min, m_max)));
        let decrease_action =
            Box::new(move || m_func_set(get_clamped(m_func_get() - m_increment, m_min, m_max)));
        Slider::new(
            menu,
            category,
            name,
            value_getter,
            increase_action,
            decrease_action,
        )
    }
}

impl Item for Slider {
    fn base(&self) -> &ItemBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ItemBase {
        &mut self.base
    }
    fn increase(&mut self) {
        (self.increase_action)();
    }
    fn decrease(&mut self) {
        (self.decrease_action)();
    }
    fn get_name(&self) -> String {
        format!("{}: {}", self.base.name(), (self.value_getter)())
    }
}

/// C++ `ssvms::Items::Toggle` (`Items/Toggle.hpp`) — `increasable = true`;
/// `exec` is `predicate() ? deactivateAction() : activateAction()`;
/// `increase`/`decrease` are `exec`; `getName` is `name + ": on"` /
/// `": off"`.
pub struct Toggle {
    base: ItemBase,
    predicate: Box<dyn Fn() -> bool>,
    activate_action: Box<dyn Fn()>,
    deactivate_action: Box<dyn Fn()>,
}

impl Toggle {
    /// C++ `Toggle(Menu&, Category&, const std::string&, Predicate, Action,
    /// Action)`
    pub(crate) fn new(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        predicate: Box<dyn Fn() -> bool>,
        activate_action: Box<dyn Fn()>,
        deactivate_action: Box<dyn Fn()>,
    ) -> Self {
        let mut base = ItemBase::new(menu, category, name);
        base.mark_increasable();
        Toggle {
            base,
            predicate,
            activate_action,
            deactivate_action,
        }
    }

    /// C++ templated `Toggle(..., TFuncGet mFuncGet, TFuncSet mFuncSet)` —
    /// predicate is `mFuncGet()`, activate is `mFuncSet(true)`, deactivate
    /// is `mFuncSet(false)` (C++ `[=]` captures ⇒ `Clone` bounds).
    pub(crate) fn from_bool<TGet, TSet>(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        m_func_get: TGet,
        m_func_set: TSet,
    ) -> Self
    where
        TGet: Fn() -> bool + Clone + 'static,
        TSet: Fn(bool) + Clone + 'static,
    {
        let set_activate = m_func_set.clone();
        Toggle::new(
            menu,
            category,
            name,
            Box::new(m_func_get),
            Box::new(move || set_activate(true)),
            Box::new(move || m_func_set(false)),
        )
    }
}

impl Item for Toggle {
    fn base(&self) -> &ItemBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ItemBase {
        &mut self.base
    }
    fn exec(&mut self) {
        if (self.predicate)() {
            (self.deactivate_action)();
        } else {
            (self.activate_action)();
        }
    }
    fn increase(&mut self) {
        self.exec();
    }
    fn decrease(&mut self) {
        self.exec();
    }
    fn get_name(&self) -> String {
        if (self.predicate)() {
            format!("{}: on", self.base.name())
        } else {
            format!("{}: off", self.base.name())
        }
    }
}

/// C++ `ssvms::Items::GoBack` (`Items/GoBack.hpp`) — `exec` is
/// `menu.goBack()`.
pub struct GoBack {
    base: ItemBase,
}

impl GoBack {
    pub(crate) fn new(menu: Weak<Menu>, category: Weak<Category>, name: &str) -> Self {
        GoBack {
            base: ItemBase::new(menu, category, name),
        }
    }
}

impl Item for GoBack {
    fn base(&self) -> &ItemBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ItemBase {
        &mut self.base
    }
    fn exec(&mut self) {
        self.menu().go_back();
    }
}

/// C++ `ssvms::Items::Goto` (`Items/Goto.hpp`) — `exec` is
/// `menu.setCategory(target)`.
pub struct Goto {
    base: ItemBase,
    /// C++ `Category& target` — held as a strong `Rc` (the C++ raw ref
    /// would dangle if the category were ever dropped from the menu).
    target: Rc<Category>,
}

impl Goto {
    pub(crate) fn new(
        menu: Weak<Menu>,
        category: Weak<Category>,
        name: &str,
        target: &Rc<Category>,
    ) -> Self {
        Goto {
            base: ItemBase::new(menu, category, name),
            target: target.clone(),
        }
    }

    /// the C++ `target` category
    pub fn target(&self) -> &Rc<Category> {
        &self.target
    }
}

impl Item for Goto {
    fn base(&self) -> &ItemBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ItemBase {
        &mut self.base
    }
    fn exec(&mut self) {
        self.menu().set_category(&self.target);
    }
}
