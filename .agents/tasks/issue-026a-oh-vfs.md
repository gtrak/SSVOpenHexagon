# Task 026a — the `oh-vfs` trait + oh-assets/oh-config routing (Phase 1 / lane 26a)

Phase 1 of issue 26 (`.agents/plans/001-rust-port/26-wasm-web.md`, the
"extract the VFS" phase; the plan's blocker #3). This is the **P0.4 VFS
contract inventory** — the investigation + the minimal contract that Phase 1
implements. The deliverable is a trait that routes **every** content
filesystem operation the asset/config layer performs through a single
abstraction, so the browser (lane 26c) can serve the same `_RELEASE` tree from
memory (fetch-into-memory) instead of `std::fs`, with **native
behavior-identical** on desktop/android.

**User guidance (the contract shape):** prefer the *smallest* contract that
matches actual use — an **asset/configuration provider**, NOT a general
filesystem. The browser needs only resolve-known-path / read /
load-config-and-defaults / predictable missing-asset handling. The interface
must **not force eager loading** (startup latency + memory). Track the
**native regression burden** explicitly — this is the largest reusable change
even if the browser port is later abandoned.

Port discipline (literal 1:1, determinism-first) applies double here: the
refactor must be **byte-for-byte behavior-identical on native** (the
`_RELEASE`-tree tests are the proof).

## 1. Inventory — every filesystem operation the asset/config layer performs

Read from source (2026-10-16, `wasm` branch). "Op" is one of:
**read a known path** / **list a directory** / **check existence** (dir or
file) / **write a known path**. Paths are *logical* (CWD-relative) strings the
C++/port already builds (`"Packs/"`, `"Packs/cube/Levels/pi.json"`,
`"config.json"`); the filesystem check is done on `root.join(logical)`.

### `oh-assets/src` (the `Assets` discovery/loading pipeline, threads `root: PathBuf`)

| file:line | op | what it reads / lists / checks |
|---|---|---|
| `pack_scan.rs:74` | **list a directory** | `std::fs::read_dir(root.join(logical))` — the single enumerator every `scan_single_folder_name` / `scan_single_by_ext` / `scan_single_by_name` (fns at `pack_scan.rs:38/47/55`) funnels into |
| `pack_scan.rs:83` | check existence (per entry) | `e.path().is_dir()` — the file-vs-folder filter for each enumerated entry |
| `asset_storage.rs:68` | check existence (file) | `path.exists()` — the `loadTexture/loadFont/loadSoundBuffer` "did the file exist" stand-in (C++ `loadFromFile` success). **The single choke point for every asset-file existence check** (fonts, textures, sound buffers, custom sounds) |
| `assets.rs:120` | check existence (dir) | `root.join("Assets").is_dir()` — gate for the `assets.json` load |
| `assets.rs:124` | **read a known path** | `read_file_with_errors(root.join("Assets").join("assets.json"))` |
| `assets.rs:167` | check existence (dir) | `is_dir(logical)` helper (`root.join(logical).is_dir()`); called 5× at `assets.rs:287/290/293/299/302` for `{pack}Shaders/`, `{pack}Sounds/`, `{pack}Music/`, `{pack}Styles/`, `{pack}Levels/` |
| `assets.rs:175` | check existence (dir) | `root.join("Packs").is_dir()` — gate for pack discovery |
| `assets.rs:181` | **list a directory** | `pack_scan::scan_single_folder_name(root, "Packs/")` — immediate pack sub-folders |
| `assets.rs:210` | check existence (file) | `root.join(pack_json).is_file()` — is `pack.json` present? |
| `assets.rs:214` | **read a known path** | `read_file_with_errors(root.join(pack_json))` — pack data |
| `assets.rs:315` | **list a directory** | `pack_scan::scan_single_by_ext(root, "{pack}Shaders/", ext)` — `.vert`/`.geom`/`.frag` (a closure, invoked 3×) |
| `assets.rs:318` | check existence (file) | `root.join(p).is_file()` — shader file present |
| `assets.rs:338` | **list a directory** | `scan_single_by_ext(root, "{pack}Sounds/", ".ogg")` — custom sounds |
| `assets.rs:352` | **list a directory** | `scan_single_by_ext(root, "{pack}Music/", ".ogg")` — music files |
| `assets.rs:366` | **list a directory** | `scan_single_by_ext(root, "{pack}Music/", ".json")` — music data |
| `assets.rs:367` | **read a known path** | `read_file_with_errors(root.join(p))` — music `.json` |
| `assets.rs:384` | **list a directory** | `scan_single_by_ext(root, "{pack}Styles/", ".json")` — style data |
| `assets.rs:386` | **read a known path** | `read_file_with_errors(root.join(p))` — style `.json` |
| `assets.rs:403` | **list a directory** | `scan_single_by_ext(root, "{pack}Levels/", ".json")` — level data |
| `assets.rs:405` | **read a known path** | `read_file_with_errors(root.join(p))` — level `.json` |
| `assets.rs:465` | check existence (dir) | `root.join("Profiles").is_dir()` — gate for the profile load |
| `assets.rs:470` | **list a directory** | `scan_single_by_ext(root, "Profiles/", ".json")` — profiles |
| `assets.rs:471` | **read a known path** | `read_file_with_errors(root.join(p))` — profile `.json` |
| `assets.rs:721,728,735` | check existence (file) | `storage.load_font/load_texture/load_sound_buffer(id, root.join(...))` → `asset_storage.rs:68` `path.exists()` — the `assets.json`-listed asset files |
| `assets.rs:342-343` | check existence (file) | `storage.load_sound_buffer(asset_id, root.join(p))` → `asset_storage.rs:68` — custom-sound `.ogg` presence |

`oh-assets/src/shaders.rs` — registry only, **no** filesystem ops.

### `oh-config/src` (the `Config` load/save, threads `base_dir: &Path` per call)

| file:line | op | what it reads / lists / checks |
|---|---|---|
| `config.rs:602` | check existence (file) | `config_path.is_file()` — is `config.json` present? |
| `config.rs:603` | **read a known path** | `std::fs::read_to_string(config_path)` — `config.json` (**direct**, not via `read_file_with_errors`) |
| `config.rs:612` | check existence (dir) | `base_dir.join("ConfigOverrides").is_dir()` — gate for the overrides |
| `config.rs:615` | **list a directory** | `std::fs::read_dir(base_dir.join("ConfigOverrides"))` — the override files (**direct**) |
| `config.rs:620` | check existence (file) | `p.is_file() && p.extension() == "json"` — per-entry filter |
| `config.rs:634` | **read a known path** | `json::read_file_with_errors(&path)` — the named override `.json` |
| `config.rs:905` | **write a known path** | `std::fs::write(path, json::write_to_string(root))` — `save_config` (the config-save write; fired by `oh-app`'s `ConfigSaveGuard` on every exit) |

### `oh-data/src` (the shared JSON-read choke point behind all the reads above)

| file:line | op | what |
|---|---|---|
| `json.rs:539-540` | **read a known path** | `pub fn read_file_with_errors(path) -> (Obj, String)` — `std::fs::read_to_string(path)` + `parse`. The single helper behind ALL the JSON reads (6× in `oh-assets`, 1× in `oh-config`). Missing file → `(Null, " in file <name>")`; parse failure → `(Null, "<formatted messages> in file <name>")`. **These error strings are pinned by the tests** (`release_tree.rs`, `config_release.rs`). |

### Other content reads that would need routing (NOT the asset/config layer — adjacent, out of 26a scope)

These are content reads in the **render/app** layer that a full web port would
eventually route too, but they belong to **issue 10** (the render backend) /
the app composition, not the 26a asset/config refactor. Listed so the full
picture is honest; the `oh-vfs` trait is designed so they *can* reuse it, but
26a does not touch them:

| file:line | op | what |
|---|---|---|
| `oh-app/src/compose.rs:246,268` | read a known path | level `.lua` scripts (`std::fs::read(root.join(path))`, cached/uncached — the `LuaScriptHost`) |
| `oh-app/src/windowed.rs:1847,1849` | read a known path | `Font::load(Assets/OpenSquare-{Regular,Bold}.ttf)` → `oh-render/src/font.rs:37` `std::fs::read` |
| `oh-app/src/windowed.rs:2465` + `android.rs:245` | read a known path | `Image::load_png(Assets/icon.png)` |
| `oh-render/src/shader.rs:84` | read a known path | GLSL shader source (`std::fs::read_to_string`) — issue 10 |
| `oh-render/src/texture.rs:132` | read a known path | PNG texture (`std::fs::File::open`) — issue 10 |
| `oh-render/src/audio.rs:274` | check existence | audio file (`std::fs::metadata`) — issue 10 |
| `oh-app/src/lifecycle.rs:42` | write (dir create) | `create_dir_all(base_dir.join("Profiles/"/"Replays/"))` — folder creation |
| `oh-app/src/dispatch.rs:301` | (resolve) | `resolve_game_root` → the `base_dir` threaded to everything above |

`oh-game/src` has **no** production fs reads (only the test mirror
`oh-game/tests/common/mod.rs:265,287` — the `run_lua_file` test seam, a copy of
the `compose.rs` read).

## 2. Findings — the ~13-15-site expectation

The plan's "oh-assets has 13 `fs::` sites" is in the right ballpark but
undercounts once you include the `read_file_with_errors` reads (which are
`std::fs::read_to_string` under the hood in `oh-data`) alongside the `Path`
predicates. The precise **production (non-test)** count:

- **`oh-assets/src`**: 1 `read_dir` (`pack_scan.rs:74`) + 8 `Path` predicates
  (`is_dir`/`is_file`/`exists`: `pack_scan.rs:83`, `asset_storage.rs:68`,
  `assets.rs:120/167/175/210/318/465`) + 6 `read_file_with_errors` reads
  (`assets.rs:124/214/367/386/405/471`) = **15 call-sites**. (The 8
  `scan_single_*` logical list call-sites in `assets.rs` all funnel into the
  single `read_dir`.)
- **`oh-config/src`**: **7** (`read_to_string`, `read_dir`, `write`, 3
  predicates, `read_file_with_errors`).
- **`oh-data/src`**: **1** (the `read_file_with_errors` helper).

But the **operation kinds collapse to 5**: `read` (file bytes), `list_dir`,
`is_dir`, `is_file`, `write`. That is why the contract is small — the ~23
call-sites are all instances of 5 primitives. The estimate's "13-15" matches
`oh-assets`'s 15 call-sites; it just did not include `oh-config`'s 7 (including
the one **WRITE**, which a "read-only provider" framing would miss) or the
`oh-data` choke point.

## 3. The minimal contract

### Where the trait lives — **a new `oh-vfs` crate** (recommended over inside `oh-assets`)

- **Both `oh-assets` and `oh-config` need it.** Putting it in `oh-assets` would
  force `oh-config → oh-assets` — a layering inversion: they are peers (
  `oh-assets` depends on `oh-data`, not `oh-config`; neither depends on the
  other today). A separate `oh-vfs` keeps both as dependents with **no new
  cross-dependency between them**.
- It is a **tiny, pure-Rust, zero-dependency** crate (just the trait +
  `VfsError` + `Entry`/`EntryKind` + `RealFs` + `MemoryVfs`), so it
  cross-compiles to `wasm32-unknown-unknown` trivially (P0.2 showed
  `oh-data`/`oh-config` already cross-compile; `oh-vfs` adds nothing).
- It is the "smallest reusable change" with a clean boundary, and it is the
  plan's own naming (blocker #3 in `26-wasm-web.md` already calls it `oh-vfs`).

### The trait (asset/configuration *provider*, not a general filesystem)

Exactly the operations in the inventory, nothing more. Path arguments are
**logical (CWD-relative) `&str`** — the same strings the C++/port already
builds — and each provider resolves them against its own root; the caller never
sees a `std::path::Path`.

```rust
//! oh-vfs: a read/write view over the game's content root (the C++'s CWD / the
//! `_RELEASE` tree).

/// The kind of a directory entry (the C++ `Path::isFolder()` / `isFile()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind { File, Dir }

/// A directory entry: the child's BARE name (e.g. `"cube"`, `"pi.json"`) + kind.
/// The caller reconstructs the logical child path as `format!("{parent}{name}")`
/// (the parent carries its trailing slash, as the C++/port already do).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

/// The VFS error. Callers today branch only on `is_err()` (a missing/unreadable
/// file is handled identically to any other read failure), so variant
/// granularity is a non-issue for behavior-identity; native keeps the io detail
/// for fidelity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsError {
    /// The entry is missing, or is not the expected kind (a `read`/`list_dir`
    /// on a non-file / non-directory). The C++ open-failure.
    NotFound,
    /// A provider-specific failure (native: a raw IO error's text; web: a
    /// `fetch` failure's text).
    Other(String),
}

pub type VfsResult<T> = Result<T, VfsError>;

/// An asset/configuration content provider. Smallest contract matching actual
/// use: resolve-known-path / read / list / existence-check / config-save.
/// **No eager "load everything" method** — the browser must not be forced to
/// pull the whole `_RELEASE` tree up front (startup latency + memory).
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
```

**Why `is_dir` + `is_file`, not a single `exists`:** the inventory needs the
dir/file distinction — `assets.rs:120` gates on `is_dir("Assets")` (a folder)
while `assets.rs:210` gates on `is_file(pack.json)` (a file). Collapsing to one
`exists(path)` would conflate the C++ `isFolder()` and `isFile()` and force the
caller to re-probe the kind. `list_dir` returns `Entry{name, kind}` (not bare
`Vec<String>`) because `pack_scan` needs the per-entry dir/file distinction to
filter (files vs folders) and to append the trailing slash to folders — one
call instead of `list_dir` + N `is_dir` re-probes (faithful to `read_dir`,
which yields entries with metadata).

**Why `write` is in the contract (and not "read-only"):** `oh-config`'s
`save_config` (`config.rs:905`) is a real, always-run op (the `ConfigSaveGuard`
fires on every exit, headless included). Any `std::fs::write` left un-routed
would fail to compile on `wasm32-unknown-unknown` (no `std::fs`), so the write
**must** route through the trait for `oh-config` to cross-compile. `RealFs::write`
is native-identical; `MemoryVfs::write` stores the bytes in-session (on-disk
config persistence on web is out of scope — v1 is a prototype).

**Eager-loading rule:** the trait has **no** `load_all` / `snapshot` /
"load everything" method. `MemoryVfs`'s *construction* may fetch the whole tree
(that is the web bootstrap, 26d's job), but the trait itself only exposes
lazy per-path access, so nothing forces eager loading through the contract.

### `base_dir` / root resolution — provider-internal, not caller-supplied

- **`RealFs`** (native + android): `RealFs::new(root: impl Into<PathBuf>)`
  holds the root. `read(logical)` = `std::fs::read(root.join(logical))`;
  `is_dir` = `root.join(logical).is_dir()`; `is_file` =
  `root.join(logical).is_file()`; `list_dir` = `std::fs::read_dir(root.join(logical))`
  mapped to `Entry{name, kind}`; `write` = `create_dir_all(parent)` +
  `std::fs::write(root.join(logical), bytes)`. **Byte-for-byte
  behavior-identical to today** — it performs the exact same `root.join(logical)`
  + `std::fs` calls the asset/config layer does now; the only change is that the
  `root` moves from a `PathBuf` field in `Assets` / a `&Path` param in `Config`
  into the `RealFs` handle.
- **`MemoryVfs`** (web, fetch-into-memory):
  `MemoryVfs::new(files: BTreeMap<String, Vec<u8>>, dirs: BTreeSet<String>)`.
  `read(logical)` = `files.get(logical).cloned().ok_or(NotFound)`; `is_file` =
  `files.contains_key(logical)`; `is_dir` = `dirs.contains(logical)`;
  `list_dir(logical)` = the `files`/`dirs` keys whose parent is `logical`
  (byte-sorted); `write(logical, bytes)` = insert into `files`. The content is
  fetched **once** (the `_RELEASE` tree packed as a site asset, or a single
  `.zip` fetched and expanded in memory) and then served from the map.
- `RealFs` must be `#[cfg(not(target_arch = "wasm32"))]`-gated (it references
  `std::fs`); the trait + `MemoryVfs` + the error/entry types are pure and
  compile for wasm.

