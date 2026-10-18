//! The audio **device** surface: `sf::SoundBuffer` / `sf::Sound` / `sf::Music`
//! + an `AudioDevice`.
//!
//! This is the low-level device surface the `sf::Sound`/`sf::Music` state
//! maps onto (play / stop / pitch / volume / playing-offset). The **game-level
//! volume curves** (per-track mixing, beat-pulse, difficulty curves) are
//! `oh-audio`'s job, not this crate's — `oh-audio` wraps this surface.
//!
//! The output device is a **portable seam** (`AudioDevice` + `AudioPlayer`)
//! with a platform backend behind it:
//!   * desktop / Android — `rodio` 0.23 (`MixerDeviceSink` + `Player`), the
//!     issue-025e backend (`mod rodio_backend`);
//!   * `wasm32-unknown-emscripten` (the web) — emscripten's OpenAL
//!     (`-lopenal`, the SFML-parity stack: SFML's audio module IS OpenAL, and
//!     emscripten's OpenAL maps onto Web Audio). `rodio`/`cpal` have NO
//!     emscripten backend (cpal's browser host is gated on `target_os =
//!     "unknown"`, so emscripten falls to cpal's null host and `open_default`
//!     returns `NoDevice`) — the OpenAL sink is the web's real device
//!     (`mod openal`).
//!
//! It is created lazily (`AudioDevice::open_default`); headless builds (no
//! audio device) simply have no backend and the `play` paths no-op while the
//! object state (volume / pitch / offset / playing) is fully testable.
//!
//! SFML semantics mirrored: volume is `0..=100` (default `100`), pitch is
//! `1.0` by default, `setPlayingOffset` is in seconds and is clamped to the
//! buffer length unless the source loops. The SFML-unit → linear gain mapping
//! (`volume / 100.0`) is applied identically by BOTH backends, so the desktop
//! and web paths agree on the numbers the device actually plays at.

use std::path::PathBuf;

use oh_math::ssvu::math::get_clamped;

use crate::error::RenderError;

// The platform backend(s) behind the portable seam (cfg-gated: rodio on
// desktop/Android, emscripten's OpenAL on the web).
#[cfg(target_os = "emscripten")]
mod openal;
#[cfg(not(target_os = "emscripten"))]
mod rodio_backend;

/// `sf::SoundBuffer` — a decoded PCM buffer.
#[derive(Clone, Debug)]
pub struct SoundBuffer {
    sample_rate: u32,
    channels: u16,
    /// Interleaved `f32` samples in `[-1, 1]`, `channels * frames`.
    data: Vec<f32>,
}

impl SoundBuffer {
    /// Wrap already-decoded interleaved `f32` samples.
    #[must_use]
    pub fn from_samples(sample_rate: u32, channels: u16, data: Vec<f32>) -> Self {
        Self {
            sample_rate,
            channels,
            data,
        }
    }

    /// `getSampleRate()`.
    #[must_use]
    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// `getChannelCount()`.
    #[must_use]
    pub fn get_channel_count(&self) -> u16 {
        self.channels
    }

    /// `getSampleCount()`.
    #[must_use]
    pub fn get_sample_count(&self) -> usize {
        self.data.len()
    }

    /// `getDuration()` (seconds).
    #[must_use]
    pub fn get_duration(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.data.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
    }

    /// The interleaved samples (GL/rodio source).
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.data
    }
}

/// `sf::Sound` — a buffered sound source.
#[derive(Clone, Debug)]
pub struct Sound {
    buffer: Option<SoundBuffer>,
    /// `0..=100` (default `100`).
    volume: f32,
    /// Pitch (default `1.0`).
    pitch: f32,
    channel: u16,
    /// Playing offset in seconds.
    offset: f32,
    playing: bool,
    paused: bool,
    looped: bool,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            buffer: None,
            volume: 100.0,
            pitch: 1.0,
            channel: 0,
            offset: 0.0,
            playing: false,
            paused: false,
            looped: false,
        }
    }
}

impl Sound {
    /// `sf::Sound()` (no buffer yet).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `setBuffer`.
    pub fn set_buffer(&mut self, buffer: SoundBuffer) {
        self.buffer = Some(buffer);
    }

    /// `getBuffer()`.
    #[must_use]
    pub fn get_buffer(&self) -> Option<&SoundBuffer> {
        self.buffer.as_ref()
    }

