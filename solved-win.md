# Solved: Rust + Skia on Windows

Problems DrawnUi.Rust hit in the Windows desktop build (skia-safe, Ganesh GL, winit + glutin) and
how they were fixed. Short, for anyone porting a Rust + Skia library to Windows. Add every new one.

1. **Skia from source only when you must.** Plain skia-safe downloads prebuilt Windows binaries.
   A patched rust-skia (we need SkMesh, not upstream yet) is wired in through `[patch.crates-io]`
   and forces a Skia source build: about 3.5 min per target dir. You need LLVM (libclang),
   Python and Ninja.

2. **Build Skia once, unpack it everywhere.** With several target dirs and parallel agents, every
   one compiled Skia; we once had 120 clang-cl processes on the machine. Fix: `dev/skia-binaries.ps1`
   builds once with `FORCE_SKIA_BUILD=1` + `BUILD_ARTIFACTSTAGINGDIRECTORY` and tars the export as
   `skia-binaries-<key>.tar.gz` (key = rust-skia commit + target + features).
   `.cargo/config.toml` `[env]` sets `SKIA_BINARIES_URL=file://C:/.../skia-binaries-{key}.tar.gz`
   (absolute `file://` URL, rust-skia reads no other form) and `FORCE_SKIA_BINARIES_DOWNLOAD=1`.
   A fresh target dir builds in about 30 s with zero clang. After a rust-skia commit or a feature
   change, run the script again, or builds panic with "Downloading of binaries was forced but
   failed". Never leave build processes running that nobody uses.

3. **Visual Studio 18 (2026) is not found.** rust-skia did not know the "18" install folder, and
   clang 20 does not resolve the VS 18 toolset by itself: bindgen fails with `'cassert' file not
   found`. Fixed in our rust-skia fork (`skia-bindings/build_support/platform/windows.rs`): the
   folder is known, and the bindgen step sets `VCToolsInstallDir` to the newest toolset under
   `VC\Tools\MSVC` when it is unset. No developer prompt needed.

4. **Short paths.** Skia's ninja build fails on long target paths ("The filename or extension is
   too long"). Keep target dirs short, e.g. `target\a\<name>`.

5. **`FORCE_SKIA_BUILD` must be removed, not emptied.** The build script only checks that it
   exists. A script that sets it for one build (the wasm one) restores the environment by
   removing the variable, or the next desktop build in that shell compiles Skia again.