### How the asset/config layer routes through it (the 26a refactor, behavior-identical on native)

- **`Assets`** (`assets.rs`): the `root: PathBuf` field becomes `vfs:
  Arc<dyn Vfs>`; every `self.root.join(logical).is_dir()` → `vfs.is_dir(logical)`,
  `.is_file()` → `vfs.is_file(logical)`, `read_file_with_errors(&self.root.join(p))`
  → `vfs.read(p)` + a pure parse (below). `Assets::new(vfs: Arc<dyn Vfs>,
  headless, levels_only)`. The `root()` getter (`assets.rs:539`) is **unused
  externally** (verified — no call sites) and is dropped/repurposed.
- **`pack_scan`** (`pack_scan.rs`): the fns take `&dyn Vfs` instead of
  `root: &Path`; `scan_dirs_or_files` calls `vfs.list_dir(logical)` and filters
  by `Entry.kind` + ext/name. (The byte-sort is already applied by `list_dir`;
  the existing `out.sort()` is a harmless no-op re-sort — keep it for fidelity.)
- **`oh-data::json::read_file_with_errors`** (`json.rs:539`): becomes a **pure**
  helper `parse_with_errors(bytes: &[u8], file_name: &str) -> (Obj, String)`
  (no `std::fs`), preserving the exact error strings (`" in file <name>"` on
  missing/empty, `"<msgs> in file <name>"` on parse failure). The asset/config
  layers do `let bytes = match vfs.read(p) { Ok(b) => b, Err(_) => /* missing:
  (Null, " in file <name>") */ }; let (obj, err) = json::parse_with_errors(&bytes,
  file_name)`. (Keeping `oh-data` free of an `oh-vfs` dep is cleaner — `oh-data`
  is the lowest layer and cross-compiles.)
