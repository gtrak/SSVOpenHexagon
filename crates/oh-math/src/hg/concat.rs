//! Port of `include/SSVOpenHexagon/Utils/Concat.hpp` (+ the float/double
//! formatting behavior the C++ `std::ostringstream` path relies on).
//!
//! The C++ `concat` is a variadic template: string-like arguments go
//! through an efficient append path, everything else goes through
//! `std::ostringstream` (`oss << ...`), which for `float`/`double` uses
//! the default precision-6 `%g` formatting (glibc). [`concat`] mirrors
//! that: non-string parts are formatted exactly like
//! `std::ostream <<` would.
//!
//! The oracle values are pinned by `oh-math` tests; the full corpus is
//! regenerable with `tools/oracle/format_probe.cpp` (see
//! `tools/oracle/run.sh`).

/// One argument of [`concat`], mirroring the C++ variadic template's
/// argument types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConcatPart<'a> {
    Str(&'a str),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    /// C++ `oss << bool` prints `1` / `0` (no `boolalpha` by default).
    Bool(bool),
}

/// `hg::Utils::concat(xs...)` — concatenates the parts, formatting each
/// exactly as C++ `std::ostream <<` would.
#[must_use]
pub fn concat(parts: &[ConcatPart<'_>]) -> String {
    let mut result = String::new();
    for part in parts {
        match part {
            ConcatPart::Str(s) => result.push_str(s),
            ConcatPart::I32(v) => result.push_str(&v.to_string()),
            ConcatPart::I64(v) => result.push_str(&v.to_string()),
            ConcatPart::U32(v) => result.push_str(&v.to_string()),
            ConcatPart::U64(v) => result.push_str(&v.to_string()),
            ConcatPart::F32(v) => result.push_str(&format_ostream_f32(*v)),
            ConcatPart::F64(v) => result.push_str(&format_ostream_f64(*v)),
            // C++: `oss << bool` → "1" / "0".
            ConcatPart::Bool(v) => result.push_str(&(*v as u8).to_string()),
        }
    }
    result
}

/// C++ default `std::ostream <<` formatting for a `double` (glibc `%g`
/// with precision 6): 6 significant digits, `%e` style when the decimal
/// exponent is `< -4` or `>= 6`, trailing zeros (and a bare decimal point)
/// dropped, exponent at least two digits with a sign (`1.23457e+06`,
/// `1e-07`).
#[must_use]
pub fn format_ostream_f64(v: f64) -> String {
    if v.is_nan() {
        // glibc: "nan".
        return "nan".to_string();
    }
    if v.is_infinite() {
        // glibc: "inf" / "-inf".
        return if v < 0.0 { "-inf" } else { "inf" }.to_string();
    }

    let negative = v < 0.0 || (v == 0.0 && v.is_sign_negative());
    let abs = v.abs();
    if abs == 0.0 {
        return if negative { "-0" } else { "0" }.to_string();
    }

    // Six significant digits in exponent form: "d.ddddd eN" (N is the
    // value's decimal exponent).
    let s = format!("{:.5e}", abs);
    let (mantissa, exp_str) = s.split_once('e').expect("always has exponent");
    let exp10: i32 = exp_str.parse().expect("always numeric");
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    debug_assert_eq!(digits.len(), 6);

    let mut out = String::with_capacity(16);
    if !(-4..6).contains(&exp10) {
        // %e style: drop trailing zeros from the six significant digits
        // (the leading digit is nonzero, so at least one remains), then
        // the C exponent: signed, at least two digits.
        let kept = 6 - count_trailing_zeros(&digits);
        out.push_str(&digits[..kept]);
        if kept > 1 {
            out.insert(1, '.');
        }
        out.push_str(&format!("e{:+03}", exp10));
    } else {
        // %f style: place the decimal point, then drop trailing zeros.
        if exp10 >= 0 {
            out.push_str(&digits[..(exp10 + 1) as usize]);
            if exp10 + 1 < digits.len() as i32 {
                out.push('.');
                out.push_str(&digits[(exp10 + 1) as usize..]);
            }
        } else {
            out.push_str("0.");
            for _ in 0..(-exp10 - 1) {
                out.push('0');
            }
            out.push_str(&digits);
        }
        if let Some(dot) = out.rfind('.') {
            let mut end = out.len();
            while end > dot + 1 && out.as_bytes()[end - 1] == b'0' {
                end -= 1;
            }
            out.truncate(end);
            if out.ends_with('.') {
                out.pop();
            }
        }
    }
    if negative {
        out.insert(0, '-');
    }
    out
}

/// C++ default `std::ostream <<` formatting for a `float`: the value is
/// converted to `double` first (exact widening), then formatted as above.
#[must_use]
pub fn format_ostream_f32(v: f32) -> String {
    format_ostream_f64(v as f64)
}

fn count_trailing_zeros(digits: &str) -> usize {
    digits.bytes().rev().take_while(|&b| b == b'0').count()
}
