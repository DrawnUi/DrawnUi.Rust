//! The mixer: adds every stream, generator and channel into the output buffer, each with its
//! volume, then the master volume. It runs where the output asks for samples (the audio thread on
//! desktop and phones, the page's timer in the browser, `Audio::render` for the null output) and
//! never waits on the app.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::ring::Ring;
use crate::sample::{Data, Kind, Loop, Sample, Stream};
use crate::{Layout, Note};

/// An f32 shared between the app and the mixer.
pub(crate) struct F32(AtomicU32);

impl F32 {
    pub(crate) fn new(v: f32) -> F32 {
        F32(AtomicU32::new(v.to_bits()))
    }
    pub(crate) fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub(crate) fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// What a stream's or generator's handle and the mixer share.
pub(crate) struct Shared {
    pub(crate) volume: F32,
    /// The handle is gone: the mixer drops the source.
    pub(crate) closed: AtomicBool,
    pub(crate) layout: Layout,
    /// A stream's samples (None for a generator).
    pub(crate) ring: Option<Ring>,
    /// `AudioStream::clear`: the mixer forgets what is queued.
    pub(crate) clear: AtomicBool,
}

/// A generator: fills its buffer (interleaved, by its layout) on the mixer's thread.
pub(crate) type Fill = Box<dyn FnMut(&mut [f32]) + Send>;

pub(crate) struct Source {
    pub(crate) shared: Arc<Shared>,
    pub(crate) fill: Option<Fill>,
}

/// What a channel's handle and the mixer share (Impulse Tracker's channel settings).
pub(crate) struct ChannelShared {
    pub(crate) volume: F32,
    pub(crate) pan: F32,
    pub(crate) muted: AtomicBool,
    pub(crate) solo: AtomicBool,
    /// Not taken by `Audio::play`.
    pub(crate) reserved: AtomicBool,
    /// The playing note's live pitch factor and volume (slides, vibrato, fades).
    pub(crate) speed: F32,
    pub(crate) note_volume: F32,
    /// A note plays (the mixer's view) or was just sent (the app's).
    pub(crate) playing: AtomicBool,
    pub(crate) pending: AtomicBool,
    /// When its last note was sent (a counter): `Audio::play` takes the oldest when all are busy.
    pub(crate) started: AtomicU64,
}

impl ChannelShared {
    pub(crate) fn new() -> ChannelShared {
        ChannelShared {
            volume: F32::new(1.0),
            pan: F32::new(0.0),
            muted: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            reserved: AtomicBool::new(false),
            speed: F32::new(1.0),
            note_volume: F32::new(1.0),
            playing: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            started: AtomicU64::new(0),
        }
    }
}

pub(crate) enum Message {
    Source(Source),
    Play { channel: usize, sample: Sample, note: Note, stream: Option<Arc<Stream>> },
    Stop(usize),
}

/// Notes kept ringing in the background (Note::keep_previous) at most.
const BACKGROUND: usize = 64;

pub(crate) struct Mixer {
    incoming: Receiver<Message>,
    sources: Vec<Source>,
    scratch: Vec<f32>,
    master: Arc<F32>,
    channels: Arc<[ChannelShared]>,
    playing: Vec<Option<Playing>>,
    background: Vec<(usize, Playing)>,
}

impl Mixer {
    pub(crate) fn new(incoming: Receiver<Message>, master: Arc<F32>, channels: Arc<[ChannelShared]>) -> Mixer {
        let playing = (0..channels.len()).map(|_| None).collect();
        Mixer { incoming, sources: Vec::new(), scratch: vec![0.0; 8192], master, channels, playing, background: Vec::with_capacity(BACKGROUND) }
    }

    /// Mixes into `out` (interleaved, `speakers` per frame, `rate` frames per second): silence
    /// where nothing plays.
    pub(crate) fn render(&mut self, out: &mut [f32], speakers: usize, rate: u32) {
        out.fill(0.0);
        while let Ok(message) = self.incoming.try_recv() {
            self.apply(message, rate);
        }
        let speakers = speakers.max(1);
        self.sources.retain(|s| !s.shared.closed.load(Ordering::Acquire));
        let frames = out.len() / speakers;
        for source in &mut self.sources {
            let shared = &source.shared;
            let need = frames * shared.layout.count();
            if self.scratch.len() < need {
                // ponytail: grows on the audio thread once, for an output buffer larger than ever before.
                self.scratch.resize(need, 0.0);
            }
            let input = &mut self.scratch[..need];
            match (&shared.ring, &mut source.fill) {
                (Some(ring), _) => {
                    if shared.clear.swap(false, Ordering::AcqRel) {
                        ring.skip_all();
                    }
                    // What is not there yet plays as silence.
                    let got = ring.pop(input);
                    input[got..].fill(0.0);
                }
                (None, Some(fill)) => {
                    input.fill(0.0);
                    fill(input);
                }
                (None, None) => continue,
            }
            let volume = shared.volume.get();
            if volume != 0.0 {
                add(out, speakers, input, shared.layout.count(), volume);
            }
        }
        self.render_channels(out, speakers);
        let master = self.master.get();
        for s in out.iter_mut() {
            *s = (*s * master).clamp(-1.0, 1.0);
        }
    }

