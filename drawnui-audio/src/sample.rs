//! Samples: sound in memory (decoded once, or made in code) with optional loop points, and music
//! decoded while it plays.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(feature = "files")]
use std::sync::{Mutex, atomic::Ordering};

use crate::Layout;
#[cfg(feature = "files")]
use crate::AudioError;
use crate::ring::Ring;

/// How a sample loops (Impulse Tracker's sample loop): frames from `start` to `end` (end excluded).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loop {
    /// Plays to the end once.
    Off,
    /// From `end` back to `start`, over and over.
    Forward { start: usize, end: usize },
    /// Back and forth between `start` and `end`.
    PingPong { start: usize, end: usize },
}

pub(crate) struct Data {
    /// Interleaved frames (`layout` samples each).
    pub(crate) frames: Vec<f32>,
    pub(crate) layout: Layout,
    pub(crate) rate: u32,
    pub(crate) looping: Loop,
}

pub(crate) enum Kind {
    Memory(Data),
    /// Encoded bytes, decoded while a channel plays them.
    #[cfg_attr(not(feature = "files"), allow(dead_code))]
    Streamed { bytes: Arc<[u8]>, layout: Layout, rate: u32 },
}

/// Sound a channel plays: cheap to clone (the samples are shared).
#[derive(Clone)]
pub struct Sample(pub(crate) Arc<Kind>);

impl Sample {
    /// A sample from frames made in code: interleaved for stereo, at `rate` frames per second.
    pub fn from_frames(frames: Vec<f32>, layout: Layout, rate: u32) -> Sample {
        Sample(Arc::new(Kind::Memory(Data { frames, layout, rate: rate.max(1), looping: Loop::Off })))
    }

    /// A WAV, MP3, OGG Vorbis or FLAC file decoded into memory now: for sound effects and
    /// instruments, which then start with no delay. More than two channels keep the first two.
    #[cfg(feature = "files")]
    pub fn decode(bytes: &[u8]) -> Result<Sample, AudioError> {
        let mut decoder = files::Decoder::new(Arc::from(bytes))?;
        let (layout, rate) = (decoder.layout, decoder.rate);
        let mut frames = Vec::new();
        while decoder.next(&mut frames)? {}
        Ok(Sample(Arc::new(Kind::Memory(Data { frames, layout, rate, looping: Loop::Off }))))
    }

    /// A file decoded while it plays, a little ahead (for music: a long file is never held
    /// decoded in memory). Only the header is read now.
    #[cfg(feature = "files")]
    pub fn streamed(bytes: impl Into<Arc<[u8]>>) -> Result<Sample, AudioError> {
        let bytes = bytes.into();
        let decoder = files::Decoder::new(bytes.clone())?;
        Ok(Sample(Arc::new(Kind::Streamed { bytes, layout: decoder.layout, rate: decoder.rate })))
    }

    /// The same sample with a loop (in frames, clamped to the sample). Memory samples only; a
    /// streamed one loops the whole file with `Note::looped`.
    pub fn with_loop(self, looping: Loop) -> Sample {
        let Kind::Memory(data) = &*self.0 else { return self };
        let len = data.frames.len() / data.layout.count();
        let clamp = |start: usize, end: usize| (start.min(len.saturating_sub(1)), end.clamp(start.min(len.saturating_sub(1)) + 1, len.max(1)));
        let looping = match looping {
            Loop::Off => Loop::Off,
            Loop::Forward { start, end } => {
                let (start, end) = clamp(start, end);
                Loop::Forward { start, end }
            }
            Loop::PingPong { start, end } => {
                let (start, end) = clamp(start, end);
                Loop::PingPong { start, end }
            }
        };
        Sample(Arc::new(Kind::Memory(Data { frames: data.frames.clone(), layout: data.layout, rate: data.rate, looping })))
    }

    pub fn layout(&self) -> Layout {
        match &*self.0 {
            Kind::Memory(d) => d.layout,
            Kind::Streamed { layout, .. } => *layout,
        }
    }

    /// The sample's own rate: played at speed 1 it sounds at its recorded pitch on any output.
    pub fn rate(&self) -> u32 {
        match &*self.0 {
            Kind::Memory(d) => d.rate,
            Kind::Streamed { rate, .. } => *rate,
        }
    }

    /// Frames in memory (0 for a streamed sample, whose length is known as it plays).
    pub fn frames(&self) -> usize {
        match &*self.0 {
            Kind::Memory(d) => d.frames.len() / d.layout.count(),
            Kind::Streamed { .. } => 0,
        }
    }

    pub fn looping(&self) -> Loop {
        match &*self.0 {
            Kind::Memory(d) => d.looping,
            Kind::Streamed { .. } => Loop::Off,
        }
    }
}

/// A streamed sample being played: its decoded frames (at the file's rate) in a ring the mixer
/// reads, filled ahead by `Feed`.
pub(crate) struct Stream {
    pub(crate) ring: Ring,
    pub(crate) layout: Layout,
    /// The decoder reached the end (and does not loop): once the ring is empty, the note ends.
    pub(crate) ended: AtomicBool,
    /// The note is gone: the feed stops.
    pub(crate) closed: AtomicBool,
}

/// How far ahead music is decoded.
#[cfg(feature = "files")]
const AHEAD_MS: usize = 500;

