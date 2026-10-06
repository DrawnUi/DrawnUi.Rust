# DrawnUI for Rust

**Draw your whole UI with Skia, from one Rust source, on every screen.** DrawnUI for Rust is a UI
engine: layouts, controls you can subclass and paint, gestures, animations, caching, shaders and
accessibility, on Windows, macOS, Linux, iOS, Android and in the browser. It shares its controls and
rules with DrawnUI for .NET and DrawnUI for React, so one design runs the same in all three.

- **Try it in your browser:** https://hellorust.drawnui.net (the DrawnUI Hello app, 20 pages)
- **Play a game made with it:** https://run.drawnui.net (Dungeon Run, a 3D runner drawn with SkMesh)
- Crate: [`drawnui`](https://crates.io/crates/drawnui) · API docs: https://docs.rs/drawnui
- Release notes: [`CHANGELOG.md`](CHANGELOG.md) · Accessibility: [`ACCESSIBILITY.md`](ACCESSIBILITY.md)
  · What it shares with DrawnUI for .NET: [`PARITY.md`](PARITY.md)

```rust
use drawnui::prelude::*;

#[derive(Default)]
struct App { count: i32 }

fn build(_app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        SkiaLabel::new("").font_size(24).text_color(Color::WHITE)
            .observe(|me, app: &App| me.set_text(format!("Count {}", app.count))),
        SkiaButton::new("Tap me").on_tapped(|_me, app: &mut App, _cx| app.count += 1),
    ))
}

fn main() {
    drawnui::run("Counter", || Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf")));
}
```

## Why DrawnUI for Rust

- **A complete UI engine, ready to use.** Rendering, layouts, gestures, a full set of UI controls
  and the building blocks to make your own: everything you need to create pixel-perfect apps, out
  of the box.
- **The same pixels everywhere.** Every control is drawn by Skia, so your app looks the same on a
  phone, a desktop and a web page. No native widgets to fight.
- **Smooth by design.** No garbage collector, so there are no collection pauses in the middle of a
  scroll or an animation. Caches keep finished parts of the screen as pictures or GPU textures and
  draw them again instead of repainting them.
- **Fast on the web.** Your code and Skia live in one WebAssembly module and talk directly, with no
  JavaScript in between: up to 1.6 times less CPU per frame than CanvasKit driven from JavaScript
  (see [Benchmarks](#benchmarks)).
- **Fluent and familiar.** Builders in the style of DrawnUI's C# fluent API: compose, `observe` your
  state, handle a tap in one line.
- **Accessible.** Screen readers on every platform, keyboard navigation with a focus ring, values a
  screen reader reads and changes.
- **Nothing to set up.** Add one dependency. Skia comes prebuilt for your platform.

## Platforms

| Platform | Graphics | Skia prebuilt for |
|---|---|---|
| Windows | OpenGL | x64, ARM64 |
| macOS | Metal, paced by the display | Apple silicon, Intel |
| Linux | OpenGL, X11 and Wayland | x64, ARM64 |
| iOS | Metal, 120 Hz on ProMotion screens | devices, simulator (Apple silicon and Intel) |
| Android | Vulkan, with OpenGL ES as the fallback | arm64, armv7, x86_64, x86 |
| Browser | WebGL2 (`wasm32-unknown-emscripten`) | all browsers with WebGL2 |

## What's inside

- **Layout:** Absolute, Column, Row, Wrap, Grid and decorated grids; templated lists with recycling
  and virtualization; measure / arrange / paint, render transforms, opacity, clipping, z-index.
- **Controls:** shapes, labels and rich text, buttons, an editor with IME, scroll with scroll bars
  and pull to refresh, carousels (also with shader transitions), drawer, shell (pages, tabs, popups,
  modals, toasts), toggles, switch, checkbox, radio buttons, slider, progress, backdrop blur.
- **Media:** images (decoded off the frame thread), image tiles, SVG, GIF, Lottie, sprites, SkSL
  shader effects and transitions, SkMesh (your own vertex and fragment programs).
- **Caching:** every DrawnUI cache type: Operations, OperationsFull, Image, GPU, ImageDoubleBuffered
  (bitmaps made on background threads on the desktop), ImageComposite. A lost GPU context comes back
  on its own.
- **Input:** tap, pan, fling, long press, hover, context menu, mouse wheel and touchpad, keyboard
  and focus.
- **Accessibility:** UI Automation on Windows, VoiceOver on macOS and iOS, TalkBack on Android,
  AT-SPI / Orca on Linux, an ARIA overlay in the browser; selectable text.
- **Animation:** value, range, spring, ping-pong, pendulum and ripple animators, timers.
- **Tested:** about 700 headless tests, many of them ports of DrawnUI for .NET's tests with the same
  numbers.

## Crates

| Crate | What it is | Version |
|---|---|---|
| [`drawnui`](https://crates.io/crates/drawnui) | the engine: controls, layouts, caches, gestures, animations, accessibility, the desktop, mobile and browser hosts | 0.1.0-preview.4 |
| [`drawnui-skia-safe`](https://crates.io/crates/drawnui-skia-safe) | Skia for Rust: rust-skia's skia-safe plus SkMesh; drawnui re-exports it as `drawnui::skia` | 0.153.6 |
| [`drawnui-skia-bindings`](https://crates.io/crates/drawnui-skia-bindings) | the native Skia under drawnui-skia-safe, downloaded prebuilt from [DrawnUi/rust-skia](https://github.com/DrawnUi/rust-skia/releases) | 0.153.6 |

An app depends on `drawnui` only; the Skia crates come with it. Use Skia's API through
`drawnui::skia` (the same as `skia_safe`, SkMesh included) and do not add skia-safe or skia-bindings
to an app: a program links one Skia.

## Getting started

**The fastest way:** copy [`templates/app`](templates/app), rename `myapp`, build. It is an empty
app with an icon, assets, a web page and build scripts for the desktop, the browser and Android.

Or by hand, in a new project (`cargo new myapp`), `Cargo.toml`:

```toml
[dependencies]
drawnui = "0.1.0-preview.4"

[profile.release]
lto = "thin"
panic = "abort"
```

The first build downloads Skia for your platform (11 to 25 MB) from
[DrawnUi/rust-skia's releases](https://github.com/DrawnUi/rust-skia/releases). `SKIA_BINARIES_URL`
points the download at a mirror. Do not set `FORCE_SKIA_BINARIES_DOWNLOAD`: inside a crate it makes
skia-bindings compile Skia from source.

`.cargo/config.toml`, for the browser's link flags and Android's 16 KB pages:

```toml
[target.wasm32-unknown-emscripten]
linker = "em++"
rustflags = [
    "-C", "link-arg=-sMAX_WEBGL_VERSION=2",
    "-C", "link-arg=-sMODULARIZE=1",
    "-C", "link-arg=-sEXPORT_NAME=createDrawnUi",
    "-C", "link-arg=-sEXPORTED_RUNTIME_METHODS=GL,HEAPU8,UTF8ToString",
    "-C", "link-arg=-sALLOW_MEMORY_GROWTH=1",
    # A 1 MB stack with the overflow check: layout and paint walk the tree recursively.
    "-C", "link-arg=-sSTACK_SIZE=1048576",
    "-C", "link-arg=-sSTACK_OVERFLOW_CHECK=1",
    "-C", "link-arg=-sERROR_ON_UNDEFINED_SYMBOLS=0",
]

[target.'cfg(target_os = "android")']
rustflags = ["-C", "link-arg=-Wl,-z,max-page-size=16384"]
```

The web page loads your build and `drawnui_host.js` from the same drawnui version (the template's
`web.ps1` / `web.sh` copy it for you):

```html
<canvas id="canvas" style="width:100vw;height:100dvh;display:block;touch-action:none"></canvas>
<script src="myapp.js"></script>
<script src="drawnui_host.js"></script>
<script>DrawnUi.start({ canvas: document.getElementById("canvas"), create: createDrawnUi });</script>
```

Per platform:

- **Windows, Linux, macOS:** `cargo run --release`. Assets go next to the executable (the
  template's `build.rs`), `icon.ico` becomes the window and taskbar icon. Linux needs the X11 /
  Wayland and GL development packages.
- **Browser:** `rustup target add wasm32-unknown-emscripten`, emsdk 6 with its tools on PATH, then
  the template's `web.ps1` / `web.sh`; serve `dist/` over http.
- **Android:** cargo-ndk and NDK 26; `cargo ndk ... rustc --lib --release --crate-type cdylib` makes
  the activity's library (`android_main` in the template's `src/lib.rs`), packed by Gradle with a
  GameActivity. Add the winit and android-activity fixes from
  [`dev/winit-android`](dev/winit-android) and
  [`dev/android-activity-null-text`](dev/android-activity-null-text) with `[patch.crates-io]`.
- **iOS:** [`dev/ios`](dev/ios) builds an `.app` bundle for the simulator, or a signed one for a
  device.

## Benchmarks

The same draw list, N rounded rectangles per frame, 1609x1163 canvas, Chrome on Windows, WebGL2:

| Draw calls | DrawnUI for Rust (Rust + Skia in one wasm module) | CanvasKit 0.42 driven from JS |
|---|---|---|
| 2,000 | 59.9 FPS, 2.8 ms CPU | 60.0 FPS, 3.7 ms CPU |
| 5,000 | 60.0 FPS, 5.1 ms CPU | 59.8 FPS, 7.4 ms CPU |
| 20,000 | 59.7 FPS, 12.5 ms CPU | 47.8 FPS, 20.6 ms CPU |

Windows desktop (OpenGL): 20,000 draw calls at 60 FPS, 10 to 12 ms CPU. MacBook Air M1 (Metal):
20,000 draw calls at a steady 60 FPS, 10.3 ms CPU. SkMesh: 2,000 animated waves (a vertex program
each, one draw call each) in 6.1 ms CPU on the M1, and at 60 FPS in 6.4 ms on a Windows desktop.

The web build links only the Skia code it uses:

| Build | raw | brotli |
|---|---|---|
| `gl` | 3.93 MB | 1.15 MB |
| `gl` + text layout (ICU, HarfBuzz) + SVG | 7.39 MB | 2.27 MB |
| CanvasKit 0.42 wasm, for scale | 7.32 MB | 2.28 MB |

### Android: Vulkan by default

A phone with a Mali-G57 MC2, 1080x2408, a 90 Hz panel, the same release build on each API. **On
heavy scenes Vulkan draws 21 to 35 % more frames; where both keep up with the panel, it needs 26 to
59 % less CPU per frame.**

| Scene | Vulkan | OpenGL ES | Vulkan better by |
|---|---|---|---|
| 20,000 draw calls | 40.2 FPS, 24.8 ms a frame | 33.3 FPS, 30.0 ms | +21 % FPS |
| 2,000 SkMesh waves | 57.2 FPS, 17.5 ms | 42.4 FPS, 23.6 ms | +35 % FPS |
| 2,000 animated shapes | 91.6 FPS, CPU 5.4 ms | 91.6 FPS, CPU 7.3 ms | -26 % CPU |
| HelloRust: Recycled cells, flings | 91.6 FPS, CPU 1.45 ms | 91.6 FPS, CPU 2.71 ms | -46 % CPU |
| HelloRust: Uneven cells, flings | 91.6 FPS, CPU 0.94 ms | 91.6 FPS, CPU 2.32 ms | -59 % CPU |
| HelloRust: Lottie & GIF | 91.6 FPS, CPU 1.94 ms | 91.6 FPS, CPU 3.67 ms | -47 % CPU |
| HelloRust: Shaders | 91.6 FPS, CPU 1.60 ms | 91.6 FPS, CPU 2.75 ms | -42 % CPU |

OpenGL ES stays available as the fallback and as an option (`Ui::gpu_backend`).

## For AI agents

[`skills/drawnui-rust/SKILL.md`](skills/drawnui-rust/SKILL.md) teaches an agent to build apps with
DrawnUI for Rust: adding drawnui, the browser build, the web page, Android, icons, cache types,
accessibility. It is also served at https://hellorust.drawnui.net/skills/drawnui-rust/SKILL.md, with
`llms.txt` / `llms-full.txt` at the site root. Pair it with the DrawnUI framework skill from
https://drawnui.net/llms.txt.

## Building this repository

```
cargo test -p drawnui -p hellorust                   # headless tests, no window
cargo run --release -p hellorust                     # the demo
cd examples/bench && cargo run --release -- 20000 g0  # 20,000 draw calls
cd examples/bench && cargo run --release -- 2000 mesh # 2,000 animated SkMesh waves
dev/build.ps1 -Example hellorust -WebOnly            # the browser build into target/web/hellorust
```

The workspace builds against three checkouts next to this repository:

- `../rust-skia`: [DrawnUi/rust-skia](https://github.com/DrawnUi/rust-skia), branch
  `drawnui-crates` (`git clone -b drawnui-crates https://github.com/DrawnUi/rust-skia.git`). Skia
  downloads prebuilt, as it does for apps.
- `../winit` and `../android-activity`: upstream plus one fix each; the commands are in
  [`dev/winit-android`](dev/winit-android/README.md) and
  [`dev/android-activity-null-text`](dev/android-activity-null-text/README.md).

To work on Skia itself, `dev/skia-binaries.ps1` builds it from source once per target and exports
the archive every later build unpacks. Our notes on each platform, for anyone bringing a Rust + Skia
library to it: [`solved-win.md`](solved-win.md), [`solved-wasm.md`](solved-wasm.md),
[`solved-mac.md`](solved-mac.md), [`solved-ios.md`](solved-ios.md),
[`solved-android.md`](solved-android.md), [`solved-linux.md`](solved-linux.md).

## Repository layout

- `drawnui/`: the engine crate; `drawnui/web/drawnui_host.js` is the browser host.
- `examples/hello/`: a counter. `examples/hellorust/`: the DrawnUI Hello app.
  `examples/bench/`: the benchmark scene. `examples/images/`: a photo feed. `examples/dungeon/`:
  Dungeon Run.
- `templates/app/`: the starter app.
- `dev/`: build, release and platform scripts.

## License

MIT, see [`LICENSE`](LICENSE).
