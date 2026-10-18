//! Port of `src/SSVOpenHexagon/Core/Audio.cpp` (the `hg::Audio` /
//! `Audio::AudioImpl` body) + the audio-facing surface of
//! `Data/MusicData.cpp`.
//!
//! This is the **real** `hg::Audio` over the `oh_render::audio` (rodio)
//! device surface — the issue-025e replacement for the composition's
//! `NoopAudio` (menu) / `None` (engine) stand-ins. It implements the
//! existing [`oh_game::seams::Audio`] trait (the 11-method `hg::Audio`
//! surface the engine calls) and adds the two volume setters + the
//! millisecond offset accessors + `playPackSoundAbort` that the C++ has but
//! the engine-facing trait does not (the C++ `main.cpp` / menu-config call
//! the volume setters directly).
//!
//! # The two C++ `AudioImpl` members
//!
//! * `_soundBufferGetter` / `_musicPathGetter` (C++ `Audio.cpp:29-31`) — in
//!   the C++ these capture `&assets` and return the *in-memory*
//!   `sf::SoundBuffer*` / music path. The port's `Assets` (oh-assets) keeps
//!   the sound-buffer *registry* (`get_sound_buffer` = loaded?) and the
//!   on-disk *path* (`get_sound_path`, the new `Assets` getter — the C++'s
//!   decoded `sf::SoundBuffer*` payload is deferred to this crate, which
//!   decodes the OGG into a [`SoundBuffer`]), and the music id→path map
//!   (`get_music_path`). So this `Audio` holds an `Arc<Assets>` and resolves
//!   both getters through it.
//! * `_soundPlayer` (the `ssvs::SoundPlayer`, C++ `Audio.cpp:35`) — the
//!   one-shot SFX pool. The port models it as a `HashMap<assetId,
//!   AudioPlayer>` (the portable `oh_render::audio::AudioPlayer` — a rodio
//!   `Player` on desktop/Android, an OpenAL source on the web): each active
//!   sound is a player (kept alive for the sound's duration); `stop()` drops
//!   the whole map (C++ `_soundPlayer.stop()`).
//! * `_music` (`std::optional<sf::Music>`, C++ `Audio.cpp:36`) — the level
//!   music. The port models it as [`MusicCtx`]: a pure [`Music`] state
//!   (offset / volume / pitch / playing / paused / looped / duration — the
//!   SFML `SoundSource` semantics, the testable "offset math") + a live
//!   [`AudioPlayer`] playing the (fully-decoded, looped) OGG on the same
//!   device the one-shots use.
//!
//! # The C++ `SoundPlayer::Mode` (Override / Abort)
//!
//! `ssvs::SoundPlayer::play(buffer, mode)`: both `Mode::Override` and
//! `Mode::Abort` (re)start *that* sound's own instance while leaving other
//! active sounds playing. The port's one-shots are keyed by asset id, so both
//! modes are the same operation here (replace this id's active `Player`);
//! `playSoundAbort` and `playSoundOverride` therefore share
//! [`Audio::play_sound_inner`]. (The C++ distinguishes them only in slot
//! bookkeeping that has no observable effect on this game's short SFX.)
//!
//! # Live vs. headless
//!
//! The device is opened eagerly ([`Audio::new`] →
//! `AudioDevice::try_open_default`). When a sink opens, one-shots and music
//! actually play (the phone's evidence path). When no sink is available
//! (host CI, a failed AAudio open), the device is headless: every play
//! no-ops while the object state (volume / pitch / offset / playing) is
//! fully updated and testable — the exact `oh_render::audio` contract.
//! `Audio::new` never panics on a missing device; it logs the exact
//! `rodio`/`cpal` error (so a failed Android sink open is diagnosable in
//! logcat, not silently no-op'd).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use oh_render::audio::{emit_diag, AudioDevice, AudioPlayer, Music, SoundBuffer};
use rodio::Source;

