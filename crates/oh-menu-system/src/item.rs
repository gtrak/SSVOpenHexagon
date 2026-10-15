//! C++ `ssvms::ItemBase` (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Menu/
//! ItemBase.hpp`) plus the dynamic-dispatch trait every item type implements
//! (C++ `UniquePtr<ItemBase>` + virtual functions).

use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::category::Category;
use crate::menu::Menu;

/// `ssvms::Action` (Global/Typedefs.hpp): `std::function<void()>`.
pub type Action = Box<dyn Fn()>;

/// `ssvms::Predicate` (Global/Typedefs.hpp): `std::function<bool()>`.
pub type Predicate = Box<dyn Fn() -> bool>;

/// `ssvms::ItemBase` data members and defaults:
///
/// ```cpp
/// Menu& menu; Category& category; std::string name;
/// bool enabled{true}; bool increasable{false}; float offset{0.f};
/// virtual void exec() {} virtual void increase() {} virtual void decrease() {};
/// virtual std::string getName() const { return name; }
/// ```
///
/// The C++ raw `Menu&` / `Category&` back-refs become `Weak` handles: the
/// owning `Menu` keeps both alive for as long as the item exists, so
/// `upgrade()` only fails if the menu was already dropped (where C++ would
/// dangle).
pub struct ItemBase {
    menu: Weak<Menu>,
    category: Weak<Category>,
    name: String,
    enabled: bool,
    increasable: bool,
    offset: f32,
}

impl ItemBase {
    /// `ItemBase(Menu&, Category&, const std::string&)`
    pub fn new(menu: Weak<Menu>, category: Weak<Category>, name: &str) -> Self {
        ItemBase {
            menu,
            category,
            name: name.to_string(),
            enabled: true,
            increasable: false,
            offset: 0.0,
        }
    }

    /// `ItemBase::getMenu`
    pub fn menu(&self) -> Rc<Menu> {
        self.menu
            .upgrade()
            .expect("menu outlives its items; upgrade failure means the menu was dropped")
    }

    /// C++ `Category& category` back-ref
    pub fn category(&self) -> Rc<Category> {
        self.category
            .upgrade()
            .expect("category outlives its items; upgrade failure means the menu was dropped")
    }

    /// `ItemBase::getName` (the bare name — the virtual display form is
    /// [`Item::get_name`], overridden by Slider/Toggle).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// C++ `getName()` default: returns the bare name.
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    /// `ItemBase::setEnabled`
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// `ItemBase::isEnabled`
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// `ItemBase::canIncrease`
    pub fn can_increase(&self) -> bool {
        self.increasable
    }

    /// `ItemBase::getOffset` (`float&`)
    pub fn offset_mut(&mut self) -> &mut f32 {
        &mut self.offset
    }

    /// C++ `increasable = true` in the Slider/Toggle constructors.
    pub(crate) fn mark_increasable(&mut self) {
        self.increasable = true;
    }
}

/// Dynamic dispatch over the item types — C++'s `UniquePtr<ItemBase>` plus
/// the virtual `exec` / `increase` / `decrease` / `getName`.
///
/// Concrete types live in [`crate::items`]; each embeds an [`ItemBase`] and
/// exposes it through `base` / `base_mut`.
pub trait Item: Any {
    fn base(&self) -> &ItemBase;
    fn base_mut(&mut self) -> &mut ItemBase;

    // ItemBase virtual defaults (empty bodies in C++)
    fn exec(&mut self) {}
    fn increase(&mut self) {}
    fn decrease(&mut self) {}

    // ItemBase::getName — virtual; default returns the bare name
    fn get_name(&self) -> String {
        self.base().get_name()
    }

    // ItemBase public accessors, mirrored on the trait so they are callable
    // through `dyn Item` (i.e. through `Menu::get_item` / `Category::get_item`)
    fn name(&self) -> &str {
        self.base().name()
    }
    fn is_enabled(&self) -> bool {
        self.base().is_enabled()
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.base_mut().set_enabled(enabled);
    }
    fn can_increase(&self) -> bool {
        self.base().can_increase()
    }
    fn menu(&self) -> Rc<Menu> {
        self.base().menu()
    }
    fn category(&self) -> Rc<Category> {
        self.base().category()
    }
}

/// C++ `operator|` on a menu item (`Menu.hpp`): `item | predicate` —
/// `item.getMenu().getMenuController().enableItemWhen(item, predicate)`.
///
/// Extension trait (a direct inherent `impl Rc<RefCell<dyn Item>>` is
/// rejected by the orphan rule) so the call site reads `item.when(pred)`.
pub trait ItemExt {
    fn when(&self, predicate: impl Fn() -> bool + 'static);
}

impl ItemExt for Rc<RefCell<dyn Item>> {
    fn when(&self, predicate: impl Fn() -> bool + 'static) {
        let menu = self.borrow().menu();
        menu.get_menu_controller().enable_item_when(self, predicate);
    }
}
