# Dungeon Run

A first-person dungeon runner made with DrawnUI for Rust. It runs in the browser, as a desktop
window and as an Android app from the same code.

## How to play

You run forward by yourself. Stay alive as long as you can.

| Action      | Keyboard             | Touch                                        |
| ----------- | -------------------- | -------------------------------------------- |
| Start       | Space, Enter         | Tap                                          |
| Change lane | Left / Right, A / D (hold to keep moving) | Drag left or right (finger or mouse) |
| Jump        | Up, W, Space         | Tap or click anywhere                        |
| Help        | F1 on the desktop (a browser may keep F1 for its own help) | The ? button |
| Pause (home or resume) | P, Escape | The II button                                |

- **Rune pillars**: change lane.
- **Red beam** and **lava**: jump.
- **Orbs**: a little health each (ten fill one cell). They come in rows of three: the third of a
  row taken without a miss counts double. The orbs lead to the lane the next pillars leave free.
- **The distance is the score**: how far you got, top left, in meters. The dungeon ends at
  3000 m: the last hazards stand at 2900 m, the corridor brightens toward the daylight at its end,
  and out of the door the run is over: YOU ESCAPED! with your time; the title keeps the session's
  best. That screen stays until a key or a tap.
- **Ghost**: hangs in any lane. Far away only its eyes burn; as you come near the torches dim,
  the fog thickens, then it reveals itself and lunges. It takes three tenths of your health in a
  blackout; the one that takes the last of it stays over you while you lie there. Change lane.
  About one power row in five after 60 m is a ghost, in any lane but never the one lane a double
  pillar row just before or after it leaves free.
- **Green cross**: gives a fifth of your health back, in a green aura.
- **Surge** (cyan arrows): four seconds much faster, and nothing hurts you; rays of light fly
  past along the borders of the screen and fade as the time runs out.
- Running itself wears you down: a cell of health every four seconds, so the orbs are what keeps
  you going. A pillar takes two tenths of your health, a beam or lava one tenth, a ghost three. The
  run ends at zero. After a hit nothing hurts you for a moment.
- The run gets faster with distance, and the colors change every 400 units.

A run starts calm: few hazards, pillars before the first jumps, the full share after about 40
seconds. In that start every hazard comes with a prompt, JUMP or CHANGE LANE; after it, the first
three of each kind in a session. Once a session: YOU TIRE AS YOU RUN at the second cell lost,
COLLECT ORBS TO HEAL after the first hit, AVOID GHOSTS after the first ghost gone by; LOW HEALTH
under four cells. Every twelve
seconds without a hit a word of praise shows instead.

A run starts with a countdown. When the health is gone, GAME OVER stays a few seconds (any key
or tap skips it), then its picture burns away over the title's run.

The title screen plays the game by itself until you start.

## Accessibility

What the game does for players who do not use a mouse, or who use a screen reader. It comes from
DrawnUI's own accessibility layer; the game only names things and gives them roles.

**Keyboard**

- The whole game works without a pointer: start, steer, jump, help, pause, every dialog.
- Tab and Shift+Tab walk the buttons. Enter or Space presses the button in focus.
- In a dialog, Tab stays inside it (the buttons behind it are taken away while it is open). Enter or
  Space presses its main button when no button is in focus; Escape closes it.
- A held lane key keeps stepping: no fast tapping is needed.

**Mouse and touch**

- One finger or one mouse button is enough: drag sideways to change lanes (a lane for every 44
  points, there and back in one drag), tap or click anywhere to jump.

**Screen readers**

- Buttons are buttons with names: Help, Pause, and the captions of the dialog buttons.
- Every text on the screen is read as text. A dialog is a dialog.
- The health is one value, "Health 70 percent", said again when it changes.
- The distance is "Distance 120 meters".
- The countdown, GAME OVER, the JUMP and CHANGE LANE prompts and the names of power-ups are said at
  once when they appear.
- In the browser this is an ARIA layer over the canvas. DrawnUI gives the same tree to desktop
  screen readers; for this game only the browser was checked.

**Not by color alone**

- The health is a count of lit cells, not only green, amber and red.
- What to do is written in words (JUMP, CHANGE LANE), and the help names each thing next to its
  sample.

**Time**

- Pause at any moment (P, Escape or the II button). An open dialog stops the run.
- GAME OVER can be skipped with any key or tap.
- The first hazards of a session come with a written prompt about a second before them.

**What is missing**

- The run itself needs sight and quick reactions. A screen reader can work the menus and hear the
  state, but there is no way to play the run blind.
