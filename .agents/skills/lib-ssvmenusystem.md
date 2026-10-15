# lib-ssvmenusystem — SSVMenuSystem

Source of truth: `extlibs/SSVMenuSystem` (vendored C++14, header-only;
AFL-3.0). Submodule pin: `f2a75d1e34f5de352d5e38cdef4002b60a59c193`
("REmove <memory> dependency"). Written against that commit. All files live
under `include/SSVMenuSystem/`: `SSVMenuSystem.hpp` (umbrella),
`Global/Typedefs.hpp`, `Menu/{Menu,Category,ItemBase,Controller}.hpp`,
`Items/{Single,Slider,Toggle,GoBack,Goto}.hpp`.

Rust port: `crates/oh-menu-system` (one module per C++ header:
`src/{menu,category,item,items,controller}.rs`; parity tests in
`crates/oh-menu-system/tests/navigation.rs`). Menu **rendering** is NOT in
this crate — the C++ library has no rendering at all; drawing the menu
font/indentation is the game's MenuGame (issue 013).

## Global/Typedefs.hpp

```cpp
using Action = std::function<void()>;                 // → Rust `pub type Action = Box<dyn Fn()>`
using Predicate = std::function<bool()>;              // → Rust `pub type Predicate = Box<dyn Fn() -> bool>`
template <typename T> class UniquePtr { ... };        // non-std owning ptr (no <memory>): move-only, deletes on drop
template <typename T, typename... Ts> UniquePtr<T> makeUnique(Ts&&...);
template <typename Base, typename T, typename... Ts> UniquePtr<Base> makeUniqueBase(Ts&&...);  // upcast to Base
```

Rust ownership mapping (plan step 4, "UniquePtr → Box/Rc"): `Menu` is shared
as `Rc<Menu>` (`Menu::new() -> Rc<Menu>`); categories are `Rc<Category>`
inside `Menu.categories: RefCell<Vec<Rc<Category>>>`; items are
`Rc<RefCell<dyn Item>>` inside `Category.items: RefCell<Vec<...>>`; the C++
raw back-refs `Menu&`/`Category&` inside items become `Weak` handles. All
`Menu`/`Category` methods take `&self` (interior mutability), so
`GoBack::exec → menu.go_back()` re-borrows the menu without conflicts.

## Menu/ItemBase.hpp — `ItemBase` (Rust: `item::ItemBase` + `item::Item` trait)

```cpp
Menu& menu; Category& category; std::string name;
bool enabled{true}; bool increasable{false}; float offset{0.f};
virtual ~ItemBase();
virtual void exec() {} virtual void increase() {} virtual void decrease() {};
void setEnabled(bool); Menu& getMenu() const;
virtual std::string getName() const { return name; }   // NOTE: virtual — Slider/Toggle override
bool isEnabled() const; bool canIncrease() const; float& getOffset();
```

Rust: `ItemBase` holds the data (`menu: Weak<Menu>`, `category:
Weak<Category>`, `name: String`, `enabled`, `increasable`, `offset: f32`);
the virtuals are the `Item` trait (`exec/increase/decrease` default no-op,
`get_name()` default = bare name) with convenience accessors
(`name/is_enabled/set_enabled/can_increase/menu/category`). Items are created
through `Category::create_*` (mirrors C++ `Category::create<T>`), which
returns the typed `Rc<RefCell<T>>` handle (C++ returns `T&`).

## Menu/Category.hpp — `Category`

```cpp
Menu& menu; std::string name; std::vector<UniquePtr<ItemBase>> items; int index{0}; float offset{0.f};
void wrapIndex() { if(index > ssvu::toInt(items.size() - 1)) index = 0;
                   else if(index < 0) index = items.size() - 1; }        // EMPTY LIST: C++ UB
template <typename T, typename... TArgs> T& create(const std::string&, TArgs&&...);
void remove() { items.erase(items.begin() + index);
                index = std::min(index, int(items.size()) - 1); }       // clamp uses POST-erase size
void sortByName();                                                      // std::sort by virtual getName()
void next() { ++index; wrapIndex(); }  void previous() { --index; wrapIndex(); }
const auto& getName() const; auto& getItem() const { return *(items[index]); }
auto& getItems(); int getIdx() const; float& getOffset();
```

Rust `category.rs`: same names snake_cased (`create_single/create_slider/
create_slider_value/create_toggle/create_toggle_bool/create_go_back/
create_goto`, `remove`, `sort_by_name`, `next/previous`, `get_name/get_item/
get_items/get_idx/get_offset_mut`). `index` is `i32`; `wrap_index` mirrors
`wrapIndex` (empty list clamps to 0; C++ is UB). `sort_by_name` sorts by the
**display** name (`Item::get_name`), so Slider/Toggle sort as `"name: value"` /
`"name: on"`.

## Menu/Menu.hpp — `Menu`

```cpp
std::vector<UniquePtr<Category>> categories;
Category* category{nullptr};        // active category (null until first createCategory)
std::stack<Category*> lastCategories;
Controller controller;

auto& createCategory(const std::string&)  // push; if(category == nullptr) setCategory(result)
void setCategory(Category&)               // lastCategories.emplace(&mCategory); category = &mCategory
void clear() noexcept; void update() { controller.update(); }
bool canGoBack() const { return lastCategories.size() > 1; }
Category& getCategory() const;                       // *category (null-deref UB if none)
auto& getCategoryByName(const std::string&) const;   // linear scan, else return *category (FALLBACK)
ItemBase& getItem() const; auto& getItems(); int getIdx() const;
auto& getMenuController();
void goBack() { lastCategories.pop(); category = lastCategories.top(); }  // pop removes CURRENT, top = previous
void next() { category->next(); }   void previous() { category->previous(); }
void exec()     { if(getItem().isEnabled()) getItem().exec(); }
void increase() { if(getItem().isEnabled()) getItem().increase(); }
void decrease() { if(getItem().isEnabled()) getItem().decrease(); }

ItemBase& operator|(ItemBase& mLhs, Predicate mRhs)  // → getMenu().getMenuController().enableItemWhen(mLhs, mRhs)
```

