# DrawnUI for Rust

A UI engine for Rust that draws everything with Skia: layouts, controls you can subclass and
paint, gestures, animations, caching, shaders and accessibility, on the desktop, on phones and in
the browser from one source. Same family, controls and contract as DrawnUI for .NET and DrawnUI
for React.

- Crate: [`drawnui`](https://crates.io/crates/drawnui) · API docs: https://docs.rs/drawnui
- Live demo (the DrawnUI Hello app in the browser): https://hellorust.drawnui.net
- Release notes: [`CHANGELOG.md`](CHANGELOG.md) · Accessibility: [`ACCESSIBILITY.md`](ACCESSIBILITY.md)
  · What is ported from DrawnUI for .NET: [`PARITY.md`](PARITY.md)

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

## Platforms

| Platform | Drawing | State |
|---|---|---|
| Windows | Skia on OpenGL (winit + glutin) | done |
| Browser (`wasm32-unknown-emscripten`) | Skia on WebGL2 | done, live demo |
| macOS | Skia on Metal, paced by the display | done (Apple silicon) |
| iOS | Skia on Metal, 120 Hz on ProMotion screens | done on the simulator and an iPhone |
| Android | Skia on Vulkan, OpenGL ES as the fallback; winit GameActivity | done on emulators and a phone |
| Linux | Skia on OpenGL (X11 and Wayland) | built and run under WSL 2; a desktop with a GPU not checked yet |

## What the engine has

- A control tree written with fluent builders; properties that invalidate only what they change;
  measure / arrange / paint, render transforms, opacity, clipping, z-index.
- Every DrawnUI cache type: Operations, OperationsFull, Image, GPU, ImageDoubleBuffered (bitmaps
  made on background threads on the desktop), ImageComposite. A lost GPU context is recreated.
- Layouts: Absolute, Column, Row, Wrap, Grid, decorated grids; templated lists with recycling and
  virtualisation.
- Controls: shapes, labels and rich text, buttons, an editor with IME, scroll with scroll bars and
  refresh, carousels (also with shader transitions), drawer, shell (pages, tabs, popups, modals,
  toasts), toggles, switch, checkbox, radio buttons, slider, progress, backdrop blur.
- Media: images (decoded off the frame thread), image tiles, SVG, GIF, Lottie, sprites, SkSL shader
  effects and transitions, SkMesh (custom vertex and fragment programs).
- Input: tap, pan, fling, long press, hover, context menu, mouse wheel and touchpad, keyboard and
  focus.
- Accessibility: screen readers on every platform (UI Automation, VoiceOver on macOS and iOS,
  TalkBack, AT-SPI / Orca, an ARIA overlay in the browser), keyboard navigation with a focus ring,
  selectable text, values a screen reader reads and adjusts. See [`ACCESSIBILITY.md`](ACCESSIBILITY.md).
- Animators: value, range, spring, ping-pong, pendulum, ripple, timers.
- About 700 headless tests, many of them ports of the .NET engine's tests with the same numbers.

## Crates

| Crate | What it is | Version |
|---|---|---|
| [`drawnui`](https://crates.io/crates/drawnui) | the engine: controls, layouts, caches, gestures, animations, accessibility, the desktop, mobile and browser hosts | 0.1.0-preview.2 |
| [`drawnui-skia-safe`](https://crates.io/crates/drawnui-skia-safe) | Skia for Rust: rust-skia's skia-safe 0.153.3 plus SkMesh; drawnui re-exports it as `drawnui::skia` | 0.153.4 |
| [`drawnui-skia-bindings`](https://crates.io/crates/drawnui-skia-bindings) | the native Skia and its C bindings under drawnui-skia-safe; downloads prebuilt Skia from [DrawnUi/rust-skia](https://github.com/DrawnUi/rust-skia/releases) | 0.153.4 |

An app depends on `drawnui` only; the two Skia crates come with it. Skia's API is `drawnui::skia`
(the same as `skia_safe`, SkMesh included): never add skia-safe or skia-bindings to an app, a
program can link one Skia only.

## Getting started

**A new app**: copy [`templates/app`](templates/app) (an empty app with an icon, assets, a web page
and build scripts for the desktop, the browser and Android; it takes drawnui from crates.io), rename
`myapp`, build. Or by hand, in a new project (`cargo new myapp`):

`Cargo.toml`:

```toml
[dependencies]
drawnui = "0.1.0-preview.2"

[profile.release]
lto = "thin"
panic = "abort"
```

**Skia needs no setup.** The first build downloads it prebuilt for your target (about 11 to 25 MB)
from [DrawnUi/rust-skia's releases](https://github.com/DrawnUi/rust-skia/releases): Windows x64,
Linux x64, macOS and iOS (Apple silicon, simulator), Android arm64 / armv7 / x86_64 and the browser.
Other targets or features (32-bit x86 Android, Linux / Windows arm64, Intel Macs) compile Skia from
source once: LLVM, Python and Ninja. Do not set `FORCE_SKIA_BINARIES_DOWNLOAD`: inside a crate it
makes skia-bindings compile Skia from source. `SKIA_BINARIES_URL` points the download at a mirror.

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
    # 1 MB stack and the overflow check: keep both (see "The wasm stack" below).
    "-C", "link-arg=-sSTACK_SIZE=1048576",
    "-C", "link-arg=-sSTACK_OVERFLOW_CHECK=1",
    "-C", "link-arg=-sERROR_ON_UNDEFINED_SYMBOLS=0",
]

[target.'cfg(target_os = "android")']
rustflags = ["-C", "link-arg=-Wl,-z,max-page-size=16384"]
```

The web page loads the build and `drawnui_host.js`, which must come from the same drawnui version
(the template's `web.ps1` / `web.sh` copy it from the dependency):

```html
<canvas id="canvas" style="width:100vw;height:100dvh;display:block;touch-action:none"></canvas>
<script src="myapp.js"></script>
<script src="drawnui_host.js"></script>
<script>DrawnUi.start({ canvas: document.getElementById("canvas"), create: createDrawnUi });</script>
```

Per platform:

- **Windows, Linux, macOS**: `cargo run --release`. Assets go next to the exe (the template's
  `build.rs`), `icon.ico` is the window and taskbar icon. Linux needs the X11 / Wayland and GL
  development packages. On Linux, rustc 1.94.1 can crash while printing the warnings of some
  dependencies (a compiler bug); `RUSTFLAGS=-Awarnings` avoids it.
- **Browser**: `rustup target add wasm32-unknown-emscripten`, emsdk 6 with its tools on PATH, then
  the template's `web.ps1` / `web.sh`; serve `dist/` over http.
- **Android**: cargo-ndk, NDK 26; `cargo ndk ... rustc --lib --release --crate-type cdylib` makes
  the activity's library (`android_main` in the template's `src/lib.rs`), packed by Gradle with a
  GameActivity. Until two fixes are released upstream, add the patched winit and android-activity
  ([`dev/winit-android`](dev/winit-android), [`dev/android-activity-null-text`](dev/android-activity-null-text))
  with `[patch.crates-io]` in the app's own `Cargo.toml`.
- **iOS**: [`dev/ios`](dev/ios) builds an `.app` bundle for the simulator or a signed one for a
  device.

### The wasm stack (read before changing the web link flags)

The browser build runs on a 1 MB main-thread stack (`-sSTACK_SIZE=1048576`), with emscripten's
stack cookie check (`-sSTACK_OVERFLOW_CHECK=1`). Keep both.

Emscripten's default stack is 64 KB. Layout and paint walk the control tree recursively, so a deep
page needs more. A wasm stack overflow is not caught by default: the stack runs into the memory
below it and corrupts it silently. What that looks like, with the desktop build of the same app fine:

- `memory allocation of N bytes failed`, `memory access out of bounds`, `Aborted()`,
  `RuntimeError: unreachable` in the browser console, often inside asset delivery or a font load;
- errors that move: a page that works alone fails after another page was opened, because the
  memory was already damaged.

Diagnosis: link once with `-sSTACK_OVERFLOW_CHECK=2` (checks every stack pointer move; slow, for
diagnosis only). An overflow then aborts at once with `stack overflow (Attempt to set SP to ...,
with stack limits [...])`. If a page needs more than 1 MB, raise `STACK_SIZE`, or make the deep walk
iterative; never remove the check.

## Benchmarks

Same draw list, N rounded rectangles per frame, 1609x1163 canvas, Chrome on Windows, WebGL2:

| Draw calls | Rust + Skia in one wasm module | CanvasKit 0.42 driven from JS |
|---|---|---|
| 2,000 | 59.9 FPS, 2.8 ms CPU | 60.0 FPS, 3.7 ms CPU |
| 5,000 | 60.0 FPS, 5.1 ms CPU | 59.8 FPS, 7.4 ms CPU |
| 20,000 | 59.7 FPS, 12.5 ms CPU | 47.8 FPS, 20.6 ms CPU |

Windows desktop (OpenGL): 20,000 draw calls at 60 FPS, 10 to 12 ms CPU. macOS (MacBook Air M1,
Metal): 20,000 draw calls at a steady 60 FPS, 10.3 ms CPU; 2,000 SkMesh waves 6.1 ms CPU.

SkMesh waves (a triangle strip each, a vertex program with a time uniform, one draw call each),
Windows desktop, 1250x875: 200 waves 60 FPS at 1.1 ms CPU; 2,000 waves 60 FPS at 6.4 ms CPU.

Payload of the bench (it links only the Skia code it uses):

| Build | raw | brotli |
|---|---|---|
| `gl` | 3.93 MB | 1.15 MB |
| `gl` + text layout (ICU, HarfBuzz) + SVG | 7.39 MB | 2.27 MB |
| CanvasKit 0.42 wasm, for scale | 7.32 MB | 2.28 MB |

### Android: Vulkan vs OpenGL ES

A phone with a Mali-G57 MC2 (Vulkan 1.1), 1080x2408, a 90 Hz panel. The same release build, the API
switched with `adb shell setprop debug.drawnui.gpu gl|vulkan`; frame stats from logcat.

**On heavy scenes Vulkan draws 21 to 35 % more frames; where both keep up with the 90 Hz panel,
Vulkan needs 26 to 59 % less CPU per frame.**

| Scene | Vulkan | OpenGL ES | Vulkan better by |
|---|---|---|---|
| 20,000 draw calls | 40.2 FPS, 24.8 ms a frame | 33.3 FPS, 30.0 ms | +21 % FPS |
| 2,000 SkMesh waves | 57.2 FPS, 17.5 ms | 42.4 FPS, 23.6 ms | +35 % FPS |
| 2,000 animated shapes | 91.6 FPS, CPU 5.4 ms | 91.6 FPS, CPU 7.3 ms | -26 % CPU |
| HelloRust: Recycled cells, flings | 91.6 FPS, CPU 1.45 ms | 91.6 FPS, CPU 2.71 ms | -46 % CPU |
| HelloRust: Uneven cells, flings | 91.6 FPS, CPU 0.94 ms | 91.6 FPS, CPU 2.32 ms | -59 % CPU |
| HelloRust: Lottie & GIF | 91.6 FPS, CPU 1.94 ms | 91.6 FPS, CPU 3.67 ms | -47 % CPU |
| HelloRust: Shaders | 91.6 FPS, CPU 1.60 ms | 91.6 FPS, CPU 2.75 ms | -42 % CPU |

Vulkan is the default on Android; OpenGL ES stays as the fallback and an option
(`Ui::gpu_backend`).

## Building this repository

```
cargo test -p drawnui -p hellorust                   # headless tests, no window
cargo run --release -p hellorust                     # the demo
cd examples/bench && cargo run --release -- 20000 g0  # 20,000 draw calls
cd examples/bench && cargo run --release -- 2000 mesh # 2,000 animated SkMesh waves
dev/build.ps1 -Example hellorust -WebOnly            # the browser build into target/web/hellorust
```

The workspace builds against three checkouts next to this repository; cargo needs all three, also
where Android is not built:

- `../rust-skia`: [DrawnUi/rust-skia](https://github.com/DrawnUi/rust-skia), branch
  `drawnui-crates` (`git clone -b drawnui-crates https://github.com/DrawnUi/rust-skia.git`). At a
  released commit the build downloads Skia prebuilt, as it does for apps.
- `../winit` and `../android-activity`: upstream with one fix each, until they are released; the
  commands are in [`dev/winit-android`](dev/winit-android/README.md) and
  [`dev/android-activity-null-text`](dev/android-activity-null-text/README.md).

After a change in rust-skia, Skia is compiled from source once per target and exported as a
binaries archive that every later build unpacks (`dev/skia-binaries.ps1`; `-Web` for the browser,
`-Android <triple>` for Android). LLVM, Python and Ninja are needed for that; on Windows keep the
target directory on a short path, Skia's build fails on long ones.

Every platform problem we hit and its fix, for anyone porting a Rust + Skia library:
[`solved-win.md`](solved-win.md), [`solved-wasm.md`](solved-wasm.md),
[`solved-mac.md`](solved-mac.md), [`solved-ios.md`](solved-ios.md),
[`solved-android.md`](solved-android.md), [`solved-linux.md`](solved-linux.md).

## For AI agents

[`skills/drawnui-rust/SKILL.md`](skills/drawnui-rust/SKILL.md) teaches an agent to build apps with
this crate (adding drawnui, the browser build and its wasm stack, the web host page, Android, icons,
cache types, accessibility). It is also served at
https://hellorust.drawnui.net/skills/drawnui-rust/SKILL.md, with `llms.txt` / `llms-full.txt` at
the site root. Pair it with the DrawnUI framework skill from https://drawnui.net/llms.txt.

## Repository layout

- `drawnui/` the engine crate; `drawnui/web/drawnui_host.js` the browser host.
- `examples/hello/` a counter; `examples/hellorust/` the DrawnUI Hello app (20 pages);
  `examples/bench/` the benchmark scene; `examples/images/` a photo feed; `examples/dungeon/`
  Dungeon Run, a first-person runner drawn with SkMesh.
- `templates/app/` the starter app.
- `dev/` build, release and platform scripts.

## License

MIT, see [`LICENSE`](LICENSE).
