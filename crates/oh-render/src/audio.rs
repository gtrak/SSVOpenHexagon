//! The audio **device** surface: `sf::SoundBuffer` / `sf::Sound` / `sf::Music`
//! + an `AudioDevice`.
//!
//! This is the low-level device surface the `sf::Sound`/`sf::Music` state
//! maps onto (play / stop / pitch / volume / playing-offset). The **game-level
//! volume curves** (per-track mixing, beat-pulse, difficulty curves) are
//! `oh-audio`'s job, not this crate's — `oh-audio` wraps this surface.
//!
//! The actual output device is `rodio` 0.23 (`MixerDeviceSink` + `Player`).
//! It is created lazily (`AudioDevice::open_default`); headless builds (no
//! audio device) simply have no sink and the `play` paths no-op while the
//! object state (volume / pitch / offset / playing) is fully testable.
//!
//! SFML semantics mirrored: volume is `0..=100` (default `100`), pitch is
//! `1.0` by default, `setPlayingOffset` is in seconds and is clamped to the
//! buffer length unless the source loops.

use std::path::PathBuf;

use oh_math::ssvu::math::get_clamped;

use crate::error::RenderError;

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

    /// Play into an audio device: build a `rodio::Player` carrying this
    /// buffer at the current volume / pitch. Returns `None` headless (no
    /// device) or when no buffer is set. The caller keeps the returned
    /// `Player` alive for the sound's duration.
    pub fn play_into(&self, device: &AudioDevice) -> Option<rodio::Player> {
        let buffer = self.buffer.as_ref()?;
        device.play_samples(
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

/// The audio output device (backed by `rodio`).
#[derive(Debug)]
pub struct AudioDevice {
    sink: Option<rodio::MixerDeviceSink>,
}

impl AudioDevice {
    /// A headless device with no output sink (used when no audio is desired,
    /// e.g. in tests and the headless game path).
    #[must_use]
    pub fn new() -> Self {
        Self { sink: None }
    }

    /// Open the default output device.
    ///
    /// # Errors
    /// [`RenderError::AudioUnavailable`] when no output device exists.
    pub fn open_default() -> Result<Self, RenderError> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|_| RenderError::AudioUnavailable)?;
        Ok(Self { sink: Some(sink) })
    }

    /// Whether a live output sink is attached.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.sink.is_some()
    }

    /// Play an interleaved `f32` buffer: connect a `rodio::Player` and append
    /// the samples. `volume` is SFML-style `0..=100`; `pitch` the rodio speed.
    /// Returns `None` when no device is attached (headless).
    pub fn play_samples(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
    ) -> Option<rodio::Player> {
        let sink = self.sink.as_ref()?;
        let channels = std::num::NonZero::new(channels)?;
        let sample_rate = std::num::NonZero::new(sample_rate)?;
        let player = rodio::Player::connect_new(sink.mixer());
        let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate, samples);
        player.append(source);
        player.set_volume((volume / 100.0).clamp(0.0, 1.0));
        player.set_speed(pitch);
        Some(player)
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
