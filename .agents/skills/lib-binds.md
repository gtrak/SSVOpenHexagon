# lib-binds — SSVStart Trigger/Combo model + Open Hexagon bind layer

Skills here are authoritative for the port. Written from real source, never
memory. Sources (all vendored in this repo unless noted):

- `extlibs/SSVStart/include/SSVStart/Input/{Trigger,Combo,Enums}.hpp`
- `extlibs/SSVStart/include/SSVStart/Utils/Input.hpp` (name maps, validity,
  hardcoded keys)
- `extlibs/SSVStart/include/SSVStart/Global/Typedefs.hpp` (KKey/MBtn aliases,
  bitsets)
- `extlibs/SSVStart/include/SSVStart/Input/Input.hpp` (`InputState`,
  `Manager`, `isDown`)
- `include/SSVOpenHexagon/Global/Config.hpp` + `src/SSVOpenHexagon/Global/Config.cpp`
  (bind tables, `Tid`, `resizeTrigger`, `getKeyboardBindNames`,
  `rebindTrigger`, `clearTriggerBind`, `getJoystickBindName`,
  `loadAllJoystickBinds`)
- `include/SSVOpenHexagon/Core/{Joystick,BindControl}.hpp` + `src/.../Core/{Joystick,BindControl}.cpp`
- SFML fork `vittorioromeo/SFML@dacceddb` `include/SFML/Window/{Keyboard,Mouse}.hpp`
  (enum integers, mirrored in `.agents/plans/001-rust-port/mapping/oracle-vectors.md`)

Rust port: `crates/oh-config/src/{triggers,joystick,binds,config}.rs`
(issue 004).

## 1. Key / button enums (SFML fork)

`ssvs::KKey = sf::Keyboard::Key`, `ssvs::MBtn = sf::Mouse::Button`
(`SSVStart/Global/Typedefs.hpp:23-24`).

`Keyboard::Key`: `Unknown = -1`, `A = 0 .. Z = 25`, `Num0 = 26 .. Num9 = 35`,
`Escape = 36`, `LControl = 37`, `LShift = 38`, `LAlt = 39`, `LSystem = 40`,
`RControl = 41`, `RShift = 42`, `RAlt = 43`, `RSystem = 44`, `Menu = 45`,
`LBracket = 46`, `RBracket = 47`, `Semicolon = 48`, `Comma = 49`,
`Period = 50`, `Apostrophe = 51`, `Slash = 52`, `Backslash = 53`,
`Grave = 54`, `Equal = 55`, `Hyphen = 56`, `Space = 57`, `Enter = 58`,
`Backspace = 59`, `Tab = 60`, `PageUp = 61`, `PageDown = 62`, `End = 63`,
`Home = 64`, `Insert = 65`, `Delete = 66`, `Add = 67`, `Subtract = 68`,
`Multiply = 69`, `Divide = 70`, `Left = 71`, `Right = 72`, `Up = 73`,
`Down = 74`, `Numpad0 = 75 .. Numpad9 = 84`, `F1 = 85 .. F15 = 99`,
`Pause = 100`, `KeyCount = 101`.

`Mouse::Button`: `Left = 0`, `Right = 1`, `Middle = 2`, `XButton1 = 3`,
`XButton2 = 4`, `ButtonCount = 5`.

These integers are the **in-memory** bind model (bitset indices, joystick
vendor tables, `rebindTrigger(key, btn, index)` int args). `config.json`
stores bind **names** (below); the enum ints are the conversion surface.

## 2. Bitsets — 1-based index, bit 0 = unbound marker

`Typedefs.hpp:28-39`:

```cpp
inline constexpr std::size_t kKeyCount{KKey::KeyCount};       // 101
inline constexpr std::size_t mBtnCount{MBtn::ButtonCount};   // 5
using KeyBitset = std::bitset<kKeyCount + inputBitOffset>;   // 102 bits
using BtnBitset = std::bitset<mBtnCount + inputBitOffset>;   // 6 bits
// inputBitOffset = 1 (Typedefs.hpp)
inline KeyBitset::reference getKeyBit(KeyBitset& bs, KKey k)  { return bs[toInt(k + 1)]; }
inline BtnBitset::reference getBtnBit(BtnBitset& bs, MBtn b) { return bs[toInt(b + 1)]; }
```

So `Key::Unknown (-1)` lands on **bit 0** — that bit IS the "unbound" flag.
A default-constructed `Combo` has ALL bits false, which is *not* marked
unbound; the JSON loader and `clearBind()` set bit 0.

## 3. `Combo` (`Input/Combo.hpp`)

State: `KeyBitset keys; BtnBitset btns;`

