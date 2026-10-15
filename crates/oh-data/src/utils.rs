//! Ports of `Utils/LevelValidator` and `Utils/BuildPackId`
//! (`include/SSVOpenHexagon/Utils/{LevelValidator,BuildPackId}.hpp` + `.cpp`).

/// C++ `hg::Utils::getLevelValidator(levelId, diffMult)` —
/// `concat(levelId, "_m_", diffMult)` where the float is formatted with
/// the **default `std::ostream` float formatting** (6 significant digits,
/// `%g`-like): `1.0` → `"1"`, `0.5` → `"0.5"`, `0.35` → `"0.35"`,
/// `1.001` → `"1.001"`, `0.00001` → `"1e-05"` (mapping §7.1).
///
/// This string is what appears in `LevelData::validators`, the
/// `server_level_whitelist` of `config.json`, and score/profile ids —
/// mismatched formatting breaks key parity.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn get_level_validator(level_id: &str, diff_mult: f32) -> String {
    format!("{level_id}_m_{}", iostream_f32_default(diff_mult))
}

/// C++ `hg::Utils::buildPackId(disambiguator, author, name, version)`
/// (mapping §7.2): every `' '`/`'\n'`/`'\t'` in each of the three strings
/// becomes `'_'`, joined with `'_'`, then the int version. Debug asserts in
/// C++: all three non-empty and `packVersion > 0` (release builds continue
/// with `_0` ids — see mapping §10.5).
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn build_pack_id(
    pack_disambiguator: &str,
    pack_author: &str,
    pack_name: &str,
    pack_version: i32,
) -> String {
    debug_assert!(!pack_disambiguator.is_empty());
    debug_assert!(!pack_author.is_empty());
    debug_assert!(!pack_name.is_empty());
    debug_assert!(pack_version > 0);
    format!(
        "{}_{}_{}_{}",
        space_to_underscore(pack_disambiguator),
        space_to_underscore(pack_author),
        space_to_underscore(pack_name),
        pack_version
    )
}

fn space_to_underscore(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' | '\n' | '\t' => '_',
            c => c,
        })
        .collect()
}

/// Default `std::ostream` float formatting for an `f32` (promoted to double
/// in C++), i.e. `%g` at precision 6:
///
/// - 6 significant digits, correctly rounded;
/// - scientific form when the decimal exponent is `< -4` or `>= 6`;
/// - trailing zeros (and a dangling `.`) removed;
/// - exponents written without leading zeros but with at least two digits
///   and a `+`/`-` sign.
#[must_use = "mirrors the C++ [[nodiscard]]"]
pub fn iostream_f32_default(v: f32) -> String {
    let x = v as f64;
    if x == 0.0 {
        return "0".to_string();
    }
    let s = format!("{:.5e}", x); // 1 + 5 significant digits, rounded
    let (mant, exp_str) = s.split_once('e').expect("e present");
    let exp: i64 = exp_str.parse().expect("int exponent");
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let digits: String = mant.chars().filter(|c| c != &'.').collect();
    debug_assert_eq!(digits.len(), 6);

    let sci = !(-4..6).contains(&exp);
    if sci {
        let trimmed: String = {
            let mut d = digits;
            while d.len() > 1 && d.ends_with('0') {
                d.pop();
            }
            d
        };
        let mant_s = if trimmed.len() == 1 {
            trimmed.clone()
        } else {
            format!("{}.{}", &trimmed[..1], &trimmed[1..])
        };
        let e_abs = exp.abs();
        let e_str = if e_abs >= 10 {
            e_abs.to_string()
        } else {
            format!("0{e_abs}")
        };
        format!(
            "{sign}{mant_s}e{}{}",
            if exp < 0 { '-' } else { '+' },
            e_str
        )
    } else if exp >= 0 {
        let int_part = if (exp as usize) + 1 >= digits.len() {
            format!(
                "{}{}",
                digits,
                "0".repeat((exp as usize) + 1 - digits.len())
            )
        } else {
            digits[..(exp as usize) + 1].to_string()
        };
        let frac_part = digits[(exp as usize) + 1..].to_string();
        let frac = frac_part.trim_end_matches('0').to_string();
        if frac.is_empty() {
            format!("{sign}{int_part}")
        } else {
            format!("{sign}{int_part}.{frac}")
        }
    } else {
        let zeros = "0".repeat((-exp - 1) as usize);
        let body = format!("{zeros}{digits}");
        let body = body.trim_end_matches('0').to_string();
        format!("{sign}0.{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_format_matches_cpp_vectors() {
        // mapping §7.1 vectors.
        assert_eq!(iostream_f32_default(1.0), "1");
        assert_eq!(iostream_f32_default(0.5), "0.5");
        assert_eq!(iostream_f32_default(1.8), "1.8");
        assert_eq!(iostream_f32_default(0.35), "0.35");
        assert_eq!(iostream_f32_default(1.001), "1.001");
        assert_eq!(iostream_f32_default(6.66), "6.66");
        assert_eq!(iostream_f32_default(0.1), "0.1");
        assert_eq!(iostream_f32_default(123.456), "123.456");
        assert_eq!(iostream_f32_default(0.00001), "1e-05");
        assert_eq!(iostream_f32_default(3.0), "3");
        assert_eq!(iostream_f32_default(1_600_000.0), "1.6e+06");
    }

    #[test]
    fn validator_ids() {
        assert_eq!(get_level_validator("pi", 0.5), "pi_m_0.5");
        assert_eq!(get_level_validator("pi", 1.0), "pi_m_1");
        assert_eq!(get_level_validator("pi", 1.8), "pi_m_1.8");
    }

    #[test]
    fn pack_id_matches_shipped_form() {
        assert_eq!(
            build_pack_id("ohvrvanilla", "vittorio romeo", "cube", 1),
            "ohvrvanilla_vittorio_romeo_cube_1"
        );
        assert_eq!(build_pack_id("a\nb", "c\td", "e f", 2), "a_b_c_d_e_f_2");
    }
}