- **`Config`** (`config.rs`): `load_config(&mut self, vfs: &dyn Vfs,
  override_ids)` — `config_path.is_file()` → `vfs.is_file("config.json")`,
  `std::fs::read_to_string` → `vfs.read("config.json")`,
  `std::fs::read_dir(ConfigOverrides)` → `vfs.list_dir("ConfigOverrides/")`,
  `json::read_file_with_errors(&path)` → `vfs.read(...)` + `parse_with_errors`.
  `save_config(&mut self, vfs: &dyn Vfs)` → `vfs.write("config.json", ...)`.
- **`oh-app`** (the callers): construct `RealFs` from the resolved `base_dir`
  (`dispatch.rs:301` `resolve_game_root` → `RealFs::new(base_dir)`;
  `android.rs` `assets_base_dir` → `RealFs::new(internal_data_dir)` — Android's
  `extract_content` (25b SG4) still lands content on disk first, then `RealFs`
  reads it — unchanged). Pass the `Arc<dyn Vfs>` to `Assets::new` /
  `Config::load_config` / `save_config`.
- **The other `Assets::new` callers** (`oh-menu/src/real.rs:38`
  `RealMenuAssets::new`; `oh-parity`, `oh-net` server, `oh-game` tests) get a
  `RealFs` instead of a `&Path`.

