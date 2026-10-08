---
name: drawnui-rust
description: Building apps with DrawnUI for Rust (the `drawnui` crate, Skia via skia-safe) for Windows, Linux, Android and the browser — adding drawnui to a project (crates.io, Skia downloaded prebuilt with no setup, the starter template), the browser (wasm32-unknown-emscripten) build setup every app must copy into its own .cargo/config.toml, above all the 1 MB wasm stack and its overflow check, how to recognize and prove a wasm stack overflow, the web host page, the Linux build (clang 16+, X11 / Wayland, icon, clipboard), and the Android app (GameActivity, Gradle, 16 KB pages, soft keyboard, TalkBack). Load before building or debugging any Rust app that uses drawnui, especially its web or Android build.
---

# DrawnUI for Rust: building an app

Load `drawnui` too for engine semantics (layouts, caching, gestures): the Rust engine keeps the
same public contract as DrawnUI for .NET and React.

## Add drawnui to a project

The repository's `templates/app` is a complete starter (every file below, an icon, a web build
script): copy it and rename `myapp`. By hand:

`Cargo.toml`: drawnui from crates.io. It brings Skia as `drawnui-skia-safe` (rust-skia's skia-safe
0.153.3 plus SkMesh) and `drawnui-skia-bindings`; the app uses it as `drawnui::skia`. Never add
skia-safe or skia-bindings to the app: a program can link one Skia only. The desktop and the
browser need no `[patch]`.

```toml
[dependencies]
drawnui = "0.1.0-preview.4"

[profile.release]
lto = "thin"
panic = "abort"
```

**Skia needs no setup.** drawnui-skia-bindings downloads it prebuilt for the target from
https://github.com/DrawnUi/rust-skia/releases: Windows x64 and ARM64, macOS (Apple silicon, Intel),
Linux x64 and ARM64, iOS (devices, simulator on Apple silicon and Intel), Android arm64 / armv7 /
x86_64 / x86, the browser. `SKIA_BINARIES_URL` only
points the download at a mirror (`file://` works, absolute paths only).

- **Never set `FORCE_SKIA_BINARIES_DOWNLOAD`.** Inside a crate it makes skia-bindings key the
  download by a local git hash it does not have; it then compiles Skia from source instead.
- The archives hold drawnui's default features (`svg`, with `textlayout`). Other features give
  another key: no archive, Skia is compiled from source (see "Skia from source" below).
- A `file://` URL works too (an offline mirror of the release assets; absolute paths only).
- `[patch]` sections only act in the top-level project: a dependency's patches never reach yours.
  Android needs two patched crates until their fixes are upstream (see the Android section).
- SkMesh (`drawnui::skia::Mesh`, `MeshSpecification`, `Canvas::draw_mesh`): custom vertex /
  fragment SkSL drawn as triangles, in every app through drawnui-skia-safe.
- On Linux, rustc 1.94.1 can crash while printing the warnings of some dependencies (a compiler
  bug); `RUSTFLAGS=-Awarnings` avoids it.

## Browser build (wasm32-unknown-emscripten)

skia-safe links only for `wasm32-unknown-emscripten` (not `wasm32-unknown-unknown`, so no
wasm-bindgen / web-sys / winit on the web). Needs `rustup target add wasm32-unknown-emscripten`
and emsdk 6 with its tools on PATH in the build shell (`emsdk_env`); Skia comes from the release.

**Link flags live in YOUR app's `.cargo/config.toml`.** Cargo reads config from the folder you build
in and its parents, never from dependencies: the flags the drawnui repo uses for its own examples
do not reach your app. Copy them:

```toml
[target.wasm32-unknown-emscripten]
linker = "em++"
rustflags = [
    "-C", "link-arg=-sMAX_WEBGL_VERSION=2",
    "-C", "link-arg=-sMODULARIZE=1",
    "-C", "link-arg=-sEXPORT_NAME=createDrawnUi",
    "-C", "link-arg=-sEXPORTED_RUNTIME_METHODS=GL,HEAPU8,UTF8ToString",
    "-C", "link-arg=-sALLOW_MEMORY_GROWTH=1",
    # 1 MB main-thread stack (emscripten's default is 64 KB) + the stack cookie check.
    "-C", "link-arg=-sSTACK_SIZE=1048576",
    "-C", "link-arg=-sSTACK_OVERFLOW_CHECK=1",
    "-C", "link-arg=-sERROR_ON_UNDEFINED_SYMBOLS=0",
]
```

