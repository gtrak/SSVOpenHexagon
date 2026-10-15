//! C++ `ssvms::Impl::Controller`
//! (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Menu/Controller.hpp`).
//!
//! The C++ controller only tracks `(ItemBase*, Predicate)` pairs and applies
//! them on `update()`; it does **not** do any key handling (the C++ game maps
//! keys to `Menu::{next,previous,exec,...}` itself in MenuGame — issue 013).

use std::cell::RefCell;
use std::ptr;
use std::rc::{Rc, Weak};

use crate::category::Category;
use crate::item::{Item, Predicate};

/// C++ `struct Pair { ItemBase* fst; Predicate snd; };`
///
/// `ItemBase*` is mirrored as (owning `Category` weak handle, item identity
/// by `Rc<RefCell<dyn Item>>` allocation address — stable for the item's
/// lifetime).
struct Pair {
    category: Weak<Category>,
    item: *const RefCell<dyn Item>,
    predicate: Predicate,
}

/// `ssvms::Impl::Controller`
#[derive(Default)]
pub struct Controller {
    pairs: Vec<Pair>,
}

impl Controller {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// `Controller::enableItemWhen(ItemBase&, Predicate)`
    pub fn enable_item_when(
        &mut self,
        item: &Rc<RefCell<dyn Item>>,
        predicate: impl Fn() -> bool + 'static,
    ) {
        let category = item.borrow().category();
        self.pairs.push(Pair {
            category: Rc::downgrade(&category),
            item: Rc::as_ptr(item),
            predicate: Box::new(predicate),
        });
    }

    /// `Controller::update` — `for(const auto& p : enableWhenPairs)
    /// p.fst->setEnabled(p.snd());` (a pair whose item was removed or whose
    /// category was dropped is skipped; C++ would call through the stale
    /// pointer).
    pub fn update(&mut self) {
        for pair in self.pairs.iter() {
            let Some(category) = pair.category.upgrade() else {
                continue;
            };
            let items = category.items();
            if let Some(i) = items
                .iter()
                .position(|x| ptr::addr_eq(Rc::as_ptr(x), pair.item))
            {
                items[i].borrow_mut().set_enabled((pair.predicate)());
            }
        }
    }
}
