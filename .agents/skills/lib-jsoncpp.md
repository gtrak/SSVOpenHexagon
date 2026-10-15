# lib-jsoncpp — JsonCpp fork used by the game (`ssvuj`)

Source of truth (read in-repo, not fetched):

- `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/json.hpp` (642 lines) — declarations
- `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/jsoncpp.inl` (2442 lines) — all definitions, inlined
- `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/JsonCpp.hpp` (11 lines) — umbrella header
- `src/SSVOpenHexagon/SSVUtilsJson/LinkedValue.cpp` (77 lines) — only TU; `LinkedValue<T>` syncs typed fields via `getExtr`/`arch`

Fork of https://jsoncpp.sourceforge.net/ by Vittorio Romeo (AFL-3.0, header comment `json.hpp:1-4`), included as a system header (`#pragma GCC system_header`, `json.hpp:11`). Both headers use `SSVOH_ASSERT` from `SSVOpenHexagon/Global/Assert.hpp` (`jsoncpp.inl:14`) and `JSON_FAIL_MESSAGE` throws `std::runtime_error` (`json.hpp:23`).

Rust port: `crates/oh-data/src/json.rs` (819 lines) — `serde_json` behind a `strip_comments` pre-pass. Divergences vs. this source are listed at the bottom.

## Reader API the game uses

`ssvuj` aliases (`include/SSVOpenHexagon/SSVUtilsJson/Global/Common.hpp`):

```cpp
using Obj    = Json::Value;               // :16
using Writer = Json::StyledStreamWriter;  // :19
using Reader = Json::Reader;             // :20
```

All game parsing goes through `ssvuj::Impl::tryParse`
(`include/SSVOpenHexagon/SSVUtilsJson/Utils/Io.hpp:22-34`):

```cpp
if(mReader.parse(mSrc, mObj, false))   // Io.hpp:26  ← collectComments = false
```

- `Reader` default ctor: `features_(Features::all())` (`jsoncpp.inl:85`) — so `allowComments_ == true`, `strictRoot_ == false`.
- `parse` overloads declared `json.hpp:467-471` (`collectComments = true` default — the game **always overrides to false**).
- Definition `jsoncpp.inl:114-146`: `jsoncpp.inl:117` `if(!features_.allowComments_) collectComments = false;`
- On failure the game logs `reader.getFormattedErrorMessages()` (`Io.hpp:31,58,64`); format `jsoncpp.inl:682-695`: per error `* Line L, Column C\n  <message>\n` (+ optional `See Line..` for `extra_`).

## `Features` defaults relevant to comments

`json.hpp:58-73`:

```cpp
struct Features
{
    bool allowComments_{true}, strictRoot_{false};   // json.hpp:60
    ...
    inline static Features all() noexcept { return {}; }            // :65-68
    inline static Features strictMode() noexcept { return {false, true}; } // :69-72
};
```

**There is no `allowTrailingCommas_` in this fork** (upstream jsoncpp has one; it was dropped). Comments are the only lexical feature, default ON; `strictRoot_` (off by default) would reject a non-container root (`jsoncpp.inl:133-145`). Trailing-comma rejection is hardcoded in `readObject`/`readArray` (below), not feature-controlled.

## Exact `//` and `/* */` tolerance, and WHERE comments are accepted

Lexing: `readToken` maps `/` to `tokenComment` and calls `readComment` (`jsoncpp.inl:215-217`, fn at 274).

- **`//` (C++ style)** — `readCppStyleComment`, `jsoncpp.inl:318-326`: consumes up to but NOT including the first `\r` or `\n`; always returns `true` (an unterminated `//` at EOF is tolerated).
- **`/* */` (C style)** — `readCStyleComment`, `jsoncpp.inl:309-317`: scans until a `*` followed by `/`; the final `return getNextChar() == '/';` means an **unclosed `/*` fails** → token becomes `tokenError` → error `"Syntax error: value, object or array expected."` (`jsoncpp.inl:170`, reported wherever the comment lexically appeared).
- Comments never attach to values here: the game parses with `collectComments=false`, so `readComment`'s placement logic (`jsoncpp.inl:284-290`, `addComment` at 294) is dead for game data.

Where comment tokens are skipped (i.e. accepted) — a comment is only legal at a spot where one of these runs:

