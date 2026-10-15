//! Port of `include/SSVOpenHexagon/Utils/Split.hpp` — `withSplit` / `split`.
//!
//! C++ semantics (verbatim): scan `str` for the first character in
//! `delims`; emit the piece between consecutive delimiters; empty pieces
//! (leading/trailing/consecutive delimiters) are skipped.

/// `hg::Utils::withSplit` — calls `f` for each non-empty piece of `str`
/// split on any delimiter in `delims` (default: space).
pub fn with_split<'s, F: FnMut(&'s str)>(mut f: F, str: &'s str, delims: &str) {
    let delims: Vec<u8> = delims.bytes().collect();
    let bytes = str.as_bytes();
    let last = bytes.len();
    let mut first = 0usize;
    while first < last {
        // C++: `second = std::find_first_of(first, last, delims)`.
        let second = bytes[first..]
            .iter()
            .position(|b| delims.contains(b))
            .map(|p| first + p)
            .unwrap_or(last);
        if first != second {
            // C++: `f(TSplitType{first, second - first})` — a `string_view`
            // over the same bytes, i.e. a borrowed `&str` here.
            f(&str[first..second]);
        }
        // C++: `first = second + 1` (loop re-checks `first != last`).
        first = second + 1;
    }
}

/// `hg::Utils::split` — `withSplit` collecting into a `Vec`
/// (`TSplitType = std::string_view` → borrowed `&str` pieces).
#[must_use]
pub fn split<'a>(str: &'a str, delims: &str) -> Vec<&'a str> {
    let mut result: Vec<&str> = Vec::new();
    with_split(&mut |piece| result.push(piece), str, delims);
    result
}