### The wasm stack: never drop these two flags

- Emscripten's default stack is **64 KB** and an overflow is **not checked** by default: the stack
  silently overwrites the memory below it. DrawnUI's layout and paint recurse down the control
  tree, so a deep page overflows 64 KB (about 90 KB measured on a demo page).
- Symptoms in the browser console, with the desktop build of the same app fine:
  `memory allocation of N bytes failed`, `memory access out of bounds`, `Aborted()`,
  `RuntimeError: unreachable`, often inside a font or image delivery; errors that move, a page
  that works alone fails after another page was opened (the memory is already damaged).
- Prove it before any other theory: link once with `-sSTACK_OVERFLOW_CHECK=2` (checks every stack
  pointer move; slow, diagnosis only). An overflow then aborts at once with
  `stack overflow (Attempt to set SP to ..., with stack limits [...])`.
- Fix: `-sSTACK_SIZE=1048576` (1 MB, the Windows main thread) and keep `-sSTACK_OVERFLOW_CHECK=1`
  (cheap cookie check: a future overflow aborts loudly instead of corrupting memory). If a page
  ever needs more, raise `STACK_SIZE` or make the deep walk iterative; never remove the check.

### Skia from source (features without a release archive)

Only when no release archive matches (other features, another target). rust-skia's own prebuilt wasm
Skia uses the old exception ABI and does not link with Rust 1.93 or newer (rust-skia issue 1287), so
the browser build needs its Skia compiled with wasm exceptions:

- `FORCE_SKIA_BUILD=1`; Skia and C/C++ code compiled with `-fwasm-exceptions`
  (`SKIA_GN_ARGS` `extra_cflags_c` / `extra_cflags_cc`, `CFLAGS_/CXXFLAGS_wasm32_unknown_emscripten`).
- emsdk 6 on Windows ships `emcc.exe` / `em++.exe` / `emar.exe`, no `.bat` files: name the `.exe`
  tools in `SKIA_GN_ARGS` (`cc`, `cxx`, `ar`, plus `skia_emsdk_dir=""`) and in
  `CC_/CXX_/AR_wasm32_unknown_emscripten`.
- Needs LLVM, Python and Ninja; Skia's build calls `python3` (where that is only the Microsoft Store
  stub, put a `python3.bat` with `@python %*` first on PATH). Keep the target dir on a short path
  on Windows: Skia's ninja build fails on long ones.
- First build about 6 minutes. Any change of rustflags / link args compiles Skia again.
- Set these variables for that build only and remove them after (`FORCE_SKIA_BUILD` is checked for
  existence: an empty value still forces a source build).
- rust-skia passes cargo's `OPT_LEVEL` to clang on non-Windows targets: build with `--release`, or
  put `[profile.dev.package.skia-bindings] opt-level = 3` in the workspace `Cargo.toml`, or Skia is
  compiled at -O0.

### Web page

Serve the built `<app>.js` + `<app>.wasm`, the crate's `web/drawnui_host.js` (from the drawnui
version you build with: the page script and the wasm speak one protocol; `cargo metadata` gives the
crate's folder) and your assets over http (fonts and images are fetched as bytes; there are no system fonts on the web):

```html
<canvas id="canvas"></canvas>
<script src="myapp.js"></script>
<script src="drawnui_host.js"></script>
<script>
  DrawnUi.start({ canvas: document.getElementById("canvas"), create: createDrawnUi });
</script>
```

**Version stamps on publish.** Browsers may keep `.js` files cached for hours while the page and
the wasm are fetched fresh: new wasm with old glue / host JavaScript breaks. Give each code file an
address that changes with its content: in the emscripten glue replace `"<app>.wasm"` (its one
`locateFile("<app>.wasm")`) with `"<app>.wasm?v=<hash of the wasm>"`, then in the page
`src="<app>.js?v=<hash of the edited glue>"` and `src="drawnui_host.js?v=<hash>"`. Assets: write
`<script>window.duiAssetVersions = {"assets/images/a.jpg": "<hash>", ...};</script>` into the page
before the host script; the host asks for each asset in the map as `assets/...?v=<hash>` (an
existing query gets `&v=`, a `#fragment` stays last). Unchanged files keep their address and stay
cached.

