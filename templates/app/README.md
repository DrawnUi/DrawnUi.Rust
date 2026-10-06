# DrawnUI for Rust: app template

A counter app that builds for Windows, Linux, macOS, the browser and Android from one source. Copy
this folder, then rename `myapp` in `Cargo.toml`, `src/main.rs`, `web/index.html`, `web.ps1` and
`web.sh`.

## What you need

- Rust 1.94 or newer.
- Nothing for Skia: drawnui (crates.io) brings it, and its build downloads Skia prebuilt for your
  target.
- Browser: `rustup target add wasm32-unknown-emscripten` and emsdk 6
  (https://emscripten.org/docs/getting_started/downloads.html), its tools on PATH (`emsdk_env`).
- Android: `cargo install cargo-ndk`, NDK 26, the Android Rust targets.

## Build

```
cargo run --release              # desktop: the window, icon.ico on Windows and Linux
./web.ps1                        # or ./web.sh: the browser build in dist/, serve it over http
```

Android: build the library as the activity's native library, then pack it into an APK with a
GameActivity (Gradle, `androidx.games:games-activity` 4.4.0):

```
cargo ndk -t arm64-v8a -t x86_64 --platform 26 -o app/src/main/jniLibs rustc --lib --release --crate-type cdylib
```

Android also needs two fixes that are not upstream yet: winit 0.30.13 keeps a relaunched activity
black (rotation, a font scale change), android-activity 0.6.1 reads the soft keyboard's text state
before it exists. The drawnui repository has both as patches (`dev/winit-android`,
`dev/android-activity-null-text`); apply them to checkouts of those crates and point your own
`Cargo.toml` at them with `[patch.crates-io]` (a `[patch]` only works in the top-level project).
Desktop and browser apps need no patch.

## Files

- `src/lib.rs`: the app (`run`), and `android_main` for Android.
- `src/main.rs`: desktop and browser entry.
- `build.rs`, `app.rc`, `icon.ico`: the Windows exe icon, assets copied next to the exe.
- `assets/`: fonts and images, read from next to the exe, from `dist/` on the web.
- `web/index.html`, `web/favicon.ico`: the page. `web.ps1` / `web.sh` add the build and the
  `drawnui_host.js` of the drawnui version you depend on (the page script and the wasm must match).
- `.cargo/config.toml`: the Skia download, the browser's link flags (keep the 1 MB stack and its
  overflow check), 16 KB pages on Android.
