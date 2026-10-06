//! SkiaSvg: file source, inline SvgString, TintColor, LockRatio sizing. Ported from the React
//! demo's SvgPage.tsx.

use drawnui::prelude::*;

use super::scrolling;
use crate::{App, hex};

const LOGO: &str = "assets/images/drawnui.svg";

const STAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#FFD700" d="M12 2l3.09 6.26L22 9.27l-5 4.87L18.18 21 12 17.77 5.82 21 7 14.14l-5-4.87 6.91-1.01z"/></svg>"##;

const CAPTION: Color = hex(0x94A3B8);

/// Builds the page.
pub fn build(_app: &mut App) -> Build<SkiaLayout> {
    // Nothing here changes once loaded: the scroll content is one bitmap.
    scrolling(SkiaStack::new().spacing(16).padding(16).use_cache(CacheType::Image).children((
        SkiaLabel::new("SkiaSvg").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        SkiaSvg::new(LOGO).width_request(200).lock_ratio(1).horizontal_options(LayoutOptions::Center),
        caption("Source=\"assets/images/drawnui.svg\" WidthRequest={200} LockRatio={1}"),
        heading("TintColor"),
        SkiaWrap::new().spacing(24).horizontal_options(LayoutOptions::Center).children(
            [Color::WHITE, hex(0xFF6B6B), hex(0x4ECDC4), hex(0xFFD93D)]
                .map(|tint| SkiaSvg::new(LOGO).width_request(72).lock_ratio(1).tint_color(tint))
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        heading("SvgString (inline markup) at three sizes"),
        SkiaRow::new().spacing(24).horizontal_options(LayoutOptions::Center).vertical_options(LayoutOptions::Center).children(
            [32, 64, 128]
                .map(|size| SkiaSvg::from_string(STAR).width_request(size).lock_ratio(1).vertical_options(LayoutOptions::Center))
                .into_iter()
                .collect::<Vec<_>>(),
        ),
        caption("Rasterized by Skia at the displayed pixel size, re-rasterized only when that size changes."),
    )))
}

fn heading(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(20).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center).margin((0, 12, 0, 0))
}

fn caption(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(CAPTION).horizontal_options(LayoutOptions::Center)
}
