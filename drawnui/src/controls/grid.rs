//! LayoutType::Grid: columns and rows sized by their definitions, children placed in cells by
//! their `column`, `row`, `column_span` and `row_span`. A port of upstream `SkiaGridStructure` and
//! `MeasureGrid`.

use skia_safe::{Rect, Size};

use super::{Kids, LayoutProps, arrange_kid, measure_kid};
use crate::control::LayoutCx;
use crate::layout::{desired, fills_height, fills_width, snap};
use crate::tree::ControlId;
use crate::types::IntoProp;

/// The size of a grid column or row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridLength {
    /// Points.
    Absolute(f32),
    /// As large as the largest child in it.
    Auto,
    /// A share, by this weight, of what the other tracks leave.
    Star(f32),
}

impl GridLength {
    pub const STAR: GridLength = GridLength::Star(1.0);

    /// One item of the upstream string form: `100`, `Auto`, `*`, `2*`.
    fn parse(text: &str) -> GridLength {
        let text = text.trim();
        let number = |text: &str| text.trim().parse::<f32>().ok();
        let parsed = match text.strip_suffix('*') {
            Some("") => Some(GridLength::STAR),
            Some(weight) => number(weight).map(GridLength::Star),
            None if text.eq_ignore_ascii_case("auto") => Some(GridLength::Auto),
            None => number(text).map(GridLength::Absolute),
        };
        parsed.unwrap_or_else(|| panic!("grid length `{text}`: expected a number, `Auto`, `*` or a weight like `2*`"))
    }
}

impl IntoProp<GridLength> for &str {
    fn into_prop(self) -> GridLength {
        GridLength::parse(self)
    }
}

/// The upstream string form: `"*, 2*, Auto, 100"`.
impl IntoProp<Vec<GridLength>> for &str {
    fn into_prop(self) -> Vec<GridLength> {
        self.split(',').map(GridLength::parse).collect()
    }
}

impl<const N: usize> IntoProp<Vec<GridLength>> for [GridLength; N] {
    fn into_prop(self) -> Vec<GridLength> {
        self.to_vec()
    }
}

/// A column or a row.
#[derive(Clone, Copy)]
struct Track {
    length: GridLength,
    /// Points, as upstream keeps its tracks: a pixel value comes out of them the same way there.
    size: f64,
}