- No sound at all, so no sound cues.
- No setting to turn down motion and flashes (the hit flash, the camera shake, the rays of a surge).
- Not tried with a real screen reader yet: the tree was read from the page, not listened to.
- The dialog has a role but no name yet.

## No 3D API: it is all Skia

The game looks 3D, but it uses no 3D engine and makes no OpenGL, WebGL or other 3D API calls. It
calls only Skia's 2D API, through DrawnUI:

- **One `SkMesh` per frame.** The game's code turns every wall, floor and pillar corner into a 2D
  screen point on the CPU and cuts off what is behind the camera. Skia gets a flat list of 2D
  triangles.
- **No depth buffer.** Far parts are drawn first and near parts over them (the painter's algorithm).
  The corridor is straight in the game's world; the camera bends it with distance, so far to near is
  always the right order.
- **Correct perspective inside each triangle.** A 2D mesh blends values in screen space. Each vertex
  carries its values divided by its depth, and the fragment program multiplies them back.
- **No textures.** Bricks, bevels, lava, runes, torch light, fog, flames and orbs are math in one
  SkSL fragment program. Two torches light every pixel.
- **A post effect** in SkSL (`SkiaShaderEffect`) adds color fringes, a vignette, grain, the ripple of
  a new zone and the red of a hit.
- **Everything else is ordinary DrawnUI** over the scene: the HUD (shapes and labels in cached
  groups), the title, the buttons, and the dialogs, which are `SkiaShell` popups with a panel drawn by
  a shader.

One honest note: Skia draws with the GPU (WebGL2 in the browser), as it does for every DrawnUI
screen. That is inside Skia. `SkMesh` needs this GPU drawing; with CPU drawing a mesh shows nothing.

## Where things are

- `src/scene.rs`: the rules of the run and the control that builds and draws the mesh.
- `src/shaders.rs`: the SkSL of the mesh and of the post effect.
- `src/lib.rs`: input, the game loop, the HUD, the title and the dialogs.
- `src/plate.rs`: the plate of a button, which lights up under the pointer.

## Build

One crate is the game on every desktop and in the browser: `src/lib.rs` is the game, `src/main.rs`
starts it, and DrawnUI brings the window (Windows, Linux) or the canvas (browser). `build.rs` puts
the icon into the Windows exe and copies `assets` (and, on Linux, `icon.ico`) next to the binary.
All commands run from the repository root.

**Windows**

```
cargo run --release -p dungeon
```

The exe is `target\release\dungeon.exe`, with its `assets` folder beside it. A release build opens
no console window.

**Linux** (also WSL 2, where WSLg shows the window)

```
dev/linux.sh run dungeon
```

`dev/linux.sh build dungeon` only builds. The packages it needs are listed at the top of the
script. Under WSLg start it on X11 (`WAYLAND_DISPLAY=` unset); its OpenGL there is software, so the
frame rate says nothing about a real GPU.

**Browser**

```
dev\build.ps1 -Example dungeon -WebOnly
```

It writes the web build to `target\web\dungeon`; serve that folder over http.

**macOS**

```
cargo run --release -p dungeon
```

The Metal host (a CAMetalLayer, frames from the view's display link on macOS 14+). The binary is
`target/release/dungeon`, with its `assets` folder beside it.

**iOS** (simulator and iPhone)

```
dev/ios/run-sim.sh --example dungeon             # the simulator ("DrawnUi iPhone 17 Pro")
dev/ios/run-device.sh --example dungeon          # a connected iPhone (add --release for a release build)
```

The scripts build `aarch64-apple-ios-sim` / `aarch64-apple-ios`, make `DungeonRun.app` (bundle id
`net.drawnui.dungeon`, the icon from `icon.ico`, the assets), install and
start it with its console (frame stats, panics). The device script signs with an Apple Development
identity and a development profile that lists the phone (see `dev/ios/run-device.sh`). Skia comes
from `target/skia-bin` (the archive of a source build for that target). Drag to change lane, tap to
jump; the ? and II buttons open the help and the pause.
The start stays black: the launch screen's color is `launch_color` in `dev/ios/apps.sh`, and the
view behind the canvas takes the game's `Ui::background`, so nothing white shows before the first
frame.

**Android**

```
dev\android.ps1 -Example dungeon
```

The game as the native library of a GameActivity: `../dungeon-android` holds the `cdylib` with
`android_main` and the Gradle app (package `net.drawnui.dungeon`). The script builds the library with
cargo-ndk (x86_64 for the emulator; `-Abi arm64-v8a` for a phone, `-Abi all` for both), packs the APK
with Gradle into `target\android\dungeon\dungeon.apk`, installs and starts it. Drag to change lane,
tap to jump; Back pauses a run and leaves the game from the title.
