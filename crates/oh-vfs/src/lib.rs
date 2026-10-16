//! `oh-vfs` — a read/write view over the game's content root (the C++'s CWD /
//! the `_RELEASE` tree).
//!
//! This crate is the P0.4 **VFS contract** from
//! `.agents/tasks/issue-026a-oh-vfs.md` §3: the single abstraction every
//! content filesystem operation in the asset/config layer routes through, so
//! the browser (lane 26c) can serve the same `_RELEASE` tree from memory
//! (fetch-into-memory) instead of `std::fs`, with **native
//! behavior-identical** on desktop/android.
//!
//! The contract is the *smallest* one that matches actual use — an
//! asset/configuration **provider**, NOT a general filesystem. The ~23
//! production call-sites in `oh-assets` / `oh-config` / `oh-data` collapse to
//! these 5 primitives (`read`, `list_dir`, `is_dir`, `is_file`, `write`); see
//! the inventory in issue-026a §1–§2.
//!
//! **No eager "load everything" method** — the browser must not be forced to
//! pull the whole `_RELEASE` tree up front (startup latency + memory).
//! `MemoryVfs`'s *construction* may fetch the whole tree (the web bootstrap,
//! 26d's job), but the trait only exposes lazy per-path access, so nothing
//! forces eager loading through the contract.
//!
//! **wasm:** the trait + `MemoryVfs` + the error/entry types are pure and
//! compile for `wasm32-unknown-unknown`; `RealFs` (which uses `std::fs`) is
//! `#[cfg(not(target_arch = "wasm32"))]`-gated, so its `std::fs` use does not
//! break the wasm build.
//!
//! C++ semantics mirrored here (see `.agents/skills/lib-assets.md`):
//! - `is_dir` / `is_file` are the C++ `Path::isFolder()` / `isFile()`
//!   distinction — **not** a single `exists`. `assets.rs` gates on
//!   `is_dir("Assets")` (a folder) while it gates on `is_file(pack.json)` (a
//!   file); collapsing to one `exists` would conflate the two and force the
//!   caller to re-probe the kind.
//! - `list_dir` returns the immediate children **byte-sorted by name** (== by
//!   full logical path, since the parent is a common prefix) — the C++
//!   `std::sort` on the normalized full-path string, the discovery-order rule
//!   in `lib-assets.md` §1. `Entry` carries the dir/file distinction in
//!   `kind` (the C++ `read_dir` yields entries with metadata), so `pack_scan`
//!   can filter files-vs-folders in one call instead of `list_dir` + N
//!   `is_dir` re-probes.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// The kind of a directory entry (the C++ `Path::isFolder()` / `isFile()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory (folder).
    Dir,
}

/// A directory entry: the child's **BARE** name (e.g. `"cube"`, `"pi.json"`)
/// + kind.
///
/// The caller reconstructs the logical child path as
/// `format!("{parent}{name}")` (the parent carries its trailing slash, as the
/// C++/port already do). `list_dir` returns BARE names (not full paths) so the
/// caller decides the folder-vs-file trailing-slash normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The child's BARE name (no parent, no trailing slash).
    pub name: String,
    /// Whether the child is a regular file or a directory.
    pub kind: EntryKind,
}

/// The VFS error.
///
/// Callers today branch only on `is_err()` (a missing/unreadable file is
/// handled identically to any other read failure — see
/// `oh_data::json::read_file_with_errors`), so variant granularity is a
/// non-issue for behavior-identity; native keeps the IO detail for fidelity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    /// The entry is missing, or is not the expected kind (a `read`/`list_dir`
    /// on a non-file / non-directory). The C++ open-failure.
    NotFound,
    /// A provider-specific failure (native: a raw IO error's text; web: a
    /// `fetch` failure's text).
    Other(String),
}

/// The VFS result alias.
pub type VfsResult<T> = Result<T, VfsError>;

/// An asset/configuration content provider.
///
/// Smallest contract matching actual use: resolve-known-path / read / list /
/// existence-check / config-save. **No eager "load everything" method** — the
/// browser must not be forced to pull the whole `_RELEASE` tree up front
/// (startup latency + memory).
///
/// Path arguments are **logical (CWD-relative) `&str`** — the same strings the
/// C++/port already builds (e.g. `"Packs/"`, `"Packs/cube/Levels/pi.json"`,
/// `"config.json"`). Each provider resolves them against its own root; the
/// caller never sees a `std::path::Path`.
pub trait Vfs {
    /// Read the whole file at logical `path`. `Err(NotFound)` when it is
    /// missing or not a regular file (the C++ open-failure).
    fn read(&self, path: &str) -> VfsResult<Vec<u8>>;