1. `skipCommentTokens` (`jsoncpp.inl:179-191`), called at the start of every `readValue` (`jsoncpp.inl:149,152`) → comments allowed before the root value, after `{`, `[`, `:`, `,` (i.e. anywhere a *value* is expected), and in the tail after the root (`jsoncpp.inl:130`).
2. `readObject` loop: comments before the member-name string are skipped (`jsoncpp.inl:358-359`), and comments after the `,`/`}` separator token are skipped (`jsoncpp.inl:387-388`).
3. `readArray`: after each element, comments are skipped before the `,`/`]` decision (`jsoncpp.inl:414-417`).

NOT accepted (fails with `Missing ':' after object member name`, `jsoncpp.inl:367-371`):
**between an object key string and its `:`** — `readObject` reads the `tokenMemberSeparator` immediately after `decodeString` with no comment skip. So `{ "a" /*c*/ : 1 }` is an error in the C++ baseline.

Also: comments are not recognized inside string literals (`readString`, `jsoncpp.inl:337-349`, only `\"`/`\\` escapes change scanning).

Whitespace: `skipSpaces` (`jsoncpp.inl:254-259`) skips exactly `' '`, `'\t'`, `'\r'`, `'\n'`.

## Trailing-comma behavior (transcribed from real code)

**Objects — `readObject` (`jsoncpp.inl:350-393`)**:

```cpp
while(readToken(tokenName))
{
    while(tokenName.type_ == tokenComment && initialTokenOk)   // :358-359
        initialTokenOk = readToken(tokenName);
    if(!initialTokenOk) break;
    if(tokenName.type_ == tokenObjectEnd && name.empty()) return true;  // :361 — `{}`
    if(tokenName.type_ != tokenString) break;                       // :362
    ... decodeString; require ':' (else "Missing ':' after object member name");
    ... readValue();
    Token comma;
    if(!readToken(comma) || (comma.type_ != tokenObjectEnd &&
                            comma.type_ != tokenArraySeparator &&
                            comma.type_ != tokenComment))           // :378-380
        return addErrorAndRecover("Missing ',' or '}' in object declaration", ...); // :382-384
    while(comma.type_ == tokenComment && finalizeTokenOk)          // :387-388
        finalizeTokenOk = readToken(comma);
    if(comma.type_ == tokenObjectEnd) return true;
}
return addErrorAndRecover("Missing '}' or object member name", ...);  // :391-393
```

So `{"a":1,}`: the `,` passes :378, next iteration reads `}` but `name` is still `"a"` (set at :363-365, never cleared), so the `name.empty()` guard at :361 does **not** fire, :362 breaks, and the function fails with **`Missing '}' or object member name`** (`jsoncpp.inl:391-393`). → **object trailing comma: REJECTED.**

**Arrays — `readArray` (`jsoncpp.inl:394-425`)**:

```cpp
currentValue() = Value(arrayValue);
skipSpaces();
if(*current_ == ']') { readToken(endArray); return true; }   // :398-403 — `[]`
int index{0};
while(true)
{
    ... readValue();
    Token token;
    ok = readToken(token);
    while(token.type_ == tokenComment && ok) ok = readToken(token);   // :414-417
    bool badTokenType = (token.type_ != tokenArraySeparator &&
                         token.type_ != tokenArrayEnd);              // :418-419
    if(!ok || badTokenType)
        return addErrorAndRecover("Missing ',' or ']' in array declaration", ...); // :421-424
    if(token.type_ == tokenArrayEnd) break;
}
```

So `[1,]`: the `,` is accepted at :418, the next `readValue` sees `]` and hits the `default:` → **`Syntax error: value, object or array expected.`** (`jsoncpp.inl:170`) → `recoverFromError(tokenArrayEnd)` → parse fails. → **array trailing comma: REJECTED.**

**Trailing garbage AFTER the root: ACCEPTED by this fork.** `parse` does `skipCommentTokens(token)` after the root (`jsoncpp.inl:130`) and then only checks `strictRoot_` (off) before `return successful;` (`jsoncpp.inl:145-146`). There is **no `tokenEndOfStream` check** — `{"a":1} junk` parses successfully. (Upstream jsoncpp rejects it; this fork does not.)

## Number parsing

