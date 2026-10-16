// Issue 25 — the cshim build: compiles crates/oh-android/cshim.c (the
// `__clear_cache` legacy-bionic shim that LuaJIT's arm64 JIT needs; see
// cshim.c for the full story) into the cdylib. The cc crate's
// CC_<target> env comes from tools/android-env.sh (the NDK clang).
fn main() {
    // Desktop targets never link this shim into a LuaJIT-carrying cdylib
    // (oh-android is the android entry crate) — but build.rs runs for
    // host builds too (cargo check -p oh-android on Linux); compile the
    // shim anyway: a plain C file with a no-op-on-host builtin.
    let mut b = cc::Build::new();
    b.file("cshim.c");
    b.warnings(false);
    b.compile("oh_android_cshim");
}
