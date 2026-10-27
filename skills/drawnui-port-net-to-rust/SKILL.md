---
name: drawnui-port-net-to-rust
description: Porting an existing DrawnUI for .NET app, screen or control (DrawnUi.Maui, DrawnUi.Net heads, Blazor; C# fluent code or XAML) to DrawnUI for Rust (the `drawnui` crate) — the workflow (the .NET code is the spec, port screen by screen, compare against the running original, keep a parity ledger), the C# to Rust mapping table (containers, properties, Assign, ObserveProperty, OnTapped, WithGestures, animations, ItemsSource / ItemTemplate, Super.*), what has no direct equivalent and what replaces it (bindings, commands, ObservableCollection, async sequences, native views, MAUI services), custom SkiaControl subclasses as Control impls with props!, and a worked example (a C# page and its Rust port). Load together with drawnui-rust (build setup per platform) before porting any .NET DrawnUI code to Rust.
---

# Porting DrawnUI .NET code to DrawnUI for Rust

DrawnUI for Rust keeps the public contract of DrawnUI for .NET: the same controls, property names
(in snake_case), defaults, layout rules, cache types and gesture order. Most of a port is mechanical
translation. The hard parts are the places where C# leans on .NET (bindings, events, async) and on
MAUI itself. This skill covers the translation. The build setup for each platform (browser, Android,
Linux, icons, assets) is in the `drawnui-rust` skill: load both.

## Workflow

1. **The .NET code is the spec.** Before you call something a bug, a limitation or "different in
   Rust", read the C# that does it and say what C# does, then what the port does. The .NET code holds
   years of edge cases that a fresh port cannot see.
2. **Keep the structure.** One C# page becomes one Rust module with a `build` function; one C#
   control becomes one Rust control. Keep the names, the order of children, the colors and the sizes.
   Do not "improve" the layout while porting: a reviewer compares side by side.
3. **Port screen by screen, and run each one** before the next. Check against the running .NET app:
   same sizes and positions (a headless test can read pixels and rects), and the same taps, hover,
   keys and scroll.
4. **Keep a parity ledger** (`PARITY.md`): one row per C# feature, edited in place, with a status
   (`ported`, `partial`, `skipped`, `changed`) and the C# file it comes from. Write down every place
   where you did something else and why.
   - Where the C# platforms disagree, take the behavior most of them share and use it on every platform.
   - Where C# has a plain bug, follow its intent and mark the row `changed`.
   - Skip code with no callers (dead branches, obsolete APIs), and say so in the row.
5. **Follow later C# fixes.** When the .NET code gets a fix after your port, port the fix and its test.

## The app shell

| .NET | Rust |
|---|---|
| A MAUI app with a `Canvas` (or `DrawnUiWindow`, `DrawnUiElement`) hosting the content | `drawnui::run_sized(title, width, height, \|\| Box::new(Ui::new(App::default(), build)))`: one window (desktop), a canvas (browser), the screen (phones) |
| `UseDrawnUi(...)` font registration, `FontFamily = "MyFont"` | `Ui::font("MyFont", "fonts/MyFont.ttf")` (the first one registered is the default), `.font_family("MyFont")` |
| `Resources/Raw` assets | an `assets` folder copied next to the exe by `build.rs`; web: served next to the page; Android: APK assets (see `drawnui-rust`) |
| The page's `BindingContext` / view model | one app state struct `App` that the `Ui` owns; every handler gets `&mut App` |
| `Super.Screen` insets, `Super.InsetsChanged` | `cx.safe_insets()`, `Ui::on_safe_insets_changed` |
| `Super.MaxFps` | `drawnui::set_max_fps(fps)` |
| `Super.DeviceRotationSnap`, `OrientationChanged` | `cx.device_rotation()`, `Build::listen_device_rotation` + `Control::on_device_rotation` |
| `Super.OnNativeAppPaused` / `OnNativeAppResumed` | `Ui::on_visibility_changed`, `Control::on_app_visibility` |
| `Canvas.RenderingMode` | `Ui::rendering_mode(...)` |

## Composition

| C# fluent / XAML | Rust |
|---|---|
| `new SkiaStack { Spacing = 16, Children = { a, b } }` | `SkiaStack::new().spacing(16).children((a, b))` (a tuple of up to 12, or a `Vec`) |
| `Children = Enumerable.Range(0, 5).Select(i => ...).ToList()` | `.children((0..5).map(\|i\| ...).collect::<Vec<_>>())` |
| `new SkiaLabel("Hi") { FontSize = 18 }` | `SkiaLabel::new("Hi").font_size(18)` |
| Property `HeightRequest`, `BackgroundColor`, `UseCache` | builder `.height_request(…)`, `.background_color(…)`, `.use_cache(CacheType::Image)`; at run time the setter `set_height_request(…)` |
| `HorizontalOptions = LayoutOptions.Fill` / `.Fill()` / `.Center()` | `.fill_x()` / `.fill()` / `.center()`, or `.horizontal_options(LayoutOptions::Center)` |
| `Padding = new Thickness(16)`, `(12, 10)`, `(l, t, r, b)` | `.padding(16)`, `.padding((12, 10))`, `.padding((l, t, r, b))`; same for `.margin` |
| `Colors.White`, `Color.Parse("#3C639F")` | `Color::WHITE`, `Color::from_rgb(0x3C, 0x63, 0x9F)` (a one-line `hex(0x3C639F)` helper in the app keeps ports readable) |
| `HorizontalTextAlignment = DrawTextAlignment.Center` | `.horizontal_text_alignment(TextAlignment::Center)` |
| `SkiaScroll { Content = x }` / `.WithContent(x)` | `SkiaScroll::new().content(x)` |
| `SkiaGrid` + `.WithColumnDefinitions("2*,3*")`, `.SetGrid(col, row)` | `SkiaGrid::new().column_definitions("2*,3*")`, `.column(c).row(r)`, `.column_span(n)` |
| Aliases `SkiaStack`, `SkiaRow`, `SkiaLayer`, `SkiaWrap`, `SkiaMasonry`, `SkiaGrid` | the same names; as in C#, a plain `SkiaLayout` does not fill, the aliases do |
| XAML | rewrite as builders: element = constructor, attribute = builder method, child elements = `.children(...)` |
| A page class `KeyboardPage : SkiaLayer` built in its constructor | a function `pub fn build(app: &mut App) -> Build<SkiaLayout>` in the page's module |

## State, references and handlers

C# keeps references to controls and changes them from events. Rust keeps the data in the app
state and lets the controls read it.

| C# | Rust |
|---|---|
| `.Assign(out _label)` then `_label.Text = x` in an event | the text lives in `App`; the label reads it: `.observe(\|me, app: &App\| me.set_text(app.status.as_str()))`. Observers run after every handler that got `&mut App`, and a setter does nothing when the value is the same |
| `.ObserveProperty(vm, nameof(Score), me => me.Text = ...)` / `.ObserveProperties(...)` | `.observe(\|me, app: &App\| ...)`: no property names, no INotifyPropertyChanged |
| `.ObservePropertyTwoWay(...)` | the control's change handler writes `App` (`.on_end_changed` on a slider, `.on_toggled` on a switch or checkbox, `.on_text_changed` on an editor), its `observe` reads it back |
| `.Assign(out _x)` to reach the control later (animate, scroll, read its size) | a `Handle<T>` in `App`: `.assign(&mut app.x)`; then `cx.get_mut(app.x)` (setters), `cx.find::<T>(app.x)` (read), `cx.scale_to(app.x, ...)` |
| `.OnTapped(me => ...)`, `Clicked = (me, args) => ...` | `.on_tapped(\|me, app: &mut App, cx\| ...)` |
| `.OnLongPressing`, `.OnTextChanged`, `.OnKeyDown`, `.OnHovered`, `ContextMenu` | `.on_long_pressing`, `.on_text_changed` (SkiaEditor), `.on_key_down` (or app-wide `Ui::on_key_down`), `.on_hovered`, `.on_context_menu` / `Ui::on_context_menu` |
| `KeyboardManager.KeyDown += ...` / `-=` in `OnDisposing` | `Ui::on_key_down(\|app, event, cx\| ...)`; nothing to unsubscribe |
| `.WithGestures((me, args, apply) => ...)` (return the control = consumed) | `.consume_gestures(\|me, app: &mut App, cx, gesture\| ...)` returning `true` = consumed |
| `.Initialize(me => ...)`, `.Adapt(me => ...)` | usually not needed: build what you need inline, or set it in `observe`, or with `cx.postpone_before_draw` |
| `Update()` / `Invalidate()` | setters invalidate by themselves; `cx.invalidate(id, Dirty::DRAW)` for state a control reads itself |
| `OnDisposing`, `Dispose` | nothing: removed controls are dropped, their animators and timers stop |

## Animations, timers and threads

| C# | Rust |
|---|---|
| `await control.ScaleToAsync(1.1, 1.1, 120, Easing.CubicOut)` (also `FadeToAsync`, `TranslateToAsync`, `RotateToAsync`) | `cx.scale_to(handle, 1.1, 1.1, 120, easing::cubic_out)` (`fade_to`, `translate_to`, `rotate_to`); each returns an `AnimationId`; a new one of the same kind on the same control replaces the running one, as in C# |
| `await a; await b;` (one after the other) | `let id = cx.scale_to(...); cx.on_finished(id, \|app: &mut App, cx\| { cx.fade_to(...); })` (runs only when the first one reaches its end, not when it is stopped) |
| `await Task.WhenAll(a, b)` | start both in the same handler |
| `.Animate(seconds, (me, animator, value, dt) => ...)`, `AnimateAsync` | `cx.animate(handle, ms, easing, \|progress, cx\| ...)` (progress 0..1); repeat, delay, ping-pong: `cx.start_animator(handle, ValueAnimator::new(from, to, ms, easing).repeat(-1).ping_pong(), \|value, cx\| ...)` |
| `.UpdateNonStop()`, a game loop, `ActionOnTickAnimator` | `cx.action_on_tick(handle, \|time_ms, cx\| ...)` |
| `Device.StartTimer`, `Task.Delay` then UI work | an animator with a delay plus `on_finished`, or `action_on_tick` that checks the time |
| A background `Task` that sets UI properties (`MainThread.BeginInvokeOnMainThread`) | a `std::thread` that sends results over a channel and wakes the UI with `cx.waker()`; the app state changes on the UI thread only |

## Lists and templates

| C# | Rust |
|---|---|
| `ItemsSource = items; ItemTemplate = new DataTemplate(() => new Cell())` | `.items(\|app: &App\| app.items.len(), make_cell, bind_cell)`: `make_cell()` returns the cell and the handles into it, `bind_cell(&handles, app, index, cx)` fills a cell for item `index` |
| `ObservableCollection` events (Add, Remove, Replace, Move, Reset) | tell the layout: `cx.items_inserted(list, index, count)`, `items_removed`, `items_changed`, `items_moved`, `items_reset` |
| `RecyclingTemplate`, `MeasureItemsStrategy`, `VirtualisationInflated` | the same names in snake_case: `.recycling_template(RecyclingTemplate::Enabled)`, `.measure_items_strategy(MeasuringStrategy::MeasureFirst)` |

## Custom controls (a SkiaControl subclass)

A C# subclass becomes a struct that embeds the control it extends (the "base") and implements
`Control`, delegating to that base through `inner`. Override only the hooks the C# class overrides.

```rust
use drawnui::controls::layout::LayoutProps;
use drawnui::controls::shape::ShapeProps;
use drawnui::prelude::*;

// C#: public class Plate : SkiaShape { ... override OnHover ... }
pub struct Plate {
    shape: SkiaShape,
    lit: Color,
}

impl Plate {
    pub fn new(lit: Color) -> Build<Plate> {
        Build::new(Plate { shape: SkiaShape::default(), lit })
    }
}

// The base's properties stay reachable on the new control (builders and setters).
impl Has<ShapeProps> for Plate {
    fn part(&self) -> &ShapeProps { &self.shape.p }
    fn part_mut(&mut self) -> &mut ShapeProps { &mut self.shape.p }
}
impl Has<LayoutProps> for Plate {
    fn part(&self) -> &LayoutProps { Has::<LayoutProps>::part(&self.shape) }
    fn part_mut(&mut self) -> &mut LayoutProps { Has::<LayoutProps>::part_mut(&mut self.shape) }
}
impl Container for Plate {} // it takes children, as SkiaShape does

impl Control for Plate {
    fn inner(&self) -> Option<&dyn Control> { Some(&self.shape) }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> { Some(&mut self.shape) }
    // C# ProcessGestures / OnGestures override:
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if gesture.kind == GestureKind::PointerEnter {
            self.shape.p.stroke_color = self.lit;
            cx.invalidate(Dirty::DRAW);
        }
        self.shape.on_gesture(cx, gesture)
    }
}
```

| C# override | Rust hook |
|---|---|
| `Paint(DrawingContext ctx)` / `OnDraw` | `fn paint(&self, cx: &mut PaintCx)`: `cx.canvas`, `cx.rect` (pixels), `cx.scale`; the tree is read-only there |
| `PaintBackground` | `fn paint_background(&self, cx: &mut PaintCx)` |
| `MeasureAbsolute` / `Measure(widthConstraint, heightConstraint, scale)` | `fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size` (pixels, margin already taken off; infinite = "as much as you need") |
| `ArrangeChildren` / `Arrange` | `fn arrange(&mut self, cx: &mut LayoutCx)` |
| `ProcessGestures(...)` returning the consumer | `fn on_gesture(&mut self, cx, gesture) -> Handled` (`Handled::Yes`, `No`, `Tapped`, `By(child)`) |
| `OnPropertyChanged` / `ApplyProperties` | `fn on_props_changed(&mut self, cx: &mut Cx)` (runs once per frame after changes, not inside the setter) |
| `ComputeEffectsMargin` (a shadow outside the rect) | `fn effects_margin(&self, scale: f32) -> Thickness` |
| `BindableProperty` declarations | `props!`: each property once, with its default and what a change invalidates |

```rust
props!(
    /// The gauge's properties.
    GaugeProps,
    /// Sets them while building.
    GaugeBuild,
    /// Sets them at run time.
    GaugeSet {
        /// The value shown, 0 to 1.
        value / set_value: f32 = 0.0, DRAW;
        /// The arc color.
        arc_color / set_arc_color: Color = Color::WHITE, DRAW;
    }
);
```

The class says what a change invalidates, as the C# property-changed callback does: `MEASURE` (size
may change: measured again), `DRAW` (own pixels: its cache is recorded again), `REPAINT` (position,
transform or opacity: its cache stays), `APPLY` (runs `on_props_changed` before the next layout);
`MEASURE_APPLY` combines two. A control must not repeat a property name of the base it embeds.

