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

## Index

| Skill | Library | Source of truth | Written by |
|---|---|---|---|
| `lib-pcg.md` | PCG engine (`pcg32_fast` + oracle vectors) | `extlibs/SSVUtils/include/SSVUtils/Internal/PCG/pcg_random.hpp` | issue 001 |
| `lib-ssvutils.md` | SSVUtils core (Frametime/Math/Containers/Rnd) | `extlibs/SSVUtils/include/SSVUtils/Core/**` | issue 001 |
| `lib-binds.md` | SSVStart Trigger/Combo + OH bind layer (key/button enums, JSON bind format, BindControl) | `extlibs/SSVStart/include/SSVStart/{Input,Utils}/**`, `Global/Config.*`, `Core/{Joystick,BindControl}.*` | issue 004 |
| `lib-assets.md` | `Global/Assets` + `AssetStorage` (pack/level/style/music discovery order, priority ordering, `LoadInfo` error accumulation, null-asset fallbacks, `assets.json`, shader registry) | `Global/{Assets,AssetStorage}.*`, `Data/{LevelData,PackData,PackInfo,MusicData,StyleData,LoadInfo}.*`, `extlibs/SSVUtils/.../FileSystem/{Path,Scan}*.hpp` | issue 005 |
| `lib-ssvstart.md` | SSVStart | `extlibs/SSVStart` | (pending) |
| `lib-ssvmenusystem.md` | SSVMenuSystem | `extlibs/SSVMenuSystem` | (pending) |
| `lib-sfml.md` | SFML (custom fork) | CMake pin + call sites | (pending) |
| `lib-luajit.md` | LuaJIT | CMake pin + LuaWrapper | (pending) |
| `lib-zlib.md` | zlib | `compress2`/`uncompress` usage | (pending) |
| `lib-libsodium.md` | libsodium | `Online/Sodium.*` | (pending) |
| `lib-sqlite.md` | SQLite + sqlite_orm | `Online/Database.cpp` | (pending) |
| `lib-imgui.md` | ImGui + imgui-sfml | `src/**` usage | (pending) |
| `lib-boostpfr.md` | boost::pfr | `Global/UtilsJson.hpp` | (pending) |
| `lib-steam.md` | Steamworks SDK | `Core/Steam.cpp` | (pending) |
| `lib-discord.md` | Discord Game SDK | `Core/Discord.cpp` | (pending) |
| `lib-rsfml.md` | rsfml (Rust) | crate source | (pending) |
| `lib-mlua.md` | mlua (Rust) | crate source | (pending) |
| `lib-jsoncpp.md` | JsonCpp fork (ssvuj reader/writer, comment + trailing-comma tolerance, `%#.16g` floats) | `include/SSVOpenHexagon/SSVUtilsJson/JsonCpp/{json.hpp,jsoncpp.inl}`, `Utils/Io.hpp` | issue 003 |
| `lib-serde-json.md` | serde_json 1.0.154 (Value/from_str/Number f64-f32, float formatting vs JsonCpp) | `~/.cargo/registry/src/**/serde_json-1.0.154/src/` | issue 003 |
| `...` | other Rust crates | crate source | (pending) |
