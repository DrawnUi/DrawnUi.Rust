# Solved: Rust + Skia on macOS

Problems DrawnUi.Rust hit in the macOS desktop build (skia-safe, Ganesh on Metal, winit + a CAMetalLayer,
Apple Silicon) and how they were fixed. Short, for anyone porting a Rust + Skia library to the Mac.
Add every new one.

1. **NaN in SkSL: ARM `fmax` propagates it, x86 `maxps` does not.** The ripple shader
   (`ripples.sksl`) called `normalize(combinedDisplacement)` on a zero vector when no ripple was
   running, so `fresnelEffect` was NaN, and then `reflectionFactor = max(NaN, 1.5)`. On x86 the CPU
   backend's SSE `maxps` returns the second operand, 1.5, so the CPU raster tests passed on Windows.
   On Apple Silicon NEON `fmax` returns NaN, the final `mix` is NaN and the idle effect renders black
   (test `effects_shader::a_tap_starts_a_ripple_that_ends_by_itself` failed on the M1 only). Only the
   x86 CPU got away with it: GPUs on x86 drew it empty too (DrawnUi.Net's HelloWpf, accelerated,
   Windows: empty with the old shader, the photo with the new one; drawnui-smart, a2b09f94). Fix
   (6b985c8): normalize only when `length > 0`, otherwise use 1. Rule: never let a NaN reach `max`,
   `min` or `clamp` in a shader, and guard `normalize` and divisions against zero.

2. **The dev profile builds Skia at `-O0` off Windows.** rust-skia's build script passes cargo's
   `OPT_LEVEL` to Skia as `-O{n}` on every non-Windows target (on Windows it skips `-O`, so the
   same command gives an optimized Skia there). Our archive was built with `cargo build -p drawnui`
   (dev, OPT_LEVEL 0), so the Mac got `-O0` Skia: 3-15x slower than Windows, and the profile
   showed trivial `skvx` helpers as real calls. The archive key records no opt level, so nothing
   flagged it. Fix (77221cc): `[profile.dev.package.skia-bindings] opt-level = 3` in the root
   `Cargo.toml`, so any profile builds `-O3` Skia. Check `extra_cflags` in
   `out/skia/args.gn` after a source build. When you rebuild, replace the old tarball, because
   the key does not change.

3. **OpenGL on macOS does not pace to the display.** glutin's `set_swap_interval(Wait(1))`
   works (`kCGLCPSwapInterval` reads back 1) and the swap blocks, but in AppKit's
   `-[NSCGLSurface synchronize]` (`NSWaitUntilHostTime`) on its own deadline: about 2x the
   refresh rate (5 / 13.4 ms intervals, 120 fps on a 60 Hz panel), so every other frame is never
   shown. Fix: the macOS host draws with Metal. A CAMetalLayer with `displaySyncEnabled` (the
   default) holds 16.67 ms with no display link, and blocking in `nextDrawable` is the pacing.
   Don't use CVDisplayLink (deprecated in macOS 15). An occluded window must not draw at all,
   because `nextDrawable` can block for up to 1 s.

4. **Retina pointer moves are small: decide a drag's direction only past a threshold.** The
   first move of a mouse drag at scale 2 can be about 1 physical pixel. The carousel measured that
   move, found it under 2 points along its axis, and locked the whole drag as "wrong direction",
   so click-and-drag never moved it while Prev/Next did. Fix (c6b22e3): under 2 points on both
   axes the direction is still unknown, and the next move decides.

5. **A fast trackpad swipe sends a wheel event before every frame.** Our wheel glide restarted
   on each event of half a notch or more, and a restarted glide took its start time from the next
   frame (progress 0), so during a fast swipe the page stood still, then jumped to the end. Fix
   (dfe36fe): the glide counts from its event's time. Frame timing on Metal ruled out drawing:
   `app.frame` < 5.2 ms, and the only frames over 20 ms were the vsync wait in `nextDrawable`.

6. **Frames from a display link, as on iOS.** macOS frames came from winit's `request_redraw`
   (AppKit's display cycle) paced by the CAMetalLayer's `nextDrawable`, and
   `drawnui::set_max_fps` (C# `Super.MaxFps`) had no effect. On macOS 14+ the window view's
   `NSView.displayLink` now drives them, as DrawnUI's Super.Mac does: paused while no frame is
   wanted, its `preferredFrameRateRange` the window's screen maximum (it follows the view to
   another screen) or the cap snapped to a divisor of it. Below macOS 14 the old path stays (no
   CVDisplayLink: deprecated). M1 Air, 60 Hz: bench 200 g0 interval p50 16.67 ms; with
   `set_max_fps(30)` 33.33 ms (spread 0.2). 120 Hz needs a ProMotion Mac to be measured.
