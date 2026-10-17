//! The embedded Lua source this crate loads into a `LuaContext` — every
//! executable snippet in ONE place (29v1).
//!
//! The 29v1 sweep verified every `executeCode` call site in the C++ tree
//! (`src/SSVOpenHexagon`) against this table:
//!
//! * `LuaScripting.cpp:144-147` — [`REDEFINE_IO_OPEN`]
//! * `LuaScripting.cpp:159-168` — [`REDEFINE_RANDOM`]
//! * `HGScripting.cpp:635-638` — [`OVERRIDE_SCORE_VALIDATION`] (the C++'s
//!   `mVar` variable name is spliced in by the caller at `{variable}`)
//! * `HGUpdate.cpp:1518` / `:1522` — `u_log(<cmd>)` / the bare `<cmd>`:
//!   the string is USER INPUT (the interactive console's command line),
//!   not embedded source (ported by `oh-game`'s `lua_console`).
//! * `HGUpdate.cpp:1595` and `Utils.cpp:29` / `:95` — `runLuaCode` /
//!   `runLuaFile`: FILE CONTENTS handed to the context, not embedded
//!   source.
//!
//! `destroyMaliciousFunctions` (LuaScripting.cpp:178-218) is NOT a snippet:
//! it is a fixed sequence of `clearVariable` calls. The port
//! (`sandbox::destroy_malicious_functions`) mirrors the C++ sequence 1:1,
//! including the double `package.loadlib` clear (the C++ :207 and :217).
//!
//! **The guard (the 29s1 class):** a hand-transcribed Lua string embedded
//! in Rust can carry a syntax error that no behaviour audit can see —
//! 29s1's `l_overrideScore` splice carried a second `)` after the
//! variable, a Lua syntax error on EVERY call, so the custom score
//! override never worked. The table-driven test below COMPILES every
//! snippet in a fresh `LuaContext` (load, no call — the C++
//! `executeCode`'s `luaL_load` step). When you add or edit an embedded
//! snippet, add/keep its row in [`embedded_snippets`] in this file.

/// `redefineIoOpen`'s snippet — the two adjacent C++ string literals
/// (LuaScripting.cpp:144-147) concatenated, verbatim.
pub(crate) const REDEFINE_IO_OPEN: &str = concat!(
    "local open = io.open; io.open = function(filename, mode) return ",
    "open(filename, mode == \"rb\" and mode or \"r\"); end",
);

/// `redefineRandom`'s snippet — the C++ raw string
/// (LuaScripting.cpp:159-168), verbatim.
pub(crate) const REDEFINE_RANDOM: &str = "math.random = function(a, b)
    if a == nil and b == nil then
        return u_rndSwitch(0, 0, 0)
    elseif b == nil then
        return u_rndSwitch(1, 0, a)
    else
        return u_rndSwitch(2, a, b)
    end
end
";

/// `l_overrideScore`'s validation splice (HGScripting.cpp:635-638),
/// verbatim: the C++ concatenates `"if (type("` + `mVar` + the raw string
/// ` ) ~= "number") then\n<8 tabs>error("Score override must be a number
/// value")\n<8 tabs>end\n`. `{variable}` is the caller's splice point
/// (the C++'s `mVar`).
///
/// The parens: the `)` right after `{variable}` closes `type(`; the
/// `if (` paren closes after `"number"`. A SECOND `)` after
/// `{variable}` is a Lua syntax error (the 29s1 bug).
pub(crate) const OVERRIDE_SCORE_VALIDATION: &str =
    "if (type({variable} ) ~= \"number\") then\n\t\t\t\t\t\t\t\terror(\"Score override must be a number value\")\n\t\t\t\t\t\t\t\tend\n";

#[cfg(test)]
mod tests {
    use super::{OVERRIDE_SCORE_VALIDATION, REDEFINE_IO_OPEN, REDEFINE_RANDOM};
    use crate::error::LuaError;
    use crate::LuaContext;

    /// One row per embedded/spliced Lua snippet the crate loads into a
    /// context. A snippet WITHOUT a row is exactly how 29s1 slipped
    /// through (a syntax error no behaviour test would see).
    ///
    /// `prelude` is code the row's snippet assumes in the C++ context
    /// (the C++'s `executeCode` runs the snippet in the live game
    /// context; the splice assumes the spliced-in global is a number).
    fn embedded_snippets() -> Vec<(&'static str, Option<&'static str>, String)> {
        vec![
            (
                "redefineIoOpen (LuaScripting.cpp:144-147)",
                None,
                REDEFINE_IO_OPEN.to_string(),
            ),
            (
                // `u_rndSwitch` is only REFERENCED at call time — the
                // snippet itself is dependency-free.
                "redefineRandom (LuaScripting.cpp:159-168)",
                None,
                REDEFINE_RANDOM.to_string(),
            ),
            (
                "l_overrideScore validation (HGScripting.cpp:635-638), the variable name `myScoreVar` spliced in",
                Some("myScoreVar = 42"), // a numeric global, like the C++'
                                         // spliced `mVar` in a sane pack
                OVERRIDE_SCORE_VALIDATION.replace("{variable}", "myScoreVar"),
            ),
        ]
    }

    /// 29v1's mechanical guard: every embedded snippet runs through the
    /// same path the C++ `executeCode` does (load + `pcall`, chunk name
    /// `"chunk"`) and raises NO `SyntaxError` — the 29s1 bug class (a
    /// hand-transcribed Lua typo inside a snippet). For these three
    /// snippets the C++ itself EXECUTES them at init / call time, so a
    /// clean `Ok` is the C++-faithful assertion (they are dependency-free:
    /// `u_rndSwitch` is only referenced when `math.random` is CALLED).
    /// (The port's public surface exposes no LuaJIT compile-only handle —
    /// `mlua`'s `Chunk::compile` is Luau-only — so the guard asserts on
    /// the `execute_code` result instead: a snippet that fails to COMPILE
    /// surfaces as `LuaError::SyntaxError`, exactly what 29s1 was.)
    #[test]
    fn every_embedded_snippet_compiles() {
        for (name, prelude, code) in embedded_snippets() {
            let ctx = LuaContext::new();
            if let Some(prelude) = prelude {
                ctx.execute_code(prelude).unwrap();
            }
            match ctx.execute_code(&code) {
                Ok(()) => {}
                Err(LuaError::SyntaxError { message }) => {
                    panic!("{name} failed to COMPILE (the 29s1 class): {message}");
                }
                Err(err) => panic!("{name} did not run clean: {err}"),
            }
        }
    }

    /// The negative half: 29s1's exact bug (a second `)` right after the
    /// variable) MUST fail with a `SyntaxError` — the guard has teeth.
    #[test]
    fn the_29s1_extra_paren_splice_does_not_compile() {
        let ctx = LuaContext::new();
        let buggy = "if (type(myScoreVar) ) ~= \"number\") then\nerror(\"x\")\nend\n";
        let err = ctx
            .execute_code(buggy)
            .expect_err("the 29s1 extra-`)` splice must not run");
        assert!(
            matches!(err, LuaError::SyntaxError { .. }),
            "expected a SyntaxError (the compile step), got: {err}"
        );
    }
}
