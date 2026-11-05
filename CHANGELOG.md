# Changelog

Versions are `0.1.0-preview.N`, tagged `v0.1.0-preview.N` (the scheme of DrawnUI for React). Each
release lists what was added, what behaves differently, and what breaks code written for the
version before it. Commits in parentheses.

## Unreleased

### Added

- `Ui::on_app_closing(|app, cx| -> bool)`: the app's closing step on the desktop (the window's close
  button, Alt+F4, Cmd+Q); false keeps the window open, and `Cx::close_app()` ends the app later
  without asking again. Without a handler the window closes as before. `App::app_closing` for hosts;
  `Headless::request_close` and `take_close_app` for tests.

## 0.1.0-preview.31 (crates.io, 2026-11-05)

### Added

- `Host::on_frame_requested(f)`: a host that draws on demand hears when another thread changed
  something (`Waker::invalidate`), once until the next frame takes the changes, on the thread that
  made the change (DrawnUI .NET `DrawnView.FrameRequested`).
- `PopupId::get()`: a popup's number, the `id` its window request carries.
- `.accessibility_modal(true)` (aria-modal): while that control shows, screen readers and Tab keep to
  it and to the popup windows open; the keyboard moves into it and back when it goes; with several,
  the one on top. Its node is read as a modal dialog.
- For tools (DrawnUI Designer): `ControlId::to_bits` / `from_bits` (an id as one number; an id of a
  control that is gone finds nothing). With feature `designer`: `Tree::drawn_rect(id)`,
  `Ui::hit_test_visual(point)` (the control a person sees at a point, through layouts that draw
  nothing), `Ui::on_after_frame(|ui, frame|)` (draw over each frame, read it, apply edits on the UI
  thread), `Tree::is_internal(id)` (controls the engine makes for itself), `Tree::popup_roots()`,
  `Tree::popup_rect(id)`,
  `Ui::snapshot_popup(id, gpu)`, `Ui::render_offscreen(gpu, w, h, scale, time)`,
  `Waker::request_hidden_frame()` (a frame off screen while the window is minimized),
  `Host::request_window_focus()`, and the case names of a choice in `PropInfo::choices`. Also with
  `designer`: `designer::control_types()` and `designer::create(type_name)` (a control of that type
  with its constructor's defaults), `Cx::move_to(control, parent, index)` (a control with everything
  inside it into another parent, its id, handlers and state kept) and `Tree::is_templated(id)` (a
  layout that makes its children from items).
- `Ui::arrows_start_keyboard_navigation(true)`: while nothing has the keyboard, the first arrow key
  starts keyboard navigation (Down / Right as Tab, Up / Left as Shift+Tab); off by default.
  `Cx::set_arrows_start_keyboard_navigation(on)` changes it at run time: a game screen turns it off
  while it plays.
  `Ui::has_no_keyboard_focus()` tells whether nothing has the keyboard.
- `KeyEvent::handled`: the window-level key handlers learn whether the canvas's keyboard navigation
  used the key (DrawnUI `KeyboardManager.IsKeyHandled`); `Ui::mark_key_handled` (`App::mark_key_handled`)
  for a host that used a key itself; `Cx::keep_accessibility_focus()` keeps the focus where it is as a
  popup closes.
- `.keyboard_wrap(true)` on an arrow-key group (a menu, a list): the arrow keys go round, from the
  last item to the first and back.
- `accessibility_is_expanded` (`set_accessibility_is_expanded`): a control that opens a menu or a list
  reads expanded or collapsed to screen readers (aria-expanded; AccessKit expanded).
- `Cx::set_accessibility_focus(control)`: app code moves the keyboard and screen-reader focus to a
  control (a custom menu that moves its own highlight).
- `Ui::set_screen_reader_running` (and `App::screen_reader_running` for hosts): whether a screen
  reader reads the app; the desktop host follows AccessKit.

### Changed

- A draggable scroll bar takes presses only while it shows (or is kept visible): a click on the strip
  of a faded-out bar goes to the content.

- SkiaShell popups and modals are accessibility modals: while one is open, the page under it is not
  read and Tab stays in it.
