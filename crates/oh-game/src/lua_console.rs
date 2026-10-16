//! The headless core of the ImGui Lua console (issue 014 step (a)).
//!
//! Literal port of `HexagonGame`'s console state machine — the part that
//! has no window/egui surface (that is step (b), with issue 15):
//!
//! * [`complete`] / [`history`] — the two `ilcTextEditCallback` callback
//!   cases (`HGUpdate.cpp:1148-1286`: tab completion over
//!   `LuaScripting::getAllFunctionNames()`, Up/Down history with the `-1`
//!   sentinel). The `ImGuiInputTextCallbackData` buffer effects are modelled
//!   as explicit [`BufferOp`]s (C++ `DeleteChars`/`InsertChars`) returned in
//!   C++ order; [`IlcEditEffect::apply`] performs them with imgui's cursor
//!   semantics. The C++ `ilcCmdLog` lines are returned in
//!   [`IlcEditEffect::log_lines`] (the caller appends them).
//! * [`interpret_command`] — the `ImGui::InputText("Command")` Enter branch
//!   of `postUpdate_ImguiLuaConsole` (`HGUpdate.cpp:1427-1545`): `!CLEAR` /
//!   `!HELP` / `!ff <seconds>` / `!advt <ticks>` / `?fn` / the `u_log`
//!   fallback with its two nested catches.
//! * [`track_code`] / [`untrack_all`] / [`update_tracked_results`] — the
//!   Track input, the "Untrack All" button, and the execute-and-erase-
//!   on-error tracked-results loop (`HGUpdate.cpp:1562-1621`).
//!
//! The engine-side glue (the C++ `Tid::LuaConsole` trigger,
//! `postUpdate_ImguiLuaConsole`'s pre-UI gate/toggle prefix, and
//! `initLuaAndPrintDocs`) is on [`HexagonGame`](crate::hexagon_game::HexagonGame)
//! below; the console state fields themselves live field-for-field on
//! [`EngineState`](crate::hexagon_game::EngineState)
//! (`HexagonGame.hpp:107-118`), with `ilcCmdLog` / `ilcLuaTrackedResults`
//! on the Lua-shared [`SharedState`](crate::lua_state::SharedState) (the
//! 007 lambdas write them: `u_log` and `u_impl_addTrackedResult`).
//!
//! # Gates (D-022/D-023)
//!
//! C++ `postUpdate_ImguiLuaConsole` gates on `window == nullptr` first, then
//! `!Config::getDebug() || Config::getOfficial()` (force-hide + clear the
//! pending toggle), then runs the `ilcShowConsoleNext` toggle swap. The port's
//! headless engine has no window, so
//! [`post_update`](crate::hexagon_game::HexagonGame::post_update) documents
//! the window return and runs the two gates the C++ runs after it (the
//! spec-mandated headless-testable prefix); the whole ImGui tail
//! (`HGUpdate.cpp:1305-1641`) is step (b).

use std::io::Write;

use oh_math::hg::concat::{concat, ConcatPart};

use crate::hexagon_game::{EngineState, HexagonGame};
use crate::lua_state::{SharedState, SharedStore};
use crate::seams::{ScriptError, ScriptHost};

/// The C++ `prefixHeaders` (`LuaMetadata.hpp:37-48`, 11 entries, verbatim) —
/// what `printDocs` / `NoScriptHost::print_docs` emit unconditionally.
///
/// The canonical copy used by the composition is
/// `oh_lua::metadata::PREFIX_HEADERS`; the dependency cycle (oh-lua depends
/// on oh-game, D-013) prevents sharing the constant, so this crate carries
/// the verbatim copy the no-Lua host needs. Keep in lockstep with the
/// oh-lua one (the C++ source is the oracle for both).
pub const PREFIX_HEADERS: [&str; 11] = [
    "## Utility Functions (u_)\n\nBelow are the utility functions, which can be identified with the \"u_\" prefix. These are overall functions that either help utilize the game engine, be incredibly beneficial to pack developers, or help simplify complex calculations.",
    "## Audio Functions (a_)\n\nBelow are the audio functions, which can be identified with the \"a_\" prefix. This library is capable of controlling everything audio with the game, including the level music and sounds. Set the music, play a specific sound, all of this is in the domain of this prefix.",
    "## Main Timeline Functions (t_)\n\nBelow are the main timeline functions, which can be identified with the \"t_\" prefix. These are functions that have effects on the main timeline itself, but they mainly consist of waiting functions. Using these functions helps time out your patterns and space them in the first place.",
    "## Event Timeline Functions (e_)\n\nBelow are the event timeline functions, which can be identified with the \"e_\" prefix. These are functions that are similar to the Main Timeline functions, but they instead are for the event timeline as opposed to the main timeline. Use these functions to help set up basic events for the game, such as handling messages. Use ``e_eval`` to do more advanced events.",
    "## Level Functions (l_)\n\nBelow are the level functions, which can be identified with the \"l_\" prefix. These are functions that have a role in altering the level mechanics themselves, including all level properties and attributes. These typically get called en masse in `onInit` to initialize properties.",
    "## Style Functions (s_)\n\nBelow are the style functions, which can be identified with the \"s_\" prefix. These are functions that have a role in altering the attributes of the current style that is on the level. Style attributes, unlike level attributes, do not get initialized in Lua and rather are premade in a JSON file (but this is subject to change).",
    "## Wall Functions (w_)\n\nBelow are the basic wall functions, which can be identified with the \"w_\" prefix. These are the functions sole responsible for wall creation in the levels. There are a variety of walls that can be made with different degrees of complexity, all of which can be used to construct your own patterns.",
    "## Custom Wall Functions (cw_)\n\nBelow are the custom wall functions, which can be identified with the \"cw_\" prefix. These are functions with foundations of [Object-oriented programming](https://en.wikipedia.org/wiki/Object-oriented_programming) to allow pack developers to customize individual walls and their properties and make the most out of them.",
    "## Custom Timeline Functions (ct_)\n\nBelow are the custom timeline functions, which can be identified with the \"ct_\" prefix. These are functions with foundations of [Object-oriented programming](https://en.wikipedia.org/wiki/Object-oriented_programming) to allow pack developers to create and manage independent timelines.",
    "## Shader Functions (shdr_)\n\nBelow are the shader functions, which can be identified with the \"shdr_\" prefix. These are functions that enable graphical manipulation of the rendered game scene via GLSL shaders.",
    "## Miscellaneous Functions\n\nBelow are the miscellaneous functions, which can have a variable prefix or no prefix at all. These are other functions that are listed that cannot qualify for one of the above eight categories and achieve some other purpose, with some functions not meant to be used by pack developers at all.",
];

/// C++ `ImGuiKey_UpArrow` / `ImGuiKey_DownArrow` — the only two keys the
/// history callback reacts to (`HGUpdate.cpp:1249-1266`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IlcEventKey {
    UpArrow,
    DownArrow,
}

/// One `ImGuiInputTextCallbackData` buffer mutation (C++
/// `DeleteChars`/`InsertChars`, `imgui.h`). Positions are byte offsets into
/// the callback's `Buf` — the values the C++ computed (word start /
/// post-delete cursor), in C++ call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferOp {
    /// C++ `data->DeleteChars(start, count)`.
    DeleteChars { start: usize, count: usize },
    /// C++ `data->InsertChars(pos, text)`.
    InsertChars { pos: usize, text: String },
}

/// The effect of one `ilcTextEditCallback` invocation: the buffer
/// mutations (C++ order) plus the lines the C++ appends to `ilcCmdLog`
/// (the completion callback's `"No match..."` / `"Possible matches..."` /
/// `"- <name>..."` lines; the history callback appends none).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IlcEditEffect {
    /// The C++ `DeleteChars`/`InsertChars` calls, in C++ order.
    pub ops: Vec<BufferOp>,
    /// The C++ `ilcCmdLog.emplace_back(...)` lines, in C++ order.
    pub log_lines: Vec<String>,
}

impl IlcEditEffect {
    /// Performs [`self.ops`](IlcEditEffect::ops) on `(buf, cursor)` with
    /// imgui's `InputText` cursor semantics (the port of
    /// `ImGuiInputTextCallbackData::DeleteChars` / `::InsertChars`):
    ///
    /// * `DeleteChars(pos, count)`: if `cursor >= pos`, `cursor -= count`
    ///   (floored at 0) — imgui: `if (CursorPos >= pos)
    ///   CursorPos = ImMax(0, CursorPos - count);`
    /// * `InsertChars(pos, text)`: if `cursor >= pos`, the cursor moves
    ///   past the inserted text — imgui: `if (CursorPos >= pos)
    ///   CursorPos += text_bytes;`
    ///
    /// The `log_lines` are NOT applied here (they belong to the engine's
    /// `ilcCmdLog`; the caller appends them — the step (b) UI or the engine
    /// methods below).
    pub fn apply(&self, buf: &mut String, cursor: &mut usize) {
        for op in &self.ops {
            match op {
                BufferOp::DeleteChars { start, count } => {
                    let end = (*start + *count).min(buf.len());
                    buf.replace_range(*start..end, "");
                    if *cursor >= *start {
                        *cursor = (*cursor).saturating_sub(*count);
                    }
                }
                BufferOp::InsertChars { pos, text } => {
                    let p = (*pos).min(buf.len());
                    buf.insert_str(p, text);
                    if *cursor >= p {
                        *cursor += text.len();
                    }
                }
            }
        }
    }
}

/// C++ file-local `Stricmp` (`HGUpdate.cpp:1124-1133`) — case-insensitive
/// equality (ASCII; `std::toupper` in the C locale is ASCII uppercase).
#[must_use]
pub(crate) fn stricmp_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// C++ file-local `Strnicmp(fnName, word, n)` (`HGUpdate.cpp:1135-1145`) == 0:
/// `fnName`'s first `n` chars case-insensitively equal `word` (all `n` of
/// them — a shorter `fnName` mismatches; `n == 0` matches everything).
#[must_use]
fn strnicmp_prefix(name: &str, word: &str) -> bool {
    name.get(0..word.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(word))
}

