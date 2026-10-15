//! C++ `ssvms::Category`
//! (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Menu/Category.hpp`).

use std::cell::{Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};

use crate::item::Item;
use crate::items::{GoBack, Goto, Single, Slider, Toggle};
use crate::menu::Menu;

/// `ssvms::Category` — a named list of items with the currently selected
/// index.
///
/// C++: `Menu& menu; std::string name; std::vector<UniquePtr<ItemBase>>
/// items; int index{0}; float offset{0.f};` — the item vector becomes
/// `RefCell<Vec<Rc<RefCell<dyn Item>>>>` (plan step 4: `UniquePtr` → `Rc`).
pub struct Category {
    menu: Weak<Menu>,
    this: Weak<Category>,
    name: String,
    items: RefCell<Vec<Rc<RefCell<dyn Item>>>>,
    index: RefCell<i32>,
    offset: RefCell<f32>,
}

impl Category {
    /// C++ `Category(Menu& mMenu, const std::string& mName)` — only ever
    /// constructed through [`Menu::create_category`], which wires the menu
    /// weak handle.
    pub(crate) fn new(menu: Weak<Menu>, name: &str) -> Rc<Self> {
        Rc::new_cyclic(|this| Category {
            menu,
            this: this.clone(),
            name: name.to_string(),
            items: RefCell::new(Vec::new()),
            index: RefCell::new(0),
            offset: RefCell::new(0.0),
        })
    }

    /// C++ private `wrapIndex()`:
    ///
    /// ```cpp
    /// if(index > ssvu::toInt(items.size() - 1)) index = 0;
    /// else if(index < 0) index = items.size() - 1;
    /// ```
    ///
    /// (C++ with an empty item list is UB here; the port clamps to 0.)
    fn wrap_index(&self) {
        let size = self.items.borrow().len() as i32;
        let mut index = self.index.borrow_mut();
        if *index > size - 1 {
            *index = 0;
        } else if *index < 0 {
            *index = size.saturating_sub(1);
        }
    }

