---
name: drawnui-rust
description: Building apps with DrawnUI for Rust (the `drawnui` crate, Skia via skia-safe) for Windows, Linux, Android and the browser — adding drawnui to a project (crates.io, Skia downloaded prebuilt with no setup, the starter template), the browser (wasm32-unknown-emscripten) build setup every app must copy into its own .cargo/config.toml, above all the 1 MB wasm stack and its overflow check, how to recognize and prove a wasm stack overflow, the web host page, the Linux build (clang 16+, X11 / Wayland, icon, clipboard), the Android app (GameActivity, Gradle, 16 KB pages, soft keyboard, TalkBack), transforms in 2D and 3D (rotation_x / rotation_y with perspective, translation_z depth, a card's back side, cached content and taps under 3D), and popup windows (context menus and dropdowns that open past the app window's edge). Load before building or debugging any Rust app that uses drawnui, especially its web or Android build, before turning, flipping or scaling controls, and before opening menus or popups.
---

# DrawnUI for Rust: building an app

Load `drawnui` too for engine semantics (layouts, caching, gestures): the Rust engine keeps the
same public contract as DrawnUI for .NET and React. Porting an existing DrawnUI .NET app or control
(C# or XAML) to Rust: load `drawnui-port-net-to-rust` as well, for the workflow and the C# to Rust
mapping.

## Add drawnui to a project

The repository's `templates/app` is a complete starter (every file below, an icon, a web build
script): copy it and rename `myapp`. By hand:

`Cargo.toml`: drawnui from crates.io. It brings Skia as `drawnui-skia-safe` (rust-skia's skia-safe
0.153 plus SkMesh) and `drawnui-skia-bindings`; the app uses it as `drawnui::skia`. Never add
skia-safe or skia-bindings to the app: a program can link one Skia only. The desktop and the
browser need no `[patch]`.

`cargo add drawnui` takes the newest preview; by hand:

