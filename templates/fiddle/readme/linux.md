# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

## Linux

Needs a C toolchain and a few development packages. On Debian or Ubuntu:

```
sudo apt install build-essential pkg-config libfontconfig1-dev libx11-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev libegl1-mesa-dev
cargo run --release
```

The app is `target/release/myapp`, with its `assets` folder and `icon.ico` next to it. It runs on
X11 and Wayland with OpenGL.
