//! Drag to reorder: a templated list reordered in place while the row being dragged floats over
//! it, Android's language preferences style. Ported from the React demo's ReorderPage.tsx and
//! ReorderCell.ts.
//!
//! The grabbed row is lifted into a ghost, a copy of the row above the scroll that follows the
//! pointer, while the real row draws blank: the gap that travels through the list is the slot
//! the row lands in. Every step is one `items_moved`, which keeps the measured heights and the
//! scroll offset, so the list reorders live under the pointer. On release the ghost glides into
//! the slot and hands the row back. Held near the top or bottom edge, the list keeps moving.

use drawnui::prelude::*;

use super::set_text;
use crate::{App, hex};

const COLORS: [u32; 7] = [0x0D6EFD, 0x6610F2, 0xD63384, 0xFD7E14, 0x20C997, 0x0DCAF0, 0xFFC107];

/// Android's language preferences, in their own name and locale tag. Latin, Cyrillic and Greek
/// only: the app registers OpenSans and nothing else for them.
const LANGUAGES: [(&str, &str); 50] = [
    ("English (United States)", "en-US"),
    ("Español (España)", "es-ES"),
    ("Français (France)", "fr-FR"),
    ("Deutsch (Deutschland)", "de-DE"),
    ("Italiano (Italia)", "it-IT"),
    ("Português (Brasil)", "pt-BR"),
    ("Nederlands (Nederland)", "nl-NL"),
    ("Svenska (Sverige)", "sv-SE"),
    ("Norsk bokmål (Norge)", "nb-NO"),
    ("Dansk (Danmark)", "da-DK"),
    ("Suomi (Suomi)", "fi-FI"),
    ("Íslenska (Ísland)", "is-IS"),
    ("Polski (Polska)", "pl-PL"),
    ("Čeština (Česko)", "cs-CZ"),
    ("Slovenčina (Slovensko)", "sk-SK"),
    ("Magyar (Magyarország)", "hu-HU"),
    ("Română (România)", "ro-RO"),
    ("Hrvatski (Hrvatska)", "hr-HR"),
    ("Slovenščina (Slovenija)", "sl-SI"),
    ("Bosanski (Bosna i Hercegovina)", "bs-BA"),
    ("Shqip (Shqipëri)", "sq-AL"),
    ("Lietuvių (Lietuva)", "lt-LT"),
    ("Latviešu (Latvija)", "lv-LV"),
    ("Eesti (Eesti)", "et-EE"),
    ("Русский (Россия)", "ru-RU"),
    ("Українська (Україна)", "uk-UA"),
    ("Беларуская (Беларусь)", "be-BY"),
    ("Български (България)", "bg-BG"),
    ("Македонски (Македонија)", "mk-MK"),
    ("Српски (Србија)", "sr-RS"),
    ("Қазақша (Қазақстан)", "kk-KZ"),
    ("Кыргызча (Кыргызстан)", "ky-KG"),
    ("Монгол (Монгол)", "mn-MN"),
    ("Ελληνικά (Ελλάδα)", "el-GR"),
    ("Türkçe (Türkiye)", "tr-TR"),
    ("Azərbaycan (Azərbaycan)", "az-AZ"),
    ("Oʻzbekcha (Oʻzbekiston)", "uz-UZ"),
    ("Català (Espanya)", "ca-ES"),
    ("Galego (España)", "gl-ES"),
    ("Euskara (Espainia)", "eu-ES"),
    ("Gaeilge (Éire)", "ga-IE"),
    ("Gàidhlig (Alba)", "gd-GB"),
    ("Cymraeg (Cymru)", "cy-GB"),
    ("Malti (Malta)", "mt-MT"),
    ("Bahasa Indonesia (Indonesia)", "id-ID"),
    ("Bahasa Melayu (Malaysia)", "ms-MY"),
    ("Filipino (Pilipinas)", "fil-PH"),
    ("Tiếng Việt (Việt Nam)", "vi-VN"),
    ("Kiswahili (Kenya)", "sw-KE"),
    ("Afrikaans (Suid-Afrika)", "af-ZA"),
];

const SPACING: i32 = 6;
const STATUS_HEIGHT: i32 = 58;
/// Milliseconds the released ghost takes to glide into its slot.
const DROP_MS: f32 = 140.0;
/// Points from either end of the viewport where a resting pointer keeps the list moving.
const EDGE_ZONE: f32 = 44.0;
/// Points per tick it moves there.
const EDGE_STEP: f32 = 7.0;
/// Milliseconds between two ticks of a drag (the React page runs one per animation frame).
const TICK_MS: f32 = 16.0;

