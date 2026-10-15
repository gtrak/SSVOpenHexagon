//! Shared errors for the `oh-render` backend.

use std::fmt;

/// A backend error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// The file could not be read (`io` detail).
    Io(std::io::ErrorKind),
    /// A texture / font / shader source was empty or malformed.
    InvalidData,
    /// No OpenGL context is available (headless / no display).
    NoContext,
    /// No audio output device is available (headless / no audio).
    AudioUnavailable,
}

impl RenderError {
    /// A stable, greppable message (used in the GPU smoke test's skip reason).
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Io(_) => "io error",
            Self::InvalidData => "invalid data",
            Self::NoContext => "no GL context available",
            Self::AudioUnavailable => "no audio device available",
        }
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "io error: {kind}"),
            Self::InvalidData => write!(f, "invalid data"),
            Self::NoContext => write!(f, "no GL context available"),
            Self::AudioUnavailable => write!(f, "no audio device available"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.kind())
    }
}