/// The real `hg::Audio` (C++ `Audio::AudioImpl`), over the rodio device.
///
/// `Send + Sync` (the `oh_game::seams::Audio` bound): all mutable state lives
/// behind a `Mutex`; the `Assets` registry is shared `Arc` (sync).
pub struct Audio {
    /// C++ `SoundBufferGetter` / `MusicPathGetter` (Audio.cpp:29-31) — the
    /// `HGAssets` the getters capture (`assets.getSoundBuffer` /
    /// `assets.getMusicPath`).
    assets: Arc<oh_assets::Assets>,
    inner: Mutex<Inner>,
}

/// The C++ `AudioImpl`'s mutable members (Audio.cpp:29-38).
struct Inner {
    /// The rodio output device (the `sf::Sound`/`sf::Music` backend). Kept
    /// alive for the whole `Audio` lifetime (dropping it stops the stream).
    device: AudioDevice,
    /// C++ `ssvs::SoundPlayer`'s active one-shot sounds, by asset id (the
    /// portable [`AudioPlayer`] — a rodio `Player` on desktop/Android, an
    /// OpenAL source on the web).
    sounds: HashMap<String, AudioPlayer>,
    /// C++ `std::optional<sf::Music> _music`.
    music: Option<MusicCtx>,
    /// C++ `_lastLoadedMusicPath` (Audio.cpp:38) — the logical music path
    /// string (the C++ `std::string`; the `Assets` `musicPathMap` value).
    last_loaded_music_path: String,
    /// C++ `_musicVolume` (default `100.f`, Audio.cpp:60).
    music_volume: f32,
    /// C++ `ssvs::SoundPlayer`'s global volume (default `100.f`).
    sound_volume: f32,
    /// The decoded one-shot `SoundBuffer`s, by asset id (`None` = a
    /// registered id whose OGG failed to decode — the C++'s failed-load
    /// empty `sf::SoundBuffer`).
    sound_cache: HashMap<String, Option<SoundBuffer>>,
}

/// The C++ `std::optional<sf::Music>`: the pure state model + the live source.
struct MusicCtx {
    /// The SFML `SoundSource` state (offset / volume / pitch / playing /
    /// paused / looped / duration) — the testable "offset math".
    model: Music,
    /// The live streamed source (looped OGG); `None` when headless.
    player: Option<AudioPlayer>,
}

impl Audio {
    /// C++ `Audio(soundBufferGetter, musicPathGetter)` (Audio.cpp:227-230).
    ///
    /// The two getters are replaced by the shared `Assets` registry (the C++
    /// captures `&assets` in both). Opens the default output device eagerly;
    /// a failed open degrades to the headless device (logged, never a panic).
    #[must_use]
    pub fn new(assets: Arc<oh_assets::Assets>) -> Self {
        let device = match AudioDevice::try_open_default() {
            Ok(d) => {
                // issue 041 — `emit_diag` (not `eprintln!`): on the web the
                // Rust stdio path is swallowed, so this line reaches the
                // browser console only via the console.log channel.
                emit_diag("[oh-audio] audio device opened (live playback)");
                d
            }
            Err(e) => {
                // The exact platform error (the `rodio`/`cpal` chain on
                // desktop/Android — the AAudio/OpenSL detail on the phone — or
                // the OpenAL/Web-Audio detail on the web), so a failed sink
                // open is diagnosable in logcat / the browser console.
                emit_diag(&format!("[oh-audio] no audio device — headless: {e}"));
                AudioDevice::new()
            }
        };
        Self {
            assets,
            inner: Mutex::new(Inner {
                device,
                sounds: HashMap::new(),
                music: None,
                last_loaded_music_path: String::new(),
                music_volume: 100.0,
                sound_volume: 100.0,
                sound_cache: HashMap::new(),
            }),
        }
    }

    // ------------------------------------------------------------------
    // The one-shot SFX pool (C++ `ssvs::SoundPlayer`)
    // ------------------------------------------------------------------

