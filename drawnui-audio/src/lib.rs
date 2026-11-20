//! Sound for DrawnUI for Rust apps: an output on Windows, macOS, Linux, iOS, Android and the
//! browser, streams of samples the app pushes, generators that make sound on the audio side, all
//! mixed with their own volumes.
//!
//! ```no_run
//! use drawnui_audio::Audio;
//!
//! let audio = Audio::open().expect("an audio output");
//! let rate = audio.sample_rate() as f32;
//! // A stream the app fills (an emulator's sound chip, a synth): mono, 100 ms of room.
//! let chip = audio.stream(1, 100);
//! let samples: Vec<f32> = (0..960).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / rate).sin() * 0.2).collect();
//! chip.push_slice(&samples);
//! // A generator runs where the sound is made, filling each buffer it is asked for.
//! let mut phase = 0.0f32;
//! let hum = audio.generator(1, move |buffer| {
//!     for s in buffer.iter_mut() {
//!         *s = (phase * std::f32::consts::TAU).sin() * 0.1;
//!         phase = (phase + 110.0 / rate) % 1.0;
//!     }
//! });
//! hum.set_volume(0.5);
//! ```
//!
//! The browser starts sound only after the user's first click, tap or key on the page (its
//! autoplay rule): until then the output is silent and `is_running` reads false; streams keep
//! their samples meanwhile, up to their room.

mod mixer;
mod ring;

#[cfg(not(target_os = "emscripten"))]
mod native;
#[cfg(target_os = "emscripten")]
mod web;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use mixer::{Fill, Mixer, Shared, Source, Volume};
use ring::Ring;

/// Why an output could not be opened.
#[derive(Debug, Clone)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audio: {}", self.0)
    }
}

impl std::error::Error for AudioError {}

/// What the app's handles and the output share.
pub(crate) struct Output {
    pub(crate) sample_rate: AtomicU32,
    pub(crate) channels: AtomicU32,
    pub(crate) running: AtomicBool,
    pub(crate) paused: AtomicBool,
    pub(crate) master: Arc<Volume>,
    sources: Mutex<Sender<Source>>,
}

enum Backend {
    #[cfg(not(target_os = "emscripten"))]
    Device(native::Device),
    #[cfg(target_os = "emscripten")]
    Web,
    Null { mixer: Mutex<Mixer>, recorded: Mutex<Vec<f32>> },
}