/// One language: `id` is its place in `LANGUAGES`, plus one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub id: usize,
    pub title: &'static str,
    pub tag: &'static str,
    color: Color,
}

fn initial() -> Vec<Item> {
    LANGUAGES
        .iter()
        .enumerate()
        .map(|(i, (title, tag))| Item { id: i + 1, title, tag, color: hex(COLORS[i % COLORS.len()]) })
        .collect()
}

/// A drag in progress.
struct Drag {
    /// Where the item is now, and where it was picked up.
    index: usize,
    start: usize,
    /// Points the pointer (and the list under it) moved since the last step.
    travel: f32,
    /// A row and the gap below it, points.
    stride: f32,
    /// Canvas pixels.
    pointer_y: f32,
    last_offset: f32,
    /// Points from the ghost's top to the pointer.
    grab_offset: f32,
}

/// What the page keeps.
pub struct State {
    pub(super) items: Vec<Item>,
    pub(super) status: String,
    pub(super) scroll: Handle<SkiaScroll>,
    pub(super) rows: Handle<SkiaLayout>,
    overlay: Handle<SkiaLayout>,
    pub(super) ghost: Handle<SkiaShape>,
    ghost_title: Handle<SkiaLabel>,
    ghost_badge: Handle<SkiaLabel>,
    /// The item the ghost stands in for: its row draws blank.
    pub(super) dragging: Option<usize>,
    drag: Option<Drag>,
    /// The glide of a released ghost, and the index it lands at.
    glide: Option<(AnimationId, usize)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            items: initial(),
            status: "drag a language by its grip, or use the buttons".to_owned(),
            scroll: Handle::default(),
            rows: Handle::default(),
            overlay: Handle::default(),
            ghost: Handle::default(),
            ghost_title: Handle::default(),
            ghost_badge: Handle::default(),
            dragging: None,
            drag: None,
            glide: None,
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    let page = &mut app.reorder;
    SkiaLayer::new().fill().children((
        SkiaStack::new().spacing(2).margin((0, 8, 0, 0)).children((
            SkiaLabel::new("")
                .font_size(13)
                .text_color(hex(0xD3D3D3))
                .horizontal_options(LayoutOptions::Center)
                .observe(|me, app: &App| {
                    me.set_text(format!(
                        "{} languages in order of preference · drag by the grip, hold at an edge to keep going",
                        app.reorder.items.len()
                    ))
                }),
            SkiaLabel::new("")
                .font_size(12)
                .text_color(hex(0x6EA8FE))
                .horizontal_options(LayoutOptions::Center)
                .observe(|me, app: &App| me.set_text(app.reorder.status.as_str())),
        )),
        // Each row is its own bitmap; the list and the scroll are not cached.
        SkiaScroll::new()
            .fill()
            .margin((0, STATUS_HEIGHT, 0, 44))
            .assign(&mut page.scroll)
            .on_scrolled(|_me, app: &mut App, cx, _offset| report(app, cx, "scrolled"))
            .content(
                SkiaStack::new()
                    .recycling_template(RecyclingTemplate::Enabled)
                    .measure_items_strategy(MeasuringStrategy::MeasureFirst)
                    .spacing(SPACING)
                    .padding((12, 8))
                    .assign(&mut page.rows)
                    .items(|app: &App| app.reorder.items.len(), row, bind),
            ),
        // The lifted row lives here, above the scroll that clips the list, and never takes the
        // drag it is showing.
        SkiaLayer::new().fill().input_transparent(true).assign(&mut page.overlay).children(
            SkiaShape::new()
                .corner_radius(8)
                .background_color(hex(0x1D4ED8))
                .stroke_width(2)
                .is_visible(false)
                .shadows(SkiaShadow::new(Color::BLACK).x(0).y(6).blur(12).opacity(0.5))
                .assign(&mut page.ghost)
                .children((
                    grip(hex(0xBFDBFE)),
                    SkiaLabel::new("")
                        .font_size(14)
                        .text_color(Color::WHITE)
                        .vertical_options(LayoutOptions::Center)
                        .margin((46, 0, 60, 0))
                        .assign(&mut page.ghost_title),
                    SkiaLabel::new("")
                        .font_size(12)
                        .text_color(hex(0xBFDBFE))
                        .horizontal_options(LayoutOptions::End)
                        .vertical_options(LayoutOptions::Center)
                        .margin((0, 0, 14, 0))
                        .assign(&mut page.ghost_badge),
                )),
        ),
        SkiaWrap::new()
            .spacing(6)
            .margin((8, 0, 8, 8))
            .horizontal_options(LayoutOptions::Center)
            .vertical_options(LayoutOptions::End)
            .children((
                button("1st below 10th", |app, cx| {
                    move_item(app, cx, 0, 9);
                    report(app, cx, "moved 1st below 10th");
                }),
                button("Reverse", |app, cx| {
                    app.reorder.items.reverse();
                    let n = app.reorder.items.len();
                    cx.items_reordered(app.reorder.rows, (0..n).rev().collect());
                    report(app, cx, "reversed");
                }),
                button("Reset", |app, cx| {
                    // order[i] = where the item that goes back to i is now.
                    let now = &app.reorder.items;
                    let order = (1..=now.len()).map(|id| now.iter().position(|item| item.id == id).unwrap_or(0)).collect();
                    app.reorder.items = initial();
                    cx.items_reordered(app.reorder.rows, order);
                    report(app, cx, "reset");
                }),
            )),
    ))
}