## 4. Native regression burden (explicit)

These tests MUST stay green and **byte-identical** — they are the
"behavior-identical" acceptance:

- **`crates/oh-assets/tests/release_tree.rs`** (10 tests, the `_RELEASE`-tree
  gate): `packs == 8`; headless `assets == 57` / full `assets == 61`;
  `levels == 37`; cube `== 9` levels; the exact `apeirogon` `LevelData` fields;
  the byte-sorted discovery-order `vec![...]` (both the `Packs/` folders and the
  `Packs/cube/Levels/*.json`); the 7-selectable priority-ordered `pack_infos`;
  the 4-shader registry; and the exact `LoadInfo` error-message strings
  (`"FATAL ERROR, Exception during asset loading: Value is not convertible to
  int.\n"`, `" in file bad.json"`).
- **`crates/oh-config/tests/config_release.rs`** (15 tests, the `_RELEASE`-tree
  gate): the exact shipped `config.json` values; the 10 `ConfigOverrides/*.json`
  names; the per-override key isolation; `save_reload_round_trip_is_stable`
  byte-identity (101 top-level keys, saved == reloaded); the bind /
  eligibility / joystick assertions.
- **Inline unit tests**: `oh-assets/src/pack_scan.rs` (3),
  `oh-assets/src/asset_storage.rs` (3), `oh-assets/src/shaders.rs` (1);
  `oh-config/src/triggers.rs` (`#[cfg(test)]` module).