`create` is the `EXPORT_NAME` from the link flags. `DrawnUi.start` resolves to `{ module, app,
requestFrame, snapshot }`: `snapshot(type = "image/png", quality)` gives a Promise<Blob> of the
canvas (a frame drawn and read in the same task: no `preserveDrawingBuffer`). Pass
`history: false` to keep the app out of the browser's history and URL hash (a page in an iframe).
`create` can wrap the module options: `create: () => createDrawnUi({ onAbort, print, printErr })`
to show panics and engine warnings. Canvas CSS: `width: 100vw; height: 100vh; height: 100dvh`
(dvh: the height Safari's toolbars leave free; with 100vh alone the bottom of the canvas sits under
them on an iPhone), `display: block`,
`touch-action: none`. Load large fallback fonts (emoji) through the font fallback so they never
delay the first frame (see Emoji and symbols).

## Windows build

`cargo run --release` from the app folder. Skia comes from the release archive. A source build
(other features) on Visual Studio 18 (2026) needs a rust-skia with VS 18 detection; without it
bindgen fails with `'cassert' file not found`.

## Linux build

The desktop host of Windows: winit (X11 and Wayland) + glutin, OpenGL. Packages (Ubuntu names):
`build-essential pkg-config ninja-build python3 libfontconfig1-dev libfreetype6-dev libx11-dev
libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev
libgl1-mesa-dev libegl1-mesa-dev`.

- **Skia from source needs clang 16 or newer** (C++20 parenthesized aggregate init in
  `SkPDFTag.cpp`; clang 14 / 15 fail with "no matching member function for call to 'emplace'").
  Ubuntu 22.04 ships up to 15: take clang from apt.llvm.org and point `LIBCLANG_PATH` at its lib
  folder for bindgen. With plain skia-safe Skia comes prebuilt.
- **Window icon:** Linux executables carry no icon resource. The host reads `icon.ico` next to the
  exe (or in the working folder) and sets it on the window (X11). Copy it there in `build.rs` when
  `CARGO_CFG_TARGET_OS` is `linux`. Wayland desktops show the icon of the app's `.desktop` entry.
- **Clipboard:** X11 selections; on Wayland desktops XWayland keeps them in step.
- **Screen readers:** the same accessibility snapshot over AT-SPI (Orca), through AccessKit.
- **WSL 2:** build in the Linux filesystem, not on `/mnt/c` (slow, and WSL's clock can run ahead of
  Windows' so cargo misses fresh edits; build from a copy synced by content). In WSLg run the app on
  X11 (`WAYLAND_DISPLAY=` unset): WSLg's own Wayland compositor (Weston 9, RDP) has crashed on a
  winit window with client-side decorations. GL in WSLg may be software (llvmpipe).

## Android build (GameActivity)

The app is a `cdylib` whose entry hands the activity to drawnui, then runs the same app code as the
desktop:

```rust
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    my_app::run();
}
```

- **GameActivity, built with Gradle.** drawnui uses winit's GameActivity backend: it gives the soft
  keyboard a real text field (GameTextInput: composition, every script) and TalkBack a view to read
  (AccessKit). NativeActivity has neither. GameActivity is an AppCompatActivity, so the APK is a
  Gradle app:

  ```gradle
  dependencies {
      implementation 'androidx.games:games-activity:4.4.0'
      implementation 'androidx.appcompat:appcompat:1.7.0'
      implementation 'androidx.core:core:1.13.0'
      // AppCompat's Kotlin parts, else "Duplicate class kotlin..." at checkDuplicateClasses.
      implementation platform('org.jetbrains.kotlin:kotlin-bom:1.8.22')
  }
  ```

  Manifest: the activity `com.google.androidgamesdk.GameActivity` with
  `<meta-data android:name="android.app.lib_name" android:value="<your lib, no lib prefix>" />`,
  theme `@style/Theme.AppCompat.NoActionBar`, a `configChanges` list (orientation, screenSize,
  keyboard, density, uiMode...) so a rotation does not recreate it, and
  `android:enableOnBackInvokedCallback="false"` on the application: Back then reaches the app as a
  key (the shell goes back; at the root the app goes to the background), where predictive back
  would close it. Do not enable GameActivity's prefab / C++ glue: android-activity has its own.
- **Native libraries and assets.** Build with cargo-ndk into the Gradle `jniLibs` (one folder per
  ABI); the app's files go into the APK as `assets/assets/...` (the host reads `assets/...` through
  the AssetManager). Clear the libraries folder before a build: a library left from another ABI
  build ships old code.