    /// True iff logical `path` exists and is a *directory* (the C++
    /// `Path::isFolder()` — the "folder exists" gates: `Assets/`, `Packs/`,
    /// `Profiles/`, `*/Shaders/`, `*/Sounds/`, `*/Music/`, `*/Styles/`,
    /// `*/Levels/`, `ConfigOverrides/`).
    fn is_dir(&self, path: &str) -> bool;

    /// True iff logical `path` exists and is a *regular file* (the C++
    /// `Path::isFile()` — `pack.json`, shader files, `config.json`, the
    /// asset-file checks, the `ConfigOverrides/*.json` entries).
    fn is_file(&self, path: &str) -> bool;

    /// The immediate children of directory `path` (BARE names + kind),
    /// **byte-sorted** by name (== by full logical path, since the parent is a
    /// common prefix — the C++ `std::sort` on the normalized full-path string,
    /// the discovery-order rule in `.agents/skills/lib-assets.md`).
    /// `Err(NotFound)` when `path` is not a directory.
    fn list_dir(&self, path: &str) -> VfsResult<Vec<Entry>>;

    /// Write `bytes` to logical `path` (the C++ `writeToFile` — the config
    /// save, `Config::save_config`). Parent directories are created on demand
    /// (native: `create_dir_all(parent)`).
    fn write(&self, path: &str, bytes: &[u8]) -> VfsResult<()>;
}

/// The native VFS provider: a read/write view rooted at a `PathBuf`.
///
/// Performs the exact `root.join(logical)` + `std::fs` calls the asset/config
/// layer does today (issue-026a §3):
/// - `read`    = `std::fs::read(root.join(logical))`
/// - `is_dir`  = `root.join(logical).is_dir()`
/// - `is_file` = `root.join(logical).is_file()`
/// - `list_dir`= `std::fs::read_dir(root.join(logical))` → `Entry{name, kind}`
/// - `write`   = `create_dir_all(parent)` +
///   `std::fs::write(root.join(logical), bytes)`
///
/// Byte-for-byte behavior-identical to today; the only change is that the
/// `root` moves from a `PathBuf` field in `Assets` / a `&Path` param in
/// `Config` into this handle.
///
/// `#[cfg(not(target_arch = "wasm32"))]`: it uses `std::fs`, which does not
/// exist on `wasm32-unknown-unknown`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct RealFs {
    root: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl RealFs {
    /// Construct a `RealFs` rooted at `root` (the resolved game root /
    /// CWD-equivalent, i.e. the `_RELEASE` directory on native).
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Vfs for RealFs {
    fn read(&self, path: &str) -> VfsResult<Vec<u8>> {
        let p = self.root.join(path);
        std::fs::read(&p).map_err(|e| {
            // Missing, or not a regular file (e.g. a directory) → the C++
            // open-failure. Any other IO failure keeps its detail.
            if e.kind() == std::io::ErrorKind::NotFound || !p.is_file() {
                VfsError::NotFound
            } else {
                VfsError::Other(e.to_string())
            }
        })
    }

    fn is_dir(&self, path: &str) -> bool {
        self.root.join(path).is_dir()
    }

    fn is_file(&self, path: &str) -> bool {
        self.root.join(path).is_file()
    }

    fn list_dir(&self, path: &str) -> VfsResult<Vec<Entry>> {
        let p = self.root.join(path);
        let rd = std::fs::read_dir(&p).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound || !p.is_dir() {
                VfsError::NotFound
            } else {
                VfsError::Other(e.to_string())
            }
        })?;
        let mut entries = rd
            .filter_map(|e| e.ok())
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let kind = if e.path().is_dir() {
                    EntryKind::Dir
                } else {
                    EntryKind::File
                };
                Entry { name, kind }
            })
            .collect::<Vec<_>>();
        // C++ `std::sort` on the normalized full-path string. Since the parent
        // is a common prefix, byte-sorting the BARE name is equivalent (the
        // discovery-order rule, `lib-assets.md` §1).
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> VfsResult<()> {
        let p = self.root.join(path);
        // `write` creates parent directories on demand (native
        // `create_dir_all(parent)`), matching `Config::save_config`'s
        // on-disk behavior.
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VfsError::Other(e.to_string()))?;
        }
        std::fs::write(&p, bytes).map_err(|e| VfsError::Other(e.to_string()))
    }
}

