//! C++ `main.cpp` folder/config lifecycle pieces — the `mainClient`
//! `:275-291` section (folder creation, config load + reapply, the config
//! save scope guard), ported for the pieces that exist in the step-(b)
//! tree. Issue 15 step (b).

use std::path::Path;
use std::sync::{Arc, RwLock};

use oh_config::Config;
use oh_vfs::Vfs;

/// C++ `sf::VideoMode::getDesktopMode()` placeholder (main.cpp:283 —
/// `Config::reapplyResolution()` reads the desktop mode through SFML,
/// which the step-(b) tree has no windowing backend for). The shipped
/// `_RELEASE/config.json` has BOTH `windowed_auto_resolution` and
/// `fullscreen_auto_resolution` false, so these values are inert for the
/// shipped config (they only matter if a user enables auto-resolution),
/// and the headless simulation never reads window dimensions at all.
/// Step (c) wires the real desktop mode through the windowing layer.
pub const DESKTOP_WIDTH: u32 = 1920;
pub const DESKTOP_HEIGHT: u32 = 1080;

/// C++ `createFolderIfNonExistant(folderName)` (main.cpp:65-79) with the
/// CWD made explicit as `base_dir` (the binary passes the process CWD; the
/// C++ `ssvu::FileSystem::Path{folderName}` is CWD-relative).
///
/// Ported log output on creation — the C++
/// `ssvu::lo("::createFolderIfNonExistant")` channel, printed
/// channel-prefixed like the rest of the port:
/// `'Profiles/' folder does not exist, creating`. An existing folder is
/// silent (the C++ returns before logging). The C++ `createFolder`
/// (`std::filesystem::create_directories`) throws on failure (uncaught
/// here → crash); the port reports to stderr and continues (documented
/// deviation — a failed folder never matters for the headless path).
pub fn create_folder_if_non_existant(base_dir: &Path, folder_name: &str) {
    let path = base_dir.join(folder_name);
    if path.is_dir() {
        return;
    }

    println!("[::createFolderIfNonExistant] '{folder_name}' folder does not exist, creating");

    if let Err(e) = std::fs::create_dir_all(&path) {
        eprintln!("[::createFolderIfNonExistant] failed to create '{folder_name}': {e}");
    }
}

/// C++ `main.cpp:279-283`:
///
/// ```cpp
/// hg::Config::loadConfig(args);
/// hg::Config::reapplyResolution();
/// ```
///
/// `override_ids` are the residual CLI args VERBATIM (the C++ passes its
/// `args` vector — which includes `argv[0]`; a token only becomes an
/// override when `ConfigOverrides/<token>.json` exists, so the executable
/// path is harmless). `Err` is a `config.json` / linked-value sync failure
/// the C++ would throw out of `syncAllFromObj` (uncaught in `mainClient`
/// → crash); the port surfaces it so the caller exits non-zero (documented
/// deviation).
///
/// (The C++ `::loadConfig` / `::reapplyResolution` channel lines
/// (`Config.cpp:494` / `:515`) are not reproduced: `oh-config` (issue 04)
/// is the data layer and its ported `load_config` / `reapply_resolution`
/// carry no logging; re-printing them here would duplicate the logic the
/// data layer owns.)
pub fn load_and_reapply(
    config: &mut Config,
    vfs: &dyn Vfs,
    override_ids: &[String],
) -> Result<(), oh_data::json::JsonError> {
    config.load_config(vfs, override_ids)?;
    config.reapply_resolution(DESKTOP_WIDTH, DESKTOP_HEIGHT);
    Ok(())
}

/// C++ `main.cpp:285-291` — the `HG_SCOPE_GUARD` registered in
/// `mainClient` right after `loadConfig` + `reapplyResolution`:
///
/// ```cpp
/// HG_SCOPE_GUARD({
///     ssvu::lo("::main") << "Saving config...\n";
///     hg::Config::saveConfig();
///     ssvu::lo("::main") << "Done saving config\n";
/// });
/// ```
///
/// Scope guards run at EVERY exit of `mainClient` — including the headless
/// branch's early `return 1`s — so the port mirrors that with a `Drop`
/// guard: a missing replay file (or any other exit) still saves the config
/// on the way out.
///
/// C++ drop order for the three `mainClient` guards is LIFO: the assets
/// `pSaveAll` guard fires FIRST, then the ImGui shutdown guard, and this
/// config-save guard LAST. The two missing pieces are not step-(b) scope:
/// `pSaveAll` needs the profile-mutation API (issue 17) and ImGui is step
/// (c) — so in the ported tree the config save is the only surviving
/// scope-end effect, at the same (last) relative position.
pub struct ConfigSaveGuard {
    config: Arc<RwLock<Config>>,
    vfs: Arc<dyn Vfs>,
}