6. **GL setup (glutin).** Take the GL config with the fewest samples (Skia anti-aliases itself);
   fall back to a GLES context when desktop GL fails; vsync through `set_swap_interval(Wait(1))`.
   Drop order matters: everything holding GPU objects drops before the GL context, the window
   last; call `abandon()` on the Skia context on exit (crash on exit with some AMD drivers,
   rust-skia #1235).

7. **Frames on demand.** The event loop waits (`ControlFlow::Wait`) and wakes for timers with
   `ControlFlow::WaitUntil`; nothing renders while idle. Images decode on worker threads and
   arrive as user events.

8. **Input details winit leaves to you.** Clipboard text through Win32 (open, empty, moveable
   global block, close). Context menus open on right-button release, like the browser
   `contextmenu`. AltGr arrives as Ctrl + Alt and still types characters. A window that loses
   focus with a button or key down never gets the release: clear the state on focus loss.
   Touchpad pixel scrolling: 100 points per notch, times the scale factor, as browsers do. IME
   commits (Chinese, Japanese, the emoji panel) come as `Ime::Commit`.

9. **Assets are read relative to the working folder.** Run an example from its own folder:
   `cd examples\hellorust; cargo run --release`.

10. **No icon by default.** Cargo embeds no icon in the exe, and winit registers its window class
    without one: Explorer, the title bar and the taskbar all show the generic icon. Fix: each app's
    `build.rs` embeds `icon.ico` as icon resource 1 (`embed-resource`, `app.rc` = `1 ICON
    "icon.ico"`, only when `CARGO_CFG_TARGET_OS` is windows: see solved-wasm.md 8), and the desktop host loads resource 1 with
    `Icon::from_resource` at 16 and 32 points times the scale factor, onto `set_window_icon` and
    `set_taskbar_icon`.

11. **A console window opens next to the app.** A Rust exe is a console program by default.
    `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in the app's `main.rs`:
    release builds are a plain Windows app, debug builds keep the console. Tools that report through
    the console (bench, images) keep it in release too.

12. **Started from Explorer, the app finds no assets.** Assets were read relative to the working
    folder, which is the exe's folder (or a shortcut's) when the app is not started with
    `cargo run` from its own folder: system font instead of the app's, no SVG, no photos (it
    looked like the SVGs vanished). Fix: the desktop host reads a relative path next to the exe
    first, then from the working folder, as .NET reads from the app's base folder; each app's
    `build.rs` copies its `assets` next to the exe (`OUT_DIR` 3 levels up = `target/<profile>`,
    skipped for the web build).

13. **A GPU reset (driver update, TDR) loses the GL context.** Create the context with
    `Robustness::RobustLoseContextOnReset` (glutin; plain and GLES contexts as fallbacks), check
    `DirectContext::abandoned()` after each frame, and on loss make a new GL context on the same
    window surface, a new Skia context and window surface; the engine drops what lived on the old
    context (`Gpu::epoch`). Abandon the lost Skia context with `abandon()` (no GL calls).

14. **ImageDoubleBuffered off the frame thread.** Skia's GPU context is not thread-safe, so the
    workers draw pictures into CPU bitmaps; the frame records the picture and keeps every GPU
    texture out of it (caches on the GPU inside are painted live into it). skia-safe's `Picture`
    and `Image` are `Send`; a winit `EventLoopProxy` brings the bitmaps back and wakes the loop.

15. **The window lagged behind the mouse while resized.** Each size step could wait 0.8 to 2.6 s
    (measured: 9 to 19 such stalls in 60 programmatic resize steps; OpenTK, the same machine: none).
    Our frames took 1-3 ms; the time was the GPU fence after SwapBuffers: a frame presented with
    work still running on the GPU made the next resize wait on DWM (Intel Arc, Windows 11). Fix:
    for 500 ms after a size change, wait for the GPU (`DirectContext::submit(SyncCpu::Yes)`) before
    SwapBuffers; other frames keep the GPU running behind. 0 stalls in 4 x 80 steps after.
16. **Where skia-bindings looks for prebuilt Skia depends on where it comes from.** From crates.io
    it keys the download by the commit in its `.cargo_vcs_info.json`, and only when
    `FORCE_SKIA_BINARIES_DOWNLOAD` is NOT set (forced, it asks `git rev-parse` in the registry
    folder, finds nothing and compiles Skia). From a git dependency (drawnui's DrawnUi/rust-skia)
    it downloads only when forced, keyed by the checkout's commit. drawnui is on crates.io with its
    Skia crates (drawnui-skia-safe / drawnui-skia-bindings): apps set only `SKIA_BINARIES_URL`; this
    repo builds them from a path checkout and keeps the force flag; the commit is the same, so the
    archives serve both.
    A git dependency also fetches every submodule: the fork's branch has no Skia submodule (a
    source build downloads Skia over HTTP when `skia-bindings/skia` is empty).

Open, not solved yet: frame interval p99 of 25 to 33 ms at 20,000 draw calls on Ganesh GL
(phase 0 bench).

17. **Windows on ARM (aarch64-pc-windows-msvc), cross-built on x64.** Two traps. clang-cl resolves
    Skia's relative source paths with `GetFullPathNameA`, limited to 260 characters; the target
    triple adds a folder level to the build dir and Skia's deepest zlib sources pass the limit
    ("The filename or extension is too long"): build in a short folder at the drive root. Then the
    bindings' C++ (the cc crate) looks for the x64-hosted ARM64 `cl.exe`, which a Visual Studio
    with only the ARM64 libraries does not have: point `CC_aarch64_pc_windows_msvc` /
    `CXX_aarch64_pc_windows_msvc` at LLVM's clang-cl, which built Skia already.
    `dev\skia-binaries.ps1 -Target aarch64-pc-windows-msvc` does both.
