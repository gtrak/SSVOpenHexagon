//! The C++ iostream input primitives the tool needs, byte-exact.

use std::io::BufRead;

/// The C locale's `isspace` set (the C++ `std::cin >>` skip-set).
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// One byte at a time (the C++ streambuf model); `None` at EOF or on a read
/// error (both poison the C++ stream the same way for these tools).
fn next_byte(input: &mut dyn BufRead) -> Option<u8> {
    let buf = input.fill_buf().ok()?;
    if buf.is_empty() {
        return None;
    }
    let b = buf[0];
    input.consume(1);
    Some(b)
}

/// C++ `std::getline` (byte-exact): reads up to (and consumes) a single
/// trailing `\n`; a lone `\r` is kept (the C++ strips only `\n`). `None` at
/// EOF or on a read error — including a final line WITHOUT a newline: the
/// C++ fills the string but sets `eofbit`, and every caller of
/// `cin_getline_string` uses only the `bool` result, so the partial data is
/// discarded (same here).
pub fn cin_getline(input: &mut dyn BufRead) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    while let Some(b) = next_byte(input) {
        if b == b'\n' {
            return Some(line);
        }
        line.push(b);
    }
    // EOF with (possibly) partial content: the C++ sets eofbit → the callers
    // see `false` and drop the data.
    None
}

/// C++ `std::cin >>`'s extraction scan (without the parse): skip `isspace`,
/// then read up to the next `isspace` byte. `None` at EOF before a token
/// starts. The terminating whitespace byte is consumed — the callers
/// (`read_integer` / `read_string`) always follow with `ignore_line`, which
/// is where the C++'s post-extraction `cin.ignore(max, '\n')` lands, so the
/// net stream position matches the C++ exactly.
pub fn read_token(input: &mut dyn BufRead) -> Option<Vec<u8>> {
    let mut s = Scanner::new(input);
    let first = s.skip_ws()?;
    let mut token = vec![first];
    while let Some(b) = s.next_byte() {
        if is_space(b) {
            break;
        }
        token.push(b);
    }
    Some(token)
}

/// C++ `std::cin.ignore(std::numeric_limits<std::streamsize>::max(), '\n')`:
/// discard up to (and including) the next `\n`, or to EOF.
pub fn ignore_line(input: &mut dyn BufRead) {
    let mut s = Scanner::new(input);
    s.ignore_line();
}

/// A `BufRead` with one byte of pushback — the C++ `num_get` stops AT the
/// first non-digit (leaving it in the stream), so `read_integer` needs to
/// scan digits without swallowing the byte that terminates them.
pub(crate) struct Scanner<'a> {
    input: &'a mut dyn BufRead,
    pushback: Option<u8>,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(input: &'a mut dyn BufRead) -> Self {
        Self {
            input,
            pushback: None,
        }
    }

    pub(crate) fn next_byte(&mut self) -> Option<u8> {
        if let Some(b) = self.pushback.take() {
            return Some(b);
        }
        let buf = self.input.fill_buf().ok()?;
        if buf.is_empty() {
            return None;
        }
        let b = buf[0];
        self.input.consume(1);
        Some(b)
    }

    /// Put one byte back (it will be returned by the next `next_byte`).
    pub(crate) fn unread(&mut self, b: u8) {
        self.pushback = Some(b);
    }

    /// The C++ `>>` skipws: the first non-`isspace` byte, or `None` at EOF.
    pub(crate) fn skip_ws(&mut self) -> Option<u8> {
        loop {
            let b = self.next_byte()?;
            if !is_space(b) {
                return Some(b);
            }
        }
    }

    /// The C++ `cin.ignore(max, '\n')`, honoring the pushback byte.
    pub(crate) fn ignore_line(&mut self) {
        while let Some(b) = self.next_byte() {
            if b == b'\n' {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cin_getline_keeps_cr_and_drops_only_lf() {
        let mut input: &[u8] = b"abc\r\ndef\n";
        assert_eq!(cin_getline(&mut input), Some(b"abc\r".to_vec()));
        assert_eq!(cin_getline(&mut input), Some(b"def".to_vec()));
        // A final line WITHOUT a newline: the C++ sets eofbit → `false`.
        assert_eq!(cin_getline(&mut input), None);
    }

    #[test]
    fn cin_getline_empty_line() {
        let mut input: &[u8] = b"\nrest\n";
        assert_eq!(cin_getline(&mut input), Some(b"".to_vec()));
    }

    #[test]
    fn read_token_skips_whitespace_and_stops_at_it() {
        let mut input: &[u8] = b"  \t\nx y\n";
        assert_eq!(read_token(&mut input), Some(b"x".to_vec()));
        assert_eq!(read_token(&mut input), Some(b"y".to_vec()));
        assert_eq!(read_token(&mut input), None);
    }

    #[test]
    fn read_token_at_eof_without_newline() {
        let mut input: &[u8] = b"  tail";
        assert_eq!(read_token(&mut input), Some(b"tail".to_vec()));
    }
}