## What has no direct equivalent

- **Bindings, `BindingContext`, converters:** state in `App` plus `observe`. A converter is the code
  inside the observe closure.
- **`ICommand`:** the handler itself. `CanExecute` becomes `responds_to_gestures(false)` set from `observe`.
- **Styles and triggers by `BindableProperty`:** a function that returns a styled builder
  (`fn primary_button(text) -> Build<SkiaButton>`), and `observe` for state-driven looks.
- **`ObservableCollection`:** a `Vec` in `App` plus the `cx.items_*` calls above.
- **`async` / `await` chains:** animation ids plus `on_finished`; I/O on threads with the waker.
- **Native views inside the canvas (`SkiaMauiElement`, a WebView, a native entry):** not available.
  Use the drawn controls (`SkiaEditor` for text input), or put the native view beside the canvas in
  the host app.
- **MAUI services (Preferences, FilePicker, Permissions, Share):** use Rust crates or platform code in
  your app. They are not part of the engine.
- **Camera (`SkiaCamera`):** a separate crate, `drawnui-camera`.

## Worked example

C# page (shortened):

```csharp
public class KeyboardPage : SkiaLayer
{
    private SkiaLabel _last;
    public KeyboardPage()
    {
        Children = new List<SkiaControl>
        {
            new SkiaScroll
            {
                Content = new SkiaStack
                {
                    Spacing = 16, Padding = new Thickness(16),
                    Children = new List<SkiaControl>
                    {
                        new SkiaLabel("Keyboard Input") { FontSize = 24, TextColor = Colors.White },
                        new SkiaLabel("Last key: waiting") { FontSize = 16, HorizontalOptions = LayoutOptions.Fill }.Assign(out _last),
                    }
                }
            }.Fill(),
        };
        KeyboardManager.KeyDown += (s, key) => _last.Text = $"Last key: down {key}";
    }
    public override void OnDisposing() { /* KeyboardManager.KeyDown -= ... */ base.OnDisposing(); }
}
```