/// C++ `ilcTextEditCallback`'s `ImGuiInputTextFlags_CallbackCompletion`
/// branch (`HGUpdate.cpp:1152-1237`).
///
/// `buf` / `cursor` are the C++ `data->Buf` / `data->CursorPos` (the command
/// buffer as currently typed); `names` is the C++
/// `LuaScripting::getAllFunctionNames()` result. Returns the buffer effect +
/// the `ilcCmdLog` lines (the caller appends them to `ilc_cmd_log`).
#[must_use]
pub fn complete(buf: &str, cursor: usize, names: &[String]) -> IlcEditEffect {
    // Locate beginning of current word (C++: scan back over
    // `' ' '\t' ',' ';'`).
    let word_end = cursor;
    let mut word_start = word_end;
    while word_start > 0 {
        let c = buf.as_bytes()[word_start - 1];
        if c == b' ' || c == b'\t' || c == b',' || c == b';' {
            break;
        }
        word_start -= 1;
    }

    // Skip starting `?` for Lua docs.
    if buf.as_bytes().get(word_start) == Some(&b'?') {
        word_start += 1;
    }

    let word = &buf[word_start..word_end];

    // Build a list of candidates (`Strnicmp(fnName, word_start, len) == 0`).
    let candidates: Vec<&String> = names.iter().filter(|n| strnicmp_prefix(n, word)).collect();

    if candidates.is_empty() {
        // C++: `snprintf(buf, 255, "No match for \"%.*s\"!\n", len, word)`
        // (the 255-byte cap is C++ buffer hygiene; a Rust `String` cannot
        // overflow, so the format is verbatim without the cap).
        return IlcEditEffect {
            ops: Vec::new(),
            log_lines: vec![format!("No match for \"{word}\"!\n")],
        };
    }

    if candidates.len() == 1 {
        // Single match. Delete the beginning of the word and replace it
        // entirely so we've got nice casing.
        let name = &candidates[0];
        return IlcEditEffect {
            ops: vec![
                BufferOp::DeleteChars {
                    start: word_start,
                    count: word_end - word_start,
                },
                // C++ `InsertChars(data->CursorPos, ...)` AFTER the delete:
                // the cursor (at `word_end`) lands at `word_start`.
                BufferOp::InsertChars {
                    pos: word_start,
                    text: name.to_string(),
                },
                BufferOp::InsertChars {
                    pos: word_start + name.len(),
                    text: " ".to_string(),
                },
            ],
            log_lines: Vec::new(),
        };
    }

    // Multiple matches. Complete as much as we can.. So inputing "C"+Tab
    // will complete to "CL" then display "CLEAR" and "CLASSIFY" as matches.
    // (The C++ loop, `HGUpdate.cpp:1212-1222`, verbatim: `c` is the running
    // case-folded char; a NUL (`c == 0`) from the FIRST candidate, or a
    // disagreement with a later candidate, stops the scan. A candidate
    // shorter than `match_len` reads its `'\0'` and stops the scan there.)
    let mut match_len = word_end - word_start;
    loop {
        let mut c: u8 = 0;
        let mut all_candidates_matches = true;
        for (i, cand) in candidates.iter().enumerate() {
            if !all_candidates_matches {
                break; // C++ `i < candidates.Size && all_candidates_matches`
            }
            let byte = cand.as_bytes().get(match_len).copied().unwrap_or(0);
            if i == 0 {
                c = byte.to_ascii_uppercase();
            } else if c == 0 || c != byte.to_ascii_uppercase() {
                all_candidates_matches = false;
            }
        }
        if !all_candidates_matches {
            break;
        }
        match_len += 1;
    }

    let mut effect = IlcEditEffect::default();
    if match_len > 0 {
        effect.ops.push(BufferOp::DeleteChars {
            start: word_start,
            count: word_end - word_start,
        });
        // C++ `InsertChars(data->CursorPos, candidates[0], candidates[0] +
        // match_len)` — the range of the first candidate up to `match_len`.
        effect.ops.push(BufferOp::InsertChars {
            pos: word_start,
            text: candidates[0][..match_len].to_string(),
        });
    }

    // List matches.
    effect.log_lines.push("Possible matches:\n".to_string());
    for cand in &candidates {
        // C++ `Utils::concat("- ", candidates[i], '\n')`.
        effect.log_lines.push(format!("- {cand}\n"));
    }
    effect
}

/// C++ `ilcTextEditCallback`'s `ImGuiInputTextFlags_CallbackHistory` branch
/// (`HGUpdate.cpp:1247-1280`): Up/Down over `ilcHistory` with the `-1`
/// sentinel ("not browsing").
///
/// `history_pos` is the C++ `ilcHistoryPos`; `buf_len` is the C++
/// `data->BufTextLen` (the whole buffer is replaced when the position
/// changes). Returns `(new_history_pos, effect)`.
#[must_use]
pub fn history(
    history: &[String],
    history_pos: i32,
    key: IlcEventKey,
    buf_len: usize,
) -> (i32, IlcEditEffect) {
    let prev_history_pos = history_pos;
    let mut pos = history_pos;
    match key {
        IlcEventKey::UpArrow => {
            if pos == -1 {
                // C++ `ilcHistoryPos = ilcHistory.size() - 1;` — for an empty
                // history the C++ `size_t` wraps (`0 - 1`) and the conversion
                // to `int` yields `-1` (no-op browse); the `i32` here does the
                // same arithmetic directly.
                pos = history.len() as i32 - 1;
            } else if pos > 0 {
                pos -= 1;
            }
        }
        IlcEventKey::DownArrow => {
            if pos != -1 {
                pos += 1;
                if pos >= history.len() as i32 {
                    pos = -1;
                }
            }
        }
    }

    if prev_history_pos == pos {
        return (pos, IlcEditEffect::default());
    }

    // C++: `history_str = (pos >= 0) ? ilcHistory[pos].c_str() : ""` — then
    // `DeleteChars(0, BufTextLen); InsertChars(0, history_str);`
    let history_str = history.get(pos as usize).map_or("", String::as_str);
    (
        pos,
        IlcEditEffect {
            ops: vec![
                BufferOp::DeleteChars {
                    start: 0,
                    count: buf_len,
                },
                BufferOp::InsertChars {
                    pos: 0,
                    text: history_str.to_string(),
                },
            ],
            log_lines: Vec::new(),
        },
    )
}

/// The C++ exception classes the numeric parses throw
/// (`std::invalid_argument` / `std::out_of_range`, libstdc++ `stod`/`stoi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IlcParseError {
    InvalidArgument,
    OutOfRange,
}

/// C++ `std::stod` (libstdc++ semantics, pinned by probe):
///
/// * leading C-locale whitespace is skipped; no converted characters ⇒
///   `invalid_argument`;
/// * a trailing junk after a valid prefix is fine (`"2.5xyz"` → 2.5, like
///   `strto_d`);
/// * `overflow` (`"1e400"`) and `underflow` (`"1e-320"`, even when it
///   rounds to a subnormal — glibc sets `ERANGE` for any inexact
///   underflow) ⇒ `out_of_range`; the `inf`/`infinity` literals (any case,
///   optional sign) parse to ±inf WITHOUT throwing;
/// * a zero literal in any form (`"0"`, `"0e-999"`, `"-0"`, ...) ⇒ 0, no
///   throw;
/// * a trailing junk after a valid prefix is ignored (`"2.5xyz"` → 2.5) —
///   C's `num_get` consumes the longest valid floating-point token, so the
///   port scans for the longest prefix Rust accepts.
///
/// Documented divergence from libstdc++ (reachable only via exotic console
/// input; the C++ console is a dev tool): hex floats (`"0x1p3"` → 8 in C++)
/// are `invalid_argument` in the port (Rust's `f64::from_str` grammar) —
/// reported as an error rather than silently taking the `"0"` prefix. `nan`
/// payloads (`"nan(1)"` → nan in C++) are `invalid_argument` too.
pub(crate) fn parse_stod(s: &str) -> Result<f64, IlcParseError> {
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if t.is_empty() {
        return Err(IlcParseError::InvalidArgument);
    }
    // Longest Rust-accepted prefix (C's `num_get` consumes the longest
    // valid floating-point token and ignores the rest).
    let mut end = t.len();
    while end > 0 && t[..end].parse::<f64>().is_err() {
        end -= 1;
    }
    if end == 0 {
        return Err(IlcParseError::InvalidArgument);
    }
    let consumed = &t[..end];
    // The `"0"` prefix of a C++ hex float (`"0x1p3"` → 8): the port does
    // not parse hex floats — error out instead of silently returning 0.
    // (`"0xyz"`-style junk is NOT a hex float in C++ either: `strtod`
    // leaves the `x` unconsumed, so the plain `"0"` parse stands.)
    let zero_prefix = match consumed.strip_prefix(['+', '-']) {
        Some(z) => z == "0",
        None => consumed == "0",
    };
    if zero_prefix {
        let tail = &t[end..];
        let hex_float = tail.get(0..1).is_some_and(|c| c == "x" || c == "X")
            && tail[1..].bytes().any(|b| b.is_ascii_hexdigit());
        if hex_float {
            return Err(IlcParseError::InvalidArgument);
        }
    }
    let v = consumed.parse::<f64>().expect("prefix just parsed");
    // Overflow: Rust rounds to ±inf only when the exact value is beyond
    // `DBL_MAX` + half an ulp (exactly like glibc, whose `strto_d` sets
    // `ERANGE` there — nearer values round to `DBL_MAX` in both, e.g.
    // `"1.7976931348623158e308"` → `DBL_MAX`); C++ throws unless the input
    // IS the inf literal.
    if v.is_infinite() {
        let lit = t.strip_prefix(['+', '-']).unwrap_or(t).to_ascii_lowercase();
        if lit != "inf" && lit != "infinity" {
            return Err(IlcParseError::OutOfRange);
        }
    }
    // Underflow: C++ throws when the exact value rounds to a subnormal or 0
    // (any nonzero input that parses to 0 underflowed; a subnormal result is
    // always inexact — no finite decimal IS an exact subnormal). A zero
    // LITERAL (zero mantissa, any exponent — `"0e-999"`) is an exact 0, no
    // throw.
    if v.is_subnormal() || (v == 0.0 && has_nonzero_mantissa_digit(consumed)) {
        return Err(IlcParseError::OutOfRange);
    }
    Ok(v)
}

