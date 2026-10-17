//! The C++ `death_saveReplay`'s filesystem half (HexagonGame.cpp:1067-1097)
//! — the production `ReplayStore` for the `oh_game::seams::ReplayStore`
//! seam (issue 029a1, the audit M2 fix: the only impl was
//! `NullReplayStore`, so no run ever persisted a replay file).
//!
//! The engine builds the path pieces exactly like the C++
//! `death_sendAndSaveReplay` (HexagonGame.cpp:1034-1043): the
//! `Replays/<levelId>/<diff>x/` dir (the `diffFormat` `%g` spelling,
//! HexagonGame.cpp:256-261) + the sanitized `<name>.z` filename, then
//! hands them to the store. This store performs the C++ `death_saveReplay`
//! body on top:
//!
//! * `std::filesystem::create_directories(dirPath)` (HexagonGame.cpp:1086)
//!   → `std::fs::create_directories`'s Rust equivalent `create_dir_all` —
//!   succeeds if the dir already exists, and C++ checks no return: a
//!   failure is silent here and surfaces through the write below;
//! * `crf.serialize_to_file(p)` (HexagonGame.cpp:1089) →
//!   `oh_replay::CompressedReplayFile::serialize_to_file` — the same raw
//!   compressed-bytes write the C++ `compressed_replay_file::
//!   serialize_to_file` performs (Replay.cpp:437-445). The compression
//!   itself (zlib level 9, the D-006 system-zlib pin, oracle-pinned by the
//!   CZHEX vectors) already ran in the engine's
//!   `death_send_and_save_replay` via `oh_replay::compress_replay_file`;
//! * the success/failure log lines on the `Replay` channel
//!   (HexagonGame.cpp:1087-1094) — the M7 success line the audit found
//!   missing.
//!
//! Rooting: the C++ path is CWD-relative (`Replays/...`), and every
//! production entrypoint chdirs to the game-data root before composing
//! (the desktop `dispatch::run`'s main.cpp:653 port; Android's
//! `chdir_to_base_dir`), so rooting the CWD-relative pieces on the
//! composition's `root` (= `base_dir`, the CWD equivalent) is
//! byte-identical to the C++ in production; in tests `root` is the
//! explicit tempdir / `_RELEASE` (no repo pollution, no chdir).
//!
//! Headless/server stay disk-silent exactly like the C++: the C++
//! headless `mainClient` branch only runs replay playback —
//! `inReplay()` gates `death`'s save (HexagonGame.cpp:970) and
//! `shouldSaveScore` rejects it (HexagonGame.cpp:1220-1227, plus the
//! "No local profile active" reject at :1178-1183) — and `mainServer`'s
//! executor is the same `runReplayUntilDeathAndGetScore` path, so no C++
//! mode ever writes a replay file there. Those compositions keep wiring
//! the no-op `NullReplayStore` (compose.rs); only the windowed composition
//! (desktop + Android, `WindowedClient::build`) wires this store.

use std::path::PathBuf;

use oh_game::seams::ReplayStore;
use oh_replay::CompressedReplayFile;

/// The production `ReplayStore` (module docs).
#[must_use]
pub struct RealReplayStore {
    /// The game-data root (= the C++'s CWD at save time, module docs).
    root: PathBuf,
}

