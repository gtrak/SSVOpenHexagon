# Issue 12 — `oh-menu-system`: SSVMenuSystem analogue

## Objective

Port `extlibs/SSVMenuSystem` (`Menu`, `Category`, `ItemBase`,
`Items/{Single,Slider,Toggle,GoBack,Goto}`, `Controller`) as a small in-repo
crate/module: the menu data model and the controller input handling, with the same
navigation/selection semantics.

## Key decisions

- Mirror `extlibs/SSVMenuSystem/include/SSVMenuSystem/**` 1:1.
- Item model: each item has a name, a "set" callback, a "get" display string, and
  type-specific behavior (toggle flips; slider adjusts with left/right; goto jumps;
  go-back pops). Copy the exact key handling.
- The rendering of menus is issue 13's concern; this crate exposes the model and
  navigation, plus what `MenuGame` needs (`getCurrentMenu`, selection index, etc.).

## Files

| File | Change |
|---|---|
| `crates/oh-menu-system/Cargo.toml` | new — dep `oh-math` |
| `crates/oh-menu-system/src/{menu,category,item,items,controller}.rs` | new |
| `crates/oh-menu-system/tests/navigation.rs` | new |
| `.agents/skills/lib-ssvmenusystem.md` | new — from real source |

## Steps

1. Port `ItemBase` + the item variants with exact `exec`/`adjust`/`getStr` semantics.
2. Port `Menu` (categories, item ownership, navigation up/down/left/right/ok/erase/exit).
3. Port `Controller`. **CORRECTION (2026-10-15):** the contract originally said
   "key bindings → item actions" — that is WRONG. The real `Controller`
   (`extlibs/SSVMenuSystem/include/SSVMenuSystem/Menu/Controller.hpp`) has NO key
   handling; it only holds `enableWhenPairs` and exposes `enableItemWhen(ItemBase&,
   Predicate)` + `update()` (which calls `setEnabled(p.snd())`). Key→action mapping
   lives in `MenuGame` (issue 013). The Rust port mirrors the real Controller.
4. Port the `UniquePtr`-based ownership to `Box`/`Rc` as appropriate.

## Verification

- Navigation tests: build a menu, drive the controller, assert selection moves and
  item callbacks fire in the same order as the C++.
- A test that `GoBack`/`Goto` change the active menu exactly as the C++.
- `tools/gate.sh` green.

## Notes / risks

- `MenuGame` (issue 13) is 6k lines and uses this heavily; keep the surface minimal
  but complete enough that issue 13 does not need to patch it.