- **16 KB pages.** Android 15+ devices and the newer emulator images refuse libraries aligned for
  4 KB pages. In your `.cargo/config.toml`:
  `[target.'cfg(target_os = "android")'] rustflags = ["-C", "link-arg=-Wl,-z,max-page-size=16384"]`.
  Gradle (AGP 8.5.1 or newer) aligns the uncompressed libraries in the APK; check with
  `zipalign -c -P 16 -v 4 app.apk`.
- **ABIs:** arm64-v8a, armeabi-v7a, x86_64 and x86, all with prebuilt Skia (`vulkan`). Link the
  app with NDK 26 (Skia's libc++ comes from it; NDK 29 does not compile Skia's ICU).
- **One crate for desktop and Android:** keep the app a library with `run()` plus `android_main`
  and a thin `main.rs`; `cargo ndk -t arm64-v8a -t x86_64 --platform 26 -o <jniLibs> rustc --lib
  --release --crate-type cdylib` builds the activity's library without a `cdylib` crate type in
  `Cargo.toml` (which would collide with the browser build's output names).
- **android-activity 0.6.1 aborts on the first editor focus** in builds with debug assertions:
  reading GameActivity's text state before it has one gives `slice::from_raw_parts` a null
  pointer ("unsafe precondition(s) violated: slice::from_raw_parts requires the pointer to be
  aligned and non-null"); release builds have the same undefined behavior silently. Until a
  release carries the null check, use a patched android-activity (`[patch.crates-io]`; the
  drawnui repository has the patch in `dev/android-activity-null-text`).
- **winit 0.30 on Android does not survive a relaunched activity.** Android destroys the activity
  and starts a new one in the same process for changes `configChanges` cannot take (navigation
  mode, wallpaper colors) and for those it leaves out. winit ignores `Destroy`, so
  `android_main` never returns, GameActivity's `onDestroy` waits for it on the Java main thread,
  and the app stays black; winit also refuses a second event loop in one process, and it drops
  insets changes. Until winit handles them, use a patched winit (`[patch.crates-io]`) that exits
  the loop on `Destroy`, allows a new loop once the old one is dropped and reports insets
  changes as a same-size `Resized` (the drawnui repository has it in `dev/winit-android`); call
  `drawnui::set_android_app` in every `android_main`.
- **GPU API.** Android draws with Vulkan by default (drawnui turns on skia-safe's `vulkan`
  feature there: a Skia built from source for Android needs it too), OpenGL ES where Vulkan cannot
  be made. `Ui::gpu_backend(GpuBackend::OpenGl)` asks for OpenGL ES; on a device `adb shell
  setprop debug.drawnui.gpu gl` (or `vulkan`) overrides it at the next start, for comparisons.
  Measured on a Mali-G57 phone: both hold 90 Hz on ordinary screens, Vulkan with about half the
  CPU per frame and 25 to 35 % more frames on heavy scenes (20,000 draw calls, 2,000 meshes).
- **Safe area (every platform with one):** `Ui::mobile_fullscreen(bool)`, DrawnUI's
  `MobileIsFullscreen`. Off (default): the root is laid out inside the safe area and the strips
  under the system bars show the canvas background. On: the root covers the bars; `SkiaShell` keeps
  its bars, pages and overlays out of them, your own content uses `Cx::content_insets()`.
  `Cx::safe_insets()` is the platform's safe area in both modes, `Ui::on_safe_insets_changed` fires
  when it changes. Do not add the insets yourself when not fullscreen (the root is already inside).
- **What the host does for you:** frames from Choreographer, animations stepped with the vsync time
  the frame shows; edge-to-edge insets reported (see Safe area); Back; the soft keyboard for a focused
  editor (typed text, Backspace, Enter; a composition arrives when committed); TalkBack (explore by
  touch, swipes, double tap) from the same accessibility snapshot as the desktop screen readers.
- **Testing on an emulator:** `adb shell input` events are injected past TalkBack (a tap clicks
  straight through); to drive TalkBack send touches through the emulator console
  (`adb emu event send EV_ABS:ABS_MT_TRACKING_ID:<new id> EV_ABS:ABS_MT_POSITION_X:<0..32767> ...
  EV_KEY:BTN_TOUCH:1 EV_SYN:0:0`). `adb shell uiautomator dump` lists the tree TalkBack reads.
  Judge frame pacing in the device (logcat, SurfaceFlinger), not by eye on the emulator window:
  the host monitor's refresh rate beats against the emulator's 60 Hz.

## App icon (every app ships one)

Desktop: embed `icon.ico` (multi-size, 16 to 256 px) as icon resource 1; the drawnui desktop host
puts the exe's icon resource 1 on the title bar and the taskbar by itself, sized for the window's
scale. Next to `Cargo.toml`: `app.rc` with `1 ICON "icon.ico"`, `[build-dependencies]
embed-resource = "3"`, and `build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=icon.ico");
    // A build script runs on the build machine: embed-resource looks at that one, not at the
    // target. Without this check the wasm build links a Windows .lib and fails
    // ("wasm-ld: error: unknown file type: ...app.lib").
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
```
 Web: a `favicon.ico` next to `index.html` and
`<link rel="icon" href="favicon.ico" />`.

## Assets next to the exe

The desktop host reads a relative path (`assets/...`) next to the exe first, then from the
working folder. Started from Explorer or a shortcut, the working folder is not the app's folder:
copy the app's `assets` next to the exe in `build.rs` (OUT_DIR is
`target/<profile>/build/<package>-<hash>/out`, so `Path::new(&out_dir).ancestors().nth(3)` is the
exe's folder; skip it when `CARGO_CFG_TARGET_OS` is `emscripten`, and emit
`cargo:rerun-if-changed=assets`). A shipped app carries the same folder beside its exe.

## Emoji and symbols

A label draws a character only from its own fonts. A plain `SkiaLabel` with just the font
registered by `Ui::font` draws a missing-glyph box (tofu) for every emoji, on every platform.
There are two sources of glyphs, and they can be used together.

**The system emoji font** (desktop and mobile; there are no system fonts on the web):

```rust
SkiaLabel::new("💤 ✋ 🔊").system_font_fallback(true)
```

A character that none of the label's fonts has is drawn with a system font that has it.
`SkiaRichLabel` and `SkiaEditor` (`use_unicode`) turn this on by themselves.
- Windows: Segoe UI Emoji, in color, for characters that look like emoji by default (💤 ✋ 🔊 🔔 💬 😀).
  Symbols that look like text by default (⚙ ❤ ☀) come out in one color, even when followed by U+FE0F.
  Their color version needs the U+FE0F sequence, and labels are not shaped (see below).
- Linux (fontconfig), macOS, iOS and Android: the platform's emoji font through Skia's font
  manager. Not measured yet.

**A font you ship** (every platform, and the only way on the web):

```rust
Ui::new(state, build)
    .font("Default", "assets/OpenSans-Regular.ttf")
    .font_fallback("FontEmoji", "assets/emoji.ttf") // loaded after the first frame
// on the label:
SkiaLabel::new("⚙ 4m").font_family_fallback("FontEmoji")
```

- `font_family_fallback` is a comma-separated list of aliases, tried in order after the label's own
  font. `SkiaButton` has it too, but has no system fallback.
- Register large fonts with `Ui::font_fallback`: the first frame does not wait for them, and the
  texts are measured again when they arrive.
- The Noto Color Emoji COLRv1 build draws in color on Windows (measured).
- Ship a subset with only the characters the app shows. The full emoji font is several MB.
  Make the subset with fontTools:
  `pyftsubset Noto-COLRv1.ttf --unicodes=U+2699,U+1F4A4,... --output-file=emoji.ttf`
- List every character you need. A character missing from the subset is a box again, unless the
  system fallback is also on.

**Both together:**

```rust
.font_family_fallback("FontEmoji").system_font_fallback(true)
```

Your font comes first, the system font fills the rest. This is the way to get a colored ⚙ on
Windows: your font has the color gear, and Segoe UI Emoji has only the plain one.

**No text shaping.** Labels draw one glyph per code point. So:
- A joined sequence (👨‍👩‍👧) draws as its parts.
- A skin tone (👍🏽) draws as the hand plus a color swatch.
- A flag draws as its two regional letters.
- U+FE0F (VS16) does not switch a symbol to its emoji look.
- Invisible code points (U+FE0F, the zero width joiner, tags) that no font of the label has draw
  nothing and take no room. They never draw a box, and `fallback_character` leaves them alone.

## Cache types

`use_cache`: `Operations`, `OperationsFull` (records the canvas clip, for a control that paints
outside its rect), `Image` (an offscreen surface on the GPU; `GPU` is the same), `ImageComposite`
(`ImageCompositeGPU` the same), `ImageDoubleBuffered`. On the desktop ImageDoubleBuffered bitmaps
are made by worker threads in CPU memory: the last bitmap shows while the next one is made, the very
first frame shows a placeholder (the background color or a faint gray); caches on the GPU inside it
are painted live into its bitmap, post-render effects and backdrops are left out. In the browser it
is made in the frame, as Image. A lost GPU / WebGL context is handled by the engine and its hosts:
caches are made again, nothing to do in the app.

## Gestures in the browser

`Ui::gestures(GesturesMode::Lock)` (DrawnUI `Canvas.Gestures`) makes the canvas own every touch:
no page scroll, rubber band, iPhone Safari pull-down or text selection starts on it, while taps
still reach the soft keyboard and the accessibility overlay. Use it for full-screen apps and games.
`Enabled` (the default) leaves the page what the app does not use (the wheel) and the page's CSS
decides touch panning. Native hosts ignore it.

## Screen readers: name every control

Screen readers (Narrator, VoiceOver, TalkBack, Orca, browsers) read the controls' roles, names and
values. A switch, checkbox, slider or progress bar has no text of its own: give each one an
`.accessibility_label("Wi-Fi")` that says what it controls, never its role ("Switch" is read twice).
A slider's or progress bar's value goes out as a value (number, range, step) and VoiceOver / TalkBack
adjust a slider by its step. A custom range control implements `Control::accessibility_value` (and
`accessibility_set_value` when it can be set). Every SkiaScroll pages for VoiceOver's three-finger
swipe (iOS) and TalkBack's scroll actions; a screen reader moving onto a node off screen scrolls it
into view. Give a container a role (`Aria::LIST`, `Aria::TOOLBAR`...) when its items form a group:
arrow keys then move inside it and it is one Tab stop.

## Rendering mode (GPU or CPU)

`Ui::rendering_mode(RenderingModeType)`, DrawnUI's `Canvas.RenderingMode`, read once before the
first frame. `Accelerated` (the default) draws on the GPU: WebGL2, OpenGL or Metal. `Default` draws
every frame on the CPU, caches are CPU bitmaps; the desktop and mobile hosts then put the frame on
the window, the browser through a 2D canvas context. A browser that refuses WebGL2 draws on the CPU
whatever is set (a console warning says so). CPU frames cost much more at large sizes (in the
browser about 50 ms for a 1968 x 1174 frame while scrolling): keep Accelerated unless the GPU is the
problem.

## No console window in release

A Rust exe is a console program by default: started outside a terminal, Windows opens a console
window next to the app. The library cannot change that; the app's `main.rs` can:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

Release builds are then a plain Windows app; debug builds keep the console for logs and panic
messages. A release build no longer shows anything printed (frame-timing lines, warnings, panic
text): keep the console in tools that report through it (benchmarks).