    /// C++ `Category::create<T>` returning `T&` — one method per concrete
    /// item type, each returning a typed `Rc<RefCell<T>>` (the same handle
    /// that was pushed into the item list).
    pub fn create_single(&self, name: &str, action: impl Fn() + 'static) -> Rc<RefCell<Single>> {
        let item = Rc::new(RefCell::new(Single::new(
            self.menu.clone(),
            self.this.clone(),
            name,
            Box::new(action),
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ `Slider(Menu&, Category&, const std::string&, ValueGetter,
    /// Action, Action)`
    pub fn create_slider(
        &self,
        name: &str,
        value_getter: impl Fn() -> String + 'static,
        increase_action: impl Fn() + 'static,
        decrease_action: impl Fn() + 'static,
    ) -> Rc<RefCell<Slider>> {
        let item = Rc::new(RefCell::new(Slider::new(
            self.menu.clone(),
            self.this.clone(),
            name,
            Box::new(value_getter),
            Box::new(increase_action),
            Box::new(decrease_action),
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ templated `Slider(..., TFuncGet, TFuncSet, T mMin, T mMax,
    /// T mIncrement)`
    pub fn create_slider_value<T, TGet, TSet>(
        &self,
        name: &str,
        m_func_get: TGet,
        m_func_set: TSet,
        m_min: T,
        m_max: T,
        m_increment: T,
    ) -> Rc<RefCell<Slider>>
    where
        T: Copy
            + PartialOrd
            + std::ops::Add<Output = T>
            + std::ops::Sub<Output = T>
            + std::fmt::Display
            + 'static,
        TGet: Fn() -> T + Clone + 'static,
        TSet: Fn(T) + Clone + 'static,
    {
        let item = Rc::new(RefCell::new(Slider::from_value(
            self.menu.clone(),
            self.this.clone(),
            name,
            m_func_get,
            m_func_set,
            m_min,
            m_max,
            m_increment,
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ `Toggle(Menu&, Category&, const std::string&, Predicate, Action,
    /// Action)`
    pub fn create_toggle(
        &self,
        name: &str,
        activated_predicate: impl Fn() -> bool + 'static,
        activate_action: impl Fn() + 'static,
        deactivate_action: impl Fn() + 'static,
    ) -> Rc<RefCell<Toggle>> {
        let item = Rc::new(RefCell::new(Toggle::new(
            self.menu.clone(),
            self.this.clone(),
            name,
            Box::new(activated_predicate),
            Box::new(activate_action),
            Box::new(deactivate_action),
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ templated `Toggle(..., TFuncGet, TFuncSet)`
    pub fn create_toggle_bool<TGet, TSet>(
        &self,
        name: &str,
        m_func_get: TGet,
        m_func_set: TSet,
    ) -> Rc<RefCell<Toggle>>
    where
        TGet: Fn() -> bool + Clone + 'static,
        TSet: Fn(bool) + Clone + 'static,
    {
        let item = Rc::new(RefCell::new(Toggle::from_bool(
            self.menu.clone(),
            self.this.clone(),
            name,
            m_func_get,
            m_func_set,
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ `GoBack(Menu&, Category&, const std::string&)`
    pub fn create_go_back(&self, name: &str) -> Rc<RefCell<GoBack>> {
        let item = Rc::new(RefCell::new(GoBack::new(
            self.menu.clone(),
            self.this.clone(),
            name,
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// C++ `Goto(Menu&, Category&, const std::string&, Category&)`
    pub fn create_goto(&self, name: &str, target: &Rc<Category>) -> Rc<RefCell<Goto>> {
        let item = Rc::new(RefCell::new(Goto::new(
            self.menu.clone(),
            self.this.clone(),
            name,
            target,
        )));
        let upcast: Rc<RefCell<dyn Item>> = item.clone();
        self.items.borrow_mut().push(upcast);
        item
    }

    /// `Category::remove` — `items.erase(items.begin() + index);
    /// index = std::min(index, int(items.size()) - 1);` (C++ is UB if the
    /// index is out of range; the port only erases a valid index and still
    /// clamps as C++ does).
    pub fn remove(&self) {
        let index = *self.index.borrow();
        let size = self.items.borrow().len() as i32;
        if (0..size).contains(&index) {
            self.items.borrow_mut().remove(index as usize);
        }
        *self.index.borrow_mut() = index.min(size - 1);
    }

    /// `Category::sortByName` — `std::sort` by the **virtual** `getName()`
    /// (so Slider/Toggle sort by their displayed `name: value` form).
    pub fn sort_by_name(&self) {
        self.items
            .borrow_mut()
            .sort_by_key(|i| i.borrow().get_name());
    }

    /// `Category::next` — `++index; wrapIndex();`
    pub fn next(&self) {
        *self.index.borrow_mut() += 1;
        self.wrap_index();
    }

    /// `Category::previous` — `--index; wrapIndex();`
    pub fn previous(&self) {
        *self.index.borrow_mut() -= 1;
        self.wrap_index();
    }

    /// `Category::getName`
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// `Category::getItem` — `*(items[index])`
    pub fn get_item(&self) -> Rc<RefCell<dyn Item>> {
        let index = *self.index.borrow() as usize;
        self.items.borrow()[index].clone()
    }

    /// `Category::getItems`
    pub fn get_items(&self) -> Vec<Rc<RefCell<dyn Item>>> {
        self.items.borrow().clone()
    }

    /// `Category::getIdx`
    pub fn get_idx(&self) -> i32 {
        *self.index.borrow()
    }

    /// `Category::getOffset` (`float&`)
    pub fn get_offset_mut(&self) -> RefMut<'_, f32> {
        self.offset.borrow_mut()
    }

    /// crate-local access for [`crate::controller::Controller::update`]
    pub(crate) fn items(&self) -> Ref<'_, Vec<Rc<RefCell<dyn Item>>>> {
        self.items.borrow()
    }
}