```toml
[dependencies]
# The newest 0.1.0 preview; `cargo update` moves a project to a newer one.
drawnui = "0.1.0-preview"

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
Pass `resolveAsset: (url) => string | undefined` to serve the app's files from the page. It is asked
for every file the engine loads (images, GIFs, sprites, fonts, SVG, Lottie, shaders) with the url the
app gave, such as `assets/logo.png`. Return a string to load from there; a `blob:` url made with
`URL.createObjectURL` works, the image worker included. Return `undefined` to load the url as usual.
Pass `fonts: [{ alias, url, weight }]` to add fonts from the page. Each is registered as if the app
had called `Ui::font`, and the first frame waits for it. A page font never becomes the default font:
only labels that name it with `.font_family("alias")` use it. Its url goes through `resolveAsset`.
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

Runs started by scripts or test tools while someone works at the machine: set
`DRAWNUI_NO_ACTIVATE=1` in the app's environment. The window then opens behind the one in use,
without taking the keyboard or flashing in the taskbar, and draws as usual. Without it an app
takes the foreground when it opens, as any app does.

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

## Transforms, 2D and 3D

Applied at render around the arranged box: layout never changes, and the control's own cache stays
valid while it moves or turns (it is not recorded again). The names are DrawnUI's in snake case.

2D:
- `translation_x` / `translation_y` (points), `rotation` (degrees, clockwise), `scale_x` /
  `scale_y` (`scale` sets both; `scale_x(-1)` mirrors), `skew_x` / `skew_y` (degrees), `opacity`.
- `anchor_x` / `anchor_y` set the pivot as a fraction of the box (0.5, 0.5 = center; 0, 0 = top left).
- Taps land on the drawn control, not on its layout box (the inverse of its render matrix).
- One-shot animations: `cx.translate_to`, `cx.rotate_to`, `cx.scale_to`, `cx.fade_to`; each stops its
  own previous run; chain with `cx.on_finished`. Any other value:
  `cx.start_animator(id, ValueAnimator::new(from, to, ms, easing), |v, cx| ...)`, or
  `cx.animate(id, ms, easing, |t, cx| ...)` for a 0..1 progress.

3D (perspective built in, no shader or matrix of your own; measured headless at scale 1):
- `rotation_y` turns around the vertical axis (a card flip, a door). A 200 x 200 square at 50 draws
  131 wide, the near edge taller than the far one (230 vs 177 px). Positive brings the left edge
  toward the viewer.
- `rotation_x` turns around the horizontal axis: at 40 the square draws 155 tall, the bottom edge
  nearer (225 wide vs 180 at the top). `rotation` stays the flat turn; `rotation_z` turns flat the
  same way, after the 3D turns. They combine: `rotation_x(16).rotation_y(20).rotation(-6)` is a card
  lying on a table.
- `anchor_x` / `anchor_y` are the hinge: with `anchor_x(0)` a `rotation_y` turn keeps the left edge
  in place (a door).
- Past 90 a control shows its content mirrored, as its back: at `rotation_y(180)` a bar drawn at the
  left shows at the right. For a real back side, show a second face while
  `cos(rotation_x) * cos(rotation_y) < 0` and mirror it (`scale_x(-1)` for a Y turn, `scale_y(-1)`
  for an X turn) so it reads right.
- `translation_z` is depth: positive moves the control toward the viewer and draws it bigger,
  negative farther and smaller, turned or flat. Scale = 576 / (576 - z) (576 px = the camera
  distance): -300 = 0.66x, +150 = 1.35x, +300 = 2.09x.
- Cached content under a turn, a scale or a skew is drawn smooth: a cache bitmap is blitted with
  linear sampling and mipmaps once anything distorts it (its own transform, an ancestor's, or the
  picture it is recorded into), with an antialiased border; only moved, it is copied pixel for pixel.
  A picture cache (`Operations`, the default of SkiaShape and SkiaLabel) holding cache bitmaps is
  recorded again once when it starts or stops being distorted, not on every frame. A cached bitmap
  drawn bigger (`translation_z` toward the viewer, a scale up) looks soft: it holds the pixels of its
  flat size.
- Gestures go through the 3D matrix: a horizontal `SkiaScroll` inside a tilted card scrolls with a
  sideways swipe, and a tap picks the item under the finger.
- `Perspective1` / `Perspective2` of DrawnUI are not there.

## Data from other threads (Waker)

The tree lives on the UI thread; never touch it from another thread. A thread that produces data
for a control (camera frames, a sensor, a download) keeps the data in its own shared slot (a
`Mutex` or an atomic) and calls `waker.invalidate(id, Dirty::DRAW)`; the control takes the data
in `paint` (or in `on_props_changed` for `Dirty::APPLY`). Get the `Waker` from `Cx::waker()` or
`Ui::waker()`; it is `Send + Sync + Clone`. The marks are applied at the start of the next frame,
a removed control is skipped, and a burst of calls before that frame wakes the loop once. Call it
once per new piece of data: every call is a frame, and nothing is drawn while no data comes.
In the browser there is no other thread: the page script that hands data to the app (an exported
function) calls the `requestFrame` that `DrawnUi.start` returns, after `invalidate`.

## Textures from outside Skia (camera, video)

A control draws a texture made elsewhere through `PaintCx::gpu()`, in its `paint`. It answers
`None` on the CPU and while the control is recorded for a worker thread (inside an
ImageDoubleBuffered cache: no GPU texture may go there), so keep such a control out of cached
parents and give it no cache. `Gpu::backend()` says which import applies: `import_gl_texture`
(OpenGL, OpenGL ES, WebGL2; `GlTarget::ExternalOes` for Android camera frames),
`import_metal_texture` (macOS, iOS), `import_vulkan_image` with `vulkan_handles()` (Android). The
imports are `unsafe`: the image borrows the texture, so keep the texture alive and unchanged until
`completed_serial()` reaches the `frame_serial()` of the frame that drew it (asking for the serial
is what makes that frame report; frames finish in order). After own GL calls while painting, call
`reset_gl_state()`. A new GPU context (`epoch()` changed) means importing again. On Android with
Vulkan, `vulkan_handles().hardware_buffers` says whether AHardwareBuffers can be imported; Skia
does not sample YCbCr images yet, so a camera frame in the device's own YUV format goes through
OpenGL ES (`GlTarget::ExternalOes`, `Ui::gpu_backend(GpuBackend::OpenGl)`) or an RGB buffer.

In the browser a page script makes the texture (a camera's `<video>` uploaded with `texImage2D`):
`DrawnUi.start(...)` returns `{ module, requestFrame }`; `module.GL.currentContext.GLctx` is the
WebGL2 context, `const id = module.GL.getNewId(module.GL.textures); module.GL.textures[id] = tex;`
gives the id `import_gl_texture` takes, and the app's own `#[unsafe(no_mangle)] pub extern "C" fn`
functions are `module._name`. The script calls one of them with the id (or "a new frame is
there"), then `requestFrame()`. In `paint`, call `reset_gl_state()` before drawing it: the script
changed GL state behind Skia. After a lost WebGL context (`epoch()` changed) make the texture again.

The script puts back exactly the GL state it changed (read it with `getParameter` first):
emscripten keeps its own copy of some of it, the PIXEL_UNPACK_BUFFER binding and the unpack
alignment, and Skia's later uploads go through emscripten, so a binding left behind makes them read
from the wrong buffer. `reset_gl_state()` resets only Skia's own cache.

A second WebGL context (a recorder at another size) works in the same module:
`const h = module.GL.registerContext(gl2, { majorVersion: 2 })`, then `module.GL.makeContextCurrent(h)`
around the calls into it; Skia's `gpu::gl::Interface::new_native()` and `gpu::direct_contexts::make_gl`
use whichever context is current. Save `module.GL.currentContext.handle` first and make it current
again before returning; drop that context's Skia objects while it is current. To encode its frames
with WebCodecs, make the frame from the canvas itself (`new VideoFrame(offscreenCanvas, { timestamp })`):
Chrome refuses one made from `transferToImageBitmap()` when the WebGL context has `alpha: false`.

## A subtree at its own scale (recording overlays)

A control can lay one child out at another size and scale than the screen's, e.g. an overlay
burned into recorded video: `cx.measure_child_scaled(child, w, h, scale)` in `measure` and
`cx.arrange_child_scaled(child, rect, scale)` in `arrange`, the same way every time (a control
has one layout). In `paint`, `cx.record_child(child, bounds)` gives a `Picture` of it (no GPU
textures inside, safe to replay on another thread or GPU context); record it again only when
`cx.child_base(child).content_epoch()` moved. To preview it, draw that picture scaled into the
control's rect. Such a child is off screen: it is never painted on the screen, takes no gestures
and is left out of accessibility.

## Device rotation (camera stills, recording)

How the device is held, whatever the screen shows (an app locked to portrait still knows):
`cx.device_rotation()` is 0, 90, 180 or 270 degrees clockwise from the device's natural position
(90 = left side up, DrawnUI's LandscapeRight; 270 = LandscapeLeft), snapped at 45 degrees; a device
lying flat counts as 0. Mobile hosts read the motion sensor only while a mounted control listens:
`Build::listen_device_rotation()` (or `Cx::listen_device_rotation(id, on)`), and that control's
`Control::on_device_rotation(cx, degrees)` runs once per new value. Turn a photo by the value read
when it was taken; read it when a recording starts and keep it. The desktop and the browser always
answer 0.

## Gestures in the browser

`Ui::gestures(GesturesMode::Lock)` (DrawnUI `Canvas.Gestures`) makes the canvas own every touch:
no page scroll, rubber band, iPhone Safari pull-down or text selection starts on it, while taps
still reach the soft keyboard and the accessibility overlay. Use it for full-screen apps and games.
`Enabled` (the default) leaves the page what the app does not use (the wheel) and the page's CSS
decides touch panning. Native hosts ignore it.

## Pinch and turn (two fingers)

The first finger drives the pointer as always. While a second one is down, every Panning carries
`gesture.manipulation` (DrawnUI's `Manipulation`): `scale` is the fingers' spread now over the
previous move minus 1 (zoom by multiplying `1.0 + scale`), `rotation` the degrees turned since the
previous move, `scale_total` / `rotation_total` their sums since the finger count changed,
`center` the middle of the fingers in canvas pixels, `touches_count`. `gesture.touches` counts the
fingers on every press gesture. A second finger makes the press no tap and no long press. Take the
Down (`Handled::Yes`) to own the press, then read `manipulation` on its Panning. Android, iOS and
the browser send the other fingers; in the browser use `GesturesMode::Lock` so the page does not
zoom instead. Tests: `Headless::pinch(center, from, to, duration_ms, steps)`.

## Popup windows (context menus, dropdowns)

`cx.open_popup_window(content, anchor, options)` shows any controls over everything and returns a
`PopupId`. On the desktop (Windows, macOS, Linux on X11) the popup is a window of its own, so it can
open past the edges of a small app window, the way native menus do: frameless, always on top, no
taskbar or Dock entry, on the monitor where it opens, at that monitor's scale. On Windows it never
takes the activation, so the app keeps the keyboard. In the browser, on Android and iOS, and on
Wayland it is drawn on the canvas and kept inside it. The code is the same everywhere.

```rust
// A right-click menu where the click was.
ui.on_context_menu(|app: &mut App, menu, cx| {
    app.popup = Some(cx.open_popup_window(my_menu(), menu.location, PopupWindowOptions::default()));
    true
})
// A dropdown below its button: anchor it to the control.
SkiaButton::new("Fruit").assign(&mut app.fruit).on_tapped(|_me, app: &mut App, cx| {
    app.popup = Some(cx.open_popup_window(fruit_list(), app.fruit, PopupWindowOptions::default()));
})
// An item picks and closes.
.on_tapped(|_me, app: &mut App, cx| {
    if let Some(popup) = app.popup.take() {
        cx.close_popup_window(popup);
    }
})
```

- The anchor is a point (canvas points, e.g. `menu.location`), a `Rect`, or a control (a handle or a
  `ControlId`). A point opens the popup with its top-left corner there. A rect or a control opens it
  below, left edges together. Where the screen ends, it opens on the other side.
- Size: what the content measures to, or `options.size` in points. A popup taller than the screen is
  cut to the screen, and its content is measured again inside it, so a scroll in it scrolls.
- The window is transparent where nothing is drawn: give the panel its own background and round corners
  if you like. Exception: Linux X11, where the window is opaque and the corners outside a round panel
  show dark.
- It closes on a press outside it (that press reaches nothing, as with a native menu; a right click
  outside still asks for a context menu, so the next menu opens there), on Escape, when the app loses
  the keyboard to another app, and on `cx.close_popup_window(id)` / `cx.close_popup_windows()`.
  With `PopupWindowOptions { close_on_pointer_leave: true, .. }` it also closes once the mouse has been
  outside both the popup and the app's window for 300 ms. `cx.is_popup_window_open(id)` says if it is
  still open.
- Its content is part of the app's tree: the same state, fonts and handlers. Taps, hover, the wheel and
  screen readers work as in the window, and Tab stays inside the top popup. Give menu items
  `accessibility_role(Aria::MENU_ITEM)`.
- An open popup that does not change costs no frame time: it is drawn again only when its content
  changes.
- `SkiaShell`'s `open_popup` is a different thing: content centered over a dimmed page, inside the app.
- Tests: `Headless::popup_windows(true)` plays a desktop host. Then `popup_window_requests()`,
  `place_popup_window(id, width, height, scale, offset)`, `draw_popup_window(id, w, h)`,
  `popup_tap(id, x, y)` and `popup_hover(id, at)` drive the popup's window. Without that call, popups
  are on the canvas, as in the browser.

## Switching input off

`responds_to_gestures(false)` is the one switch that turns a control off for the user: a button,
switch, checkbox, radio, slider, scroll, drawer or carousel. A tap on a switched-off button, toggle or
radio stops there and does nothing (no pressed look, no `on_tapped`, `on_down` or `on_up`); moves, the
wheel and hover pass on, so a scroll under it still scrolls. Keyboard and screen readers see it
disabled (no Tab stop, reported unavailable), and a change at run time reaches them. A scroll, drawer
or carousel that is off only stops moving itself: its content still takes taps. SkiaButton's old
`is_disabled(true)` / `set_is_disabled(true)` still work but are deprecated; code that read the
button's `p.is_disabled` field reads `p.responds_to_gestures` (its opposite) since 0.1.0-preview.9.

## Touch on iPhone: a slow drag may stall, then jump

On iOS a slow drag can stop for a few seconds and then jump to where the finger is: the system holds
the touch's moves while it decides whether it is a system swipe (home, Control Center, back). Native
apps see the same; it is not an engine bug. Do not smooth or drop the first move that arrives after
the stall: it is where the finger really is.

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
