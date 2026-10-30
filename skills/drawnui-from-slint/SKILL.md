---
name: drawnui-from-slint
description: Coming to DrawnUI for Rust (the `drawnui` crate) from Slint — what changes (plain Rust builders instead of a `.slint` file, app state in your own struct, observers and handlers instead of bindings and callbacks), the same counter app in both, a concept-by-concept mapping table (properties, bindings, two-way bindings, callbacks, TouchArea, FocusScope, layouts, Rectangle, Text, Image, Flickable, `for` over models, VecModel and ModelNotify, `if`, named elements, `animate`, states, Timer, invoke_from_event_loop, std-widgets), the mappings people get wrong (observe is not dependency-tracked, explicit list change calls, no declarative states, Timer vs action_on_tick, layout and units), and what DrawnUI adds on top. Load when porting a Slint UI to DrawnUI for Rust or answering "how do I do X from Slint in drawnui"; load drawnui-rust too for build setup.
---

# Coming from Slint

DrawnUI for Rust (the `drawnui` crate) draws the whole UI with Skia. You write the UI in plain Rust instead of a
`.slint` file: there is no markup language, and the compiler and rust-analyzer check every line of the UI.
DrawnUI is MIT licensed on every platform, embedded devices included, with no attribution badge and no
per-device royalty.

## The same app in both

Slint:

```slint
export component App inherits Window {
    in-out property <int> count: 0;
    VerticalLayout {
        spacing: 16px;
        padding: 24px;
        Text { text: "Tapped \{root.count} times"; }
        Button {
            text: "Tap me";
            clicked => { root.count += 1; }
        }
    }
}
```

DrawnUI:

```rust
use drawnui::prelude::*;

#[derive(Default)]
struct App {
    count: i32,
}

fn build(_app: &mut App) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(16).padding(24).children((
        SkiaLabel::new("").observe(|me, app: &App| me.set_text(format!("Tapped {} times", app.count))),
        SkiaButton::new("Tap me").on_tapped(|_me, app: &mut App, _cx| app.count += 1),
    ))
}

fn main() {
    drawnui::run("Counter", || Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf")));
}
```

The state lives in your own struct (`App`). Controls read it in `observe`, handlers change it. Fonts come
from files or bytes you register (`.font(alias, path)`); the browser has no system fonts.

## Concepts