impl ConfigSaveGuard {
    /// `vfs` = the content root the config was loaded from (the binary roots
    /// a `RealFs` at the game base dir); the guard writes the logical
    /// `"config.json"` under that root at drop.
    #[must_use]
    pub fn new(config: Arc<RwLock<Config>>, vfs: Arc<dyn Vfs>) -> Self {
        Self { config, vfs }
    }
}

impl Drop for ConfigSaveGuard {
    fn drop(&mut self) {
        println!("[::main] Saving config...");
        // The guard's `write` lock is the only lock held across
        // `save_config` (which operates on the guarded `Config` and never
        // re-locks) — the no-lock-across-reacquisition discipline.
        // A poisoned lock (a thread panicked while holding the config)
        // skips the save but still prints both C++ log lines; in the
        // binary the config is main-thread-only, so this is unreachable.
        if let Ok(mut config) = self.config.write() {
            let _ = config.save_config(&*self.vfs);
        }
        println!("[::main] Done saving config");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oh-app-lifecycle-{}-{}-{tag}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace('-', "_")
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// C++ `createFolderIfNonExistant`: creates the folder when missing,
    /// is a silent no-op when present (the C++ `isFolder()` early return —
    /// the log line is pinned by the binary's subprocess tests).
    #[test]
    fn folder_created_once_idempotent_after() {
        let dir = temp_dir("folder");
        create_folder_if_non_existant(&dir, "Profiles/");
        create_folder_if_non_existant(&dir, "Replays/");
        assert!(dir.join("Profiles").is_dir());
        assert!(dir.join("Replays").is_dir());
        // Second pass: the folders exist → the C++ returns before logging
        // and before touching the filesystem again.
        create_folder_if_non_existant(&dir, "Profiles/");
        assert!(dir.join("Profiles").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    /// No `config.json` / `ConfigOverrides/` in the base dir → the C++
    /// `root()` stays the null object (all defaults) and
    /// `reapplyResolution` recalculates sizes from them.
    #[test]
    fn load_and_reapply_missing_config_keeps_defaults() {
        let dir = temp_dir("defaults");
        let mut config = Config::default();
        load_and_reapply(&mut config, &oh_vfs::RealFs::new(dir.as_path()), &[]).expect("load");
        assert_eq!(config.get_windowed_width(), 800, "NSDMI default");
        assert!(config.get_official(), "NSDMI default");
        // recalculateSizes: max(800, 600) * 1.3.
        assert_eq!(config.get_size_x(), 800.0_f32 * 1.3_f32);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A shipped-style `config.json` + a named override in
    /// `ConfigOverrides/`: the override is applied ONLY when its
    /// extension-less name is among the residual args (the C++
    /// `ssvu::contains(mOverridesIds, ...)` test).
    #[test]
    fn load_and_reapply_applies_only_named_overrides() {
        let dir = temp_dir("override");
        fs::create_dir_all(dir.join("ConfigOverrides")).expect("dir");
        fs::write(
            dir.join("ConfigOverrides/foo.json"),
            "{\"windowed_width\": 640}",
        )
        .expect("override");

        let mut config = Config::default();
        load_and_reapply(
            &mut config,
            &oh_vfs::RealFs::new(dir.as_path()),
            &["open-hexagon".to_string(), "foo".to_string()],
        )
        .expect("load");
        assert_eq!(config.get_windowed_width(), 640, "named override applied");

        let mut config = Config::default();
        load_and_reapply(&mut config, &oh_vfs::RealFs::new(dir.as_path()), &["open-hexagon".to_string()]).expect("load");
        assert_eq!(
            config.get_windowed_width(),
            800,
            "unlisted override NOT applied"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The scope guard saves `config.json` at drop with the C++'s two log
    /// lines (the lines themselves are pinned by the binary's subprocess
    /// tests; here: the SAVE side effect — including on the early-return
    /// paths, which the guard's `Drop` placement guarantees).
    #[test]
    fn config_save_guard_saves_at_drop() {
        let dir = temp_dir("guard");
        let mut config = Config::default();
        config.set_current_resolution(1024, 768);
        let shared = Arc::new(RwLock::new(config));
        let path = dir.join("config.json");
        {
            let _guard = ConfigSaveGuard::new(shared.clone(), Arc::new(oh_vfs::RealFs::new(dir.as_path())));
            // (C++ `return` — the guard fires here, at scope end.)
        }
        assert!(path.is_file(), "config.json saved by the guard");
        let mut reloaded = Config::default();
        reloaded
            .load_config(&oh_vfs::RealFs::new(dir.as_path()), &[] as &[String])
            .expect("reload");
        assert_eq!(reloaded.get_windowed_width(), 1024, "round-trip");
        let _ = fs::remove_dir_all(&dir);
    }
}
