//! Navigation/selection parity tests for `oh-menu-system` against the C++
//! `extlibs/SSVMenuSystem` semantics:
//!
//! - `Menu::{next,previous,exec,increase,decrease}` delegate to the active
//!   category's current item (`Menu.hpp`);
//! - `Category::{next,previous}` wrap the index (`Category.hpp wrapIndex`);
//! - item callbacks fire in the same order as the C++ would;
//! - `GoBack::exec` / `Goto::exec` change the active menu exactly as C++
//!   (`menu.goBack()` pops `lastCategories`; `menu.setCategory(target)`
//!   pushes onto it).

use std::cell::RefCell;
use std::rc::Rc;

use oh_menu_system::item::ItemExt;
use oh_menu_system::{Item, Menu};

fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

#[test]
fn first_category_becomes_active() {
    let menu = Menu::new();
    let _main = menu.create_category("main");
    // C++: createCategory sets the category only if it was nullptr; the
    // setCategory call pushed it onto lastCategories (size 1 ⇒ !canGoBack).
    assert_eq!(menu.get_category().get_name(), "main");
    assert!(!menu.can_go_back());

    let sub = menu.create_category("sub");
    // second createCategory does NOT change the active category
    assert_eq!(menu.get_category().get_name(), "main");

    menu.set_category(&sub);
    assert_eq!(menu.get_category().get_name(), "sub");
    assert!(menu.can_go_back());
}

#[test]
fn navigation_wraps_and_tracks_idx() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    main.create_single("a", || {});
    main.create_single("b", || {});
    main.create_single("c", || {});

    assert_eq!(menu.get_idx(), 0);
    menu.next();
    assert_eq!(menu.get_idx(), 1);
    menu.next();
    assert_eq!(menu.get_idx(), 2);
    menu.next();
    assert_eq!(menu.get_idx(), 0); // C++ wrapIndex: index > size-1 ⇒ 0
    menu.previous();
    assert_eq!(menu.get_idx(), 2); // C++ wrapIndex: index < 0 ⇒ size-1
    assert_eq!(menu.get_item().borrow().get_name(), "c");
}

#[test]
fn single_exec_fires_actions_in_order() {
    let log = log();
    let menu = Menu::new();
    let main = menu.create_category("main");
    for name in ["one", "two", "three"] {
        let l = log.clone();
        main.create_single(name, move || l.borrow_mut().push(name.to_string()));
    }

    // C++: exec, next, exec, exec, next, next, exec
    menu.exec();
    menu.next();
    menu.exec();
    menu.exec();
    menu.next();
    menu.next();
    menu.exec();
    assert_eq!(*log.borrow(), vec!["one", "two", "two", "one"]);
}

#[test]
fn menu_navigation_targets_active_category_only() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    main.create_single("a", || {});
    main.create_single("b", || {});
    let sub = menu.create_category("sub");
    sub.create_single("x", || {});
    sub.create_single("y", || {});

    menu.next();
    assert_eq!(menu.get_item().borrow().get_name(), "b");

    menu.set_category(&sub);
    assert_eq!(menu.get_idx(), 0);
    menu.next();
    assert_eq!(menu.get_item().borrow().get_name(), "y");
    menu.previous();
    assert_eq!(menu.get_item().borrow().get_name(), "x");
}

#[test]
fn toggle_flips_and_reports_state() {
    let on = Rc::new(RefCell::new(false));
    let events = log();
    let menu = Menu::new();
    let main = menu.create_category("main");
    let on2 = on.clone();
    let on3 = on.clone();
    let e2 = events.clone();
    let _toggle = main.create_toggle_bool(
        "snd",
        move || *on2.borrow(),
        move |x| {
            *on3.borrow_mut() = x;
            e2.borrow_mut()
                .push(if x { "on" } else { "off" }.to_string());
        },
    );

    assert_eq!(menu.get_item().borrow().get_name(), "snd: off");
    menu.exec(); // predicate false ⇒ activateAction (C++ Toggle::exec)
    assert!(*on.borrow());
    assert_eq!(menu.get_item().borrow().get_name(), "snd: on");
    menu.increase(); // C++ Toggle::increase == exec
    assert!(!*on.borrow());
    menu.decrease();
    assert!(*on.borrow());
    assert_eq!(*events.borrow(), vec!["on", "off", "on"]);
}