impl RealReplayStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl ReplayStore for RealReplayStore {
    fn save(&self, dir_path: &str, filename: &str, crf: &CompressedReplayFile) -> bool {
        // C++: `std::filesystem::path p; p /= dirPath; p /= filename;`
        let p = self.root.join(dir_path).join(filename);
        // The C++'s log-message spelling of `p` (the CWD-relative
        // `dirPath` + `filename` — `dir_path` already carries the
        // trailing `/`).
        let p_str = format!("{dir_path}{filename}");

        // C++: `std::filesystem::create_directories(dirPath);` — the
        // return is unchecked: a failure is silent here and the write
        // below reports it.
        let _ = std::fs::create_dir_all(self.root.join(dir_path));

        if !crf.serialize_to_file(&p) {
            // C++ HexagonGame.cpp:1087-1089.
            println!("[Replay] Failed to save new compressed replay file '{p_str}'\n");
            return false;
        }

        // C++ HexagonGame.cpp:1093-1094.
        println!("[Replay] Successfully saved new compressed replay file '{p_str}'\n");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oh_replay::{compress_replay_file, decompress_replay_file, ReplayData, ReplayFile};

    fn fixture_replay_file() -> ReplayFile {
        let mut data = ReplayData::default();
        data.record_input(true, false, false, false);
        data.record_input(false, true, false, false);
        data.record_input(false, false, true, true);
        ReplayFile {
            version: 0,
            player_name: "tester".to_string(),
            seed: 42,
            data,
            pack_id: "cube_1".to_string(),
            level_id: "lv1".to_string(),
            first_play: false,
            difficulty_mult: 2.0,
            played_score: 123.45,
        }
    }

    /// (a) A real file appears at the C++-shaped path and its CONTENTS
    /// round-trip through the existing parse path (`deserialize_from_file`
    /// + `decompress_replay_file` — the headless replay run's own reader).
    #[test]
    fn save_writes_the_file_and_round_trips() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = RealReplayStore::new(root.path().to_path_buf());

        let rf = fixture_replay_file();
        let crf = compress_replay_file(&rf).expect("compression");
        let filename = format!("{}.z", rf.create_filename());
        // The engine's exact dir_path shape (the `2.0` f32's `%g` spelling
        // is `2`, `diff_format`).
        assert!(store.save("Replays/lv1/2x/", &filename, &crf));

        let path = root.path().join("Replays/lv1/2x").join(&filename);
        assert!(path.is_file(), "the replay file must exist on disk");
        let bytes = std::fs::read(&path).expect("read the written file");
        assert!(!bytes.is_empty(), "the file must not be empty");

        // The existing parse path (client.rs's headless replay run reads
        // exactly this way).
        let mut read_back = CompressedReplayFile::default();
        assert!(
            read_back.deserialize_from_file(&path),
            "the written file must open through the C++ reader"
        );
        assert_eq!(
            read_back.data, crf.data,
            "the on-disk bytes must be the compressed bytes verbatim"
        );
        let decoded = decompress_replay_file(&read_back).expect("decompression");
        assert_eq!(decoded, rf, "the round-tripped replay must equal the source");
    }

    /// (b) The directory creation is idempotent: a second save to the same
    /// already-existing dir succeeds (the C++ `create_directories` succeeds
    /// when the path exists).
    #[test]
    fn save_into_an_existing_dir_succeeds() {
        let root = tempfile::tempdir().expect("tempdir");
        let store = RealReplayStore::new(root.path().to_path_buf());

        let rf = fixture_replay_file();
        let crf = compress_replay_file(&rf).expect("compression");
        assert!(store.save("Replays/lv1/2x/", "a.z", &crf));
        assert!(
            store.save("Replays/lv1/2x/", "b.z", &crf),
            "a second save into the same (already created) dir must succeed"
        );
        assert!(root.path().join("Replays/lv1/2x/a.z").is_file());
        assert!(root.path().join("Replays/lv1/2x/b.z").is_file());
    }

    /// (c) A failure path returns `false` without panicking: when a path
    /// component exists as a FILE, the dir creation fails silently (the
    /// C++'s unchecked `create_directories`) and the write cannot open —
    /// the C++'s "Failed to save" branch, `return false`.
    #[test]
    fn save_reports_failure_without_panicking() {
        let root = tempfile::tempdir().expect("tempdir");
        // Make the `<levelId>` component a regular file: `Replays/lv1` is
        // no longer creatable as a directory.
        std::fs::create_dir_all(root.path().join("Replays")).expect("Replays dir");
        std::fs::write(root.path().join("Replays/lv1"), b"not a dir").expect("blocker file");

        let store = RealReplayStore::new(root.path().to_path_buf());
        let crf = compress_replay_file(&fixture_replay_file()).expect("compression");
        assert!(
            !store.save("Replays/lv1/2x/", "a.z", &crf),
            "an unopenable target must return false (the C++ failure branch)"
        );
    }
}
