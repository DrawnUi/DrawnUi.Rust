//! Uneven rows: MeasureVisible measures the visible cells on demand, estimates the rest and
//! measures it ahead, with LoadMore at both ends. Ported from the React demo's
//! UnevenCellsPage.tsx and FeedCell.ts.

use drawnui::prelude::*;

use super::set_text;
use crate::{App, hex};

const STATUS_HEIGHT: i32 = 36;


const WORDS: [&str; 28] = [
    "drawn", "ui", "renders", "every", "pixel", "itself", "skia", "canvas", "recycled", "cells", "measure", "visible",
    "estimates", "the", "rest", "and", "refines", "in", "idle", "time", "uneven", "rows", "news", "feed", "social",
    "timeline", "product", "catalog",
];

const PALETTE: [u32; 7] = [0x0D6EFD, 0x6610F2, 0xD63384, 0xFD7E14, 0x20C997, 0x0DCAF0, 0xFFC107];

/// One post in the uneven feed.
pub struct FeedItem {
    id: i32,
    title: String,
    pub(super) body: String,
    pub(super) color: Color,
}

/// What the page keeps.
#[derive(Default)]
pub struct State {
    pub(super) scroll: Handle<SkiaScroll>,
    pub(super) feed: Handle<SkiaLayout>,
    pub(super) items: Vec<FeedItem>,
    loading: &'static str,
    busy: bool,
    /// The debug line, written on every scroll and by STATS.
    pub(super) debug: String,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    app.uneven.items = (1..=200).map(make_item).collect();
    SkiaLayer::new().fill().children((
        SkiaLabel::new("")
            .font_size(13)
            .text_color(hex(0xD3D3D3))
            .horizontal_options(LayoutOptions::Center)
            .margin((0, 10, 0, 0))
            .observe(|me, app: &App| {
                let (count, loading) = (app.uneven.items.len(), app.uneven.loading);
                let separator = if loading.is_empty() { "" } else { " · " };
                me.set_text(format!("{count} uneven cells · MeasureVisible · LoadMore at both ends{separator}{loading}"));
            }),
        // Neither the scroll nor the list is cached: each cell is its own bitmap.
        SkiaScroll::new()
            .fill()
            .margin((0, STATUS_HEIGHT, 0, 0))
            .load_more_offset(300)
            .load_more_top_offset(100)
            .assign(&mut app.uneven.scroll)
            .on_scrolled(|_me, app: &mut App, cx, _offset| refresh(app, cx))
            .on_load_more(|me, app: &mut App, cx| load_more(app, cx, me.id(), false))
            .on_load_more_top(|me, app: &mut App, cx| load_more(app, cx, me.id(), true))
            .content(
                SkiaStack::new()
                    .recycling_template(RecyclingTemplate::Enabled)
                    .measure_items_strategy(MeasuringStrategy::MeasureVisible)
                    .spacing(8)
                    .padding((16, 8))
                    .assign(&mut app.uneven.feed)
                    .items(|app: &App| app.uneven.items.len(), feed_cell, bind_feed),
            ),
        // Jump toolbar: wraps on narrow windows.
        SkiaWrap::new()
            .spacing(6)
            .margin((8, 0, 8, 36))
            .horizontal_options(LayoutOptions::Center)
            .vertical_options(LayoutOptions::End)
            .children((
                jump_button("HOME", |app, cx| jump(app, cx, 0, RelativePositionType::Start)),
                jump_button("MIDDLE", |app, cx| jump(app, cx, app.uneven.items.len() / 2, RelativePositionType::Start)),
                jump_button("END", |app, cx| jump(app, cx, app.uneven.items.len().saturating_sub(1), RelativePositionType::End)),
                SkiaButton::new("STATS")
                    .font_size(12)
                    .background_color(hex(0x20C997))
                    .width_request(104)
                    .on_tapped(|_me, app: &mut App, cx| refresh(app, cx)),
            )),
        // The React page's debug line: the list's DebugString and the canvas frame time / FPS.
        SkiaLabel::new("")
            .font_size(11)
            .text_color(hex(0x00FF00))
            .background_color(Color::new(0xDD000000))
            .input_transparent(true)
            .margin((8, 4))
            .horizontal_options(LayoutOptions::Center)
            .vertical_options(LayoutOptions::End)
            .max_lines(1)
            .observe(|me, app: &App| me.set_text(app.uneven.debug.as_str())),
    ))
}

