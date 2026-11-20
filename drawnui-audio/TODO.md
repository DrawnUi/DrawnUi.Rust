# drawnui-audio TODO

One line each; found while building the first version. Nothing here blocks the main path.

- Verify at runtime on macOS, iOS (Core Audio) and in real Safari (Mac off on 2026-11-20); only compile checks so far.
- Verify at runtime on Android (AAudio, API 26+ = the examples' minSdk); compiles with cargo-ndk, no phone was attached. Needs examples/audio/android (copy of gallery3d/android).
- Linux: not even compiled (WSL lacks libasound2-dev; installing it is Nick's call). cpal needs ALSA headers at build time.
- iOS: set the AVAudioSession category (playback vs ambient: the silent switch mutes ambient), and resume after an interruption (a call).
- iOS Safari: Web Audio follows the silent switch unless navigator.audioSession.type = "playback".
- Windows: headphones plugged in while playing: check the stream follows the new default (cpal 0.18 reports StreamInvalidated, we reopen) without a gap the user hears.
- Measure real latency per platform (device buffer + our 60 ms target); offer a buffer-size hint if needed.
- Browser: background tab timers run at 1 s, so the 80 ms queue underruns; apps pause on hidden (documented), consider an AudioWorklet pump later.
- Streams and generators run at the output rate; a stream at its own rate (an emulator at 44.1 kHz fixed) would need the samples' interpolated playback.
- A generator that ends by itself (one-shot sounds) instead of the app dropping its handle.
- Impulse Tracker player: a hook that runs a sequencer on the audio side per tick (row timing exact to the sample), then an .it reader.
- Tracker app / live playing: an output buffer-size setting for low latency; IT volume envelopes and new note action "fade".
- Interpolation: cubic for all; a sinc option for sound designers; at loop seams the neighbours are clamped, not wrapped.
- Web: decoding a long file with Sample::decode runs on the page thread (a 2-minute MP3 took 120 ms natively); prefer Sample::streamed for music.
- Accessibility snapshot in the example lags ~1 s behind changes (engine; seen in UIA reads).
- One output per page in the browser (Audio::open twice returns an error); desktop allows several.