- A live region (`accessibility_live`) is read by screen readers on the next frame after it changes.
- One key order on every host, as DrawnUI .NET: the focused control's own key handling, then the
  canvas's keyboard navigation (Tab, Enter / Space, group arrows, sliders, Escape closing the top
  popup), then the controls that listen to every key (`listen_keys`, `DrawnGame`) and the window-level
  handlers, all with `KeyEvent::handled`: a game skips the keys navigation used. In the browser, Enter or
  Space on an overlay element presses it first and reaches the app as handled.
- When a popup window closes, an app's `set_accessibility_focus` in that frame wins over the return of
  the keyboard to the popup's opener.
- Browser: menu items with a checkbox or radio role read checked or not checked.
- A menu or dropdown in a popup window of its own takes the keyboard on its checked (else first) item
  once its window shows, as on the canvas.
- A gradient's `light` above 1 multiplies each color's HSL lightness and keeps its hue and saturation,
  as .NET MAUI does: lighter colors stay vivid.
- A label's `fill_gradient` goes to one place: the background when the label has a background color
  with alpha above 0 (the glyphs keep `text_color`), else the glyphs.
- The arrow keys, Home, End, PageUp and PageDown in a recycled list or layout reach every item: the
  scroll brings an item that is not drawn yet into view, then it gets the keyboard.
- When the control the keyboard is on leaves the screen, the keyboard follows the screen reader if
  one runs, else it goes to nothing and the keys are the app's again.
- A control behind an open dialog that still holds the keyboard is no focus: Enter does not press it
  and the first arrow key starts inside the dialog.
- Browser: while a dialog is open, the accessibility overlay itself is the dialog around its controls.
- A looped carousel: a finger release moves one slide at most from the slide the gesture started on,
  also across the wrap.

### Breaks

- `AccessibilityNode` (custom hosts) has new fields, `modal` and `is_expanded`; `KeyEvent` has a new
  field, `handled`.
- A control that listens to every key (`listen_keys`, `DrawnGame`) hears a key after the canvas's
  keyboard navigation: a game skips the keys it used (`!event.handled`), or a focused button's Space
  reaches both.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.30 (crates.io, 2026-11-02)

### Added

- Tooltips: `.tooltip("Save")` on any control shows its text when the mouse rests on it (600 ms),
  centered above it (below it where there is no room above) and staying put while the mouse moves
  over it, at once when moving to the next control; it closes when the mouse
  leaves, on a press, the wheel or a key, and never shows for touch. `Ui::tooltip_style` sets the
  look and the delays (or builds custom content); `Ui::tooltips_from_accessibility(true)` shows a
  control's accessibility hint, or the label of an icon control, when it has no tooltip. On the
  desktop the tooltip is a window of its own, so it can reach past the app window's edge.
  `TooltipStyle::at_cursor` opens it at the mouse instead; `offset_x` / `offset_y` move it.
  `Cx::set_tooltip_style` and `Cx::set_tooltips_from_accessibility` change them at run time.
