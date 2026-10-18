# In-repo skills (`.agents/skills/`)

Skills here are **authoritative reference** for the port. Each documents one
library (vendored C++, third-party C++ dependency, or Rust crate), written from
**real source** — never from memory, docs.rs, or ~/.cargo/registry.

## Rules for authors

1. **Real source only.** Vendored C++ under `extlibs/` and `public/`; Rust crates
   from their checked-out source under `~/.cargo/registry/src/**` (read it with
   `read`/`grep`, do not fetch). Record the exact version/commit the skill was
   written against.
2. **Exact API surface used.** Document what this project *uses*, with exact
   signatures/semantics copied from source, plus enough of the surrounding API
   that a worker does not have to re-read the original.
3. **Exact math/constants.** Any formula, constant, or enum value that affects
   behavior is quoted verbatim (f32/f64, sign conventions, units).
4. **Corrections are allowed and required.** If code reality contradicts a skill,
   code wins for the implementation AND the worker makes a minimal, factual
   correction to the skill file, listed in the report under "skill corrections".
5. **One file per library**, named `lib-<name>.md`. Keep it dense and greppable.

> **038 layout note:** the C++ tree's root is now `cpp-original/` (issue 038
> reorg, D-038). The root symlinks `src` and `include` into `cpp-original/`
> keep pre-reorg citation paths resolving — cite through them or through
> `cpp-original/…`; both reach the same pristine files. Submodule paths are
> `cpp-original/extlibs/…` (see `.gitmodules`).

## Index

