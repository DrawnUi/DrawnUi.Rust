//! The app. `run` starts it on the desktop and in the browser (`main.rs`), `android_main` on Android.

use drawnui::prelude::*;

#[derive(Default)]
struct App {
    count: i32,
}

fn build(_app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        SkiaLabel::new("Hello, DrawnUI").font_size(28).text_color(Color::WHITE),
        SkiaLabel::new("")
            .font_size(18)
            .text_color(Color::from_rgb(180, 190, 210))
            .observe(|me, app: &App| me.set_text(format!("Tapped {} times", app.count))),
        SkiaButton::new("Tap me").accessibility_role(Aria::BUTTON).on_tapped(|_me, app: &mut App, _cx| app.count += 1),
    ))
}

pub fn run() {
    drawnui::run("My app", || {
        Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(Color::from_rgb(18, 18, 24)))
    });
}

/// Android: the activity starts the app here (build the library as a cdylib, see README.md).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    drawnui::set_android_app(app);
    run();
}
