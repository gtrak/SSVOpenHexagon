//! Port of `include/SSVOpenHexagon/Global/AssetStorage.hpp` +
//! `src/SSVOpenHexagon/Global/AssetStorage.cpp`.
//!
//! The C++ wraps three SFML handle maps (`sf::Texture`, `sf::Font`,
//! `sf::SoundBuffer`). The *actual* SFML `loadFromFile` payload belongs to
//! the render/audio backend (issue 10 / the audio issue); this port keeps
//! the **registry semantics** the game relies on and records, per id, whether
//! the load succeeded:
//!
//! - `loadTexture/loadFont/loadSoundBuffer` = C++
//!   `tryEmplaceAndThenLoadFromFile`: `map.emplace(...)` and **only if the
//!   insert happened** run `loadFromFile`. Consequences copied verbatim:
//!   - a failed load leaves the (empty) entry in the map — `has*` is `true`,
//!     `get*` returns the empty object (here: `Some(false)`);
//!   - a second load of the same id is a no-op and returns `false` (first
//!     load wins, even if it failed).
//!
//! The `loadFromFile` success here is stood in by "the file exists on disk"
//! (deterministic, no SFML). A real binary parse is issue 10's job; the
//! registry shape (id → loaded?) is what the game and the fail-soft getters
//! actually observe.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A reference to an asset slot (stand-in for an SFML handle reference; issue
/// 10 replaces the payload). Mirrors the two C++ getter shapes:
///
/// - `Present(id)` — a slot exists: either a successfully loaded handle or
///   the *empty* object left after a failed `loadFromFile` (C++ `getTexture`
///   returns that empty object's pointer; `hasTexture` is true).
/// - `Null` — no slot at all: C++ `getTextureOrNullTexture` /
///   `getFontOrNullFont` return a `static` null object so the game keeps
///   running on a missing asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRef {
    Present(String),
    Null,
}

/// C++ `hg::AssetStorage` (pimpl over three maps).
///
/// `BTreeMap` stands in for `std::unordered_map`: the C++ iteration order of
/// these maps is never observable (only `find`/`emplace` are used), so the
/// ordered container is a faithful, deterministic choice.
#[derive(Debug, Default, Clone)]
pub struct AssetStorage {
    textures: BTreeMap<String, bool>,
    fonts: BTreeMap<String, bool>,
    sound_buffers: BTreeMap<String, bool>,
    /// The on-disk path each sound buffer was loaded from (first load wins,
    /// mirroring `tryEmplaceAndThenLoadFromFile`). The C++ `AssetStorage`
    /// keeps the *decoded* `sf::SoundBuffer*` in memory; the port defers the
    /// decode to `oh-audio`, which needs the path to build its
    /// `SoundBuffer` (the `Assets` `soundBuffers`/pack-`Sounds/` registry).
    sound_paths: BTreeMap<String, PathBuf>,
}

impl AssetStorage {
    #[must_use = "mirrors the C++ default-constructed _impl"]
    pub fn new() -> Self {
        Self::default()
    }

    /// C++ `tryEmplaceAndThenLoadFromFile(map, id, path)`:
    /// `map.emplace(...)` and, **only if inserted**, `loadFromFile(path)`.
    /// The loader stand-in (`Path::exists`) decides success.
    fn try_emplace_and_load(map: &mut BTreeMap<String, bool>, id: &str, path: &Path) -> bool {
        if map.contains_key(id) {
            // Second load is a no-op: `inserted` is false, C++ returns
            // `inserted && loadFromFile(...) == false`.
            return false;
        }
        let loaded = path.exists();
        map.insert(id.to_string(), loaded);
        loaded
    }

    /// C++ `loadTexture(id, path)`.
    pub fn load_texture(&mut self, id: &str, path: &Path) -> bool {
        Self::try_emplace_and_load(&mut self.textures, id, path)
    }

    /// C++ `loadFont(id, path)`.
    pub fn load_font(&mut self, id: &str, path: &Path) -> bool {
        Self::try_emplace_and_load(&mut self.fonts, id, path)
    }