/// An audio output: the device's sample rate and channels, the sources mixed into it, pause and
/// resume. Dropping it stops the sound.
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

    /// An output with no device, for tests: nothing plays; `render` mixes the sources and keeps
    /// what it mixed, `recorded` returns it.
    pub fn null(sample_rate: u32, channels: u16) -> Audio {
        let (output, mixer) = Audio::parts(sample_rate, channels);
        output.running.store(true, Ordering::Release);
        Audio { output, backend: Backend::Null { mixer: Mutex::new(mixer), recorded: Mutex::new(Vec::new()) } }
    }

    fn parts(sample_rate: u32, channels: u16) -> (Arc<Output>, Mixer) {
        let (sender, receiver) = channel();
        let master = Arc::new(Volume::new(1.0));
        let output = Arc::new(Output {
            sample_rate: AtomicU32::new(sample_rate),
            channels: AtomicU32::new(channels as u32),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            master: master.clone(),
            sources: Mutex::new(sender),
        });
        (output, Mixer::new(receiver, master))
    }

    /// Frames per second the output plays. Produce samples at this rate. It can change when the
    /// output device changes (headphones plugged in): read it again rather than keeping it.
    pub fn sample_rate(&self) -> u32 {
        self.output.sample_rate.load(Ordering::Acquire)
    }

    /// The output's channels (2 for stereo). Sources are mono or stereo whatever this is.
    pub fn channels(&self) -> u16 {
        self.output.channels.load(Ordering::Acquire) as u16
    }

    /// The output is playing: opened, not paused, and in the browser unlocked by the user's first
    /// gesture.
    pub fn is_running(&self) -> bool {
        self.output.running.load(Ordering::Acquire) && !self.output.paused.load(Ordering::Acquire)
    }

    /// A stream the app pushes samples into: `channels` 1 (mono) or 2 (stereo, interleaved), with
    /// room for `room_ms` milliseconds at the current sample rate. Samples that do not fit are
    /// dropped; when it runs dry it plays silence.
    pub fn stream(&self, channels: u16, room_ms: u32) -> AudioStream {
        let channels = channels.clamp(1, 2) as usize;
        let rate = self.sample_rate().max(1) as usize;
        let frames = (rate * room_ms.max(1) as usize).div_ceil(1000);
        let shared = self.add(channels, Some(Ring::new(frames * channels)), None);
        AudioStream { shared, output: self.output.clone() }
    }

    /// A source that makes its own sound: `fill` gets each buffer the output needs (interleaved,
    /// `channels` 1 or 2 per frame, zeroed) and writes into it. It runs on the audio thread (the
    /// page's own thread in the browser): keep it short, no locks, no allocation.
    pub fn generator(&self, channels: u16, fill: impl FnMut(&mut [f32]) + Send + 'static) -> AudioGenerator {
        let shared = self.add(channels.clamp(1, 2) as usize, None, Some(Box::new(fill)));
        AudioGenerator { shared }
    }

    fn add(&self, channels: usize, ring: Option<Ring>, fill: Option<Fill>) -> Arc<Shared> {
        let shared = Arc::new(Shared { volume: Volume::new(1.0), closed: AtomicBool::new(false), channels, ring, clear: AtomicBool::new(false) });
        let source = Source { shared: shared.clone(), fill };
        let _ = self.output.sources.lock().unwrap_or_else(|e| e.into_inner()).send(source);
        shared
    }

    /// The volume of everything (1 = as mixed).
    pub fn set_volume(&self, volume: f32) {
        self.output.master.set(volume);
    }

    pub fn volume(&self) -> f32 {
        self.output.master.get()
    }

    /// Stops the output (the app went to the background); the sources keep what they hold.
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

    /// Null output: mixes `frames` frames of every source now, keeps them for `recorded` and
    /// returns them (interleaved). Other outputs: an empty Vec (their device asks for samples).
    pub fn render(&self, frames: usize) -> Vec<f32> {
        let Backend::Null { mixer, recorded } = &self.backend else { return Vec::new() };
        let mut out = vec![0.0; frames * self.channels().max(1) as usize];
        if !self.output.paused.load(Ordering::Acquire) {
            mixer.lock().unwrap_or_else(|e| e.into_inner()).render(&mut out, self.channels() as usize);
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
    /// around its target: skip a step when it is high, add one when it is low.
    pub fn queued(&self) -> usize {
        self.ring().len() / self.shared.channels
    }

    /// `queued` in milliseconds at the output's current sample rate.
    pub fn queued_ms(&self) -> f32 {
        let rate = self.output.sample_rate.load(Ordering::Acquire).max(1);
        self.queued() as f32 * 1000.0 / rate as f32
    }

    /// The most frames it holds.
    pub fn room(&self) -> usize {
        self.ring().capacity() / self.shared.channels
    }

    pub fn channels(&self) -> u16 {
        self.shared.channels as u16
    }

    /// Drops what is queued (the mixer forgets it at its next buffer).
    pub fn clear(&self) {
        self.shared.clear.store(true, Ordering::Release);
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared.volume.set(volume);
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
        self.shared.volume.set(volume);
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
mod tests {
    use super::Audio;

    #[test]
    fn mixes_streams_and_generators_with_their_volumes() {
        let audio = Audio::null(48_000, 2);
        let chip = audio.stream(1, 100);
        chip.push_slice(&[0.5; 4]);
        let stereo = audio.stream(2, 100);
        stereo.push_slice(&[0.1, -0.1, 0.1, -0.1]);
        stereo.set_volume(2.0);
        let tone = audio.generator(1, |b| b.fill(0.25));
        tone.set_volume(0.5);
        let out = audio.render(3);
        // Frames 0-1: 0.5 + stereo*2 + 0.125; frame 2: the stereo stream ran dry.
        let expect = [0.825, 0.425, 0.825, 0.425, 0.625, 0.625];
        for (o, e) in out.iter().zip(expect) {
            assert!((o - e).abs() < 1e-6, "{out:?}");
        }
        assert_eq!(chip.queued(), 1);
    }

    #[test]
    fn a_full_stream_drops_and_a_dry_one_plays_silence() {
        let audio = Audio::null(1000, 1);
        let s = audio.stream(1, 10); // 10 frames of room
        assert_eq!(s.room(), 10);
        assert_eq!(s.push_slice(&[1.0; 15]), 10);
        assert!(!s.push(1.0));
        assert!((s.queued_ms() - 10.0).abs() < 1e-3);
        let out = audio.render(12);
        assert_eq!(&out[..10], &[1.0; 10]);
        assert_eq!(&out[10..], &[0.0; 2]);
        assert_eq!(s.queued(), 0);
    }

    #[test]
    fn dropped_sources_leave_the_mix_and_pause_holds_the_queue() {
        let audio = Audio::null(1000, 1);
        let g = audio.generator(1, |b| b.fill(0.5));
        assert_eq!(audio.render(1), [0.5]);
        drop(g);
        assert_eq!(audio.render(1), [0.0]);
        let s = audio.stream(1, 10);
        s.push_slice(&[0.3; 4]);
        audio.pause();
        assert!(!audio.is_running());
        assert_eq!(audio.render(2), [0.0, 0.0]);
        assert_eq!(s.queued(), 4);
        audio.resume();
        s.clear();
        assert_eq!(audio.render(2), [0.0, 0.0]);
        assert_eq!(audio.recorded().len(), 6);
    }

    #[test]
    fn master_volume_scales_and_the_sum_is_clamped() {
        let audio = Audio::null(1000, 1);
        let _a = audio.generator(1, |b| b.fill(0.8));
        let _b = audio.generator(1, |b| b.fill(0.8));
        assert_eq!(audio.render(1), [1.0]);
        audio.set_volume(0.25);
        assert!((audio.render(1)[0] - 0.4).abs() < 1e-6);
    }
}
