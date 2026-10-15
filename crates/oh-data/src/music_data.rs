//! Port of `include/SSVOpenHexagon/Data/MusicData.hpp` +
//! `src/SSVOpenHexagon/Data/MusicData.cpp`.
//!
//! The `playRandomSegment` / `playSegment` / `playSeconds` methods need the
//! `Audio` engine (asset loading — issue 05 / audio issue); the data-layer
//! state they operate on (`segments`, `first_play`) is ported here and kept
//! accessible so those methods can be added verbatim when `Audio` lands.

/// C++ `hg::MusicData::Segment` — `float time, beatPulseDelayOffset`
/// (NO defaults).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub time: f32,
    pub beat_pulse_delay_offset: f32,
}

impl Segment {
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub const fn new(time: f32, beat_pulse_delay_offset: f32) -> Self {
        Self {
            time,
            beat_pulse_delay_offset,
        }
    }
}

/// C++ `hg::MusicData`.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicData {
    segments: Vec<Segment>,
    pub id: String,
    pub file_name: String,
    pub name: String,
    pub album: String,
    pub author: String,
    pub first_play: bool,
}

impl Default for MusicData {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            id: String::new(),
            file_name: String::new(),
            name: String::new(),
            album: String::new(),
            author: String::new(),
            first_play: true,
        }
    }
}

impl MusicData {
    /// C++ 5-arg constructor (defaults: `segments = {}`, `first_play = true`).
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn new(m_id: &str, m_file_name: &str, m_name: &str, m_album: &str, m_author: &str) -> Self {
        Self {
            segments: Vec::new(),
            id: m_id.to_string(),
            file_name: m_file_name.to_string(),
            name: m_name.to_string(),
            album: m_album.to_string(),
            author: m_author.to_string(),
            first_play: true,
        }
    }

    /// C++ `getSegment(index)` — `segments[index]`.
    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn get_segment(&self, index: usize) -> Option<&Segment> {
        self.segments.get(index)
    }

    /// C++ `addSegment(seconds, beatPulseDelayOffset)`.
    pub fn add_segment(&mut self, m_seconds: f32, m_beat_pulse_delay_offset: f32) {
        self.segments
            .push(Segment::new(m_seconds, m_beat_pulse_delay_offset));
    }

    #[must_use = "mirrors the C++ [[nodiscard]]"]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_append_in_order() {
        let mut md = MusicData::new("a", "a.ogg", "A", "", "");
        md.add_segment(1.0, 0.0);
        md.add_segment(2.5, 0.25);
        assert_eq!(md.get_segment(1), Some(&Segment::new(2.5, 0.25)));
        assert_eq!(md.segments().len(), 2);
        assert!(md.first_play);
    }
}