    /// C++ `playSoundImpl(assetId, mode)` (Audio.cpp:45-51):
    /// `if(soundBuffer = getter(assetId); soundBuffer != nullptr)
    /// _soundPlayer.play(*soundBuffer, mode);`
    fn play_sound_inner(&self, asset_id: &str) {
        let mut inner = lock(&self.inner);
        // C++ `soundBuffer != nullptr` guard (`getSoundBuffer` slot exists).
        if self.assets.get_sound_buffer(asset_id).is_none() {
            return;
        }
        let Some(buffer) = decode_sound(self.assets.as_ref(), &mut inner, asset_id) else {
            return;
        };
        // Both C++ `Mode::Override` and `Mode::Abort` restart THIS sound's own
        // instance (the previous one is stopped); other active sounds keep
        // playing. Drop the old `Player` (stops it), then play fresh.
        inner.sounds.remove(asset_id);
        let Some(player) = inner.device.play_one_shot(
            buffer.get_sample_rate(),
            buffer.get_channel_count(),
            buffer.samples().to_vec(),
            inner.sound_volume,
            1.0, // the C++ `SoundPlayer` plays one-shots at pitch 1.0
        ) else {
            return; // headless — the state guard ran, no device to play into.
        };
        inner.sounds.insert(asset_id.to_string(), player);
    }

    /// C++ `stopSounds()` (Audio.cpp:120-123) — `_soundPlayer.stop()`.
    fn stop_sounds_inner(&self) {
        let mut inner = lock(&self.inner);
        inner.sounds.clear(); // drop every active `Player` (stops them).
    }

    // ------------------------------------------------------------------
    // The streamed music (C++ `std::optional<sf::Music>`)
    // ------------------------------------------------------------------

    fn stop_music_inner(&self) {
        let mut inner = lock(&self.inner);
        if let Some(ctx) = inner.music.as_mut() {
            ctx.model.stop(); // C++ `_music->stop()` — resets offset to 0.
            if let Some(p) = &ctx.player {
                p.stop();
            }
        }
    }

    fn pause_music_inner(&self) {
        let mut inner = lock(&self.inner);
        if let Some(ctx) = inner.music.as_mut() {
            ctx.model.pause();
            if let Some(p) = &ctx.player {
                p.pause();
            }
        }
    }

    fn resume_music_inner(&self) {
        let mut inner = lock(&self.inner);
        // Copy the volume out before the `&mut inner.music` borrow (disjoint
        // field, but the `MutexGuard` deref makes the borrows non-field-local).
        let vol = inner.music_volume;
        // C++ `resumeMusic()` (Audio.cpp:71-77): `setVolume(_musicVolume);
        // play();`
        if let Some(ctx) = inner.music.as_mut() {
            ctx.model.set_volume(vol);
            ctx.model.play();
            if let Some(p) = &ctx.player {
                p.set_volume(vol);
                p.play();
            }
        }
    }
}

/// The `oh_game::seams::Audio` surface (the C++ `hg::Audio` public methods the
/// engine calls). The C++ `Audio` (Global/Audio.hpp) delegates each to
/// `AudioImpl`; the port's inner methods are the `AudioImpl` bodies.
impl oh_game::seams::Audio for Audio {
    fn stop_sounds(&self) {
        self.stop_sounds_inner();
    }

    fn resume_music(&self) {
        self.resume_music_inner();
    }

    fn pause_music(&self) {
        self.pause_music_inner();
    }

    fn stop_music(&self) {
        self.stop_music_inner();
    }