Rust port:

```rust
// In the app state: what the page shows.
#[derive(Default)]
pub struct KeyboardState {
    last: String,
}

pub fn build(_app: &mut App) -> Build<SkiaScroll> {
    SkiaScroll::new().fill().content(SkiaStack::new().spacing(16).padding(16).children((
        SkiaLabel::new("Keyboard Input").font_size(24).text_color(Color::WHITE),
        SkiaLabel::new("")
            .font_size(16)
            .fill_x()
            .observe(|me, app: &App| me.set_text(app.keyboard.last.as_str())),
    )))
}

// Wired once on the Ui: Ui::new(...).on_key_down(|app: &mut App, event, _cx| keyboard::key(app, event))
pub fn key(app: &mut App, event: &KeyEvent) -> bool {
    app.keyboard.last = format!("Last key: down {}", event.key);
    false // watched, not used
}
```

The label's text moved from a field written by the event into the state the label observes. The
subscription moved from the page's constructor to the `Ui`, and the unsubscribe is gone.

## Traps seen in real ports

- **Handler state type:** every handler names the state type once (`app: &mut App`). A handler that
  names another type compiles, then panics when it runs ("handler expects app state ..."): use the
  `Ui`'s state type everywhere.
- **Units:** builder sizes are points, as in C#. `measure`, `paint` and `cx.rect` work in pixels
  (multiply by `scale`), as the C# `Measure`/`Paint` overrides do.
- **Cache types:** keep the C# `UseCache` of each control: the defaults are the same in both engines,
  so a port that drops `UseCache` changes performance, not looks.
- **The browser build:** every browser app needs the link flags and the 1 MB stack from
  `drawnui-rust`; a stack overflow there corrupts memory without an error message.
- **Behavior differences:** check the port's `PARITY.md` (in the DrawnUI for Rust repository) for
  every control you use. It lists what is ported, partial or skipped, and how each changed row differs.

## See also

- `drawnui-rust`: adding drawnui, the build setup per platform, assets, icons, accessibility.
- `drawnui` and `drawnui-fluent` (the .NET skills): what the C# code you are reading means.
- DrawnUI for Rust: https://github.com/DrawnUi/DrawnUi.Rust (README, PARITY.md, examples), demo
  https://hellorust.drawnui.net (each page has a .NET twin in HelloMaui).