/// Whether the decimal literal's MANTISSA (sign excluded, up to the
/// exponent marker) carries a nonzero digit — `"0e-999"` is the zero
/// literal even though the exponent has nonzero digits.
fn has_nonzero_mantissa_digit(s: &str) -> bool {
    let mantissa = s
        .strip_prefix(['+', '-'])
        .unwrap_or(s)
        .split(['e', 'E'])
        .next()
        .unwrap_or(s);
    mantissa.bytes().any(|b| b.is_ascii_digit() && b != b'0')
}

/// C++ `std::stoi` (libstdc++ `strtol` base-10 semantics, pinned by probe):
///
/// * leading C-locale whitespace skipped; sign; a DECIMAL prefix is
///   consumed and the rest ignored (`"7xyz"` → 7, `"0x7"` → 0, `"12.5"` →
///   12);
/// * no digit at all ⇒ `invalid_argument`;
/// * value outside `int` (32-bit) ⇒ `out_of_range` (libstdc++ checks
///   `ERANGE` from `strtol`).
pub(crate) fn parse_stoi(s: &str) -> Result<i32, IlcParseError> {
    let t = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (sign, rest) = match t.as_bytes().first() {
        Some(b'-') => (-1, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    let digits_len = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    let digits = &rest[..digits_len];
    if digits.is_empty() {
        return Err(IlcParseError::InvalidArgument);
    }
    // More than 19 digits cannot fit i64 either → `strtol` saturates →
    // `ERANGE`.
    match digits.parse::<i64>() {
        Err(_) => Err(IlcParseError::OutOfRange),
        Ok(mag) => {
            let v = mag * sign as i64;
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                Err(IlcParseError::OutOfRange)
            } else {
                Ok(v as i32)
            }
        }
    }
}

/// C++ `postUpdate_ImguiLuaConsole`'s command-interpretation block
/// (`HGUpdate.cpp:1427-1545`) — the `ImGui::InputText("Command",
/// &ilcCmdBuffer, ...)` Enter branch, headless.
///
/// `cmd` is the C++ `cmdString` (the buffer content at Enter; the engine
/// method clears `ilc_cmd_buffer` around this call). Control flow is the
/// C++ one, statement for statement:
///
/// 1. log `# <cmd>\n` (C++ `concat("# ", cmdString, '\n')`);
/// 2. reset `ilcHistoryPos` to `-1`, drop the LAST duplicate from
///    `ilcHistory`, push `cmd` (BEFORE interpretation — C++ `:1436-1443`);
/// 3. split on `' '` (C++ `Utils::split<std::string>` — empty pieces
///    dropped) and dispatch `!CLEAR` / `!HELP` / `!ff` / `!advt` / `?fn` /
///    the `u_log` fallback.
pub fn interpret_command(
    shared: &mut SharedState,
    est: &mut EngineState,
    cmd: &str,
    host: &dyn ScriptHost,
) {
    // C++ `const std::string cmdString = ilcCmdBuffer;` (the buffer clear
    // happens in the engine method, mirroring `ilcCmdBuffer.clear()`).
    shared.ilc_cmd_log.push(format!("# {cmd}\n"));

    // History dedupe + push (C++ `:1436-1443` — the push precedes the
    // split/dispatch, so even `!CLEAR` is recorded first... and then the
    // `!CLEAR` branch erases the whole log, `#` line included).
    est.ilc_history_pos = -1;
    for i in (0..est.ilc_history.len()).rev() {
        if est.ilc_history[i] == cmd {
            est.ilc_history.remove(i);
            break;
        }
    }
    est.ilc_history.push(cmd.to_string());

    // C++ `Utils::split<std::string>(cmdString)` — split on the default
    // delimiter `" "`, empty pieces dropped.
    let cmd_split: Vec<&str> = cmd.split(' ').filter(|s| !s.is_empty()).collect();

    if stricmp_eq(cmd, "!CLEAR") {
        // C++ `:1448-1450`.
        shared.ilc_cmd_log.clear();
    } else if stricmp_eq(cmd, "!HELP") {
        // C++ `:1452-1461` — the raw-string help block, verbatim.
        shared.ilc_cmd_log.push(HELP_TEXT.to_string());
    } else if cmd_split.len() > 1 && cmd_split[0] == "!ff" {
        // C++ `:1462-1482` — `std::stod(cmdSplit.at(1))`.
        match parse_stod(cmd_split[1]) {
            Ok(seconds) => {
                // C++ `concat("[ff]: fast forwarding to ", seconds, '\n')`
                // — the double goes through `std::ostream` (6-significant-
                // digit `%g`), hence `oh_math::hg::concat`.
                shared.ilc_cmd_log.push(concat(&[
                    ConcatPart::Str("[ff]: fast forwarding to "),
                    ConcatPart::F64(seconds),
                    ConcatPart::Str("\n"),
                ]));
                est.fast_forward_target = Some(seconds);
            }
            Err(IlcParseError::InvalidArgument) => {
                // C++ `catch(const std::invalid_argument&)`.
                shared
                    .ilc_cmd_log
                    .push("[error]: invalid argument for <seconds>\n".to_string());
            }
            Err(IlcParseError::OutOfRange) => {
                // C++ `catch(const std::out_of_range&)`.
                shared
                    .ilc_cmd_log
                    .push("[error]: out of range for <seconds>\n".to_string());
            }
        }
    } else if cmd_split.len() > 1 && cmd_split[0] == "!advt" {
        // C++ `:1484-1504` — `std::stoi(cmdSplit.at(1))`. Note the C++
        // reuses the `<seconds>` wording on BOTH error strings
        // (`HGUpdate.cpp:1499,1503`) — kept verbatim.
        match parse_stoi(cmd_split[1]) {
            Ok(ticks) => {
                // C++ `concat("[advt]: advancing simulation by ", ticks,
                // " ticks\n")`.
                shared.ilc_cmd_log.push(concat(&[
                    ConcatPart::Str("[advt]: advancing simulation by "),
                    ConcatPart::I32(ticks),
                    ConcatPart::Str(" ticks\n"),
                ]));
                // C++ `:1494` — `ticks >= 0 ? ticks : 0`.
                est.advance_tick_count = Some(ticks.max(0));
            }
            Err(IlcParseError::InvalidArgument) => {
                shared
                    .ilc_cmd_log
                    .push("[error]: invalid argument for <seconds>\n".to_string());
            }
            Err(IlcParseError::OutOfRange) => {
                shared
                    .ilc_cmd_log
                    .push("[error]: out of range for <seconds>\n".to_string());
            }
        }
    } else if let Some(rest) = cmd.strip_prefix('?') {
        // C++ `:1506-1511` — `getRTrim(cmdString.substr(1))` (C-locale
        // `rTrim` = trailing `isspace` = trailing ASCII whitespace).
        let rest = rest.trim_end();
        // C++ `LuaScripting::getDocsForFunction(rest)`.
        let docs = host.docs_for_function(rest);
        // C++ `concat("[?]: ", docs)` — `docs` already carries its trailing
        // newlines when found (`"{ret} {name}({args}):\n{docs}\n\n"`), none
        // for the `"UNKNOWN FUNCTION"` fallback.
        shared.ilc_cmd_log.push(format!("[?]: {docs}"));
    } else {
        // C++ `:1512-1537` — the nested-catch fallback.
        //
        // ```cpp
        // try {
        //     try {
        //         lua.executeCode(Utils::concat("u_log(", cmdString, ")\n"));
        //     }
        //     catch(std::runtime_error& mError) {
        //         lua.executeCode(cmdString + "\n");
        //     }
        // }
        // catch(std::runtime_error& mError) {
        //     std::string temp = "[error]: ";
        //     temp += mError.what();
        //     temp += '\n';
        //     ilcCmdLog.emplace_back(temp);
        // }
        // catch(...) {
        //     ilcCmdLog.emplace_back("[error]: unknown\n");
        // }
        // ```
        match host.execute_code(&format!("u_log({cmd})\n")) {
            Ok(()) => {}
            Err(ScriptError::Runtime(_)) => {
                // Inner `catch(std::runtime_error&)`: retry bare `<cmd>`.
                match host.execute_code(&format!("{cmd}\n")) {
                    Ok(()) => {}
                    Err(ScriptError::Runtime(what)) => {
                        // Outer `catch(std::runtime_error&)` — the RETRY's
                        // `what()`.
                        shared.ilc_cmd_log.push(format!("[error]: {what}\n"));
                    }
                    Err(ScriptError::Unknown) => {
                        // Outer `catch(...)` — the retry threw non-runtime.
                        shared.ilc_cmd_log.push("[error]: unknown\n".to_string());
                    }
                }
            }
            Err(ScriptError::Unknown) => {
                // The inner catch only binds `std::runtime_error`; a
                // non-runtime throw from the FIRST attempt jumps straight to
                // the outer `catch(...)`.
                shared.ilc_cmd_log.push("[error]: unknown\n".to_string());
            }
        }
    }
}

/// C++ `!HELP`'s log entry (`HGUpdate.cpp:1454-1461`'s `R"(...)"`), verbatim.
const HELP_TEXT: &str = "Built-in commands:\n\
!clear          Clears the console\n\
!help           Display this help\n\
!ff <seconds>   Fast-forward simulation to specified time\n\
!advt <ticks>   Advance simulation by specified number of ticks\n\
?fn             Display Lua docs for function `fn`\n";

/// C++ `postUpdate_ImguiLuaConsole`'s Track-`InputText` Enter branch
/// (`HGUpdate.cpp:1562-1572`): LR-trim `ilcTrackBuffer`, record the
/// `u_impl_addTrackedResult(...)` snippet + the display name, clear the
/// buffer. (The C++ records even an empty trimmed string — an empty snippet
/// then fails at execution time and is erased by
/// [`update_tracked_results`].)
pub fn track_code(est: &mut EngineState) {
    // C++ `Utils::getLRTrim(ilcTrackBuffer)` (C-locale `isspace` trim =
    // ASCII whitespace).
    let code_to_track = est.ilc_track_buffer.trim();
    est.ilc_lua_tracked
        .push(format!("u_impl_addTrackedResult({code_to_track})\n"));
    est.ilc_lua_tracked_names.push(code_to_track.to_string());
    est.ilc_track_buffer.clear();
}

/// C++ "Untrack All" button (`HGUpdate.cpp:1577-1581`).
pub fn untrack_all(est: &mut EngineState) {
    est.ilc_lua_tracked.clear();
    est.ilc_lua_tracked_names.clear();
}

/// C++ tracked-results loop (`HGUpdate.cpp:1584-1621`): clear
/// `ilcLuaTrackedResults`, execute each tracked snippet; on the FIRST error
/// log the exact line, erase the snippet + name pair, and stop.
///
/// The C++ tail (`if(!problem)` → the results table) is the step (b) UI; the
/// port keeps the C++ `SSVOH_ASSERT`s as `debug_assert`s over the gathered
/// state.
pub fn update_tracked_results(
    shared: &mut SharedState,
    est: &mut EngineState,
    host: &dyn ScriptHost,
) {
    if est.ilc_lua_tracked.is_empty() {
        return;
    }

    shared.ilc_lua_tracked_results.clear();
    let mut problem = false;

    for i in 0..est.ilc_lua_tracked.len() {
        let code = est.ilc_lua_tracked[i].clone();

        match host.execute_code(&code) {
            Ok(()) => {}
            Err(ScriptError::Runtime(what)) => {
                // C++ `catch(std::runtime_error& e)` — `concat("[error]:
                // error '", e.what(), "' while tracking ", code, '\n')`.
                shared
                    .ilc_cmd_log
                    .push(format!("[error]: error '{what}' while tracking {code}\n"));
                est.ilc_lua_tracked.remove(i);
                est.ilc_lua_tracked_names.remove(i);
                problem = true;
                break;
            }
            Err(ScriptError::Unknown) => {
                // C++ `catch(...)` — `concat("[error]: unknown error while
                // tracking ", code, '\n')`.
                shared
                    .ilc_cmd_log
                    .push(format!("[error]: unknown error while tracking {code}\n"));
                est.ilc_lua_tracked.remove(i);
                est.ilc_lua_tracked_names.remove(i);
                problem = true;
                break;
            }
        }
    }

    if !problem {
        // C++ `SSVOH_ASSERT(ilcLuaTracked.size() == ilcLuaTrackedNames.size())`
        // + the results-size assert (`HGUpdate.cpp:1623-1624`).
        debug_assert_eq!(est.ilc_lua_tracked.len(), est.ilc_lua_tracked_names.len());
        debug_assert_eq!(
            est.ilc_lua_tracked.len(),
            shared.ilc_lua_tracked_results.len()
        );
    }
}

// ---------------------------------------------------------------------------
// Engine methods (the C++ call sites this crate owns headlessly)
// ---------------------------------------------------------------------------

impl<S: SharedStore> HexagonGame<S> {
    /// C++ `postUpdate_ImguiLuaConsole`'s command block
    /// (`HGUpdate.cpp:1427-1545`) — the step (b) UI's `InputText("Command")`
    /// Enter will call this with its buffer content. `cmd` is the C++
    /// `cmdString`; `ilc_cmd_buffer` is cleared exactly as the C++ clears
    /// `ilcCmdBuffer`.
    pub fn post_update_imgui_lua_console_command(&self, cmd: &str) {
        let host: &dyn ScriptHost = &*self.0.script_host;
        let mut g = self.st();
        let mut e = self.est();
        // C++ `const std::string cmdString = ilcCmdBuffer; ilcCmdBuffer.clear();`
        e.ilc_cmd_buffer.clear();
        interpret_command(&mut g, &mut e, cmd, host);
    }

    /// C++ `ilcTextEditCallback`'s completion case — the step (b) UI's
    /// `CallbackCompletion` will call this with the widget's cursor.
    /// Applies the buffer ops to `ilc_cmd_buffer`, appends the C++
    /// `ilcCmdLog` lines; returns the post-edit cursor (the imgui rules in
    /// [`IlcEditEffect::apply`]).
    #[must_use]
    pub fn ilc_complete(&self, cursor: usize) -> usize {
        let host: &dyn ScriptHost = &*self.0.script_host;
        let names = host.all_function_names();
        let buffer = self.est().ilc_cmd_buffer.clone();
        let effect = complete(&buffer, cursor, &names);
        let mut c = cursor;
        let mut e = self.est();
        effect.apply(&mut e.ilc_cmd_buffer, &mut c);
        for line in &effect.log_lines {
            self.st().ilc_cmd_log.push(line.clone());
        }
        c
    }

    /// C++ `ilcTextEditCallback`'s history case — Up/Down on the widget.
    /// Replaces `ilc_cmd_buffer` (the whole C++ `BufTextLen`) with the
    /// history entry (or clears it at the sentinel); returns the post-edit
    /// cursor.
    #[must_use]
    pub fn ilc_history_key(&self, key: IlcEventKey) -> usize {
        let (new_pos, effect) = {
            let e = self.est();
            history(
                &e.ilc_history,
                e.ilc_history_pos,
                key,
                e.ilc_cmd_buffer.len(),
            )
        };
        let mut e = self.est();
        e.ilc_history_pos = new_pos;
        let mut cursor = 0;
        effect.apply(&mut e.ilc_cmd_buffer, &mut cursor);
        cursor
    }

    /// C++ Track `InputText` Enter (`HGUpdate.cpp:1562-1572`).
    pub fn ilc_track(&self) {
        let mut e = self.est();
        track_code(&mut e);
    }

    /// The C++ Track `InputText` buffer (`ilcTrackBuffer`): the step (b) UI
    /// writes the widget text here before calling [`Self::ilc_track`] (the
    /// C++ `InputText("Track")` Enter branch reads the member).
    pub fn ilc_set_track_buffer(&self, s: &str) {
        self.est().ilc_track_buffer = s.to_string();
    }

    /// C++ "Untrack All" button (`HGUpdate.cpp:1577-1581`).
    pub fn ilc_untrack_all(&self) {
        let mut e = self.est();
        untrack_all(&mut e);
    }

    /// C++ tracked-results loop (`HGUpdate.cpp:1584-1621`) — the step (b)
    /// UI runs it each frame the console is shown; headless it is callable
    /// directly.
    pub fn ilc_update_tracked_results(&self) {
        let host: &dyn ScriptHost = &*self.0.script_host;
        let mut g = self.st();
        let mut e = self.est();
        update_tracked_results(&mut g, &mut e, host);
    }

    /// C++ `HexagonGame::initLuaAndPrintDocs` (`HGScripting.cpp:1149-1153`):
    /// `initLua(); LuaScripting::printDocs();` — the `-printLuaDocs`
    /// entry's engine half (the CLI flag itself is issue 15's `oh-app`).
    /// The C++ writes `std::cout`; the testable writer form is
    /// [`Self::init_lua_and_print_docs_to`].
    pub fn init_lua_and_print_docs(&self) {
        self.0.script_host.init_lua();
        let mut stdout = std::io::stdout().lock();
        if let Err(e) = self.0.script_host.print_docs(&mut stdout) {
            // The C++ has no error path here (`std::cout` failbit); surface
            // it on stderr rather than failing silently.
            eprintln!("printDocs failed: {e}");
        }
    }

    /// The writer form of [`Self::init_lua_and_print_docs`] (testability —
    /// `mainPrintLuaDocs`, `main.cpp:167-189`, is mirrored by the headless
    /// test over a `Vec<u8>`).
    pub fn init_lua_and_print_docs_to(&self, out: &mut dyn Write) -> std::io::Result<()> {
        self.0.script_host.init_lua();
        self.0.script_host.print_docs(out)
    }

    /// C++ `Tid::LuaConsole` input trigger (`HexagonGame.cpp:412-419`): the
    /// debug-gated `ilcShowConsoleNext = true`. The step (b) input wiring
    /// (issue 15) calls this; no input system is built here.
    pub fn set_ilc_show_console_next(&self) {
        if self.config().get_debug() {
            self.est().ilc_show_console_next = true;
        }
    }

    // -- state readers (the step (b) UI + the headless tests) -------------

    /// C++ `ilcShowConsole`.
    #[must_use]
    pub fn ilc_show_console(&self) -> bool {
        self.est().ilc_show_console
    }

    /// C++ `ilcShowConsoleNext` (the pending toggle).
    #[must_use]
    pub fn ilc_show_console_next(&self) -> bool {
        self.est().ilc_show_console_next
    }

    /// C++ `ilcHistory` (snapshot).
    #[must_use]
    pub fn ilc_history(&self) -> Vec<String> {
        self.est().ilc_history.clone()
    }

    /// C++ `ilcHistoryPos`.
    #[must_use]
    pub fn ilc_history_pos(&self) -> i32 {
        self.est().ilc_history_pos
    }

    /// C++ `ilcCmdBuffer` (the pending command text).
    #[must_use]
    pub fn ilc_cmd_buffer(&self) -> String {
        self.est().ilc_cmd_buffer.clone()
    }

    /// C++ `ilcLuaTracked` (snapshot).
    #[must_use]
    pub fn ilc_lua_tracked(&self) -> Vec<String> {
        self.est().ilc_lua_tracked.clone()
    }

    /// C++ `ilcLuaTrackedNames` (snapshot).
    #[must_use]
    pub fn ilc_lua_tracked_names(&self) -> Vec<String> {
        self.est().ilc_lua_tracked_names.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use oh_data::{MusicData, PackData, StyleData};
    use oh_math::Vec2;
    use oh_rng::{Pcg32Fast, RandomNumberGenerator};

    use crate::hexagon_game::{EffectTimelineManager, EngineState};
    use crate::lua_state::SharedState;
    use crate::timeline::{Timeline2, Timeline2Runner};
    use crate::vertex_batch::FastVertexVectorTris;
    use crate::{CPlayer, HexagonGameStatus, LevelStatus};

    /// A constructed `EngineState` with every field at its C++ initial value
    /// (the `HexagonGame::new` literal, `hexagon_game.rs` — test stand-in).
    fn engine_state() -> EngineState {
        EngineState {
            pack_id: String::new(),
            level_id: String::new(),
            level_data: None,
            player: CPlayer::new(&Vec2::new(0.0, 0.0), 36.0, 22.0, 1.0, 1.0),
            background_camera: None,
            overlay_camera: None,
            pre_shake_centers: None,
            effect_timeline_manager: EffectTimelineManager::new(),
            flash_polygon: FastVertexVectorTris::new(),
            message_timeline: Timeline2::new(),
            message_timeline_runner: Timeline2Runner::new(),
            main_timeline_runner: Timeline2Runner::new(),
            event_timeline_runner: Timeline2Runner::new(),
            particles: Vec::new(),
            trail_particles: Vec::new(),
            swap_particles: Vec::new(),
            must_spawn_pb_particles: false,
            swap_particles_spawn_info: None,
            next_pb_particle_spawn: 0.0,
            pb_text_growth: 0.0,
            message_text: String::new(),
            pb_text: String::new(),
            status_text: String::new(),
            fps_text: String::new(),
            time_text: String::new(),
            replay_text: String::new(),
            must_change_sides: false,
            must_start: false,
            must_take_screenshot: false,
            input_focused: false,
            input_swap: false,
            death_input_ignore: 0.0,
            active_replay: None,
            last_seed: 0,
            last_replay_data: oh_replay::ReplayData::default(),
            last_first_play: false,
            last_played_score: 0.0,
            restart_id: String::new(),
            restart_first_time: true,
            input_impl_last_movement: 0,
            input_movement: 0,
            input_impl_cw: false,
            input_impl_ccw: false,
            player_now_ready_to_swap: false,
            execute_random_inputs: false,
            always_spin_right: false,
            debug_pause: false,
            ilc_show_console: false,
            ilc_delta_clock: std::time::Instant::now(),
            ilc_history: Vec::new(),
            ilc_history_pos: -1,
            ilc_cmd_buffer: String::new(),
            ilc_track_buffer: String::new(),
            ilc_show_console_next: false,
            ilc_lua_tracked: Vec::new(),
            ilc_lua_tracked_names: Vec::new(),
            random_input_rng: Pcg32Fast::from_seed(0),
            fast_forward_target: None,
            advance_tick_count: None,
            time_until_rich_presence_update: 0.0,
            // C++ `discordAttempt{1}` / `discordHung{false}` (HexagonGame.hpp:91,89).
            discord_hung: false,
            discord_attempt: 1,
        }
    }

    fn shared_state() -> SharedState {
        SharedState::new(
            RandomNumberGenerator::new(0),
            LevelStatus::new(false, 1600.0),
            HexagonGameStatus::new(),
            StyleData::default(),
            MusicData::default(),
            false,
            true,
            "packs/base".into(),
            "level".into(),
            "pack".into(),
            PackData {
                folder_path: "packs/base/".into(),
                id: "pack".into(),
                disambiguator: "test".into(),
                name: "base".into(),
                author: "tester".into(),
                description: String::new(),
                version: 1,
                priority: 100.0,
                dependencies: Vec::new(),
            },
        )
    }

    /// The recording `ScriptHost` (unit tests): canned docs/names + a queue
    /// of `execute_code` results + the executed codes.
    struct RecHost {
        inner: Mutex<Inner>,
    }
    struct Inner {
        names: Vec<String>,
        docs: std::collections::HashMap<String, String>,
        execute_results: Vec<Result<(), ScriptError>>,
        executed: Vec<String>,
    }
    impl RecHost {
        #[must_use]
        fn new() -> Self {
            Self {
                inner: Mutex::new(Inner {
                    names: Vec::new(),
                    docs: std::collections::HashMap::new(),
                    execute_results: Vec::new(),
                    executed: Vec::new(),
                }),
            }
        }
        fn set_names(&self, names: Vec<String>) {
            self.inner.lock().expect("mock poisoned").names = names;
        }
        fn set_doc(&self, name: &str, doc: &str) {
            self.inner
                .lock()
                .expect("mock poisoned")
                .docs
                .insert(name.to_string(), doc.to_string());
        }
        fn push_result(&self, r: Result<(), ScriptError>) {
            self.inner
                .lock()
                .expect("mock poisoned")
                .execute_results
                .push(r);
        }
        fn executed(&self) -> Vec<String> {
            self.inner.lock().expect("mock poisoned").executed.clone()
        }
    }
    impl Default for RecHost {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ScriptHost for RecHost {
        fn init_lua(&self) {}
        fn run_lua_file(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn run_on_init(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_load(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_update(&self, _: f32) -> Result<(), String> {
            Ok(())
        }
        fn run_on_step(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_input(&self, _: f32, _: i32, _: bool, _: bool) -> Result<Option<bool>, String> {
            Ok(None)
        }
        fn run_on_pre_death(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_death(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_increment(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_pre_unload(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_unload(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_cursor_swap(&self) -> Result<(), String> {
            Ok(())
        }
        fn run_on_render_stage(&self, _: i32, _: f32) -> Result<(), String> {
            Ok(())
        }
        fn read_variable_f32(&self, _: &str) -> Result<f32, String> {
            Err("no".into())
        }
        fn variable_exists(&self, _: &str) -> bool {
            false
        }
        fn read_variable_string(&self, _: &str) -> Result<String, String> {
            Err("no".into())
        }
        fn execute_code(&self, code: &str) -> Result<(), ScriptError> {
            let mut inner = self.inner.lock().expect("mock poisoned");
            let idx = inner.executed.len();
            inner.executed.push(code.to_string());
            inner.execute_results.get(idx).cloned().unwrap_or(Ok(()))
        }
        fn all_function_names(&self) -> Vec<String> {
            self.inner.lock().expect("mock poisoned").names.clone()
        }
        fn docs_for_function(&self, name: &str) -> String {
            self.inner
                .lock()
                .expect("mock poisoned")
                .docs
                .get(name)
                .cloned()
                .unwrap_or_else(|| "UNKNOWN FUNCTION".to_string())
        }
        fn print_docs(&self, _: &mut dyn std::io::Write) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run_cmd(host: &RecHost, shared: &mut SharedState, est: &mut EngineState, cmd: &str) {
        interpret_command(shared, est, cmd, host);
    }

    // -- !CLEAR / !HELP -----------------------------------------------------

    #[test]
    fn clear_command_wipes_the_log_including_its_own_echo() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        shared.ilc_cmd_log.push("earlier line\n".into());
        run_cmd(&host, &mut shared, &mut est, "!CLEAR");
        // C++: the `# !CLEAR` echo is pushed FIRST, then the `!CLEAR`
        // branch clears the whole log — the log is EMPTY afterwards.
        assert!(shared.ilc_cmd_log.is_empty());
        // The history still records it (push precedes the dispatch).
        assert_eq!(est.ilc_history, vec!["!CLEAR".to_string()]);
    }

    #[test]
    fn clear_is_case_insensitive() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        shared.ilc_cmd_log.push("x\n".into());
        run_cmd(&host, &mut shared, &mut est, "!cLeAr");
        assert!(shared.ilc_cmd_log.is_empty());
    }

    #[test]
    fn help_command_appends_the_verbatim_block() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!HELP");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !HELP\n".to_string(),
                "Built-in commands:\n\
                 !clear          Clears the console\n\
                 !help           Display this help\n\
                 !ff <seconds>   Fast-forward simulation to specified time\n\
                 !advt <ticks>   Advance simulation by specified number of ticks\n\
                 ?fn             Display Lua docs for function `fn`\n"
                    .to_string(),
            ]
        );
    }

    // -- !ff ----------------------------------------------------------------

    #[test]
    fn ff_command_logs_ostream_double_and_sets_target() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff 2.5");
        // C++ `concat` → `std::ostream` default format (6 significant
        // digits, `%g`): 2.5 → "2.5".
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !ff 2.5\n".to_string(),
                "[ff]: fast forwarding to 2.5\n".to_string()
            ]
        );
        assert_eq!(est.fast_forward_target, Some(2.5));
    }

    #[test]
    fn ff_integer_seconds_format_like_ostream() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff 2");
        // C++ `oss << 2.0` → "2" (NOT "2.0").
        assert_eq!(
            shared.ilc_cmd_log.last().expect("log line"),
            "[ff]: fast forwarding to 2\n"
        );
        assert_eq!(est.fast_forward_target, Some(2.0));
    }

    #[test]
    fn ff_large_value_formats_exponentially_like_ostream() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff 1e20");
        // C++ `oss << 1e20` → "1e+20" (glibc 6-digit `%g`).
        assert_eq!(
            shared.ilc_cmd_log.last().expect("log line"),
            "[ff]: fast forwarding to 1e+20\n"
        );
    }

    #[test]
    fn ff_invalid_argument_error_string() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff abc");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !ff abc\n".to_string(),
                "[error]: invalid argument for <seconds>\n".to_string()
            ]
        );
        assert_eq!(est.fast_forward_target, None);
    }

    #[test]
    fn ff_out_of_range_error_string() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff 1e400");
        // libstdc++ `stod("1e400")` throws `out_of_range` (probe-pinned).
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !ff 1e400\n".to_string(),
                "[error]: out of range for <seconds>\n".to_string()
            ]
        );
        assert_eq!(est.fast_forward_target, None);

        // Underflow to (inexact) subnormal also throws (probe-pinned).
        let mut est2 = engine_state();
        let mut shared2 = shared_state();
        run_cmd(&host, &mut shared2, &mut est2, "!ff 1e-320");
        assert_eq!(
            shared2.ilc_cmd_log.last().expect("log line"),
            "[error]: out of range for <seconds>\n"
        );
        assert_eq!(est2.fast_forward_target, None);
    }

    #[test]
    fn ff_without_argument_falls_to_the_lua_fallback() {
        // `cmdSplit.size() > 1` is false for a bare `!ff` (one token) — the
        // C++ dispatch does NOT match the `!ff` branch; it goes to the
        // fallback (executed as `u_log(!ff)\n` then `!ff\n`).
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Runtime("e1".into())));
        host.push_result(Err(ScriptError::Runtime(
            "attempt to do something (!ff)".into(),
        )));
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!ff");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !ff\n".to_string(),
                "[error]: attempt to do something (!ff)\n".to_string()
            ]
        );
        assert_eq!(est.fast_forward_target, None);
        assert_eq!(
            host.executed(),
            vec!["u_log(!ff)\n".to_string(), "!ff\n".to_string()]
        );
    }

    // -- !advt --------------------------------------------------------------

    #[test]
    fn advt_command_logs_and_sets_tick_count() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!advt 7");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !advt 7\n".to_string(),
                "[advt]: advancing simulation by 7 ticks\n".to_string()
            ]
        );
        assert_eq!(est.advance_tick_count, Some(7));
    }

    #[test]
    fn advt_negative_ticks_clamp_to_zero() {
        // C++ `:1489-1494` — the LOG uses the raw parsed `ticks` (the
        // emplace precedes the clamp line), only the STATE is clamped:
        // `advanceTickCount = ticks >= 0 ? ticks : 0;`.
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!advt -3");
        assert_eq!(
            shared.ilc_cmd_log.last().expect("log line"),
            "[advt]: advancing simulation by -3 ticks\n"
        );
        assert_eq!(est.advance_tick_count, Some(0));
    }

    #[test]
    fn advt_invalid_argument_reuses_the_seconds_wording() {
        // C++ `:1499` — the `!advt` branch reuses the `<seconds>` error
        // string (a C++ copy/paste quirk, kept verbatim).
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!advt abc");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !advt abc\n".to_string(),
                "[error]: invalid argument for <seconds>\n".to_string()
            ]
        );
        assert_eq!(est.advance_tick_count, None);
    }

    #[test]
    fn advt_out_of_range_reuses_the_seconds_wording() {
        // C++ `:1503` + libstdc++ `stoi("2147483648")` throws (probe-pinned).
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!advt 2147483648");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# !advt 2147483648\n".to_string(),
                "[error]: out of range for <seconds>\n".to_string()
            ]
        );
        assert_eq!(est.advance_tick_count, None);
    }

    #[test]
    fn advt_min_int_parses() {
        // libstdc++ `stoi("-2147483648")` is in range (probe-pinned); the
        // clamp still yields 0.
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!advt -2147483648");
        assert_eq!(
            shared.ilc_cmd_log.last().expect("log line"),
            "[advt]: advancing simulation by -2147483648 ticks\n"
        );
        assert_eq!(est.advance_tick_count, Some(0));
    }

    // -- ?fn ----------------------------------------------------------------

    #[test]
    fn question_fn_command_logs_docs() {
        let host = RecHost::new();
        host.set_doc(
            "u_rndReal",
            "float u_rndReal(int a, int b):\nReturns a random value in `[a; b]`.\n\n",
        );
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "?u_rndReal");
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# ?u_rndReal\n".to_string(),
                "[?]: float u_rndReal(int a, int b):\n\
                 Returns a random value in `[a; b]`.\n\n"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn question_fn_unknown_function_keeps_the_cpp_fallback() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "?noSuchFn");
        // C++ `getDocsForFunction` returns the bare string (no trailing
        // newline) → the log line carries none.
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# ?noSuchFn\n".to_string(),
                "[?]: UNKNOWN FUNCTION".to_string()
            ]
        );
    }

    #[test]
    fn question_fn_rtrims_the_name() {
        // C++ `Utils::getRTrim(cmdString.substr(1))`.
        let host = RecHost::new();
        host.set_doc("u_log", "the docs\n\n");
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "?u_log   ");
        assert_eq!(
            shared.ilc_cmd_log.last().expect("log line"),
            "[?]: the docs\n\n"
        );
    }

    // -- the u_log fallback --------------------------------------------------

    #[test]
    fn fallback_u_log_success_logs_nothing_else() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "u_log('hi')");
        assert_eq!(shared.ilc_cmd_log, vec!["# u_log('hi')\n".to_string()]);
        assert_eq!(host.executed(), vec!["u_log(u_log('hi'))\n".to_string()]);
    }

    #[test]
    fn fallback_retries_bare_on_runtime_error() {
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Runtime(
            "attempt to call a nil value (global 'u_log')".into(),
        )));
        host.push_result(Ok(()));
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "x = 1");
        // The retry succeeded → no error line.
        assert_eq!(shared.ilc_cmd_log, vec!["# x = 1\n".to_string()]);
        assert_eq!(
            host.executed(),
            vec!["u_log(x = 1)\n".to_string(), "x = 1\n".to_string()]
        );
    }

    #[test]
    fn fallback_double_error_logs_the_retry_what() {
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Runtime("first".into())));
        host.push_result(Err(ScriptError::Runtime(
            "attempt to perform arithmetic on a nil value".into(),
        )));
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "1 + y");
        // The outer catch carries the SECOND (retry) exception's `what()`.
        assert_eq!(
            shared.ilc_cmd_log,
            vec![
                "# 1 + y\n".to_string(),
                "[error]: attempt to perform arithmetic on a nil value\n".to_string()
            ]
        );
    }

    #[test]
    fn fallback_unknown_short_circuits_to_unknown() {
        // The inner catch binds `std::runtime_error` only: a non-runtime
        // throw from the FIRST attempt goes straight to the outer
        // `catch(...)` — the retry is NEVER executed.
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Unknown));
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "boom");
        assert_eq!(
            shared.ilc_cmd_log,
            vec!["# boom\n".to_string(), "[error]: unknown\n".to_string()]
        );
        assert_eq!(host.executed(), vec!["u_log(boom)\n".to_string()]);
    }

    // -- history bookkeeping -------------------------------------------------

    #[test]
    fn history_dedupes_the_last_duplicate() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "!HELP");
        run_cmd(&host, &mut shared, &mut est, "!ff 2");
        run_cmd(&host, &mut shared, &mut est, "!HELP");
        // The first `!HELP` was erased (last-match scan) and re-pushed.
        assert_eq!(
            est.ilc_history,
            vec!["!ff 2".to_string(), "!HELP".to_string()]
        );
    }

    #[test]
    fn every_command_resets_history_pos() {
        let host = RecHost::new();
        let mut shared = shared_state();
        let mut est = engine_state();
        est.ilc_history_pos = 3;
        run_cmd(&host, &mut shared, &mut est, "!HELP");
        assert_eq!(est.ilc_history_pos, -1);
    }

    #[test]
    fn empty_command_echoes_and_runs_the_fallback() {
        // C++ `cmdString[0]` on an empty string is `'\0'` → the fallback
        // (`u_log()` errors, the bare `"\n"` executes fine) → no error line.
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Runtime("not enough parameters".into())));
        let mut shared = shared_state();
        let mut est = engine_state();
        run_cmd(&host, &mut shared, &mut est, "");
        assert_eq!(shared.ilc_cmd_log, vec!["# \n".to_string()]);
        assert_eq!(est.ilc_history, vec![String::new()]);
    }

    // -- completion ----------------------------------------------------------

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn completion_single_match_replaces_with_nice_casing_and_space() {
        let buf = "u_l";
        let effect = complete(buf, 3, &names(&["u_log", "a_playSound"]));
        assert!(effect.log_lines.is_empty());
        assert_eq!(
            effect.ops,
            vec![
                BufferOp::DeleteChars { start: 0, count: 3 },
                BufferOp::InsertChars {
                    pos: 0,
                    text: "u_log".to_string()
                },
                BufferOp::InsertChars {
                    pos: 5,
                    text: " ".to_string()
                }
            ]
        );
        let mut out = buf.to_string();
        let mut cursor = 3;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "u_log ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn completion_is_case_insensitive() {
        let effect = complete("U_LOG", 5, &names(&["u_log"]));
        assert!(matches!(
            effect.ops.first(),
            Some(BufferOp::DeleteChars { start: 0, count: 5 })
        ));
        let mut out = "U_LOG".to_string();
        let mut cursor = 5;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "u_log ");
    }

    #[test]
    fn completion_word_scan_stops_at_delimiters() {
        // A word started after `' '` (the preceding `a_b` must not match).
        let buf = "a_b u_l";
        let effect = complete(buf, 7, &names(&["u_log"]));
        assert_eq!(
            effect.ops,
            vec![
                BufferOp::DeleteChars { start: 4, count: 3 },
                BufferOp::InsertChars {
                    pos: 4,
                    text: "u_log".to_string()
                },
                BufferOp::InsertChars {
                    pos: 9,
                    text: " ".to_string()
                }
            ]
        );
        let mut out = buf.to_string();
        let mut cursor = 7;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "a_b u_log ");
    }

    #[test]
    fn completion_skips_a_leading_question_mark() {
        // `?u_l` completes the `u_l` word (the `?` is not part of it).
        let buf = "?u_l";
        let effect = complete(buf, 4, &names(&["u_log"]));
        assert_eq!(
            effect.ops,
            vec![
                BufferOp::DeleteChars { start: 1, count: 3 },
                BufferOp::InsertChars {
                    pos: 1,
                    text: "u_log".to_string()
                },
                BufferOp::InsertChars {
                    pos: 6,
                    text: " ".to_string()
                }
            ]
        );
        let mut out = buf.to_string();
        let mut cursor = 4;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "?u_log ");
    }

    #[test]
    fn completion_no_match_logs_the_word() {
        let effect = complete("zzz", 3, &names(&["u_log"]));
        assert!(effect.ops.is_empty());
        assert_eq!(effect.log_lines, vec!["No match for \"zzz\"!\n"]);
    }

    #[test]
    fn completion_many_matches_complete_lcp_and_list() {
        // The C++'s own example: input "c"+Tab over CLEAR/CLASSIFY-style
        // names → the shared prefix is inserted, then the list.
        let buf = "u_";
        let effect = complete(buf, 2, &names(&["u_clear", "u_classify", "u_countdown"]));
        // LCP of {u_clear, u_classify, u_countdown} past "u_": 'c' then
        // 'l' vs 'o' diverge → match_len = 3 → "u_c".
        assert_eq!(
            effect.ops,
            vec![
                BufferOp::DeleteChars { start: 0, count: 2 },
                BufferOp::InsertChars {
                    pos: 0,
                    text: "u_c".to_string()
                }
            ]
        );
        assert_eq!(
            effect.log_lines,
            vec![
                "Possible matches:\n",
                "- u_clear\n",
                "- u_classify\n",
                "- u_countdown\n"
            ]
        );
        let mut out = buf.to_string();
        let mut cursor = 2;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "u_c");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn completion_many_matches_zero_lcp_lists_only() {
        // Divergent first char → `match_len` stays at the word length → no
        // delete/insert, only the list.
        let effect = complete("", 0, &names(&["u_log", "a_play"]));
        assert!(effect.ops.is_empty());
        assert_eq!(
            effect.log_lines,
            vec!["Possible matches:\n", "- u_log\n", "- a_play\n"]
        );
    }

    #[test]
    fn completion_prefix_candidate_stops_the_lcp_scan() {
        // One candidate IS the other's prefix: the scan stops at the
        // shorter one's `'\0'` (`c == 0`), so no chars are ADDED — but C++
        // `if (match_len > 0)` still re-deletes/re-inserts the word in the
        // first candidate's casing (`match_len` starts at the word length).
        let effect = complete("U_LOG", 5, &names(&["u_log", "u_log2"]));
        assert_eq!(
            effect.ops,
            vec![
                BufferOp::DeleteChars { start: 0, count: 5 },
                BufferOp::InsertChars {
                    pos: 0,
                    text: "u_log".to_string()
                }
            ]
        );
        assert_eq!(
            effect.log_lines,
            vec!["Possible matches:\n", "- u_log\n", "- u_log2\n"]
        );
        let mut out = "U_LOG".to_string();
        let mut cursor = 5;
        effect.apply(&mut out, &mut cursor);
        assert_eq!(out, "u_log");
    }

    // -- history navigation --------------------------------------------------

    #[test]
    fn history_up_from_sentinel_jumps_to_last() {
        let h = vec!["one".into(), "two".into(), "three".into()];
        let (pos, effect) = history(&h, -1, IlcEventKey::UpArrow, 5);
        assert_eq!(pos, 2);
        let mut buf = "stale".to_string();
        let mut cursor = 5;
        effect.apply(&mut buf, &mut cursor);
        assert_eq!(buf, "three");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn history_up_walks_and_stops_at_zero() {
        let h = vec!["one".into(), "two".into()];
        let (pos1, _) = history(&h, -1, IlcEventKey::UpArrow, 0);
        assert_eq!(pos1, 1);
        let (pos2, _) = history(&h, 1, IlcEventKey::UpArrow, 0);
        assert_eq!(pos2, 0);
        // At 0, Up is a no-op (C++ `else if (ilcHistoryPos > 0)`).
        let (pos3, effect3) = history(&h, 0, IlcEventKey::UpArrow, 0);
        assert_eq!(pos3, 0);
        assert!(effect3.ops.is_empty());
    }

    #[test]
    fn history_down_past_the_end_returns_to_sentinel_and_clears() {
        let h = vec!["one".into()];
        let (pos, effect) = history(&h, 0, IlcEventKey::DownArrow, 3);
        assert_eq!(pos, -1);
        let mut buf = "one".to_string();
        let mut cursor = 3;
        effect.apply(&mut buf, &mut cursor);
        assert_eq!(buf, "");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn history_down_from_sentinel_is_a_no_op() {
        let h = vec!["one".into()];
        let (pos, effect) = history(&h, -1, IlcEventKey::DownArrow, 3);
        assert_eq!(pos, -1);
        assert!(effect.ops.is_empty());
    }

    #[test]
    fn history_up_with_empty_history_is_a_no_op() {
        // C++ `size_t` wrap: `0 - 1` → `-1` (no browse) → unchanged → no
        // buffer mutation.
        let (pos, effect) = history(&[], -1, IlcEventKey::UpArrow, 4);
        assert_eq!(pos, -1);
        assert!(effect.ops.is_empty());
    }

    // -- tracked variables ----------------------------------------------------

    #[test]
    fn track_code_records_snippet_and_name_and_clears_buffer() {
        let mut est = engine_state();
        est.ilc_track_buffer = "  myVar\t ".into();
        track_code(&mut est);
        assert_eq!(
            est.ilc_lua_tracked,
            vec!["u_impl_addTrackedResult(myVar)\n".to_string()]
        );
        assert_eq!(est.ilc_lua_tracked_names, vec!["myVar".to_string()]);
        assert!(est.ilc_track_buffer.is_empty());
    }

    #[test]
    fn untrack_all_clears_both_lists() {
        let mut est = engine_state();
        est.ilc_lua_tracked.push("snippet\n".into());
        est.ilc_lua_tracked_names.push("name".into());
        untrack_all(&mut est);
        assert!(est.ilc_lua_tracked.is_empty());
        assert!(est.ilc_lua_tracked_names.is_empty());
    }

    #[test]
    fn tracked_results_error_erases_pair_and_stops() {
        let host = RecHost::new();
        // snippet 1 fails (runtime); snippet 2 is never executed.
        host.push_result(Err(ScriptError::Runtime(
            "attempt to use a nil value".into(),
        )));
        let mut shared = shared_state();
        let mut est = engine_state();
        est.ilc_lua_tracked
            .push("u_impl_addTrackedResult(a)\n".into());
        est.ilc_lua_tracked
            .push("u_impl_addTrackedResult(b)\n".into());
        est.ilc_lua_tracked_names.push("a".into());
        est.ilc_lua_tracked_names.push("b".into());
        shared.ilc_lua_tracked_results.push("stale".into());

        update_tracked_results(&mut shared, &mut est, &host);

        // The failed pair is erased; the stale result was cleared; the error
        // line is the C++ one (the snippet text, including its `'\n'`, is
        // part of the message — C++ `code` is the stored snippet).
        // The C++ `code` is the stored snippet, which carries its own
        // trailing `\n` (from `concat("u_impl_addTrackedResult(",
        // codeToTrack, ")\n")`) → the log line ends `\n\n`.
        assert_eq!(
            shared.ilc_cmd_log,
            vec!["[error]: error 'attempt to use a nil value' while tracking u_impl_addTrackedResult(a)\n\n"]
        );
        assert_eq!(
            est.ilc_lua_tracked,
            vec!["u_impl_addTrackedResult(b)\n".to_string()]
        );
        assert_eq!(est.ilc_lua_tracked_names, vec!["b".to_string()]);
        assert!(shared.ilc_lua_tracked_results.is_empty());
        assert_eq!(
            host.executed(),
            vec!["u_impl_addTrackedResult(a)\n".to_string()]
        );
    }

    #[test]
    fn tracked_results_unknown_error_string() {
        let host = RecHost::new();
        host.push_result(Err(ScriptError::Unknown));
        let mut shared = shared_state();
        let mut est = engine_state();
        est.ilc_lua_tracked
            .push("u_impl_addTrackedResult(q)\n".into());
        est.ilc_lua_tracked_names.push("q".into());

        update_tracked_results(&mut shared, &mut est, &host);

        assert_eq!(
            shared.ilc_cmd_log,
            vec!["[error]: unknown error while tracking u_impl_addTrackedResult(q)\n\n"]
        );
        assert!(est.ilc_lua_tracked.is_empty());
        assert!(est.ilc_lua_tracked_names.is_empty());
    }

    // (The no-error tracked path — `ilcLuaTrackedResults` gathered by the
    // real `u_impl_addTrackedResult` lambda — is covered by the integration
    // test `tracked_results_gathered_via_the_real_lambda`: the loop's tail
    // `SSVOH_ASSERT`s compare the results vec against the tracked pairs,
    // and only the real lambda (inside `execute_code`) can fill it.)

    #[test]
    fn tracked_results_noop_when_nothing_tracked() {
        let host = RecHost::new();
        let mut shared = shared_state();
        shared.ilc_lua_tracked_results.push("stale".into());
        let mut est = engine_state();
        update_tracked_results(&mut shared, &mut est, &host);
        // C++ `if (ilcLuaTracked.size() > 0)` — the stale result survives.
        assert_eq!(shared.ilc_lua_tracked_results, vec!["stale".to_string()]);
    }

    // -- parse helpers ---------------------------------------------------------

    #[test]
    fn parse_stod_pins_libstdcxx_behaviors() {
        assert_eq!(parse_stod("2.5"), Ok(2.5));
        assert_eq!(parse_stod("  2.5"), Ok(2.5)); // leading whitespace skipped
        assert_eq!(parse_stod("2.5xyz"), Ok(2.5)); // trailing junk (prefix)
        assert_eq!(parse_stod("0e-999"), Ok(0.0)); // zero literal, no throw
        assert_eq!(parse_stod("-0"), Ok(-0.0));
        assert_eq!(parse_stod("inf"), Ok(f64::INFINITY)); // literal inf
        assert!(parse_stod("nan").expect("nan parses").is_nan());
        assert_eq!(parse_stod("1e400"), Err(IlcParseError::OutOfRange)); // overflow
        assert_eq!(parse_stod("1e-320"), Err(IlcParseError::OutOfRange)); // inexact underflow
        assert_eq!(parse_stod("abc"), Err(IlcParseError::InvalidArgument));
        assert_eq!(parse_stod(""), Err(IlcParseError::InvalidArgument));
        assert_eq!(parse_stod("   "), Err(IlcParseError::InvalidArgument));
        // Rust rounds `"1.7976931348623158e308"` to `DBL_MAX` (probe-pinned,
        // exactly like glibc — no `ERANGE` at that boundary); a truly
        // beyond-DBL_MAX value (`"1e309"`) rounds to inf in both → throw.
        assert_eq!(parse_stod("1.7976931348623158e308"), Ok(f64::MAX));
        assert_eq!(parse_stod("1e309"), Err(IlcParseError::OutOfRange));
        // Documented divergence: C++ hex floats (`stod("0x1p3")` → 8) are
        // `invalid_argument` in the port (never silently 0).
        assert_eq!(parse_stod("0x1p3"), Err(IlcParseError::InvalidArgument));
    }

    // -- engine glue (bare `HexagonGame<SharedState>` + the seams) -------

    use oh_config::Config;
    use std::sync::{Arc, RwLock};

    use crate::hexagon_game::HexagonGame;
    use crate::lua_state::ReentrantMutex;
    use crate::seams::{NoJoystick, NoneSteamSink, NullProfiles, NullReplayStore, PcgCosmeticRng};

    /// A bare headless engine (no Lua host: `NoScriptHost` stands in the
    /// C++ engine's always-present `Lua::LuaContext`).
    fn bare_engine(host: Arc<dyn ScriptHost>) -> HexagonGame {
        let config = Arc::new(RwLock::new(Config::default()));
        let assets = oh_assets::Assets::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE"),
            true,
            true,
        )
        .expect("_RELEASE tree");
        let game = HexagonGame::new(
            Arc::new(NoneSteamSink),
            config,
            Arc::new(assets),
            None,
            Arc::new(NullProfiles),
            Arc::new(NullReplayStore),
            host,
            Arc::new(NoJoystick),
            Arc::new(PcgCosmeticRng::new()),
            false,
            None, // C++ headless tests: no renderer
            None, // C++ headless tests: no Discord manager
        );
        let state = Arc::new(ReentrantMutex::new(shared_state()));
        game.attach_state(state);
        game
    }

    #[test]
    fn engine_command_method_clears_buffer_and_records() {
        let game = bare_engine(Arc::new(RecHost::new()));
        // Pre-seed the buffer (the step (b) UI would have typed into it).
        {
            let mut e = game.est();
            e.ilc_cmd_buffer = "typed".into();
        }
        game.post_update_imgui_lua_console_command("!HELP");
        assert_eq!(game.ilc_cmd_buffer(), "");
        assert_eq!(game.ilc_history(), vec!["!HELP".to_string()]);
        assert_eq!(
            game.state_handle().lock().ilc_cmd_log,
            vec!["# !HELP\n".to_string(), HELP_TEXT.to_string()]
        );
    }

    #[test]
    fn engine_completion_method_applies_to_the_buffer() {
        // Two candidates → the C++ many-match branch: LCP re-insert (no
        // trailing space — that belongs to the single-match branch) + the
        // `Possible matches` log lines.
        let host = RecHost::new();
        host.set_names(vec!["u_log".to_string(), "u_log2".to_string()]);
        let game = bare_engine(Arc::new(host));
        {
            let mut e = game.est();
            e.ilc_cmd_buffer = "?u_l".into();
        }
        let cursor = game.ilc_complete(4);
        // Cursor: 4 -delete(1,3)→ 1, +insert "u_log"@1 → 6 (end of "?u_log").
        assert_eq!(game.ilc_cmd_buffer(), "?u_log");
        assert_eq!(cursor, 6);
        assert_eq!(
            game.state_handle().lock().ilc_cmd_log,
            vec![
                "Possible matches:\n".to_string(),
                "- u_log\n".to_string(),
                "- u_log2\n".to_string(),
            ]
        );
    }

    #[test]
    fn engine_completion_single_match_adds_the_trailing_space() {
        let host = RecHost::new();
        host.set_names(vec!["u_log".to_string()]);
        let game = bare_engine(Arc::new(host));
        {
            let mut e = game.est();
            e.ilc_cmd_buffer = "u_l".into();
        }
        let cursor = game.ilc_complete(3);
        assert_eq!(game.ilc_cmd_buffer(), "u_log ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn engine_history_key_walks_the_history() {
        let game = bare_engine(Arc::new(RecHost::new()));
        game.post_update_imgui_lua_console_command("!ff 2");
        game.post_update_imgui_lua_console_command("!advt 7");
        // Type into the buffer, then browse.
        {
            let mut e = game.est();
            e.ilc_cmd_buffer = "scratch".into();
        }
        let _ = game.ilc_history_key(IlcEventKey::UpArrow);
        assert_eq!(game.ilc_cmd_buffer(), "!advt 7");
        assert_eq!(game.ilc_history_pos(), 1);
        let _ = game.ilc_history_key(IlcEventKey::DownArrow);
        assert_eq!(game.ilc_cmd_buffer(), "");
        assert_eq!(game.ilc_history_pos(), -1);
    }

    #[test]
    fn set_ilc_show_console_next_is_debug_gated() {
        let game = bare_engine(Arc::new(RecHost::new()));
        // Default config: debug=false → the C++ trigger body does not run.
        game.set_ilc_show_console_next();
        assert!(!game.ilc_show_console_next());

        // debug=true (and NOT official — `get_debug` reports the debug
        // default while official) → `ilcShowConsoleNext = true`.
        {
            let mut c = game.0.config.write().expect("config poisoned");
            c.set_official(false);
            c.set_debug(true);
        }
        game.set_ilc_show_console_next();
        assert!(game.ilc_show_console_next());
    }

    #[test]
    fn post_update_runs_the_gate_and_the_toggle_swap() {
        let game = bare_engine(Arc::new(RecHost::new()));
        // Default config (official=true): the console is force-hidden even
        // with a pending toggle.
        game.set_ilc_show_console_next(); // no-op (debug=false)
        {
            let mut e = game.est();
            e.ilc_show_console = true;
            e.ilc_show_console_next = true;
        }
        game.post_update();
        assert!(!game.ilc_show_console());
        assert!(!game.ilc_show_console_next());

        // debug=true + official=false: the pending toggle flips the
        // visibility exactly as the C++ `ilcShowConsoleNext` branch does.
        {
            let mut c = game.0.config.write().expect("config poisoned");
            c.set_debug(true);
            c.set_official(false);
        }
        game.set_ilc_show_console_next();
        game.post_update();
        assert!(game.ilc_show_console());
        assert!(!game.ilc_show_console_next());
        game.set_ilc_show_console_next();
        game.post_update();
        assert!(!game.ilc_show_console());
    }

    #[test]
    fn engine_tracked_methods_round_trip() {
        let game = bare_engine(Arc::new(RecHost::new()));
        {
            let mut e = game.est();
            e.ilc_track_buffer = "  myVar ".into();
        }
        game.ilc_track();
        assert_eq!(
            game.ilc_lua_tracked(),
            vec!["u_impl_addTrackedResult(myVar)\n"]
        );
        assert_eq!(game.ilc_lua_tracked_names(), vec!["myVar".to_string()]);
        game.ilc_untrack_all();
        assert!(game.ilc_lua_tracked().is_empty());
        assert!(game.ilc_lua_tracked_names().is_empty());
        // No tracked snippets → the C++ `size() > 0` gate is false: a call
        // must not touch the shared results vec.
        game.ilc_update_tracked_results();
        assert!(game
            .state_handle()
            .lock()
            .ilc_lua_tracked_results
            .is_empty());
    }

    #[test]
    fn init_lua_and_print_docs_to_no_script_host_prints_eleven_headers() {
        // The C++ `printDocs` prints the 11 category headers even with an
        // empty store (flag-off build / uninitialized context) — the
        // `NoScriptHost` is the port of that empty store.
        let game = bare_engine(Arc::new(crate::seams::NoScriptHost));
        let mut out = Vec::new();
        game.init_lua_and_print_docs_to(&mut out).expect("print");
        let text = String::from_utf8(out).expect("utf8");
        let mut position = 0;
        for header in PREFIX_HEADERS {
            let rel = text
                .get(position..)
                .and_then(|rest| rest.find(header))
                .unwrap_or_else(|| panic!("header missing or out of order: {header}"));
            position += rel + header.len();
        }
        assert!(
            !text.contains("* **`"),
            "no entry lines with no registrations: {text}"
        );
        // NoScriptHost's docs lookup is the C++ empty-store fallback.
        game.post_update_imgui_lua_console_command("?u_log");
        assert_eq!(
            game.state_handle().lock().ilc_cmd_log.last().expect("line"),
            "[?]: UNKNOWN FUNCTION"
        );
    }

    #[test]
    fn parse_stoi_pins_libstdcxx_behaviors() {
        assert_eq!(parse_stoi("7"), Ok(7));
        assert_eq!(parse_stoi("  7"), Ok(7));
        assert_eq!(parse_stoi("7xyz"), Ok(7)); // decimal prefix
        assert_eq!(parse_stoi("12.5"), Ok(12));
        assert_eq!(parse_stoi("0x7"), Ok(0)); // base 10: the `0` prefix
        assert_eq!(parse_stoi("2147483647"), Ok(i32::MAX));
        assert_eq!(parse_stoi("-2147483648"), Ok(i32::MIN));
        assert_eq!(parse_stoi("2147483648"), Err(IlcParseError::OutOfRange));
        assert_eq!(parse_stoi("-2147483649"), Err(IlcParseError::OutOfRange));
        assert_eq!(
            parse_stoi("99999999999999999999"),
            Err(IlcParseError::OutOfRange)
        ); // i64 saturation → C++ ERANGE
        assert_eq!(parse_stoi("abc"), Err(IlcParseError::InvalidArgument));
        assert_eq!(parse_stoi("-"), Err(IlcParseError::InvalidArgument));
        assert_eq!(parse_stoi(""), Err(IlcParseError::InvalidArgument));
    }
}
