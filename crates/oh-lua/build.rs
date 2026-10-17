// Compiles ffi_stub.c (issue 029k1 / K3) — the link-compatibility shim for
// `luaopen_ffi`; see the header of ffi_stub.c for the full rationale.
//
// The stub is only needed by the LuaJIT backend: mlua's `luajit` feature
// references the `luaopen_ffi` symbol at link time even though the FFI-less
// VM never calls it (ffi_stub.c). The Luau backend has no `luaopen_ffi`
// symbol to satisfy, so the object is simply not built there (lane 030a:
// the backend is now a feature — the CARGO_FEATURE_BACKEND_LUAJIT variable
// is set by cargo exactly when `backend-luajit` is enabled).
fn main() {
    // The two backend features are mutually exclusive by contract (the
    // spec's "features first, target default after"): exactly one may be
    // on. mlua does not itself reject the double-enabled state, so fail
    // here with an actionable message instead of a deep mlua compile
    // error.
    let luajit = std::env::var("CARGO_FEATURE_BACKEND_LUAJIT").is_ok();
    let luau = std::env::var("CARGO_FEATURE_BACKEND_LUAU").is_ok();
    if luajit && luau {
        panic!("oh-lua: `backend-luajit` and `backend-luau` are mutually exclusive — enable exactly one (e.g. `--no-default-features --features backend-luau`)");
    }

    if luajit {
        cc::Build::new()
            .file("ffi_stub.c")
            .warnings(false)
            .opt_level(2)
            .compile("oh-lua-ffi-stub");
    }
    println!("cargo:rerun-if-changed=ffi_stub.c");
}