fn button(caption: &str, action: fn(&mut App, &mut Cx)) -> Build<SkiaButton> {
    SkiaButton::new(caption).font_size(13).background_color(hex(0x495057)).on_tapped(move |_me, app: &mut App, cx| action(app, cx))
}

/// The three grip bars, shared by a row and by the ghost that stands in for it.
fn grip(color: Color) -> Build<SkiaLayout> {
    // Absolute: a stack would pack the bars at the top of the row. The whole row height is
    // grabbable, the bars sit in the middle.
    SkiaLayer::new()
        .width_request(26)
        .horizontal_options(LayoutOptions::Start)
        .vertical_options(LayoutOptions::Fill)
        .margin((10, 0, 0, 0))
        .children(
            SkiaStack::new().spacing(3).vertical_options(LayoutOptions::Center).children(
                (0..3)
                    .map(|_| {
                        SkiaShape::new()
                            .corner_radius(1)
                            .height_request(2)
                            .width_request(16)
                            .background_color(color)
                            .horizontal_options(LayoutOptions::Center)
                    })
                    .collect::<Vec<_>>(),
            ),
        )
}

/// The controls of one row that a bind writes.
#[derive(Default)]
struct RowHandles {
    cell: Handle<SkiaLayout>,
    frame: Handle<SkiaShape>,
    grip: Handle<SkiaLayout>,
    title: Handle<SkiaLabel>,
    badge: Handle<SkiaLabel>,
}

/// One row: a grip, a caption and the locale tag.
fn row() -> (Build<SkiaLayout>, RowHandles) {
    let mut handles = RowHandles::default();
    let cell = SkiaLayout::new()
        .height_request(44)
        .fill_x()
        // One bitmap per row, blitted while scrolling.
        .use_cache(CacheType::Image)
        .assign(&mut handles.cell);
    let row = handles.cell;
    let cell = cell.children((
            SkiaShape::new().corner_radius(8).fill().background_color(hex(0x111827)).stroke_width(1).assign(&mut handles.frame),
            // The drag lives on the grip: it takes the press, so the list's scroll never sees this pan.
            grip(hex(0x94A3B8))
                .accessibility_role(Aria::BUTTON)
                .assign(&mut handles.grip)
                .consume_gestures(move |me, app: &mut App, cx, gesture| drag(app, cx, row, me.base().scale, gesture)),
            SkiaLabel::new("")
                .font_size(14)
                .text_color(Color::WHITE)
                .vertical_options(LayoutOptions::Center)
                .margin((46, 0, 60, 0))
                .assign(&mut handles.title),
            SkiaLabel::new("")
                .font_size(12)
                .text_color(hex(0x94A3B8))
                .horizontal_options(LayoutOptions::End)
                .vertical_options(LayoutOptions::Center)
                .margin((0, 0, 14, 0))
                .assign(&mut handles.badge),
        ));
    (cell, handles)
}

fn bind(row: &RowHandles, app: &App, index: usize, cx: &mut Cx) {
    let item = app.reorder.items[index];
    set_text(cx, row.title, item.title);
    set_text(cx, row.badge, item.tag);
    if let Some(mut frame) = cx.get_mut(row.frame) {
        frame.set_stroke_color(item.color);
    }
    if let Some(mut grip) = cx.get_mut(row.grip) {
        grip.set_accessibility_label(format!("Reorder {}", item.title));
    }
    // The ghost stands in for this row: it leaves the gap the ghost drops into.
    if let Some(mut cell) = cx.get_mut(row.cell) {
        cell.set_opacity(if app.reorder.dragging == Some(item.id) { 0 } else { 1 });
    }
}