- `readNumber` (`jsoncpp.inl:327-336`): consumes digits plus `.` `e` `E` `+` `-`.
- `decodeNumber` (`jsoncpp.inl:429-468`): the token is treated as a double iff it contains `.`/`e`/`E`/`+` or a `-` that is not the first char (`:431-437`); otherwise exact integer math — negative values clamp at `minLargestInt`/`maxLargestInt` (i64), positives at `maxLargestUInt` (u64); **overflow falls back to `decodeDouble`** (`:453-458`). In-range: `-n` → `LargestInt`, `≤ maxInt` → `LargestInt` (intValue), else u64 (uintValue).
- `decodeDouble` (`jsoncpp.inl:469-498`): `sscanf(buffer, "%lf", &value)` (`:485`, `:490`); `count != 1` → error `'...' is not a number.` (`:492-495`). **No finiteness check**: `1e999` parses to C++ `inf` and is stored in a `realValue`.

## `Value` accessor API the game calls

`ssvuj::Converter` specializations (`include/SSVOpenHexagon/SSVUtilsJson/Utils/BasicConverters.hpp`):

| C++ call | line | Rust analogue (`oh-data/src/json.rs`) |
|---|---|---|
| `mObj.asInt()` (char, uchar, int) | :61-63 | `as_int` |
| `mObj.asFloat()` (float) | :64 | `as_float` |
| `mObj.asDouble()` (double) | :65 | `as_float` |
| `mObj.asBool()` (bool) | :66 | `as_bool` |
| `mObj.asString()` (std::string) | :67 | `as_str` |
| `mObj.asCString()` (const char*) | :68 | — (not ported) |
| `mObj.asLargestInt()` (long) | :78 | `as_int` |
| `mObj.asUInt()` (uint, ushort) | :92, :106 | — (via `as_int` for ints) |

Semantics (definitions in `jsoncpp.inl`), per type:

- `asInt` (`:1191-1209`): intValue/uintValue asserted in int range (else throws `"LargestInt out of int range"` / `"LargestUInt out of int range"`); realValue → `int(value_.real_)` (truncate toward zero, asserted in range); null → 0; bool → 1/0; **other → throws `"Value is not convertible to int."`**
- `asUInt` (`:1211-1229`): same shape; null → 0; real asserted `0..=maxUInt`
- `asInt64`/`asUInt64` (`:1231`, `:1249`), `asLargestInt`/`asLargestUInt` (`:1268-1281`, = the 64-bit pair, `JSON_HAS_INT64` defined at `json.hpp:29`)
- `asDouble` (`:1284-1295`): int/uint → double; real → value; null → 0.0; bool → 1.0/0.0; other throws
- `asFloat` (`:1297-1308`): same with float casts; real path is `static_cast<float>(value_.real_)` — float converters go through the **f64** first
- `asBool` (`:1310-1320`): bool/null/ints/real; string → throws
- `asString` (`:1177-1189`): null → `""`; string → value; bool → `"true"`/`"false"`; int/uint/real → `valueToString`; array/object → throws `"Type is not convertible to std::string"`
- `isConvertibleTo` (`jsoncpp.inl:1323`+), `type()` (`json.hpp:183`), `size()`/`empty()` (`json.hpp:236-237`), `get(key, default)` (`json.hpp:254-255`), `isMember` (`json.hpp:259-260`), `operator[]` (`json.hpp:227-235`) are used by `Main.hpp`/game code; the C++ **const `operator[]` on a missing key returns the null singleton** (reads as 0/0.0/`false`/`""`, `size()==0`), and a non-array `operator[](int)` also yields null — the Rust port splits that into `get_or_null` / `get_missing_as_null` / `get_at_or_null` (`json.rs:247-270`).

Wrong-type conversions throw `std::runtime_error` (`JSON_FAIL_MESSAGE`, `json.hpp:23`); only the per-pack asset-load path in `src/SSVOpenHexagon/Global/Assets.cpp` (uses `Json::ValueType` at `Assets.cpp:678-679`) is where this can be caught.

## Writer (number & string formatting)

The game's writer is `ssvuj::Writer = Json::StyledStreamWriter` (`Common.hpp:19`), used by `ssvuj::writeToStream`/`writeToString`/`writeToFile` (`Io.hpp:97-123`). Defaults: `rightMargin_(74)`, indentation `"\t"` (`jsoncpp.inl:2225-2229`; `json.hpp:603` ctor default `"\t"`). `isMultineArray` (`jsoncpp.inl:2336-2361`): multiline iff `size*3 >= 74`, or any child is a non-empty array/object, or computed line length `>= 74`; inline arrays render as `[ a, b ]` on one line.

Reals — `valueToString(double)`, `jsoncpp.inl:1876-1902`:

