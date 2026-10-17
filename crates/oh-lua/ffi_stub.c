// Link-compatibility shim for mlua's `luajit` feature (issue 029k1 / K3).
//
// The port builds its vendored LuaJIT with -DLUAJIT_DISABLE_FFI (matching
// the C++ build's CMake `LUAJIT_DISABLE_FFI=ON`, CMakeLists.txt:127 — the
// C++ VM has no FFI extension; see extlibs/luajit-src/README.md). An
// FFI-less libluajit.a does not export `luaopen_ffi`, but mlua 0.10.5's
// `luajit` feature still *references* the symbol in its stdlib loader
// (mlua `src/state/raw.rs`, `load_std_libs`): the `luaL_requiref` call is
// runtime-guarded by `StdLib::FFI` and safe mode rejects the flag with a
// SafetyError, yet the compiler emits the relocation, so the symbol must
// exist at link time.
//
// The port only ever creates states through mlua's safe constructor
// (crates/oh-lua/src/context.rs — `StdLib::ALL_SAFE`, which excludes FFI),
// so `luaopen_ffi` is never called. This object exists solely to satisfy
// the linker; if it were ever called, it raises the same error a C++ VM
// gives for `require("ffi")`.
//
// Only stable LuaJIT C-ABI declarations are used (lua_State is an opaque
// struct pointer; lua_pushlstring/lua_error are exported by every LuaJIT
// build, FFI on or off) — no LuaJIT headers are needed, which keeps this
// stub buildable regardless of how/where the VM itself was compiled.
struct lua_State;
extern int lua_pushlstring(struct lua_State *L, const char *s, unsigned long len);
extern int lua_error(struct lua_State *L);

int luaopen_ffi(struct lua_State *L) {
    lua_pushlstring(L, "module 'ffi' is not available in this build",
                    (unsigned long)(sizeof("module 'ffi' is not available in this build") - 1));
    return lua_error(L);
}
