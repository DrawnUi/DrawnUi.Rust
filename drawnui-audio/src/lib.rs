//! Sound for DrawnUI for Rust apps: an output on Windows, macOS, Linux, iOS, Android and the
//! browser, in Impulse Tracker's model: numbered channels that play samples (with volume,
//! panning, loops and pitch), plus streams the app pushes and generators that make their sound on
//! the audio side, all mixed with their own volumes.
//!
//! ```no_run
//! use drawnui_audio::{Audio, Layout, Note, Sample};
//!
//! let audio = Audio::open().expect("an audio output");
//! // A sample (`Sample::decode` reads WAV, MP3, OGG and FLAC files), played on a channel or on
//! // the next free one.
//! let brick = Sample::from_frames(vec![0.0; 4800], Layout::Mono, 48_000);
//! audio.channel(3).set_pan(-0.5);
//! audio.channel(3).play(&brick, Note::default());
//! audio.play(&brick, Note { volume: 0.6, ..Note::default() });
//! // A stream the app fills (an emulator's sound chip, a synth): mono, 100 ms of room.
//! let chip = audio.stream(Layout::Mono, 100);
//! chip.push_slice(&[0.0; 960]);
//! ```
//!
//! The browser starts sound only after the user's first click, tap or key on the page (its
//! autoplay rule): until then the output is silent and `is_running` reads false; streams keep
//! their samples meanwhile, up to their room.

mod mixer;
mod ring;
mod sample;

#[cfg(not(target_os = "emscripten"))]
mod native;
#[cfg(target_os = "emscripten")]
mod web;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use mixer::{ChannelShared, Fill, Message, Mixer, Shared, Source, F32};
use ring::Ring;
pub use sample::{Loop, Sample};
use sample::{Feed, Kind};

/// Channels an output has at most (Impulse Tracker's 64).
pub const MAX_CHANNELS: usize = 64;

/// Why an output could not be opened or a file read.
#[derive(Debug, Clone)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audio: {}", self.0)
    }
}

impl std::error::Error for AudioError {}

/// Samples per frame of a sample, stream or generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Mono,
    /// Left and right, interleaved.
    Stereo,
}

impl Layout {
    pub fn count(self) -> usize {
        match self {
            Layout::Mono => 1,
            Layout::Stereo => 2,
        }
    }
}

/// How a channel plays a sample (one row of a tracker pattern).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// The note's volume (1 = as recorded); `Channel::set_note_volume` changes it while it plays.
    pub volume: f32,
    /// Pitch as a speed factor: 1 = the sample's own pitch, 2 = an octave up.
    pub speed: f32,
    /// Plays the whole sample over and over when it has no loop of its own (music, an engine).
    pub looped: bool,
    /// The frame the note starts at (Impulse Tracker's sample offset).
    pub start: usize,
    /// The note playing on the channel keeps ringing in the background instead of being cut
    /// (Impulse Tracker's "new note action: continue").
    pub keep_previous: bool,
    /// Sets the channel's panning with the note (-1 left to 1 right), as a tracker row's panning
    /// column does: a sound placed left or right on whichever channel `Audio::play` picks.
    pub pan: Option<f32>,
}

impl Default for Note {
    fn default() -> Note {
        Note { volume: 1.0, speed: 1.0, looped: false, start: 0, keep_previous: false, pan: None }
    }
}

/// What the app's handles and the output share.
pub(crate) struct Output {
    pub(crate) sample_rate: AtomicU32,
    pub(crate) speakers: AtomicU32,
    pub(crate) running: AtomicBool,
    pub(crate) paused: AtomicBool,
    pub(crate) master: Arc<F32>,
    pub(crate) channels: Arc<[ChannelShared]>,
    /// Channels `Audio::play` chooses from and `Audio::channel` reaches.
    active: AtomicUsize,
    notes: AtomicU64,
    messages: Mutex<Sender<Message>>,
    /// Music being decoded ahead of its channel.
    feeds: Mutex<Vec<Arc<Feed>>>,
}

impl Output {
    fn send(&self, message: Message) {
        let _ = self.messages.lock().unwrap_or_else(|e| e.into_inner()).send(message);
    }

    /// Decodes the music of every playing streamed note a little further (never on the audio thread).
    pub(crate) fn feed(&self) {
        let mut feeds = self.feeds.lock().unwrap_or_else(|e| e.into_inner());
        feeds.retain(|f| !f.stream.closed.load(Ordering::Acquire));
        for feed in feeds.iter() {
            feed.top_up();
        }
    }
}

enum Backend {
    #[cfg(not(target_os = "emscripten"))]
    Device(native::Device),
    #[cfg(target_os = "emscripten")]
    Web,
    Null { mixer: Mutex<Mixer>, recorded: Mutex<Vec<f32>> },
}