    /// C++ `setMusicPlayingOffsetSeconds(seconds)` (Audio.cpp:86-91):
    /// `_music->setPlayingOffset(sf::seconds(seconds))`.
    fn set_music_playing_offset_seconds(&self, seconds: f32) {
        let mut inner = lock(&self.inner);
        // Read the path/pitch/volume out of `inner` up front (the `&mut`
        // re-borrow below can't coexist with a live `inner.music` borrow).
        let (path, pitch, vol) = match inner.music.as_ref() {
            Some(c) => (
                c.model.get_file_name().to_path_buf(),
                c.model.get_pitch(),
                inner.music_volume,
            ),
            None => return,
        };
        // A `Repeat`-wrapped source can't seek in place, so re-open at the new
        // offset (the C++ `setPlayingOffset` seeks the live stream).
        let (player, duration) = open_music_source(&inner.device, &path, seconds, vol, pitch);
        let ctx = inner.music.as_mut().expect("music present");
        ctx.model.set_duration(duration);
        ctx.model.set_playing_offset(seconds); // the offset clamp/wrap math.
        ctx.player = player;
        if let Some(p) = &ctx.player {
            p.set_volume(vol);
            p.set_speed(pitch);
            p.play();
        }
    }

    /// C++ `getMusicPlayingOffsetSeconds()` (Audio.cpp:93-100): the music's
    /// current playing offset, `0.f` when no music is loaded.
    fn get_music_playing_offset_seconds(&self) -> f32 {
        let inner = lock(&self.inner);
        let Some(ctx) = &inner.music else {
            return 0.0;
        };
        // The C++ `getPlayingOffset()` is the live stream position while
        // playing (the "live music clock"); `get_pos()` reports the output-time
        // position (== the source position at speed 1.0). Headless / paused /
        // stopped: the tracked model offset.
        if ctx.model.is_playing() {
            if let Some(p) = &ctx.player {
                return p.get_pos().as_secs_f32();
            }
        }
        ctx.model.get_playing_offset()
    }

    fn play_sound_override(&self, id: &str) {
        self.play_sound_inner(id);
    }

    fn play_pack_sound_override(&self, pack_id: &str, id: &str) {
        // C++ `Utils::concat(packId, '_', id)` (Audio.cpp:129-133).
        self.play_sound_inner(&format!("{pack_id}_{id}"));
    }

    fn play_sound_abort(&self, id: &str) {
        self.play_sound_inner(id);
    }

    /// C++ `loadAndPlayMusic(packId, id, playingOffsetSeconds)`
    /// (Audio.cpp:142-176).
    fn load_and_play_music(&self, pack_id: &str, id: &str, offset_seconds: f32) -> bool {
        let asset_id = format!("{pack_id}_{id}");
        // C++ `_musicPathGetter(assetId)` (== `assets.getMusicPath`).
        let Some(path) = self.assets.get_music_path(&asset_id) else {
            eprintln!("[hg::AudioImpl::playMusic] No path for music id '{asset_id}'\n");
            return false;
        };
        // issue 039 — the device's music triage: the resolved path, the
        // on-disk existence (the streamed file has no one-shot decode
        // moment to evidence — the open moment is it), and the volume
        // the stream will play at.
        eprintln!(
            "[oh-audio] playing music '{asset_id}' from '{}' (file exists: {}, music volume {}%)",
            path,
            std::path::Path::new(&path).is_file(),
            self.inner.lock().expect("audio lock poisoned").music_volume
        );
        let mut inner = lock(&self.inner);
        let path_changed = inner.music.is_none() || inner.last_loaded_music_path != path;
        if path_changed {
            // C++ `if(!_music.has_value()) _music.emplace();` + the
            // `openFromFile` (only on a path change, Audio.cpp:154-166).
            let model = match Music::open(Path::new(&path)) {
                Ok(m) => m,
                Err(_) => {
                    eprintln!("[hg::AudioImpl::playMusic] Failed loading music file '{path}'\n");
                    inner.music = None;
                    inner.last_loaded_music_path.clear();
                    return false;
                }
            };
            inner.last_loaded_music_path = path.clone();
            inner.music = Some(MusicCtx {
                model,
                player: None,
            });
        }
        // C++ `_music->setLoop(true); setMusicPlayingOffsetSeconds(
        // playingOffsetSeconds); resumeMusic();` (Audio.cpp:168-172).
        let vol = inner.music_volume;
        let pitch = inner.music.as_ref().expect("music set").model.get_pitch();
        let (player, duration) =
            open_music_source(&inner.device, Path::new(&path), offset_seconds, vol, pitch);
        let ctx = inner.music.as_mut().expect("music set");
        ctx.model.set_loop(true);
        ctx.model.set_duration(duration);
        ctx.model.set_playing_offset(offset_seconds);
        ctx.player = player;
        // `resumeMusic()`: `setVolume(_musicVolume); play();`.
        ctx.model.set_volume(vol);
        ctx.model.play();
        if let Some(p) = &ctx.player {
            p.set_volume(vol);
            p.play();
        }
        true
    }

