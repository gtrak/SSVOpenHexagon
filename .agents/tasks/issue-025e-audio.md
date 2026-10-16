# Task 025e — port `oh-audio` (the missed subsystem) and make the phone play sound

Lane 025e. Master green (gate 4/4, 723 tests) at `c7bf1c64`. Literal-port
discipline. Commit every ~10-15 tool calls (see the hygiene section).

## Why this lane exists (the audit finding)

`oh-audio` is named in PLAN.md's crate table (mirroring
`src/SSVOpenHexagon/Core/Audio.cpp`, 318 lines, + `Data/MusicData.cpp`) but
the issue table (01–24) has **no audio issue** — the crate was planned and
never issued, so nothing was ever ported except the *seams*:

- `oh-render/src/audio.rs`: the SFML device surface over rodio 0.23
  (SoundBuffer/Sound/Music, volume/pitch/offset) — no call site opens a
  device.
- `oh-game/src/seams.rs`: `trait Audio` (11 methods, 1:1 with the C++
  `hg::Audio` public surface) + `NoAudio`.
- `oh-app/src/windowed.rs`: `NoopAudio` (menu sink) — *every* windowed path
  uses it, so all menu sounds (select/error/beep) are silent.
- `oh-lua/src/api/audio.rs`: the 11 `a_*` audio functions ARE ported and
  call the trait — so their *behavior* is a no-op today.
- Gameplay impact (this is why it matters beyond menu blips):
  `HGUpdate.cpp:94-95, 764-818` — `getMusicDMSyncFactor()` drives the
  pulse/beat timing (`status.pulse`, `beatPulse`, `pulseDelay`,
  `beatPulseDelay`); with no live music clock those reads return the
  no-audio values, so beat-synced gameplay is degraded.

## Goal

1. **`crates/oh-audio`**: the real `hg::Audio` implementation over
   `oh-render::audio` (rodio), transcribing
   `src/SSVOpenHexagon/Core/Audio.cpp` verbatim: ctor/dtor, sound/music
   volume, resume/pause/stop music, playing offsets, stopSounds,
   playSoundOverride / playPackSoundOverride / playSoundAbort /
   playPackSoundAbort, setCurrentMusicPitch, plus the music DATA side
   (`Data/MusicData.hpp`: segments, `getSegment`, the DM sync bits,
   `first_play`) — read the C++ and mirror it; cite file:line in comments
   the way the rest of the port does.
2. **Wire it in**: replace `NoopAudio`/`NoAudio` in the live composition
   (`oh-app/src/windowed.rs` → `compose_with_config`; the C++
   `main.cpp:392-401` creates `hg::Audio` and passes it to both
   `MenuGame` and `HexagonGame`). Keep the `Option<Arc<dyn Audio>>` seam
   (headless/server keep `None` — the C++'s `audio == nullptr`).
3. **Sound asset loading**: the C++ `Assets.cpp` loads `soundBuffers` /
   music into the audio object; check what the port's `oh-assets` already
   exposes (the menu already resolves `select.ogg`/`error.ogg`/
   `beep.ogg` ids) and load through `oh-render::audio`'s SoundBuffer
   decode path (ogg — check what rodio needs and whether a decoder
   feature/format crate is required; document any addition in DECISIONS
   terms, do NOT bump existing pins wantonly).
4. **The DM sync factor**: implement `getMusicDMSyncFactor()` 1:1 from the
   C++ (it reads the current music position + the music data's DM fields)
   so the gameplay pulse/beat path gets a live value again.

## Acceptance

- Host: `tools/gate.sh` 4/4 (723+ tests) + `cargo check --workspace
  --target aarch64-linux-android` green + fmt/clippy clean. Add unit tests
  for the pure parts (music segment selection/clamping, offset math,
  `getMusicDMSyncFactor` for the DM cases — the values must be derived
  from the C++ formulas, not invented).
- **On the phone** (moto g power 2021, adb `192.168.10.138:38721`, arm64):
  a logcat-visible sign that a real audio device opened and played — e.g.
  the app logging the chosen backend/device, plus an Android-side signal
  (check `adb shell dumpsys media.audio_flinger | grep -i <package>` or
  `dumpsys audio`, or an AudioTrack/AAudio creation line in logcat). A
  silent-on-device result is a FAILURE to report honestly, not to paper
  over: if cpal/rodio cannot open an Android sink, say so precisely (what
  was tried, the exact error) and stop — that is a decision point for the
  orchestrator, not a place to invent a workaround.
- Do not break the desktop path or the headless determinism tests (the
  audio seam stays `None` headless).

## Environment / hygiene

- Build: `export CARGO_APK_RELEASE_KEYSTORE=$PWD/tools/android/openhexagon-release.keystore
  CARGO_APK_RELEASE_KEYSTORE_PASSWORD=openhexagon
  RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384'`;
  `. tools/android-env.sh && bash tools/package-android.sh` (arm64 default).
- Install: `adb -s 192.168.10.138:38721 install -r
  target/release/apk/open-hexagon.apk`; wake the phone first
  (`input keyevent KEYCODE_WAKEUP`) or screenshots are 1 color.
- **Worker hygiene** (the previous lane died of context bloat at 41.7M
  tokens): commit every ~10-15 tool calls; prefer `python3 -c` PIL
  histograms / `dumpsys` greps over reading images; read an image at most
  once, at the end; if the session gets long, stop and report progress.
- Do NOT use the emulator (torn down). Do NOT touch non-phone processes.