/// An audio output: its sample rate and speakers, the channels, streams and generators mixed into
/// it, pause and resume. Dropping it stops the sound.
pub struct Audio {
    output: Arc<Output>,
    backend: Backend,
}

impl Audio {
    /// The system's default output device. Desktop and phones: it starts playing at once. The
    /// browser: 48000 Hz stereo, playing from the user's first click, tap or key on the page.
    pub fn open() -> Result<Audio, AudioError> {
        let (output, mixer) = Audio::parts(0, 0);
        #[cfg(not(target_os = "emscripten"))]
        let backend = Backend::Device(native::Device::open(output.clone(), mixer)?);
        #[cfg(target_os = "emscripten")]
        let backend = {
            web::open(output.clone(), mixer)?;
            Backend::Web
        };
        Ok(Audio { output, backend })
    }

    /// An output with no device, for tests: nothing plays; `render` mixes everything on the spot
    /// and keeps it, `recorded` returns it.
    pub fn null(sample_rate: u32, speakers: u16) -> Audio {
        let (output, mixer) = Audio::parts(sample_rate, speakers);
        output.running.store(true, Ordering::Release);
        Audio { output, backend: Backend::Null { mixer: Mutex::new(mixer), recorded: Mutex::new(Vec::new()) } }
    }

    fn parts(sample_rate: u32, speakers: u16) -> (Arc<Output>, Mixer) {
        let (sender, receiver) = channel();
        let master = Arc::new(F32::new(1.0));
        let channels: Arc<[ChannelShared]> = (0..MAX_CHANNELS).map(|_| ChannelShared::new()).collect();
        let output = Arc::new(Output {
            sample_rate: AtomicU32::new(sample_rate),
            speakers: AtomicU32::new(speakers as u32),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            master: master.clone(),
            channels: channels.clone(),
            active: AtomicUsize::new(16),
            notes: AtomicU64::new(0),
            messages: Mutex::new(sender),
            feeds: Mutex::new(Vec::new()),
        });
        (output, Mixer::new(receiver, master, channels))
    }

    /// Frames per second the output plays. Streams and generators produce at this rate; samples
    /// play at their own. It can change when the output device changes (headphones plugged in):
    /// read it again rather than keeping it.
    pub fn sample_rate(&self) -> u32 {
        self.output.sample_rate.load(Ordering::Acquire)
    }

    /// The output's speakers (2 for stereo). Everything plays on any output: mono goes to every
    /// speaker, stereo to the first two.
    pub fn speakers(&self) -> u16 {
        self.output.speakers.load(Ordering::Acquire) as u16
    }

    /// The output is playing: opened, not paused, and in the browser unlocked by the user's first
    /// gesture.
    pub fn is_running(&self) -> bool {
        self.output.running.load(Ordering::Acquire) && !self.output.paused.load(Ordering::Acquire)
    }

    /// How many channels there are (1 to 64; 16 at first, like a new Impulse Tracker song's).
    pub fn set_channels(&self, count: usize) {
        self.output.active.store(count.clamp(1, MAX_CHANNELS), Ordering::Release);
    }

    pub fn channels(&self) -> usize {
        self.output.active.load(Ordering::Acquire)
    }

    /// Channel `index` (from 0; wraps around the channel count): its volume, panning and the
    /// notes it plays.
    pub fn channel(&self, index: usize) -> Channel {
        Channel { index: index % self.channels(), output: self.output.clone() }
    }

    /// Plays `sample` on the next free channel that is not reserved; when all are busy, on the one
    /// whose note started first (that note is cut). Returns the channel, for a slide or a stop.
    pub fn play(&self, sample: &Sample, note: Note) -> Channel {
        let channels = &self.output.channels[..self.channels()];
        let open = |c: &&ChannelShared| !c.reserved.load(Ordering::Acquire);
        let busy = |c: &ChannelShared| c.playing.load(Ordering::Acquire) || c.pending.load(Ordering::Acquire);
        let index = channels
            .iter()
            .enumerate()
            .filter(|(_, c)| open(c))
            .find(|(_, c)| !busy(c))
            .or_else(|| channels.iter().enumerate().filter(|(_, c)| open(c)).min_by_key(|(_, c)| c.started.load(Ordering::Acquire)))
            .map_or(0, |(i, _)| i);
        let channel = Channel { index, output: self.output.clone() };
        channel.play(sample, note);
        channel
    }