    fn apply(&mut self, message: Message, rate: u32) {
        match message {
            Message::Source(source) => self.sources.push(source),
            Message::Play { channel, sample, note, stream } => {
                let Some(slot) = self.playing.get_mut(channel) else { return };
                let shared = &self.channels[channel];
                if let Some(mut previous) = slot.take()
                    && note.keep_previous
                {
                    // Impulse Tracker's "note continue": the old note rings on with the volume
                    // and pitch it had.
                    previous.fixed = Some((shared.note_volume.get(), shared.speed.get()));
                    if self.background.len() == BACKGROUND {
                        self.background.remove(0);
                    }
                    self.background.push((channel, previous));
                }
                *slot = Playing::new(sample, note, stream, rate);
                shared.playing.store(slot.is_some(), Ordering::Release);
                shared.pending.store(false, Ordering::Release);
            }
            Message::Stop(channel) => {
                if let Some(slot) = self.playing.get_mut(channel) {
                    *slot = None;
                    self.channels[channel].playing.store(false, Ordering::Release);
                }
            }
        }
    }

    fn render_channels(&mut self, out: &mut [f32], speakers: usize) {
        let solo = self.channels.iter().any(|c| c.solo.load(Ordering::Relaxed));
        let gains = |c: &ChannelShared| -> f32 {
            let silent = c.muted.load(Ordering::Relaxed) || (solo && !c.solo.load(Ordering::Relaxed));
            if silent { 0.0 } else { c.volume.get() }
        };
        for (index, slot) in self.playing.iter_mut().enumerate() {
            let Some(playing) = slot else { continue };
            let c = &self.channels[index];
            let ended = playing.mix(out, speakers, gains(c) * c.note_volume.get(), c.pan.get(), c.speed.get());
            if ended {
                *slot = None;
                c.playing.store(false, Ordering::Release);
            }
        }
        let channels = &self.channels;
        self.background.retain_mut(|(index, playing)| {
            let c = &channels[*index];
            let (volume, speed) = playing.fixed.unwrap_or((1.0, 1.0));
            !playing.mix(out, speakers, gains(c) * volume, c.pan.get(), speed)
        });
    }
}

/// A note playing on a channel.
struct Playing {
    sample: Sample,
    stream: Option<Cursor>,
    /// Position in the sample's frames, and the direction (ping-pong loops go back).
    pos: f64,
    forward: bool,
    /// Sample frames per output frame at speed 1 (the sample's rate over the output's).
    step: f64,
    looping: Loop,
    /// Volume and pitch frozen when the note went to the background.
    fixed: Option<(f32, f32)>,
}

impl Playing {
    fn new(sample: Sample, note: Note, stream: Option<Arc<Stream>>, rate: u32) -> Option<Playing> {
        let step = sample.rate() as f64 / rate.max(1) as f64;
        let looping = match (sample.looping(), note.looped) {
            (Loop::Off, true) if sample.frames() > 0 => Loop::Forward { start: 0, end: sample.frames() },
            (looping, _) => looping,
        };
        let stream = stream.map(Cursor::new);
        if stream.is_none() && sample.frames() == 0 {
            return None;
        }
        Some(Playing { sample, stream, pos: note.start as f64, forward: true, step, looping, fixed: None })
    }

