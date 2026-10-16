//! Port of the `ssvufs::scan` call sites in `Assets.cpp`
//! (`scanSingleByExt` / `scanSingleByName` / `scanSingleFolderName`).
//!
//! **Discovery order (the rule to copy):** SSVUtils' `Impl::scan`
//! (`extlibs/SSVUtils/.../Internal/ScanHelper.hpp`) does a single-level
//! `readdir`/`opendir`, filters entries, and — with the default
//! `Sort::Alphabetic` (every Assets call site uses the default) —
//! `std::sort`s the target `std::vector<Path>` by `Path::operator<`, which
//! compares the **normalized full-path string** byte-wise (`std::string`
//! `strcmp`). The raw `readdir` order is therefore never observed: it is
//! always normalized to byte-wise lexicographic order of the full normalized
//! path before any consumer sees it.
//!
//! Normalization facts (C++ `Path::normalize`, `Path.hpp`):
//! - a folder path gets a trailing `/` appended (`"Packs/cube"` →
//!   `"Packs/cube/"`);
//! - a file path does NOT (`"Packs/cube/Levels/pi.json"` stays as-is);
//! - `.` / `..` entries (`Path::isRootOrParent`) are skipped.
//!
//! This port reproduces exactly that final ordering and shape: folder
//! results carry a trailing slash, file results do not, and the list is
//! byte-sorted on the full logical path.
//!
//! All functions take a `&dyn Vfs` (the content provider, issue 026a §3) plus
//! a **logical** path relative to its root (the string form the C++ code
//! operates on, e.g. `"Packs/"` or `"Packs/cube/Levels/"`). The enumeration
//! goes through `Vfs::list_dir`; the returned strings are the logical
//! (CWD-relative) paths the C++ stores in `PackData::folderPath`,
//! `PackInfo::path`, `LevelData::packPath` and the asset-id maps.

use oh_vfs::{EntryKind, Vfs};

/// `scanSingleFolderName(path)` — C++
/// `scan<Mode::Single, Type::Folder>(buffer, path)` (default
/// `Sort::Alphabetic`): the immediate sub-**folders** of `logical`, each
/// returned as a logical path WITH a trailing slash, byte-sorted.
pub fn scan_single_folder_name(vfs: &dyn Vfs, logical: &str) -> Vec<String> {
    scan_dirs_or_files(vfs, logical, false, None)
}

/// `scanSingleByExt(path, extension)` — C++
/// `scan<Mode::Single, Type::File, Pick::ByExt>(buffer, path, extension)`:
/// the immediate sub-**files** whose name ends with `extension`
/// (CASE-SENSITIVE, `ssvu::endsWith`), as logical paths WITHOUT a trailing
/// slash, byte-sorted.
pub fn scan_single_by_ext(vfs: &dyn Vfs, logical: &str, extension: &str) -> Vec<String> {
    scan_dirs_or_files(vfs, logical, true, Some(extension))
}

/// `scanSingleByName(path, name)` — C++
/// `scan<Mode::Single, Type::File, Pick::ByName>(buffer, path, name)`: the
/// immediate sub-**files** whose name equals `name` exactly (case-sensitive),
/// byte-sorted.
pub fn scan_single_by_name(vfs: &dyn Vfs, logical: &str, name: &str) -> Vec<String> {
    scan_dirs_or_files(vfs, logical, true, None)
        .into_iter()
        .filter(|p| p.strip_prefix(logical).is_some_and(|n| n == name))
        .collect()
}

/// Shared enumeration + filter + byte-sort. `want_files` selects files
/// (`true`) or folders (`false`); `ext` (only meaningful for files) is the
/// case-sensitive extension filter. The C++ `isRootOrParent` guard drops the
/// `.`/`..` pseudo-entries (absent from `Vfs::list_dir` — both `RealFs`'s
/// `read_dir` and `MemoryVfs` never yield them — so no explicit guard).
fn scan_dirs_or_files(
    vfs: &dyn Vfs,
    logical: &str,
    want_files: bool,
    ext: Option<&str>,
) -> Vec<String> {
    let mut out: Vec<String> = match vfs.list_dir(logical) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    }
    .into_iter()
    .filter(|e| {
        let is_dir = e.kind == EntryKind::Dir;
        if want_files {
            !is_dir
        } else {
            is_dir
        }
    })
    .filter(|e| match ext {
        Some(suffix) => e.name.ends_with(suffix),
        None => true,
    })
    .map(|e| {
        if want_files {
            format!("{logical}{}", e.name)
        } else {
            format!("{logical}{}/", e.name)
        }
    })
    .collect();

    // C++ `std::sort` on `Path` (= normalized full-path string, byte-wise).
    // `Vfs::list_dir` already returns byte-sorted entries, so this is a
    // harmless no-op re-sort kept for fidelity (issue 026a §3).
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("oh-assets-scan-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn folder_scan_is_byte_sorted_with_trailing_slash() {
        let root = tmp("folders");
        for d in ["beta", "alpha", "gamma"] {
            fs::create_dir_all(root.join("Packs").join(d)).unwrap();
        }
        let vfs = oh_vfs::RealFs::new(root.clone());
        let got = scan_single_folder_name(&vfs, "Packs/");
        assert_eq!(got, vec!["Packs/alpha/", "Packs/beta/", "Packs/gamma/"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ext_scan_is_case_sensitive_and_byte_sorted() {
        let root = tmp("files");
        fs::create_dir_all(root.join("Levels")).unwrap();
        for f in ["b.json", "A.json", "a.json", "c.JSON", "note.txt"] {
            fs::write(root.join("Levels").join(f), "{}").unwrap();
        }
        // `.json` is case-sensitive: `c.JSON` and `note.txt` are excluded.
        // Byte-wise: 'A' (0x41) < 'a' (0x61) < 'b' (0x62) — matches C++
        // `std::sort` on the path strings.
        let vfs = oh_vfs::RealFs::new(root.clone());
        let got = scan_single_by_ext(&vfs, "Levels/", ".json");
        assert_eq!(got, vec!["Levels/A.json", "Levels/a.json", "Levels/b.json"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn name_scan_matches_exactly() {
        let root = tmp("byname");
        fs::create_dir_all(root.join("Levels")).unwrap();
        fs::write(root.join("Levels").join("pi.json"), "{}").unwrap();
        fs::write(root.join("Levels").join("piX.json"), "{}").unwrap();
        let vfs = oh_vfs::RealFs::new(root.clone());
        assert_eq!(
            scan_single_by_name(&vfs, "Levels/", "pi.json"),
            vec!["Levels/pi.json"]
        );
        let _ = fs::remove_dir_all(&root);
    }
}