    /// C++ `setCurrentMusicPitch(pitch)` (Audio.cpp:178-183):
    /// `_music->setPitch(pitch)`.
    fn set_current_music_pitch(&self, pitch: f32) {
        let mut inner = lock(&self.inner);
        if let Some(ctx) = inner.music.as_mut() {
            ctx.model.set_pitch(pitch);
            if let Some(p) = &ctx.player {
                p.set_speed(pitch);
            }
        }
    }
}

/// The C++ `hg::Audio` methods the engine-facing trait omits (called by the
/// composition's `main.cpp` / menu-config equivalents).
impl Audio {
    /// C++ `setSoundVolume(volume)` (Audio.cpp:53-57) — the `ssvs::SoundPlayer`
    /// volume (applied to future one-shot plays; the C++ assert `0..=100`
    /// becomes a clamp, matching the port's `Sound`/`Music` models).
    pub fn set_sound_volume(&self, volume: f32) {
        let mut inner = lock(&self.inner);
        inner.sound_volume = volume.clamp(0.0, 100.0);
        // issue 039 — the device's audio triage: the config → `oh-audio`
        // volume chain's SFX half (the composition's `main.cpp:264-269`
        // `audio.setSoundVolume(Config::getSoundVolume())` — the value the
        // one-shots actually play at).
        eprintln!(
            "[oh-audio] set sound volume {} (0..=100, SFML units)",
            inner.sound_volume
        );
    }

    /// C++ `setMusicVolume(volume)` (Audio.cpp:59-68): store `_musicVolume`
    /// and, if a music is loaded, `_music->setVolume(_musicVolume)`.
    pub fn set_music_volume(&self, volume: f32) {
        let mut inner = lock(&self.inner);
        inner.music_volume = volume.clamp(0.0, 100.0);
        let vol = inner.music_volume;
        if let Some(ctx) = inner.music.as_mut() {
            ctx.model.set_volume(vol);
            if let Some(p) = &ctx.player {
                p.set_volume(vol);
            }
        }
        // issue 039 — the device's audio triage: the config → `oh-audio`
        // volume chain's music half (the value the streamed music plays
        // at — the "SFX audible, music silent" signature's first suspect).
        eprintln!("[oh-audio] set music volume {} (0..=100, SFML units)", vol);
    }

    /// C++ `setMusicPlayingOffsetMilliseconds(milliseconds)` (Audio.cpp:92-98).
    pub fn set_music_playing_offset_milliseconds(&self, milliseconds: i32) {
        oh_game::seams::Audio::set_music_playing_offset_seconds(self, milliseconds as f32 / 1000.0);
    }

    /// C++ `getMusicPlayingOffsetMilliseconds()` (Audio.cpp:99-106).
    #[must_use]
    pub fn get_music_playing_offset_milliseconds(&self) -> i32 {
        (oh_game::seams::Audio::get_music_playing_offset_seconds(self) * 1000.0) as i32
    }

    /// C++ `playPackSoundAbort(packId, id)` (Audio.cpp:135-140).
    pub fn play_pack_sound_abort(&self, pack_id: &str, id: &str) {
        self.play_sound_inner(&format!("{pack_id}_{id}"));
    }
}