    /// Adds this note into `out` for the whole buffer; true when it ended.
    fn mix(&mut self, out: &mut [f32], speakers: usize, volume: f32, pan: f32, speed: f32) -> bool {
        let layout = self.sample.layout();
        // Mono: equal power, 1 in both at the center. Stereo: a balance that turns one side down.
        let (gl, gr) = match layout {
            Layout::Mono => {
                let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2)
            }
            Layout::Stereo => ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0)),
        };
        let step = self.step * speed.max(0.0) as f64;
        // The sample's data, held apart from `self` (one reference count per buffer, no copy).
        let sample = self.sample.clone();
        for frame in out.chunks_exact_mut(speakers) {
            // The frame at the position, and whether the note ends after it.
            let ((l, r), ended) = if let Some(cursor) = &mut self.stream {
                match cursor.next(step, layout) {
                    Some(f) => (f, false),
                    None => return true,
                }
            } else {
                let Kind::Memory(data) = &*sample.0 else { return true };
                let f = read(data, self.pos);
                (f, self.advance(data, step))
            };
            if volume != 0.0 {
                let (l, r) = (l * gl * volume, r * gr * volume);
                if speakers == 1 {
                    frame[0] += (l + r) * 0.5;
                } else {
                    frame[0] += l;
                    frame[1] += r;
                }
            }
            if ended {
                return true;
            }
        }
        false
    }

    /// Moves on by `step` frames through the loop; true at the end of a sample that does not loop.
    fn advance(&mut self, data: &Data, step: f64) -> bool {
        let len = (data.frames.len() / data.layout.count()) as f64;
        match self.looping {
            Loop::Off => {
                self.pos += step;
                self.pos >= len
            }
            Loop::Forward { start, end } => {
                self.pos += step;
                let (start, end) = (start as f64, end as f64);
                if self.pos >= end {
                    self.pos = start + (self.pos - end) % (end - start).max(1.0);
                }
                false
            }
            Loop::PingPong { start, end } => {
                let (start, last) = (start as f64, (end as f64 - 1.0).max(start as f64));
                if self.forward {
                    self.pos += step;
                    if self.pos >= last {
                        self.pos = (last - (self.pos - last)).max(start);
                        self.forward = false;
                    }
                } else {
                    self.pos -= step;
                    if self.pos <= start {
                        self.pos = (start + (start - self.pos)).min(last);
                        self.forward = true;
                    }
                }
                false
            }
        }
    }
}

/// A frame of a memory sample at a fractional position (cubic Hermite between its neighbours).
fn read(data: &Data, pos: f64) -> (f32, f32) {
    let n = data.layout.count();
    let len = data.frames.len() / n;
    let i = pos.floor() as isize;
    let t = (pos - pos.floor()) as f32;
    let at = |k: isize, ch: usize| data.frames[(k.clamp(0, len as isize - 1) as usize) * n + ch];
    let value = |ch: usize| hermite(at(i - 1, ch), at(i, ch), at(i + 1, ch), at(i + 2, ch), t);
    match data.layout {
        Layout::Mono => {
            let v = value(0);
            (v, v)
        }
        Layout::Stereo => (value(0), value(1)),
    }
}

fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

/// Reads a streamed sample from its ring at any speed: four frames around the position.
struct Cursor {
    stream: Arc<Stream>,
    window: [[f32; 2]; 4],
    frac: f64,
    /// Frames still to play after the decoder ended and the ring ran dry (the window's tail).
    tail: Option<usize>,
}

impl Cursor {
    fn new(stream: Arc<Stream>) -> Cursor {
        let mut cursor = Cursor { stream, window: [[0.0; 2]; 4], frac: 0.0, tail: None };
        // The window starts one frame before the first: frames 0, 1, 2 come in now.
        for _ in 0..3 {
            cursor.shift();
        }
        cursor
    }

    /// Takes the next frame from the ring into the window; silence when it is not there yet.
    fn shift(&mut self) {
        self.window.rotate_left(1);
        let n = self.stream.layout.count();
        let mut f = [0.0f32; 2];
        if self.stream.ring.pop(&mut f[..n]) == n {
            if n == 1 {
                f[1] = f[0];
            }
        } else if self.stream.ended.load(Ordering::Acquire) {
            // Three padding frames in: the window's current frame is past the end.
            let left = self.tail.get_or_insert(3);
            *left = left.saturating_sub(1);
        }
        self.window[3] = f;
    }

    fn next(&mut self, step: f64, _layout: Layout) -> Option<(f32, f32)> {
        if self.tail == Some(0) {
            return None;
        }
        let t = self.frac as f32;
        let w = &self.window;
        let frame = (hermite(w[0][0], w[1][0], w[2][0], w[3][0], t), hermite(w[0][1], w[1][1], w[2][1], w[3][1], t));
        self.frac += step;
        while self.frac >= 1.0 {
            self.frac -= 1.0;
            self.shift();
        }
        Some(frame)
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        self.stream.closed.store(true, Ordering::Release);
    }
}

/// Adds `input` (`from` samples per frame) into `out` (`to` per frame): mono goes to every speaker,
/// stereo to the first two (averaged for a mono output).
fn add(out: &mut [f32], to: usize, input: &[f32], from: usize, volume: f32) {
    for (frame, src) in out.chunks_exact_mut(to).zip(input.chunks_exact(from)) {
        match (from, to) {
            (1, _) => frame.iter_mut().for_each(|o| *o += src[0] * volume),
            (_, 1) => frame[0] += (src[0] + src[1]) * 0.5 * volume,
            _ => {
                frame[0] += src[0] * volume;
                frame[1] += src[1] * volume;
            }
        }
    }
}