/// The debug line: the list's DebugString, the frame time and the frames per second.
fn refresh(app: &mut App, cx: &mut Cx) {
    let list = cx.find::<SkiaLayout>(app.uneven.feed).map(|feed| feed.debug_string()).unwrap_or_default();
    let stats = cx.frame_stats();
    app.uneven.debug = format!("{list} · {:.1} ms · {} fps", stats.frame_time_ms, stats.fps);
}

/// Deterministic pseudo-random body, so a given id always renders the same height. Same numbers
/// as the C# `MakeItem`: a 32-bit linear congruential generator seeded from the id.
pub(super) fn make_item(id: i32) -> FeedItem {
    let mut seed = ((id as i64 + 100_000) * 2_654_435_761) as u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed as f64 / 4_294_967_296.0
    };
    let count = 4 + (rnd() * 56.0) as usize;
    let mut body = String::new();
    for word in 0..count {
        if word > 0 {
            body.push(' ');
        }
        body.push_str(WORDS[(rnd() * WORDS.len() as f64) as usize]);
    }
    body[..1].make_ascii_uppercase();
    body.push('.');
    FeedItem { id, title: format!("Post {id}"), body, color: hex(PALETTE[id.rem_euclid(PALETTE.len() as i32) as usize]) }
}

fn jump_button(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .font_size(12)
        .background_color(hex(0x0D6EFD))
        .width_request(104)
        .on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

fn jump(app: &App, cx: &mut Cx, index: usize, position: RelativePositionType) {
    cx.scroll_to_index(app.uneven.scroll, index, position, 400);
}

/// Appending keeps every measured height; prepending keeps the visible rows in place: the list
/// reports what was added above the viewport and the scroll follows.
fn load_more(app: &mut App, cx: &mut Cx, scroll: ControlId, top: bool) {
    if std::mem::replace(&mut app.uneven.busy, true) {
        return;
    }
    app.uneven.loading = if top { "loading history…" } else { "loading next page…" };
    // C# awaits Task.Delay(400) here.
    cx.after(scroll, 400, move |app: &mut App, cx| {
        let items = &mut app.uneven.items;
        if top {
            let first = items.first().map_or(1, |item| item.id);
            items.splice(0..0, (first - 30..first).map(make_item));
            cx.items_inserted(app.uneven.feed, 0, 30);
        } else {
            // The list reads a longer count as appended.
            let last = items.last().map_or(0, |item| item.id);
            items.extend((last + 1..=last + 100).map(make_item));
        }
        app.uneven.loading = "";
        app.uneven.busy = false;
    });
}

/// The controls of one cell that a bind writes.
#[derive(Default)]
struct FeedHandles {
    stripe: Handle<SkiaShape>,
    title: Handle<SkiaLabel>,
    body: Handle<SkiaLabel>,
    footer: Handle<SkiaLabel>,
}

/// Uneven recycled cell: the body text wraps to 1..6 lines, so every row has its own height.
fn feed_cell() -> (Build<SkiaLayout>, FeedHandles) {
    let mut handles = FeedHandles::default();
    // A base SkiaLayout does not fill; without fill_x the cell would size to the stripe.
    let cell = SkiaLayout::new()
        .fill_x()
        .padding((16, 12, 16, 12))
        .background_color(hex(0x111827))
        // The previous bitmap shows while a cell is recorded again.
        .use_cache(CacheType::ImageDoubleBuffered)
        .children((
            SkiaShape::new().corner_radius(3).width_request(6).fill_y().assign(&mut handles.stripe),
            SkiaStack::new().spacing(6).margin((18, 0, 0, 0)).children((
                SkiaLabel::new("").font_size(15).font_family("FontTextBold").text_color(Color::WHITE).assign(&mut handles.title),
                SkiaLabel::new("").font_size(13).text_color(hex(0xCBD5E1)).fill_x().assign(&mut handles.body),
                SkiaLabel::new("").font_size(11).text_color(hex(0x64748B)).assign(&mut handles.footer),
            )),
        ));
    (cell, handles)
}

fn bind_feed(cell: &FeedHandles, app: &App, index: usize, cx: &mut Cx) {
    let item = &app.uneven.items[index];
    if let Some(mut stripe) = cx.get_mut(cell.stripe) {
        stripe.set_background_color(item.color);
    }
    set_text(cx, cell.title, item.title.as_str());
    set_text(cx, cell.body, item.body.as_str());
    set_text(cx, cell.footer, format!("#{} · {} chars", item.id, item.body.len()));
}