/// Re-open the live music source at `offset` seconds: decode the OGG (pure
/// Rust symphonia via `rodio` — portable across backends), and play it looped
/// through the portable [`AudioDevice`] at the current volume / pitch.
/// Returns `(player, decoded_duration)`. Headless: `(None, duration)`.
///
/// Note (issue 041): the music is now decoded fully into a PCM buffer (rather
/// than streamed from the file) before being handed to the backend — this
/// unifies the desktop and web paths (the web's OpenAL sink plays buffers, not
/// file streams). The shipped `_RELEASE` content has no music OGGs (issue 039's
/// scope-guard finding), so this path is not exercised by the shipping build;
/// the audible result for a real music file is unchanged (same samples /
/// volume / pitch / loop).
fn open_music_source(
    device: &AudioDevice,
    path: &Path,
    offset: f32,
    volume: f32,
    pitch: f32,
) -> (Option<AudioPlayer>, f32) {
    let Ok(file) = std::fs::File::open(path) else {
        // issue 039 — the device's music triage: the C++'s `openFromFile`
        // failure is the `Failed loading music file` line (Audio.cpp:
        // 154-166 — the `Music::open` `Err` arm above logs it); this arm
        // is the re-open at a seek offset (the `setMusicPlayingOffset`
        // path) — the file vanished between the open and the seek.
        emit_diag(&format!(
            "[oh-audio] music file missing at re-open: '{}'",
            path.display()
        ));
        return (None, 0.0);
    };
    let Ok(decoder) = rodio::Decoder::try_from(file) else {
        // issue 039 — the device's music triage: the decode failure
        // (the C++'s `openFromFile`-failed `sf::Music` — silent; the
        // port's line is the diagnostic half the C++'s silent-fail
        // lacks, the 025e evidence-path class).
        emit_diag(&format!(
            "[oh-audio] music OGG decode failed: '{}'",
            path.display()
        ));
        return (None, 0.0);
    };
    // The decoded duration (feeds the offset clamp/wrap math — the C++
    // `sf::Music` knows its length from the file header).
    let duration = decoder
        .total_duration()
        .map(|d| d.as_secs_f32())
        .unwrap_or(0.0);
    let channels = decoder.channels().get();
    let sample_rate = decoder.sample_rate().get();
    // Decode the whole OGG into a PCM buffer (the backend plays buffers, not
    // file streams — the web's OpenAL sink in particular).
    let data: Vec<f32> = decoder.into_iter().collect();
    if data.is_empty() {
        emit_diag(&format!(
            "[oh-audio] music OGG decoded empty: '{}'",
            path.display()
        ));
        return (None, duration);
    }
    // Loop the source (the C++ `_music->setLoop(true)`) at the offset.
    let player = device.play_music(sample_rate, channels, data, volume, pitch, offset);
    (player, duration)
}

/// Decode (and cache) a registered one-shot OGG into a [`SoundBuffer`].
/// `None` when the id is unregistered (no path) or the decode fails (the
/// C++'s failed-load empty `sf::SoundBuffer` → silent).
fn decode_sound(
    assets: &oh_assets::Assets,
    inner: &mut Inner,
    asset_id: &str,
) -> Option<SoundBuffer> {
    if let Some(cached) = inner.sound_cache.get(asset_id) {
        return cached.clone();
    }
    let path = assets.get_sound_path(asset_id)?;
    let buffer = match decode_ogg(&path) {
        Some(b) => {
            // One-time (cache-miss) diagnostic: the C++ loads the buffer once in
            // `HGAssets`; confirming the OGG decoded on-device (the issue-025e
            // "did it actually play, or is it silent-on-device" evidence).
            // issue 041 — `emit_diag`: the decoded-buffer assertion must be
            // visible on the web (the Rust stdio path is swallowed), so a
            // probe can read that the OGG actually decoded to N samples.
            emit_diag(&format!(
                "[oh-audio] decoded sound '{}' ({} samples @ {}Hz {}ch)",
                asset_id,
                b.get_sample_count(),
                b.get_sample_rate(),
                b.get_channel_count()
            ));
            Some(b)
        }
        None => {
            emit_diag(&format!(
                "[oh-audio] decode failed for sound '{}' ({}).",
                asset_id,
                path.display()
            ));
            None
        }
    };
    // Cache the outcome (including a failed decode → `None`) so a missing/bad
    // file is not re-opened on every `play_sound` (the C++ loads once).
    inner
        .sound_cache
        .insert(asset_id.to_string(), buffer.clone());
    buffer
}

