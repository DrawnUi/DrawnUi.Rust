//! Root menu, ported from the React demo's RootPage.tsx: dark body, logo and bold title, then the
//! sample cards flowing in a wrap of two columns, their titles in gradients that cycle through the
//! drawnui.net accents.

use drawnui::prelude::*;

use crate::catalog::{SAMPLES, Sample};
use crate::{App, hex};

const MAX_WIDTH: i32 = 820;
const PAGE_PADDING: i32 = 24;
const GAP: i32 = 16;

/// Title gradients cycle through the drawnui.net accents.
const TITLE_GRADIENTS: [[u32; 2]; 6] = [
    [0x6EA8FE, 0x0D6EFD],
    [0xD63384, 0xFD7E14],
    [0x20C997, 0x0DCAF0],
    [0xFFC107, 0xFD7E14],
    [0xA98EFF, 0x6610F2],
    [0x0DCAF0, 0x6EA8FE],
];

/// Catalog columns for a canvas this wide (points): two when the inner width reaches 640.
pub fn columns(width: f32) -> i32 {
    let inner = width.min(MAX_WIDTH as f32) - (PAGE_PADDING * 2) as f32;
    if inner >= 640.0 { 2 } else { 1 }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayer::new().fill().assign(&mut app.catalog).children(SkiaScroll::new().fill().content(
        // Nothing here changes: the scroll content is one bitmap, blitted at the scroll offset.
        // Auto-width and centered: its Fill children take the constraint up to MAX_WIDTH.
        SkiaStack::new()
            .spacing(24)
            .padding((PAGE_PADDING, 24, PAGE_PADDING, 40))
            .horizontal_options(LayoutOptions::Center)
            .maximum_width_request(MAX_WIDTH)
            .use_cache(CacheType::Operations)
            .children((
                SkiaSvg::new("assets/images/drawnui.svg")
                    .use_cache(CacheType::Image)
                    .width_request(120)
                    .lock_ratio(1)
                    .horizontal_options(LayoutOptions::Center)
                    .margin((0, 16, 0, 0))
                    .accessibility_role(Aria::IMG)
                    .accessibility_label("DrawnUI logo"),
                SkiaLabel::new("DrawnUI for Rust")
                    .font_size(48)
                    .font_family("FontTextBold")
                    .text_color(Color::WHITE)
                    .horizontal_options(LayoutOptions::Center)
                    .accessibility_role(Aria::HEADING),
                SkiaLabel::new("A UI rendering engine on top of Skia: layouts, controls, gestures, effects and animations")
                    .font_size(16)
                    .text_color(hex(0xADB5BD))
                    .horizontal_options(LayoutOptions::Center)
                    .horizontal_text_alignment(TextAlignment::Center)
                    .margin((0, -12, 0, 0)),
                // Two columns on wide screens, one on phones (React computes the card width from the
                // window): Split gives every card its share of the width.
                SkiaWrap::new()
                    .spacing(GAP)
                    .split(2)
                    .use_cache(CacheType::ImageComposite)
                    .observe(|me, app: &App| me.set_split(app.columns))
                    .children(SAMPLES.iter().enumerate().map(|(i, sample)| card(i, sample)).collect::<Vec<_>>()),
                // The React footer: one centered label, the repository fragment a colored span.
                SkiaLabel::new("")
                    .use_cache(CacheType::Operations)
                    .font_size(12)
                    .text_color(hex(0x6C757D))
                    .horizontal_options(LayoutOptions::Center)
                    .margin((0, 16, 0, 0))
                    .spans((
                        TextSpan::new("hellorust.drawnui.net · "),
                        TextSpan::new("github.com/DrawnUi/DrawnUi.Rust")
                            .text_color(hex(0x6EA8FE))
                            .on_tapped(|_me, _app: &mut App, cx| cx.open_url("https://github.com/DrawnUi/DrawnUi.Rust")),
                        TextSpan::new(format!(" (repo will go public soon) · MIT · Publish {}", crate::PUBLISH)),
                    )),
            )),
    ))
}

/// The gradient of the title of card `index`.
pub(crate) fn title_gradient(index: usize) -> [Color; 2] {
    TITLE_GRADIENTS[index % TITLE_GRADIENTS.len()].map(hex)
}

fn card(index: usize, sample: &'static Sample) -> Build<SkiaShape> {
    let [from, to] = title_gradient(index);
    SkiaShape::new()
        .corner_radius(12)
        .background_color(hex(0x2B3035))
        .stroke_color(hex(0x373B3E))
        .stroke_width(1)
        .fill_x()
        .accessibility_role(Aria::BUTTON)
        .accessibility_label(sample.title)
        .accessibility_hint(sample.text)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .on_tapped(move |_me, app: &mut App, cx| {
            // A left click or a touch opens the page; the other buttons do not (React checks the button).
            if cx.gesture().is_none_or(|gesture| gesture.button == MouseButton::Left) {
                cx.go_to(app.shell, sample.route, true);
            }
        })
        .children((
            SkiaStack::new().spacing(6).padding((24, 20, 48, 20)).children((
                SkiaLabel::new(sample.title)
                    .font_size(22)
                    .font_family("FontTextBold")
                    .text_color(Color::WHITE)
                    .fill_gradient(SkiaGradient::new(GradientType::Linear, [from, to]).angle(0.0))
                    .accessibility_role(Aria::PRESENTATION),
                SkiaLabel::new(sample.text).font_size(13).text_color(hex(0xADB5BD)).accessibility_role(Aria::PRESENTATION),
            )),
            SkiaLabel::new("›")
                .font_size(28)
                .text_color(hex(0x6EA8FE))
                .horizontal_options(LayoutOptions::End)
                .vertical_options(LayoutOptions::Center)
                .margin((0, 0, 20, 0))
                .accessibility_role(Aria::PRESENTATION),
        ))
}