    /// The source duration in seconds (`0` when no buffer is set).
    #[must_use]
    pub fn get_duration(&self) -> f32 {
        self.buffer.as_ref().map_or(0.0, SoundBuffer::get_duration)
    }

    // --- `sf::SoundSource` shared surface -----------------------------------
    /// `play()` — start playing (from `0`, or from the set offset when not
    /// looping; SFML resumes the current position).
    pub fn play(&mut self) {
        self.playing = true;
        self.paused = false;
        if !self.looped {
            self.offset = 0.0;
        }
    }

    /// `stop()` — stop and rewind to `0`.
    pub fn stop(&mut self) {
        self.playing = false;
        self.paused = false;
        self.offset = 0.0;
    }

    /// `pause()`.
    pub fn pause(&mut self) {
        if self.playing {
            self.paused = true;
        }
    }

    /// `resume()`.
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// `isPlaying()` — playing and not paused.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing && !self.paused
    }

    /// `isPaused()`.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// `setVolume` (clamped `0..=100`).
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = get_clamped(volume, 0.0, 100.0);
    }
    /// `getVolume()`.
    #[must_use]
    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    /// `setPitch`.
    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
    }
    /// `getPitch()`.
    #[must_use]
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    /// `setChannel`.
    pub fn set_channel(&mut self, channel: u16) {
        self.channel = channel;
    }
    /// `getChannel()`.
    #[must_use]
    pub fn get_channel(&self) -> u16 {
        self.channel
    }

    /// `setLoop`.
    pub fn set_loop(&mut self, looped: bool) {
        self.looped = looped;
    }
    /// `getLoop()`.
    #[must_use]
    pub fn get_loop(&self) -> bool {
        self.looped
    }

    /// `setPlayingOffset(seconds)` — clamped to the duration when not looping.
    pub fn set_playing_offset(&mut self, seconds: f32) {
        let duration = self.get_duration();
        if self.looped && duration > 0.0 {
            self.offset = seconds.rem_euclid(duration);
        } else if duration > 0.0 {
            self.offset = get_clamped(seconds, 0.0, duration);
        } else {
            self.offset = seconds.max(0.0);
        }
    }
    /// `getPlayingOffset()`.
    #[must_use]
    pub fn get_playing_offset(&self) -> f32 {
        self.offset
    }

    /// Play into an audio device: build a portable [`AudioPlayer`] carrying
    /// this buffer at the current volume / pitch. Returns `None` headless (no
    /// device) or when no buffer is set. The caller keeps the returned
    /// `AudioPlayer` alive for the sound's duration.
    pub fn play_into(&self, device: &AudioDevice) -> Option<AudioPlayer> {
        let buffer = self.buffer.as_ref()?;
        device.play_one_shot(
            buffer.get_sample_rate(),
            buffer.get_channel_count(),
            buffer.samples().to_vec(),
            self.volume,
            self.pitch,
        )
    }
}

/// `sf::Music` — a streamed (file-based) sound source.
///
/// Decoding/streaming of the actual file is the caller's concern at the
/// `rodio::Decoder` boundary; the device surface (play / stop / pitch /
/// volume / playing-offset) is fully modelled here and testable headless.
#[derive(Clone, Debug)]
pub struct Music {
    path: PathBuf,
    /// `0..=100` (default `100`).
    volume: f32,
    pitch: f32,
    channel: u16,
    offset: f32,
    playing: bool,
    paused: bool,
    looped: bool,
    /// `0` when the (not-yet-decoded) duration is unknown.
    duration: f32,
}

impl Music {
    /// `sf::Music::openFromFile(path)`.
    ///
    /// # Errors
    /// [`RenderError::Io`] if the file does not exist.
    pub fn open(path: &std::path::Path) -> Result<Self, RenderError> {
        std::fs::metadata(path).map_err(|e| RenderError::Io(e.kind()))?;
        Ok(Self {
            path: path.to_path_buf(),
            volume: 100.0,
            pitch: 1.0,
            channel: 0,
            offset: 0.0,
            playing: false,
            paused: false,
            looped: false,
            duration: 0.0,
        })
    }

    /// `getFileName()` (the C++ `getFileName()`).
    #[must_use]
    pub fn get_file_name(&self) -> &std::path::Path {
        &self.path
    }

    /// Record the decoded duration (seconds) once known.
    pub fn set_duration(&mut self, seconds: f32) {
        self.duration = seconds;
    }

