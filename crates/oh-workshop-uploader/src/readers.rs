//! The C++ reader utilities (`src/OHWorkshopUploader/main.cpp:44-123`),
//! ported 1:1.
//!
//! # Documented deviation (D-014): EOF
//!
//! Every C++ reader loops on a failed `std::cin` extraction, which — once
//! `eofbit` is set — fails forever: at EOF the C++ spins printing its prompt
//! line indefinitely (`read_integer` main.cpp:44-64, `read_directory_path`
//! main.cpp:81-95, `read_file_path` main.cpp:97-111, the changelog loops
//! main.cpp:553-557 / 597-601). The port prints the prompt line exactly once
//! on EOF and returns `None`, which the CLI flow turns into "abort the run,
//! exit 1" (the C++ never reaches its exit there — it spins).

use std::fs;
use std::io::{BufRead, ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::input::{cin_getline, ignore_line, read_token, Scanner};

/// C++ `read_integer<T>` (main.cpp:44-64): `cin >> T` retrying with the
/// prompt on every failure; the successful parse also consumes the rest of
/// the line. `None` on EOF (the documented deviation above).
///
/// Byte-exact `num_get` semantics (main.cpp:46): the extraction is the
/// longest leading `[+-]?[0-9]+` slice — `"5x"` extracts `5` and leaves the
/// `x` for the post-extraction `cin.ignore(max, '\n')`; a token without a
/// leading integer fails with nothing consumed (beyond the skipped
/// whitespace); an out-of-range value fails AFTER consuming the digits.
pub fn read_integer<I>(input: &mut dyn BufRead, out: &mut dyn Write) -> Option<I>
where
    I: std::str::FromStr,
{
    let mut s = Scanner::new(input);
    loop {
        // C++ `std::cin >>`'s skipws (main.cpp:46).
        let Some(first) = s.skip_ws() else {
            // EOF: the C++ clears the flags, ignores (a no-op at EOF) and
            // prompts — forever. The port prompts once and aborts (the
            // documented deviation).
            let _ = writeln!(out, "Please insert an integer.");
            return None;
        };

        // The candidate: an optional sign, then one or more decimal digits —
        // the longest leading `[+-]?[0-9]+` (main.cpp:46-49).
        let mut candidate = String::new();
        let signed = matches!(first, b'+' | b'-');
        if signed {
            candidate.push(first as char);
        } else if !first.is_ascii_digit() {
            // No leading integer: the C++ fails the extraction (nothing
            // consumed beyond the skipped whitespace), clears, then ignores
            // the rest of the line (main.cpp:47-51) and prompts.
            s.ignore_line();
            let _ = writeln!(out, "Please insert an integer.");
            continue;
        } else {
            candidate.push(first as char);
        }

        // The remaining digits, stopped at the first non-digit (left in the
        // stream) or at EOF.
        let mut terminated: Option<u8> = None;
        while let Some(d) = s.next_byte() {
            if d.is_ascii_digit() {
                candidate.push(d as char);
            } else {
                terminated = Some(d);
                break;
            }
        }
        if terminated.is_none() {
            // The digits run exactly into EOF: the C++ extraction sets
            // eofbit, so `read_integer`'s `while(!(cin >> result))` treats it
            // as a failure and spins on the prompt forever. The port prompts
            // once and aborts (the documented deviation).
            let _ = writeln!(out, "Please insert an integer.");
            return None;
        }
        s.unread(terminated.expect("set above"));

        // The post-extraction `cin.clear(); cin.ignore(max, '\n')`
        // (main.cpp:62-63 on success, main.cpp:47-51 on failure) — consumes
        // the non-digit tail + the newline either way.
        s.ignore_line();

        match candidate.parse::<I>() {
            Ok(value) => return Some(value),
            // Overflow / sign-for-unsigned: the C++ sets `failbit` → retry.
            Err(_) => {
                let _ = writeln!(out, "Please insert an integer.");
            }
        }
    }
}

/// C++ `cin_getline_string` (main.cpp:61-64).
pub fn cin_getline_string(input: &mut dyn BufRead) -> Option<Vec<u8>> {
    cin_getline(input)
}

/// C++ `cin_getline_path` (main.cpp:66-79): `cin_getline_string` + the
/// `Read '...'` echo on the same stream. The raw bytes become the path (no
/// UTF-8 assumption — on Linux the `OsString` keeps them verbatim; the echo
/// is lossy for non-UTF-8 bytes, display-only).
pub fn cin_getline_path(input: &mut dyn BufRead, out: &mut dyn Write) -> Option<PathBuf> {
    let buf = cin_getline_string(input)?;
    let _ = writeln!(out, "Read '{}'", String::from_utf8_lossy(&buf));
    Some(path_from_bytes(&buf))
}

/// The raw input bytes as a `Path` (byte-exact on Linux/macOS; on Windows
/// the bytes are lossily decoded as UTF-8 into the (UTF-16-backed) path —
/// Rust Windows paths cannot hold arbitrary bytes, and the C++ read them
/// as a byte string interpreted by the current locale; issue-22 documents
/// this as the Windows interpretation).
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// C++ `read_directory_path` (main.cpp:81-95): read + validate a directory,
/// retrying with the C++ error line. The `Error code: '...'` text is EMPTY
/// for "does not exist" and "exists but is not a directory" — the C++
/// `std::filesystem::exists/is_directory` leave `ec` default (no error) in
/// both cases (main.cpp:86-89); only a real system error (e.g. ELOOP,
/// EACCES) yields the raw `strerror` text.
pub fn read_directory_path(input: &mut dyn BufRead, out: &mut dyn Write) -> Option<PathBuf> {
    read_validated_path(input, out, true)
}

/// C++ `read_file_path` (main.cpp:97-111).
pub fn read_file_path(input: &mut dyn BufRead, out: &mut dyn Write) -> Option<PathBuf> {
    read_validated_path(input, out, false)
}

fn read_validated_path(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    want_directory: bool,
) -> Option<PathBuf> {
    let noun = if want_directory { "directory" } else { "file" };
    loop {
        let Some(path) = cin_getline_path(input, out) else {
            // EOF: the C++ prints the error line with the default (empty)
            // `ec` and spins; the port prints it once and aborts (the
            // documented deviation).
            let _ = writeln!(
                out,
                "Please insert a valid path to an existing {noun}. Error code: ''"
            );
            return None;
        };
        match path_error(&path, want_directory) {
            None => return Some(path),
            Some(error_code) => {
                let _ = writeln!(
                    out,
                    "Please insert a valid path to an existing {noun}. Error code: '{error_code}'"
                );
            }
        }
    }
}

/// The C++ `exists(p, ec)` + `is_directory(p, ec)` / `is_regular_file(p, ec)`
/// checks (a single `ec`, main.cpp:86-89 / 102-105) as one result:
/// `None` = the path satisfies the check; `Some(msg)` = the `Error code`
/// text (empty for "missing" / "wrong kind" — the C++ leaves `ec` default
/// there; the raw `strerror` for a real system error).
fn path_error(path: &Path, want_directory: bool) -> Option<String> {
    // `fs::metadata` follows symlinks, like the C++ checks.
    match fs::metadata(path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            let ok = if want_directory {
                file_type.is_dir()
            } else {
                file_type.is_file()
            };
            if ok {
                None
            } else {
                // The C++ leaves `ec` default for "exists but wrong kind".
                Some(String::new())
            }
        }
        Err(error) => {
            // The C++ `not_found` status leaves `ec` default (no error text).
            if error.kind() == ErrorKind::NotFound {
                Some(String::new())
            } else {
                Some(errno_message(&error))
            }
        }
    }
}

