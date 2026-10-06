// The code from DrawFiddle. It defines `struct App` (with Default), `fn build`, and may define
// `fn configure(ui: drawnui::Ui<App>) -> drawnui::Ui<App>`.
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
        SkiaButton::new("Tap me").on_tapped(|_me, app: &mut App, _cx| app.count += 1),
    ))
}