#[test]
fn slider_clamps_and_renders() {
    let value = Rc::new(RefCell::new(10));
    let menu = Menu::new();
    let main = menu.create_category("main");
    let v1 = value.clone();
    let v2 = value.clone();
    let _slider = main.create_slider_value(
        "vol",
        move || *v1.borrow(),
        move |x| *v2.borrow_mut() = x,
        0,
        100,
        10,
    );

    assert_eq!(menu.get_item().borrow().get_name(), "vol: 10");
    menu.increase();
    assert_eq!(*value.borrow(), 20);
    for _ in 0..20 {
        menu.increase();
    }
    assert_eq!(*value.borrow(), 100); // ssvu::getClamped at max
    menu.decrease();
    assert_eq!(*value.borrow(), 90);
    for _ in 0..20 {
        menu.decrease();
    }
    assert_eq!(*value.borrow(), 0); // clamped at min
    menu.decrease();
    assert_eq!(*value.borrow(), 0);
    assert!(menu.get_item().borrow().can_increase());
}

#[test]
fn go_back_pops_to_previous_category() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    main.create_single("stay", || {});
    main.create_single("no-op2", || {});
    let sub = menu.create_category("sub");
    sub.create_go_back("back");

    menu.set_category(&sub);
    menu.exec(); // GoBack::exec → menu.goBack()
    assert_eq!(menu.get_category().get_name(), "main");
    assert!(!menu.can_go_back()); // back to [main] on lastCategories

    // a disabled GoBack does NOT pop (C++ Menu::exec checks isEnabled)
    menu.set_category(&sub);
    let back = sub.get_items()[0].clone();
    back.borrow_mut().set_enabled(false);
    menu.exec();
    assert_eq!(menu.get_category().get_name(), "sub");
}

#[test]
fn goto_activates_target_and_pushes_history() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    menu.create_category("sub"); // keep "opts" away from index 1
    let opts = menu.create_category("opts");
    main.create_goto("toOpts", &opts);

    menu.exec(); // Goto::exec → menu.set_category(opts)
    assert_eq!(menu.get_category().get_name(), "opts");
    // lastCategories is now [main, opts]
    assert!(menu.can_go_back());
    menu.go_back();
    assert_eq!(menu.get_category().get_name(), "main");
}

#[test]
fn disabled_items_skip_and_controller_enables() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    let log = log();
    let l = log.clone();
    let item = main.create_single("act", move || l.borrow_mut().push("act".to_string()));

    item.borrow_mut().set_enabled(false);
    menu.exec(); // C++: if(getItem().isEnabled()) — skipped
    assert!(log.borrow().is_empty());

    // C++ `item | predicate` → Controller::enableItemWhen; Menu::update applies it
    let flag = Rc::new(RefCell::new(true));
    let f = flag.clone();
    let dyn_item: Rc<RefCell<dyn Item>> = item.clone();
    dyn_item.when(move || *f.borrow());
    menu.update();
    assert!(item.borrow().is_enabled());
    menu.exec();
    assert_eq!(*log.borrow(), vec!["act"]);

    *flag.borrow_mut() = false;
    menu.update();
    assert!(!item.borrow().is_enabled());
    menu.exec();
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn remove_and_sort_by_name() {
    let menu = Menu::new();
    let main = menu.create_category("main");
    main.create_single("zebra", || {});
    main.create_single("apple", || {});
    main.create_single("mango", || {});

    main.sort_by_name();
    let names: Vec<String> = main
        .get_items()
        .iter()
        .map(|i| i.borrow().get_name())
        .collect();
    assert_eq!(names, vec!["apple", "mango", "zebra"]);

    main.next(); // index 1 (mango)
    main.remove(); // C++: erase(items.begin() + index); index = min(index, size-1)
    assert_eq!(main.get_idx(), 1);
    assert_eq!(main.get_item().borrow().get_name(), "zebra");

    main.remove(); // index 1 of [apple, zebra]
    assert_eq!(main.get_idx(), 0);
    assert_eq!(main.get_item().borrow().get_name(), "apple");
}

#[test]
fn category_lookup_and_offset() {
    let menu = Menu::new();
    let _a = menu.create_category("alpha");
    menu.create_category("beta");
    assert_eq!(menu.get_categories().len(), 2);
    assert_eq!(menu.get_category_by_name("beta").get_name(), "beta");
    // unknown name ⇒ active category (C++ fallback: return *category)
    assert_eq!(menu.get_category_by_name("nope").get_name(), "alpha");

    let main = menu.get_category();
    *main.get_offset_mut() = 5.0;
    assert_eq!(*main.get_offset_mut(), 5.0);
}