    /// The duration in seconds (`0` until `set_duration`).
    #[must_use]
    pub fn get_duration(&self) -> f32 {
        self.duration
    }

    // --- `sf::SoundSource` shared surface -----------------------------------
    pub fn play(&mut self) {
        self.playing = true;
        self.paused = false;
        if !self.looped {
            self.offset = 0.0;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.paused = false;
        self.offset = 0.0;
    }

    pub fn pause(&mut self) {
        if self.playing {
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing && !self.paused
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = get_clamped(volume, 0.0, 100.0);
    }
    #[must_use]
    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
    }
    #[must_use]
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    pub fn set_channel(&mut self, channel: u16) {
        self.channel = channel;
    }
    #[must_use]
    pub fn get_channel(&self) -> u16 {
        self.channel
    }

    pub fn set_loop(&mut self, looped: bool) {
        self.looped = looped;
    }
    #[must_use]
    pub fn get_loop(&self) -> bool {
        self.looped
    }

    pub fn set_playing_offset(&mut self, seconds: f32) {
        if self.looped && self.duration > 0.0 {
            self.offset = seconds.rem_euclid(self.duration);
        } else if self.duration > 0.0 {
            self.offset = get_clamped(seconds, 0.0, self.duration);
        } else {
            self.offset = seconds.max(0.0);
        }
    }
    #[must_use]
    pub fn get_playing_offset(&self) -> f32 {
        self.offset
    }
}

/// Emit one audio diagnostic line where it is actually visible:
/// * emscripten — the browser console (Rust stdio is swallowed by the
///   emscripten glue's default `/dev/tty` null path, so `eprintln!` never
///   reaches a human; `console.log` does — the issue-026 sink's channel);
/// * every other target — `eprintln!` (the desktop/Android logcat path the
///   issue-039 audio triage lines use, unchanged).
///
/// This is the single channel the web's `[oh-audio]` diagnostics travel on, so
/// a probe (or a human in DevTools) can read which world the device is in and
/// whether the Web Audio context is unlocked.
#[cfg(target_os = "emscripten")]
pub fn emit_diag(msg: &str) {
    web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(msg));
}
#[cfg(not(target_os = "emscripten"))]
pub fn emit_diag(msg: &str) {
    eprintln!("{msg}");
}

/// A portable handle to a sound that is (or will be) playing, abstracting over
/// the concrete backend (a `rodio::Player` on desktop/Android, an OpenAL
/// source on the web). The engine (`oh-audio`) holds these and drives
/// stop / pause / play / volume / speed / position through them without
/// naming a backend type.
pub struct AudioPlayer {
    backend: Box<dyn PlayerBackend>,
}

impl std::fmt::Debug for AudioPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioPlayer").finish_non_exhaustive()
    }
}

/// The backend half of an [`AudioPlayer`] (the `rodio::Player` or OpenAL
/// source). `Send` so the `oh-audio` `Audio` (a `Send + Sync` seam type) can
/// hold players behind its `Mutex`.
trait PlayerBackend: Send {
    /// `stop()` — stop and rewind to 0 (the SFML `SoundSource::stop`).
    fn stop(&self);
    /// `pause()`.
    fn pause(&self);
    /// `play()` / `resume()`.
    fn play(&self);
    /// Set the gain. `v01` is LINEAR `0.0..=1.0` (the caller has already
    /// applied the SFML-unit → linear mapping).
    fn set_volume(&self, v01: f32);
    /// Set the playback rate (the SFML `pitch`).
    fn set_speed(&self, pitch: f32);
    /// The current playing position (rodio: output-time; OpenAL: source
    /// offset). `0` when the position is unknown / not playing.
    fn get_pos(&self) -> std::time::Duration;
    /// A short state string for diagnostics (e.g. the AL source state).
    fn describe(&self) -> String;
}

impl AudioPlayer {
    /// `stop()` — stop and rewind to 0.
    pub fn stop(&self) {
        self.backend.stop();
    }
    /// `pause()`.
    pub fn pause(&self) {
        self.backend.pause();
    }
    /// `play()` / `resume()`.
    pub fn play(&self) {
        self.backend.play();
    }
    /// `setVolume` — SFML-style `0..=100` (the SAME mapping the desktop path
    /// uses: `/100.0` → linear, clamped). Applied identically on the web so
    /// both backends play at the same gain for a given SFML volume.
    pub fn set_volume(&self, sfml_units: f32) {
        self.backend
            .set_volume((sfml_units / 100.0).clamp(0.0, 1.0));
    }
    /// `setPitch` — the playback rate.
    pub fn set_speed(&self, pitch: f32) {
        self.backend.set_speed(pitch);
    }
    /// `getPlayingOffset` (the live source position, seconds).
    #[must_use]
    pub fn get_pos(&self) -> std::time::Duration {
        self.backend.get_pos()
    }
    /// A short state string for diagnostics.
    #[must_use]
    pub fn describe(&self) -> String {
        self.backend.describe()
    }
}