- **The broader gate**: `tools/gate.sh` 4/4 + `cargo test --workspace` (698) —
  the VFS refactor is native-identical, so the whole workspace stays green.

The "behavior-identical" acceptance, concretely: every count/path assertion
above is unchanged — the same `packs`/`assets`/`levels`/`levels-by-pack` counts,
the same byte-sorted discovery order, the same exact error strings, the same
101-key config round-trip. `RealFs` performs the identical
`root.join(logical)` + `std::fs` calls, so the observable behavior (counts,
paths, error text, ordering) is unchanged.

## 5. Acceptance criteria for 26a

1. `cargo test --workspace` green (698).
2. The `_RELEASE`-tree tests (`oh-assets/tests/release_tree.rs`,
   `oh-config/tests/config_release.rs`) pass **unchanged** (no assertion edits).
3. `oh-assets` and `oh-config` route **every** content fs op through the `Vfs`
   trait — no `std::fs` / `Path::is_dir` / `Path::is_file` / `Path::exists` /
   direct-`std::fs`-backed `read_file_with_errors` remains in their `src/`
   (`pack_scan` uses `vfs.list_dir`; `asset_storage` uses `vfs.is_file`;
   `assets.rs` uses `vfs.read`/`is_dir`/`is_file`; `config.rs` uses
   `vfs.read`/`is_file`/`is_dir`/`list_dir`/`write`).
