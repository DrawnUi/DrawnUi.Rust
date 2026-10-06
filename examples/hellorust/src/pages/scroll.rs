//! SkiaScroll features: Header (flow / sticky / behind + parallax), Footer, scroll bars, pull to
//! refresh, SnapToChildren, TrackIndexPosition. Ported from the React demo's ScrollPage.tsx.

use drawnui::prelude::*;

use super::{column, scrolling};
use crate::{App, hex};

const PALETTE: [u32; 8] = [0x0F3460, 0x533483, 0x1B4332, 0x7B2D26, 0x495057, 0x0D6EFD, 0xD63384, 0x2D6A4F];

const WELL: Color = hex(0x212529);

/// What the page keeps.
#[derive(Default)]
pub struct State {
    pub(super) refresh_scroll: Handle<SkiaScroll>,
    /// The last part of the refresh card's title.
    pub(super) refresh_state: &'static str,
    /// CurrentIndex of the snapping strip and of the tracked list.
    pub(super) snap_index: Option<usize>,
    pub(super) track_index: Option<usize>,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    app.scroll.refresh_state = "pull down past 60 pt";
    // The content holds scrolls that move by themselves, so neither it nor their cards are
    // cached; the rows keep the Operations cache every shape has.
    scrolling(column().children((
        SkiaLabel::new("SkiaScroll").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        card(
            title("Header + Footer in the flow · Tag=\"Header\" / Tag=\"Footer\" children scroll with the content"),
            SkiaScroll::new()
                .height_request(240)
                .background_color(WELL)
                .ignore_wrong_direction(true)
                .header(band(70, 0x0F3460, "Header (70 pt) · scrolls away", 16))
                .content(rows(14, "Row"))
                .footer(band(50, 0x533483, "Footer (50 pt) · after the content", 14)),
        ),
        card(
            title("HeaderSticky · the header stays at the top, drawn over the content"),
            SkiaScroll::new()
                .height_request(220)
                .background_color(WELL)
                .header_sticky(true)
                .ignore_wrong_direction(true)
                .header(band(44, 0x0D6EFD, "Sticky header", 15))
                .content(rows(14, "Under sticky")),
        ),
        card(
            title("HeaderBehind + HeaderParallaxRatio=0.5 · the content covers the header, which moves at half speed"),
            SkiaScroll::new()
                .height_request(260)
                .background_color(WELL)
                .header_behind(true)
                .header_parallax_ratio(0.5)
                .content_offset(-24)
                .ignore_wrong_direction(true)
                .header(SkiaLayer::new().height_request(160).children((
                    SkiaImage::new("assets/images/baboon.jpg").aspect(TransformAspect::AspectCover).fill(),
                    SkiaLabel::new("Parallax cover")
                        .font_size(22)
                        .font_family("FontTextBold")
                        .text_color(Color::WHITE)
                        .center()
                        .drop_shadow_color(Color::BLACK)
                        .drop_shadow_size(4),
                )))
                .content(
                    SkiaShape::new()
                        .corner_radius((24, 24, 0, 0))
                        .background_color(hex(0x2B3035))
                        .fill_x()
                        .children(rows(12, "Content over the cover")),
                ),
        ),
        card(
            title("SkiaScrollBar Tag=\"ScrollBar\" IsDraggable · drag the thumb or press the track, auto-hides 1 s after scrolling · horizontal: ScrollBarsVisibility=\"Horizontal\""),
            (
                SkiaScroll::new()
                    .height_request(200)
                    .background_color(WELL)
                    .scroll_bar_thumb_color(hex(0x6EA8FE))
                    .scroll_bar_track_color(Color::new(0x22FFFFFF))
                    .ignore_wrong_direction(true)
                    .scroll_bar(SkiaScrollBar::new().is_draggable(true).thickness(8))
                    .content(rows(16, "Scrollbar row")),
                SkiaScroll::new()
                    .orientation(ScrollOrientation::Horizontal)
                    .height_request(70)
                    .background_color(WELL)
                    .scroll_bars_visibility(ScrollBarVisibility::Horizontal)
                    .scroll_bar_thumb_color(hex(0xFFC107))
                    .content(
                        SkiaRow::new().spacing(8).padding(8).children(
                            (0..14)
                                .map(|i| {
                                    SkiaShape::new()
                                        .corner_radius(6)
                                        .width_request(120)
                                        .height_request(50)
                                        .background_color(hex(PALETTE[i % PALETTE.len()]))
                                        .children(SkiaLabel::new(format!("H {}", i + 1)).font_size(13).text_color(Color::WHITE).center())
                                })
                                .collect::<Vec<_>>(),
                        ),
                    ),
            ),
        ),
        card(
            title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "RefreshEnabled + RefreshIndicator (Tag=\"RefreshIndicator\") · RefreshDistanceLimit 60, RefreshShowDistance 50 · {}",
                    app.scroll.refresh_state
                ))
            }),
            (
                SkiaScroll::new()
                    .height_request(220)
                    .background_color(WELL)
                    .refresh_enabled(true)
                    .refresh_distance_limit(60)
                    .ignore_wrong_direction(true)
                    .refresh_indicator(
                        SkiaLayer::new().height_request(50).children(
                            SkiaShape::new()
                                .corner_radius(20)
                                .background_color(hex(0x0D6EFD))
                                .center()
                                .width_request(160)
                                .height_request(36)
                                .children(
                                    SkiaLabel::new("↻ refresh")
                                        .font_family_fallback("FontSymbols,FontSymbols2")
                                        .font_size(14)
                                        .text_color(Color::WHITE)
                                        .center(),
                                ),
                        ),
                    )
                    .on_refresh(|me, app: &mut App, cx| {
                        app.scroll.refresh_state = "refreshing… (2 s)";
                        cx.after(me.id(), 2000, |app: &mut App, cx| {
                            if let Some(mut scroll) = cx.get_mut(app.scroll.refresh_scroll) {
                                scroll.set_is_refreshing(false);
                            }
                            app.scroll.refresh_state = "done · pull again";
                        });
                    })
                    .assign(&mut app.scroll.refresh_scroll)
                    .content(rows(12, "Pull down")),
            ),
        ),
        card(
            title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "SnapToChildren=\"Center\" + TrackIndexPosition=\"Center\" (horizontal) · CurrentIndex {}",
                    index_text(app.scroll.snap_index)
                ))
            }),
            SkiaScroll::new()
                .orientation(ScrollOrientation::Horizontal)
                .height_request(120)
                .background_color(WELL)
                .snap_to_children(SnapToChildrenType::Center)
                .track_index_position(RelativePositionType::Center)
                .on_index_changed(|_me, app: &mut App, _cx, index| app.scroll.snap_index = index)
                .content(
                    SkiaRow::new().spacing(12).padding(8).children(
                        (0..10)
                            .map(|i| {
                                SkiaShape::new()
                                    .corner_radius(10)
                                    .width_request(200)
                                    .height_request(100)
                                    .background_color(hex(PALETTE[i % PALETTE.len()]))
                                    .children(SkiaLabel::new(format!("Snap {i}")).font_size(18).text_color(Color::WHITE).center())
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
        ),
        card(
            title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "TrackIndexPosition=\"Start\" (vertical) · CurrentIndex {} · SnapToChildren=\"Side\"",
                    index_text(app.scroll.track_index)
                ))
            }),
            SkiaScroll::new()
                .height_request(180)
                .background_color(WELL)
                .track_index_position(RelativePositionType::Start)
                .snap_to_children(SnapToChildrenType::Side)
                .ignore_wrong_direction(true)
                .on_index_changed(|_me, app: &mut App, _cx, index| app.scroll.track_index = index)
                .content(rows(16, "Tracked")),
        ),
    )))
}

