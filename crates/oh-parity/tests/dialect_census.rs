//! Lane 030c — the third-party dialect census (task §2.1): the number IS
//! the evidence.
//!
//! Issue 30 §12's only plausible HARD blocker for unifying on Luau is a
//! 5.1-ism that Luau removed (`setfenv`, `getfenv`, `loadstring`, `unpack`,
//! `newproxy`, `module`, `gcinfo`) appearing in a SHIPPED pack — the
//! in-repo census of 2026-10-17 found zero across all 42
//! `_RELEASE/Packs/**/*.lua`, but that number was recorded once, in a plan
//! doc. This test re-runs that census on every gate run, over the same
//! scope (all shipped pack scripts — every level script is under
//! `Packs/<pack>/Scripts/`; verified: zero `.lua` files exist in
//! `_RELEASE` outside `Packs/`), prints the per-pattern count, and fails
//! the gate if any is non-zero. A future pack that introduces a 5.1-ism
//! therefore cannot land silently: the moment the number stops being zero,
//! this test says exactly which pattern, in which file.
//!
//! Matching is WORD-BOUNDARY semantics, not raw substring: the `ffi`
//! census must count the identifier `ffi` (or `ffi.` / `ffi(`), not the 172
//! occurrences of the substring inside `u_getDifficultyMult` — the
//! original plan §6 census found zero, which is only coherent under
//! boundary matching. A hit at a word boundary flags *use of the global*,
//! the only form that can break under Luau (a user variable that merely
//! contains the substring, e.g. `local difficulty = 1`, is Luau-fine).
//!
//! The reference patterns beyond the seven (`ffi`, `bit.`, `bit32.`,
//! `jit.`, `require(`, `string.dump`) are the rest of the plan §6 census:
//! not all Luau-removed (`bit32` EXISTS on Luau; `require` is absent from
//! Luau's standard surface), but part of the same "third-party surface we
//! do not test" question — zero here too, asserted too.
//!
//! Backend-agnostic (a pure file scan — no VM involved), so it runs under
//! either feature mode. Self-contained on purpose: it uses only the
//! `_RELEASE` root (a three-line helper below), not `common`'s engine
//! composition — the census must not drag the 285-function Lua API into a
//! file whose only job is to count substrings.

use std::path::Path;

/// The `_RELEASE` root (the same idiom as `common::release_dir` — this test
/// binary is the only one that uses the root without the engine composition,
/// so it carries the three lines itself rather than importing the module
/// whose other symbols it would not read).
fn release_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../_RELEASE")
        .to_path_buf()
}

/// One census pattern. `lead_boundary` = the byte before the match must not
/// be a word char or `.` (an identifier start, unqualified — this is what
/// separates the global `unpack(` from the Luau-safe `table.unpack(`);
/// `tail_boundary` = the byte after the match must not be a word char (the
/// identifier is EXACTLY the needle — `setfenv` but not a hypothetical
/// `xsetfenvy`). Patterns whose needle ends in an anchor byte (`(` or `.`)
/// need no tail boundary.
struct Pattern {
    name: &'static str,
    needle: &'static str,
    lead_boundary: bool,
    tail_boundary: bool,
}

const PATTERNS: [Pattern; 13] = [
    // -- the seven 5.1-isms Luau removed (the hard-blocker candidates) --
    Pattern {
        name: "setfenv",
        needle: "setfenv",
        lead_boundary: true,
        tail_boundary: true,
    },
    Pattern {
        name: "getfenv",
        needle: "getfenv",
        lead_boundary: true,
        tail_boundary: true,
    },
    Pattern {
        name: "loadstring",
        needle: "loadstring",
        lead_boundary: true,
        tail_boundary: true,
    },
    // The global CALL form (`table.unpack` is Luau-safe and never counted):
    Pattern {
        name: "unpack(global)",
        needle: "unpack(",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "newproxy",
        needle: "newproxy",
        lead_boundary: true,
        tail_boundary: true,
    },
    Pattern {
        name: "module(call)",
        needle: "module(",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "gcinfo",
        needle: "gcinfo",
        lead_boundary: true,
        tail_boundary: true,
    },
    // -- the plan §6 reference set (the wider third-party surface) -------
    Pattern {
        name: "ffi",
        needle: "ffi",
        lead_boundary: true,
        tail_boundary: true,
    },
    Pattern {
        name: "bit.",
        needle: "bit.",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "bit32.",
        needle: "bit32.",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "jit.",
        needle: "jit.",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "require(call)",
        needle: "require(",
        lead_boundary: true,
        tail_boundary: false,
    },
    Pattern {
        name: "string.dump",
        needle: "string.dump",
        lead_boundary: true,
        tail_boundary: true,
    },
];

