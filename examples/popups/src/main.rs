//! Popup windows: a right-click menu and a dropdown that may open past the window's edges, the
//! way native menus do. On the desktop each one is a window of its own; the browser, phones and
//! Wayland draw them on the canvas, inside it.

// Release builds are a plain Windows app with no console window; debug builds keep the console
// for logs and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drawnui::prelude::*;

#[derive(Default)]
struct App {
    picked: String,
    /// The popup open now, closed when an item is picked.
    popup: Option<PopupId>,
    dropdown: Handle<SkiaButton>,
}

const PANEL: Color = Color::from_rgb(0x26, 0x2B, 0x3B);
const EDGE: Color = Color::from_rgb(0x3B, 0x42, 0x56);
const HOVER: Color = Color::from_rgb(0x34, 0x3B, 0x52);

/// One line of a menu: highlighted under the mouse, picked with a tap (or Enter, or a screen
/// reader), which closes the menu.
fn item(text: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(6)
        .fill_x()
        .padding((12, 7, 12, 7))
        .accessibility_role(Aria::MENU_ITEM)
        .on_hovered(|me, _app: &mut App, _cx, on| me.set_background_color(if on { HOVER } else { Color::TRANSPARENT }))
        .on_tapped(move |_me, app: &mut App, cx| {
            app.picked = text.to_owned();
            if let Some(popup) = app.popup.take() {
                cx.close_popup_window(popup);
            }
        })
        .children(SkiaLabel::new(text).font_size(15).text_color(Color::WHITE))
}

/// A panel with round corners: the popup window is transparent around them.
fn menu(items: &[&'static str]) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(PANEL)
        .stroke_color(EDGE)
        .stroke_width(1.5)
        .width_request(200)
        .padding(4)
        .accessibility_role(Aria::MENU)
        .children(SkiaLayout::column().fill_x().children(items.iter().map(|t| item(t)).collect::<Vec<_>>()))
}

fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16).padding(20).children((
        SkiaLabel::new("Right-click anywhere: the menu opens where you clicked, also past the window's edge.")
            .font_size(15)
            .text_color(Color::from_rgb(180, 190, 210)),
        SkiaButton::new("Choose a fruit").assign(&mut app.dropdown).on_tapped(|_me, app: &mut App, cx| {
            let fruits = ["Apple", "Banana", "Cherry", "Grape", "Mango", "Orange", "Peach", "Pear"];
            app.popup = Some(cx.open_popup_window(menu(&fruits), app.dropdown, PopupWindowOptions::default()));
        }),
        SkiaLabel::new("")
            .font_size(18)
            .text_color(Color::WHITE)
            .observe(|me, app: &App| me.set_text(if app.picked.is_empty() { "Nothing picked yet".to_owned() } else { format!("Picked: {}", app.picked) })),
    ))
}

fn main() {
    drawnui::run_sized("Popup windows", 360.0, 240.0, || {
        Box::new(
            Ui::new(App::default(), build)
                .font("Default", "assets/OpenSans-Regular.ttf")
                .background(Color::from_rgb(18, 18, 24))
                .on_context_menu(|app: &mut App, at, cx| {
                    let items = ["Cut", "Copy", "Paste", "Select all", "Settings…"];
                    let options = PopupWindowOptions { close_on_pointer_leave: true, ..Default::default() };
                    app.popup = Some(cx.open_popup_window(menu(&items), at.location, options));
                    true
                }),
        )
    });
}