/// Writes the new order: the item at `from` goes to `to`. False when either is outside the list.
fn move_item(app: &mut App, cx: &mut Cx, from: usize, to: usize) -> bool {
    let items = &mut app.reorder.items;
    if from >= items.len() || to >= items.len() {
        return false;
    }
    let item = items.remove(from);
    items.insert(to, item);
    cx.items_moved(app.reorder.rows, from, to);
    true
}

fn report(app: &mut App, cx: &mut Cx, what: &str) {
    let offset = cx.find::<SkiaScroll>(app.reorder.scroll).map_or(0.0, |scroll| scroll.viewport_offset_y());
    let tags = app.reorder.items.iter().take(5).map(|item| item.tag).collect::<Vec<_>>().join(", ");
    app.reorder.status = format!("{what} · offset {offset:.0} pt · {tags}…");
}

/// Pixels per point, and the overlay's top-left in canvas pixels: where the ghost is laid out.
fn overlay_space(app: &App, cx: &Cx) -> (f32, Point) {
    cx.base(app.reorder.overlay).map_or((1.0, Point::default()), |b| (b.scale.max(0.1), Point::new(b.rect.left, b.rect.top)))
}

/// The grip's gestures (React ConsumeGestures): Down lifts the row of the cell, Panning carries
/// it, Up drops it. True = taken.
fn drag(app: &mut App, cx: &mut Cx, cell: Handle<SkiaLayout>, scale: f32, gesture: &Gesture) -> bool {
    match gesture.kind {
        GestureKind::Down => {
            let Some(index) = cx.base(cell).and_then(|b| b.context_index) else { return false };
            lift(app, cx, index, gesture.location.y);
            app.reorder.drag.is_some()
        }
        GestureKind::Panning if app.reorder.drag.is_some() => {
            pan(app, gesture.location.y, gesture.delta.y / scale.max(0.1));
            true
        }
        GestureKind::Up if app.reorder.drag.is_some() => {
            release(app, cx, gesture.location.x, gesture.location.y);
            true
        }
        _ => false,
    }
}

/// Picks up the row of item `index` under the pointer (`pointer_y`, canvas pixels): the ghost
/// takes its place and the real row goes blank (the grip's Down).
fn lift(app: &mut App, cx: &mut Cx, index: usize, pointer_y: f32) {
    // A drop still gliding lands at once.
    if let Some((glide, at)) = app.reorder.glide.take() {
        cx.stop_animation(glide);
        land(app, cx, at);
    }
    let Some(rect) = cx.item_rect(app.reorder.rows, index) else { return };
    let (scale, origin) = overlay_space(app, cx);
    let item = app.reorder.items[index];
    let page = &mut app.reorder;
    page.dragging = Some(item.id);
    set_text(cx, page.ghost_title, item.title);
    set_text(cx, page.ghost_badge, item.tag);
    if let Some(mut ghost) = cx.get_mut(page.ghost) {
        ghost.set_stroke_color(item.color);
        ghost.set_width_request(rect.width() / scale);
        ghost.set_height_request(rect.height() / scale);
        ghost.set_left((rect.left - origin.x) / scale);
        ghost.set_top((rect.top - origin.y) / scale);
        ghost.set_is_visible(true);
    }
    // Blanks the row the ghost now stands in for.
    cx.items_changed(page.rows, index);
    let last_offset = cx.find::<SkiaScroll>(page.scroll).map_or(0.0, |scroll| scroll.viewport_offset_y());
    // The pan is the grip's for the whole drag.
    if let Some(mut scroll) = cx.get_mut(page.scroll) {
        scroll.set_responds_to_gestures(false);
    }
    page.drag = Some(Drag {
        index,
        start: index,
        travel: 0.0,
        stride: rect.height() / scale + SPACING as f32,
        pointer_y,
        last_offset,
        grab_offset: (pointer_y - rect.top) / scale,
    });
    cx.after(page.ghost, TICK_MS, tick);
    report(app, cx, "picked up");
}

