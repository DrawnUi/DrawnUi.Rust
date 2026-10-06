//! Recycled cells: the templated layout is the scroll's only content, the DrawnUI way.
//! 100 000 items, only the visible cells exist. Ported from the React demo's CellsPage.tsx and
//! ContactCell.ts.

use drawnui::prelude::*;

use super::set_text;
use crate::{App, hex};

const STATUS_HEIGHT: i32 = 36;


/// Huge data source: 100 000 items. Item `i` of the C# list is the number `i + 1`.
const ITEMS: usize = 100_000;

/// What the page keeps.
#[derive(Default)]
pub struct State {
    scroll: Handle<SkiaScroll>,
    pub(super) feed: Handle<SkiaLayout>,
    /// The number of the contact tapped last.
    pub(super) last_tapped: Option<usize>,
    /// The debug line, written on every scroll.
    pub(super) debug: String,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayer::new().fill().children((
        SkiaLabel::new("")
            .font_size(13)
            .text_color(hex(0xD3D3D3))
            .horizontal_options(LayoutOptions::Center)
            .margin((0, 10, 0, 0))
            .observe(|me, app: &App| {
                let tapped = app.cells.last_tapped.map_or("-".to_owned(), |item| item.to_string());
                me.set_text(format!("100 000 recycled cells · last tapped: {tapped}"));
            }),
        // Neither the scroll nor the list is cached: each cell is its own bitmap.
        SkiaScroll::new()
            .fill()
            .margin((0, STATUS_HEIGHT, 0, 0))
            .assign(&mut app.cells.scroll)
            .on_scrolled(|_me, app: &mut App, cx, _offset| refresh(app, cx))
            .content(
                SkiaStack::new()
                    .recycling_template(RecyclingTemplate::Enabled)
                    .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                    .spacing(8)
                    .padding((16, 8))
                    .assign(&mut app.cells.feed)
                    .items(|_app: &App| ITEMS, contact_cell, bind_contact),
            ),
        // Jump toolbar: wraps on narrow windows.
        SkiaWrap::new()
            .spacing(6)
            .margin((8, 0, 8, 36))
            .horizontal_options(LayoutOptions::Center)
            .vertical_options(LayoutOptions::End)
            .children((
                jump_button("HOME", |app, cx| jump(app, cx, 0, RelativePositionType::Start)),
                jump_button("BACKWARD", |app, cx| jump(app, cx, first_visible(app, cx).saturating_sub(5), RelativePositionType::Start)),
                jump_button("MIDDLE", |app, cx| jump(app, cx, ITEMS / 2, RelativePositionType::Start)),
                jump_button("FORWARD", |app, cx| jump(app, cx, first_visible(app, cx) + 5, RelativePositionType::Start)),
                jump_button("END", |app, cx| jump(app, cx, ITEMS, RelativePositionType::End)),
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
            .observe(|me, app: &App| me.set_text(app.cells.debug.as_str())),
    ))
}

/// The debug line: the list's DebugString, the frame time and the frames per second.
fn refresh(app: &mut App, cx: &mut Cx) {
    let list = cx.find::<SkiaLayout>(app.cells.feed).map(|feed| feed.debug_string()).unwrap_or_default();
    let stats = cx.frame_stats();
    app.cells.debug = format!("{list} · {:.1} ms · {} fps", stats.frame_time_ms, stats.fps);
}

fn jump_button(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .font_size(12)
        .background_color(hex(0x0D6EFD))
        .width_request(104)
        .on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

fn jump(app: &App, cx: &mut Cx, index: usize, position: RelativePositionType) {
    cx.scroll_to_index(app.cells.scroll, index.min(ITEMS - 1), position, 400);
}

fn first_visible(app: &App, cx: &Cx) -> usize {
    cx.find::<SkiaLayout>(app.cells.feed).and_then(|feed| feed.visible_items()).map_or(0, |rows| rows.0)
}

/// The labels of one cell that a bind writes.
#[derive(Default)]
struct ContactHandles {
    initials: Handle<SkiaLabel>,
    title: Handle<SkiaLabel>,
    subtitle: Handle<SkiaLabel>,
}

/// Recycled cell, the DrawnUI way: visuals built once here, `bind_contact` runs on every rebind.
fn contact_cell() -> (Build<SkiaLayout>, ContactHandles) {
    let mut handles = ContactHandles::default();
    // Absolute layout: avatar at the start, text column fills the remaining width. A base
    // SkiaLayout does not fill (only the aliases do), so the cell says it.
    let cell = SkiaLayout::new()
        .fill_x()
        .padding((12, 10))
        .background_color(hex(0x111827))
        // One bitmap per cell, blitted while scrolling; the ripple is drawn over it.
        .use_cache(CacheType::Image)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .on_tapped(|me, app: &mut App, _cx| {
            if let Some(index) = me.base().context_index {
                app.cells.last_tapped = Some(index + 1);
            }
        })
        .children((
            SkiaShape::new()
                .shape_type(ShapeType::Circle)
                .width_request(42)
                .lock_ratio(1)
                .vertical_options(LayoutOptions::Center)
                .background_color(hex(0x1F2937))
                .children(SkiaLabel::new("").font_size(14).text_color(hex(0x67E8F9)).center().assign(&mut handles.initials)),
            // Avatar 42 + gap 12. The labels get the remaining width, so MaxLines can truncate.
            SkiaStack::new().spacing(3).vertical_options(LayoutOptions::Center).margin((54, 0, 0, 0)).children((
                SkiaLabel::new("").font_size(15).text_color(Color::WHITE).assign(&mut handles.title),
                SkiaLabel::new("").font_size(12).text_color(hex(0x94A3B8)).max_lines(1).assign(&mut handles.subtitle),
            )),
        ));
    (cell, handles)
}

fn bind_contact(cell: &ContactHandles, _app: &App, index: usize, cx: &mut Cx) {
    let i = index + 1;
    set_text(cx, cell.initials, (i % 100).to_string());
    set_text(cx, cell.title, format!("Contact {i}"));
    set_text(cx, cell.subtitle, format!("Recycled drawn cell #{i} — scroll me fast"));
}
