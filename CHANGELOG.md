# Changelog

Versions are `0.1.0-preview.N`, tagged `v0.1.0-preview.N` (the scheme of DrawnUI for React). Each
release lists what was added, what behaves differently, and what breaks code written for the
version before it. Commits in parentheses.

## Unreleased

### Changed

- `accessibility_role` and `accessibility_live` take any text, not only a constant: a role from
  data or a binding (`String`) works as an `Aria` constant does, which stays free
  (`Cow<'static, str>`, as DrawnUI's string AccessibilityRole).

### Added

- A scroll's content, header, footer and refresh indicator can be set or removed at run time:
  `Cx::set_scroll_content`, `set_scroll_header`, `set_scroll_footer`, `set_refresh_indicator`
  (DrawnUI's settable SkiaScroll.Content / Header / Footer / RefreshIndicator).
- `on_down` / `on_up` on every control (DrawnUI SkiaButton Down / Up): a press and its release, with
  the point in the control's points; the control gets them when it takes the press or no child under
  it does. A tap gives Down, Tapped, Up (the recognizer's order; C# raises Up before Tapped).
- SkiaSvg `on_success` / `on_error` (DrawnUI Success / Error): the picture of a source or markup is
  there, or could not be loaded or parsed; on the next frame, with the source.
- SkiaSprite `on_started` / `on_finished` (DrawnUI Started / Finished): playing starts; it ends
  after the last run, or when stopped or started again while it plays (Finished, then Started).
- `Mut::set_scale` and `ControlProps::scale()` (DrawnUI `Scale`: sets scale_x and scale_y together,
  reads the smaller), beside the builder's `scale`.
- SkiaCarousel `on_scrolled` / `on_stopped`; SkiaDrawer `on_scrolled` / `on_stopped` /
  `on_transition_changed` (DrawnUI SnappingLayout.Scrolled and TransitionChanged, Stopped): the
  position in points, once a frame while it moves; where it came to rest.
- SkiaScroll `max_velocity`, `max_bounce_velocity`, `rubber_effect`, `rubber_damping`,
  `change_distance_panned`, `scrolling_speed_ms`, `auto_scrolling_speed_ms` (DrawnUI properties of the
  same names and defaults; they were fixed values), and `Cx::set_viewport_offset_x` / `_y` (DrawnUI
  ViewportOffsetX / ViewportOffsetY set: a jump, kept inside the content).
- SkiaSwitch / SkiaCheckbox / SkiaRadioButton `default_value` (DrawnUI DefaultValue): a change sets
  `is_toggled` without running `on_toggled`.
- SkiaShaderCarousel `on_from_to_changed` (DrawnUI FromToChanged): the transition is between other
  slides; read `transition_from_index` / `transition_to_index` in it.
- SkiaDecoratedGrid's line defaults are written with its public `horizontal_gradient()` /
  `vertical_gradient()`, so code that copies a default can reset to it.

### Breaking

- `AccessibilityNode::role` and `live` are `Cow<'static, str>`: compare `&*node.role`.

### Fixed

- A side or bottom drawer dragged and held still before the release closed (or opened): any speed
  toward the next snap point went there, also a resting fingertip's drift. Every snapping control
  now counts a release under 100 points per second as no speed, as the carousel already did
  (C# 662cb81b).
- A label wrapping in a star column of a Grid next to an Auto column was cut to one line with an
  ellipsis: its Auto row took the height of a first measure at the grid's whole width. A child on
  a single Auto row is offered the height the row can still grow to, and the row grows to it
  (C# 7cf1007c). A vertical Fill child on such a row is measured once more (unbounded, for the row).

## 0.1.0-preview.6 (crates.io, 2026-10-08)

### Added

- Web host: `DrawnUi.start({ resolveAsset })`. The page decides where every file the engine loads
  comes from: images, GIFs and sprites (the decode worker too), fonts, SVG, Lottie and shaders. A
  string is used as is (a `blob:` url works); `undefined` loads the url as before.
- Web host: `DrawnUi.start({ fonts: [{ alias, url, weight }] })` registers fonts from the page before
  the first frame, as the app's `Ui::font`. A page font never becomes the default font: labels
  name it with `font_family`.

### Fixed

- macOS / iOS (Metal): the frame's input, animations and layout run before the next drawable is
  taken, so waiting for it no longer adds to the frame (`Ui::prepare`, `App::prepare`).
- A control that changes under a hidden screen (a live picture behind a pushed page) asked for a
  frame each time; it asks for none now and shows its latest state when the screen is shown again.
  Animations of hidden controls (the control or an ancestor) pause and go on from where they were
  when shown (C# 329f6c44).
- A recycled templated Grid with `invert` bound almost every item again when items were appended
  (141 binds for 12 new items at 170): every slot was measured again for its new row's height. A
  view that does not fill its height and fits keeps its size now (35).
- SkiaLottie applies layer masks (Skottie's geometric merge: add, subtract, intersect, lighten,
  darken, difference, inverted). A spinner made of masked solid layers drew as a filled square.

### Toolchain

The same as 0.1.0-preview.5: the Skia crates stay at 0.153.6.

## 0.1.0-preview.5 (crates.io, 2026-10-08)

### Added

- Hover for cards and lists (a3946c8): `receives_hover` (on by default for buttons, sliders, toggles, carousels
  and drawers), `is_hovered` on every control under the mouse that takes hover, `on_hovered`,
  `Ui::hovered`. While a scroll, carousel or drawer moves its content, hover waits and is checked
  once when it stops; leaving the canvas clears it at once; touch never hovers.

### Changed

- ImageComposite draws a change deeper than a child again by its area only (a card in a stack in the
  cached list), where it was and where it is, transforms included; past 16 areas or half the control
  it draws everything (a3946c8).

### Fixed

- A U+FE0F (after an emoji), a zero width joiner or a tag that no font of the label has drew a
  missing-glyph box; it draws nothing now (e8078b4).
- ImageComposite drew its content shifted when a child's effects margin moved to another side at
  the same total size (a glow); such a change draws everything again (e8078b4).

### Breaking

- `CompositeRecord` has the fields `areas` and `changed`, and is no longer `Eq`.

### Toolchain

The same as 0.1.0-preview.4: the Skia crates stay at 0.153.6.

## 0.1.0-preview.4 (crates.io, 2026-10-06)

### Added

- API docs on docs.rs for `drawnui` and the Skia crates (`drawnui-skia-bindings` /
  `drawnui-skia-safe` 0.153.6 ship the bindings docs.rs builds with, offline).

### Toolchain

The same as 0.1.0-preview.3, with the Skia crates at 0.153.6 (DrawnUi/rust-skia `drawnui-crates`,
3c86a74; Skia unchanged, archives at https://github.com/DrawnUi/rust-skia/releases/tag/0.153.6, key
prefix `3c86a74286ccd80ee10e`).

## 0.1.0-preview.3 (crates.io, 2026-10-06)

### Added

- Skia prebuilt for every platform: Windows x64 and ARM64, macOS on Apple silicon and Intel, Linux
  x64 and ARM64, iOS devices and the simulator on Apple silicon and Intel, Android arm64, armv7,
  x86_64 and x86, the browser. `drawnui-skia-bindings` / `drawnui-skia-safe` 0.153.5 download them
  from https://github.com/DrawnUi/rust-skia/releases/tag/0.153.5 (key prefix `ed7ffc41299b2c46319a`).
- 32-bit x86 Android (i686-linux-android) builds: the Skia bindings keep C++'s 8-byte alignment of
  opaque Skia storage there (DrawnUi/rust-skia ed7ffc4).

### Toolchain

The same as 0.1.0-preview.2, with the Skia crates at 0.153.5 (DrawnUi/rust-skia `drawnui-crates`,
ed7ffc4).

## 0.1.0-preview.2 (crates.io, 2026-10-06)

### Changed

- Skia needs no setup: `drawnui-skia-bindings` / `drawnui-skia-safe` 0.153.4 download it prebuilt
  from https://github.com/DrawnUi/rust-skia/releases (the release of the crate version, key prefix
  `0c5b9b74975c23470958`). An app drops `SKIA_BINARIES_URL` from its `.cargo/config.toml`; it still
  works to point at a mirror.

### Toolchain

The same as 0.1.0-preview.1, with the Skia crates at 0.153.4 (DrawnUi/rust-skia `drawnui-crates`,
0c5b9b7).

## 0.1.0-preview.1 (crates.io, 2026-10-06)

`drawnui` 0.1.0-preview.1, `drawnui-skia-safe` 0.153.3 and `drawnui-skia-bindings` 0.153.3 on
crates.io. An app depends on `drawnui` only.

The first release. "Since 22e3868" marks what is new against the commit DrawnUI Fiddle pinned
before releases existed (2026-10-02): code that uses it needs this release.

### Toolchain

| | |
|---|---|
| Rust | 1.94.1, edition 2024 |
| Skia crates | `drawnui-skia-safe` / `drawnui-skia-bindings` 0.153.3 on crates.io, published from DrawnUi/rust-skia (branch `drawnui-crates`, 3bf9e2f): rust-skia 0.153.3 plus SkMesh and Visual Studio 18 detection, under names of their own (the code says `skia_safe`, apps use `drawnui::skia`) |
| Skia binaries | release assets keyed by the commit the Skia crates were published from (key prefix `3bf9e2f2c49ef7d4b78b`), built once per target |
| Browser | emsdk 6.0.10, target `wasm32-unknown-emscripten` |
| Android | NDK 26, platform 26, 16 KB pages; winit 0.30.13 and android-activity 0.6.1 with the patches in `dev/winit-android`, `dev/android-activity-null-text` |
| Apple | Xcode; Metal |

Skia archives (the app sets `SKIA_BINARIES_URL` to the release's download folder +
`skia-binaries-{key}.tar.gz`, without `FORCE_SKIA_BINARIES_DOWNLOAD`; skia-bindings fills in the key):

| Target | Archive |
|---|---|
| Windows | `skia-binaries-3bf9e2f2c49ef7d4b78b-x86_64-pc-windows-msvc-ganesh-gl-jpegd-jpege-pdf-svg-textlayout.tar.gz` |
| Linux | `skia-binaries-3bf9e2f2c49ef7d4b78b-x86_64-unknown-linux-gnu-ganesh-gl-jpegd-jpege-pdf-svg-textlayout.tar.gz` |
| Browser | `skia-binaries-3bf9e2f2c49ef7d4b78b-wasm32-unknown-emscripten-ganesh-gl-jpegd-jpege-pdf-svg-textlayout.tar.gz` |
| Android arm64 | `skia-binaries-3bf9e2f2c49ef7d4b78b-aarch64-linux-android-ganesh-gl-jpegd-jpege-pdf-svg-textlayout-vulkan.tar.gz` |
| Android armv7 | `skia-binaries-3bf9e2f2c49ef7d4b78b-armv7-linux-androideabi-ganesh-gl-jpegd-jpege-pdf-svg-textlayout-vulkan.tar.gz` |
| Android x86_64 | `skia-binaries-3bf9e2f2c49ef7d4b78b-x86_64-linux-android-ganesh-gl-jpegd-jpege-pdf-svg-textlayout-vulkan.tar.gz` |
| macOS | `skia-binaries-3bf9e2f2c49ef7d4b78b-aarch64-apple-darwin-ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout.tar.gz` |
| iOS | `skia-binaries-3bf9e2f2c49ef7d4b78b-aarch64-apple-ios-ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout.tar.gz` |
| iOS simulator | `skia-binaries-3bf9e2f2c49ef7d4b78b-aarch64-apple-ios-sim-ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout.tar.gz` |

The Apple archives are built on a Mac (`dev/release-apple.sh`); 32-bit x86 Android has none (it does
not build).

The archives hold the default features (`svg`, which brings `textlayout`). An app that turns drawnui's
features off or on gets another key, finds no archive and compiles Skia from source (LLVM, Python,
Ninja; about 4-6 minutes once).

Browser link flags (`.cargo/config.toml` of the app; the template has them):
`-sMAX_WEBGL_VERSION=2 -sMODULARIZE=1 -sEXPORT_NAME=createDrawnUi
-sEXPORTED_RUNTIME_METHODS=GL,HEAPU8,UTF8ToString -sALLOW_MEMORY_GROWTH=1 -sSTACK_SIZE=1048576
-sSTACK_OVERFLOW_CHECK=1 -sERROR_ON_UNDEFINED_SYMBOLS=0`, linker `em++`.

### Added

- The engine: control tree with fluent builders, measure / arrange / paint, every DrawnUI cache type,
  layouts (Absolute, Column, Row, Wrap, Grid, templated lists with recycling), the controls of the
  DrawnUI Hello app (labels, rich text, editor, scroll, carousel, drawer, shell, toggles, slider,
  progress, backdrop, images, SVG, GIF, Lottie, sprites, shader effects), gestures, animators,
  keyboard and accessibility. Hosts: Windows, Linux, macOS, iOS, Android, the browser.
  `templates/app`: a starter app for every target.
- Since 22e3868:
  - Desktop screen readers through AccessKit: UI Automation, NSAccessibility, AT-SPI (e5c86f6);
    keyboard navigation (Tab, arrow-key groups, focus ring) on the desktop (1d8f67e, afef66f).
  - Selectable label text on the desktop and touch: `Gesture::touch` (d8d66f7).
  - Runtime children changes: `Cx::insert_child`, `replace_child`, `move_child`, `clear_children` (0761d48).
  - Android host: GameActivity, soft keyboard, TalkBack, Choreographer frames, Vulkan by default
    with OpenGL ES as fallback, `Ui::gpu_backend` (6f72225, 24bdae0, 6f1d2c1, 1d22342);
    `drawnui::set_android_app`, `drawnui::AndroidApp`.
  - Linux host (a1081b9); iOS (simulator and device) with 120 Hz from a display link, macOS display
    link, `drawnui::set_max_fps` / `max_fps` (0b68ce3, aaa3735).
  - Safe area as DrawnUI MobileIsFullscreen: `Ui::mobile_fullscreen`, `Ui::on_safe_insets_changed`,
    `Cx::safe_insets`, `Cx::content_insets` (a35ed91).
  - `Ui::rendering_mode(RenderingModeType)`: Accelerated (GPU, the default) or Default (CPU) (10f1e38).
  - `Ui::gestures(GesturesMode::Lock)`: the browser page never scrolls or selects on the canvas (aa1b1c3).
  - System font fallback: `SkiaLabel::system_font_fallback`, on in SkiaRichLabel and SkiaEditor
    (`use_unicode`), `Fonts::match_character` (aefa669).
  - Accessibility values: `AccessibilityValue`, `Control::accessibility_value`,
    `Control::accessibility_set_value`; Increment / Decrement / SetValue / ScrollIntoView from
    screen readers (ac49a93).
  - A SkiaScroll pages for screen readers on iOS and Android (VoiceOver's three-finger swipe,
    TalkBack's scroll forward / back): `Aria::SCROLL_VIEW`, `Ui::accessibility_scroll`.

### Changed

Since 22e3868:
- On phones (Android, iOS, a phone's browser) the root is laid out inside the safe area by default;
  `Ui::mobile_fullscreen(true)` draws under the system bars as before (a35ed91).
- A release after a long press sends no Tapped (4f0ac22).
- Key handlers only observe keys aimed at page elements outside the canvas, and keys of an input
  method that is composing go to it (ce7baf2).
- Text is measured with unhinted advances on every platform: a layout can move by a pixel (a1081b9).
- Wrap: children whose widths add up to exactly the line share it (6356a2d).
- Pan velocity is measured over 16 ms; moves that arrive in one burst count once (6d525f7).
- ImageDoubleBuffered shows its placeholder on every frame until the first bitmap (1b81dd8).
- A slider's or progress bar's value is no longer its accessibility label: the label is the app's,
  the value goes out as a value (ac49a93).

### Breaking

Since 22e3868:
- `Gesture` has a new public field `touch` (d8d66f7) and `AccessibilityNode` the fields `parent`,
  `group`, `value` and `scrolls` (1d8f67e, 66c4b5b, ac49a93): code that builds them with struct
  literals must set them. The accessibility snapshot has a node for every SkiaScroll, the parent
  of the nodes inside it.
- The browser page script `drawnui_host.js` and the wasm must come from the same version (the
  accessibility overlay's message gained fields). Copy the script from the drawnui you build with
  (the template's `web.ps1` / `web.sh` do).
- Skia comes with drawnui as `drawnui-skia-safe` (crates.io): an app drops its own skia-safe and
  any `[patch]` for `skia-safe` / `skia-bindings`, uses `drawnui::skia`, and sets only
  `SKIA_BINARIES_URL` (`FORCE_SKIA_BINARIES_DOWNLOAD` makes a crate compile Skia from source). SkMesh
  (`drawnui::skia::Mesh`, `Canvas::draw_mesh`) works in apps.