impl Track {
    fn new(length: GridLength) -> Track {
        Track { length, size: if let GridLength::Absolute(points) = length { points as f64 } else { 0.0 } }
    }
    fn is_auto(&self) -> bool {
        self.length == GridLength::Auto
    }
    fn is_star(&self) -> bool {
        matches!(self.length, GridLength::Star(_))
    }
    fn weight(&self) -> f64 {
        if let GridLength::Star(weight) = self.length { weight as f64 } else { 0.0 }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    child: ControlId,
    column: usize,
    row: usize,
    columns: usize,
    rows: usize,
    /// The content measure of the first pass: the pixels offered and the points that came back.
    /// Kept to the next measure, so a child that did not change is not measured for its content
    /// again.
    content: Option<((f32, f32), (f64, f64))>,
}

/// A child over several tracks, one of them Auto, that asks for this much.
struct Span {
    start: usize,
    length: usize,
    is_column: bool,
    requested: f64,
}

#[derive(Default)]
pub(crate) struct Grid {
    columns: Vec<Track>,
    rows: Vec<Track>,
    cells: Vec<Cell>,
    /// The cells of the measure before.
    previous: Vec<Cell>,
    spans: Vec<Span>,
}

/// The tracks a measure starts with: the definitions, or one default track; `split` columns of
/// equal share when there are no (or fewer) column definitions.
fn start(tracks: &mut Vec<Track>, definitions: &[GridLength], default: GridLength, split: i32) {
    tracks.clear();
    tracks.extend(definitions.iter().map(|length| Track::new(*length)));
    let split = if definitions.is_empty() || split > 1 { split.max(0) as usize } else { 0 };
    while tracks.len() < split {
        tracks.push(Track::new(GridLength::STAR));
    }
    if tracks.is_empty() {
        tracks.push(Track::new(default));
    }
}

/// All tracks and the gaps between them.
fn total(tracks: &[Track], gap: f64) -> f64 {
    // Added up one by one in upstream's order: another order can end on the other side of a half pixel.
    let mut sum = 0.0;
    for (index, track) in tracks.iter().enumerate() {
        sum += track.size;
        if index > 0 {
            sum += gap;
        }
    }
    sum
}

/// `length` tracks from `start`, then the gaps between them.
fn extent(tracks: &[Track], start: usize, length: usize, gap: f64) -> f64 {
    tracks[start..start + length].iter().map(|track| track.size).sum::<f64>() + (length - 1) as f64 * gap
}

/// Where a track begins.
fn edge(tracks: &[Track], index: usize, gap: f64) -> f64 {
    let mut edge = 0.0;
    for track in &tracks[..index] {
        edge += track.size;
        edge += gap;
    }
    edge
}

/// What the tracks of a cell are: (one is Auto, one is star, all are absolute).
fn kinds(tracks: &[Track]) -> (bool, bool, bool) {
    let (auto, star) = (tracks.iter().any(Track::is_auto), tracks.iter().any(Track::is_star));
    (auto, star, !auto && !star)
}

/// The room a cell has while the tracks are not final: its own tracks plus what the grid has left.
fn room(tracks: &[Track], start: usize, length: usize, gap: f64, constraint: f64) -> f64 {
    let own = extent(tracks, start, length, gap);
    if kinds(&tracks[start..start + length]).2 { own } else { constraint - total(tracks, gap) + own }
}

/// Star tracks share what the others leave, by weight. On an unbounded axis a star means nothing:
/// the tracks take `content`, the largest child in a star cell, times their weight.
fn resolve_stars(tracks: &mut [Track], gap: f64, constraint: f64, content: f64) {
    for track in tracks.iter_mut().filter(|track| track.is_star()) {
        track.size = 0.0;
    }
    let weights: f64 = tracks.iter().map(Track::weight).sum();
    if weights > 0.0 {
        // ponytail: an overfull grid gives its stars nothing; upstream gives them a negative size.
        let share = if constraint.is_finite() { ((constraint - total(tracks, gap)) / weights).max(0.0) } else { content };
        for track in tracks.iter_mut().filter(|track| track.is_star()) {
            track.size = share * track.weight();
        }
    }
}

/// A child over several tracks asks for more than they have: the Auto ones share what is missing.
/// Nothing happens when a star track is among them, that one takes it.
fn resolve_span(tracks: &mut [Track], gap: f64, requested: f64) {
    let missing = requested - total(tracks, gap);
    let autos = tracks.iter().filter(|track| track.is_auto()).count();
    if missing <= 0.0 || autos == 0 || kinds(tracks).1 {
        return;
    }
    for track in tracks.iter_mut().filter(|track| track.is_auto()) {
        track.size += missing / autos as f64;
    }
}

impl Grid {
    /// Sizes the tracks and measures every child at its cell. `w` and `h` are the box inside the
    /// padding, pixels; `auto` says which sides of the grid take the size of its tracks.
    /// Returns the size of the tracks and the gaps, whole pixels.
    pub(crate) fn measure(&mut self, cx: &mut LayoutCx, p: &LayoutProps, kids: &mut Kids, w: f32, h: f32, auto: (bool, bool)) -> Size {
        let scale = cx.scale;
        // Points and back, the way upstream goes: through `f32` at the ends, `f64` in between.
        let points = |pixels: f32| (pixels / scale) as f64;
        let pixels = |points: f64| (points * scale as f64) as f32;
        let (column_gap, row_gap) = (p.column_spacing as f64, p.row_spacing as f64);
        let (width, height) = (points(w), points(h));
        let Grid { columns, rows, cells, previous, spans } = self;
        start(columns, &p.column_definitions, p.default_column_definition, p.split);
        start(rows, &p.row_definitions, p.default_row_definition, 0);

        // The cells; a child beyond the definitions makes tracks of the default kind.
        std::mem::swap(cells, previous);
        cells.clear();
        for i in 0..cx.child_count() {
            let child = cx.child(i);
            let cp = &cx.child_base(child).p;
            if !cp.is_visible {
                continue;
            }
            let (column, row) = (cp.column.max(0) as usize, cp.row.max(0) as usize);
            let (across, down) = (cp.column_span.max(1) as usize, cp.row_span.max(1) as usize);
            while columns.len() < column + across {
                columns.push(Track::new(p.default_column_definition));
            }
            while rows.len() < row + down {
                rows.push(Track::new(p.default_row_definition));
            }
            cells.push(Cell { child, column, row, columns: across, rows: down, content: None });
        }

        // Pass 1, content: a child sizes a track through an Auto track, or through a star one on
        // an unbounded axis. Every other child waits for its final cell.
        spans.clear();
        // The largest child in a star cell, per side: what a star is on an unbounded axis.
        let mut in_stars = (0f64, 0f64);
        for (index, cell) in cells.iter_mut().enumerate() {
            let (column_auto, column_star, _) = kinds(&columns[cell.column..cell.column + cell.columns]);
            let (row_auto, row_star, _) = kinds(&rows[cell.row..cell.row + cell.rows]);
            if !(column_auto || row_auto || (column_star && !w.is_finite()) || (row_star && !h.is_finite())) {
                continue;
            }
            let room_w = room(columns, cell.column, cell.columns, column_gap, width);
            let room_h = room(rows, cell.row, cell.rows, row_gap, height);
            // A Fill child would take all the room and make its Auto track that large. It is
            // measured unbounded instead, and the track takes its content, cut to the room.
            let base = cx.child_base(cell.child);
            let (unbound_w, unbound_h) = (column_auto && fills_width(&base.p), row_auto && fills_height(&base.p));
            let offer_w = if unbound_w { f32::INFINITY } else { pixels(room_w).max(0.0) };
            let offer_h = if unbound_h { f32::INFINITY } else { pixels(room_h).max(0.0) };
            let same = |a: f32, b: f32| a.to_bits() == b.to_bits();
            let kept = previous
                .get(index)
                .filter(|old| old.child == cell.child && !base.need_measure && base.scale == scale)
                .and_then(|old| old.content)
                .filter(|(offered, _)| same(offered.0, offer_w) && same(offered.1, offer_h));
            let size = match kept {
                Some((_, size)) => size,
                None => {
                    measure_kid(kids, cx, cell.child, offer_w, offer_h);
                    // In points, from the size before it was rounded to pixels, as upstream takes it.
                    let raw = desired(cx.child_base(cell.child));
                    (points(raw.width), points(raw.height))
                }
            };
            cell.content = Some(((offer_w, offer_h), size));
            if column_star {
                in_stars.0 = in_stars.0.max(size.0);
            }
            if row_star {
                in_stars.1 = in_stars.1.max(size.1);
            }
            let requested_w = if unbound_w && room_w.is_finite() && room_w >= 0.0 { size.0.min(room_w) } else { size.0 };
            let requested_h = if unbound_h && room_h.is_finite() && room_h >= 0.0 { size.1.min(room_h) } else { size.1 };
            let mut track = |tracks: &mut [Track], start: usize, length: usize, is_column: bool, requested: f64| {
                if length == 1 {
                    tracks[start].size = tracks[start].size.max(requested);
                } else if let Some(span) = spans.iter_mut().find(|s| (s.start, s.length, s.is_column) == (start, length, is_column)) {
                    span.requested = span.requested.max(requested);
                } else {
                    spans.push(Span { start, length, is_column, requested });
                }
            };
            if column_auto {
                track(columns, cell.column, cell.columns, true, requested_w);
            }
            if row_auto {
                track(rows, cell.row, cell.rows, false, requested_h);
            }
        }

        for span in spans.iter() {
            let (tracks, gap) = if span.is_column { (&mut *columns, column_gap) } else { (&mut *rows, row_gap) };
            resolve_span(&mut tracks[span.start..span.start + span.length], gap, span.requested);
        }

        // A Fill child with a minimum keeps its tracks at least that large.
        for cell in cells.iter() {
            let cp = &cx.child_base(cell.child).p;
            if fills_width(cp) && cp.minimum_width_request >= 0.0 {
                let minimum = (cp.minimum_width_request + cp.margin.horizontal()) as f64 / cell.columns as f64;
                for track in &mut columns[cell.column..cell.column + cell.columns] {
                    track.size = track.size.max(minimum);
                }
            }
            if fills_height(cp) && cp.minimum_height_request >= 0.0 {
                let minimum = (cp.minimum_height_request + cp.margin.vertical()) as f64 / cell.rows as f64;
                for track in &mut rows[cell.row..cell.row + cell.rows] {
                    track.size = track.size.max(minimum);
                }
            }
        }

        // Stars, then the last track of a grid that does not take the size of its tracks: it
        // takes what is left. Never on an unbounded axis.
        resolve_stars(columns, column_gap, width, in_stars.0);
        resolve_stars(rows, row_gap, height, in_stars.1);
        if !auto.0 && width.is_finite() && total(columns, column_gap) < width {
            columns.last_mut().expect("a grid has a column").size += width - total(columns, column_gap);
        }
        if !auto.1 && height.is_finite() && total(rows, row_gap) < height {
            rows.last_mut().expect("a grid has a row").size += height - total(rows, row_gap);
        }

        // Pass 2: every child at the cell it is arranged in, so what is inside it is laid out for
        // that box. A child that comes back larger than its Auto track (wrapped text) grows it.
        let mut auto_row_grew = false;
        for cell in cells.iter() {
            let cell_width = extent(columns, cell.column, cell.columns, column_gap);
            let cell_height = extent(rows, cell.row, cell.rows, row_gap);
            if cell_width <= 0.0 || cell_height <= 0.0 {
                continue;
            }
            // A child on a single Auto row is offered the height the row can still grow to (C#
            // 7cf1007c): the row's size came from pass 1 at the grid's whole width, so text wrapping
            // in a star column would be cut to that one-line height. A vertical Fill child, which
            // would take all of it, is measured unbounded; the row grows only within the room.
            let on_auto_row = cell.rows == 1 && rows[cell.row].is_auto();
            let room_h = room(rows, cell.row, 1, row_gap, height);
            let offer = match on_auto_row {
                true if fills_height(&cx.child_base(cell.child).p) => f64::INFINITY,
                true => cell_height.max(room_h),
                false => cell_height,
            };
            // Upstream rounds the height it offers to a pixel and leaves the width as it is.
            let offer_h = if offer.is_finite() { (offer * scale as f64).round_ties_even() as f32 } else { f32::INFINITY };
            measure_kid(kids, cx, cell.child, pixels(cell_width), offer_h);
            let raw = desired(cx.child_base(cell.child));
            if cell.columns == 1 && columns[cell.column].is_auto() {
                columns[cell.column].size = columns[cell.column].size.max(points(raw.width));
            }
            if on_auto_row {
                let grown = if room_h.is_finite() { points(raw.height).min(room_h) } else { points(raw.height) };
                if grown > rows[cell.row].size {
                    rows[cell.row].size = grown;
                    auto_row_grew = true;
                }
            }
        }
        // Star rows were shared out against the smaller Auto rows: what is left now, if anything.
        if auto_row_grew && height.is_finite() {
            resolve_stars(rows, row_gap, height, in_stars.1);
        }
        Size::new(snap(pixels(total(columns, column_gap))), snap(pixels(total(rows, row_gap))))
    }