```cpp
char buffer[32];
sprintf(buffer, "%#.16g", value);          // jsoncpp.inl:1879
char* ch = buffer + strlen(buffer) - 1;
if(*ch != '0') return buffer;
while(ch > buffer && *ch == '0') --ch;
char* last_nonzero = ch;
while(ch >= buffer)
{
    switch(*ch)
    {
        case '0'..'9': --ch; continue;
        case '.': *(last_nonzero + 2) = '\0'; return buffer;
        default: return buffer;
    }
}
return buffer;
```

i.e. **16 significant digits via `%#.16g`, then strip trailing zeros but keep one trailing digit past the last non-zero**: `8.0` → `"8.0"`, `100.5` → `"100.50"`, `9.45f` (as double) → `"9.449999809265137"`, `1e-5`-scale → scientific `d.dddddddddddddddde-xx`. Used by `StyledStreamWriter` at `jsoncpp.inl:2248` (also `StyledWriter` :2033, `FastWriter` :1977). Integers: `valueToString(LargestInt/LargestUInt)` (`jsoncpp.inl:1847`, `:1858`) — plain decimal, no range formatting.

Strings — `valueToQuotedString` (`jsoncpp.inl:1908`+): escapes `\"` `\\` `\b` `\f` `\n` `\r` `\t`; other control chars `0x01..=0x1F` as `\uXXXX` **uppercase** hex; `0x7F` and above pass through unescaped.

Booleans/literals written as `true`/`false`/`null` (`valueToString(bool)` `jsoncpp.inl:1904`; `jsoncpp.inl:2246-2252`).

## Divergences of the Rust reader (`crates/oh-data/src/json.rs`) vs. this source

Behavioral parity is the goal; the following are the known/verified differences:

1. **Trailing garbage after root**: C++ `parse` has no end-of-input check (`jsoncpp.inl:130,145-146`) and accepts `{"a":1} junk`; `serde_json::from_str` (used at `json.rs:92`) rejects it with `"trailing characters"`. Rust is *stricter*.
2. **Non-finite reals**: C++ `decodeDouble` stores `inf`/`nan` for e.g. `1e999` (`jsoncpp.inl:469-498`); serde_json maps via `Number::from_f64`, which returns `None` for non-finite → `"not a JSON number"` error (see `lib-serde-json.md`). Rust is *stricter*.
3. **Error wording on trailing commas**: rejection matches (both reject object and array trailing commas — see above), but C++ reports `"Missing '}' or object member name"` / `"Syntax error: value, object or array expected."` while Rust surfaces serde's `"trailing comma"` / `"expected value"` (noted in `json.rs:92-98`).
4. **Unterminated `/*`**: both reject, but C++ reports the generic `"Syntax error: value, object or array expected."` while Rust emits its own `"Syntax error: unterminated /* comment"` (`json.rs:195-201`).
5. **Comment placement**: `json.rs:strip_comments` replicates every acceptance point, including the key-`:` gap rejection (`json.rs:150-160` ↔ `jsoncpp.inl:367-371`) and `//`-at-EOF tolerance (both). Comment spans are blanked (newlines kept) so C++-style line/column error positions survive.
6. **Integer/real storage**: fork stores `intValue`/`uintValue`/`realValue`; serde stores `PosInt(u64)`/`NegInt(i64)`/`Float(f64)`. Boundary behavior is re-implemented in `json.rs` `as_int`/`as_float` (`json.rs:336-370`): out-of-`i64` uint → error (fork's assert-throw), real → truncate toward zero (fork's `int(value_.real_)`), null → 0/0.0. Negative magnitudes beyond `i64::MIN`: fork falls back to double (`jsoncpp.inl:453-458`); serde parses as `f64` — same value class, possibly different exact bits far out in the tail.
7. **Writer float fallback**: `json.rs:518` uses `Number::from_f64(f as f64).unwrap_or_else(|| Number::from(0))` for arch'ing a `float` — a non-finite f32 becomes JSON `0` in Rust, whereas the C++ fork would write `inf`/`nan` via `%#.16g`. (Theoretical: shipped config data is finite.)

Everything else in `json.rs` (indentation, `isMultineArray` 74-column rule, string escaping incl. unescaped `0x7F`, `%#.16g` + trim, trailing `\n` after root) is a verbatim re-implementation of this fork (see `json.rs:589-817` and `lib-serde-json.md` — note: the trailing-comma error wording is serde's, see item 3).