/// All `.lua` files under `_RELEASE/Packs/**` (sorted: deterministic).
fn shipped_lua_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}")) {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "lua") {
                out.push(p);
            }
        }
    }
    let packs = root.join("Packs");
    walk(&packs, &mut out);
    out.sort();
    out
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Count the census matches (see the pattern-boundary contract above).
fn count(text: &str, p: &Pattern) -> usize {
    let bytes = text.as_bytes();
    let needle = p.needle.len();
    let mut i = 0usize;
    let mut n = 0usize;
    while let Some(off) = text[i..].find(p.needle) {
        let abs = i + off;
        let lead_ok = !p.lead_boundary
            || abs == 0
            || (!is_word_char(bytes[abs - 1]) && bytes[abs - 1] != b'.');
        let tail_ok =
            !p.tail_boundary || abs + needle >= bytes.len() || !is_word_char(bytes[abs + needle]);
        if lead_ok && tail_ok {
            n += 1;
        }
        i = abs + needle;
    }
    n
}

fn p(name: &str) -> &'static Pattern {
    PATTERNS
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("pattern {name}"))
}

/// The census harness itself is tested: the boundary rules must catch the
/// 5.1-form usages and reject both the Luau-safe forms and the
/// false-positive substrings (a pattern that silently stops matching would
/// turn the zero into a lie).
#[test]
fn census_patterns_discriminate() {
    // The 5.1 usages (the risk) must be counted:
    assert_eq!(
        count("local a, b = unpack({1, 2})\n", p("unpack(global)")),
        1
    );
    assert_eq!(count("x = unpack(t, 1, 2)\n", p("unpack(global)")), 1);
    assert_eq!(count("local f = setfenv\nsetfenv(1, t)\n", p("setfenv")), 2);
    assert_eq!(count("local e = getfenv(0)\n", p("getfenv")), 1);
    assert_eq!(count("loadstring('return 1')()\n", p("loadstring")), 1);
    assert_eq!(count("local t = newproxy(true)\n", p("newproxy")), 1);
    assert_eq!(count("module('mylib')\n", p("module(call)")), 1);
    assert_eq!(
        count(r#"local kb = collectgarbage("gcinfo")"#, p("gcinfo")),
        1
    );
    assert_eq!(count("ffi.string(ptr, 3)\n", p("ffi")), 1);
    assert_eq!(count("require('ffi')\n", p("require(call)")), 1);
    // The Luau-safe / false-positive forms must NOT be counted:
    assert_eq!(
        count("local a, b = table.unpack({1, 2})\n", p("unpack(global)")),
        0
    );
    assert_eq!(count("u_getDifficultyMult()\n", p("ffi")), 0); // ffi ⊂ Difficulty
    assert_eq!(count("myloadstring()\n", p("loadstring")), 0); // longer identifier
    assert_eq!(count("xmodule('t')\n", p("module(call)")), 0);
    assert_eq!(count("mybit.x(1)\n", p("bit.")), 0); // bit. ⊂ mybit. (lead boundary)
}

/// The census: per-pattern counts over every shipped pack script, printed
/// (the number is the evidence) and pinned at zero.
#[test]
fn shipped_packs_carry_no_removed_51_isms() {
    let root = release_dir();
    let files = shipped_lua_files(&root);
    assert!(
        !files.is_empty(),
        "the census found no shipped .lua files under _RELEASE/Packs — the scope moved"
    );

    eprintln!(
        "030c dialect census: {} shipped .lua files under _RELEASE/Packs",
        files.len()
    );
    let mut hits = 0usize;
    for p in &PATTERNS {
        let mut total = 0usize;
        let mut where_hits: Vec<String> = Vec::new();
        for f in &files {
            let text =
                std::fs::read_to_string(f).unwrap_or_else(|e| panic!("read {}: {e}", f.display()));
            let c = count(&text, p);
            total += c;
            if c > 0 {
                where_hits.push(format!(
                    "{}:{}",
                    f.strip_prefix(&root).unwrap().display(),
                    c
                ));
            }
        }
        eprintln!("  {:<16} = {total}", p.name);
        if total > 0 {
            hits += 1;
            eprintln!(
                "    -> {} (the number stopped being zero — the 5.1-ism is now in-tree)",
                where_hits.join(" ")
            );
        }
    }
    assert_eq!(
        hits, 0,
        "{hits} census pattern(s) are non-zero — see the lines above"
    );
    eprintln!(
        "030c dialect census: all {} patterns = 0 (the expected zero, re-verified)",
        PATTERNS.len()
    );
}