    /// C++ `loadSoundBuffer(id, path)`.
    pub fn load_sound_buffer(&mut self, id: &str, path: &Path) -> bool {
        // The decode target (issue 025e / `oh-audio`): record the path on the
        // FIRST insert only (first load wins, exactly like the C++'s
        // `emplace`-then-load — a second load of the same id is a no-op).
        let is_new = !self.sound_buffers.contains_key(id);
        let loaded = Self::try_emplace_and_load(&mut self.sound_buffers, id, path);
        if is_new {
            self.sound_paths.insert(id.to_string(), path.to_path_buf());
        }
        loaded
    }

    /// The on-disk path a registered sound buffer was loaded from (`None` for
    /// an unknown id). C++ analog: `getSoundBuffer(id)` returns the in-memory
    /// `sf::SoundBuffer*`; the port exposes the source path so `oh-audio` can
    /// decode it into a `SoundBuffer`.
    pub fn get_sound_path(&self, id: &str) -> Option<PathBuf> {
        self.sound_paths.get(id).cloned()
    }

    /// C++ `getTexture(id)` → `sf::Texture*` (non-null iff a slot exists,
    /// including a failed-load empty object). Here: `Some(loaded_flag)`.
    pub fn get_texture(&self, id: &str) -> Option<bool> {
        self.textures.get(id).copied()
    }

    /// C++ `getFont(id)`.
    pub fn get_font(&self, id: &str) -> Option<bool> {
        self.fonts.get(id).copied()
    }

    /// C++ `getSoundBuffer(id)`.
    pub fn get_sound_buffer(&self, id: &str) -> Option<bool> {
        self.sound_buffers.get(id).copied()
    }

    /// C++ `hasTexture(id)` (`find != end`; true for a failed-load empty
    /// entry).
    pub fn has_texture(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }

    /// C++ `hasFont(id)`.
    pub fn has_font(&self, id: &str) -> bool {
        self.fonts.contains_key(id)
    }

    /// C++ `hasSoundBuffer(id)`.
    pub fn has_sound_buffer(&self, id: &str) -> bool {
        self.sound_buffers.contains_key(id)
    }

    /// C++ `getTextureOrNullTexture(id)`: the slot if present, else the
    /// `static` null texture.
    pub fn texture_or_null(&self, id: &str) -> AssetRef {
        if self.textures.contains_key(id) {
            AssetRef::Present(id.to_string())
        } else {
            AssetRef::Null
        }
    }

    /// C++ `getFontOrNullFont(id)`.
    pub fn font_or_null(&self, id: &str) -> AssetRef {
        if self.fonts.contains_key(id) {
            AssetRef::Present(id.to_string())
        } else {
            AssetRef::Null
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_load_keeps_a_slot_and_second_load_is_noop() {
        let mut s = AssetStorage::new();
        let missing = std::path::Path::new("/nonexistent/nope.png");

        // first load: inserted, but the file is missing → false (failed).
        assert!(!s.load_texture("t", missing));
        // the empty entry remains: has = true, get = Some(false).
        assert!(s.has_texture("t"));
        assert_eq!(s.get_texture("t"), Some(false));

        // second load of the same id is a no-op (first load wins).
        assert!(!s.load_texture("t", missing));
        assert_eq!(s.get_texture("t"), Some(false));
    }

    #[test]
    fn missing_id_is_a_null_fallback() {
        let s = AssetStorage::new();
        assert!(!s.has_texture("nope"));
        assert_eq!(s.get_texture("nope"), None);
        assert_eq!(s.texture_or_null("nope"), AssetRef::Null);
    }

    #[test]
    fn existing_file_loads_as_present() {
        let mut s = AssetStorage::new();
        // the crate's own Cargo.toml always exists.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(s.load_texture("t", &manifest));
        assert_eq!(s.get_texture("t"), Some(true));
        assert_eq!(s.texture_or_null("t"), AssetRef::Present("t".to_string()));
        assert_eq!(s.font_or_null("nope"), AssetRef::Null);
    }
}
