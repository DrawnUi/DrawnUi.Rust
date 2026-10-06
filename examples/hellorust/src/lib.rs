//! HelloRust: the demo app of DrawnUI for Rust, a port of HelloMaui (C#) and the React demo.
//! One source for the desktop window, the browser and Android (`examples/hellorust-android`).

use drawnui::prelude::*;

mod catalog;
mod pages;

use catalog::SAMPLES;

/// The publish counter shown in the root page footer (as HelloReact's `publish.ts`): one more on
/// every publish to hellorust.drawnui.net.
pub const PUBLISH: u32 = 10;

/// A color from `0xRRGGBB`, the way the C# pages write `Color.Parse("#RRGGBB")`.
pub const fn hex(rgb: u32) -> Color {
    Color::new(0xFF00_0000 | rgb)
}

#[derive(Default)]
pub struct App {
    /// Navigation between the catalog and the sample pages, as the React demo's SkiaShell.
    shell: Handle<SkiaShell>,
    /// The debug FPS counter, kept above the system bars.
    fps: Handle<SkiaLabelFps>,
    catalog: Handle<SkiaLayout>,
    /// The sample page on top; `None` while the catalog shows.
    route: Option<&'static str>,
    /// Pages the shell built whose work (timers, animations) has not started yet.
    opening: Vec<&'static str>,
    /// A page is being pushed: its work starts once it is in place, not while it slides in.
    pushing: bool,
    /// Where the shell is, for the Shell page: route, stack, popups, modals, toasts.
    shell_info: String,
    /// Catalog columns: two when the canvas is wide enough, one otherwise.
    columns: i32,
    cells: pages::cells::State,
    uneven: pages::uneven::State,
    scroll: pages::scroll::State,
    transforms: pages::transforms::State,
    keyboard: pages::keyboard::State,
    text: pages::text::State,
    layouts: pages::layouts::State,
    images: pages::images::State,
    sprites: pages::sprites::State,
    animations: pages::animations::State,
    a11y: pages::a11y::State,
    looks: pages::looks::State,
    shaders: pages::shaders::State,
    reorder: pages::reorder::State,
    shell_page: pages::shell::State,
    snapping: pages::snapping::State,
    pong: pages::pong::State,
    editor: pages::editor::State,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    let shell = SkiaShell::new()
        .assign(&mut app.shell)
        .titles(SAMPLES.iter().map(|sample| (sample.route, sample.title)))
        .on_navigating(|_me, app: &mut App, _cx, e| {
            if e.source == NavigationSource::Push && !e.cancel {
                app.pushing = true;
            }
        })
        .on_route_changed(|_me, app: &mut App, cx, route| {
            let name = split_route(route).0;
            app.route = SAMPLES.iter().find(|sample| sample.route == name).map(|sample| sample.route);
            // A page rebuilt from a link (the shell reports no navigation for it, as React's)
            // starts its work now. A pushed one waits for the end of its slide: its animations
            // running during the slide cost a dropped frame now and then (measured in Chrome).
            if !app.pushing {
                pages::start(app, cx);
            }
        })
        .on_navigated(|_me, app: &mut App, cx, e| {
            app.pushing = false;
            pages::start(app, cx);
            pages::shell::navigated(app, e);
        })
        .on_changed(|me, app: &mut App, _cx| app.shell_info = pages::shell::describe(me.control_mut()))
        .root(pages::root::build(app));
    let shell = SAMPLES
        .iter()
        .fold(shell, |shell, sample| shell.route(sample.route, move |app: &mut App, _arguments: &ShellArguments| pages::page(app, sample)));
    // The FPS counter is for development: debug builds only, in the .NET samples' look, a sibling
    // above the shell (every hello app).
    let fps: Vec<Build<SkiaLabelFps>> = match cfg!(debug_assertions) {
        true => vec![
            SkiaLabelFps::new()
                .assign(&mut app.fps)
                .margin(fps_margin(Thickness::ZERO))
                .vertical_options(LayoutOptions::End)
                .horizontal_options(LayoutOptions::End)
                .rotation(-45.0)
                .background_color(hex(0x8B0000))
                .text_color(Color::WHITE)
                .z_index(110),
        ],
        false => Vec::new(),
    };
    SkiaLayer::new().fill().children((shell, fps))
}

/// The FPS counter's margin: the .NET samples' (4 from the right, 24 from the bottom), inside the
/// safe area the content keeps out of itself (`Cx::content_insets`).
fn fps_margin(insets: Thickness) -> Thickness {
    Thickness::new(0.0, 0.0, 4.0 + insets.right, 24.0 + insets.bottom)
}

/// What the app and its tests share: pages that listen to the keyboard subscribe while they are
/// open, and every label is read as text, every button is a button (the React demo's startup).
/// The Images page's photo loads while the catalog shows, so the page opens with it.
fn configure(ui: Ui<App>) -> Ui<App> {
    ui.preload_images([pages::images::PHOTO])
        .on_key_down(pages::key)
        .on_key_up(pages::key)
        .on_key_char(pages::key)
        .default_accessibility_role::<SkiaLabel>(Aria::TEXT)
        .default_accessibility_role::<SkiaButton>(Aria::BUTTON)
        // A right click, long press or Menu key that no control took: the versions in a toast.
        .on_context_menu(|app: &mut App, _menu, cx| {
            let text = format!("drawnui {} · Skia m{}", drawnui::VERSION, drawnui::skia::MILESTONE);
            cx.show_toast(app.shell, &text, 3000.0);
            true
        })
        .on_canvas_resized(|app: &mut App, size, cx| {
            app.columns = pages::root::columns(size.width);
            // The shell follows the safe area by itself; the FPS counter is the app's (zero
            // unless mobile_fullscreen: the root is inside the safe area already).
            let insets = cx.content_insets();
            if let Some(mut fps) = cx.get_mut(app.fps) {
                fps.set_margin(fps_margin(insets));
            }
        })
        // A hidden window gets no frames: the game pauses, as the React PongPage on visibilitychange.
        .on_visibility_changed(|app: &mut App, visible, cx| {
            if app.route == Some("pong") {
                pages::pong::visibility(app, cx, visible);
            }
        })
}

/// Runs the app: the desktop window, the browser canvas, the Android activity.
pub fn run() {
    // The hello apps open at 980 x 820 points on the desktop (every head).
    drawnui::run_sized("DrawnUI for Rust", 980.0, 820.0, || {
        // Same fonts and aliases as the React demo's ConfigureFonts. The first one is the default
        // font, which is what its styles give every label and button caption; Bold and
        // FontWeight 600 pick the Semibold face. The symbol and emoji fonts (the Noto Color Emoji
        // faces + hands subset) do not hold back the first frame.
        Box::new(
            configure(Ui::new(App::default(), build))
                .font("FontText", "assets/OpenSans-Regular.ttf")
                .font_weight("FontText", "assets/OpenSans-Semibold.ttf", 600)
                .font("FontTextBold", "assets/OpenSans-Semibold.ttf")
                // Pong's score and messages, as in the .NET Pong samples.
                .font("FontGame", "assets/Orbitron-Regular.ttf")
                .font_fallback("FontSymbols", "assets/NotoSansMathSymbols-Subset.ttf")
                .font_fallback("FontSymbols2", "assets/NotoSansSymbols2-Subset.ttf")
                .font_fallback("FontEmoji", "assets/NotoColorEmoji-Subset.ttf")
                .background(pages::PAGE_BACKGROUND),
        )
    });
}
