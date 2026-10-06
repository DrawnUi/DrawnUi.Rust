# Solved: Rust + Skia on iOS

Problems DrawnUi.Rust hit on iOS (skia-safe Ganesh on Metal, winit 0.30 + a CAMetalLayer on its
UIView) and how they were fixed. Short, for anyone porting a Rust + Skia library to iOS. Add
every new one.

1. **winit makes the UIWindow the size you ask for.** `with_inner_size(1000, 700)`, harmless on the
   desktop, made a 1000 x 700 point UIWindow on the phone: content laid out too wide and cut
   off at the bottom. On iOS ask for no size; the window is the screen.

2. **`inner_size` is the safe area on iOS.** winit's `inner_size()` excludes the status bar, the
   Dynamic Island and the home indicator; `outer_size()` is the whole view. Draw at `outer_size`
   and give the app the difference as insets (`App::safe_insets`, points), computed from
   `inner_position` / `outer_position`, again after every resize (rotation).

3. **A redraw asked for inside a frame is lost.** On iOS winit's `request_redraw` is UIKit's
   `setNeedsDisplay`, and the frame itself runs inside `drawRect`, which drops a request made
   during it. So an animation stopped after one frame: a pan followed the finger (every touch
   asks for a frame), but after the finger lifted there was no fling and no bounce, which looked
   like a lost Up. Fix: a request made while drawing is kept and sent from `about_to_wait`.

4. **Windows only after launch.** UIKit makes windows only once the app has launched: the host
   creates its window and GPU objects in the first `resumed`, not before `run_app`.

5. **The stock simulators can be gone.** A cleaned `~/Library/Developer/CoreSimulator` leaves
   `simctl list` showing devices that fail to boot ("cannot be located on disk").
   `dev/ios/run-sim.sh` creates and reuses its own "DrawnUi iPhone 17 Pro" on the newest runtime.

6. **Return arrives as text, not as Enter.** winit's iOS view implements only UIKeyInput: the
   soft keyboard's Return comes as `insertText("\n")`, a `KeyboardInput` with
   `Key::Character("\n")` and no physical key, so the host named it Unknown and a single-line
   editor never submitted. The host now names it Enter on iOS. (Backspace does arrive as a
   physical Backspace.)

7. **`CADisableMinimumFrameDurationOnPhone` alone does not give 120 Hz.** On an iPhone 16 Pro the
   frame interval stayed at 16.67 ms with the key: frames came from winit's `request_redraw`,
   which on iOS is UIKit's `setNeedsDisplay` / `drawRect`, paced at 60 Hz. Fix, as DrawnUI's
   Super.iOS: a `CADisplayLink` drives the frames (paused while none is wanted), its
   `preferredFrameRateRange` the display's maximum, or `drawnui::set_max_fps` (C# `Super.MaxFps`)
   snapped to a divisor of it. Measured on Pong: interval p50 8.33 ms (FPS 120); with
   `set_max_fps(60)` 16.67 ms. The Info.plist key stays: without it iOS keeps apps at 60.

8. **Driving a real iPhone from a script.** `devicectl` installs and launches but cannot tap or
   take screenshots, and libimobiledevice's screenshot service is gone since iOS 17. A UI-test
   runner can: `dev/ios/drive` (XcodeGen project; the test drives the app by its bundle id) taps,
   swipes, rotates, presses Home, types, screenshots and dumps the accessibility tree.
   `xcodebuild test` failed with "Root install style is not supported on this device" until the
   scheme listed the test target explicitly (`schemes:` in project.yml).