/// The C++ `std::error_code::message()` (the raw `strerror` text) for a Rust
/// `io::Error` — the Rust `Display` appends " (os error N)", which the C++
/// line does not carry.
fn errno_message(error: &std::io::Error) -> String {
    if let Some(code) = error.raw_os_error() {
        let text = error.to_string();
        let suffix = format!(" (os error {code})");
        if let Some(stripped) = text.strip_suffix(&suffix) {
            return stripped.to_string();
        }
    }
    error.to_string()
}

/// C++ `read_string` (main.cpp:113-123): a single `cin >>` token, no retry,
/// the rest of the line consumed. (Dead code in the C++ too — no call sites
/// in main.cpp — ported for the 1:1 utility set.) An EOF read returns the
/// empty string, as the C++'s unchanged (default-constructed) result.
#[allow(dead_code)] // dead in the C++ as well (ported for 1:1)
pub fn read_string(input: &mut dyn BufRead) -> String {
    match read_token(input) {
        Some(token) => {
            ignore_line(input);
            String::from_utf8_lossy(&token).into_owned()
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_integer_prefixes_overflow_and_eof() {
        let mut input: &[u8] = b"5x\n+7\n-2\nabc\n3\n";
        let mut out: Vec<u8> = Vec::new();
        // "5x" extracts 5 (the rest of the line is ignored), then +7, -2.
        assert_eq!(read_integer::<i32>(&mut input, &mut out), Some(5));
        assert_eq!(read_integer::<i32>(&mut input, &mut out), Some(7));
        assert_eq!(read_integer::<i32>(&mut input, &mut out), Some(-2));
        // "abc" fails → prompt + retry; then 3.
        assert_eq!(read_integer::<i32>(&mut input, &mut out), Some(3));
        assert_eq!(out, b"Please insert an integer.\n");
        // EOF → the documented deviation: one prompt, then None.
        assert_eq!(read_integer::<i32>(&mut input, &mut out), None);
        assert_eq!(
            out,
            b"Please insert an integer.\nPlease insert an integer.\n"
        );
    }

    #[test]
    fn read_integer_u64_overflow_retries() {
        let mut input: &[u8] = b"99999999999999999999\n7\n";
        let mut out: Vec<u8> = Vec::new();
        assert_eq!(read_integer::<u64>(&mut input, &mut out), Some(7));
        assert_eq!(out, b"Please insert an integer.\n");
    }

    #[test]
    fn read_integer_negative_into_u64_retries() {
        let mut input: &[u8] = b"-5\n5\n";
        let mut out: Vec<u8> = Vec::new();
        // The C++ `cin >> unsigned` on a negative value → failbit.
        assert_eq!(read_integer::<u64>(&mut input, &mut out), Some(5));
        assert_eq!(out, b"Please insert an integer.\n");
    }

    #[test]
    fn read_string_is_one_token_then_ignores_the_line() {
        let mut input: &[u8] = b"xyz rest of line\nnext\n";
        assert_eq!(read_string(&mut input), "xyz");
        // The rest of the first line was consumed.
        assert_eq!(std::str::from_utf8(input), Ok("next\n"));
    }

    #[test]
    fn read_string_at_eof_is_empty() {
        let mut input: &[u8] = b"";
        assert_eq!(read_string(&mut input), "");
    }

    #[test]
    fn path_error_missing_is_empty_error_code() {
        // The C++ `exists()` on a missing path: no error (empty `ec`).
        assert_eq!(
            path_error(Path::new("/definitely/not/a/real/path"), true),
            Some(String::new())
        );
    }

    #[test]
    fn path_error_right_kind_passes() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(path_error(dir.path(), true), None);
        let file = tempfile::NamedTempFile::new().expect("tempfile");
        assert_eq!(path_error(file.path(), false), None);
        // A file where a directory is wanted (and vice versa): empty `ec`.
        assert_eq!(path_error(file.path(), true), Some(String::new()));
        assert_eq!(path_error(dir.path(), false), Some(String::new()));
    }
}
