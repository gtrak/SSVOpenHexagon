# Mapping — audio (`Core/Audio.cpp`, `Data/MusicData.cpp`)

Status: **LANDED** (issue 025e, commit `04ace03b`). Crate: `crates/oh-audio`
(+ the device surface in `crates/oh-render/src/audio.rs`, the trait in
`crates/oh-game/src/seams.rs`).

## Why this file exists

This mapping row was missing, which is how the subsystem was missed: the
crate table in PLAN.md named `oh-audio`, but no *issue* was ever written
for it and no mapping doc covered its translation units — so every lane
faithfully implemented its own contract while this one had none. Issue 27
is the guard; this file is its first repair.

## C++ → Rust surface

| C++ | Rust |
|---|---|
| `Audio::Audio(SoundBufferGetter, ...)` (`Audio.cpp:229`) | `oh_audio::Audio::new` (rodio `AudioDevice` + asset getter) |
| `setSoundVolume` / `setMusicVolume` (`:236`, `:241`) | `Audio::set_sound_volume` / `set_music_volume` |
| `resumeMusic` / `pauseMusic` / `stopMusic` (`:246`, `:251`, `:256`) | `Audio::resume_music` / `pause_music` / `stop_music` |
| `setMusicPlayingOffset{Seconds,Milliseconds}` (`:261`, `:266`) | `Audio::set_music_playing_offset_seconds` (`get_music_playing_offset_seconds`) |
| `stopSounds` (`:281`) | `Audio::stop_sounds` |
| `playSoundOverride` / `playPackSoundOverride` (`:286`, `:291`) | `Audio::play_sound_override` / `play_pack_sound_override` |
| `playSoundAbort` / `playPackSoundAbort` (`:297`, `:302`) | `Audio::play_sound_abort` |
| `setCurrentMusicPitch` (`:313`) | `Audio::set_current_music_pitch` |
| `MusicData` segments (`Data/MusicData.cpp`) | `oh-data/src/music_data.rs` (audio-facing surface) + `oh-audio`'s segment selection |
| `HexagonGame::getMusicDMSyncFactor` (`HexagonGame.cpp:1497`) | `HexagonGame::get_music_dm_sync_factor` = `difficulty_mult.powf(0.12)`, **pure** — see below |
| `getOptionalMusicDMSyncFactor` (`:1500-1503`) | `get_optional_music_dm_sync_factor` (`syncMusicToDM ? factor : 1.0`) |
| `refreshMusicPitch` (`:1505-1512`) | `HexagonGame::refresh_music_pitch` |
| `Assets.cpp` soundBuffer/music loading | `oh-assets` (asset storage) + `oh-audio`'s OGG decode into cached `SoundBuffer`s |

## Determinism note (verified during landing)

`getMusicDMSyncFactor()` is `std::pow(difficultyMult, 0.12f)` — a **pure
function of the difficulty multiplier**, despite the "DM sync" name. It
does not sample a music clock, so the live audio path cannot perturb the
simulation, and the headless determinism/replay/parity guarantees are
unaffected. `refreshMusicPitch` merely *writes* pitch into the audio
object (no read back into state) — same conclusion.

## Music-driven pulse/beat (`HGUpdate.cpp:94-95, 764-818`)

`status.pulse` / `beatPulse` / `pulseDelay` / `beatPulseDelay` are scaled
by `getMusicDMSyncFactor()` — i.e. by the difficulty multiplier, not by
playback position. With the audio object present (`Some(Arc<dyn Audio>)`)
the same values are produced; the only prior difference was the silent
audio object, which never fed back into these formulas. The headless path
(`None`) produces them identically, which the determinism suite pins.

## On-device evidence (Android, moto g power 2021, release APK)

- `[oh-audio] audio device opened (live playback)`
- `AAudioStreamBuilder_openStream() returns 0 = AAUDIO_OK` (48 kHz / 2 ch)
- `AudioTrack: createTrack_l(): usage=1 content=2` + `AudioFlinger::registerAudioTrackClient`
- `[oh-audio] decoded sound 'select.ogg' (84992 samples @ 44100Hz 2ch)`
- `dumpsys media.audio_flinger`: our track **Active**; audible output confirmed by the operator.
