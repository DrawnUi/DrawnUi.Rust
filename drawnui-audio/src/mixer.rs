//! The mixer: adds every source into the output buffer, each with its volume, then the master
//! volume. It runs where the output asks for samples (the audio thread on desktop and phones, the
//! page's timer in the browser, `Audio::render` for the null output) and never waits on the app.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::ring::Ring;

/// A volume shared between a handle (the app) and the mixer.
pub(crate) struct Volume(AtomicU32);

impl Volume {
    pub(crate) fn new(v: f32) -> Volume {
        Volume(AtomicU32::new(v.to_bits()))
    }
    pub(crate) fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub(crate) fn set(&self, v: f32) {
        self.0.store(v.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

/// What a source's handle and the mixer share.
pub(crate) struct Shared {
    pub(crate) volume: Volume,
    /// The handle is gone: the mixer drops the source.
    pub(crate) closed: AtomicBool,
    /// Samples per frame of this source: 1 (mono) or 2 (stereo, interleaved).
    pub(crate) channels: usize,
    /// A stream's samples (None for a generator).
    pub(crate) ring: Option<Ring>,
    /// `AudioStream::clear`: the mixer forgets what is queued.
    pub(crate) clear: AtomicBool,
}

/// A generator: fills its buffer (interleaved, `channels` per frame) on the mixer's thread.
pub(crate) type Fill = Box<dyn FnMut(&mut [f32]) + Send>;

pub(crate) struct Source {
    pub(crate) shared: Arc<Shared>,
    pub(crate) fill: Option<Fill>,
}

pub(crate) struct Mixer {
    incoming: Receiver<Source>,
    sources: Vec<Source>,
    scratch: Vec<f32>,
    master: Arc<Volume>,
}

impl Mixer {
    pub(crate) fn new(incoming: Receiver<Source>, master: Arc<Volume>) -> Mixer {
        Mixer { incoming, sources: Vec::new(), scratch: vec![0.0; 8192], master }
    }

    /// Mixes into `out` (interleaved, `channels` per frame): silence where nothing plays.
    pub(crate) fn render(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        while let Ok(source) = self.incoming.try_recv() {
            self.sources.push(source);
        }
        self.sources.retain(|s| !s.shared.closed.load(Ordering::Acquire));
        let channels = channels.max(1);
        let frames = out.len() / channels;
        for source in &mut self.sources {
            let shared = &source.shared;
            let need = frames * shared.channels;
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
            if volume == 0.0 {
                continue;
            }
            add(out, channels, input, shared.channels, volume);
        }
        let master = self.master.get();
        for s in out.iter_mut() {
            *s = (*s * master).clamp(-1.0, 1.0);
        }
    }
}

/// Adds `input` (`from` channels) into `out` (`to` channels): mono goes to every channel, stereo to
/// the first two (averaged for a mono output).
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
