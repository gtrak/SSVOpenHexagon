//! Port of `include/SSVOpenHexagon/Utils/String.hpp` — trim / case
//! helpers, verbatim (C-locale semantics: ASCII whitespace and A–Z).
//!
//! C++ `std::isspace` (C locale) = space, `\t`, `\n`, `\v`, `\f`, `\r` —
//! exactly `is_ascii_whitespace`; `std::toupper`/`tolower` (C locale)
//! only map A–Z ↔ a–z.

/// C-locale `isspace` over a byte (the game's strings are handled
/// byte-wise by the C++ `std::string` overloads).
#[must_use]
const fn isspace_c(ch: u8) -> bool {
    ch.is_ascii_whitespace()
}

/// C-locale `toupper` over a byte.
#[must_use]
const fn toupper_c(ch: u8) -> u8 {
    ch.to_ascii_uppercase()
}

/// C-locale `tolower` over a byte.
#[must_use]
const fn tolower_c(ch: u8) -> u8 {
    ch.to_ascii_lowercase()
}

/// `hg::Utils::lTrim` — removes leading whitespace (in place).
pub fn l_trim(str: &mut String) {
    let start = str.bytes().take_while(|&b| isspace_c(b)).count();
    str.drain(..start);
}

/// `hg::Utils::rTrim` — removes trailing whitespace (in place).
pub fn r_trim(str: &mut String) {
    let end = str.bytes().rev().take_while(|&b| isspace_c(b)).count();
    str.truncate(str.len() - end);
}

/// `hg::Utils::lrTrim` — `lTrim` then `rTrim`.
pub fn lr_trim(str: &mut String) {
    l_trim(str);
    r_trim(str);
}

/// `hg::Utils::getLTrim` — trimmed copy.
#[must_use]
pub fn get_l_trim(s: String) -> String {
    let mut s = s;
    l_trim(&mut s);
    s
}

/// `hg::Utils::getRTrim` — trimmed copy.
#[must_use]
pub fn get_r_trim(s: String) -> String {
    let mut s = s;
    r_trim(&mut s);
    s
}

/// `hg::Utils::getLRTrim` — trimmed copy.
#[must_use]
pub fn get_lr_trim(s: String) -> String {
    let mut s = s;
    lr_trim(&mut s);
    s
}

/// `hg::Utils::uppercasify` — A–Z in place (C++ takes `std::string&`).
///
/// `as_bytes_mut` is `unsafe` (it may produce invalid UTF-8); here each
/// byte is mapped by the C-locale `toupper`, which only touches the
/// ASCII letters A–Z, so the string stays valid UTF-8.
pub fn uppercasify(s: &mut str) {
    for b in unsafe { s.as_bytes_mut() } {
        *b = toupper_c(*b);
    }
}

/// `hg::Utils::toUppercase` — uppercased copy.
#[must_use]
pub fn to_uppercase(mut s: String) -> String {
    uppercasify(&mut s);
    s
}

/// `hg::Utils::lowercasify` — a–z in place (C++ takes `std::string&`).
///
/// `as_bytes_mut` is `unsafe` (it may produce invalid UTF-8); here each
/// byte is mapped by the C-locale `tolower`, which only touches the
/// ASCII letters a–z, so the string stays valid UTF-8.
pub fn lowercasify(s: &mut str) {
    for b in unsafe { s.as_bytes_mut() } {
        *b = tolower_c(*b);
    }
}

/// `hg::Utils::toLowercase` — lowercased copy.
#[must_use]
pub fn to_lowercase(mut s: String) -> String {
    lowercasify(&mut s);
    s
}
