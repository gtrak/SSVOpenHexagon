//! C++ `mainPrintLuaDocs` (main.cpp:167-189) — the `-printLuaDocs`
//! entrypoint. Issue 15 step (b).

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use oh_game::seams::{NullProfiles, ScriptHost};

use crate::compose::compose_with;

/// C++ `mainPrintLuaDocs()` (main.cpp:167-189), verbatim in the pieces that
/// exist:
///
/// ```cpp
/// hg::HGAssets assets{nullptr /* steamManager */, true /* headless */};
/// hg::HexagonGame hg{nullptr, nullptr, assets, nullptr, nullptr, nullptr};
/// std::cout << "\n\n\n\n\n";
/// hg.initLuaAndPrintDocs();
/// std::cout << "\n\n\n\n\n";
/// ssvu::lo("::mainPrintLuaDocs") << "Finished\n";
/// return 0;
/// ```
///
/// `root` = the process CWD (the C++'s CWD-relative asset root). The C++
/// `initLuaAndPrintDocs` = `initLua()` + `LuaScripting::printDocs()`
/// (HGScripting.cpp:1149-1153): the host's `init_lua` registers the 285
/// API globals (in the D-023 feature-ON build the registrations record
/// the docs metadata), and `oh_lua::docs::print_docs` then emits the 11
/// category headers + entries to `out` (the C++ `std::cout` form).
///
/// Documented deviations (the missing C++ pieces, all out of step-(b)
/// scope):
/// * Steam manager (issue 20): absent — the C++ passes `nullptr` here too;
/// * the C++ fatal `std::terminate()` on asset-load failure
///   (`Assets.cpp:323-352`) → `[FATAL ERROR]` line + exit 1;
/// * audio / window / client / discord (issues 17/20/21 + step (c)): the
///   `None`/absent equivalents the engine seams already accept;
/// * profiles: `NullProfiles` (the C++ `HGAssets` profile surface's
///   mutation API is unported — issue 17); the printDocs path never reads
///   profiles anyway.
pub fn print_lua_docs(root: &Path, out: &mut dyn Write) -> std::io::Result<i32> {
    let assets = match oh_assets::Assets::new(
        root, true,  /* headless — the C++ `HGAssets{nullptr, true}` */
        false, /* levelsOnly — the C++ two-arg ctor default */
    ) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            // C++ would `std::terminate()`; the port reports and exits 1.
            writeln!(out, "[FATAL ERROR] Asset loading failed: {}", e.reason)?;
            return Ok(1);
        }
    };

    // The C++ constructs the full `HexagonGame` (null audio/window/client/
    // discord) — the composition builds exactly that, window-less.
    let game = compose_with(
        assets,
        root.to_path_buf(),
        false, /* window */
        Arc::new(NullProfiles),
    );

    // C++ `std::cout << "\n\n\n\n\n";` — exactly FIVE newlines (a
    // `print!`, not a `println!`).
    out.write_all(b"\n\n\n\n\n")?;

    // C++ `hg.initLuaAndPrintDocs()` (HGScripting.cpp:1149-1153).
    game.host.init_lua();
    oh_lua::docs::print_docs(out)?;

    out.write_all(b"\n\n\n\n\n")?;

    // C++ `ssvu::lo("::mainPrintLuaDocs") << "Finished\n";`
    writeln!(out, "[::mainPrintLuaDocs] Finished")?;
    Ok(0)
}

/// The binary entrypoint (the C++ `std::cout` form): `root` = process
/// CWD. (The composition's `SharedLua` single-thread contract: call from
/// the thread that owns `std::io::stdout` — the binary's main thread.)
pub fn main_print_lua_docs(root: &Path) -> i32 {
    let mut stdout = std::io::stdout();
    print_lua_docs(root, &mut stdout).unwrap_or_else(|e| {
        eprintln!("[::mainPrintLuaDocs] stdout write failed: {e}");
        1
    })
}
