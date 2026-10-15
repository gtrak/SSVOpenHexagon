//! C++ `main.cpp:80-154` — the CLI argument surface: `ParsedArgs`,
//! `parseArgs` and `getFirstCompressedReplayFilenameFromArgs`.
//!
//! Issue 15 step (b): the `open-hexagon` binary's argument parsing, verbatim
//! against `src/SSVOpenHexagon/Core/main.cpp`.

/// C++ `ParsedArgs` (main.cpp:80-88).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedArgs {
    /// C++ `args` — every unrecognized token, in order. Includes `argv[0]`
    /// (the executable path): the C++ loop starts at `i = 0`, so the
    /// program name is pushed here unless it happens to equal a flag.
    pub args: Vec<String>,
    /// C++ `cliLevelName` — the `-l` value (later wins; the C++ overwrites).
    pub cli_level_name: Option<String>,
    /// C++ `cliLevelPack` — the `-p` value (later wins; the C++ overwrites).
    pub cli_level_pack: Option<String>,
    /// C++ `printLuaDocs`.
    pub print_lua_docs: bool,
    /// C++ `headless`.
    pub headless: bool,
    /// C++ `server`.
    pub server: bool,
}

/// C++ `parseArgs(argc, argv)` (main.cpp:90-140), verbatim.
///
/// `argv` is the FULL argv slice (Rust `std::env::args()` yields `argv[0]`
/// first, exactly like the C++ `main(argc, argv)` — the loop starts at
/// `i = 0` either way). Ported behaviours the unit tests pin:
///
/// * `-p` / `-l` consume the NEXT token as their value — even when that
///   token is another flag (`-p -l x` ⇒ `cliLevelPack = "-l"` and `x`
///   lands in `args`);
/// * a `-p` / `-l` with NO following token falls through to `args`: the
///   C++ `i + 1 < argc` guard fails, none of the other flag checks match,
///   and the bare flag is pushed;
/// * a later `-p` / `-l` OVERWRITES the earlier value (the C++ assigns to
///   the `std::optional`, it does not append);
/// * `-printLuaDocs` / `-headless` / `-server` are idempotent sets;
/// * everything else (including `argv[0]`) goes to `args`.
#[must_use]
pub fn parse_args(argv: &[String]) -> ParsedArgs {
    let mut result = ParsedArgs::default();

    let mut i = 0;
    while i < argv.len() {
        // Find command-line pack name (to immediately run level)
        if argv[i] == "-p" && i + 1 < argv.len() {
            i += 1;
            result.cli_level_pack = Some(argv[i].clone());
            i += 1;
            continue;
        }

        // Find command-line level name (to immediately run level)
        if argv[i] == "-l" && i + 1 < argv.len() {
            i += 1;
            result.cli_level_name = Some(argv[i].clone());
            i += 1;
            continue;
        }

        // Find command-line argument to print Lua docs
        if argv[i] == "-printLuaDocs" {
            result.print_lua_docs = true;
            i += 1;
            continue;
        }

        // Find command-line argument to run in headless mode
        if argv[i] == "-headless" {
            result.headless = true;
            i += 1;
            continue;
        }

        // Find command-line argument to run in server mode
        if argv[i] == "-server" {
            result.server = true;
            i += 1;
            continue;
        }

        result.args.push(argv[i].clone());
        i += 1;
    }

    result
}