| Skill | Library | Source of truth | Written by |
|---|---|---|---|
| `lib-pcg.md` | PCG engine (`pcg32_fast` + oracle vectors) | `extlibs/SSVUtils/include/SSVUtils/Internal/PCG/pcg_random.hpp` | issue 001 |
| `lib-ssvutils.md` | SSVUtils core (Frametime/Math/Containers/Rnd) | `extlibs/SSVUtils/include/SSVUtils/Core/**` | issue 001 |
| `lib-binds.md` | SSVStart Trigger/Combo + OH bind layer (key/button enums, JSON bind format, BindControl) | `extlibs/SSVStart/include/SSVStart/{Input,Utils}/**`, `Global/Config.*`, `Core/{Joystick,BindControl}.*` | issue 004 |
| `lib-assets.md` | `Global/Assets` + `AssetStorage` (pack/level/style/music discovery order, priority ordering, `LoadInfo` error accumulation, null-asset fallbacks, `assets.json`, shader registry) | `Global/{Assets,AssetStorage}.*`, `Data/{LevelData,PackData,PackInfo,MusicData,StyleData,LoadInfo}.*`, `extlibs/SSVUtils/.../FileSystem/{Path,Scan}*.hpp` | issue 005 |
| `lib-ssvstart.md` | SSVStart | `extlibs/SSVStart` | (pending) |
| `lib-ssvmenusystem.md` | SSVMenuSystem (Menu/Category/ItemBase/Items{Single,Slider,Toggle,GoBack,Goto}/Controller — exact navigation semantics: wrapIndex, goBack pop+top, exec disabled-skip, Toggle flip, Slider clamp+`": value"`, Goto/GoBack stack effects; Controller is enable-when only, NO key handling; rendering is MenuGame's) | `extlibs/SSVMenuSystem` pin `f2a75d1`, `crates/oh-menu-system/src/**`, `crates/oh-menu-system/tests/navigation.rs` | issue 012 |
| `lib-sfml.md` | SFML (custom fork) — drawing-layer surface only: `Vector2`, `Color`, `Vertex`, `PrimitiveType`, `Drawable`/`RenderTarget`/`RenderStates`, `Texture`/`Sprite`, `Font`/`Text`, `RectangleShape`/`Shape`/`Transformable`, `View`/`Transform`, `RenderTexture` (include-only), `Shader` (+`setUniformUnsafe`)/`Glsl`, `Clock` (3.x `restart()`-only)/`Time`/`degrees`, `Keyboard::Key`/`Mouse::Button` (tables cross-ref `oracle-vectors.md`); the full `sf::Packet` codec lives in `lib-sfml-packet.md` | pin `vittorioromeo/SFML@dacceddb` (`CMakeLists.txt:108-112`), fork headers verified byte-identical to the pin, `mapping/oracle-vectors.md` | issue 011 |
| `lib-sfml-packet.md` | SFML `sf::Packet` wire codec (custom fork): per-type byte layout (bool 1 byte; u16/u32/u64 big-endian; float/double raw host-endian — NOT swapped; `std::string` u32-BE length + raw; wstring 4-byte-BE per char), `checkSize`/`m_isValid` poison semantics, C++→Rust API map to `oh-replay::Packet` | pin `vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`, `src/SFML/Network/Packet.cpp`, `crates/oh-replay/src/packet.rs`, `mapping/oracle-vectors.md` | issue 016 |
| `lib-render-stack.md` | `oh-render` platform stack per D-011: `winit` 0.30.13 (run_app/ApplicationHandler, event+key/button→SFML mapping), `glutin` 0.32.3 (Display/bootstrap, direct — NO `glutin-winit`), `glow` 0.18.0 (unsafe raw-GL), `rodio` 0.23.0 (MixerDeviceSink/Player), `fontdue` 0.9.4 (metrics/rasterize); headless limitation + skip-with-reason GPU tests | `crates/oh-render/src/**` + `tests/{backend,golden_frames}.rs`, `~/.cargo/registry/src/**/{winit-0.30.13,glutin-0.32.3,glow-0.18.0,rodio-0.23.0,fontdue-0.9.4}`, `Cargo.lock`, `DECISIONS.md` D-011 | issue 011 |
| `lib-luajit.md` | LuaJIT (5.1 C API; all-double numbers, `%.14g` C-API number strings vs 17-digit Lua `tostring`, 5.1 truthiness/hex/`newproxy`, `error()` tracebacks, sandbox surface, `_getGlobal`/`_setGlobal` dotted-name semantics) | CMake pin `vittorioromeo/LuaJIT@4dc5ffb` (`LUAJIT_DISABLE_FFI`), luajit-src 210.5.12 (Rust), `src/SSVOpenHexagon/{Utils/LuaWrapper.cpp,Core/LuaScripting.cpp}`, verified empirically via `crates/oh-lua` tests | issue 006 |
| `lib-zlib.md` | zlib | `compress2`/`uncompress` usage | (pending) |
| `lib-libsodium.md` | libsodium as used by `Online/Sodium.*` + `Shared.cpp`: `crypto_kx` key exchange (PS→RT keys, symmetry invariant), `crypto_secretbox` (XSalsa20-Poly1305, tag-prefixed; random 24-byte nonces carried in `PEncryptedMsg`), keyed-BLAKE2b `sodiumHash` (zero 32-byte key), `randomUInt64` (host-endian), `sodiumKeyToString` (decimal concatenation); sodiumoxide 0.2.7 API map + deterministic KAT strategy | `Online/Sodium.{hpp,cpp}`, `Shared.cpp:634-686, 690-719`, sodiumoxide 0.2.7 vendored source, `mapping/oracle-vectors.md` ("Net wire vectors") | issue 016 |
| `lib-sqlite.md` | SQLite + sqlite_orm (the `Online/Database` schema + every query): the byte-exact sqlite_orm DDL (double-quoted ids, trailing-space columns, reverse `make_table` creation order, the `sqlite_sequence` placement), the full 19-method query map with C++ citations, the PK-excluded-INSERT / UNIQUE(userId) / signed-staleness / INNER-JOIN / in-place-addScore traps, rusqlite 0.31 `bundled` (D-004) | `src/SSVOpenHexagon/Online/Database.cpp` (391 lines), `public/sqlite_orm/sqlite_orm.h` (16137-16152, 18005-18013), `public/sqlite/sqlite3.c` 3.36.0, `tools/oracle/{db_probe.cpp,db_probe_schema.txt}`, `crates/oh-net/src/{database,db_records}.rs` + `tests/database.rs` | issue 017 |
| `lib-steamworks.md` | `steamworks` 0.13.1 (the issue-20 binding): the full C++ SDK-call enumeration of `Core/Steam.cpp` (with line cites), the C++→crate API map, the 3 gaps + fallbacks (hand-written `RequestCurrentStats` FFI; the encrypted-app-ticket `#if`-branch is INACTIVE in this tree — `SecretSteamKey.hpp` absent; UGC folder buffer 4096 vs 512), the loader-stub link/load model + the `steam` cargo feature split (default OFF = the C++'s `SSVOH_ANDROID` no-SDK build), the open `Callback` trait | `src/SSVOpenHexagon/Core/Steam.cpp` (1062 lines), `public/steam/{steam_api.h,steam_api_flat.h:432,steamencryptedappticket.h}`, `_RELEASE/steam_appid.txt` (1358090), `~/.cargo/registry/src/**/{steamworks-0.13.1,steamworks-sys-0.13.0}`, `crates/oh-steam/src/**` | issue 020 |
| `lib-imgui.md` | ImGui + imgui-sfml | `src/**` usage | (pending) |
| `lib-boostpfr.md` | boost::pfr | `Global/UtilsJson.hpp` | (pending) |
| `lib-steam.md` | Steamworks SDK | `Core/Steam.cpp` | (pending) |
| `lib-discord.md` | Discord Game SDK | `Core/Discord.cpp` | (pending) |
| `lib-rsfml.md` | rsfml (Rust) | crate source | (pending) |
| `lib-mlua.md` | mlua 0.10.5 (luajit+vendored: `Lua::new` safe-stdlib set, `Chunk::eval`, `Value`/`Error` mapping, `create_function` closure form, `Value::to_string` formatting trap, `FunctionToPush` port incl. last-N arg window) | `~/.cargo/registry/src/**/mlua-0.10.5/src/`, `crates/oh-lua/src/**` | issue 006 |
| `lib-lua-api.md` | the `oh-lua` game API (285 registered globals; `add_lua_fn!`/`add_lua_fn_try!`/`add_lua_fn_raw!` spread-argument dispatch to per-arity methods, exact C++ `FunctionToPush` arity/last-N-window semantics, `GameApiState`/`GameApi`/`AssetsApi`/`SharedLua`, `reg_accessor!` macros, menu-context name-set delta, deprecated-function warning messages) | `crates/oh-lua/src/api/**`, `crates/oh-lua/tests/{api,runtime}.rs`, `src/SSVOpenHexagon/Core/{LuaScripting,HGScripting,MenuGame}.cpp` | issue 007 |
| `lib-jsoncpp.md` | JsonCpp fork (ssvuj reader/writer, comment + trailing-comma tolerance, `%#.16g` floats) | `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/{json.hpp,jsoncpp.inl}`, `Utils/Io.hpp` | issue 003 |
| `lib-gameplay.md` | Gameplay types: CWall/CPlayer/CCustomWall(+Manager)/SpeedData/HexagonGameStatus/Ticker/timeline2/CustomTimeline/FastVertexVectorTris/PointInPolygon — vertex ordering, collision-escape walk, push displacement, curve-rotation sign, RNG-in-update coupling, time bookkeeping | `include/SSVOpenHexagon/{Components,Core,Data,Utils}/**` + `src/SSVOpenHexagon/{Components,Core,Utils}/**`, `mapping/gameplay.md` | issue 008 |
| `lib-serde-json.md` | serde_json 1.0.154 (Value/from_str/Number f64-f32, float formatting vs JsonCpp) | `~/.cargo/registry/src/**/serde_json-1.0.154/src/` | issue 003 |
| `...` | other Rust crates | crate source | (pending) |
