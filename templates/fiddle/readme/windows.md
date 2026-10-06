# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

## Windows

Needs the Visual Studio C++ build tools ("Desktop development with C++", or the free Build Tools).

```
cargo run --release
```

The app is `target\release\myapp.exe`, with its `assets` folder next to it.
