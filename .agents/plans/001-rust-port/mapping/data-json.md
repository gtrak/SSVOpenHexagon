# Mapping 03 — Data & JSON (`03-data-json.md`)

Mechanical transcription of the C++ JSON/data layer of Open Hexagon, for the
Rust port. Every JSON key string, type, and default below is copied from the
named C++ source. Where behavior is subtle (e.g. comment tolerance), it is
verified against the actual vendored parser (see §1.6, which includes
empirical test results).

Sources (all under `/home/gary/dev/SSVOpenHexagon`):

- `include/SSVOpenHexagon/Data/{CapColor,ColorData,LevelData,LevelStatus,LoadInfo,MusicData,PackData,PackInfo,ProfileData,StyleData}.hpp`
- `src/SSVOpenHexagon/Data/{CapColor,ColorData,LevelData,LevelStatus,LoadInfo,MusicData,ProfileData,StyleData}.cpp`
- `include/SSVOpenHexagon/Utils/LoadFromJson.hpp` + `src/SSVOpenHexagon/Utils/LoadFromJson.cpp`
- `include/SSVOpenHexagon/Global/UtilsJson.hpp` + `src/SSVOpenHexagon/Global/UtilsJson.cpp`
- `include/SSVOpenHexagon/SSVUtilsJson/**` + `src/SSVOpenHexagon/SSVUtilsJson/LinkedValue.cpp`
- `include/SSVOpenHexagon/Global/{Assets,AssetStorage,Config}.hpp` + `src/SSVOpenHexagon/Global/{Assets,AssetStorage,Config}.cpp`
- `include/SSVOpenHexagon/Utils/{LevelValidator,BuildPackId,Concat}.hpp` + `src/SSVOpenHexagon/Utils/{LevelValidator,BuildPackId}.cpp`
- `include/SSVOpenHexagon/Core/{Joystick,BindControl}.hpp` + `src/SSVOpenHexagon/Core/{Joystick,BindControl}.cpp`
- Supporting: `include/SSVOpenHexagon/Global/Version.hpp`,
  `extlibs/SSVUtils/include/SSVUtils/Core/FileSystem/{Path,Scan,Internal/ScanHelper}.hpp`,
  `extlibs/SSVStart/include/SSVStart/Utils/Input.hpp`,
  `extlibs/SSVStart/include/SSVStart/Input/Combo.hpp`
