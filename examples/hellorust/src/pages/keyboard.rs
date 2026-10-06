//! Keyboard input: the window-level key handlers with modifier state and a history. Ported from
//! the React demo's KeyboardPage.tsx (the Blazor sandbox KeyboardProbe page).

use drawnui::prelude::*;

use super::scrolling;
use crate::{App, hex};

const WAITING: &str = "Waiting for input";

/// What the page keeps.
pub struct State {
    hero: &'static str,
    last: String,
    modifiers: String,
    /// Newest first, five at most.
    history: [String; 5],
    /// The last 40 typed characters.
    chars: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            hero: "Keyboard input ready",
            last: "Last key: waiting".to_owned(),
            modifiers: "Modifiers: shift false, ctrl false, alt false".to_owned(),
            history: std::array::from_fn(|_| WAITING.to_owned()),
            chars: String::new(),
        }
    }
}

/// Builds the page.
pub fn build(_app: &mut App) -> Build<SkiaLayout> {
    // The probe card changes on every key; the rest is static.
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        SkiaLabel::new("Keyboard Input").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new("The window-level key handlers (Ui::on_key_down / on_key_up / on_key_char, React KeyboardManager): shortcuts, game input, drawn editors. Click the page, then press letters, arrows, modifiers or function keys.")
            .font_size(13)
            .text_color(hex(0xADB5BD))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center),
        // The Blazor probe canvas: cream card, blue banner, last key, modifiers, recent events.
        SkiaStack::new().spacing(14).padding(20).background_color(hex(0xFEFDF6)).children((
            SkiaLabel::new("")
                .font_size(28)
                .font_family("FontTextBold")
                .text_color(hex(0x252B37))
                .fill_x()
                .observe(|me, app: &App| me.set_text(app.keyboard.hero)),
            SkiaShape::new().corner_radius(18).background_color(hex(0x3C639F)).height_request(92).fill_x().padding(16).children(
                SkiaLabel::new("Press letters, arrows, modifiers, or function keys")
                    .font_size(18)
                    .text_color(Color::WHITE)
                    .fill_x()
                    .vertical_options(LayoutOptions::Center),
            ),
            SkiaLabel::new("").font_size(16).text_color(hex(0x41495A)).fill_x().observe(|me, app: &App| me.set_text(app.keyboard.last.as_str())),
            SkiaLabel::new("").font_size(14).text_color(hex(0x636F80)).fill_x().observe(|me, app: &App| me.set_text(app.keyboard.modifiers.as_str())),
            SkiaLabel::new("")
                .font_size(14)
                .text_color(hex(0x636F80))
                .fill_x()
                .observe(|me, app: &App| me.set_text(format!("KeyChar (printable, no Ctrl/Alt): \"{}\"", app.keyboard.chars))),
            SkiaStack::new().spacing(8).padding(14).background_color(hex(0xF2EDE0)).children((
                SkiaLabel::new("Recent events").font_size(18).font_family("FontTextBold").text_color(hex(0x47321C)).fill_x(),
                (0..5)
                    .map(|i| {
                        SkiaLabel::new("")
                            .font_size(14)
                            .text_color(hex(0x5C4A35))
                            .fill_x()
                            .observe(move |me, app: &App| me.set_text(app.keyboard.history[i].as_str()))
                    })
                    .collect::<Vec<_>>(),
            )),
        )),
    )))
}

/// A key event while the page is open (the page subscribes as the React one does). Never used:
/// the page only watches, the browser keeps its own keys.
pub fn key(app: &mut App, event: &KeyEvent) -> bool {
    let page = &mut app.keyboard;
    match event.kind {
        KeyKind::Char => {
            page.chars.push_str(event.text);
            let extra = page.chars.chars().count().saturating_sub(40);
            if let Some((cut, _)) = page.chars.char_indices().nth(extra) {
                page.chars.drain(..cut);
            }
        }
        KeyKind::Down | KeyKind::Up => {
            let phase = if event.kind == KeyKind::Down { "down" } else { "up" };
            let key = if event.key.is_empty() { "Unknown" } else { event.key };
            let keyboard = event.keyboard;
            page.hero = "Keyboard probe live";
            page.last = format!("Last key: {phase} {key}");
            page.modifiers = format!(
                "Modifiers: shift {}, ctrl {}, alt {}",
                keyboard.is_shift_pressed(),
                keyboard.is_control_pressed(),
                keyboard.is_alt_pressed()
            );
            page.history.rotate_right(1);
            page.history[0] = format!("{phase} {key}");
        }
    }
    false
}
