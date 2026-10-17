// Compiles ffi_stub.c (issue 029k1 / K3) — the link-compatibility shim for
// `luaopen_ffi`; see the header of ffi_stub.c for the full rationale.
fn main() {
    cc::Build::new()
        .file("ffi_stub.c")
        .warnings(false)
        .opt_level(2)
        .compile("oh-lua-ffi-stub");
    println!("cargo:rerun-if-changed=ffi_stub.c");
}