/// CurrentIndex as the C# page prints it: -1 while no child is at the tracked point.
fn index_text(index: Option<usize>) -> String {
    index.map_or("-1".to_owned(), |index| index.to_string())
}

/// A header or footer: a colored band with a centered caption.
fn band(height: i32, color: u32, text: &str, size: i32) -> Build<SkiaLayout> {
    SkiaLayer::new()
        .height_request(height)
        .background_color(hex(color))
        .children(SkiaLabel::new(text).font_size(size).text_color(Color::WHITE).center())
}

fn rows(count: usize, prefix: &str) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(6).padding(8).children(
        (0..count)
            .map(|i| {
                SkiaShape::new()
                    .corner_radius(6)
                    .background_color(hex(PALETTE[i % PALETTE.len()]))
                    .fill_x()
                    .height_request(40)
                    .children(
                        SkiaLabel::new(format!("{prefix} {}", i + 1))
                            .font_size(13)
                            .text_color(Color::WHITE)
                            .vertical_options(LayoutOptions::Center)
                            .margin((12, 0)),
                    )
            })
            .collect::<Vec<_>>(),
    )
}

fn title(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(12)
        .text_color(hex(0x6EA8FE))
        .font_attributes(FontAttributes::Bold)
        .text_transform(TextTransform::Uppercase)
}

/// A card whose first child is the title label, which some cards keep changing. Not cached: a
/// cache above a scroll is recorded again on every frame the scroll moves.
fn card(title: Build<SkiaLabel>, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(8)
        .background_color(hex(0x2B3035))
        .fill_x()
        .use_cache(CacheType::None)
        .children(SkiaStack::new().spacing(10).padding((16, 12)).children((title, content)))
}