    pub(crate) fn columns(&self) -> usize {
        self.columns.len()
    }

    pub(crate) fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The gap before column `index` in pixels, from `left` (the corner inside the padding,
    /// pixels): its start and end, computed as the cells are.
    pub(crate) fn column_gap(&self, index: usize, gap: f32, left: f32, scale: f32) -> (f32, f32) {
        Self::gap_before(&self.columns, index, gap, left, scale)
    }

    /// The gap before row `index`; see `column_gap`.
    pub(crate) fn row_gap(&self, index: usize, gap: f32, top: f32, scale: f32) -> (f32, f32) {
        Self::gap_before(&self.rows, index, gap, top, scale)
    }

    fn gap_before(tracks: &[Track], index: usize, gap: f32, from: f32, scale: f32) -> (f32, f32) {
        let pixels = |points: f64| (points * scale as f64) as f32;
        let end = edge(tracks, index, gap as f64) + (from / scale) as f64;
        (pixels(end - gap as f64), pixels(end))
    }

    /// Places every child in its cell. `left` and `top` are the corner inside the padding, pixels.
    pub(crate) fn arrange(&self, cx: &mut LayoutCx, p: &LayoutProps, kids: &mut Kids, left: f32, top: f32) {
        let scale = cx.scale;
        let pixels = |points: f64| (points * scale as f64) as f32;
        let (column_gap, row_gap) = (p.column_spacing as f64, p.row_spacing as f64);
        let (left, top) = ((left / scale) as f64, (top / scale) as f64);
        for cell in &self.cells {
            let x = edge(&self.columns, cell.column, column_gap) + left;
            let y = edge(&self.rows, cell.row, row_gap) + top;
            let right = x + extent(&self.columns, cell.column, cell.columns, column_gap);
            let bottom = y + extent(&self.rows, cell.row, cell.rows, row_gap);
            arrange_kid(kids, cx, cell.child, Rect::new(pixels(x), pixels(y), pixels(right), pixels(bottom)));
        }
    }
}
