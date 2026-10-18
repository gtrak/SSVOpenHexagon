//! The `rodio` output backend (desktop + Android) — the issue-025e device
//! surface (`MixerDeviceSink` + `Player`), now sitting behind the portable
//! `AudioDevice` / `AudioPlayer` seam (`super`).
//!
//! On the web this module is not compiled: `rodio`/`cpal` have no
//! emscripten backend (cpal's browser host is gated on `target_os = "unknown"`,
//! so `wasm32-unknown-emscripten` falls to cpal's null host and the default
//! sink open fails with `NoDevice`) — the web uses the OpenAL backend
//! (`super::openal`) instead.

use rodio::Source;

use crate::audio::{AudioOpenError, AudioPlayer, DeviceBackend, PlayerBackend};

/// The rodio output device (the `MixerDeviceSink` the `Player`s connect to).
pub struct RodioBackend {
    sink: rodio::MixerDeviceSink,
}

impl RodioBackend {
    /// Open the default rodio sink, surfacing the exact `rodio`/`cpal` error
    /// (so a failed Android/AAudio sink open is diagnosable, not silently
    /// no-op'd).
    pub fn open() -> Result<Self, AudioOpenError> {
        let sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| AudioOpenError {
            detail: format!("{e:#}"),
        })?;
        Ok(Self { sink })
    }

    /// The shared play path (one-shot vs looped). `allow`: the 7 parameters are
    /// each distinct (rate / channels / samples / volume / pitch / looping /
    /// offset) — no two are bundleable without losing clarity.
    #[allow(clippy::too_many_arguments)]
    fn play_inner(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
        looping: bool,
        offset_secs: f32,
    ) -> Option<AudioPlayer> {
        let channels_nz = std::num::NonZero::new(channels)?;
        let sample_rate_nz = std::num::NonZero::new(sample_rate)?;
        let buffer = rodio::buffer::SamplesBuffer::new(channels_nz, sample_rate_nz, samples);
        // Loop the buffer for the music source (the C++ `_music->setLoop(true)`);
        // a one-shot plays once.
        let mut source: Box<dyn rodio::Source<Item = f32> + Send> = if looping {
            Box::new(buffer.repeat_infinite())
        } else {
            Box::new(buffer)
        };
        // Seek to the offset before playing (the offset is applied to the
        // source; a `Repeat`-wrapped source forwards `try_seek` to its inner
        // buffer, so the loop resumes from the offset).
        if offset_secs > 0.0 {
            source
                .try_seek(std::time::Duration::from_secs_f32(offset_secs))
                .ok();
        }
        let player = rodio::Player::connect_new(self.sink.mixer());
        player.append(source);
        // The SFML-unit → linear gain mapping (issue 039's device evidence:
        // 100 SFML units → rodio 1.0) — applied identically by the web's
        // OpenAL backend so both paths agree on the numbers.
        player.set_volume((volume / 100.0).clamp(0.0, 1.0));
        player.set_speed(pitch);
        Some(AudioPlayer {
            backend: Box::new(RodioPlayer { player }),
        })
    }
}

impl DeviceBackend for RodioBackend {
    fn is_ready(&self) -> bool {
        true
    }

    fn play_one_shot(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
    ) -> Option<AudioPlayer> {
        self.play_inner(sample_rate, channels, samples, volume, pitch, false, 0.0)
    }

    fn play_music(
        &self,
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        volume: f32,
        pitch: f32,
        offset_secs: f32,
    ) -> Option<AudioPlayer> {
        self.play_inner(
            sample_rate,
            channels,
            samples,
            volume,
            pitch,
            true,
            offset_secs,
        )
    }

    fn unlock(&self) {
        // No autoplay policy on desktop/Android — nothing to unlock.
    }

    fn describe(&self) -> String {
        "device present (rodio)".to_string()
    }
}

/// The rodio half of an [`AudioPlayer`] (a `rodio::Player`).
struct RodioPlayer {
    player: rodio::Player,
}

impl PlayerBackend for RodioPlayer {
    fn stop(&self) {
        self.player.stop();
    }

    fn pause(&self) {
        self.player.pause();
    }

    fn play(&self) {
        self.player.play();
    }

    fn set_volume(&self, volume: f32) {
        // SFML-style `0..=100` → rodio's native linear `0..=1` (issue 039's
        // device evidence: 100 SFML units → 1.0).
        self.player.set_volume((volume / 100.0).clamp(0.0, 1.0));
    }

    fn set_speed(&self, pitch: f32) {
        self.player.set_speed(pitch);
    }

    fn get_pos(&self) -> std::time::Duration {
        self.player.get_pos()
    }

    fn describe(&self) -> String {
        "rodio player".to_string()
    }
}