/// The portable output-device backend the [`AudioDevice`] seam delegates to.
/// `Send + Sync` so the `oh-audio` `Audio` can hold the device behind its
/// `Mutex` (the `Send + Sync` seam bound).
trait DeviceBackend: Send + Sync {
    /// Whether a live output sink is attached.
    fn is_ready(&self) -> bool;
    /// Play a one-shot `sf::Sound` (interleaved `f32`, plays once from 0).
    /// `volume` is SFML-style `0..=100`; `pitch` the rate. Returns `None`
    /// when a parameter is degenerate (the backend keeps the headless no-op
    /// semantics).
    fn play_one_shot(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
    ) -> Option<AudioPlayer>;
    /// Play a looped `sf::Music` (interleaved `f32`, repeats forever) from
    /// `offset_secs`. `volume` is SFML-style `0..=100`; `pitch` the rate.
    fn play_music(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
        offset_secs: f32,
    ) -> Option<AudioPlayer>;
    /// Resume / unlock the audio on the first user gesture. A no-op on
    /// desktop/Android (no autoplay policy); on the web the browser requires a
    /// user gesture before a Web Audio context will run.
    fn unlock(&self);
    /// A one-line diagnostic: which world (device present / absent) + the
    /// unlock (context) state + the last source state / error.
    fn describe(&self) -> String;
}

/// The error of a failed default-device open, with the platform's own detail
/// (the `rodio`/`cpal` error on desktop/Android, the OpenAL/Web-Audio detail
/// on the web) so a failed open is diagnosable, not silently no-op'd. The
/// `Display` is just the detail (the caller supplies the "no audio device"
/// framing — the issue-039 `oh-audio` line does), so it reads
/// `no audio device — headless: <platform detail>`.
#[derive(Debug, Clone)]
pub struct AudioOpenError {
    /// The platform's own error detail.
    pub detail: String,
}

impl std::fmt::Display for AudioOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}
impl std::error::Error for AudioOpenError {}

/// The audio output device (the portable seam over the platform backend:
/// rodio on desktop/Android, emscripten's OpenAL on the web).
pub struct AudioDevice {
    backend: Option<Box<dyn DeviceBackend>>,
}

impl std::fmt::Debug for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("backend", &self.backend.as_ref().map(|_| "present"))
            .finish_non_exhaustive()
    }
}

impl AudioDevice {
    /// A headless device with no output backend (used when no audio is desired,
    /// e.g. in tests and the headless game path).
    #[must_use]
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Open the default output device.
    ///
    /// # Errors
    /// [`RenderError::AudioUnavailable`] when no output device exists.
    pub fn open_default() -> Result<Self, RenderError> {
        Self::try_open_default().map_err(|_| RenderError::AudioUnavailable)
    }

    /// Open the default output device, surfacing the exact platform error —
    /// the `oh-audio` composition logs this so a failed sink open is
    /// diagnosable (not silently no-op'd).
    pub fn try_open_default() -> Result<Self, AudioOpenError> {
        #[cfg(not(target_os = "emscripten"))]
        {
            let backend = rodio_backend::RodioBackend::open()?;
            Ok(Self {
                backend: Some(Box::new(backend)),
            })
        }
        #[cfg(target_os = "emscripten")]
        {
            openal::OpenalBackend::open().map(|backend| Self {
                backend: Some(Box::new(backend)),
            })
        }
    }

