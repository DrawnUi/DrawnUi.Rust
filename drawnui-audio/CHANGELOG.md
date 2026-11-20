# Changelog

## 0.1.0 (not released)

The first version: sound output on Windows, macOS, Linux, iOS, Android (cpal) and the browser
(Web Audio through emscripten's OpenAL, started by the user's first click, tap or key), streams the
app pushes (mono or stereo, lock-free, a fixed room, `queued` to pace a producer), generators that
fill buffers on the audio side, a mixer with a volume per source and a master volume, pause and
resume, a new device opened by itself when the playing one goes away, and a null output that
records the mix for tests.
