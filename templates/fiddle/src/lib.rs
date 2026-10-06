//! The app: `run` starts it on the desktop, in the browser and on iOS (`main.rs`), `android_main`
//! on Android. The code from DrawFiddle is in `app.rs`.

// The fiddle code: `struct App`, `fn build`, and maybe `fn configure`.
include!("app.rs");
// Calls the fiddle code's `configure` when it has one.
include!("configure.rs");

/// The window's background, the one chosen in DrawFiddle (0xRRGGBB).
const BACKGROUND: u32 = 0x121218;

pub fn run() {
    drawnui::run("My App", || {
        // The fonts DrawFiddle gives every snippet. The first one is the default font.
        let ui = drawnui::Ui::new(App::default(), build)
            .font("FontText", "assets/fonts/OpenSans-Regular.ttf")
            .font_weight("FontText", "assets/fonts/OpenSans-Semibold.ttf", 600)
            .font("FontTextBold", "assets/fonts/OpenSans-Semibold.ttf")
            .font("FontTextTitle", "assets/fonts/OpenSans-Semibold.ttf")
            .font("FontGame", "assets/fonts/Orbitron-Regular.ttf")
            .font_fallback("FontSymbols", "assets/fonts/NotoSansMathSymbols-Subset.ttf")
            .font_fallback("FontSymbols2", "assets/fonts/NotoSansSymbols2-Subset.ttf")
            .font_fallback("FontEmoji", "assets/fonts/NotoColorEmoji-Subset.ttf")
            .background(drawnui::skia::Color::new(0xFF00_0000 | BACKGROUND));
        Box::new(configure_ui(ui))
    });
}

/// Android: the activity starts the app here (the library is built as a cdylib, see README.md).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    run();
}