- `Ui::on_popup_window_closed`: runs for every popup window that closed, whatever closed it (a menu
  bar unselects its title). The wheel outside a popup window closes it and still scrolls.
  When a popup window closes, the keyboard goes back to the control that opened it (a menu bar's
  title, a dropdown's button), as native menus do.
- `PopupWindowOptions::passive`: a popup that only shows (no input, no accessibility node), as a
  tooltip; `PopupWindowOptions::tooltip` and `place_tooltip` place it.
- Popup windows close when the app's window loses the keyboard, on every host, and a closed popup
  leaves the accessibility tree in the next frame, so the keyboard and screen readers move on at
  once. On macOS a popup window shows only what it draws: round corners and its own shadow.

### Changed

- The default `SkiaSwitch` (no ControlStyle) is 46 x 22 points, a low track that fits a settings
  row; its thumb is 18 points. The other styles keep their sizes.
- The default look (no ControlStyle) uses the blue accent #3B82F6 on buttons, switches, checkboxes,
  radio buttons, sliders, progress bars and the editor caret.

### Breaks

- `PopupWindowOptions` has two new fields (`passive`, `tooltip`): write it with `..Default::default()`.
- `PopupWindowRequest` (custom hosts) has two new fields, `passive` and `tooltip`.

## 0.1.0-preview.29 (crates.io, 2026-11-01)

### Added

- `Cx::editor_cursor_at(editor, point)`: the caret index a tap at a point would place, for an app
  that focuses a field itself and puts the caret where the user tapped. (1140c8a)

### Changed

- A caret placed right after `set_text` (`set_cursor_position`, `select_all`...) lands in the new
  text: `set_cursor_position(usize::MAX)` puts it after the new text.
  (1140c8a)

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.28 (crates.io, 2026-10-31)

### Changed

- Faster layout of text: a label measures each word once and keeps it while its text and fonts stay
  the same, so a new layout (a resize, a parent that changed, a column fitting its content) only
  breaks the measured words into lines. A full layout of a page of cards takes about a third less
  time than in preview.26.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.27 (crates.io, 2026-10-31)

### Added

- Tools can read a running app: `Tree::type_name`, `Tree::screen_rect` (where a control shows,
  through scrolling and transforms) and `Ui::hit_test` (the control seen at a point). With the
  feature `designer`, `Tree::props` and `Tree::set_prop` read and write every property of a control
  by name, as text. (1f35892)
- `PopupWindowOptions::cpu`: a popup window drawn on the CPU with per-pixel alpha (Windows), so
  round corners and shadows show the desktop through them. `PopupWindowOptions::on_canvas`: a popup
  drawn on the app's canvas also on the desktop. (beb18ae)
- `Cursor::ResizeLeftRight` and `Cursor::ResizeUpDown`, for splitters (desktop and browser).
  (f1007c4)
- Feature `hot-reload` for the browser: an app built as a side module is swapped into the running
  page with `hotSwap` and keeps its state (`hot_reload!`). (b96284a)

### Changed

- A Column with no width set (a Row with no height set) that does not fill is as wide as its widest
  child; a child that fills it counts with its content size, up to the room there is, and is then
  stretched to the column's width. Give the column a width, or make it fill, when it should take
  all the room. (35c2508)
- Labels lay their text out less often when only the room around them changes. (aaf89be)
- Popup windows close when the app's window is moved, resized or minimized, as native menus do.
  (3531743, 301cee2)
- A popup with an arrow-key group in it (a menu, a dropdown list) takes the keyboard as it opens, on
  its current choice (a checked radio, option or tab item), else its first item. While a popup is
  open, Tab goes round its stops. (bc37c03, 44c29df)
- The arrows move between the items of a group whose role sits on a wrapper around their layout,
  and through a menu or list whose items sit in several columns, in reading order. (bc37c03, 44c29df)

### Breaks

- `PopupWindowOptions` has two new fields: write it with `..Default::default()`.
- `Cursor` has two new cases: a `match` over it needs them (or `_`).
- `PopupWindowRequest` (custom hosts) has a new field, `cpu`.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.26 (crates.io, 2026-10-31)

### Changed

- A cached label keeps the ink that reaches past its letters' boxes, such as the hook of "j" or an
  italic slant, so it looks the same with every cache type.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.25 (crates.io, 2026-10-31)

### Changed

- A single-line `SkiaEditor` puts Arabic or Hebrew text with Start alignment at the right edge of the
  field, as Start does in a label.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.24 (crates.io, 2026-10-31)

### Changed

- `HorizontalTextAlignment` Start and End follow each paragraph's direction: an Arabic or Hebrew
  paragraph starts at the right edge of a label or an editor (End puts it at the left). Center is
  unchanged, and left-to-right text works as before.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.23 (crates.io, 2026-10-30)

### Changed

- `SkiaEditor`: Left / Right at the visual end of a line with mixed directions stays there; it moves
  on only to the next or the previous line.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.22 (crates.io, 2026-10-30)

### Changed

- `SkiaEditor` in right-to-left and mixed text: the caret is drawn where its position is on screen,
  a tap puts it under the finger, Left and Right move it on screen, and a selection highlights the
  selected letters. Taps and arrows never stop inside an emoji sequence or between a letter and its
  marks. A label's own text selection follows the same rules.

### Added

- `TextLine`: `lefts`, `rtl`, `base_rtl`, `index_at_x()` and `boxes()` for editors built on a label.

### Toolchain

Skia crates unchanged (0.153.7). New dependency: `unicode-segmentation` 1.13.

## 0.1.0-preview.21 (crates.io, 2026-10-30)

### Changed

- A space between spans keeps the style of the span it was written in: a highlighted, underlined or
  colored span no longer takes the space before it, and its own spaces stay inside its background.
  Two spans that both have a space at their meeting point keep both spaces.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.20 (crates.io, 2026-10-30)

### Added

- Text shaping with HarfBuzz (`harfrust`) where the text needs it: Arabic and Hebrew letters join and
  run right to left, lines mixing right-to-left and left-to-right text follow the Unicode
  bidirectional order, Indic and South-East Asian scripts get their conjuncts and vowel signs, and
  emoji sequences (families, skin tones, flags, keycaps) draw as one emoji. Other text keeps the
  plain, faster path.
- `SkiaRichLabel`: `***bold italic***` and `___bold italic___`.

### Toolchain

Skia crates unchanged (0.153.7). New dependencies: `harfrust` 0.15, `unicode-bidi` 0.3.

## 0.1.0-preview.19 (crates.io, 2026-10-30)

### Changed

- A swipe on a carousel moves at most one slide, as phone paging does: from slide N a release lands on
  N-1, N or N+1, however far the finger dragged and however fast it flicked. `scroll_to` and
  `selected_index` still go anywhere.
- An editor focused from code shows its caret at once, at the end of its text, so typing appends. A
  caret position the app set before focusing is kept.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.18 (crates.io, 2026-10-27)

### Added

- `SkiaLayout::item_at(x, y)`: the item under a point in any templated layout, also where items sit
  side by side (Row, Wrap, Masonry, Grid, a split Column). For drag and drop over a grid of tiles.

### Changed

- Keyboard arrows in a masonry go to the tile on screen that way: Up / Down along the column,
  Left / Right to the nearest tile of the next column, PageUp / PageDown one screen along the column.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.17 (crates.io, 2026-10-27)

### Added

- Masonry layout: `SkiaMasonry` (`LayoutType::Masonry`). `split` columns of equal width, each item
  placed under the shortest column, so items of different heights leave no gaps. Works with children
  and with `items` (recycled views, taps, `scroll_to_index`, `on_load_more`); an append or a changed
  item measures only those items.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.16 (crates.io, 2026-10-25)

### Changed

- Frosted glass (SkiaBackdrop) follows every transform: inside a scaled, moved, rotated or 3D-turned
  parent it shows exactly what is behind it, with the same blur on screen.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.15 (crates.io, 2026-10-25)

### Changed

- A shader effect on a control with a picture cache (Operations, OperationsFull: the default of
  SkiaShape, SkiaLabel, SkiaSvg) now replaces the control and works on its content, as on an Image
  cache: its picture is drawn into an image once per cache, which is the effect's `iImage1`. Before, the
  picture was drawn and the effect over it, so a transparent result (a dissolve) left the content
  showing. Uncached controls are unchanged: they paint, then the effect draws over them.
- A SkiaBackdrop inside a picture cache (a parent's Operations / OperationsFull cache, or its own)
  keeps the copy of what is behind it that was made when the picture was recorded, as in DrawnUI for
  .NET: the library no longer records a parent's cache again because a backdrop is inside it. Give
  the parents of a backdrop `use_cache(CacheType::None)` (or an Image cache that holds what the glass
  should show). The backdrop logs this once.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.14 (crates.io, 2026-10-25)

### Fixed

- macOS: popup windows never came on screen (the window was made at the right place but stayed
  hidden). They now show at once, without taking the key window from the app; checked on a Mac in
  the example and in an app.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.13 (crates.io, 2026-10-25)

### Added

- `Ui::on_window_focus` and `Cx::window_focused`: the app's window got the keyboard or lost it to
  another app, once per change. On the desktop it comes right away from the window's focus events
  (a popup window of the app is not leaving it); when a click brings a background window forward,
  Windows and macOS report the focus before that click's Down reaches any control. The browser
  reports window focus and blur, phones the app being shown or hidden. Hosts: `App::window_focus`.

### Toolchain

Skia crates unchanged (0.153.7).

## 0.1.0-preview.12 (crates.io, 2026-10-25)

### Added

- Popup windows (`Cx::open_popup_window`, `PopupAnchor`, `PopupWindowOptions`, `PopupId`): any
  controls shown over everything from a point, a rect or a control, placed as native menus are
  (flipped where the screen ends). On Windows, macOS and Linux X11 each one is a window of its own that
  may extend past the app's window (frameless, on top, no taskbar entry; on Windows it never takes the
  activation); in the browser, on Android, iOS and Wayland it is drawn on the canvas. It closes on a
  press outside, Escape, the app losing the keyboard, or `close_on_pointer_leave`. Hosts:
  `Host::popup_windows`, `App::popup_frame` and the other `App::popup_*` methods. Example
  `examples/popups`.
- AI skill `skills/drawnui-port-net-to-rust`: porting DrawnUI .NET apps and controls to Rust
  (workflow, C# to Rust mapping, custom controls, a worked example).

### Toolchain

Skia crates unchanged (0.153.7). Popup windows are checked on Windows and on Linux X11; on macOS
they did not come on screen in this version (fixed in preview.14).

## 0.1.0-preview.11 (crates.io, 2026-10-21)

### Added

- Android on Vulkan: Skia is told the device features the device was made with (the sampler YCbCr
  conversion), so images with a YCbCr conversion (camera frames in the device's own format, imported
  with `import_vulkan_image`) can be drawn. `VulkanHandles::sampler_ycbcr_conversion` now means Skia
  knows it too.

### Changed

- App hidden / shown (`Ui::on_visibility_changed`, `Control::on_app_visibility`) follows DrawnUI C#'s
  moments: Android hidden at onPause and shown at onResume (before: hidden only when the window ended
  at onStop, so a system dialog or a split-screen switch counted as shown), iOS shown only when the app
  is active again (before: already when it came to the foreground); hidden on resigning active as
  before. Each change is reported once (Android and iOS sent several events for one change). Android
  apps need the DrawnUi/winit fork at rev fd36859 (README).

### Toolchain

The Skia crates move to 0.153.7 (drawnui-skia-safe `BackendContext::set_device_features2`); Skia
itself is the same as 0.153.6.

## 0.1.0-preview.10 (crates.io, 2026-10-19)

### Added

- 3D transforms (C# RotationX, RotationY, RotationZ, TranslationZ): `rotation_x` / `rotation_y` turn a
  control around its horizontal / vertical axis through the anchor, seen with perspective (a camera
  at 576 pixels, as DrawnUI); `rotation_z` turns it after them; `translation_z` is depth: positive
  comes toward the viewer and draws bigger, negative goes away and draws smaller, turned or flat
  (576 / (576 - z): -300 = 0.66x, +150 = 1.35x). A tap goes through the inverse matrix, so a turned
  control takes it where it is drawn.
- `Waker` (`Cx::waker`, `Ui::waker`): another thread marks a control dirty and wakes the frame loop,
  for data that arrives off the UI thread (camera frames, sensors). The marks are applied at the
  start of the next frame; a burst wakes the loop once; no frames while nothing comes. In the
  browser the page script calls `requestFrame` after it.
- `Control::on_app_visibility`: a control that listens (`Build::listen_app_visibility`,
  `Cx::listen_app_visibility`) hears the app hidden and shown, at the moments of
  `Ui::on_visibility_changed` (C# `Super.OnNativeAppPaused` / `OnNativeAppResumed`).
- Device rotation (C# `Super.DeviceRotationSnap`, `Super.OrientationChanged`): `Cx::device_rotation`
  is how the device is held, 0, 90, 180 or 270 degrees clockwise from its natural position
  (LandscapeRight 90, LandscapeLeft 270), snapped at 45 degrees; flat counts as 0, as in C#. A
  control that listens (`Build::listen_device_rotation`, `Cx::listen_device_rotation`) hears each
  new value (`Control::on_device_rotation`). Android reads the accelerometer every 200 ms (as C#'s
  OrientationEventListener), iOS reads it through CoreMotion by the same rule (C# reads UIDevice's
  orientation; here it stayed unknown on a phone held still from launch), both only while a control
  listens and the app is in front. 0 on the desktop and in the browser.
- `Cx::postpone_before_draw` (C# `PostponeExecutionBeforeDraw`): runs a closure with the app state
  at the start of the next frame, whatever is shown.
- `PaintCx::gpu`: the frame's GPU context for a control's paint (`None` on the CPU and in pictures
  for bake workers), to draw textures made outside Skia (a camera frame): `Gpu::backend`,
  `import_gl_texture` (2D or external OES), `import_metal_texture`, `import_vulkan_image`,
  `vulkan_handles`, `reset_gl_state`, and `frame_serial` / `completed_serial` to release a
  borrowed texture once the GPU finished the frame that drew it. drawnui now depends on
  drawnui-skia-bindings directly (the crate skia-safe already uses).
- Android on Vulkan: the device is made with what importing camera and video frames needs, where
  the device has it (VK_ANDROID_external_memory_android_hardware_buffer with
  VK_EXT_queue_family_foreign, sampler YCbCr conversion, external memory, dedicated allocation,
  external semaphores) and Skia is told the enabled extensions; `VulkanHandles::hardware_buffers` /
  `sampler_ycbcr_conversion` say what was enabled. Skia itself does not sample YCbCr images yet
  (the Skia crates do not pass device features to it).
- `props!` takes doc comments for the three items it makes (before each name) and passes a
  property's doc comments onto its setter too, so a crate that denies `missing_docs` can use it;
  the syntax without them is unchanged.
- `ImageProps::build_paint` is public: the paint SkiaImage draws with (effects, blur, the app's
  filters), for controls that draw their own pictures.
- Off-screen children: `LayoutCx::measure_child_scaled` / `arrange_child_scaled` lay a child out
  at a scale of its own (a video recording's overlay), `PaintCx::record_child` records it as a
  picture another thread can replay, `Base::content_epoch` says when it changed. Such a child is
  never painted on screen, takes no gestures and has no accessibility node.
  `PaintCx::child_count` / `child` / `child_base`.
- `DRAWNUI_NO_ACTIVATE=1` (desktop): the window opens without taking the foreground or flashing
  in the taskbar, for runs that scripts or test tools start while someone works at the machine.
- Pinch and turn (C# `Manipulation`): while two or more fingers are down, Panning carries
  `Gesture::manipulation` (`scale`, `rotation`, their totals, the center, `touches_count`);
  `Gesture::touches` counts the fingers; a second finger makes the press no tap. The hosts send the
  other fingers through `App::pointer_finger` (Android, iOS, the browser); `Headless::pinch`.

### Changed

- The FPS pill (`Ui::show_fps`) and SkiaLabelFps count every frame drawn in a row, also the frames
  input or the `Waker` asked for (a camera preview showed "FPS -"), as C# counts every canvas
  frame; after a pause of more than 100 ms the count starts again.

### Breaking

- `Gesture` has two more fields, `touches` and `manipulation`: code that builds a `Gesture` with a
  struct literal sets them (`0`, `None`).

### Fixed

- A cached bitmap (Image, ImageDoubleBuffered, ImageComposite) drawn scaled, turned or skewed showed
  stair steps on every edge: it was always blitted with nearest sampling and an aliased border. It now
  takes linear sampling with mipmaps whenever its own transform, an ancestor's or the picture it is
  recorded into distorts it (a cached child of a turned SkiaShape), with an antialiased border; a
  picture holding such bitmaps is recorded again when it starts or stops being distorted. Moved only,
  it stays nearest, pixel for pixel. A tilted cached card against the same card uncached: 252 pixels
  off by more than 48 before, 0 now.
- Windows: a minimized window was never reported hidden (`Ui::on_visibility_changed`,
  `Control::on_app_visibility`), because winit reports no occlusion there. Minimizing now counts as
  hidden and restoring as shown; a window covered by another one still counts as shown.

### Docs

- How a page script feeds the app in the browser (`DrawnUi.start` returns `module` and
  `requestFrame`; a WebGL texture registered in `module.GL.textures` for `import_gl_texture`), in
  `web/drawnui_host.js` and the public skill.
- Transforms, 2D and 3D, in the public skill: the pivot, rotation_x / rotation_y with measured
  sizes, a card's back side, translation_z as depth, smooth cached content, taps through 3D.

### Toolchain

The same as 0.1.0-preview.9: the Skia crates stay at 0.153.6.

## 0.1.0-preview.9 (crates.io, 2026-10-15)

### Changed

- `responds_to_gestures` is the one switch that turns a control's input off, and accessibility
  follows it (DrawnUI 6w, DrawnUi.Net 31e5e564). SkiaButton has it now; its `is_disabled` /
  `set_is_disabled` stay as deprecated aliases of the opposite.

### Breaking

- SkiaButton's `p.is_disabled` field is gone: read `p.responds_to_gestures` (the builder and setter
  keep working, deprecated).

### Docs

- The public skill (`skills/drawnui-rust`) covers switching input off and the iPhone's slow-drag stall
  (the system holds a touch's moves while it decides on a system swipe; not an engine bug).

### Fixed

- A tap on a switched-off button, switch, checkbox or radio fell through to the control under it. It
  now stops there and does nothing: the press, the tap, the long press and the context menu are taken;
  moves, the wheel and hover pass on. No pressed state, no tapped / `on_down` / `on_up` handler (the
  press handlers follow the control's "can interact" answer). A scroll, drawer or carousel that is off
  still lets its content take taps.

### Toolchain

The same as 0.1.0-preview.8: the Skia crates stay at 0.153.6.

## 0.1.0-preview.8 (crates.io, 2026-10-10)

### Fixed

- SkiaShell with the browser history: a hash typed into the address bar, or a plain `#/route` link,
  while a page was open went back to the root instead of opening that hash (the new browser entry
  has no depth of the shell's, read as 0 = a Back to the first entry). An entry is now taken as a
  Back only when its hash is the stack it was pushed with.

### Toolchain

The same as 0.1.0-preview.7: the Skia crates stay at 0.153.6.

## 0.1.0-preview.7 (crates.io, 2026-10-10)

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
- SkiaButton `lock_panning` (DrawnUI LockPanning): the button keeps the pans of its press, a scroll
  under it stays still.
- SkiaSprite `Mut::set_current_frame` (DrawnUI CurrentFrame set).
- SkiaEditor `is_focused(true)` on the builder (DrawnUI IsFocused): the editor takes the focus once
  mounted.
- SkiaShaderCarousel `on_from_to_changed` (DrawnUI FromToChanged): the transition is between other
  slides; read `transition_from_index` / `transition_to_index` in it.
- SkiaDecoratedGrid's line defaults are written with its public `horizontal_gradient()` /
  `vertical_gradient()`, so code that copies a default can reset to it.

### Breaking

- `AccessibilityNode::role` and `live` are `Cow<'static, str>`: compare `&*node.role`.

### Fixed

- The hand cursor showed over everything inside and beside an open SkiaShell popup (its background
  wrapper takes taps to close it) and over disabled buttons: the cursor now follows the control's own
  "can interact" answer, as the accessibility snapshot does.
- `Cx::open_url` did nothing on Android and iOS: Android starts an ACTION_VIEW intent, iOS calls
  UIApplication `openURL:options:completionHandler:` (type-checked; not run on a device yet).
- No per-frame checks while content moves or animates: the animators of hidden controls and the
  hover of hidden controls are looked at again only after a change that can show or hide a control
  (`is_visible` set, a mount, a list cell shown or released, a new animator), not every frame; a
  frame that something else asks for walks no ancestors for queued changes; SkiaShaderCarousel looks
  at its transition while moving only with an `on_from_to_changed` handler.
- A side or bottom drawer dragged and held still before the release closed (or opened): any speed
  toward the next snap point went there, also a resting fingertip's drift. Every snapping control
  now counts a release under 100 points per second as no speed, as the carousel already did
  (C# 662cb81b).
- A label wrapping in a star column of a Grid next to an Auto column was cut to one line with an
  ellipsis: its Auto row took the height of a first measure at the grid's whole width. A child on
  a single Auto row is offered the height the row can still grow to, and the row grows to it
  (C# 7cf1007c). A vertical Fill child on such a row is measured once more (unbounded, for the row).

### Toolchain

The same as 0.1.0-preview.6: the Skia crates stay at 0.153.6; built and tested with Rust 1.94.1.
The install examples (README, the skill, the templates) now name `drawnui = "0.1.0-preview"`, the
newest preview, instead of one version.

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
