# Solved: Rust + Skia on Linux

Problems DrawnUi.Rust hit on Linux (skia-safe Ganesh on OpenGL through glutin, winit 0.30 on
Wayland and X11, AccessKit over AT-SPI), built and run in WSL 2 (Ubuntu 22.04, WSLg) and how they
were fixed. Short, for anyone porting a Rust + Skia library to Linux. Add every new one.

1. **Packages (Ubuntu).** `build-essential pkg-config ninja-build python3 git curl
   libfontconfig1-dev libfreetype6-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
   libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev libgl1-mesa-dev libegl1-mesa-dev`, a clang
   of 16 or newer (entry 2) with its libclang for bindgen (`LIBCLANG_PATH`), `rsync` (entry 3),
   Rust through rustup. For the checks below: `xdotool imagemagick xclip at-spi2-core
   python3-pyatspi dbus-x11 mesa-utils mesa-utils-extra weston`.

2. **Skia needs clang 16 or newer.** With Ubuntu 22.04's clang 14 and 15 Skia stops in
   `src/pdf/SkPDFTag.cpp`: "no matching member function for call to 'emplace'" (C++20
   parenthesized aggregate initialization, clang 16+) and libstdc++'s ranges
   (`views::reverse`, "constraints not satisfied for alias template 'sentinel_t'"). clang 18
   from apt.llvm.org (`llvm.sh 18`, then `update-alternatives` for `clang` / `clang++`) builds it.
   The Android NDK 26 (clang 17) and Windows builds were never affected.

3. **WSL: build on the Linux filesystem, from a copy synced by content.** A cargo target dir on
   the Windows drive (`/mnt/c`) is slow. And WSL's clock ran a minute ahead of Windows' (it syncs
   from the hypervisor and `date -s` / `hwclock -s` do not hold): a file edited on Windows right
   after a build had an older time than the build, cargo skipped it, and a stale binary ran (once
   with "found possibly newer version of crate", once silently). `dev/linux.sh` copies a checkout
   under `/mnt/` to `~/.cache/drawnui-linux/src` with `rsync -rc` (changed files get the current
   time), links the sibling checkouts the workspace patches in, and builds there.

4. **Driving WSL from Windows.** `wsl -- bash -c '...'` passes the command through a second shell
   inside WSL (`$vars` expand there, to nothing); `wsl --exec bash -lc '...'` passes it as is (`-l`
   for `~/.cargo/bin` on PATH). Git Bash rewrites `/tmp/...` arguments into Windows paths: set
   `MSYS_NO_PATHCONV=1`. A GUI app started by a `wsl` call that returns dies with it: keep that
   call running (a background task) while the app is tested.

5. **WSLg's compositor crashes on the window under Wayland.** With `WAYLAND_DISPLAY` set (WSLg's
   default) winit uses Wayland; after the first frame WSLg's Weston 9 (RDP backend) dropped the
   connection ("Io error: Broken pipe", `event loop failed: ExitFailure(1)`) and restarted, and its
   X server stayed gone until `wsl --shutdown`. A plain EGL client (`es2gears_wayland`) runs there,
   and the same app runs on a headless Weston 9 (`weston --backend=headless-backend.so`): the crash
   is WSLg's RDP backend with this window (likely winit's client-side title bar, a subsurface at a
   negative offset). In WSLg run on X11: `WAYLAND_DISPLAY= dev/linux.sh run`.

6. **FreeType rounds hinted advances to whole pixels.** Linux measured "One label, many styles:"
   at 16 px as 172 where Windows (DirectWrite) and CanvasKit measure 171.344, so two label tests
   wrapped differently. Skia's default hinting is Normal; FreeType then returns hinted advances.
   `Font::set_linear_metrics(true)` gives the unhinted advances on every platform (Windows did not
   change) without touching how glyphs are drawn. Android and the browser measure through FreeType
   too.

7. **No exe resources for the window icon.** The host reads the app's `icon.ico` (the file Windows
   embeds) next to the exe or in the working folder, Skia decodes its largest picture, winit sets
   it (X11 `_NET_WM_ICON`; `xprop` shows "Icon (256 x 256): (not shown)"). The example `build.rs`
   copies `icon.ico` next to the exe. Wayland has no window icons: the desktop shows the icon of the
   app's .desktop entry.