Key behaviors (pinned by tests):
- `createCategory` on an empty menu: the new category becomes active AND is
  pushed onto `lastCategories` (via `setCategory`) — so after the first
  `createCategory`, `canGoBack()` is still false (stack size 1).
- `goBack` pops the *current* entry, activates the *new* top; only legal
  when `canGoBack()` (C++ UB otherwise; Rust panics).
- `setCategory` always pushes the *target* onto the stack (that's how
  `Goto` gains a back-entry).
- `exec/increase/decrease` are no-ops on a disabled item.
- The game drives this via `MenuGame` (e.g. `src/SSVOpenHexagon/Core/
  MenuGame.cpp`: `scrollToEnabledMenuItem` loops `menu->next()` while the
  current item is disabled; keys call `getCurrentMenu()->{next,previous,
  exec,increase,decrease,goBack,update}`).

## Menu/Controller.hpp — `Controller` (Rust `controller::Controller`)

```cpp
struct Pair { ItemBase* fst; Predicate snd; };
std::vector<Pair> enableWhenPairs;
void enableItemWhen(ItemBase& mItem, Predicate mPred);
void update() { for(const auto& p : enableWhenPairs) p.fst->setEnabled(p.snd()); }
```

That is the **whole** C++ controller — it has NO key handling (no keymaps,
no `execOnKey`). Key→action mapping belongs to MenuGame (issue 013). Rust:
`enable_item_when(item, pred)` stores (category `Weak`, item identity by
`Rc::as_ptr`, predicate); `update()` re-applies every predicate (pairs whose
item was removed are skipped — C++ would call through the stale pointer).
The C++ `operator|` is `oh_menu_system::ItemExt::when` (extension trait,
because an inherent `impl Rc<RefCell<dyn Item>>` is rejected by the orphan
rule): `dyn_item.when(pred)` then `menu.update()`.

## Items — exact semantics

- **Single** (`Items/Single.hpp`): `exec()` = `action()`. Nothing else
  (default `increase/decrease` no-op, `increasable` false).
- **Slider** (`Items/Slider.hpp`): ctor A `(ValueGetter, Action increase,
  Action decrease)`; ctor B templated `(mFuncGet, mFuncSet, mMin, mMax,
  mIncrement)` with `[=]` lambdas:
  `valueGetter = toStr(mFuncGet())`,
  `increase = mFuncSet(getClamped(mFuncGet() + mIncrement, mMin, mMax))`,
  `decrease = mFuncSet(getClamped(mFuncGet() - mIncrement, mMin, mMax))`
  (`ssvu::getClamped` — ternary clamp, ported as `oh_math::ssvu::math::
  get_clamped`). `increasable = true`; `exec` is the no-op default (a Slider
  is adjusted via `increase`/`decrease`, never `exec`); `getName` =
  `name + ": " + valueGetter()`. Rust: `create_slider` /
  `create_slider_value` (functors wrapped in `Rc` shared by the derived
  closures — equivalent to the C++ by-value captures).
- **Toggle** (`Items/Toggle.hpp`): ctors `(Predicate, Action activate,
  Action deactivate)` / templated `(mFuncGet, mFuncSet)` / `(bool&)`.
  `increasable = true`; **`exec()` = `predicate() ? deactivateAction() :
  activateAction()`** (exec when ON deactivates); `increase()` and
  `decrease()` both call `exec()`; `getName` = `name + ": on"` /
  `": off"`. Rust: `create_toggle` / `create_toggle_bool`.
- **GoBack** (`Items/GoBack.hpp`): `exec()` = `menu.goBack()` — pops the
  active category's stack entry (see Menu semantics above).
- **Goto** (`Items/Goto.hpp`): holds `Category& target`; `exec()` =
  `menu.setCategory(target)` — activates the target and pushes it onto
  `lastCategories` (so `goBack` from there returns to the pre-goto
  category). Rust holds `target` as a strong `Rc<Category>` (the C++ raw
  ref would dangle if the target were dropped).

## C++ gotchas mirrored/preserved

- `Category::getItem()` on an empty category, `Menu::getCategory()` before
  any category, and `goBack` with stack size ≤ 1 are C++ UB; the port
  panics with a message instead.
- C++ `Menu::createCategory` pushes into a `std::vector` of raw-pointer
  `UniquePtr`s: reallocation invalidates every previously returned
  `Category&` (the game never holds them past the call). The `Rc` port has
  no such hazard.
- The `int index` / `size_t` mixes in `wrapIndex`/`remove` are the C++'s;
  the port uses `i32` and guards the degenerate empty-list case (clamp to 0).

## Port notes (Rust)

- `Menu::new() -> Rc<Menu>` via `Rc::new_cyclic`; `Category` likewise
  (`this: Weak<Category>` self-handle for item back-refs).
- `Item` is object-safe (`base()`/`base_mut()` + virtuals); `dyn Item` is
  stored in `RefCell` so `Menu::get_item` / `Category::get_item` return
  cloneable `Rc<RefCell<dyn Item>>` handles.
- The only external dep is `oh-math` (`ssvu::math::get_clamped`).
