//! Port of `extlibs/SSVMenuSystem` — the SSVMenuSystem menu **data model and
//! navigation** (plan 001, issue 012; ground rules D-000/D-002).
//!
//! Mirrors the C++ headers 1:1:
//!
//! - [`Menu`] ← `Menu/Menu.hpp` (category ownership, active category +
//!   back-stack, `next`/`previous`/`exec`/`increase`/`decrease`/`goBack`)
//! - [`Category`] ← `Menu/Category.hpp` (named item list, selection index,
//!   wrap-around `next`/`previous`, `remove`, `sortByName`)
//! - [`ItemBase`] / [`Item`] ← `Menu/ItemBase.hpp` (name, enabled,
//!   increasable, offset + virtual `exec`/`increase`/`decrease`/`getName`)
//! - [`items`] ← `Items/{Single,Slider,Toggle,GoBack,Goto}.hpp`
//! - [`Controller`] ← `Menu/Controller.hpp` (enable-when predicates only —
//!   the C++ controller has **no** key handling; key→action mapping lives in
//!   the game's MenuGame, which is issue 013)
//!
//! Ownership: C++ `UniquePtr` becomes `Rc`/`Rc<RefCell<..>>`. `Menu` is
//! always shared as `Rc<Menu>` (`Menu::new` returns it); items are
//! `Rc<RefCell<dyn Item>>` inside their `Category`; item back-refs to
//! menu/category are `Weak` (the C++ raw `Menu&`/`Category&`). Because all
//! `Menu`/`Category` methods take `&self` (interior mutability),
//! `GoBack::exec → menu.go_back()` and `Goto::exec → menu.set_category()`
//! can re-borrow the menu mid-execution without conflicts.
//!
//! Menu **rendering** is out of scope (issue 013, MenuGame port).
//!
//! Skill: `.agents/skills/lib-ssvmenusystem.md`.

pub mod category;
pub mod controller;
pub mod item;
pub mod items;
pub mod menu;

pub use category::Category;
pub use controller::Controller;
pub use item::{Action, Item, ItemBase, ItemExt, Predicate};
pub use menu::Menu;
