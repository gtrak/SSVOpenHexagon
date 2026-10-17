# luajit-src (local patched copy)

> **Local patch — issue 029k1 (K3).** This is a verbatim copy of the
> [`luajit-src 210.5.12+a4f56a4`](https://crates.io/crates/luajit-src) crate
> (LuaJIT source tree untouched) with one behavioral change: the build now
> always passes `-DLUAJIT_DISABLE_FFI` to the LuaJIT build
> (`src/lib.rs` → `build_unix` `XCFLAGS`; `extras/msvcbuild.bat` →
> `LJCOMPILE`).
>
> Why: the C++ game compiles its LuaJIT with CMake
> `LUAJIT_DISABLE_FFI=ON` (`CMakeLists.txt:127`, "TODO (P2): revisit this
> decision about FFI"), so the C++ VM has **no FFI extension**.
> `LUAJIT_DISABLE_FFI` makes `lj_arch.h` set `LJ_HASFFI 0`: the FFI C
> sources (`lib_ffi.c`, `lj_ffi*.c`) compile to empty objects and
> `require("ffi")` is not registered — the port's VM now matches the C++
> build at the compile level, not just observably (the mlua safe-set usage
> already kept `ffi` unreachable at runtime).
>
> The port consumes it via `[patch.crates-io]` in the workspace
> `Cargo.toml` (same crate name/version as the registry crate).

---

# luajit-src

[![Build Status]][github-actions]

[Build Status]: https://github.com/khvzak/luajit-src-rs/workflows/CI/badge.svg
[github-actions]: https://github.com/khvzak/luajit-src-rs/actions

This crate contains the sources of [LuaJIT] 2.1 (from Mike Pall) and logic to build it.
Intended to be consumed by the [mlua-sys] crate.

[LuaJIT]: https://github.com/LuaJIT/LuaJIT
[mlua-sys]: https://crates.io/crates/mlua-sys

# License

This project is licensed under [MIT license](http://opensource.org/licenses/MIT)
