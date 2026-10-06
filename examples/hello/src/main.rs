//! The smallest DrawnUI app: a counter. The same code runs on the desktop and in the browser.

// Release builds are a plain Windows app with no console window; debug builds keep the console
// for logs and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drawnui::prelude::*;

#[derive(Default)]
struct App {
    count: i32,
    badge: Handle<SkiaShape>,
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(24).children((
        SkiaLabel::new("DrawnUI for Rust").font_size(28).text_color(Color::WHITE),
        SkiaLabel::new("")
            .font_size(18)
            .text_color(Color::from_rgb(180, 190, 210))
            .observe(|me, app: &App| me.set_text(format!("The button was tapped {} times", app.count))),
        SkiaLayout::row().spacing(12).children((
            SkiaButton::new("Tap me").on_tapped(|_me, app: &mut App, cx| {
                app.count += 1;
                // Every tap turns the badge a little.
                cx.rotate_to(app.badge, app.count as f32 * 15.0, 250, easing::cubic_out);
            }),
            SkiaButton::new("Reset")
                .background_color(Color::from_rgb(70, 80, 100))
                .on_tapped(|_me, app: &mut App, cx| {
                    app.count = 0;
                    cx.rotate_to(app.badge, 0, 250, easing::cubic_out);
                }),
        )),
        SkiaShape::new()
            .corner_radius(16)
            .width_request(120)
            .height_request(120)
            .background_color(Color::from_rgb(40, 44, 60))
            .stroke_color(Color::from_rgb(90, 160, 255))
            .stroke_width(2)
            .assign(&mut app.badge)
            .children((SkiaLabel::new("").font_size(40).text_color(Color::WHITE).center().observe(|me, app: &App| me.set_text(app.count.to_string())),)),
    ))
}

fn main() {
    drawnui::run("DrawnUI for Rust", || {
        Box::new(Ui::new(App::default(), build).font("Default", "assets/OpenSans-Regular.ttf").background(Color::from_rgb(18, 18, 24)))
    });
}