8. **Clipboard.** arboard without default features: X11 selections (XWayland keeps them in step
   with the Wayland clipboard on Wayland desktops). One clipboard object for the app's life: X11
   hands copied text out from the app that copied it.

9. **Checking the screen-reader tree.** WSL without systemd has no session bus, so AT-SPI is not
   there. `dbus-run-session -- script` with `/usr/libexec/at-spi-bus-launcher --launch-immediately`,
   `busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status IsEnabled b true`, then
   the app, then `pyatspi` (python3-pyatspi): the tree (roles, names, screen bounds) and
   `queryAction().doAction(0)` (click). pyatspi 2.38 warns "AddAccessible with unknown signature"
   on AccessKit's newer AT-SPI messages; reading still works.

10. **Driving X11 input under WSLg.** `xdotool getwindowgeometry` adds Weston's frame offset twice
    (clicks land 38 / 59 px off); `xdotool mousemove --window <id> x y` is relative to the window.
    Never `click --window` / `type --window`: those are synthetic events (XSendEvent) that winit
    ignores; move, then a plain `click` / `type` / `key` (XTEST). `import -window <id>` captures the
    window. GL in WSLg is llvmpipe (software) here: frame times there mean nothing for real hardware.

11. **The driver may ignore vsync.** The host asks for swap interval 1 and the call succeeds, but
    WSLg's software OpenGL returned from every swap at once: Pong drew a frame every 1.5 ms, WSLg
    showed some of them at its own moments, and the ball stuttered. DrawnUI's OpenTK host never
    trusts the driver in its app mode (vsync off, frames spaced at the monitor's refresh). The
    Rust GL host (Windows, Linux) now judges the first 60 animation frames in a row: a median
    interval under half the refresh period switches on host pacing, frames on a refresh grid
    (the next slot one period after the last slot, so late wake-ups do not add up: 16.89 ms per
    frame from the last start, 16.68 on the grid) and animations stepped by the slot. WSLg Pong:
    frames 16.68 ms apart, animation step 16.68 ms on every frame. Windows with working vsync:
    pacing stays off (bench 16.75-16.81 ms).

12. **Mouse moves arrive in bursts (WSLg): flings went twice as far.** The release velocity was
    each move's distance over the time since the previous move, arrival times (as DrawnUI's
    AppoMobi.Gestures does with `DateTime.Now`). Through WSLg the moves of a drag come in pairs
    0.01 ms apart every 15 ms, then in uneven gaps: the second of a pair measured millions of px/s
    and every fling started at the 3000 pt/s limit. Measured against the previous move at least a
    few ms older, releases came out 40 to 60 % below the hand's speed (the first move of a burst
    carries only part of its distance). What works (checked by replaying logged flings against
    the same motion resampled as even 8 ms moves): moves under 1 ms apart are one position, a
    move's velocity is its displacement over the last 16 ms (the earlier position interpolated),
    and the fling's sample average replaces a sample with the next one of the same burst. Evenly
    spaced moves measure as before.

13. **Hearing Orca without speakers.** Orca 42 (`apt install orca speech-dispatcher
    speech-dispatcher-espeak-ng espeak-ng x11-xkb-utils`; without `xkbcomp` it dies at start)
    writes every sentence it speaks to its debug log: `orca --replace --debug-file=<file>`, the
    `SPEECH OUTPUT:` lines. Run it in the AT-SPI session of entry 9 next to the app (X11 under
    WSLg), drive the app with `xdotool key Tab` / `Return`, read the log. HelloRust: "Recycled
    cells push button. 100 000 items in a SkiaScroll, ..." per card (name, role, description), Enter
    opens the page, "Back push button", "Home push button"; past the last control Tab returns to
    the window ("frame"). Trap: a speech-dispatcher left from an earlier session (its session bus
    gone) makes the next Orca hang at start, and its watchdog kills it ("Killed", the log ends at
    "Launching"), or it starts but speaks nothing: stop speech-dispatcher and remove
    `$XDG_RUNTIME_DIR/speech-dispatcher` between runs.
