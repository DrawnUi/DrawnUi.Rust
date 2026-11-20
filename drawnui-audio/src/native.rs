//! Desktop and phones: the default output device through cpal (WASAPI, Core Audio, ALSA, AAudio).
//! A thread of ours owns the device's stream: it opens it, pauses and resumes it, and opens the
//! default device again when the one playing goes away (headphones unplugged). The mixer outlives
//! each stream, so the sources keep playing on the new device.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::mixer::Mixer;
use crate::{AudioError, Output};

enum Command {
    Pause,
    Resume,
    /// The stream reported an error: open the default device again.
    Lost,
    Quit,
}

pub(crate) struct Device {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Device {
    pub(crate) fn open(output: Arc<Output>, mixer: Mixer) -> Result<Device, AudioError> {
        let (commands, receive) = channel();
        let (ready, opened) = channel();
        let lost = commands.clone();
        let thread = std::thread::Builder::new()
            .name("drawnui-audio".into())
            .spawn(move || run(output, Arc::new(Mutex::new(mixer)), receive, lost, ready))
            .map_err(|e| AudioError(e.to_string()))?;
        match opened.recv() {
            Ok(Ok(())) => Ok(Device { commands, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(AudioError("the audio thread ended".into())),
        }
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        let _ = self.commands.send(if paused { Command::Pause } else { Command::Resume });
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(output: Arc<Output>, mixer: Arc<Mutex<Mixer>>, commands: Receiver<Command>, lost: Sender<Command>, ready: Sender<Result<(), AudioError>>) {
    let mut stream = match start(&output, &mixer, &lost) {
        Ok(stream) => {
            let _ = ready.send(Ok(()));
            Some(stream)
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    loop {
        // While the device is gone, look for one twice a second.
        let command = if stream.is_none() { commands.recv_timeout(Duration::from_millis(500)).ok() } else { commands.recv().ok().or(Some(Command::Quit)) };
        match command {
            Some(Command::Quit) => break,
            Some(Command::Pause) => {
                if let Some(s) = &stream {
                    let _ = s.pause();
                }
            }
            Some(Command::Resume) => {
                if let Some(s) = &stream {
                    let _ = s.play();
                }
            }
            Some(Command::Lost) => {
                output.running.store(false, Ordering::Release);
                stream = None;
            }
            None => {}
        }
        if stream.is_none() {
            // Drain the Lost reports the old stream may have sent after the first one.
            stream = start(&output, &mixer, &lost).ok();
            if let Some(s) = &stream
                && output.paused.load(Ordering::Acquire)
            {
                let _ = s.pause();
            }
        }
    }
}

/// Opens the default output device and starts its stream.
fn start(output: &Arc<Output>, mixer: &Arc<Mutex<Mixer>>, lost: &Sender<Command>) -> Result<cpal::Stream, AudioError> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| AudioError("no output device".into()))?;
    let supported = device.default_output_config().map_err(|e| AudioError(e.to_string()))?;
    let config: cpal::StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, mixer, lost),
        SampleFormat::I16 => build::<i16>(&device, &config, mixer, lost),
        SampleFormat::U16 => build::<u16>(&device, &config, mixer, lost),
        SampleFormat::I32 => build::<i32>(&device, &config, mixer, lost),
        other => return Err(AudioError(format!("unsupported sample format {other}"))),
    }?;
    stream.play().map_err(|e| AudioError(e.to_string()))?;
    output.sample_rate.store(config.sample_rate, Ordering::Release);
    output.channels.store(config.channels as u32, Ordering::Release);
    output.running.store(true, Ordering::Release);
    Ok(stream)
}

fn build<T: SizedSample + FromSample<f32>>(device: &cpal::Device, config: &cpal::StreamConfig, mixer: &Arc<Mutex<Mixer>>, lost: &Sender<Command>) -> Result<cpal::Stream, AudioError> {
    let channels = config.channels as usize;
    let mixer = mixer.clone();
    let lost = lost.clone();
    // ponytail: the float buffer grows once to the largest buffer the device asks for.
    let mut mixed: Vec<f32> = Vec::with_capacity(8192);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                mixed.resize(data.len(), 0.0);
                // Only this thread locks it while the stream plays: never waits.
                match mixer.try_lock() {
                    Ok(mut m) => m.render(&mut mixed, channels),
                    Err(_) => mixed.fill(0.0),
                }
                for (d, s) in data.iter_mut().zip(&mixed) {
                    *d = T::from_sample(*s);
                }
            },
            move |error: cpal::Error| {
                // A glitch or a refused real-time priority plays on; a stream that followed the
                // default device to a new one plays on. The device gone: open it again.
                use cpal::ErrorKind::*;
                if matches!(error.kind(), DeviceNotAvailable | StreamInvalidated | HostUnavailable | BackendError) {
                    let _ = lost.send(Command::Lost);
                }
            },
            None,
        )
        .map_err(|e| AudioError(e.to_string()))
}