    /// A stream the app pushes samples into, with room for `room_ms` milliseconds at the current
    /// sample rate. Samples that do not fit are dropped; when it runs dry it plays silence.
    pub fn stream(&self, layout: Layout, room_ms: u32) -> AudioStream {
        let rate = self.sample_rate().max(1) as usize;
        let frames = (rate * room_ms.max(1) as usize).div_ceil(1000);
        let shared = self.add(layout, Some(Ring::new(frames * layout.count())), None);
        AudioStream { shared, output: self.output.clone() }
    }

    /// A source that makes its own sound: `fill` gets each buffer the output needs (interleaved by
    /// `layout`, zeroed) and writes into it. It runs on the audio thread (the page's own thread in
    /// the browser): keep it short, no locks, no allocation.
    pub fn generator(&self, layout: Layout, fill: impl FnMut(&mut [f32]) + Send + 'static) -> AudioGenerator {
        let shared = self.add(layout, None, Some(Box::new(fill)));
        AudioGenerator { shared }
    }

    fn add(&self, layout: Layout, ring: Option<Ring>, fill: Option<Fill>) -> Arc<Shared> {
        let shared = Arc::new(Shared { volume: F32::new(1.0), closed: AtomicBool::new(false), layout, ring, clear: AtomicBool::new(false) });
        self.output.send(Message::Source(Source { shared: shared.clone(), fill }));
        shared
    }

    /// The volume of everything (1 = as mixed).
    pub fn set_volume(&self, volume: f32) {
        self.output.master.set(volume.max(0.0));
    }

    pub fn volume(&self) -> f32 {
        self.output.master.get()
    }

    /// Stops the output (the app went to the background); everything keeps its place.
    pub fn pause(&self) {
        self.set_paused(true);
    }

    /// Plays again after `pause`.
    pub fn resume(&self) {
        self.set_paused(false);
    }

    /// Pauses while the app is hidden, plays when it is shown: pass the value of
    /// `Ui::on_visibility_changed`.
    pub fn set_paused(&self, paused: bool) {
        if self.output.paused.swap(paused, Ordering::AcqRel) == paused {
            return;
        }
        match &self.backend {
            #[cfg(not(target_os = "emscripten"))]
            Backend::Device(device) => device.set_paused(paused),
            #[cfg(target_os = "emscripten")]
            Backend::Web => web::set_paused(paused),
            Backend::Null { .. } => {}
        }
    }

    /// Null output: mixes `frames` frames of everything now, keeps them for `recorded` and returns
    /// them (interleaved). Other outputs: an empty Vec (their device asks for samples).
    pub fn render(&self, frames: usize) -> Vec<f32> {
        let Backend::Null { mixer, recorded } = &self.backend else { return Vec::new() };
        let mut out = vec![0.0; frames * self.speakers().max(1) as usize];
        if !self.output.paused.load(Ordering::Acquire) {
            self.output.feed();
            mixer.lock().unwrap_or_else(|e| e.into_inner()).render(&mut out, self.speakers() as usize, self.sample_rate());
        }
        recorded.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&out);
        out
    }

    /// Null output: everything `render` mixed so far, interleaved.
    pub fn recorded(&self) -> Vec<f32> {
        match &self.backend {
            Backend::Null { recorded, .. } => recorded.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            #[allow(unreachable_patterns)]
            _ => Vec::new(),
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        #[cfg(target_os = "emscripten")]
        if let Backend::Web = self.backend {
            web::close();
        }
    }
}

/// One of the output's numbered channels (a tracker channel): it plays one note at a time, with
/// its own volume, panning, mute and solo. Cheap to clone; any number of handles reach it.
#[derive(Clone)]
pub struct Channel {
    index: usize,
    output: Arc<Output>,
}

impl Channel {
    pub fn index(&self) -> usize {
        self.index
    }

    fn shared(&self) -> &ChannelShared {
        &self.output.channels[self.index]
    }

    /// Plays `sample`: the note playing here is cut (or keeps ringing, `Note::keep_previous`).
    /// A streamed sample starts decoding now. A file that cannot be decoded plays nothing.
    pub fn play(&self, sample: &Sample, note: Note) {
        let shared = self.shared();
        let stream = match &*sample.0 {
            Kind::Memory(_) => None,
            #[cfg(feature = "files")]
            Kind::Streamed { bytes, layout, rate } => match Feed::start(bytes.clone(), *layout, *rate, note.looped) {
                Ok(feed) => {
                    let stream = feed.stream.clone();
                    self.output.feeds.lock().unwrap_or_else(|e| e.into_inner()).push(feed);
                    Some(stream)
                }
                Err(_) => return,
            },
            #[cfg(not(feature = "files"))]
            Kind::Streamed { .. } => return,
        };
        shared.speed.set(note.speed);
        shared.note_volume.set(note.volume);
        if let Some(pan) = note.pan {
            shared.pan.set(pan.clamp(-1.0, 1.0));
        }
        shared.pending.store(true, Ordering::Release);
        shared.started.store(self.output.notes.fetch_add(1, Ordering::AcqRel) + 1, Ordering::Release);
        self.output.send(Message::Play { channel: self.index, sample: sample.clone(), note, stream });
    }

