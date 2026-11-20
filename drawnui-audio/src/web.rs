//! The browser: Web Audio through emscripten's OpenAL. The page's own thread mixes on a 10 ms
//! timer and queues 10 ms buffers about 80 ms ahead. The audio context is made inside the user's
//! first click, tap or key on the page (the autoplay rule), so it starts running at once and the
//! console stays clean; until then the output is silent and the streams keep their samples.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mixer::Mixer;
use crate::{AudioError, Output};

const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const FRAMES: usize = 512;
const BUFFERS: usize = 8;

const ALC_FREQUENCY: i32 = 0x1007;
const AL_SOURCE_STATE: i32 = 0x1010;
const AL_PLAYING: i32 = 0x1012;
const AL_BUFFERS_PROCESSED: i32 = 0x1016;
const AL_FORMAT_STEREO_FLOAT32: i32 = 0x10011;
const EVENT_TARGET_DOCUMENT: *const i8 = 1 as *const i8;
const CALLING_THREAD: usize = 2;

type KeyOrMouse = extern "C" fn(i32, *const c_void, *mut c_void) -> bool;

unsafe extern "C" {
    fn alcOpenDevice(name: *const i8) -> *mut c_void;
    fn alcCreateContext(device: *mut c_void, attributes: *const i32) -> *mut c_void;
    fn alcMakeContextCurrent(context: *mut c_void) -> u8;
    fn alcDestroyContext(context: *mut c_void);
    fn alcCloseDevice(device: *mut c_void) -> u8;
    fn alGenSources(n: i32, out: *mut u32);
    fn alGenBuffers(n: i32, out: *mut u32);
    fn alDeleteSources(n: i32, sources: *const u32);
    fn alDeleteBuffers(n: i32, buffers: *const u32);
    fn alBufferData(buffer: u32, format: i32, data: *const c_void, size: i32, freq: i32);
    fn alSourceQueueBuffers(source: u32, n: i32, buffers: *const u32);
    fn alSourceUnqueueBuffers(source: u32, n: i32, buffers: *mut u32);
    fn alSourceStop(source: u32);
    fn alGetSourcei(source: u32, param: i32, out: *mut i32);
    fn alSourcePlay(source: u32);
    fn emscripten_alcDevicePauseSOFT(device: *mut c_void);
    fn emscripten_alcDeviceResumeSOFT(device: *mut c_void);
    fn emscripten_set_interval(cb: extern "C" fn(*mut c_void), ms: f64, user: *mut c_void) -> i32;
    fn emscripten_clear_interval(id: i32);
    fn emscripten_set_keydown_callback_on_thread(target: *const i8, user: *mut c_void, capture: bool, cb: Option<KeyOrMouse>, thread: usize) -> i32;
    fn emscripten_set_mousedown_callback_on_thread(target: *const i8, user: *mut c_void, capture: bool, cb: Option<KeyOrMouse>, thread: usize) -> i32;
    fn emscripten_set_touchend_callback_on_thread(target: *const i8, user: *mut c_void, capture: bool, cb: Option<KeyOrMouse>, thread: usize) -> i32;
}

struct Web {
    output: Arc<Output>,
    mixer: Mixer,
    timer: i32,
    device: *mut c_void,
    context: *mut c_void,
    source: u32,
    free: Vec<u32>,
    all: Vec<u32>,
    chunk: Vec<f32>,
}

thread_local! {
    /// One output per page: everything runs on the page's thread.
    static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
}

pub(crate) fn open(output: Arc<Output>, mixer: Mixer) -> Result<(), AudioError> {
    if WEB.with_borrow(|w| w.is_some()) {
        return Err(AudioError("one audio output per page".into()));
    }
    output.sample_rate.store(RATE, Ordering::Release);
    output.channels.store(CHANNELS as u32, Ordering::Release);
    // SAFETY: emscripten's timer and html5 event registrations with plain function pointers.
    let timer = unsafe {
        emscripten_set_keydown_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, Some(gesture), CALLING_THREAD);
        emscripten_set_mousedown_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, Some(gesture), CALLING_THREAD);
        emscripten_set_touchend_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, Some(gesture), CALLING_THREAD);
        emscripten_set_interval(pump, 10.0, std::ptr::null_mut())
    };
    let web = Web { output, mixer, timer, device: std::ptr::null_mut(), context: std::ptr::null_mut(), source: 0, free: Vec::new(), all: Vec::new(), chunk: vec![0.0; FRAMES * CHANNELS] };
    WEB.set(Some(web));
    Ok(())
}

