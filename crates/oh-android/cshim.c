// Issue 25 — real-device finding (moto g power 2021, Android 11):
// `dlopen failed: cannot locate symbol "__clear_cache"` — LuaJIT's arm64
// JIT code-cache flush calls `__clear_cache` (luajit-src lj_mcode.c), a
// legacy libgcc/bionic symbol that modern Android REMOVED from bionic's
// exported surface. Rather than patch the vendored LuaJIT, export the
// symbol from our own .so: the dynamic linker resolves the LuaJIT
// reference against this definition (a load-module's own exports are
// searched first). `__builtin___clear_cache` compiles to the ic/ivau +
// dsb + isb instruction sequence — the exact semantics the removed
// bionic symbol had. x86 emulators never hit this (no __clear_cache on
// x86); arm64 real hardware does.
#include <stdint.h>

void __clear_cache(void *begin, void *end);
void __clear_cache(void *begin, void *end) {
    __builtin___clear_cache((char *)begin, (char *)end);
}