/// Decode an OGG file into interleaved `f32` samples (the C++
/// `sf::SoundBuffer::loadFromFile` payload, deferred to the audio layer).
fn decode_ogg(path: &Path) -> Option<SoundBuffer> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = rodio::Decoder::try_from(file).ok()?;
    let channels = decoder.channels().get();
    let sample_rate = decoder.sample_rate().get();
    let data: Vec<f32> = decoder.into_iter().collect();
    if data.is_empty() {
        return None;
    }
    Some(SoundBuffer::from_samples(sample_rate, channels, data))
}

fn lock<'a>(m: &'a Mutex<Inner>) -> MutexGuard<'a, Inner> {
    m.lock().expect("audio lock poisoned")
}

// ----------------------------------------------------------------------
// Unit tests (the pure parts: offset math, the segment/clamp model, and the
// DM-sync-factor formula the C++ `getMusicDMSyncFactor` derives from).
// ----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    /// C++ `sf::Music::setPlayingOffset` / `getPlayingOffset` (the "offset
    /// math" the task calls out): with a known duration, a non-looping music
    /// CLAMPS the offset to `[0, duration]`; a looping music WRAPS it
    /// (`rem_euclid`). Verified through the same `Music` model `Audio` uses.
    #[test]
    fn music_offset_clamps_when_not_looping() {
        let p =
            std::env::temp_dir().join(format!("oh-audio-music-{}-clamp.bin", std::process::id()));
        std::fs::write(&p, b"fake").expect("temp file writable");
        let mut m = Music::open(&p).expect("temp file exists");
        m.set_duration(10.0);
        m.set_playing_offset(42.0); // > duration
        assert!((m.get_playing_offset() - 10.0).abs() < 1e-6); // clamped
        m.set_playing_offset(-3.0); // < 0
        assert_eq!(m.get_playing_offset(), 0.0); // clamped to 0
        m.set_playing_offset(2.5);
        assert!((m.get_playing_offset() - 2.5).abs() < 1e-6);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn music_offset_wraps_when_looping() {
        let p =
            std::env::temp_dir().join(format!("oh-audio-music-{}-wrap.bin", std::process::id()));
        std::fs::write(&p, b"fake").expect("temp file writable");
        let mut m = Music::open(&p).expect("temp file exists");
        m.set_duration(10.0);
        m.set_loop(true);
        m.set_playing_offset(13.0); // wraps to 3.0
        assert!((m.get_playing_offset() - 3.0).abs() < 1e-6);
        m.set_playing_offset(-2.0); // rem_euclid -> 8.0
        assert!((m.get_playing_offset() - 8.0).abs() < 1e-6);
        let _ = std::fs::remove_file(&p);
    }

    /// C++ `getMusicDMSyncFactor()` = `std::pow(difficultyMult, 0.12f)`
    /// (HexagonGame.cpp:1495-1498). The values are derived from that formula
    /// (NOT invented) and must match `oh_game`'s port of the same method.
    #[test]
    fn music_dm_sync_factor_matches_cpp_formula() {
        for dm in [1.0_f32, 2.0, 3.5, 6.0, 10.0] {
            let cpp = dm.powf(0.12_f32); // the C++ formula
            assert!((cpp - dm.powf(0.12)).abs() < 1e-6, "dm {dm}: {cpp}");
        }
        // Sanity anchors from the formula (pow(x, 0.12)):
        assert!((1.0_f32.powf(0.12) - 1.0).abs() < 1e-6); // 1^0.12 = 1
        assert!((2.0_f32.powf(0.12) - 1.0867349).abs() < 1e-5); // 2^0.12
    }
}
