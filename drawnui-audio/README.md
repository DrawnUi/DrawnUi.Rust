# drawnui-audio

Sound for [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust) apps, on Windows, macOS,
Linux, iOS, Android and the browser: an output on the system's default device, streams of samples
the app pushes, generators that make their sound on the audio side, and a mixer with a volume per
source and a master volume.

```toml
[dependencies]
drawnui-audio = { git = "https://github.com/DrawnUi/DrawnUi.Rust" }
```

## Open an output

```rust
use drawnui_audio::Audio;

let audio = Audio::open()?;          // the default output device
let rate = audio.sample_rate();      // frames per second to produce (48000 in the browser)
let channels = audio.channels();     // the device's channels; sources are mono or stereo anyway
```

Make the samples at `sample_rate()`. Read it again when you need it: it follows the device when
the output moves to another one (headphones plugged in or out).

## Streams: samples the app pushes

```rust
let chip = audio.stream(1, 100);     // mono, room for 100 ms
chip.push(sample);                   // one sample: no allocation, no lock
chip.push_slice(&samples);           // or many; returns how many fit
let ms = chip.queued_ms();           // what waits to be played
chip.set_volume(0.8);
```

A stream plays what it holds and plays silence when it runs dry; samples that do not fit its room
are dropped, so it never grows. An app that makes its sound in steps (an emulator, a synth driven
by the frame loop) keeps `queued_ms` near a target, 50 to 80 ms, by making a step more or less.
Hold back only while the output is pulling (the queue dropped since the last frame): a paused or
stalled output, or a null one nobody renders, keeps the queue full, and a loop that holds back
whenever it is high would stop for good.
Stereo streams take interleaved samples (`audio.stream(2, 100)`).

## Generators: sound made on the audio side

```rust
let mut phase = 0.0f32;
let hum = audio.generator(1, move |buffer| {
    for s in buffer.iter_mut() {
        *s = (phase * std::f32::consts::TAU).sin() * 0.2;
        phase = (phase + 110.0 / 48_000.0) % 1.0;
    }
});
hum.set_volume(0.5);                 // drop `hum` to remove it
```

The closure fills each buffer the output asks for (zeroed, interleaved). It runs on the audio
thread (on the page's thread in the browser): keep it short, with no locks and no allocation.

## Mixing, pause and the app's lifecycle

Every stream and generator is mixed with its own volume, then `audio.set_volume` applies to
everything. `audio.pause()` and `audio.resume()` stop and restart the output; the streams keep
their samples meanwhile. Pause while the app is in the background:

```rust
Ui::new(app, build).on_visibility_changed(|app: &mut App, visible, _cx| app.audio.set_paused(!visible))
```

## The browser

The page may play sound only after the user's first click, tap or key on it. The output opens its
audio there, at 48000 Hz stereo, so it plays from that moment with no warning in the console;
until then `is_running()` reads false and the streams keep their samples. The browser build links
emscripten's OpenAL (Web Audio) by itself; no build flag is needed.

## Tests

`Audio::null(rate, channels)` is an output with no device: `render(frames)` mixes every source on
the spot and returns the samples, `recorded()` returns all of them, so an app can test its sound.

```rust
let audio = Audio::null(48_000, 1);
let s = audio.stream(1, 50);
s.push_slice(&[0.5; 4]);
assert_eq!(audio.render(4), [0.5; 4]);
```

## Example

`examples/audio` in the repository: a tone the app pushes every frame and a chime from a
generator, with their volumes and a master volume, on the desktop and in the browser
(`dev\build.ps1 -Example audio -WebOnly` for the page).