/// The in-memory VFS provider (web, fetch-into-memory).
///
/// Holds the content as two maps keyed by logical path:
/// - `files`: logical file path (no trailing slash) → bytes, e.g.
///   `"Packs/cube/pack.json"`;
/// - `dirs`: logical directory path (no trailing slash), e.g. `"Packs"`,
///   `"Packs/cube"`.
///
/// The content is fetched **once** (the `_RELEASE` tree packed as a site asset,
/// or a single `.zip` fetched and expanded in memory — the web bootstrap,
/// 26d's job) and then served from the maps. `write` stores the bytes
/// in-session (on-disk config persistence on web is out of scope — v1 is a
/// prototype).
///
/// `files` is a `RefCell` because the trait's `write(&self, …)` must mutate
/// the in-memory store through a shared reference (interior mutability);
/// `dirs` is set at construction and never mutated. Pure (no `std::fs`) —
/// compiles for `wasm32-unknown-unknown`.
#[derive(Debug, Clone, Default)]
pub struct MemoryVfs {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
    dirs: BTreeSet<String>,
}

impl MemoryVfs {
    /// Construct a `MemoryVfs` from pre-built `files` (logical path → bytes)
    /// and `dirs` (logical directory paths). See the type docs for the key
    /// convention (no trailing slashes).
    #[must_use]
    pub fn new(files: BTreeMap<String, Vec<u8>>, dirs: BTreeSet<String>) -> Self {
        Self {
            files: RefCell::new(files),
            dirs,
        }
    }
}