/// C++ `getFirstCompressedReplayFilenameFromArgs` (main.cpp:146-154): the
/// FIRST arg CONTAINING the `.ohr.z` substring — a substring test anywhere
/// in the token (the C++ `arg.find(".ohr.z") != std::string::npos`), NOT a
/// "does not start with `-`" / suffix test. Scans `args` only (the
/// recognized flags never reach it, but `argv[0]` does — verbatim).
#[must_use]
pub fn get_first_compressed_replay_filename_from_args(args: &[String]) -> Option<String> {
    args.iter().find(|arg| arg.contains(".ohr.z")).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The full-surface parse (spec: "unit-test with argv slices"):
    /// `argv[0]` + a pack/level pair + a mode flag + a leftover token.
    #[test]
    fn full_argv_slice_parses_every_field() {
        let parsed = parse_args(&argv(&[
            "open-hexagon",
            "-p",
            "cube",
            "-l",
            "apeirogon",
            "-headless",
            "replay.ohr.z",
        ]));
        assert_eq!(parsed.cli_level_pack, Some("cube".to_string()));
        assert_eq!(parsed.cli_level_name, Some("apeirogon".to_string()));
        assert!(!parsed.print_lua_docs);
        assert!(parsed.headless);
        assert!(!parsed.server);
        // `argv[0]` + the unrecognized token, in order (C++ `i = 0` start).
        assert_eq!(
            parsed.args,
            vec!["open-hexagon".to_string(), "replay.ohr.z".to_string()]
        );
    }

    /// `-p` as the LAST token: the C++ `i + 1 < argc` guard fails, so the
    /// bare `-p` falls through to `args` and no pack is set.
    #[test]
    fn p_without_following_value_falls_into_args() {
        let parsed = parse_args(&argv(&["open-hexagon", "-p"]));
        assert_eq!(parsed.cli_level_pack, None);
        assert_eq!(
            parsed.args,
            vec!["open-hexagon".to_string(), "-p".to_string()]
        );
    }

    /// `-p` consumes the next token UNCONDITIONALLY — even another flag:
    /// `-p -l x` ⇒ pack `"-l"`, level not set, `x` in `args`.
    #[test]
    fn p_eats_the_following_token_even_when_it_is_a_flag() {
        let parsed = parse_args(&argv(&["open-hexagon", "-p", "-l", "x"]));
        assert_eq!(parsed.cli_level_pack, Some("-l".to_string()));
        assert_eq!(parsed.cli_level_name, None);
        assert_eq!(
            parsed.args,
            vec!["open-hexagon".to_string(), "x".to_string()]
        );
    }

    /// Later wins: the C++ assigns into the `std::optional`, so the second
    /// `-p` / `-l` overwrites the first.
    #[test]
    fn later_p_and_l_overwrite_earlier_values() {
        let parsed = parse_args(&argv(&["-p", "a", "-p", "b", "-l", "one", "-l", "two"]));
        assert_eq!(parsed.cli_level_pack, Some("b".to_string()));
        assert_eq!(parsed.cli_level_name, Some("two".to_string()));
        assert!(parsed.args.is_empty());
    }

    /// Repeated mode flags are idempotent; unknown tokens (dashed or not)
    /// all land in `args`, in order.
    #[test]
    fn repeated_mode_flags_and_unknown_tokens() {
        let parsed = parse_args(&argv(&[
            "open-hexagon",
            "-headless",
            "-headless",
            "-server",
            "-printLuaDocs",
            "-unknown",
            "somefile.ohr.z",
        ]));
        assert!(parsed.headless);
        assert!(parsed.server);
        assert!(parsed.print_lua_docs);
        assert_eq!(
            parsed.args,
            vec![
                "open-hexagon".to_string(),
                "-unknown".to_string(),
                "somefile.ohr.z".to_string(),
            ]
        );
    }

    /// C++ `getFirstCompressedReplayFilenameFromArgs`: the FIRST arg with
    /// the `.ohr.z` substring wins; the scan is a plain substring test.
    #[test]
    fn replay_scan_first_match_wins_substring_anywhere() {
        let args = argv(&["open-hexagon", "notes.txt", "a.ohr.z.bak", "b.ohr.z"]);
        assert_eq!(
            get_first_compressed_replay_filename_from_args(&args),
            Some("a.ohr.z.bak".to_string())
        );
        // `argv[0]` is part of the scan (the C++ scans `args`, which
        // contains the executable path).
        let args = argv(&["open-hexagon.ohr.z", "b.ohr.z"]);
        assert_eq!(
            get_first_compressed_replay_filename_from_args(&args),
            Some("open-hexagon.ohr.z".to_string())
        );
    }

    /// No `.ohr.z` substring anywhere → `nullopt`.
    #[test]
    fn replay_scan_none_when_absent() {
        let args = argv(&["open-hexagon", "replay.ohr", "replay.z", "-headless"]);
        assert_eq!(get_first_compressed_replay_filename_from_args(&args), None);
    }
}