/// Fills a playing stream's ring off the audio thread (the output's own thread on desktop and
/// phones, the page's timer in the browser).
pub(crate) struct Feed {
    pub(crate) stream: Arc<Stream>,
    #[cfg(feature = "files")]
    state: Mutex<FeedState>,
    #[cfg(feature = "files")]
    looped: bool,
    #[cfg(feature = "files")]
    bytes: Arc<[u8]>,
}

#[cfg(feature = "files")]
struct FeedState {
    decoder: Option<files::Decoder>,
    /// Decoded frames that did not fit the ring yet; they go first next time.
    pending: Vec<f32>,
    sent: usize,
    /// Packets decoded since the file (re)started: a looped file that gives none ends instead of
    /// starting over forever.
    packets: usize,
}

impl Feed {
    /// Starts decoding a streamed sample, the first part now so the note starts at once.
    #[cfg(feature = "files")]
    pub(crate) fn start(bytes: Arc<[u8]>, layout: Layout, rate: u32, looped: bool) -> Result<Arc<Feed>, AudioError> {
        let decoder = files::Decoder::new(bytes.clone())?;
        let room = (rate as usize * AHEAD_MS / 1000).max(1024) * layout.count();
        let stream = Arc::new(Stream { ring: Ring::new(room), layout, ended: AtomicBool::new(false), closed: AtomicBool::new(false) });
        let state = Mutex::new(FeedState { decoder: Some(decoder), pending: Vec::new(), sent: 0, packets: 0 });
        let feed = Arc::new(Feed { stream, state, looped, bytes });
        feed.top_up();
        Ok(feed)
    }

    /// Decodes until the ring is full or the file ends (then starts again when it loops).
    pub(crate) fn top_up(&self) {
        #[cfg(feature = "files")]
        {
            let stream = &self.stream;
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let state = &mut *state;
            loop {
                if state.sent < state.pending.len() {
                    state.sent += stream.ring.push(&state.pending[state.sent..]);
                    if state.sent < state.pending.len() {
                        return; // the ring is full
                    }
                }
                state.pending.clear();
                state.sent = 0;
                let Some(decoder) = state.decoder.as_mut() else {
                    stream.ended.store(true, Ordering::Release);
                    return;
                };
                match decoder.next(&mut state.pending) {
                    Ok(true) => state.packets += 1,
                    // The end (or a file that cannot be read further): again from the start, or done.
                    Ok(false) | Err(_) => {
                        let again = self.looped && state.packets > 0;
                        state.decoder = if again { files::Decoder::new(self.bytes.clone()).ok() } else { None };
                        state.packets = 0;
                    }
                }
            }
        }
    }
}

#[cfg(feature = "files")]
mod files {
    use std::io::Cursor;
    use std::sync::Arc;

    use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    use crate::{AudioError, Layout};

    /// A file's audio, packet by packet, as interleaved f32 frames (two channels at most).
    pub(crate) struct Decoder {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn AudioDecoder>,
        track: u32,
        pub(crate) layout: Layout,
        pub(crate) rate: u32,
        scratch: Vec<f32>,
    }

    fn error(e: impl std::fmt::Display) -> AudioError {
        AudioError(format!("decoding: {e}"))
    }

    impl Decoder {
        pub(crate) fn new(bytes: Arc<[u8]>) -> Result<Decoder, AudioError> {
            let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
            let format = symphonia::default::get_probe().probe(&Hint::new(), source, FormatOptions::default(), MetadataOptions::default()).map_err(error)?;
            let track = format.default_track(TrackType::Audio).ok_or_else(|| error("no audio track"))?;
            let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| error("no audio track"))?;
            let rate = params.sample_rate.ok_or_else(|| error("no sample rate"))?;
            let channels = params.channels.as_ref().map_or(2, |c| c.count());
            let decoder = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).map_err(error)?;
            let track = track.id;
            let layout = if channels == 1 { Layout::Mono } else { Layout::Stereo };
            Ok(Decoder { format, decoder, track, layout, rate, scratch: Vec::new() })
        }

        /// Appends the next packet's frames to `out`; false at the end of the file.
        pub(crate) fn next(&mut self, out: &mut Vec<f32>) -> Result<bool, AudioError> {
            loop {
                let Some(packet) = self.format.next_packet().map_err(error)? else { return Ok(false) };
                if packet.track_id != self.track {
                    continue;
                }
                let buffer = match self.decoder.decode(&packet) {
                    Ok(buffer) => buffer,
                    // A damaged packet is skipped, as players do.
                    Err(Error::DecodeError(_)) => continue,
                    Err(e) => return Err(error(e)),
                };
                let channels = buffer.spec().channels().count().max(1);
                self.scratch.resize(buffer.samples_interleaved(), 0.0);
                buffer.copy_to_slice_interleaved(&mut self.scratch);
                match (channels, self.layout) {
                    (1, _) => out.extend_from_slice(&self.scratch),
                    (2, Layout::Stereo) => out.extend_from_slice(&self.scratch),
                    // More than two: the first two (front left and right).
                    (n, _) => {
                        for frame in self.scratch.chunks_exact(n) {
                            out.extend_from_slice(&frame[..2]);
                        }
                    }
                }
                return Ok(true);
            }
        }
    }
}