/// A click, tap or key on the page: the first one makes the audio context (allowed to play now);
/// later ones resume it if the browser suspended it.
extern "C" fn gesture(_kind: i32, _event: *const c_void, _user: *mut c_void) -> bool {
    WEB.with_borrow_mut(|w| {
        let Some(w) = w else { return };
        // SAFETY: OpenAL calls on the page's thread with the ids it handed out.
        unsafe {
            if w.device.is_null() {
                let device = alcOpenDevice(std::ptr::null());
                if device.is_null() {
                    return;
                }
                let attributes = [ALC_FREQUENCY, RATE as i32, 0];
                let context = alcCreateContext(device, attributes.as_ptr());
                if context.is_null() {
                    alcCloseDevice(device);
                    return;
                }
                alcMakeContextCurrent(context);
                let mut source = 0;
                alGenSources(1, &mut source);
                let mut buffers = [0u32; BUFFERS];
                alGenBuffers(BUFFERS as i32, buffers.as_mut_ptr());
                (w.device, w.context, w.source) = (device, context, source);
                w.free = buffers.to_vec();
                w.all = buffers.to_vec();
                w.output.running.store(true, Ordering::Release);
            } else if !w.output.paused.load(Ordering::Acquire) {
                emscripten_alcDeviceResumeSOFT(w.device);
            }
        }
    });
    // The page and the app get the event as usual.
    false
}

/// Every 10 ms: the buffers that played are mixed again and queued.
extern "C" fn pump(_user: *mut c_void) {
    WEB.with_borrow_mut(|w| {
        let Some(w) = w else { return };
        if w.device.is_null() || w.output.paused.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: as in `gesture`; the chunk outlives the alBufferData call, which copies it.
        unsafe {
            let mut processed = 0;
            alGetSourcei(w.source, AL_BUFFERS_PROCESSED, &mut processed);
            for _ in 0..processed.max(0) {
                let mut buffer = 0;
                alSourceUnqueueBuffers(w.source, 1, &mut buffer);
                w.free.push(buffer);
            }
            while let Some(buffer) = w.free.pop() {
                w.mixer.render(&mut w.chunk, CHANNELS);
                alBufferData(buffer, AL_FORMAT_STEREO_FLOAT32, w.chunk.as_ptr().cast(), (w.chunk.len() * 4) as i32, RATE as i32);
                alSourceQueueBuffers(w.source, 1, &buffer);
            }
            let mut state = 0;
            alGetSourcei(w.source, AL_SOURCE_STATE, &mut state);
            if state != AL_PLAYING {
                alSourcePlay(w.source);
            }
        }
    });
}

pub(crate) fn set_paused(paused: bool) {
    WEB.with_borrow_mut(|w| {
        let Some(w) = w else { return };
        if w.device.is_null() {
            return;
        }
        // SAFETY: the device this output opened.
        unsafe {
            if paused {
                emscripten_alcDevicePauseSOFT(w.device);
            } else {
                emscripten_alcDeviceResumeSOFT(w.device);
            }
        }
    });
}

pub(crate) fn close() {
    let Some(w) = WEB.take() else { return };
    // SAFETY: the ids this output made; the callbacks are unregistered before the state goes.
    unsafe {
        emscripten_clear_interval(w.timer);
        emscripten_set_keydown_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, None, CALLING_THREAD);
        emscripten_set_mousedown_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, None, CALLING_THREAD);
        emscripten_set_touchend_callback_on_thread(EVENT_TARGET_DOCUMENT, std::ptr::null_mut(), true, None, CALLING_THREAD);
        if !w.device.is_null() {
            alSourceStop(w.source);
            alDeleteSources(1, &w.source);
            alDeleteBuffers(w.all.len() as i32, w.all.as_ptr());
            alcMakeContextCurrent(std::ptr::null_mut());
            alcDestroyContext(w.context);
            alcCloseDevice(w.device);
        }
    }
}