    /// Cuts the note (Impulse Tracker's note cut).
    pub fn stop(&self) {
        self.output.send(Message::Stop(self.index));
    }

    /// A note plays here (or was just sent).
    pub fn is_playing(&self) -> bool {
        let s = self.shared();
        s.playing.load(Ordering::Acquire) || s.pending.load(Ordering::Acquire)
    }

    /// The channel's volume (1 = full), on top of each note's.
    pub fn set_volume(&self, volume: f32) {
        self.shared().volume.set(volume.max(0.0));
    }

    pub fn volume(&self) -> f32 {
        self.shared().volume.get()
    }

    /// Panning from -1 (left) through 0 (center) to 1 (right).
    pub fn set_pan(&self, pan: f32) {
        self.shared().pan.set(pan.clamp(-1.0, 1.0));
    }

    pub fn pan(&self) -> f32 {
        self.shared().pan.get()
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared().muted.store(muted, Ordering::Release);
    }

    pub fn is_muted(&self) -> bool {
        self.shared().muted.load(Ordering::Acquire)
    }

    /// While any channel is solo, only solo channels are heard.
    pub fn set_solo(&self, solo: bool) {
        self.shared().solo.store(solo, Ordering::Release);
    }

    pub fn is_solo(&self) -> bool {
        self.shared().solo.load(Ordering::Acquire)
    }

    /// Kept for the app's own notes: `Audio::play` never picks it (a music channel).
    pub fn set_reserved(&self, reserved: bool) {
        self.shared().reserved.store(reserved, Ordering::Release);
    }

    pub fn is_reserved(&self) -> bool {
        self.shared().reserved.load(Ordering::Acquire)
    }

    /// The playing note's pitch as a speed factor (slides, vibrato); a new note starts at its own.
    pub fn set_speed(&self, speed: f32) {
        self.shared().speed.set(speed.max(0.0));
    }

    pub fn speed(&self) -> f32 {
        self.shared().speed.get()
    }

    /// The playing note's volume (volume slides, fades); a new note starts at its own.
    pub fn set_note_volume(&self, volume: f32) {
        self.shared().note_volume.set(volume.max(0.0));
    }

    pub fn note_volume(&self) -> f32 {
        self.shared().note_volume.get()
    }
}

/// Samples the app pushes, mixed into the output. One writer: push from one thread at a time.
/// Dropping it removes it from the mix.
pub struct AudioStream {
    shared: Arc<Shared>,
    output: Arc<Output>,
}

impl AudioStream {
    /// One sample (one frame of a mono stream). False when the stream is full: it was dropped.
    /// No allocation and no lock: cheap enough to call per sample.
    #[inline]
    pub fn push(&self, sample: f32) -> bool {
        self.ring().push(std::slice::from_ref(&sample)) == 1
    }

    /// Several samples (interleaved for stereo); returns how many went in, the rest was dropped.
    pub fn push_slice(&self, samples: &[f32]) -> usize {
        self.ring().push(samples)
    }

    /// Frames waiting to be played. An app that makes sound in steps (an emulator) keeps this
    /// around its target: a step fewer when it is high, one more when it is low.
    pub fn queued(&self) -> usize {
        self.ring().len() / self.shared.layout.count()
    }

    /// `queued` in milliseconds at the output's current sample rate.
    pub fn queued_ms(&self) -> f32 {
        let rate = self.output.sample_rate.load(Ordering::Acquire).max(1);
        self.queued() as f32 * 1000.0 / rate as f32
    }

    /// The most frames it holds.
    pub fn room(&self) -> usize {
        self.ring().capacity() / self.shared.layout.count()
    }

    pub fn layout(&self) -> Layout {
        self.shared.layout
    }

    /// Drops what is queued (the mixer forgets it at its next buffer).
    pub fn clear(&self) {
        self.shared.clear.store(true, Ordering::Release);
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.set(volume.max(0.0));
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume.get()
    }

    fn ring(&self) -> &Ring {
        self.shared.ring.as_ref().expect("a stream has a ring")
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

/// A source made by `Audio::generator`: its volume; dropping it removes it from the mix.
pub struct AudioGenerator {
    shared: Arc<Shared>,
}

impl AudioGenerator {
    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.set(volume.max(0.0));
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume.get()
    }
}

impl Drop for AudioGenerator {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests;