impl Vfs for MemoryVfs {
    fn read(&self, path: &str) -> VfsResult<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or(VfsError::NotFound)
    }

    fn is_dir(&self, path: &str) -> bool {
        // `is_dir` is called with or without a trailing slash in the real code
        // (`assets.rs` gates on `is_dir("Packs")` while the scans use
        // `list_dir("Packs/")`); normalize so both resolve to the same dir.
        self.dirs.contains(path.trim_end_matches('/'))
    }

    fn is_file(&self, path: &str) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn list_dir(&self, path: &str) -> VfsResult<Vec<Entry>> {
        let dir = path.trim_end_matches('/');
        // The root (empty after trim) is always a directory; any other path
        // must be a registered dir, else `NotFound`.
        if !dir.is_empty() && !self.dirs.contains(dir) {
            return Err(VfsError::NotFound);
        }
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut entries = Vec::new();
        for key in self.files.borrow().keys() {
            if let Some(name) = child_name(key, &prefix) {
                entries.push(Entry {
                    name,
                    kind: EntryKind::File,
                });
            }
        }
        for key in self.dirs.iter() {
            if let Some(name) = child_name(key, &prefix) {
                entries.push(Entry {
                    name,
                    kind: EntryKind::Dir,
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> VfsResult<()> {
        self.files
            .borrow_mut()
            .insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
}

/// The BARE child name of `key` under `prefix`, if `key` is an immediate
/// child (the remainder after `prefix` is non-empty and contains no `/`).
fn child_name(key: &str, prefix: &str) -> Option<String> {
    let rest = key.strip_prefix(prefix)?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small in-memory `_RELEASE`-shaped tree shared by the
    /// `MemoryVfs` tests:
    ///
    /// ```text
    /// Packs/
    ///   base/pack.json
    ///   cube/pack.json
    ///   cube/Levels/pi.json
    /// config.json
    /// ```
    fn memory_tree() -> (BTreeMap<String, Vec<u8>>, BTreeSet<String>) {
        let mut files = BTreeMap::new();
        files.insert("Packs/base/pack.json".to_string(), b"base-pack".to_vec());
        files.insert("Packs/cube/pack.json".to_string(), b"cube-pack".to_vec());
        files.insert(
            "Packs/cube/Levels/pi.json".to_string(),
            b"pi-level".to_vec(),
        );
        files.insert("config.json".to_string(), b"config".to_vec());
        let mut dirs = BTreeSet::new();
        dirs.insert("Packs".to_string());
        dirs.insert("Packs/base".to_string());
        dirs.insert("Packs/cube".to_string());
        dirs.insert("Packs/cube/Levels".to_string());
        (files, dirs)
    }

    #[test]
    fn memory_vfs_read_and_predicates() {
        let (files, dirs) = memory_tree();
        let vfs = MemoryVfs::new(files, dirs);

        // read: exact bytes.
        assert_eq!(vfs.read("Packs/cube/Levels/pi.json").unwrap(), b"pi-level");
        assert_eq!(vfs.read("Packs/cube/pack.json").unwrap(), b"cube-pack");
        assert_eq!(vfs.read("config.json").unwrap(), b"config");

        // is_dir: with and without the trailing slash.
        assert!(vfs.is_dir("Packs"));
        assert!(vfs.is_dir("Packs/"));
        assert!(vfs.is_dir("Packs/cube"));
        assert!(vfs.is_dir("Packs/cube/Levels"));
        // is_file.
        assert!(vfs.is_file("Packs/cube/pack.json"));
        assert!(vfs.is_file("config.json"));
        // a dir is not a file, a file is not a dir.
        assert!(!vfs.is_file("Packs/cube"));
        assert!(!vfs.is_dir("Packs/cube/pack.json"));
    }

    #[test]
    fn memory_vfs_list_dir_is_byte_sorted_with_kind() {
        let (files, dirs) = memory_tree();
        let vfs = MemoryVfs::new(files, dirs);

        // BARE names, byte-sorted, with the right EntryKind.
        assert_eq!(
            vfs.list_dir("Packs/").unwrap(),
            vec![
                Entry {
                    name: "base".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "cube".into(),
                    kind: EntryKind::Dir
                },
            ]
        );
        // 'L' (0x4C) < 'p' (0x70): the dir precedes the file.
        assert_eq!(
            vfs.list_dir("Packs/cube/").unwrap(),
            vec![
                Entry {
                    name: "Levels".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "pack.json".into(),
                    kind: EntryKind::File
                },
            ]
        );
        assert_eq!(
            vfs.list_dir("Packs/cube/Levels/").unwrap(),
            vec![Entry {
                name: "pi.json".into(),
                kind: EntryKind::File
            }]
        );
        // list_dir also works without the trailing slash (normalized).
        assert_eq!(
            vfs.list_dir("Packs/cube").unwrap(),
            vec![
                Entry {
                    name: "Levels".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "pack.json".into(),
                    kind: EntryKind::File
                },
            ]
        );
    }

    #[test]
    fn memory_vfs_list_dir_byte_order_is_case_sensitive() {
        // Prove byte-wise (case-sensitive) ordering: 'A'(0x41) <
        // 'Z'(0x5A) < 'a'(0x61); and 'a.json' < 'alpha' ('.' < 'l').
        let mut files = BTreeMap::new();
        files.insert("Packs/A.json".to_string(), vec![2]);
        files.insert("Packs/a.json".to_string(), vec![1]);
        let mut dirs = BTreeSet::new();
        dirs.insert("Packs".to_string());
        dirs.insert("Packs/Zeta".to_string());
        dirs.insert("Packs/alpha".to_string());
        let vfs = MemoryVfs::new(files, dirs);
        assert_eq!(
            vfs.list_dir("Packs/").unwrap(),
            vec![
                Entry {
                    name: "A.json".into(),
                    kind: EntryKind::File
                },
                Entry {
                    name: "Zeta".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "a.json".into(),
                    kind: EntryKind::File
                },
                Entry {
                    name: "alpha".into(),
                    kind: EntryKind::Dir
                },
            ]
        );
    }

    #[test]
    fn memory_vfs_missing_paths() {
        let (files, dirs) = memory_tree();
        let vfs = MemoryVfs::new(files, dirs);

        // read: missing → NotFound.
        assert!(matches!(vfs.read("nope.json"), Err(VfsError::NotFound)));
        // is_dir / is_file: missing → false.
        assert!(!vfs.is_dir("nope"));
        assert!(!vfs.is_file("nope.json"));
        // list_dir: not a registered dir → NotFound (a file is not a dir).
        assert!(matches!(vfs.list_dir("nope/"), Err(VfsError::NotFound)));
        assert!(matches!(
            vfs.list_dir("Packs/cube/pack.json"),
            Err(VfsError::NotFound)
        ));
    }

    #[test]
    fn memory_vfs_write_inserts_into_files() {
        let vfs = MemoryVfs::default();
        assert!(!vfs.is_file("config.json"));
        vfs.write("config.json", b"{\"a\":1}").unwrap();
        assert!(vfs.is_file("config.json"));
        assert_eq!(vfs.read("config.json").unwrap(), b"{\"a\":1}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn real_fs_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        // Build a nested structure: dirs + files with distinct bytes.
        //   Packs/
        //     base/pack.json
        //     cube/pack.json
        //     cube/Levels/pi.json
        //   config.json
        std::fs::create_dir_all(root.join("Packs/base")).unwrap();
        std::fs::create_dir_all(root.join("Packs/cube/Levels")).unwrap();
        std::fs::write(root.join("Packs/base/pack.json"), b"base-pack").unwrap();
        std::fs::write(root.join("Packs/cube/pack.json"), b"cube-pack").unwrap();
        std::fs::write(root.join("Packs/cube/Levels/pi.json"), b"pi-level").unwrap();
        std::fs::write(root.join("config.json"), b"config").unwrap();

        let vfs = RealFs::new(root.to_path_buf());

        // read: exact bytes.
        assert_eq!(vfs.read("Packs/cube/Levels/pi.json").unwrap(), b"pi-level");
        assert_eq!(vfs.read("Packs/cube/pack.json").unwrap(), b"cube-pack");
        assert_eq!(vfs.read("config.json").unwrap(), b"config");

        // is_dir / is_file.
        assert!(vfs.is_dir("Packs"));
        assert!(vfs.is_dir("Packs/cube"));
        assert!(vfs.is_dir("Packs/cube/Levels"));
        assert!(vfs.is_file("Packs/cube/pack.json"));
        assert!(vfs.is_file("config.json"));
        // a file is not a dir, a dir is not a file.
        assert!(!vfs.is_file("Packs/cube"));
        assert!(!vfs.is_dir("Packs/cube/pack.json"));

        // list_dir: BARE names, byte-sorted, with the right EntryKind.
        assert_eq!(
            vfs.list_dir("Packs/").unwrap(),
            vec![
                Entry {
                    name: "base".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "cube".into(),
                    kind: EntryKind::Dir
                },
            ]
        );
        assert_eq!(
            vfs.list_dir("Packs/cube/").unwrap(),
            vec![
                Entry {
                    name: "Levels".into(),
                    kind: EntryKind::Dir
                },
                Entry {
                    name: "pack.json".into(),
                    kind: EntryKind::File
                },
            ]
        );
        assert_eq!(
            vfs.list_dir("Packs/cube/Levels/").unwrap(),
            vec![Entry {
                name: "pi.json".into(),
                kind: EntryKind::File
            }]
        );

        // write: creates parents + persists to disk (independent of the
        // VFS handle).
        vfs.write("Profiles/saved.json", b"saved-bytes").unwrap();
        assert!(vfs.is_file("Profiles/saved.json"));
        assert_eq!(vfs.read("Profiles/saved.json").unwrap(), b"saved-bytes");
        assert_eq!(
            std::fs::read(root.join("Profiles/saved.json")).unwrap(),
            b"saved-bytes"
        );

        // missing paths: read → NotFound; is_dir/is_file → false;
        // list_dir on a non-dir → NotFound.
        assert!(matches!(vfs.read("nope.json"), Err(VfsError::NotFound)));
        // a directory is not a regular file → read → NotFound.
        assert!(matches!(vfs.read("Packs/cube"), Err(VfsError::NotFound)));
        assert!(!vfs.is_dir("nope"));
        assert!(!vfs.is_file("nope.json"));
        assert!(matches!(vfs.list_dir("nope/"), Err(VfsError::NotFound)));
        // a file is not a directory → list_dir → NotFound.
        assert!(matches!(
            vfs.list_dir("Packs/cube/pack.json"),
            Err(VfsError::NotFound)
        ));
    }
}
