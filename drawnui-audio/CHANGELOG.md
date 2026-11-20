# Changelog

## 0.1.0 (not released)

Impulse Tracker's model: samples (decoded from WAV, MP3, OGG Vorbis and FLAC once, streamed
while they play, or made in code, with forward and ping-pong loops) played as notes on up to 64
numbered channels with volume, panning, mute, solo and live pitch and note volume; a note cuts the
channel's note or lets it ring on in the background; `Audio::play` takes the next free channel.
Every sample plays at its own pitch on any output. Streams and generators take `Layout::Mono` or
`Layout::Stereo`; the output's channel count is `speakers()`.

The first version: sound output on Windows, macOS, Linux, iOS, Android (cpal) and the browser
(Web Audio through emscripten's OpenAL, started by the user's first click, tap or key), streams the
app pushes (mono or stereo, lock-free, a fixed room, `queued` to pace a producer), generators that
fill buffers on the audio side, a mixer with a volume per source and a master volume, pause and
resume, a new device opened by itself when the playing one goes away, and a null output that
records the mix for tests.