    /// Whether a live output backend is attached.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.backend.as_ref().is_some_and(|b| b.is_ready())
    }

    /// Play a one-shot `sf::Sound` (interleaved `f32`, plays once from 0).
    /// `volume` is SFML-style `0..=100`; `pitch` the rate. Returns `None` when
    /// no device is attached (headless) or a parameter is degenerate.
    pub fn play_one_shot(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
    ) -> Option<AudioPlayer> {
        self.backend
            .as_ref()
            .and_then(|b| b.play_one_shot(sample_rate, channels, samples, volume, pitch))
    }

    /// Play a looped `sf::Music` (interleaved `f32`, repeats forever) from
    /// `offset_secs`. `volume` is SFML-style `0..=100`; `pitch` the rate.
    /// Returns `None` when no device is attached (headless) or a parameter is
    /// degenerate.
    pub fn play_music(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
        offset_secs: f32,
    ) -> Option<AudioPlayer> {
        self.backend
            .as_ref()
            .and_then(|b| b.play_music(sample_rate, channels, samples, volume, pitch, offset_secs))
    }

    /// Resume / unlock the audio on the first user gesture (the web's autoplay
    /// policy; a no-op on desktop/Android).
    pub fn unlock(&self) {
        if let Some(b) = self.backend.as_ref() {
            b.unlock();
        }
    }

    /// A one-line diagnostic: which world (device present / headless) + the
    /// unlock (context) state + the last source state / error. The web's
    /// acceptance observable (the `tools/web-probe.mjs` "audio diagnostics"
    /// section surfaces this line).
    #[must_use]
    pub fn describe(&self) -> String {
        self.backend
            .as_ref()
            .map_or_else(|| "no device (headless)".to_string(), |b| b.describe())
    }
}

impl Default for AudioDevice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_1s() -> SoundBuffer {
        // 1 second of silence, mono, 8 kHz.
        SoundBuffer::from_samples(8000, 1, vec![0.0f32; 8000])
    }

    #[test]
    fn sound_buffer_duration() {
        let b = buffer_1s();
        assert_eq!(b.get_sample_rate(), 8000);
        assert_eq!(b.get_channel_count(), 1);
        assert!((b.get_duration() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sound_play_stop_state() {
        let mut s = Sound::new();
        s.set_buffer(buffer_1s());
        assert!(!s.is_playing());
        s.play();
        assert!(s.is_playing());
        assert!(!s.is_paused());
        s.pause();
        assert!(s.is_paused());
        assert!(!s.is_playing());
        s.resume();
        assert!(s.is_playing());
        s.stop();
        assert!(!s.is_playing());
        assert_eq!(s.get_playing_offset(), 0.0);
    }

    #[test]
    fn sound_volume_clamps_and_pitch() {
        let mut s = Sound::new();
        s.set_volume(250.0);
        assert_eq!(s.get_volume(), 100.0);
        s.set_volume(-5.0);
        assert_eq!(s.get_volume(), 0.0);
        s.set_pitch(2.0);
        assert_eq!(s.get_pitch(), 2.0);
        s.set_channel(3);
        assert_eq!(s.get_channel(), 3);
    }

    #[test]
    fn sound_playing_offset_clamped_to_duration() {
        let mut s = Sound::new();
        s.set_buffer(buffer_1s()); // 1.0 s, non-looping
        s.set_playing_offset(5.0);
        assert!((s.get_playing_offset() - 1.0).abs() < 1e-6);
        s.set_playing_offset(0.25);
        assert!((s.get_playing_offset() - 0.25).abs() < 1e-6);

        // looping wraps instead of clamping
        s.set_loop(true);
        s.set_playing_offset(1.25);
        assert!((s.get_playing_offset() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn music_state_and_offset() {
        // A real (fake) file so `open`'s existence check passes on any OS.
        let p =
            std::env::temp_dir().join(format!("oh-render-music-test-{}.bin", std::process::id()));
        std::fs::write(&p, b"fake").expect("temp file writable");
        let mut m = Music::open(&p).expect("temp file exists");
        m.set_duration(10.0);
        m.play();
        assert!(m.is_playing());
        m.set_volume(50.0);
        assert_eq!(m.get_volume(), 50.0);
        m.set_playing_offset(42.0);
        assert!((m.get_playing_offset() - 10.0).abs() < 1e-6); // clamped
        m.set_loop(true);
        m.set_playing_offset(13.0);
        assert!((m.get_playing_offset() - 3.0).abs() < 1e-6); // wrapped
        m.stop();
        assert!(!m.is_playing());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn headless_device_is_not_ready_and_play_noops() {
        let device = AudioDevice::new();
        assert!(!device.is_ready());
        let mut s = Sound::new();
        s.set_buffer(buffer_1s());
        s.play();
        // headless: play_into returns None, but the state is still "playing".
        assert!(s.play_into(&device).is_none());
        assert!(s.is_playing());
    }
}