/// The pointer moved during a drag (the grip's Panning): canvas pixels, and the move in points.
fn pan(app: &mut App, pointer_y: f32, delta_points: f32) {
    if let Some(drag) = app.reorder.drag.as_mut() {
        drag.pointer_y = pointer_y;
        drag.travel += delta_points;
    }
}

/// The pointer was released at (x, y), canvas pixels (the grip's Up): outside the list the row
/// goes back to where it was picked up; the ghost glides into the slot.
fn release(app: &mut App, cx: &mut Cx, x: f32, y: f32) {
    let Some(drag) = app.reorder.drag.take() else { return };
    let outside = cx.base(app.reorder.scroll).is_some_and(|b| x < b.rect.left || x > b.rect.right || y < b.rect.top || y > b.rect.bottom);
    let mut index = drag.index;
    if outside && index != drag.start && move_item(app, cx, index, drag.start) {
        index = drag.start;
    }
    if let Some(mut scroll) = cx.get_mut(app.reorder.scroll) {
        scroll.set_responds_to_gestures(true);
    }
    let (rows, ghost) = (app.reorder.rows, app.reorder.ghost);
    let from = cx.base(ghost).map_or(0.0, |b| b.p.top);
    let (scale, origin) = overlay_space(app, cx);
    // Eased out, so it lands rather than stops. The slot is read every frame: the list may still
    // be catching up with the last move.
    let glide = cx.start_animator(ghost, ValueAnimator::new(0.0, 1.0, DROP_MS, easing::cubic_out), move |progress, cx| {
        let to = cx.item_rect(rows, index).map_or(from, |rect| (rect.top - origin.y) / scale);
        if let Some(mut ghost) = cx.get_mut(ghost) {
            ghost.set_top(from + (to - from) * progress);
        }
    });
    cx.on_finished(glide, move |app: &mut App, cx| {
        app.reorder.glide = None;
        land(app, cx, index);
    });
    app.reorder.glide = Some((glide, index));
}

/// The ghost landed at `index`: it hides and the real row draws itself again.
fn land(app: &mut App, cx: &mut Cx, index: usize) {
    app.reorder.dragging = None;
    if let Some(mut ghost) = cx.get_mut(app.reorder.ghost) {
        ghost.set_is_visible(false);
    }
    cx.items_changed(app.reorder.rows, index);
    report(app, cx, "dropped");
}

/// One tick of a drag: held at an edge the list keeps moving, so the row keeps advancing; the
/// ghost follows the pointer; every stride of travel is one step through the list.
fn tick(app: &mut App, cx: &mut Cx) {
    let (scale, origin) = overlay_space(app, cx);
    let (scroll, ghost) = (app.reorder.scroll, app.reorder.ghost);
    let Some(drag) = app.reorder.drag.as_mut() else { return };
    if let Some(viewport) = cx.base(scroll).map(|b| b.rect) {
        let zone = EDGE_ZONE * scale;
        let direction = if drag.pointer_y < viewport.top + zone {
            1.0
        } else if drag.pointer_y > viewport.bottom - zone {
            -1.0
        } else {
            0.0
        };
        let offset_of = |cx: &Cx| cx.find::<SkiaScroll>(scroll).map_or(0.0, |scroll| scroll.viewport_offset_y());
        if direction != 0.0 {
            let offset = offset_of(cx);
            cx.scroll_to(scroll, 0.0, offset + direction * EDGE_STEP, 0);
        }
        // The list moving under the pointer advances the row as well.
        let offset = offset_of(cx);
        drag.travel += drag.last_offset - offset;
        drag.last_offset = offset;
    }
    let top = (drag.pointer_y - origin.y) / scale - drag.grab_offset;
    if let Some(mut ghost) = cx.get_mut(ghost) {
        ghost.set_top(top);
    }
    loop {
        let Some(drag) = app.reorder.drag.as_ref() else { return };
        if drag.stride <= 0.0 || drag.travel.abs() < drag.stride {
            break;
        }
        let step: isize = if drag.travel > 0.0 { 1 } else { -1 };
        let (from, stride) = (drag.index, drag.stride);
        let moved = from.checked_add_signed(step).is_some_and(|to| move_item(app, cx, from, to));
        let drag = app.reorder.drag.as_mut().expect("the drag");
        if !moved {
            // Hit an end.
            drag.travel = 0.0;
            break;
        }
        drag.index = from.wrapping_add_signed(step);
        drag.travel -= step as f32 * stride;
        report(app, cx, "dragging");
    }
    cx.after(ghost, TICK_MS, tick);
}
