//! C++ `ssvms::Menu` (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Menu/
//! Menu.hpp`).

use std::cell::{RefCell, RefMut};
use std::rc::{Rc, Weak};

use crate::category::Category;
use crate::controller::Controller;
use crate::item::Item;

/// `ssvms::Menu` — owns the categories, tracks the active one (plus the
/// `std::stack` of previous categories for `goBack`), and holds the
/// [`Controller`].
///
/// C++:
///
/// ```cpp
/// std::vector<UniquePtr<Category>> categories;
/// Category* category{nullptr};
/// std::stack<Category*> lastCategories;
/// Controller controller;
/// ```
///
/// Rust: a `Menu` is always shared as `Rc<Menu>` (`Menu::new` returns it)
/// and interior mutation goes through `RefCell` fields, so every method
/// takes `&self`. That is what lets `GoBack::exec → menu.go_back()` and
/// `Goto::exec → menu.set_category()` re-borrow the menu mid-execution
/// (C++ does this through raw pointers).
pub struct Menu {
    categories: RefCell<Vec<Rc<Category>>>,
    category: RefCell<Option<Rc<Category>>>,
    last_categories: RefCell<Vec<Rc<Category>>>,
    controller: RefCell<Controller>,
    this: Weak<Menu>,
}

impl Menu {
    /// `Menu::new` — C++ default-constructs `Menu` with `category ==
    /// nullptr`; the first `create_category` becomes the active category.
    pub fn new() -> Rc<Self> {
        Rc::new_cyclic(|this| Menu {
            categories: RefCell::new(Vec::new()),
            category: RefCell::new(None),
            last_categories: RefCell::new(Vec::new()),
            controller: RefCell::new(Controller::new()),
            this: this.clone(),
        })
    }

    /// `Menu::createCategory(const std::string&)`
    pub fn create_category(&self, name: &str) -> Rc<Category> {
        let category = Category::new(self.this.clone(), name);
        self.categories.borrow_mut().push(category.clone());
        if self.category.borrow().is_none() {
            self.set_category(&category);
        }
        category
    }

    /// `Menu::setCategory(Category&)` — pushes onto `lastCategories`, then
    /// activates the category.
    pub fn set_category(&self, category: &Rc<Category>) {
        self.last_categories.borrow_mut().push(category.clone());
        *self.category.borrow_mut() = Some(category.clone());
    }

    /// `Menu::clear` — drops all categories (the active-category handle and
    /// the back-stack keep their categories alive; C++ left the raw
    /// pointers dangling).
    pub fn clear(&self) {
        self.categories.borrow_mut().clear();
    }

    /// `Menu::update` → `controller.update()`
    pub fn update(&self) {
        self.controller.borrow_mut().update();
    }

    /// `Menu::canGoBack` — `lastCategories.size() > 1`
    pub fn can_go_back(&self) -> bool {
        self.last_categories.borrow().len() > 1
    }

    /// `Menu::getCategory` — panics like the C++ null-deref would, when no
    /// category has been created yet.
    pub fn get_category(&self) -> Rc<Category> {
        self.category
            .borrow()
            .clone()
            .expect("Menu has no active category (create_category first)")
    }

    /// `Menu::getCategories`
    pub fn get_categories(&self) -> Vec<Rc<Category>> {
        self.categories.borrow().clone()
    }

    /// `Menu::getCategoryByName` — first category with the given name,
    /// else the active category (C++ falls back to `*category`).
    pub fn get_category_by_name(&self, name: &str) -> Rc<Category> {
        let categories = self.categories.borrow();
        categories
            .iter()
            .find(|c| c.get_name() == name)
            .cloned()
            .unwrap_or_else(|| self.get_category())
    }

    /// `Menu::getItem` — `category->getItem()`
    pub fn get_item(&self) -> Rc<RefCell<dyn Item>> {
        self.get_category().get_item()
    }

    /// `Menu::getItems`
    pub fn get_items(&self) -> Vec<Rc<RefCell<dyn Item>>> {
        self.get_category().get_items()
    }

    /// `Menu::getIdx` — `category->getIdx()`
    pub fn get_idx(&self) -> i32 {
        self.get_category().get_idx()
    }

    /// `Menu::getMenuController`
    pub fn get_menu_controller(&self) -> RefMut<'_, Controller> {
        self.controller.borrow_mut()
    }

    /// `Menu::goBack` — `lastCategories.pop(); category =
    /// lastCategories.top();` (C++ is UB unless `can_go_back()`).
    pub fn go_back(&self) {
        let previous = {
            let mut stack = self.last_categories.borrow_mut();
            stack.pop();
            stack
                .last()
                .cloned()
                .expect("goBack called with lastCategories.size() <= 1 (C++: UB)")
        };
        *self.category.borrow_mut() = Some(previous);
    }

    /// `Menu::next` — `category->next()`
    pub fn next(&self) {
        self.get_category().next();
    }

    /// `Menu::previous` — `category->previous()`
    pub fn previous(&self) {
        self.get_category().previous();
    }

    /// `Menu::exec` — `if(getItem().isEnabled()) getItem().exec();`
    pub fn exec(&self) {
        let item = self.get_item();
        if item.borrow().is_enabled() {
            item.borrow_mut().exec();
        }
    }

    /// `Menu::increase` — `if(getItem().isEnabled()) getItem().increase();`
    pub fn increase(&self) {
        let item = self.get_item();
        if item.borrow().is_enabled() {
            item.borrow_mut().increase();
        }
    }

    /// `Menu::decrease` — `if(getItem().isEnabled()) getItem().decrease();`
    pub fn decrease(&self) {
        let item = self.get_item();
        if item.borrow().is_enabled() {
            item.borrow_mut().decrease();
        }
    }
}
