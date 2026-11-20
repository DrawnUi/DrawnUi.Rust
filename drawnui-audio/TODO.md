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
- Resampling: sources must be at the output rate; a stream with its own rate (sound files at 44.1 kHz) needs a resampler. Needed with sound files.
- Sound files (WAV/OGG decode, play once, loop): next version, as the spec says.
- A generator that ends by itself (one-shot sounds) instead of the app dropping its handle.
- One output per page in the browser (Audio::open twice returns an error); desktop allows several.
