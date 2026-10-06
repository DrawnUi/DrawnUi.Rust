# Solved: Rust + Skia on Android

Problems DrawnUi.Rust hit on Android (skia-safe Ganesh on OpenGL ES, winit 0.30 with its
NativeActivity, later GameActivity, glutin over EGL, the APK first packed by hand, then by Gradle)
and how they were fixed. Short, for
anyone porting a Rust + Skia library to Android. Add every new one.

1. **Skia's build calls `python3`.** On Windows `python3` is often only the Microsoft Store stub:
   ninja stopped at `link libcpu-features.a` with "Python was not found". Put a `python3.bat`
   forwarding to `python` first on PATH for the Skia build (`dev\skia-binaries.ps1 -Android`).

2. **NDK 29 does not compile Skia's ICU.** `umapfile.cpp: use of undeclared identifier
   'posix_madvise'`. NDK 26 builds it (5 minutes per ABI). Build Skia and link the app with the
   same NDK: its libc++ comes with the Skia static libraries.

3. **No prebuilt Skia for a forked rust-skia.** The rust-skia binaries are keyed by commit; a fork
   (SkMesh here) builds Skia once per ABI and exports it as an archive
   (`BUILD_ARTIFACTSTAGINGDIRECTORY`), which `SKIA_BINARIES_URL` + `FORCE_SKIA_BINARIES_DOWNLOAD`
   then hand to every build. A target dir whose skia-bindings build is already done exports
   nothing: delete it to export again.

4. **16 KB pages.** The API 37 emulator image (`ps16k`) and Android 15+ phones with 16 KB pages
   refuse a library linked with 4 KB alignment, which NDK 26 does by default. Link with
   `-Wl,-z,max-page-size=16384` (`.cargo\config.toml`, every Android target) and `zipalign -P 16`.

5. **Edge to edge: the content rect is the whole window.** With targetSdk 35+ the activity draws
   under the status and navigation bars, and NativeActivity's content rect (`content_rect()`)
   covers them. The insets come from the decor view through JNI:
   `getWindow().getDecorView().getRootWindowInsets().getInsets(Type.systemBars() |
   Type.displayCutout())` (API 30+; the system window insets before). Read them at start, after a
   resize and after a resume, not every frame; until the view is attached they are null.

6. **Back closes the app.** With targetSdk 33+ the system sends Back (button and gesture) to the
   predictive-back callback, whose default finishes the activity: winit never sees it.
   `android:enableOnBackInvokedCallback="false"` makes it a KEYCODE_BACK key (winit:
   `NamedKey::BrowserBack`); the shell goes back. winit reports every key handled, so at the app's
   root the host itself calls `activity.moveTaskToBack(true)`, as Back does since Android 12.

7. **The window is made on `resumed` and its surface lost on `suspended`.** The native window
   exists only while the activity is in the foreground: make the window and the GL context on
   the first `resumed`, drop the EGL surface on `suspended` (no GPU work then) and make a new one
   on the next `resumed`; the context and Skia's GPU context stay.

8. **Files of the app are APK assets.** There is no folder next to an exe: read `assets/...`
   through the NDK AssetManager (`AndroidApp::asset_manager()`), also from worker threads.

9. **stdout and stderr go nowhere.** Panics and host lines are written to logcat through
   `__android_log_write` (tag `drawnui`); a panic hook does it for panics.

10. **Packing without Gradle** (until GameActivity, entry 15; Gradle packs the APK since). `aapt2 compile` the icons, `aapt2 link` the manifest, assets and
    `android.jar`, add `lib/<abi>/*.so`, `zipalign -P 16`, `apksigner` with the debug keystore
    (`dev\android.ps1`). Appending the libraries to aapt2's zip in place left headers the
    installer warned about ("header mismatch"): write a new zip with every entry, keeping
    resources.arsc stored as targetSdk 30+ requires.

11. **PowerShell splats a string character by character.** A one-item array from an `if`
    expression is unwrapped to a string; `cargo ndk ... @release` then passed "-", "-", "r"...
    ("unexpected argument '-'"). Type it `[string[]]`. And `$profile` is PowerShell's own variable.

12. **x86 (32-bit) does not compile skia-bindings.** The bindings in the i686-linux-android
    archive assert struct sizes rustc does not agree with (`100 - 104 would overflow`): a layout
    mismatch between bindgen's view of the target and rustc's. Skia itself builds; the app does
    not. 32-bit x86 Android devices are practically gone: the APK carries arm64, armv7 and x86_64.

13. **Frames from Choreographer: the callback must reach winit as a user event.** Paced by
    eglSwapBuffers alone, with the CPU time at frame start as the animation clock, Pong's ball
    stepped 11-23 ms per frame on a 60 Hz screen (jitter). The host asks for
    `AChoreographer_postFrameCallback64` (dlsym, API 29+; `AChoreographer_postFrameCallback`
    before, whose `long` time is 32 bits on armv7) on android_main's thread, which has a looper,
    and steps animations with the vsync time plus one period (CLOCK_MONOTONIC, Choreographer's
    clock). Waking the looper from the callback is not enough: winit 0.30 drops a wake that brings
    no redraw request or user event, so the frame never came. The callback sends an
    `EventLoopProxy` user event; its handler asks for the next callback BEFORE drawing while frames
    are wanted (C# keeps its callback chain; asked for after the draw, a frame ending late lost a
    vsync), then requests the redraw. That handler must not ask for a frame itself, or frames never
    stop at rest. Emulator, Pong: animation step 16.67 ms every frame (before: p5 11-13, p95 19-23),
    frame interval p95 18 ms (before 19-23); flings: SurfaceFlinger present intervals as before
    (over 25 ms: 0-0.8% both); no frames at rest or in the background.

14. **`FontMgr::new()` parses the system font list on every call.** On Android Skia's platform
    font manager reads and parses /system/etc/fonts.xml each time it is made: 14-32 ms per call on
    the emulator (first call 19-59 ms). Made per font file and per SVG parse, it cost about 9 calls
    on the main thread at startup and one more for every SVG. The engine keeps one manager
    (`fonts::font_mgr()`, a thread-local; handing it out is a reference-count bump, about 2 us).

15. **NativeActivity gives the soft keyboard no text field.** The input method connects with
    inputType 0: Gboard shows a floating bar (voice, backspace, enter) or, on a phone, letters
    sent as key events only (no composition, no other scripts). A text field needs Java code:
    GameActivity (androidx.games:games-activity 4.4.0) has one (GameTextInput) and is what
    android-activity and AccessKit support. It is an AppCompatActivity, so the APK is built with
    Gradle (AppCompat, core; theme Theme.AppCompat.NoActionBar; activity
    `com.google.androidgamesdk.GameActivity` with the `android.app.lib_name` meta-data).
    winit 0.30 drops GameActivity's text events: the host reads `AndroidApp::text_input_state()`
    after every event while an editor has the keyboard.

16. **AppCompat duplicates Kotlin classes.** `checkReleaseDuplicateClasses`: kotlin-stdlib 1.8
    and the old kotlin-stdlib-jdk7 / jdk8 1.6 parts it pulls in define the same classes. Align
    them with `implementation platform('org.jetbrains.kotlin:kotlin-bom:1.8.22')`.

17. **android-activity 0.6.1 slices a null pointer.** Before GameTextInput has a state, its text
    pointer is null, and `text_input_state()` passes it to `slice::from_raw_parts` (undefined
    behavior; with debug assertions: "unsafe precondition(s) violated: slice::from_raw_parts
    requires the pointer to be aligned and non-null", abort). Fixed by a null check in a sibling
    checkout of android-activity (branch `drawnui-null-text`, `[patch.crates-io]`) until it is
    upstream.

18. **Every text state given to GameTextInput restarts the input method.** Its Java `setState`
    ends with `restartInput()`. Resetting the field after each key restarted the keyboard on
    every key. The field starts as a few zero-width spaces with the caret after them and is
    compared with what it held before: deleted characters go to the editor as Backspace, new ones
    as typed text; it is reset only when the zero-width spaces run low, it grows long, or the
    caret leaves its end. Zero-width spaces, not spaces: the keyboard's own rules (two spaces
    make ". ") would act on spaces.

19. **TalkBack: AccessKit's winit adapter needs GameActivity.** It injects into GameActivity's
    surface view (`mSurfaceView`); with NativeActivity winit marks hover events handled, so
    explore by touch never reaches a view. The adapter is made in the host's `resumed`: the
    launch wrapper that makes the host on the first resume must also pass that resume on, or no
    adapter exists and TalkBack sees an empty SurfaceView.

20. **Testing the keyboard and TalkBack on the emulator.** `adb shell input` events are injected
    past TalkBack (a tap clicks, swipes do not move its focus, key shortcuts do nothing), and
    `sendevent` is denied on Play images. `adb emu event send` writes to the virtual touchscreen
    and goes through TalkBack like a finger: per touch a new `ABS_MT_TRACKING_ID`,
    `ABS_MT_POSITION_X/Y` in 0..32767, `BTN_TOUCH`, then `EV_SYN:0:0` (the console has no alias
    for SYN_REPORT). `uiautomator dump` shows the tree TalkBack reads (in Git Bash set
    `MSYS_NO_PATHCONV=1`, or `/sdcard/...` becomes a Windows path). With the emulator's hardware
    keyboard, Gboard shows a floating bar; its menu has "Show on-screen keyboard".

21. **A relaunched activity stayed black.** Android destroys the activity and starts a new one in
    the same process for every configuration change the manifest's `configChanges` does not take:
    the navigation mode (an overlay, which no `configChanges` value covers), Android 12+ wallpaper
    colors, the locale, the font scale, "Don't keep activities". winit 0.30 only logged the
    `Destroy` ("TODO: forward onDestroy notification"), so `android_main` never returned, and
    GameActivity's `onDestroy` waits on the Java main thread for exactly that: the new activity
    never started. A second `android_main` could not have built an event loop anyway (winit allows
    one per process), and the host kept the old activity in a `OnceLock`. Fixed with a patched
    winit (exit the loop on `Destroy`, allow a new loop once the old one is dropped), the
    activity replaced for every `android_main`, the Choreographer proxy kept per thread (frame
    callbacks run on the thread that posted them). Every activity starts the app again (the state
    of the old one is not kept).

22. **The new activity read the insets of the old configuration.** Read right after the window
    came, the decor view still had the previous navigation mode's insets (48 pt with gesture
    navigation, 24 pt with three buttons); the right ones came with GameActivity's
    `WindowInsetsChanged` 50 to 300 ms later, which winit 0.30 also dropped ("TODO: handle
    Android InsetsChanged"). The patched winit reports it as a `Resized` of the same size (iOS
    reports a safe-area change as `Resized` too); the host reads the insets again and keeps its
    surface when the size did not change (the keyboard's insets come the same way).

23. **Vulkan without waiting for the GPU.** skia-safe's `vulkan` feature needs a Skia archive
    built with it, per ABI (`dev\skia-binaries.ps1 -Android <triple>`). rust-skia's vulkan-window
    example submits with `SyncCpu::Yes`, the CPU waiting for the GPU on every frame. The host
    (ash) instead hands Skia the swapchain image's acquire semaphore (`Surface::wait`) and has the
    flush signal a render semaphore (`FlushInfo` signal semaphores, `flush_surface_with_access`
    with `Present`, which also moves the image to the presentable layout); the present waits on it.
    Each swapchain image is wrapped once, Skia follows its layout. The images are made with
    TRANSFER_SRC where allowed: SkiaBackdrop copies from the window. FIFO present; Choreographer
    still asks for the frames. A resize of the same size (the keyboard's insets) no longer counts
    as a resize: no 500 ms of waiting for the GPU.

24. **Vulkan vs OpenGL ES on a phone** (Blackview BV8800, Mali-G57 MC2, Vulkan 1.1, 90 Hz panel
    = 10.91 ms; release build, the host's stats per 300 frames). HelloRust pages (Shaders, Lottie
    & GIF, fling series on Recycled and Uneven cells): both hold every vsync (interval p95 11.3 to
    11.9 ms), CPU per frame about half on Vulkan (cells p50 1.0 vs 1.9 to 3.7 ms, Lottie 1.95 vs
    3.8). The bench scene: 2,000 animated shapes both 91.6 FPS, CPU p50 6.0 vs 7.6 ms; 20,000 draw
    calls (G0) 39 to 40 FPS vs 32 (25 vs 31 ms frames); 2,000 SkMesh waves 57 FPS vs 42 to 43 (17.5
    vs 23 to 24 ms). Vulkan is the default on Android.

25. **Measuring on a device.** `adb shell setprop debug.drawnui.gpu gl` (or `vulkan`) picks the
    API at the next start; `debug.drawnui.bench 2000,mesh` (`20000,g0`, `2000`; `off`) makes the
    HelloRust APK run the bench scene. `uiautomator dump` shows the app's nodes only once
    accessibility is in use, so open pages with the keyboard: one `input keyevent 61` (Tab) per
    call (several key codes in one call lose some), Enter (66) opens the focused card.
