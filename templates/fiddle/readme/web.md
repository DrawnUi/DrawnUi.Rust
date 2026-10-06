# My App

Made with [DrawFiddle](https://drawfiddle.com) and [DrawnUI for Rust](https://github.com/DrawnUi/DrawnUi.Rust),
a UI engine that draws everything with Skia. Your code is in `src/app.rs`; `src/lib.rs` starts it
with the fonts DrawFiddle uses.

You need [Rust](https://rustup.rs) 1.94 or newer. Nothing for Skia: the first build downloads it
prebuilt for your platform. API docs: https://docs.rs/drawnui

## Web

Needs the WebAssembly target and Emscripten 6.0.10:

```
rustup target add wasm32-unknown-emscripten
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk
./emsdk install 6.0.10
./emsdk activate 6.0.10
source ./emsdk_env.sh        # Windows PowerShell: .\emsdk_env.ps1
```

Then, in this folder:

```
./web.sh                     # Windows PowerShell: .\web.ps1
python3 -m http.server 8080 -d dist
```

Open http://localhost:8080. `dist/` is the whole site (the page, `myapp.js` / `myapp.wasm`, the
assets and `drawnui_host.js`): upload it to any static host.