4. **Discrimination check**: a `MemoryVfs` test (in `oh-vfs` or `oh-assets`)
   that builds a small in-memory `_RELEASE`-shaped tree (a couple of packs +
   levels + a `config.json` + a `ConfigOverrides/*.json`) and drives
   `Assets::new(Arc::new(memory_vfs), ...)` +
   `Config::load_config(memory_vfs, ...)` end-to-end **with no `std::fs`**
   (proving routing works off-disk). Assert the same counts/paths the `RealFs`
   `_RELEASE` tests assert (a reduced set, e.g. 1 pack / N levels / 1 override
   applied).
5. `oh-vfs` cross-compiles: `cargo check -p oh-vfs --target
   wasm32-unknown-unknown` green (the trait + `MemoryVfs` + error/entry types
   are pure; `RealFs` is `#[cfg(not(target_arch = "wasm32"))]`-gated so its
   `std::fs` use does not break the wasm build).

## 6. Out of scope

- **No web build yet** — that is 26c (the `oh-web` crate) / 26d (WebAudio +
  content-packing). This lane only extracts the trait + routes native.
- **No behavior change on native** — the refactor is behavior-identical; the
  `_RELEASE`-tree tests are the proof.
- **Net / Steam / Discord untouched** (feature-gated OFF on web per the plan;
  not part of this refactor).
- **The render-backend content reads** (`oh-render` font/shader/texture/audio;
  the `oh-app` `LuaScriptHost` `.lua` read; the icon) are **issue-10 /
  app-composition** concerns — NOT routed here. They would be routed in
  26c/26d if/when the web port reaches them; the trait is designed so they *can*
  reuse it, but 26a does not touch them.
- **Android's `extract_content`** (25b SG4) is unchanged — it still lands
  `_RELEASE` on disk, then `RealFs` reads it. (The web analogue —
  fetch-into-memory — is 26d's `MemoryVfs` construction.)

## 7. Sub-goals (commit after EACH green one)

1. **`oh-vfs` crate**: the `Vfs` trait + `VfsError` + `Entry`/`EntryKind` +
   `RealFs` (`#[cfg(not(target_arch = "wasm32"))]`) + `MemoryVfs`. Zero deps.
   Unit-test `RealFs` (round-trip read/is_dir/is_file/list_dir/write against a
   temp dir) and `MemoryVfs` (same ops over an in-memory map). Add to the
   workspace `Cargo.toml` `members`.
2. **`oh-data` pure parse**: split `read_file_with_errors` into a pure
   `parse_with_errors(bytes, file_name)` (no `std::fs`); keep the exact error
   strings. (The old `read_file_with_errors` can remain a thin
   `std::fs`-backed wrapper for any non-asset/config caller, or be removed if
   nothing else uses it — verify.)
3. **Route `oh-assets`**: `Assets` holds `vfs`; `pack_scan` takes `&dyn Vfs`;
   `asset_storage` uses `vfs.is_file`. `release_tree.rs` + inline tests green
   unchanged.
4. **Route `oh-config`**: `load_config`/`save_config` take `&dyn Vfs`.
   `config_release.rs` green unchanged.
5. **Update the callers**: `oh-app` (`dispatch`/`client`/`android`/
   `print_lua_docs`) + `oh-menu`/`oh-parity`/`oh-net`/`oh-game` tests construct
   a `RealFs`. Add the `MemoryVfs` discrimination test (acceptance #4).

## 8. Constraints

- **Commit after every green sub-goal.** Never leave work uncommitted.
- **Desktop gate is sacred**: `tools/gate.sh` 4/4 + `cargo test --workspace`
  (698) at every landing point. The desktop path must be behavior-identical.
- Do not modify `_RELEASE/` tracked content. Do not bump dependency versions
  (no new external deps — `oh-vfs` is dependency-free).
- f32/f64, literal-port discipline; the byte-sorted discovery order and the
  exact `LoadInfo`/config error strings are pinned — do not "improve" them.
- No servers, no background processes.

## 9. Report

Sub-goal-by-sub-goal status, commands, commit hashes, gate tail; the
`oh-vfs` trait as landed (diff vs §3); the `MemoryVfs` discrimination test
output; confirmation that `oh-assets`/`oh-config` `src/` contain no
`std::fs`/`Path` predicate; and the explicit out-of-scope list (what 26c/26d
still must route).
