# drawnui-audio

Sound for [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust) apps, on Windows, macOS,
Linux, iOS, Android and the browser, in Impulse Tracker's model: numbered **channels** that play
**samples** with their own volume, panning, loops and pitch. Next to them, **streams** of samples
the app pushes (an emulator's sound chip, a synth) and **generators** that make their sound on the
audio side. Everything is mixed with its own volume, then a master volume.

```toml
[dependencies]
drawnui-audio = "0.1"
```

## Open an output

```rust
use drawnui_audio::{Audio, Layout, Note, Sample};

let audio = Audio::open()?;          // the default output device
let rate = audio.sample_rate();      // 48000 in the browser; follows a device change
let speakers = audio.speakers();     // 2 for stereo; everything plays on any output
```

## Samples

```rust
let brick = Sample::decode(&bytes)?;                     // WAV, MP3, OGG Vorbis, FLAC, decoded once
let music = Sample::streamed(bytes)?;                    // decoded while it plays, a little ahead
let blip = Sample::from_frames(frames, Layout::Mono, 44_100);   // made in code
let pad = blip.with_loop(Loop::Forward { start: 2000, end: 8000 });   // or Loop::PingPong
```

A sample plays at its own pitch on any output: a 22050 Hz sound on a 48000 Hz device sounds as
recorded (cubic interpolation). Files with more than two channels keep the first two.

## Channels and notes

```rust
audio.set_channels(16);                      // 1 to 64; 16 at first, like a new IT song
let lead = audio.channel(0);
lead.set_volume(0.8);
lead.set_pan(-0.3);                          // -1 left, 0 center, 1 right
lead.play(&brick, Note::default());          // the note playing here is cut
lead.play(&brick, Note { speed: 1.5, volume: 0.6, ..Note::default() });
lead.set_speed(1.2);                         // live pitch of the playing note (slides, vibrato)
lead.set_note_volume(0.3);                   // live volume of the playing note (fades)
lead.stop();                                 // note cut
audio.channel(3).set_muted(true);
audio.channel(4).set_solo(true);             // while any channel is solo, only solo ones play

audio.channel(0).set_reserved(true);         // the music's channel: `play` below never takes it
audio.channel(0).play(&music, Note { looped: true, ..Note::default() });
audio.play(&brick, Note { pan: Some(0.7), ..Note::default() });   // the next free channel
```

`Note`: `volume` (1 = as recorded), `speed` (pitch: 2 = an octave up), `looped` (the whole sample
over and over when it has no loop of its own), `start` (frame offset, IT's sample offset), `pan`
(sets the channel's panning with the note), `keep_previous` (IT's new note action "continue": the
note playing on the channel keeps ringing in the background instead of being cut).

`audio.play` takes the first free channel that is not reserved; when all are busy, the one whose
note started first. It returns the channel, for a slide or a stop. A mono sample is panned with
equal power (full in both speakers at the center); a stereo one is balanced (the far side turns
down).

## Streams: samples the app pushes

```rust
let chip = audio.stream(Layout::Mono, 100);  // room for 100 ms at the output's rate
chip.push(sample);                           // one sample: no allocation, no lock
chip.push_slice(&samples);                   // or many; returns how many fit
let ms = chip.queued_ms();                   // what waits to be played
chip.set_volume(0.8);
```

A stream plays at the output's rate. It plays silence when it runs dry and drops samples beyond
its room, so it never grows. An app that makes its sound in steps (an emulator, a synth driven by
the frame loop) keeps `queued_ms` near a target, 50 to 80 ms, by making a step more or less.
Hold back only while the output is pulling (the queue dropped since the last frame): a paused or
stalled output, or a null one nobody renders, keeps the queue full, and a loop that holds back
whenever it is high would stop for good.

## Generators: sound made on the audio side

```rust
let mut phase = 0.0f32;
let hum = audio.generator(Layout::Mono, move |buffer| {
    for s in buffer.iter_mut() {
        *s = (phase * std::f32::consts::TAU).sin() * 0.2;
        phase = (phase + 110.0 / 48_000.0) % 1.0;
    }
});
hum.set_volume(0.5);                 // drop `hum` to remove it
```

The closure fills each buffer the output asks for (zeroed, interleaved, at the output's rate). It
runs on the audio thread (on the page's thread in the browser): keep it short, with no locks and
no allocation.

## Volume, pause and the app's lifecycle

`audio.set_volume` applies to everything. `audio.pause()` and `audio.resume()` stop and restart the
output; everything keeps its place meanwhile. Pause while the app is in the background:

```rust
Ui::new(app, build).on_visibility_changed(|app: &mut App, visible, _cx| app.audio.set_paused(!visible))
```

When the playing device goes away (headphones unplugged), the output opens the default device
again by itself.

## The browser

The page may play sound only after the user's first click, tap or key on it. The output opens its
audio there, at 48000 Hz stereo, so it plays from that moment with no warning in the console; until
then `is_running()` reads false and the streams keep their samples. The browser build links
emscripten's OpenAL (Web Audio) by itself; no build flag is needed.

## Features

`files` (on by default): `Sample::decode` and `Sample::streamed` (WAV, MP3, OGG Vorbis, FLAC through
symphonia, pure Rust; about 650 KB more in a browser build). An app that only pushes streams or
makes samples in code can leave it out: `default-features = false`.

## Tests

`Audio::null(rate, speakers)` is an output with no device: `render(frames)` mixes everything on the
spot (and decodes streamed music) and returns the samples, `recorded()` returns all of them.

```rust
let audio = Audio::null(48_000, 1);
let s = audio.stream(Layout::Mono, 50);
s.push_slice(&[0.5; 4]);
assert_eq!(audio.render(4), [0.5; 4]);
```

## Example

`examples/audio` in the repository: a sample played on channels (left, right, a chord at other
pitches), a tone the app pushes every frame and a chime from a generator, with their volumes and a
master volume, on the desktop and in the browser (`dev\build.ps1 -Example audio -WebOnly`).