- On-disk samples: `_RELEASE/**` (assets.json, pack.json, level/style/music
  json, config.json, ConfigOverrides/*.json)

Legend: `getExtr<T>(obj, "key", default)` = "read JSON key `"key"` as T; if the
key is absent use `default`". `getExtr<T>(obj, "key")` = same but **no default**:
if absent the value comes from the JSON null singleton (int→0, float→0.0,
bool→false, string→""), see §1.4.

---

## §1. JSON foundation (`SSVUtilsJson`)

### 1.1 Vendored parser: jsoncpp fork

Files: `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/json.hpp` (642 lines,
declarations) and `jsoncpp.inl` (2442 lines, all definitions). Header-only
(`#pragma GCC system_header` on both; included via `JsonCpp.hpp`).

- Namespace `Json`. Core type `Json::Value` (tagged union: null, int, uint,
  real, string, bool, array, object).
- Objects store members in `std::map<CZString, Value>` → **object member order
  in serialized output is lexicographic by key bytes** (`CZString::operator<`
  is `std::strcmp`). Arrays store elements in the same map keyed by index
  `CZString(index)`; `Value::size()` for arrays = `last index + 1`.
- `static const Value nullJsonValue{};` (json.hpp, namespace `Json`) — the
  null singleton returned by const `operator[]` on missing keys. It is a
  `static` (internal linkage) variable in a header, i.e. **one copy per
  translation unit**; all inline functions reference the copy of their own TU.
- Number parsing (`Reader::decodeNumber`): a token containing `.`, `e`, `E`,
  `+` (or a `-` not at the start) is parsed with `sscanf("%lf")` and stored as
  `realValue`; otherwise parsed as 64-bit integer (stored `intValue` if it
  fits `LargestInt`, else `uintValue`). `readNumber` consumes digits plus
  `.`, `e`, `E`, `+`, `-`.
- String decoding: standard escapes `\"` `\/` `\\` `\b` `\f` `\n` `\r` `\t`
  and `\uXXXX` (including UTF-16 surrogate pairs → UTF-8).
- `Reader::Features`: `allowComments_ = true`, `strictRoot_ = false` by
  default (`Features::all()`; `strictMode()` = `{false, true}`). The default
  `Reader()` uses `Features::all()`. `strictRoot_` (unused by the game) would
  require the root to be array or object.
- **WARNING (latent bug, unused by game):** `Reader::parse(std::istream&, ...)`
  does `std::getline(sin, doc, (char)EOF)` — it reads only the first line.
  All game code uses `parse(const std::string&, ...)` (see `Io.hpp`), so this
  is never exercised.
- Error reporting: `Reader::getFormattedErrorMessages()` produces
  `* Line L, Column C\n  <message>\n` per error. `LoadInfo::addFormattedError`
  (§2.5) strips the leading `* ` (2 chars) and replaces the first `\n` with
  ` - ` etc.
- Writers: `FastWriter`, `StyledWriter`, `StyledStreamWriter`. The game uses
  only `StyledStreamWriter` (alias `ssvuj::Writer`), default indentation
  `"\t"`: each member on its own line, `"key" : value`, arrays either
  multi-line or single-line `[ a, b, c ]` when `lineLength < 74` (rightMargin)
  and no child is a non-empty array/object; a trailing `\n` is appended after
  the root. Strings are quoted by `valueToQuotedString` (escapes `"` `\` `\b`
  `\f` `\n` `\r` `\t`, control chars as `\uXXXX` uppercase hex, 4 digits).
  Doubles are written via `sprintf(buffer, "%#.16g", value)` with trailing
  zero-trimming (hence e.g. `7.300000190734863` in shipped config.json).
- `JSON_ASSERT_MESSAGE(cond, msg)` → `throw std::runtime_error(msg)` on
  failure (release builds included — this is how type-mismatch crashes happen,
  §1.4).

### 1.2 Aliases and helpers (`Global/Common.hpp`, `Utils/Utils.hpp`)

`namespace ssvuj`:

- `using Obj = Json::Value;` `using Key = std::string;` `using Idx = unsigned int;`
- `using Writer = Json::StyledStreamWriter;` `using Reader = Json::Reader;`
- `using Iterator / ConstIterator = Json::Value::(const_)iterator;`
- `template <typename T> struct Converter;` — extension point; `fromObj(const Obj&, T&)` and `toObj(Obj&, const T&)`.

`Utils/Main.hpp` (all in `ssvuj`):

- `getObj(obj)` / `getObj(obj, key)` / `getObj(arr, idx)` — accessors.
  - const: `obj[key]` → missing key returns the **null singleton** reference; missing array index → null singleton.
  - non-const: `obj[key]` **inserts** a null member when the key is missing; `arr[idx]` inserts an element at that index when missing.
- `getObjSize(obj)` = `obj.size()`; `getObjSize(obj, key)`; `getObjSize(arr, idx)`.
- `hasObj(obj, key)` = `obj.isObject() && obj.isMember(key)`; `hasObj(arr, idx)` = `arr.isArray() && idx < arr.size()`.
- `isObj(obj)` = `obj.isObject()`; `isObjArray(obj)` = `obj.isArray()`.
  - **Trap:** `isObj` takes `const Obj&`; a `const char*` argument is
    implicitly converted to a *string* `Value`, so `isObj("version")` is
    *always false*. This happens in `loadProfileFromJson` (§3.3) — see
    subtlety S6.
- `isObjType<T>(obj)` (§1.5).
- `arch(obj, value)` / `arch(obj, key, value)` / `arch(arr, idx, value)` — `Converter<T>::toObj`.
- `extr(obj, value)` / `extr(obj, key, value)` / `extr(arr, idx, value)` — `Converter<T>::fromObj` (note: does not call `T`'s constructor; the caller passes a reference to a default-constructed value).
- `getExtr<T>(obj)` / `getExtr<T>(obj, key)` / `getExtr<T>(arr, idx)` — convert and return by value (no default; see §1.4 for missing-key semantics).
- `getExtr<T>(obj, key, def)` = `hasObj(obj, key) ? getExtr<T>(obj, key) : def`; same for `(arr, idx, def)`.
- `getArch(value)` — fresh `Obj` archived from `value`.
- `getKey(iterator)` = `getExtr<Key>(itr.key())`.
- free `begin/end` for `Obj`.

### 1.3 IO (`Utils/Io.hpp`)

- `readFromString(Obj& obj, const std::string& str)` → `Impl::tryParse`:
  `Json::Reader reader; reader.parse(str, obj, false)` (collectComments =
  **false**). On failure logs
  `ssvuj::logReadError << reader.getFormattedErrorMessages() << "\nFrom: [" << str << "]"`
  and returns false.
- `readFromFile(Obj& obj, const ssvufs::Path& path)` → same, with
  `path.getContentsAsStr(thread_local static buffer)` (whole file, binary
  read).
- `readFromFile(Obj& obj, const ssvufs::Path& path, std::string& error)`:
  on failure `error = ""` if the reader produced no message, else
  `error = reader.getFormattedErrorMessages() + " in file " + path.getFileName()`.
- `getFromStr(str)` / `getFromFile(path)` — return `Obj`, swallow errors.
- `getFromFileWithErrors(path)` → `std::pair<Obj, std::string>` — the workhorse
  used by Assets and Config-overrides.
- `writeToStream(obj, ostream)` → `Json::StyledStreamWriter writer; writer.write(stream, obj); stream.flush();`
- `writeToString`, `writeToFile(obj, path)` (plain `std::ofstream`, no
  pretty-printing beyond the StyledStreamWriter default), `getWriteToString`.

### 1.4 Missing-key and type-mismatch semantics (empirically verified)

Because `getExtr<T>(obj, key)` (no default) reads through const
`operator[]`:

| Case | Result |
|---|---|
| key absent, `T = int` | `0` (null → `asInt()` → 0) |
| key absent, `T = float` | `0.0f` |
| key absent, `T = bool` | `false` |
| key absent, `T = std::string` | `""` |
| key absent, `T = vector<T>` | `size()` of null = 0 → empty vector |
| value is string but `T` numeric | **throws** `std::runtime_error("Value is not convertible to int.")` (or `float`/`double`/`bool`) |
| value null, numeric T | 0 / 0.0 (no throw) |

These exceptions are only caught in `HGAssetsImpl::loadPackAssets`
(Assets.cpp, try/catch around the per-pack subloaders) → recorded in
`LoadInfo::errorMessages` as `FATAL ERROR, Exception during asset loading: <what>`
and the whole pack asset load aborts. Nothing else in the JSON layer catches
exceptions (Config, profiles, assets.json: uncaught → crash).

Verified with a standalone harness compiled against
`SSVUtilsJson/JsonCpp/JsonCpp.hpp` (default `Reader`, `parse(str, root, false)`).

### 1.5 Type checks (`Utils/TypeChecker.hpp`)

`ssvuj::Impl::isObjType<T>`: `Obj`→always true; `char`,`int`→`isInt()`;
`unsigned char`,`unsigned int`→`isUInt()`; `float`,`double`→`isDouble()`
(**true also for integral values** — `isDouble() = isReal() || isIntegral()`);
`bool`→`isBool()`; `const char*`,`std::string`→`isString()`; `long`→`isInt64()`;
`unsigned long`→`isUInt64()`.

### 1.6 `//` comments and trailing commas — EXACT behavior

**Parser:** the vendored jsoncpp fork (§1.1), no preprocessing anywhere in the
codebase (verified: no string-stripping code touches JSON text before
`Reader::parse`).

Comment tolerance (verbatim from `jsoncpp.inl`):

- `Reader::readToken`: `case '/': token.type_ = tokenComment; ok = readComment();`
- `readComment()`: `if(c == '*') successful = readCStyleComment(); else if(c == '/') successful = readCppStyleComment();`
- `readCppStyleComment()`: consumes until `'\r'` or `'\n'` (the newline itself is NOT consumed), always succeeds.
- `readCStyleComment()`: consumes until `*` followed by `/`; success iff that pair was found.
- `skipCommentTokens(token)`: `if(features_.allowComments_) { do { readToken(token); } while(token.type_ == tokenComment); } else readToken(token);` — used by `readValue()` and by `parse()` after the root.
- `readObject()`/`readArray()` additionally skip `tokenComment` tokens between
  members/elements, and `readObject` skips comments after the key
  (`while(tokenName.type_ == tokenComment ...)`).
- Since the default `Features::all()` has `allowComments_ = true`, **both `//`
  and `/* */` comments are accepted in every position**: before the root,
  between members, after a comma, at end of line after a value, inside arrays.
  Comments are never collected (game always parses with
  `collectComments = false`).

Trailing commas: **NOT accepted**, in objects OR arrays. Empirical results
(harness, same reader settings as the game):

```
{"a": 1, "b": 2,}      -> PARSE-FAIL  "* Line 4, Column 5
                            Missing '}' or object member name"
{"arr": [1, 2, 3,]}    -> PARSE-FAIL  "* Line 2, Column 25
                            Syntax error: value, object or array expected."
{
  "x": [
    1, // one
    2,
  ],
}                       -> PARSE-FAIL (same array error at the ',' after 2)
```

Note: `readObject` contains `if(tokenName.type_ == tokenObjectEnd &&
name.empty()) return true;` which looks like trailing-comma support but is
not: `name` is only empty on the very first iteration, so the branch only
handles an immediately-closed empty object `{}` (and `{ /*c*/ }`). After a
trailing comma, `name` still holds the previous key and the loop breaks with
the error above.

**Consequence for the Rust port:** the parser must accept `//` and `/* */`
comments (including inside string-free positions, at token boundaries), and
must reject (or at least faithfully report) trailing commas. No shipped
`_RELEASE` JSON file uses trailing commas; many DO use `//` comments
(e.g. `Packs/artwork/Styles/pointless.json`, `Packs/cube/Music/*.json`).

### 1.7 Basic converters (`Utils/BasicConverters.hpp`)

- `Converter<T>` for enums: `fromObj` = `T(getExtr<underlying>(obj))`; `toObj` = archive underlying.
- Macro-generated: `Obj` (copy), `char`/`unsigned char` (`T(mObj.asInt())`),
  `int` (`asInt`), `float` (`asFloat`), `double` (`asDouble`), `bool`
  (`asBool`), `std::string` (`asString`), `const char*` (`asCString`).
- `long` → `asLargestInt()`; `unsigned int`, `unsigned short` → `asUInt()`;
  `unsigned long` → `asLargestUInt()`.
- `std::vector<TItem, TAlloc>`: `fromObj` = `resize(obj.size()); for i in
  0..size: extr(obj, i, v[i])` (missing index → null → zero/empty value);
  `toObj` = `for i: arch(obj, i, v[i])`.
- `std::unordered_map<TKey, TValue, ...>`: `fromObj` = for each name in
  `obj.getMemberNames()`: `emplace(name, getExtr<TValue>(obj[name]))`;
  `toObj` = for each `[k,v]`: `arch(obj, k, v)`. (Object members only; a null
  object yields an empty map.)
- C arrays `TItem[TN]`: per-index `extr`/`arch` over `0..TN`.

### 1.8 Game-specific converters (`Global/UtilsJson.hpp` / `.cpp`)

All in `namespace ssvuj`, specialized `Converter`s:

- `sf::Vector2f`: `[x, y]` — array indices 0 and 1, floats.
- `sf::Color`: `[r, g, b, a]` — array indices 0..3, each converted as
  `unsigned char` from `asInt()`. (JSON colors are 4-element integer arrays;
  e.g. `"value": [255, 255, 255, 255]`.)
- `sf::Keyboard::Key`: a **string** name; `fromObj` =
  `ssvs::getKKey(getExtr<std::string>(obj))`; `toObj` =
  `ssvs::getKKeyName(value)`; special case `Key::Unknown` → archives `""`
  (empty string).
- `sf::Mouse::Button`: a **string** name; `ssvs::getMBtn` / `ssvs::getMBtnName`.
- `ssvs::Input::Combo`: an **array of strings**, one entry per bound
  input:
  - `fromObj`: clears the bind; for each string element:
    - `""` → `addKey(KKey::Unknown)` (marks the combo unbound);
    - valid key name (`ssvs::isKKeyNameValid`) → `addKey(getExtr<sf::Keyboard::Key>(i))`;
    - valid button name (`ssvs::isMBtnNameValid`) → `addBtn(getExtr<sf::Mouse::Button>(i))`;
    - otherwise logs `"<elem> is not a valid input name, an empty bind has
      been put in its place"` and `addKey(KKey::Unknown)`.
    - Note: a combo with multiple entries means ANY of them triggers
      (OR semantics); a single combo is AND of its entries? No — each entry is
      one input; multiple entries in one combo = multiple alternative inputs
      for the same bind slot.
  - `toObj`: if `combo.isUnbound()` → `arch(obj, 0, sf::Keyboard::Key(-1))`
    i.e. archives `[""]` (index 0 = empty string). Otherwise archives all set
    keys first (looping `Key(0)..Key(KeyCount-1)`, ascending enum order, each
    as its name), then all set buttons (`Button(0)..ButtonCount-1`), at
    increasing array indices.
- `ssvs::Input::Trigger`: wraps `std::vector<Combo>` — serialized as an array
  of combos: `[[ "kSpace", "bMiddle", "", "" ], ...]` (see shipped
  `config.json`, §6).

Key/button name vocabulary (`extlibs/SSVStart/Utils/Input.hpp`,
prefixes `SSVS_KEY_PREFIX "k"` / `SSVS_BTN_PREFIX "b"`):

- Keys: `"k" + <SFML Keyboard::Key enum name>` for: `A`–`Z`, `Num0`–`Num9`,
  `Escape`, `LControl`, `LShift`, `LAlt`, `LSystem`, `RControl`, `RShift`,
  `RAlt`, `RSystem`, `Menu`, `LBracket`, `RBracket`, `Semicolon`, `Comma`,
  `Period`, `Apostrophe`, `Slash`, `Backslash`, `Grave`, `Equal`, `Hyphen`,
  `Space`, `Enter`, `Backspace`, `Tab`, `PageUp`, `PageDown`, `End`, `Home`,
  `Insert`, `Delete`, `Add`, `Subtract`, `Multiply`, `Divide`, `Left`,
  `Right`, `Up`, `Down`, `Numpad0`–`Numpad9`, `F1`–`F15`, `Pause`.
- Buttons: `bLeft`, `bRight`, `bMiddle`, `bXButton1`, `bXButton2`.
- `KeyBitset`/`BtnBitset` use a **1-based** index (`getKeyBit(keys, k)` is
  bit `int(k)+1`; bit 0 / index 0 is the "Unknown/unbound" marker), which is
  why `getKeyboardBindNames` (Config.cpp) iterates `j` over `0..=KeyCount`
  and names `KKey(j - 1)`.
- Hardcoded-key set (`isKKeyHardcoded`, used by HexagonGame, not by the
  converters): `J K L R Y Escape LAlt Enter Backspace Up Down F1 F2 F3 F4`.

### 1.9 `LinkedValue<T>` (`SSVUtilsJson/LinkedValue/`)

`ssvuj::LinkedValue<T>`: holds `name` (JSON key, `const char*`), `value`,
`defValue`.

- `syncFrom(const Obj& obj)`: `value = getExtr(obj, name, defValue);`
- `syncTo(Obj& obj) const`: `arch(obj, name, value);`
- `resetToDefault()`: `value = defValue;`
- `getDefault()`.
- Implicit conversion to `T&` / `const T&`; `operator=` from `T`.
- Explicit instantiations: `bool`, `int`, `float`, `unsigned int`,
  `unsigned short`, `std::string`, `std::vector<std::string>`,
  `ssvs::Input::Trigger`.
- Used by Config for **every** config value (§6).

---

## §2. Data types (`include/SSVOpenHexagon/Data/` + `src/.../Data/`)

### 2.1 CapColor (`CapColor.hpp` / `CapColor.cpp`)

```cpp
namespace CapColorMode {
struct Main         { };
struct MainDarkened { };
struct ByIndex      { int _index; };
}
using CapColor = vittorioromeo::tinyvariant<
    CapColorMode::Main, CapColorMode::MainDarkened,
    CapColorMode::ByIndex, ColorData>;
```

`parseCapColor(const ssvuj::Obj& obj) noexcept`:

1. If `obj` is a JSON string (`isObjType<std::string>`):
   - `"main"`            → `CapColorMode::Main{}`
   - `"main_darkened"`   → `CapColorMode::MainDarkened{}`
2. Else if `obj` is a JSON object:
   - `legacy` (bool, default **true**):
     - true  → `ByIndex{ index }`, `index` = `getExtr<int>(obj, "index", 0)`
     - false → `ColorData{
                 main=false,
                 dynamic        = getExtr<bool>(obj, "dynamic", false),
                 dynamic_offset = getExtr<bool>(obj, "dynamic_offset", false),
                 dynamic_darkness = getExtr<float>(obj, "dynamic_darkness", 1.f),
                 hue_shift      = getExtr<float>(obj, "hue_shift", 0.f),
                 offset         = getExtr<float>(obj, "offset", 0.f),
                 value          = getExtr<sf::Color>(obj, "value", sf::Color::White),
                 pulse          = pulse_from_json(obj) }`
                 (note: `main` is hardwired to `false` here, unlike
                 `ColorData(mRoot)` which reads `"main"`.)
3. Fallback (null / number / array): `ByIndex{0}`.

Resolution in `StyleData::getCapColorResult()`:

- `Main`          → `getMainColor()`
- `MainDarkened`  → `getColorDarkened(getMainColor(), 1.4f)`
- `ByIndex x`     → `getColor(x._index)` (mod-indexed into computed colors)
- `ColorData d`   → `calculateColor(currentHue, pulseFactor, d)`

JSON keys used: `"main"`, `"main_darkened"` (as strings), `"legacy"`,
`"index"`, `"dynamic"`, `"dynamic_offset"`, `"dynamic_darkness"`,
`"hue_shift"`, `"offset"`, `"value"`, `"pulse"`.

### 2.2 ColorData + PulseColor (`ColorData.hpp` / `ColorData.cpp`)

```cpp
struct PulseColor { int r; int g; int b; int a; };   // NO in-struct defaults

struct ColorData
{
    bool  main{};
    bool  dynamic{};
    bool  dynamicOffset{};
    float dynamicDarkness{};
    float hueShift{};
    float offset{};
    sf::Color color{};      // (0,0,0,255)
    PulseColor pulse{};     // uninitialized members! (see note)
};
```

Note: `PulseColor` has no member initializers; the default-constructed
`ColorData{}` therefore has an indeterminate `pulse` unless built via the
JSON constructor (which always sets it) — in practice only JSON-constructed
instances are used.

`ColorData(mRoot)` — JSON keys and defaults:

| field | JSON key | type | default |
|---|---|---|---|
| main | `"main"` | bool | `false` |
| dynamic | `"dynamic"` | bool | `false` |
| dynamicOffset | `"dynamic_offset"` | bool | `false` |
| dynamicDarkness | `"dynamic_darkness"` | float | `1.f` |
| hueShift | `"hue_shift"` | float | `0.f` |
| offset | `"offset"` | float | `0.f` |
| color | `"value"` | `sf::Color` (`[r,g,b,a]` int array) | `sf::Color::Black` |
| pulse | `"pulse"` | see below | `{0,0,0,255}` |

`pulse_from_json(root)`:

- if `!hasObj(root, "pulse")` → `{0, 0, 0, 255}`;
- else `pulseObj = getObj(root, "pulse")` (an array) and
  `r = getExtr<int>(pulseObj, 0)`, `g = ... 1`, `b = ... 2`, `a = ... 3`
  (array indices; missing index → null → 0).

Also a plain 8-arg constructor `(main, dynamic, dynamicOffset,
dynamicDarkness, hueShift, offset, color, pulse)` used by `parseCapColor`.

### 2.3 LevelData (`LevelData.hpp` / `LevelData.cpp`)

Fields (all non-static, in declaration order; **no in-class defaults** except
via the single constructor):

```cpp
std::string packPath;
std::string packId;
std::string id;
std::string name;
std::string description;
std::string author;
int  menuPriority;
bool selectable;
std::string musicId;
std::string soundId;
std::string styleId;
std::string luaScriptPath;
std::vector<float> difficultyMults;
bool unscored;
std::unordered_map<float, std::string> validators;
std::unordered_map<float, std::string> validatorsWithoutPackId;
```

Constructor `LevelData(mRoot, mPackPath, mPackId)` — level JSON keys:

| field | JSON key | type | default |
|---|---|---|---|
| packPath | — (param) | string | caller's pack folder path (WITH trailing slash, §4.1) |
| packId | — (param) | string | caller's pack id |
| id | `"id"` | string | `"nullId"` |
| name | `"name"` | string | `"nullName"` |
| description | `"description"` | string | `""` |
| author | `"author"` | string | `""` |
| menuPriority | `"menuPriority"` | int | `0` |
| selectable | `"selectable"` | bool | `true` |
| musicId | `"musicId"` | string | `"nullMusicId"` |
| soundId | `"soundId"` | string | `"nullSoundId"` |
| styleId | `"styleId"` | string | `"nullStyleId"` |
| luaScriptPath | `"luaFile"` | string | `"nullLuaPath"`, **concatenated**: `packPath + luaFile` |
| difficultyMults | `"difficultyMults"` | `vector<float>` | `{}` (then `1.f` is appended and the vector sorted) |
| unscored | `"unscored"` | bool | `false` |

Post-construction: for each `dm` in `difficultyMults` (after appending `1.0`
and sorting):

- `validators[dm]                 = getLevelValidator(packId + "_" + id, dm)`
- `validatorsWithoutPackId[dm]    = getLevelValidator(id, dm)`

(`getLevelValidator` format: §7.1.)

Methods:

- `getNthDiffMult(index)`: wraps `index` modulo `difficultyMults.size()`
  (negative and out-of-range both wrap), returns that element.
- `getValidator(diffMult)` / `getValidatorWithoutPackId(diffMult)`:
  `assert(found); return map.at(diffMult)`.

Level asset id used by the engine = `packId + "_" + levelData.id`
(Assets.cpp, `concatIntoBuf`).

### 2.4 LevelStatus (`LevelStatus.hpp` / `LevelStatus.cpp`)

Runtime per-level state (NOT loaded from JSON — mutated by Lua; defaults
below are the C++ in-class defaults):

```cpp
std::unordered_map<std::string, std::string> trackedVariables;  // {}

bool  scoreOverridden{false};
std::string scoreOverride;                        // ""

bool  syncMusicToDM{true};
float musicPitch{1.f};
std::string beepSound{"beep.ogg"};
std::string levelUpSound{"increment.ogg"};
std::string swapSound{"swap.ogg"};
std::string deathSound{"death.ogg"};

float speedMult{1.f};
float playerSpeedMult{1.f};
float speedInc{0.f};
float speedMax{0.f};
float rotationSpeed{0.f};
float rotationSpeedInc{0.f};
float rotationSpeedMax{0.f};
float delayMult{1.f};
float delayInc{0.f};
float delayMin{0.f};
float delayMax{0.f};
float fastSpin{0.f};
float incTime{15.f};
float pulseMin{75.f};
float pulseMax{80.f};
float pulseSpeed{0.f};
float pulseSpeedR{0.f};
float pulseDelayMax{0.f};
float pulseInitialDelay{0.f};
float swapCooldownMult{1.f};

// beat pulse (center polygon)
float beatPulseInitialDelay{0.f};
float beatPulseMax{0.f};
float beatPulseDelayMax{0.f};
float beatPulseSpeedMult{1.f};

float radiusMin{72.f};
float wallSkewLeft{0.f};
float wallSkewRight{0.f};
float wallAngleLeft{0.f};
float wallAngleRight{0.f};
float wallSpawnDistance{1600.f};

float cameraShake{0};

unsigned int sides{6};
unsigned int sidesMax{6};
unsigned int sidesMin{6};

bool swapEnabled{false};
bool tutorialMode{false};
bool _3DRequired{false};
bool shadersRequired{false};
bool incEnabled{true};
bool rndSideChangesEnabled{true};
bool darkenUnevenBackgroundChunk{true};

bool manualPulseControl{false};
bool manualBeatPulseControl{false};

unsigned long currentIncrements{0u};
```

Constructor `LevelStatus(syncMusicToDM, wallSpawnDistance)` overrides only
those two. Helpers: `hasSpeedMaxLimit() = speedMax > 0.f`,
`hasDelayMaxLimit() = delayMax > 0.f`.

### 2.5 LoadInfo (`LoadInfo.hpp` / `LoadInfo.cpp`)

```cpp
unsigned int packs{0};
unsigned int levels{0};
unsigned int assets{0};
std::vector<std::string> errorMessages;
```

`addFormattedError(std::string& error)` — formats a jsoncpp
`getFormattedErrorMessages()` string (only if non-empty):

1. `error.erase(0, 2)` — removes the leading `"* "`.
2. First `\n`: insert one space before it, replace the `\n` with `-`, erase
   the following character (the 2-space indent) → `"... Line L, Column C - message"`.
3. All remaining `\n` are deleted.
4. `errorMessages.emplace_back(error)`.

### 2.6 MusicData (`MusicData.hpp` / `MusicData.cpp`)

```cpp
struct Segment { float time; float beatPulseDelayOffset; };  // NO defaults

class MusicData
{
private:
    std::vector<Segment> segments;          // {}
public:
    std::string id;                         // ""
    std::string fileName;                   // ""
    std::string name;                       // ""
    std::string album;                      // ""
    std::string author;                     // ""
    bool firstPlay{true};
};
```

Constructors: default; and `(id, fileName, name, album, author)`.

Methods:

- `getSegment(index)` → `segments[index]`.
- `addSegment(seconds, beatPulseDelayOffset)` → push `Segment`.
- `playRandomSegment(packId, audio)`: first call (`firstPlay` true) →
  segment 0 and set `firstPlay=false`; otherwise
  `idx = ssvu::getRndI(0, segments.size())` (uniform) → `playSegment`.
- `playSegment(packId, audio, idx)`: `playSeconds(packId, audio,
  segments[idx].time)`.
- `playSeconds(packId, audio, seconds)`: `audio.loadAndPlayMusic(packId, id,
  seconds)`; on failure logs
  `MusicData::playSeconds << "Failed playing music '" << packId << '_' << id << "'\n"`.

JSON shape is handled by `Utils::loadMusicFromJson` (§3.1). Music asset id in
the storage maps = `packId + "_" + musicData.id`.

### 2.7 PackData / PackDependency / PackInfo

`PackDependency` (no defaults):

```cpp
std::string disambiguator;
std::string name;
std::string author;
int minVersion;
```

`PackData` (no defaults):

```cpp
std::string folderPath;      // pack folder path WITH trailing slash
std::string id;              // buildPackId(...) — §7.2
std::string disambiguator;
std::string name;
std::string author;
std::string description;
int  version;
float priority;
std::vector<PackDependency> dependencies;
```

`PackInfo` (no defaults):

```cpp
std::string id;
std::string path;
```

Loaded from `pack.json` in `HGAssetsImpl::loadPackData` — see §4.2 for the
exact keys/defaults.

### 2.8 ProfileData (`ProfileData.hpp` / `ProfileData.cpp`)

```cpp
class ProfileData
{
private:
    GameVersion version;                    // {major, minor, micro}
    std::string name;
    std::unordered_map<std::string, float> scores;
    std::unordered_set<std::string> favoriteLevelsDataIDs;
};
```

`GameVersion` (`Global/Version.hpp`): `{int major; int minor; int micro;}`
with `<`, `==`, `!=`; `GAME_VERSION = {2,1,7}`, `GAME_VERSION_STR = "2.1.7"`.

Constructor `(mVersion, mName, mScores, mFavorites /*vector<string>*/)` —
favorites converted via `Utils::toUnorderedSet`.

Methods: `getVersion`, `getName`, `getScores`, `getFavoriteLevelIds()`
(mutable + const), `setScore(id, score)` (`scores[id] = score`),
`getScore(id)` → **0.0f if missing**, `addFavoriteLevel`,
`removeFavoriteLevel`, `isLevelFavorite`.

JSON loading via `Utils::loadProfileFromJson` (§3.3); saving in
`saveCurrentLocalProfile` / `saveAllProfiles` (§4.7).

### 2.9 StyleData (`StyleData.hpp` / `StyleData.cpp`)

Public state (defaults in braces are in-class):

```cpp
std::string id{};
float hueMin{};  float hueMax{};  float hueIncrement{};
bool  huePingPong{};
float pulseMin{};  float pulseMax{};  float pulseIncrement{};
float maxSwapTime{};
float _3dDepth{};  float _3dSkew{};  float _3dSpacing{};
float _3dDarkenMult{};  float _3dAlphaMult{};  float _3dAlphaFalloff{};
float _3dPulseMax{};  float _3dPulseMin{};  float _3dPulseSpeed{};
float _3dPerspectiveMult{};
float bgTileRadius{10000.f};
unsigned int BGColorOffset{0};
float BGRotOff{0};   // degrees
```

Private state: `currentHue{0}`, `currentSwapTime{0}`, `pulseFactor{0}`,
`currentMainColor{Black}`, `currentPlayerColor{Black}`,
`currentTextColor{Black}`, `currentWallColor{White}`,
`current3DOverrideColor{Black}`, `std::vector<sf::Color> currentColors`,
`_3dOverrideColor`, `ColorData mainColorData/playerColor/textColor/wallColor`,
`CapColor capColor`, `std::vector<ColorData> colorDatas`.

JSON constructor `StyleData(mRoot)` — exact keys and defaults:

| field | JSON key | type | default |
|---|---|---|---|
| id | `"id"` | string | `"nullId"` |
| hueMin | `"hue_min"` | float | `0.f` |
| hueMax | `"hue_max"` | float | `360.f` |
| hueIncrement | `"hue_increment"` | float | `0.f` |
| huePingPong | `"hue_ping_pong"` | bool | `false` |
| pulseMin | `"pulse_min"` | float | `0.f` |
| pulseMax | `"pulse_max"` | float | `0.f` |
| pulseIncrement | `"pulse_increment"` | float | `0.f` |
| maxSwapTime | `"max_swap_time"` | float | `100.f` |
| _3dDepth | `"3D_depth"` | float | `15.f` |
| _3dSkew | `"3D_skew"` | float | `0.18f` |
| _3dSpacing | `"3D_spacing"` | float | `1.f` |
| _3dDarkenMult | `"3D_darken_multiplier"` | float | `1.5f` |
| _3dAlphaMult | `"3D_alpha_multiplier"` | float | `0.5f` |
| _3dAlphaFalloff | `"3D_alpha_falloff"` | float | `3.f` |
| _3dPulseMax | `"3D_pulse_max"` | float | `3.2f` |
| _3dPulseMin | `"3D_pulse_min"` | float | `0.f` |
| _3dPulseSpeed | `"3D_pulse_speed"` | float | `0.01f` |
| _3dPerspectiveMult | `"3D_perspective_multiplier"` | float | `1.f` |
| _3dOverrideColor | `"3D_override_color"` | `sf::Color` | `sf::Color::Transparent` |
| mainColorData | `"main"` | ColorData object | **no hasObj check** — `ColorData(getObj(mRoot, "main"))`; if the key is missing the null singleton is used, which yields the all-defaults ColorData (`"value"`→Black, pulse {0,0,0,255}) |
| playerColor | `"player_color"` | ColorData object | mainColorData (via `colorDataFromObjOrDefault`) |
| textColor | `"text_color"` | ColorData object | mainColorData |
| wallColor | `"wall_color"` | ColorData object | mainColorData |
| capColor | `"cap_color"` | via `parseCapColor(getObj(mRoot, "cap_color"))` | (missing → ByIndex{0}) |
| colorDatas | `"colors"` | array of ColorData | iterated by index `0..getObjSize-1` (empty if missing) |

`colorDataFromObjOrDefault(root, key, def)` = `hasObj(root, key) ?
ColorData{getObj(root, key)} : def`.

Post-construction: `currentHue = hueMin`.

Style asset id in storage = `packId + "_" + styleData.id`.

Behavior (needed for the Rust port of rendering; not JSON, but part of the
file):

- `update(FT ft, float mult = 1.f)`:
  - `currentSwapTime += ft*mult`; wraps to 0 when `> maxSwapTime`.
  - `currentHue += hueIncrement*ft*mult`; when below `hueMin`: ping-pong →
    clamp to `hueMin`, `hueIncrement *= -1`; else wrap to `hueMax`. Above
    `hueMax` symmetric.
  - `pulseFactor += pulseIncrement*ft` (note: **no** `mult` here); at
    `pulseMin`/`pulseMax` reflect (`pulseIncrement *= -1`).
- `calculateColor(hue, pulseFactor, cd)`:
  - `color = cd.color`; if `cd.dynamic`: `h = fmod(hue + cd.hueShift, 360)/360`,
    `dyn = getColorFromHue(h)`; if `!cd.main`: if `cd.dynamicOffset`:
    if `cd.offset != 0`: each channel `+= dyn.channel / cd.offset`;
    `color.a += dyn.a`; else `color = getColorDarkened(dyn, cd.dynamicDarkness)`;
    else (main): `color = dyn`.
  - final: each channel = `componentClamp(color.c + cd.pulse.c * pulseFactor)`.
- `computeColors()`: recomputes the four current colors from their ColorData;
  `current3DOverrideColor = _3dOverrideColor.a != 0 ? _3dOverrideColor :
  getMainColor()`; recomputes `currentColors` from `colorDatas`; if more than
  one color: `rotation = currentSwapTime / (maxSwapTime/2.f)`;
  `ssvu::rotate(currentColors, begin + getMod(rotation + BGColorOffset,
  currentColors.size()))`.
- `drawBackgroundImpl(...)`: `div = tau/sides*1.0001`, `distance =
  bgTileRadius`; per side `i`: `angle = toRad(BGRotOff) + div*i`; color =
  `colors[i mod size]`; darken when `(i%2==0 && i==sides-1)` and
  `darkenUnevenBackgroundChunk` (factor 1.4); black-and-white → Black.
- `drawBackgroundMenuHexagonImpl(...)`: radius 75 (4:3) or 100; ring colors:
  main (or White in B&W) at `radius+10`, cap (or Black in B&W) at `radius`.

---

## §3. `Utils/LoadFromJson` (`LoadFromJson.hpp` / `.cpp`)

Three free functions in `hg::Utils`:

### 3.1 `loadMusicFromJson(const ssvuj::Obj& mRoot) -> MusicData`

| MusicData field | JSON key | type | default |
|---|---|---|---|
| id | `"id"` | string | **none** (→ `""` if missing) |
| fileName | `"file_name"` | string | **none** (→ `""`) |
| name | `"name"` | string | **none** (→ `""`) |
| album | `"album"` | string | **none** (→ `""`) |
| author | `"author"` | string | **none** (→ `""`) |

Then `for segment in getObj(mRoot, "segments")` (iterate the array; missing
`"segments"` → null → zero-size iteration):
`addSegment(getExtr<float>(segment, "time"),       // NO default → 0.0f if missing
            getExtr<float>(segment, "beat_pulse_delay_offset", 0.f))`.

Note: iteration is over the **array elements** of `"segments"` (each element
is an object with `"time"` and optionally `"beat_pulse_delay_offset"`).

### 3.2 `loadVersionFromJson(const ssvuj::Obj& mRoot) -> GameVersion`

| GameVersion field | JSON key | type | default |
|---|---|---|---|
| major | `"major"` | int | **none** (→ 0) |
| minor | `"minor"` | int | **none** (→ 0) |
| micro | `"micro"` | int | **none** (→ 0) |

### 3.3 `loadProfileFromJson(const ssvuj::Obj& mRoot) -> ProfileData`

| ProfileData field | JSON key | type | default |
|---|---|---|---|
| version | `"version"` (object) | GameVersion | `GameVersion{-1, 0, 0}` — BUT see bug below |
| name | `"name"` | string | **none** (→ `""`) |
| scores | `"scores"` | `unordered_map<string, float>` | **none** (→ empty; object keys → float values, missing/null value → 0.0f) |
| favorites | `"favorites"` | `vector<string>` | `{}` |

**BUG (verbatim from source):**

```cpp
const GameVersion version =
    ssvuj::isObj("version")                       // <-- checks the string literal, not mRoot!
        ? loadVersionFromJson(ssvuj::getObj(mRoot, "version"))
        : GameVersion{-1, 0, 0};
```

`ssvuj::isObj("version")` constructs a `Value` from the C-string and asks
whether *that* is an object → always false. Hence **every loaded profile gets
`version = {-1, 0, 0}` regardless of the file**; `loadVersionFromJson` is dead
code at runtime. (Savable via §4.7 which writes the real GAME_VERSION.) A Rust
port must decide: replicate the bug or read `"version"` properly — flag to the
orchestrator; the shipped `_RELEASE` has no `Profiles/` directory so this is
invisible in the release data.

---

## §4. `Global/Assets` (`Assets.hpp` / `Assets.cpp`)

`HGAssets` is a pimpl façade over `HGAssetsImpl`. Everything below is
transcribed from `HGAssetsImpl` in `Assets.cpp` (1874 lines).

### 4.1 Storage maps (impl members)

```cpp
Steam::steam_manager* steamManager;
bool levelsOnly{false};
Utils::UniquePtr<AssetStorage> assetStorage;

std::unordered_map<std::string, LevelData> levelDatas;                  // key: "<packId>_<levelId>"
std::unordered_map<std::string, std::vector<std::string>> levelDataIdsByPack; // key: packId
std::unordered_map<std::string, PackData> packDatas;                   // key: packId
std::vector<PackInfo> packInfos;
std::vector<PackInfo> selectablePackInfos;                             // packs that have Levels/ AND pass dependency check

std::unordered_map<std::string, std::string> musicPathMap;             // key: "<packId>_<fileBase>" -> full .ogg path
std::map<std::string, MusicData>  musicDataMap;                        // key: "<packId>_<musicId>" (sorted map)
std::map<std::string, StyleData>  styleDataMap;                        // key: "<packId>_<styleId>"
std::map<std::string, ProfileData> profileDataMap;                     // key: profile name
ProfileData* currentProfilePtr{nullptr};

std::unordered_set<std::string> packIdsWithMissingDependencies;

struct LoadedShader {
    Utils::UniquePtr<sf::Shader> shader;
    std::string path;
    sf::Shader::Type shaderType;
    std::size_t id;
};
std::unordered_map<std::string, LoadedShader> shaders;                 // key: "<packId>_<fileName>" (with extension)
std::unordered_map<std::string, std::size_t> shadersPathToId;          // key: full shader path
std::vector<sf::Shader*> shadersById;

std::string buf;                                                       // concat buffer
std::unordered_map<std::string, std::string> luaFileCache;             // populated elsewhere (Lua)
LoadInfo loadInfo;
std::unordered_set<std::string> cachedWorkshopPackIds;                 // Steam-fallback cache
```

Path note (critical for the Rust port): `ssvufs::Path` **normalizes** on
`getStr()`: backslashes → `/`, duplicate slashes collapsed, and **a trailing
`/` is appended iff the path exists as a folder** (`Path::normalize()`).
`ssvufs::scan` composes results as `mPath + entryName` through `operator+=`,
so scan of `"Packs/"` yields folder paths **with trailing slash**
(`"Packs/base/"`) and file paths without (`"Packs/base/Levels/pi.json"`).
Consequently `PackData::folderPath` always ends with `/`, and subfolder
checks like `packPath + "Levels/"` work.

### 4.2 `loadPackData(packPath)` — pack discovery + `pack.json`

1. Requires `ssvufs::Path{packPath + "/pack.json"}.isFile()`; else returns
   false (caller logs `"Error loading pack data '<packPath>'\n"` into
   `loadInfo.errorMessages`).
2. Parses via `ssvuj::getFromFileWithErrors(packPath + "/pack.json")`; parse
   errors are appended via `loadInfo.addFormattedError(error)`.
3. Keys:

| PackData field | JSON key | type | default |
|---|---|---|---|
| disambiguator | `"disambiguator"` | string | `"no disambiguator"` |
| name | `"name"` | string | `"unknown name"` |
| author | `"author"` | string | `"unknown author"` |
| description | `"description"` | string | `"no description"` |
| version | `"version"` | int | `0` |
| priority | `"priority"` | float | `100` |

4. `packId = Utils::buildPackId(disambiguator, author, name, version)` (§7.2).
   NOTE: `buildPackId` asserts `packVersion > 0` — a pack.json without
   `"version"` (default 0) **asserts in debug builds**.
5. `dependencies`: only read `if hasObj(packRoot, "dependencies")`; array of
   objects with keys `"disambiguator"` (string, no default), `"name"` (string,
   no default), `"author"` (string, no default), `"min_version"` (int, no
   default).
6. `packInfos.emplace_back(PackInfo{packId, packPath})`;
   `packDatas.emplace(packId, PackData{...})` with `folderPath =
   packPath.getStr()`.

### 4.3 Pack discovery order and priority (`loadAllPackDatas`)

1. `"Packs/"` must exist as a folder, else the whole load fails
   (`"Folder Packs/ does not exist"`).
2. **Local packs first**: `scanSingleFolderName("Packs/")` =
   `scan<Mode::Single, Type::Folder, Pick::Any>` (default
   `Sort::Alphabetic`) of the immediate subfolders of `Packs/` (i.e.
   `"Packs/artwork/"`, `"Packs/base/"`, `"Packs/cube/"`, ...; each tried via
   `loadPackData`).
3. **Steam Workshop packs second** (only if `steamManager != nullptr`):
   - if `steamManager->is_initialized()`:
     `steamManager->for_workshop_pack_folders(tryLoadPackFromPath)` —
     enumerates installed workshop pack folders (order defined by SSVStart's
     Steam wrapper, after the local ones).
   - else if `loadWorkshopPackDatasFromCache()`: reads
     `"workshopCache.json"` (top-level key `"cachedPacks"` = array of
     strings = pack folder paths; if the value is null or not an array →
     log + return; if the array is empty → log + return; missing key →
     log `"[ERROR]: Cannot locate cache array in workshop cache file\n"` +
     return). Each cached path is passed to `loadPackData`.
4. A pack is only loaded once: duplicate pack ids simply overwrite
   `packDatas` (`unordered_map::emplace` — actually `emplace` does NOT
   overwrite: a second pack with the same id is dropped from `packDatas` but
   still appended to `packInfos`; in practice ids are unique because the
   disambiguator differs).
5. Each successful `loadPackData` increments `loadInfo.packs`.

**Priority rules** (applied after all pack assets load, in the constructor):

- `levelDataIdsByPack[pack]` vectors are sorted by
  `levelDatas[id].menuPriority` ascending (stable: `std::sort`, comparator
  reads `menuPriority`).
- `packInfos` and `selectablePackInfos` are sorted by
  `PackData::priority` ascending (smaller = earlier; shipped packs use very
  negative priorities: base `-99999.04`, cube `-99999.02` etc., so the
  bundled packs come before user packs with default priority 100).

### 4.4 `loadPackAssets(packData, headless)` — per-pack asset load

For pack folder `packPath` (string, trailing slash) and `packId`:

Order and gating (exact `if`s):

| step | condition | action |
|---|---|---|
| shaders | `!headless` && `packPath + "Shaders/"` is a folder && `!levelsOnly` | `loadShadersOfType(".vert", Vertex)`, `(".geom", Geometry)`, `(".frag", Fragment)` over `packPath + "Shaders/"` (single-level scan by ext); each: `shadersById.push_back`, `id = size-1`, `shaders.emplace(packId + "_" + fileName, ...)` (fileName INCLUDES extension, e.g. `shaders.emplace("cube_foo.frag")`), `shadersPathToId.emplace(fullPath, id)`, `++loadInfo.assets`; load failure → log and continue |
| custom sounds | `!headless` && `!levelsOnly` && `packPath + "Sounds/"` folder | all `.ogg` files: `assetStorage->loadSoundBuffer(packId + "_" + fileName, path)` (fileName WITH `.ogg`), `++loadInfo.assets` |
| music files | `packPath + "Music/"` folder && `!levelsOnly` && `!headless` | all `.ogg`: `musicPathMap.emplace(packId + "_" + fileNameNoExtensions, fullPath)`, `++loadInfo.assets` |
| music data | same folder && `!levelsOnly` | all `.json`: `loadMusicFromJson`, `musicDataMap.emplace(packId + "_" + musicData.id)`, `++loadInfo.assets` (parse error → `loadInfo.addFormattedError`) |
| styles | `packPath + "Styles/"` folder | all `.json`: `StyleData{object}`, `styleDataMap.emplace(packId + "_" + styleData.id)`, `++loadInfo.assets` |
| levels | `packPath + "Levels/"` folder | all `.json`: `LevelData{object, packPathString, packId}`; `assetId = packId + "_" + levelData.id`; `levelDataIdsByPack[packId].emplace_back(assetId)`; `levelDatas.emplace(assetId, levelData)`; `++loadInfo.levels` |

Whole section wrapped in `try { ... } catch (std::runtime_error&) {
loadInfo.errorMessages.emplace_back("FATAL ERROR, Exception during asset
loading: " + ex.what()); return false; } catch (...) { ... "unknown." ... }`
— this is the ONLY exception handler in the JSON path (§1.4).

Afterwards: `if (packHasLevels(packId)) selectablePackInfos.emplace_back(PackInfo{packId, packPath});`

All scans are `Mode::Single` (non-recursive), `Type::File`, `Pick::ByExt`,
default `Sort::Alphabetic` (byte-wise sort of full path strings), and the
extension match `endsWith(name, ".json")` is **case-sensitive**
(`ssvu::endsWith`), unlike `Path::hasExtension` which is not.

### 4.5 `verifyAllPackDependencies`

For each pack, each dependency: satisfied iff some loaded pack has
`disambiguator == pd.disambiguator && name == pd.name && author == pd.author &&
version >= pd.minVersion`. If not: log
`"Missing pack dependency '<pd.name>' for pack '<packData.name>'\n"` into
`loadInfo.errorMessages` and `packIdsWithMissingDependencies.emplace(packId)`.
Then `selectablePackInfos` is filtered to remove packs in that set. Always
returns true (missing deps never abort the game).

### 4.6 `assets.json` (`Assets/assets.json`)

Loaded in the constructor only when `!levelsOnly && !mHeadless`:

- `"Assets/"` must be a folder, else `std::terminate()` with
  `"Folder Assets/ does not exist"`.
- `ssvuj::getFromFileWithErrors("Assets/assets.json")` →
  `loadAssetsFromJson(assetStorage, "Assets/", object)`; parse error appended
  via `addFormattedError`.
- `loadAssetsFromJson` reads three keys (all `vector<string>`, **no defaults**
  → missing key = empty list):
  - `"fonts"` → for each `f`: `assetStorage.loadFont(f, "Assets/" + f)`;
    failure logs `Failed to load font '<f>'`.
  - `"textures"` → `loadTexture(f, "Assets/" + f)`; failure logs
    `Failed to load texture '<f>'`.
  - `"soundBuffers"` → `loadSoundBuffer(f, "Assets/" + f)`; failure logs
    `Failed to load sound buffer '<f>'`.
- Asset ids for these = the bare file name (e.g. `"icon"`, `"beep"`).
- Shipped `assets.json` contents: fonts `OpenSquare-Regular.ttf`,
  `OpenSquare-Bold.ttf`; 16 textures (bottomBar...titleBar); 17 soundBuffers
  (beep...swapBlip). See §9.

### 4.7 Profiles (`Profiles/*.json`)

`loadAllLocalProfiles`:

- `"Profiles/"` must be a folder, else returns false (logged, NOT fatal).
- Each `.json` file (single-level scan by ext `.json`, alphabetic):
  `getFromFileWithErrors` → error via `addFormattedError` →
  `ProfileData{loadProfileFromJson(object)}` → `addLocalProfile(move)`.
- `addLocalProfile`: **removes favorite ids that are not valid level ids**
  (`erase_if` where `levelDatas.find(favId) == levelDatas.end()` — note the
  favorite id is the full asset id `<packId>_<levelId>`), then
  `profileDataMap.emplace(name, profile)`.

`saveCurrentLocalProfile` / `saveAllProfiles` — exact write format
(`ssvuj::arch` + `writeToFile`, StyledStreamWriter, tab-indented):

```
{
    "version"   : { "major" : M, "minor" : m, "micro" : u },   // M,m,u = GAME_VERSION (2,1,7)
    "name"      : "<profile name>",
    "scores"    : { "<levelAssetId>" : <float>, ... },
    "favorites" : [ "<levelAssetId>", ... ]
}
```

written to `"Profiles/" + name + ".json"`. (Object key order in the output is
the jsoncpp map order — sorted by key bytes.)

`pCreate(name)`: writes a stub `{"name": n, "scores": {}, "favorites": {}}`
(note: favorites is archived as an **empty object**, not array), clears
`profileDataMap`, re-runs `loadAllLocalProfiles` (failure →
`std::terminate()`).
Other profile ops: `pSetCurrent` (asserts existence; sets
`currentProfilePtr`), `pRemove` (erase from map only — file NOT deleted),
`getLocalProfileNames` (insertion order of the unordered_map — arbitrary),
`getCurrentLocalProfileFilePath` = `"Profiles/" + name + ".json"`.

### 4.8 Reload paths (dev tools)

- `reloadPack(packId, mPath)` (mPath = pack folder path WITHOUT trailing
  slash in callers? It is appended: `mPath + "Levels/"` etc. — callers pass
  e.g. `"Packs/<folder>"`): reloads Levels (upserts into
  `levelDatas`/`levelDataIdsByPack`), Styles, Music data, Music .ogg files
  (`musicPathMap.emplace` — note: does NOT replace existing entries), Sounds
  (new buffers only). Returns a multi-line status string, e.g.
  `"Levels successfully reloaded\ninvalid style folder path\n..."`.
- `reloadLevel(packId, mPath, mId)`: loads `Levels/<mId>.json`
  (by exact file name, case-sensitive), then `Styles/<styleId>.json`,
  `Music/<musicId>.json`, `Music/<musicId>.ogg` (skipped when
  `musicId == "nullMusicId"`), `Sounds/<soundId>.ogg` (skipped when
  `soundId == "nullSoundId"`; skipped when the buffer already exists with
  note `"custom sound file <id>.ogg is already loaded"`). Status strings
  verbatim in source, e.g. `"no matching level data file found\n"`.
- `reloadAllShaders`: re-`loadFromFile` each stored shader path.

### 4.9 Lookups (fail-soft semantics)

- `getMusicData(packId, id)` / `getStyleData(packId, id)`: missing asset →
  log `Asset '<packId>_<id>' not found`, **assert the map is non-empty and
  return the first (map-ordered) entry** — i.e. graceful fallback, not null.
- `getShader`/`getShaderId`: missing → log + `nullptr`/`nullopt`.
- `getLevelData(assetId)`: asserts validity, `at()`.
- `findPackData(disambiguator, name, author)`: linear search over
  `packDatas` by the three fields (first match in unordered order).
- `getMusicPath(assetId)`: map find → `const string*` or nullptr.
- `getLocalScore/setLocalScore` delegate to the current profile (asserts a
  profile is active).

---

## §5. `Global/AssetStorage` (`AssetStorage.hpp` / `.cpp`)

Pimpl wrapper over three maps:

```cpp
std::unordered_map<std::string, sf::Texture>      _textures;
std::unordered_map<std::string, sf::Font>         _fonts;
std::unordered_map<std::string, sf::SoundBuffer>  _soundBuffers;
```

- `loadTexture(id, path)` / `loadFont(id, path)` / `loadSoundBuffer(id, path)`:
  `tryEmplaceAndThenLoadFromFile` = `map.emplace(piecewise_construct, {id}, {})`
  and, **only if the insert happened**, `value.loadFromFile(path)` (SFML).
  Returns the SFML load result. Consequences:
  - a failed load leaves the empty entry in the map (id → null SFML object);
  - a second load of the same id is a no-op (first load wins, even if it
    failed).
- `getTexture/getFont/getSoundBuffer(id)` → `sf::X*` or nullptr.
- `hasTexture/hasFont/hasSoundBuffer(id)` → `map.find != end` (true even for
  the failed-load empty entry).

No JSON involved — ids come from `assets.json` (§4.6) and pack sound scans
(§4.4).

---

## §6. `Global/Config` (`Config.hpp` / `Config.cpp`)

### 6.1 Storage model

Every config value is a function-local static `ssvuj::LinkedValue<T>` created
by the `X` macro:

```cpp
#define X(name, type, key, ...) \
    static auto& name() noexcept { \
        static auto res = ::ssvuj::LinkedValue<type>(key, type{__VA_ARGS__}); \
        return res; \
    }
```

So each value: lives in a C++ static, is named by its JSON key, and has a
compile-time default. `root()` is a static `ssvuj::Obj` loaded once from
`"config.json"` in the CWD if present (log: `"User-defined `config.json` file
found\n"` else `"No suitable config file found, using defaults\n"`).

- `loadConfig(overrideIds)`: if `"ConfigOverrides/"` is a folder, for each
  `.json` file (single-level, by ext, alphabetic) whose
  `getFileNameNoExtensions()` is in `overrideIds` (log
  `"applying config override '<name>'\n"`): shallow-merge its members into
  `root()` (`root()[key] = value` per top-level key). Then
  `syncAllFromObj()` (every LinkedValue `syncFrom(root())`) and
  `fixupMissingTriggers()` (any trigger with an empty combos vector is
  resized to 4 unbound combos).
- `saveConfig()`: `syncAllToObj()` (every LinkedValue `syncTo(root())`) then
  `ssvuj::writeToFile(root(), "config.json")`.
- `resetConfigToDefaults()`: `resetAllFromDefault()` + `reapplyResolution()`.
- `resetBindsToDefaults()`: reset only the 28 bind LinkedValues.
- Unknown keys in config.json are kept in `root()` and re-saved (the root
  object is the source of truth, not the X table).

Other module-level state: `float sizeX{1500}, sizeY{1500};`,
`constexpr float defaultSpawnDistance{1600};` (`getSpawnDistance()` returns
this constant — **there is no config key for spawn distance**),
`std::string uneligibilityReason;`.

### 6.2 COMPLETE config-key table

Every key in `Config.cpp` (the `X_LINKEDVALUES` + `X_LINKEDVALUES_BINDS`
macros; verified against a grep of all string literals in the file).
"Official-locked" = the getter returns the default while
`getOfficial()` is true (and `isEligibleForScore` compares against it).

Non-bind keys (73):

| # | JSON key | C++ type | default | LinkedValue name | getter | setter | official-locked |
|---|---|---|---|---|---|---|---|
| 1 | `official` | bool | `true` | official | getOfficial | setOfficial | — |
| 2 | `no_pulse` | bool | `false` | noPulse | getNoPulse | setNoPulse | ✔ |
| 3 | `no_rotation` | bool | `false` | noRotation | getNoRotation | setNoRotation | ✔ |
| 4 | `no_background` | bool | `false` | noBackground | getNoBackground | setNoBackground | ✔ |
| 5 | `no_sound` | bool | `false` | noSound | getNoSound | setNoSound | |
| 6 | `no_music` | bool | `false` | noMusic | getNoMusic | setNoMusic | |
| 7 | `black_and_white` | bool | `false` | blackAndWhite | getBlackAndWhite | setBlackAndWhite | ✔ |
| 8 | `pulse_enabled` | bool | `true` | pulseEnabled | **none** (persisted only) | none | |
| 9 | `3D_enabled` | bool | `true` | _3DEnabled | get3D | set3D | |
| 10 | `shaders_enabled` | bool | `true` | shadersEnabled | getShaders | setShaders | |
| 11 | `3D_multiplier` | float | `1.f` | _3DMultiplier | get3DMultiplier | none | |
| 12 | `3D_max_depth` | uint | `100` | _3DMaxDepth | get3DMaxDepth | none | |
| 13 | `invincible` | bool | `false` | invincible | getInvincible | setInvincible | ✔ |
| 14 | `auto_restart` | bool | `false` | autoRestart | getAutoRestart | setAutoRestart | |
| 15 | `sound_volume` | float | `100.f` | soundVolume | getSoundVolume | setSoundVolume | |
| 16 | `music_volume` | float | `100.f` | musicVolume | getMusicVolume | setMusicVolume | |
| 17 | `flash_enabled` | bool | `true` | flashEnabled | getFlash | setFlash | |
| 18 | `zoom_factor` | float | `1.27f` | zoomFactor | getZoomFactor | (set via recalculateSizes / auto) | |
| 19 | `pixel_multiplier` | int | `1` | pixelMultiplier | getPixelMultiplier | none | |
| 20 | `player_speed` | float | `9.45f` | playerSpeed | getPlayerSpeed | none | ✔ |
| 21 | `player_focus_speed` | float | `4.625f` | playerFocusSpeed | getPlayerFocusSpeed | none | ✔ |
| 22 | `player_size` | float | `7.3f` | playerSize | getPlayerSize | none | ✔ |
| 23 | `limit_fps` | bool | `true` | limitFPS | getLimitFPS | setLimitFPS | |
| 24 | `vsync` | bool | `false` | vsync | getVsync | setVsync | |
| 25 | `auto_zoom_factor` | bool | `true` | autoZoomFactor | getAutoZoomFactor | none (auto) | ✔ |
| 26 | `fullscreen` | bool | `false` | fullscreen | getFullscreen | setFullscreen | |
| 27 | `windowed_auto_resolution` | bool | `false` | windowedAutoResolution | getWindowedAutoResolution | (via setCurrentResolution/Auto) | |
| 28 | `fullscreen_auto_resolution` | bool | `false` | fullscreenAutoResolution | getFullscreenAutoResolution | (via setCurrentResolution/Auto) | |
| 29 | `fullscreen_width` | uint | `1920` | fullscreenWidth | getFullscreenWidth | (via setCurrentResolution/Auto) | |
| 30 | `fullscreen_height` | uint | `1080` | fullscreenHeight | getFullscreenHeight | (via setCurrentResolution/Auto) | |
| 31 | `windowed_width` | uint | `800` | windowedWidth | getWindowedWidth | (via setCurrentResolution/Auto) | |
| 32 | `windowed_height` | uint | `600` | windowedHeight | getWindowedHeight | (via setCurrentResolution/Auto) | |
| 33 | `show_messages` | bool | `true` | showMessages | getShowMessages | none | |
| 34 | `debug` | bool | `false` | debug | getDebug | setDebug | ✔ |
| 35 | `beatpulse_enabled` | bool | `true` | beatPulse | getBeatPulse | none | ✔ |
| 36 | `show_tracked_variables` | bool | `true` | showTrackedVariables | getShowTrackedVariables | none | |
| 37 | `music_speed_dm_sync` | bool | `true` | musicSpeedDMSync | getMusicSpeedDMSync | setMusicSpeedDMSync | |
| 38 | `max_fps` | uint | `200` | maxFPS | getMaxFPS | setMaxFPS | |
| 39 | `antialiasing_level` | uint | `4` | antialiasingLevel | getAntialiasingLevel | setAntialiasingLevel | |
| 40 | `show_fps` | bool | `false` | showFPS | getShowFPS | setShowFPS | |
| 41 | `music_speed_mult` | float | `1.0f` | musicSpeedMult | getMusicSpeedMult | setMusicSpeedMult | |
| 42 | `draw_text_outlines` | bool | `true` | drawTextOutlines | getDrawTextOutlines | setDrawTextOutlines | |
| 43 | `darken_uneven_background_chunk` | bool | `true` | darkenUnevenBackgroundChunk | getDarkenUnevenBackgroundChunk | setDarkenUnevenBackgroundChunk | |
| 44 | `rotate_to_start` | bool | `false` | rotateToStart | getRotateToStart | setRotateToStart | |
| 45 | `joystick_deadzone` | float | `5.0f` | joystickDeadzone | getJoystickDeadzone | setJoystickDeadzone | |
| 46 | `text_padding` | float | `8.0f` | textPadding | getTextPadding | setTextPadding | |
| 47 | `text_scaling` | float | `1.0f` | textScaling | getTextScaling | setTextScaling | |
| 48 | `timescale` | float | `1.0f` | timescale | getTimescale | setTimescale | ✔ |
| 49 | `show_key_icons` | bool | `false` | showKeyIcons | getShowKeyIcons | setShowKeyIcons | |
| 50 | `key_icons_scale` | float | `0.75f` | keyIconsScale | getKeyIconsScale | setKeyIconsScale | |
| 51 | `first_time_playing` | bool | `true` | firstTimePlaying | getFirstTimePlaying | setFirstTimePlaying | |
| 52 | `show_level_info` | bool | `false` | showLevelInfo | getShowLevelInfo | setShowLevelInfo | |
| 53 | `show_timer` | bool | `true` | showTimer | getShowTimer | setShowTimer | ✔ |
| 54 | `show_status_text` | bool | `true` | showStatusText | getShowStatusText | setShowStatusText | ✔ |
| 55 | `server_ip` | string | `"139.162.199.162"` | serverIp | getServerIp | setServerIp | |
| 56 | `server_port` | ushort | `50505` | serverPort | getServerPort | setServerPort | |
| 57 | `server_control_port` | ushort | `50506` | serverControlPort | getServerControlPort | setServerControlPort | |
| 58 | `server_level_whitelist` | `vector<string>` | `defaultServerLevelWhitelist()` (174 validator ids, §6.5) | serverLevelWhitelist | getServerLevelWhitelist | setServerLevelWhitelist | |
| 59 | `save_last_login_username` | bool | `true` | saveLastLoginUsername | getSaveLastLoginUsername | setSaveLastLoginUsername | |
| 60 | `last_login_username` | string | `""` | lastLoginUsername | getLastLoginUsername | setLastLoginUsername | |
| 61 | `show_login_at_startup` | bool | `false` | showLoginAtStartup | getShowLoginAtStartup | setShowLoginAtStartup | |
| 62 | `camera_shake_multiplier` | float | `1.f` | cameraShakeMultiplier | getCameraShakeMultiplier | setCameraShakeMultiplier | |
| 63 | `angle_tilt_intensity` | float | `1.f` | angleTiltIntensity | getAngleTiltIntensity | setAngleTiltIntensity | |
| 64 | `show_player_trail` | bool | `true` | showPlayerTrail | getShowPlayerTrail | setShowPlayerTrail | |
| 65 | `player_trail_alpha` | uint | `35` | playerTrailAlpha | getPlayerTrailAlpha | setPlayerTrailAlpha | |
| 66 | `player_trail_scale` | float | `0.9f` | playerTrailScale | getPlayerTrailScale | setPlayerTrailScale | |
| 67 | `player_trail_decay` | float | `3.0f` | playerTrailDecay | getPlayerTrailDecay | setPlayerTrailDecay | |
| 68 | `player_trail_has_swap_color` | bool | `true` | playerTrailHasSwapColor | getPlayerTrailHasSwapColor | setPlayerTrailHasSwapColor | |
| 69 | `show_swap_particles` | bool | `true` | showSwapParticles | getShowSwapParticles | setShowSwapParticles | |
| 70 | `play_swap_ready_sound` | bool | `true` | playSwapReadySound | getPlaySwapReadySound | setPlaySwapReadySound | |
| 71 | `show_swap_blinking_effect` | bool | `true` | showSwapBlinkingEffect | getShowSwapBlinkingEffect | setShowSwapBlinkingEffect | |
| 72 | `use_lua_file_cache` | bool | `false` | useLuaFileCache | getUseLuaFileCache | setUseLuaFileCache | |
| 73 | `disable_game_rendering` | bool | `false` | disableGameRendering | getDisableGameRendering | setDisableGameRendering | |

Joystick binds (12) — type `uint` = SFML joystick button index on
controller 0; **`33` is the "unbound" sentinel** (used by
`JoystickBindControl::erase`):

| # | JSON key | type | default | LinkedValue | Jid index |
|---|---|---|---|---|---|
| 74 | `j_select` | uint | `1` | joystickSelect | Select(0) |
| 75 | `j_exit` | uint | `2` | joystickExit | Exit(1) |
| 76 | `j_focus` | uint | `4` | joystickFocus | Focus(2) |
| 77 | `j_swap` | uint | `5` | joystickSwap | Swap(3) |
| 78 | `j_force_restart` | uint | `3` | joystickForceRestart | ForceRestart(4) |
| 79 | `j_restart` | uint | `0` | joystickRestart | Restart(5) |
| 80 | `j_replay` | uint | `6` | joystickReplay | Replay(6) |
| 81 | `j_screenshot` | uint | `7` | joystickScreenshot | Screenshot(7) |
| 82 | `j_next` | uint | `11` | joystickNextPack | NextPack(8) |
| 83 | `j_previous` | uint | `10` | joystickPreviousPack | PreviousPack(9) |
| 84 | `j_add_favorite` | uint | `8` | joystickAddToFavorites | AddToFavorites(10) |
| 85 | `j_favorite_menu` | uint | `9` | joystickFavoritesMenu | FavoritesMenu(11) |

Keyboard/mouse triggers (16) — type `ssvs::Input::Trigger` = array of combos,
each combo = array of input-name strings (§1.8). Defaults (one input per
combo; `k` = key name, `b` = button name):

| # | JSON key | default combos | Tid index |
|---|---|---|---|
| 86 | `t_rotate_ccw` | `[[kA],[kLeft],[bLeft]]` | RotateCCW(0) |
| 87 | `t_rotate_cw` | `[[kD],[kRight],[bRight]]` | RotateCW(1) |
| 88 | `t_focus` | `[[kLShift],[bXButton1]]` | Focus(2) |
| 89 | `t_select` | `[[kSpace],[bMiddle]]` | Select(3) |
| 90 | `t_exit` | `[[kT],[bXButton2]]` | Exit(4) |
| 91 | `t_force_restart` | `[[kUp],[kR]]` | ForceRestart(5) |
| 92 | `t_restart` | `[[kSpace],[kEnter],[bMiddle]]` | Restart(6) |
| 93 | `t_replay` | `[[kY]]` | Replay(7) |
| 94 | `t_screenshot` | `[[kF12]]` | Screenshot(8) |
| 95 | `t_swap` | `[[kSpace],[bMiddle]]` | Swap(9) |
| 96 | `t_up` | `[[kW]]` | Up(10) |
| 97 | `t_down` | `[[kS]]` | Down(11) |
| 98 | `t_next` | `[[kPageDown]]` | NextPack(12) |
| 99 | `t_previous` | `[[kPageUp]]` | PreviousPack(13) |
| 100 | `t_lua_console` | `[[kF1]]` | LuaConsole(14) |
| 101 | `t_pause` | `[[kF2]]` | Pause(15) |

`Tid` enum (Config.hpp): `Unknown=-1, RotateCCW=0, RotateCW, Focus, Select,
Exit, ForceRestart, Restart, Replay, Screenshot, Swap, Up, Down, NextPack,
PreviousPack, LuaConsole, Pause, TriggersCount`.

### 6.3 Serialization (write) key names and formats

`saveConfig()` writes `root()` to `"config.json"` with `StyledStreamWriter`
(default indent `"\t"`), i.e. the shipped `config.json` layout:
one `"key" : value` per line, top-level keys in **jsoncpp map (byte-sorted)
order**, floats via `%#.16g` (hence `7.300000190734863`), trigger arrays as
nested arrays of strings:

```
"t_exit" :
[
    [ "kT" ],
    [ "bXButton2" ],
    [ "" ],
    [ "" ]
],
```

An unbound combo slot serializes as `[""]` (empty string at index 0, §1.8);
a bound slot as `["<kName|bName>"]`. Combo slots are serialized keys-first,
buttons-after. Note the shipped file's inner arrays have exactly one element
each; padding to 4 slots only happens via `resizeTrigger` (dev/binding
paths), and `fixupMissingTriggers` pads empty triggers to 4 `[""]` slots
in-memory before the next save.

### 6.4 Bind helpers (behavior, not JSON)

- `maxBinds = 4`; `resizeTrigger`: drop unbound combos (agglomerate), pop
  from the back while `> 4`, pad with `Combo({KKey::Unknown})` (unbound) while
  `< 4`.
- `keyboardBindsSanityCheck()`: `resizeTrigger` on the first 12 triggers
  (RotateCCW..Down) — NOT on NextPack/PreviousPack/LuaConsole/Pause.
- `getKeyboardBindNames(Tid)`: for combos up to the first unbound: first set
  key bit (loop `j` in `0..=KeyCount`, name = `KKey(j-1)`) else first set
  button bit (`MBtn(j-1)`); joined by `", "`; each name passed through
  `bindToHumanReadableName` (strip leading `"k"`; `bLeft`→`LMB`,
  `bRight`→`RMB`, `bMiddle`→`MMB`); the whole result is upper-cased.
- `rebindTrigger(trig, key, btn, index)`: `index >= 4` → index 0 + clear that
  slot; `key > -1` → `addKey(KKey(key))` else `addBtn(MBtn(btn))`.
- `clearTriggerBind(trig, index)`: `combos[index].clearBind()`.
- `getJoystickBindName(Jid)`: static 12×2 table
  `{{"A","SQUARE"},{"B","CROSS"},{"X","CIRCLE"},{"Y","TRIANGLE"},
  {"LB","L1"},{"RB","R1"},{"BACK","L2"},{"START","R2"},
  {"LEFT STICK","SELECT"},{"RIGHT STICK","START"},{"LT","LEFT STICK"},
  {"RT","RIGHT STICK"}}`; value `33` → `""`; vendor `0x045E` (MS) → col 0,
  `0x54C` (Sony) → col 1 (both: value `>= 12` → `""`); other/absent →
  `toStr(value)`.
- `loadAllJoystickBinds()`: for `i in 0..12`:
  `Joystick::setJoystickBind(joystickTriggerGetters[i](), i)`.
- `triggerGetters` / `joystickTriggerGetters` / `joystickTriggerSetters`
  are `std::array` of function pointers indexed by the Tid/Jid values
  (1-based mapping as above).

### 6.5 Default server level whitelist

`defaultServerLevelWhitelist()` — static `vector<string>` of 174 level
validator ids (`<packId>_<levelId>_m_<mult>`, §7.1) covering the bundled
vanilla packs (cube/orthoplex/hypercube, disambiguator `ohvrvanilla`,
author `vittorio romeo`, version 1) plus Workshop packs (Vanity
`vanitylevels_Vipre_Vanity_10006_*`, Rotationality
`rotationality_SkyMidnight_Rotationality_(Remastered)_2_*`, Travel
`1D2B-E31E-EC19_Syyrion_Travel_13103_*`, Delta
`ndeltalevels_NDigger_delta_1_*`, OHL2/3/4
`ohlNpack_Hunter_&_..._1_*`). It is the *default value* of the
`server_level_whitelist` key; the full list is reproduced verbatim in the
shipped `_RELEASE/config.json` (§9) and in `Config.cpp` lines 22–230.

### 6.6 Score eligibility (`isEligibleForScore` + `getUneligibilityReason`)

Fails (in this order) with the exact reasons:
`"official mode off"`, `"spawn distance modified"` (spawn distance is the
constant 1600, so this branch is effectively dead), `"debug mode on"`,
`"modified zoom factor"`, `"player speed modified"`,
`"player focus speed modified"`, `"player size modified"`,
`"invincibility on"`, `"pulse off"`, `"rotation off"`; otherwise true.

### 6.7 Derived geometry

`recalculateSizes()`: `sizeX = sizeY = max(getWidth(), getHeight()) * 1.3f`;
if `getAutoZoomFactor()`: `zoomFactor = max(1024.f / width, 768.f / height)`.
`getWidth/Height` = fullscreen ? fullscreenWidth/Height :
windowedWidth/Height. `reapplyResolution()`: if windowed_auto_resolution →
set windowed sizes to desktop mode; if fullscreen_auto_resolution → set
fullscreen sizes to desktop mode; then `recalculateSizes()`.

---

## §7. `Utils/LevelValidator` + `Utils/BuildPackId`

### 7.1 Level validator id

```cpp
std::string getLevelValidator(const std::string& levelId, const float diffMult)
{
    return Utils::concat(levelId, "_m_", diffMult);
}
```

Format: `<levelId>_m_<diffMult>` where `<diffMult>` is the **default
`std::ostream` float formatting** (`Utils::concat` uses a
`std::ostringstream` for non-string arguments → 6 significant digits,
`%g`-like): `1.0f` → `"1"`, `0.5f` → `"0.5"`, `1.8f` → `"1.8"`,
`0.35f` → `"0.35"`, `1.001f` → `"1.001"`, `6.66f` → `"6.66"`.

So a level asset is uniquely addressed as
`<packId>_<levelId>_m_<mult>` — e.g.
`ohvrvanilla_vittorio_romeo_cube_1_pi_m_0.5`. This string is what appears in:
`LevelData::validators` / `validatorsWithoutPackId` (per diffMult),
`config.json` `server_level_whitelist`, and score/profile ids.

NOTE for the port: float formatting MUST match C++ iostream default
(`%.6g` semantics) or validator ids will mismatch between C++ and Rust
builds (e.g. `0.1f` → `"0.1"`, `123.456f` → `"123.456"`, `0.00001f` →
`"1e-05"`).

### 7.2 Pack id

```cpp
std::string buildPackId(const std::string& packDisambiguator,
    const std::string& packAuthor, const std::string& packName,
    const int packVersion);
```

- asserts (debug): disambiguator/author/name non-empty, `packVersion > 0`.
- For each of disambiguator, author, name: every `' '`, `'\n'`, `'\t'`
  character is replaced with `'_'` (function `spaceToUnderscore`).
- Result: `concat(disambiguator, '_', author, '_', name, '_', version)` —
  version is an int (no float formatting). Example:
  `"ohvrvanilla" + "_" + "vittorio_romeo" + "_" + "cube" + "_" + "1"` =
  `ohvrvanilla_vittorio_romeo_cube_1`.

---

## §8. `Core/Joystick` + `Core/BindControl`

### 8.1 `Joystick` (`Joystick.hpp` / `.cpp`)

Enums:

```cpp
enum class Jdir : int { Unknown=-1, Left, Right, Up, Down, JoystickDirectionsCount };
enum class Jid : int  { Unknown=-1, Select=0, Exit, Focus, Swap, ForceRestart,
                        Restart, Replay, Screenshot, NextPack, PreviousPack,
                        AddToFavorites, FavoritesMenu, JoystickBindsCount };
```

`JoystickState` (function-local static, value-initialized):

```cpp
bool ignoreAllPresses;
std::array<bool, 4> dirWasPressed;   // Jdir count
std::array<bool, 4> dirPressed;
std::array<bool, 12> wasPressed;     // Jid count
std::array<bool, 12> pressed;
unsigned int joystickInputs[12];     // Jid -> SFML button index (from config, §6.2 rows 74-85)
```

- `setJoystickBind(unsigned button, int buttonID)`:
  `joystickInputs[buttonID] = button` (buttonID = Jid value 0..11; fed by
  `Config::loadAllJoystickBinds`).
- `ignoreAllPresses(bool)`: sets all `*WasPressed` arrays and
  `ignoreAllPresses` to the flag.
- `update(float deadzone)` (called every frame with
  `Config::getJoystickDeadzone()`):
  - early return while `ignoreAllPresses`.
  - controller id is hardcoded `joyId = 0`.
  - `axisPressed(deadzone, 0, axis)`: SFML `getAxisPosition`; `< -deadzone`
    → Left, `> deadzone` → Right, else Dead.
  - Dirs (dpad `PovX/PovY` OR left stick `X/Y`; note Y is inverted):
    Left = xIs(Left); Right = xIs(Right); Up = yIs(Right); Down = yIs(Left).
    `doDir` latches: `dirWasPressed = exchange(dirPressed, check)`.
  - Buttons: for each Jid in order Select, Exit, Focus, Swap, ForceRestart,
    Restart, Replay, Screenshot, NextPack, PreviousPack, AddToFavorites,
    FavoritesMenu: `wasPressed = exchange(pressed,
    sf::Joystick::isButtonPressed(0, joystickInputs[jid]))`.
  - XBOX 360 mapping comment in source: buttons A0 B1 X2 Y3 LB4 RB5 back6
    start7 L8 R9; triggers LT +Z, RT -Z; dpad left -povX, right +povX, up
    +povY, down -povY.
- `pressed(Jdir|Jid)`, `risingEdge(Jdir|Jid)` (= pressed && !wasPressed).

JSON relevance: the 12 `j_*` config keys (§6.2) initialize
`joystickInputs`; `joystick_deadzone` drives `update`.

### 8.2 `BindControl` (`BindControl.hpp` / `.cpp`)

`BindControlBase : ssvms::ItemBase` — menu item with `waitingForBind` + `ID`.

`KeyboardBindControl` (template ctor over get/set/clear/callback fns):

- Members: `triggerGetter` (returns `Trigger&`), `sizeGetter` =
  `getRealSize(triggerGetter().getCombos())`, `addBind` =
  `mFuncSet(key, btn, sizeGetter())`, `clearBind` = `mFuncClear(sizeGetter()-1)`,
  `callback`, `hardcodedKey` (default `KKey::Unknown`).
- Ctor sanitization: if the user manually put `hardcodedKey` into any combo,
  `mFuncClear(i)` for that slot.
- `getRealSize(combos)`: count of leading non-unbound combos.
- `exec()`: toggles `waitingForBind`.
- `erase()`: if size==0 → false; else `clearBind()` (clears the LAST active
  slot) + `callback(trigger, ID)` → true.
- `newKeyboardBind(KKey)`: reject if `== hardcodedKey` (stop waiting);
  stop if key already in any active combo (returns true, no change);
  else `applyBind(key, MBtn::Left)` → `addBind` (config write at slot
  `sizeGetter()`), `callback`, stop waiting.
- `newKeyboardBind(MBtn)`: same, `applyBind(KKey::Unknown, btn)`.
- `getName()`: `name + ": " + Config::getKeyboardBindNames(Tid(ID))`
  (plus trailing `"_"` while waiting for a bind).

`JoystickBindControl`:

- `valueGetter`/`setButton`/`callback` fns; `ID` = Jid.
- `erase()`: if `valueGetter() == 33` → false; else `setButton(33)` +
  `callback(33, ID)` (33 = unbound sentinel, §6.2).
- `newJoystickBind(unsigned joy)`: no-op+stop if `joy == valueGetter()`;
  else `setButton(joy)` + `callback(joy, ID)`.
- `getName()`: `name + ": " + Config::getJoystickBindName(Jid(ID))` (+ `"_"`
  while waiting).

---

## §9. On-disk layout under `_RELEASE/`

(Verified against the actual tree; relative to the process CWD.)

```
_RELEASE/
├── config.json                     # §6 (saved by saveConfig; user-editable)
├── scores.json / users.json        # server-side (OHServerControl), not loaded by the game
├── workshopCache.json              # (optional) {"cachedPacks": ["<path>", ...]} — §4.3
├── Assets/
│   ├── assets.json                 # {"fonts":[...], "textures":[...], "soundBuffers":[...]} — §4.6
│   ├── menubackgrounds.json        # read by MenuGame (MenuGame.cpp:1794ff): list of background names
│   ├── OpenSquare-Regular.ttf / OpenSquare-Bold.ttf
│   ├── *.png, *.ogg                # the assets named by assets.json
├── ConfigOverrides/                # §6.1: *.json shallow-merged into config root when the
│   ├── debug.json                  #   (extension-less) file name is in the override id list
│   ├── fullscreen.json, highfps.json, noaudio.json, no3D.json,
│   ├── windowed.json, fraps.json, localserver.json, remoteserver.json,
│   └── nocosmeticscript.json
├── Packs/<packFolder>/             # one folder per pack (local + workshop)
│   ├── pack.json                   # §4.2: disambiguator, name, author, version, priority,
│   │                               #   description, dependencies[{disambiguator,name,author,min_version}]
│   ├── Levels/*.json               # §2.3 level data (keys: id,name,description,author,
│   │                               #   menuPriority,selectable,musicId,soundId,styleId,luaFile,
│   │                               #   difficultyMults,unscored)
│   ├── Styles/*.json               # §2.9 style data
│   ├── Music/
│   │   ├── *.json                  # §3.1 music data (id,file_name,name,album,author,
│   │   │                           #   segments[{time,beat_pulse_delay_offset}])
│   │   └── *.ogg                   # audio files (keyed by file basename w/o extension)
│   ├── Sounds/*.ogg                # custom sound buffers (keyed by full file name)
│   ├── Shaders/*.vert|.geom|.frag  # GLSL (keyed by full file name incl. extension)
│   └── Scripts/**/*.lua            # Lua; located via level "luaFile" (packPath + luaFile)
└── Profiles/*.json                 # §2.8/§4.7 (created at runtime; absent in the release tree)
```

Shipped packs (8): `artwork` (Levels/Music/Scripts/Shaders/Styles, pack
priority default 100? — no: artwork pack.json priority omitted → 100),
`base` (Scripts only; priority `-99999.04`), `cube` (priority
`-99999.02`, depends on base), `experimental`, `hypercube`, `orthoplex`,
`tutorial`, `workshopexample`. Subfolder presence is optional per pack —
each is guarded by an `isFolder()` check (§4.4); the base pack has no
Levels/, so it is not "selectable".

Which fields each loader reads:

| file | loader | JSON keys read |
|---|---|---|
| `Assets/assets.json` | `loadAssetsFromJson` | `fonts`, `textures`, `soundBuffers` (arrays of file names, no defaults) |
| `Packs/*/pack.json` | `loadPackData` | `disambiguator`("no disambiguator"), `name`("unknown name"), `author`("unknown author"), `description`("no description"), `version`(0), `priority`(100), `dependencies[]`(`disambiguator`,`name`,`author`,`min_version`) |
| `Packs/*/Levels/*.json` | `LevelData(mRoot, packPath, packId)` | `id`("nullId"), `name`("nullName"), `description`(""), `author`(""), `menuPriority`(0), `selectable`(true), `musicId`("nullMusicId"), `soundId`("nullSoundId"), `styleId`("nullStyleId"), `luaFile`("nullLuaPath", prefixed with packPath), `difficultyMults`({}, +1.0, sorted), `unscored`(false) |
| `Packs/*/Styles/*.json` | `StyleData(mRoot)` | `id`("nullId"), `hue_min`(0), `hue_max`(360), `hue_increment`(0), `hue_ping_pong`(false), `pulse_min`(0), `pulse_max`(0), `pulse_increment`(0), `max_swap_time`(100), `3D_depth`(15), `3D_skew`(0.18), `3D_spacing`(1), `3D_darken_multiplier`(1.5), `3D_alpha_multiplier`(0.5), `3D_alpha_falloff`(3), `3D_pulse_max`(3.2), `3D_pulse_min`(0), `3D_pulse_speed`(0.01), `3D_perspective_multiplier`(1), `3D_override_color`(Transparent), `main`(ColorData), `player_color`(=main), `text_color`(=main), `wall_color`(=main), `cap_color`(parseCapColor), `colors`(array of ColorData) |
| `Packs/*/Music/*.json` | `loadMusicFromJson` | `id`, `file_name`, `name`, `album`, `author` (NO defaults → ""), `segments[]`(`time` no default, `beat_pulse_delay_offset`(0)) |
| `Profiles/*.json` | `loadProfileFromJson` | `version` (object; effectively ignored, §3.3 bug), `name` (no default), `scores` (map, no default), `favorites` (array, `{}`) |
| `config.json` | `Config::root()` + LinkedValues | 101 keys, §6.2 |
| `ConfigOverrides/*.json` | `loadConfig` | shallow-merged per top-level key into the config root |
| `Assets/menubackgrounds.json` | MenuGame (out of scope here) | read at MenuGame.cpp:1804 |

Case sensitivity: folder/file discovery uses `ssvufs::scan` (dirent +
`endsWith`/exact-name) — **case-sensitive** (ext must match `.json`/`.ogg`
lowercase; names must match exactly, e.g. `reloadLevel` looks for
`"<mId>.json"`). `Path::hasExtension` (unused by the loaders) is the only
case-insensitive helper.

---

## §10. Top subtleties (for the Rust port)

1. **Trailing commas are NOT accepted** by the vendored jsoncpp fork, while
   `//` and `/* */` comments are accepted anywhere (empirically verified,
   §1.6). The task brief's assumption of trailing-comma tolerance is wrong;
   replicate rejection (with the exact error strings) or be deliberately
   lenient — but know the C++ baseline.
2. **Missing keys without a default silently produce zero values** via the
   null singleton (§1.4): `""`, `0`, `0.0f`, `false`, empty container. Many
   loaders rely on this (e.g. music `id` defaults to `""`, not `"nullId"`).
3. **Type mismatches throw `std::runtime_error`** (`"Value is not
   convertible to float."` etc.) that are only caught around per-pack asset
   loading (§1.4) — a bad `config.json` value type (string where number
   expected) crashes the game at `syncAllFromObj`.
4. `isObj("version")` bug in `loadProfileFromJson` (§3.3): profile version is
   always `{-1,0,0}` at load time.
5. `pack.json` without `"version"` → default 0 → `buildPackId` **asserts**
   (`packVersion > 0`) in debug builds; release continues with `_0` ids.
   Shipped packs all have `version: 1`.
6. Object member order in *written* JSON is byte-sorted (jsoncpp
   `std::map`), floats are `%#.16g` (§6.3); a Rust writer that preserves
   insertion order produces a valid but differently-ordered config.json.
7. `ssvufs::Path` normalization: folders get a trailing slash appended at
   `getStr()` time (§4.1) — all pack subfolder path constructions depend on
   it (`"Packs/base/" + "Levels/"`).
8. Extension matching in scans is case-sensitive and non-recursive
   (single-level), alphabetically sorted by full path (§4.4).
9. Float formatting of diffMult in validator ids is iostream default
   6-significant-digit (§7.1) — `1.0` serializes as `"1"`, `0.35` as
   `"0.35"`; mismatched formatting breaks score/leaderboard key parity.
10. `LevelData` constructor appends `1.0` to `difficultyMults` and sorts;
    `unscored` levels still get validator entries. `getNthDiffMult` wraps
    (modulo) rather than clamping.
11. Asset-load fail-soft quirks: `getMusicData`/`getStyleData` fall back to
    the *first* map entry when missing (with an assert the map is
    non-empty); `AssetStorage` keeps a null entry after a failed load
    (`has*` = true, `get*` = empty SFML object); `musicPathMap.emplace` on
    reload never replaces an existing mapping.
12. The `pulse_enabled` config key has **no getter** (persisted, never read).
13. Joystick: controller id hardcoded to 0; `33` = unbound sentinel button;
    dpad Y axis inverted vs. stick Y (§8.1); `update()` no-ops entirely
    while `ignoreAllPresses` (latching all was-pressed flags first).
14. `Reader::parse(std::istream&)` reads only the first line (latent bug,
    unused); all game parsing goes through the string overload with
    `collectComments = false`.
15. Config overrides are shallow (top-level key replacement only) and are
    applied *before* `syncAllFromObj`; they are selected by the
    extension-less file name being present in the startup arg list
    (`loadConfig(overrideIds)`).