| Slint | DrawnUI for Rust |
| --- | --- |
| `.slint` file, `slint::include_modules!()` | Rust functions that return `Build<...>`; nothing to compile separately |
| `export component X inherits Window` | `drawnui::run(title, \|\| Box::new(Ui::new(App::default(), build)))`; `run_sized` for a window size |
| `in` / `out` / `in-out property` | fields of your `App` struct |
| a binding, `text: root.count` | `.observe(\|me, app: &App\| me.set_text(...))` |
| two-way binding `<=>` | none: the control reports through its handler (`.on_end_changed` on a slider, `.on_toggled`, `.on_text_changed`), the handler writes `App`, the control's `observe` reads it back |
| `callback clicked()`, `clicked => { ... }` | `.on_tapped(\|me, app: &mut App, cx\| ...)`; the closure names the state type |
| `TouchArea` | any control: `.on_tapped`, `.on_long_pressing`, `.on_hovered`, `.consume_gestures(...)` for raw gestures |
| `FocusScope`, `key-pressed` | `.on_key_down(\|me, app: &mut App, cx, key\| ...)` on a control (return `true` when used), or `Ui::on_key_down` for the whole app |
| `VerticalLayout`, `HorizontalLayout` | `SkiaStack::new()`, `SkiaRow::new()` (`.spacing`, `.padding`) |
| `GridLayout` | `SkiaGrid::new().column_definitions("2*,3*")`, `.column(c).row(r)` on each child |
| a plain `Rectangle` holding children | `SkiaLayer::new()` (children placed by their alignment and `.margin`) |
| `Rectangle { background; border-radius }` | `SkiaShape::new()` (`.background_color`, `.corner_radius`, stroke) |
| `Text` | `SkiaLabel::new("...")` (`.font_size`, `.text_color`); markdown: `SkiaRichLabel` |
| `Image` | `SkiaImage`; also `SkiaSvg`, `SkiaGif`, `SkiaLottie` |
| `Flickable`, `ScrollView` | `SkiaScroll::new().content(...)` |
| `for x in [static data]: ...` | `.children(data.iter().map(...).collect::<Vec<_>>())` |
| `for item in model: ...` | `.items(\|app: &App\| app.items.len(), make_cell, bind_cell)`: recycled, virtualized cells |
| `VecModel`, `ModelNotify` (`row_added`, `row_changed`, ...) | the data is a `Vec` in `App`; tell the list what changed: `cx.items_inserted`, `items_removed`, `items_changed`, `items_moved`, `items_reset` |
| `if condition: Element { }` | `.observe(\|me, app: &App\| me.set_is_visible(...))`, or add and remove children with `cx.add_child` / `cx.remove` |
| a named element (`label := Text { }`) reached from code | a `Handle<T>` in `App`: `.assign(&mut app.label)`, then `cx.get_mut(app.label)` |
| `animate x { duration: 200ms; }` | `cx.translate_to`, `scale_to`, `fade_to`, `rotate_to`; any value: `cx.animate(control, ms, easing, \|t, cx\| ...)` (`t` from 0 to 1) |
| `states [ ... ]` with transitions | start the animations above from the handler or observer that changes the state |
| `Timer { interval; triggered => ... }` | `cx.after(control, ms, \|app: &mut App, cx\| ...)` for one shot, owned by a control (it stops when the control goes; call `after` again from the callback to repeat); `cx.action_on_tick(control, \|time_ms, cx\| ...)` for every frame |
| `slint::invoke_from_event_loop` from another thread | send results over a channel and wake the UI with `cx.waker()` |
| `std-widgets` (`Button`, `Slider`, `Switch`, `CheckBox`, `LineEdit`) | `SkiaButton`, `SkiaSlider`, `SkiaSwitch`, `SkiaCheckbox`, `SkiaEditor`, with Material, Cupertino and Windows looks |

## Rules that differ from Slint

- **`observe` is not dependency-tracked.** Every observer runs again at the start of the next frame after any
  handler got `&mut App`, whatever it changed. Keep observers cheap: read a field and call a setter. A setter does
  nothing when the value is the same.
- **Lists change only when you say so.** After changing the `Vec` behind `.items(...)`, call the matching
  `cx.items_*`. There is no diffing: a count that changed without a call is read as an append or a reset.
- **No declarative states.** A state is data in `App`; the handler that changes it also starts the animations.
- **Layout:**
  - A `SkiaStack` gives a child only its own height unless the child is Fill on that axis.
  - A `SkiaRow` measures a child with the width that is left, so Fill children share the rest.
  - For Slint-like stretch between columns, use `SkiaGrid` with star columns.
  - Alignment belongs to the child (`horizontal_options`, `vertical_options`), not to the parent layout.
  - Sizes are points, multiplied by the screen scale.
- **Code style:** app UI reads state in `observe` and changes it in handlers. Direct setters through handles
  (`cx.get_mut(handle)`) are for animations, games and benchmarks.

## What DrawnUI adds

- SkSL shader effects on any control, shader transitions between carousel pages, SkMesh with your own vertex and
  fragment programs.
- Backdrop blur and frosted glass, also under rotated and 3D-turned parents.
- 2D and 3D transforms: `rotation_x` / `rotation_y` with perspective, `translation_z` depth.
- Lottie, GIF and SVG controls.
- Recycled, virtualized lists: long lists, uneven heights with loading at both ends, drag to reorder,
  templated wrap, grid and masonry layouts.
- Every DrawnUI cache type (picture, GPU, double-buffered on background threads, composite); a lost GPU or WebGL
  context recovers by itself.
- Sprites and a game loop.
- Popups past the window's edge: real windows on Windows, macOS and Linux X11, drawn inside the canvas elsewhere.
- A shell with pages, tabs, popups, modals and toasts; carousel, drawer, editor with IME, slider, switch, checkbox,
  radio button, progress.
- A headless test host: render frames and drive gestures in plain `cargo test`.
- The same engine and controls in C# (.NET) and TypeScript (React): a screen ported between them keeps its look
  and logic.

See them all running at https://hellorust.drawnui.net.