- `Combo()` — all-zero bits (NOT unbound-marked).
- `Combo(il<KKey> mKeys, il<MBtn> mBtns = {})` — `addKey`/`addBtn` each.
- `Combo(il<MBtn> mBtns)` — `Combo{{}, mBtns}` (keys empty).
- `addKey(k)`: `keyBit(k) = true; if (k != Unknown) keyBit(Unknown) = false;`
- `addBtn(b)`: `btnBit(b) = true; keyBit(Unknown) = false;`
- `clearBind()`: clear all 101 key bits + 5 button bits, then
  `keyBit(Unknown) = true` (mark unbound).
- `isUnbound()` = `keyBit(Unknown)` (bit 0).
- `getKeys()` / `getBtns()` — const refs to the raw bitsets.
- `isDown(Manager&, InputState&, Mode)` (Input.cpp): per set key/button,
  OR the `Manager`'s pressed state across all input sources
  (`Overlap`), or stop at the first that is down (`Exclusive`).

Semantics used by Open Hexagon: multiple entries in a trigger = ANY-of
(alternative inputs for the same action); a single combo with multiple keys
= OR as well (SSVStart has no AND-combo concept in the game's use).

## 4. `Trigger` (`Input/Trigger.hpp`)

`std::vector<Combo> combos;` + `==`/`!=` on the vector + `getCombos()`.
Default = empty vector (the game pads via `Config::fixupMissingTriggers`
to 4 unbound slots, and `resizeTrigger` normalizes to exactly 4).

`Input/Enums.hpp`: `enum Type { Always, Once }`, `enum Mode { Overlap,
Exclusive }` — used by the SSVStart `Manager`/`Bind` refresh (issue 10),
not by the bind table itself.

## 5. Name vocabulary (`Utils/Input.hpp`)

Prefixes: `SSVS_KEY_PREFIX "k"`, `SSVS_BTN_PREFIX "b"`.

- Key names: `"k" + <SFML Key enum name>` for all 101 names (A..Pause,
  `Num0..Num9` vs `Numpad0..Numpad9` — the top row is `Num*`, the numpad is
  `Numpad*`).
- Button names: `bLeft, bRight, bMiddle, bXButton1, bXButton2`.
- `isKKeyNameValid` / `isMBtnNameValid` — exact string membership.
- `getKKey(name)` / `getMBtn(name)` — lookup, `assert(valid)`.
- `getKKeyName(key)` / `getMBtnName(btn)` — name for the enum value
  (`getKKeyName` asserts `key != Unknown`).
- `getStrKKeyHardcodedMap` — keys with hardcoded behavior (used by
  `KeyboardBindControl`'s `hardcodedKey`): `J K L R Y Escape LAlt Enter
  Backspace Up Down F1 F2 F3 F4` (i.e. `kJ kK kL kR kY kEscape kLAlt
  kEnter kBackspace kUp kDown kF1 kF2 kF3 kF4`).

## 6. JSON serialization (SSVOpenHexagon `Global/UtilsJson.cpp`)

- `Converter<sf::Keyboard::Key>`: string name; `Key::Unknown` archives
  `""` (empty string).
- `Converter<sf::Mouse::Button>`: string name.
- `Converter<Combo>`: an array of strings, one per bound input.
  - fromObj: start cleared (unbound); per element: `""` → `addKey(Unknown)`;
    valid key name → `addKey`; valid button name → `addBtn`; invalid → log
    `"<elem> is not a valid input name, an empty bind has been put in its
    place"` and `addKey(Unknown)`.
  - toObj: unbound → `[""]`; else all set keys (ascending enum order, as
    names) then all set buttons.
- `Converter<Trigger>`: array of combos (vector converter semantics —
  missing index → zero/empty; a non-array → empty trigger).

`config.json` examples (shipped `_RELEASE/config.json`):

```json
"t_select" : [ [ "kSpace" ], [ "bMiddle" ], [ "" ], [ "" ] ],
"t_lua_console" : [ [ "kF1" ] ]
```

The 4-slot padding exists only where `resizeTrigger` /
`fixupMissingTriggers` ran in-memory before the last save; `t_next`,
`t_previous`, `t_lua_console`, `t_pause` are NOT touched by
`keyboardBindsSanityCheck` and stay 1-slot in the shipped file.

## 7. Open Hexagon bind tables (`Global/Config`)

`Tid` (Config.hpp): `Unknown=-1, RotateCCW=0, RotateCW, Focus, Select,
Exit, ForceRestart, Restart, Replay, Screenshot, Swap, Up, Down, NextPack,
PreviousPack, LuaConsole, Pause, TriggersCount` (16 entries).

- Defaults (one input per combo) and JSON keys — see mapping doc §6.2 rows
  86–101 (e.g. `t_select` = `[kSpace, bMiddle]`, `t_focus` =
  `[kLShift, bXButton1]`, `t_rotate_ccw` = `[kA, kLeft, bLeft]`).
- `maxBinds = 4`. `resizeTrigger(trig)`: erase every unbound combo
  (agglomerate); pop back while `> 4`; pad with `Combo({KKey::Unknown})`
  while `< 4`.
- `keyboardBindsSanityCheck()`: `resizeTrigger` on the FIRST 12 triggers
  only (RotateCCW..Down) — NOT NextPack/PreviousPack/LuaConsole/Pause
  (called from `HexagonGame` construction).
- `getKeyboardBindNames(Tid)`: iterate combos up to the first unbound;
  per combo run BOTH loops — first set key bit (`j` in `0..=KeyCount`,
  name = `KKey(j-1)`) then first set button bit (`MBtn(j-1)`) — i.e. a
  combo holding a key AND a button names both; append via
  `bindToHumanReadableName` (strip leading `k`; `bLeft→LMB`,
  `bRight→RMB`, `bMiddle→MMB`), join `", "`, upper-case the whole result
  (`Utils::uppercasify` = per-char `std::toupper`).
- `rebindTrigger(trig, key, btn, index)`: `index >= 4` → index 0 AND
  `clearBind()` slot 0; `key > -1` → `addKey(KKey(key))` else
  `addBtn(MBtn(btn))`.
- `clearTriggerBind(trig, index)`: `combos[index].clearBind()`.
- `getTrigger(Tid)` → the `LinkedValue<Trigger>`'s value via the
  `triggerGetters` function-pointer array.

Joystick: `Jid` (Core/Joystick.hpp): `Unknown=-1, Select=0, Exit, Focus,
Swap, ForceRestart, Restart, Replay, Screenshot, NextPack, PreviousPack,
AddToFavorites, FavoritesMenu, JoystickBindsCount` (12).

- The 12 `j_*` config values are SFML joystick button indices on
  controller 0 (hardcoded `joyId = 0`); **`33` is the "unbound" sentinel**
  (no SFML button has index 33).
- `getJoystickBindName(Jid, vendorId)`: static 12×2 table
  `{{"A","SQUARE"},{"B","CROSS"},{"X","CIRCLE"},{"Y","TRIANGLE"},
  {"LB","L1"},{"RB","R1"},{"BACK","L2"},{"START","R2"},
  {"LEFT STICK","SELECT"},{"RIGHT STICK","START"},{"LT","LEFT STICK"},
  {"RT","RIGHT STICK"}}`; value 33 → `""`; vendor `0x045E` (MS) → column 0,
  `0x54C` (Sony) → column 1 (value `>= 12` → `""` for both); any other /
  absent vendor → `toStr(value)`.
- `loadAllJoystickBinds()`: `Joystick::setJoystickBind(getter[i](), i)` for
  `i in 0..12`.
- `joystickBindsSanityCheck()` is declared in Config.hpp but **has no
  definition in Config.cpp** (dead API).
- `Joystick::update(deadzone)`: poll controller 0 (PovX/PovY/X/Y axes,
  8 SFML buttons); stick Y is inverted vs. dpad Y (Up = `yIs(Right)`,
  Down = `yIs(Left)`); `risingEdge = pressed && !wasPressed`;
  `ignoreAllPresses(true)` latches all was-pressed flags and makes
  `update` a complete no-op.

## 8. `BindControl` (`Core/BindControl`)

Menu items (SSVMenuSystem `ItemBase`; the Rust port keeps the logic and
drops the menu plumbing — issue 11).

`KeyboardBindControl` (template over get/set/clear/callback fns):

- state: `waitingForBind`, `ID` (Tid), `hardcodedKey`
  (default `KKey::Unknown`).
- ctor: copies the combos; for each combo with the hardcoded key set →
  `mFuncClear(i)` (sanitize manual config edits).
- `getRealSize(combos)` = count of leading non-unbound combos.
- `addBind(key, btn)` = `mFuncSet(key, btn, getRealSize())` =
  `Config::rebindTrigger(trigger, key, btn, realSize)`.
- `clearBind` = `mFuncClear(getRealSize() - 1)`.
- `exec()` toggles `waitingForBind`.
- `erase()`: size 0 → false; else clear the LAST active slot +
  `callback(trigger, ID)`.
- `newKeyboardBind(KKey)`: reject (stop) if `== hardcodedKey`; stop if the
  key is already in any ACTIVE combo of this trigger (return true, no
  change); else `applyBind(key, MBtn::Left)` → addBind at the real size +
  callback + stop. (The `MBtn::Left` arg is ignored by `rebindTrigger`
  when `key > -1`.)
- `newKeyboardBind(MBtn)`: same with `applyBind(KKey::Unknown, btn)`.
- `getName()`: `name + ": " + Config::getKeyboardBindNames(ID)` + trailing
  `"_"` while waiting.

`JoystickBindControl`: `valueGetter`/`setButton`/callback, `ID` = Jid.

- `erase()`: value 33 → false; else `setButton(33)` + `callback(33, ID)`.
- `newJoystickBind(joy)`: no-op+stop if `joy == value`; else set +
  callback + stop.
- `getName()`: `name + ": " + Config::getJoystickBindName(ID)` (+ `"_"`).
