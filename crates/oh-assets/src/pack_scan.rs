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
//! All functions take `root` (the process-CWD equivalent, i.e. the
//! `_RELEASE` directory) plus a **logical** path relative to it (the string
//! form the C++ code operates on, e.g. `"Packs/"` or
//! `"Packs/cube/Levels/"`). Filesystem checks are done on `root.join(...)`,
//! but the returned strings are the logical (CWD-relative) paths the C++
//! stores in `PackData::folderPath`, `PackInfo::path`, `LevelData::packPath`
//! and the asset-id maps.

use std::path::Path;

/// `scanSingleFolderName(path)` — C++
/// `scan<Mode::Single, Type::Folder>(buffer, path)` (default
/// `Sort::Alphabetic`): the immediate sub-**folders** of `logical`, each
/// returned as a logical path WITH a trailing slash, byte-sorted.
pub fn scan_single_folder_name(root: &Path, logical: &str) -> Vec<String> {
    scan_dirs_or_files(root, logical, false, None)
}

/// `scanSingleByExt(path, extension)` — C++
/// `scan<Mode::Single, Type::File, Pick::ByExt>(buffer, path, extension)`:
/// the immediate sub-**files** whose name ends with `extension`
/// (CASE-SENSITIVE, `ssvu::endsWith`), as logical paths WITHOUT a trailing
/// slash, byte-sorted.
pub fn scan_single_by_ext(root: &Path, logical: &str, extension: &str) -> Vec<String> {
    scan_dirs_or_files(root, logical, true, Some(extension))
}

/// `scanSingleByName(path, name)` — C++
/// `scan<Mode::Single, Type::File, Pick::ByName>(buffer, path, name)`: the
/// immediate sub-**files** whose name equals `name` exactly (case-sensitive),
/// byte-sorted.
pub fn scan_single_by_name(root: &Path, logical: &str, name: &str) -> Vec<String> {
    scan_dirs_or_files(root, logical, true, None)
        .into_iter()
        .filter(|p| p.strip_prefix(logical).is_some_and(|n| n == name))
        .collect()
}

/// Shared enumeration + filter + byte-sort. `want_files` selects files
/// (`true`) or folders (`false`); `ext` (only meaningful for files) is the
/// case-sensitive extension filter. The C++ `isRootOrParent` guard drops the
/// `.`/`..` pseudo-entries (absent from Rust's `read_dir`, kept for
/// clarity).
fn scan_dirs_or_files(
    root: &Path,
    logical: &str,
    want_files: bool,
    ext: Option<&str>,
) -> Vec<String> {
    let real = root.join(logical);
    let mut out: Vec<String> = match std::fs::read_dir(&real) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    }
    .filter_map(|e| e.ok())
    .filter(|e| {
        if e.file_name().to_string_lossy() == ".." {
            return false;
        }
        let is_dir = e.path().is_dir();
        if want_files {
            !is_dir
        } else {
            is_dir
        }
    })
    .filter(|e| match ext {
        Some(suffix) => e.file_name().to_string_lossy().ends_with(suffix),
        None => true,
    })
    .map(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        if want_files {
            format!("{logical}{name}")
        } else {
            format!("{logical}{name}/")
        }
    })
    .collect();

    // C++ `std::sort` on `Path` (= normalized full-path string, byte-wise).
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
        let got = scan_single_folder_name(&root, "Packs/");
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
        let got = scan_single_by_ext(&root, "Levels/", ".json");
        assert_eq!(got, vec!["Levels/A.json", "Levels/a.json", "Levels/b.json"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn name_scan_matches_exactly() {
        let root = tmp("byname");
        fs::create_dir_all(root.join("Levels")).unwrap();
        fs::write(root.join("Levels").join("pi.json"), "{}").unwrap();
        fs::write(root.join("Levels").join("piX.json"), "{}").unwrap();
        assert_eq!(
            scan_single_by_name(&root, "Levels/", "pi.json"),
            vec!["Levels/pi.json"]
        );
        let _ = fs::remove_dir_all(&root);
    }
}
