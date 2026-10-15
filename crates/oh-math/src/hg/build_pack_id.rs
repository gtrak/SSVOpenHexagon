//! Port of `include/SSVOpenHexagon/Utils/BuildPackId.hpp` +
//! `src/SSVOpenHexagon/Utils/BuildPackId.cpp` — verbatim.

use crate::hg::concat::{concat, ConcatPart};

/// `hg::Utils::buildPackId` — joins disambiguator/author/name with `_`
/// (spaces, newlines and tabs become `_` in each of the three strings)
/// and appends the int version.
///
/// C++ preconditions: none of the three strings empty, `packVersion > 0`
/// (`SSVOH_ASSERT`).
#[must_use]
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

    let space_to_underscore = |s: &str| -> String {
        s.chars()
            .map(|c| match c {
                ' ' | '\n' | '\t' => '_',
                other => other,
            })
            .collect()
    };

    let disambiguator = space_to_underscore(pack_disambiguator);
    let author = space_to_underscore(pack_author);
    let name = space_to_underscore(pack_name);

    concat(&[
        ConcatPart::Str(&disambiguator),
        ConcatPart::Str("_"),
        ConcatPart::Str(&author),
        ConcatPart::Str("_"),
        ConcatPart::Str(&name),
        ConcatPart::Str("_"),
        ConcatPart::I32(pack_version),
    ])
}
