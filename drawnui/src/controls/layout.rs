//! SkiaLayout: Absolute, Column, Row, Wrap and Grid, plus the SkiaStack / SkiaRow / SkiaLayer / SkiaWrap / SkiaGrid
//! aliases. The grid is in `grid.rs`.
//! A Column with `items` is a list of recycled cells (`list.rs`); a Row, Wrap, Grid or split Column
//! with `items` lays out every item like a child: through slots that views from a pool are bound
//! to while they can be seen (RecyclingTemplate Enabled), or one cell per item (Disabled).

use skia_safe::{Rect, Size};

use crate::control::{Control, Has, LayoutCx};
use crate::layout::{content_box, content_rect, fills_height, fills_width, margins, place, snap};
use crate::props;
use crate::tree::{Base, Build, Container, ControlId, ControlProps};
use crate::types::LayoutOptions;

#[path = "grid.rs"]
mod grid;
#[path = "list.rs"]
mod list;
#[path = "decorated_grid.rs"]
pub mod decorated_grid;

pub use decorated_grid::{DecoratedGridBuild, DecoratedGridSet, SkiaDecoratedGrid};
pub use grid::GridLength;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LayoutType {
    /// Children overlap, each placed by its own layout options.
    #[default]
    Absolute,
    Column,
    Row,
    /// Children flow left to right and go on to a new line when the line is full.
    Wrap,
    /// Columns and rows; a child sits in the cell its `column` and `row` name.
    Grid,
}

/// What a list does with the cell of a row that left the viewport.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RecyclingTemplate {
    /// The cell is bound to the next row that enters: as many cells as rows on screen.
    #[default]
    Enabled,
    /// One cell per item, kept for it and never bound to another item. For small lists.
    Disabled,
}

/// How a list learns the sizes of its rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MeasuringStrategy {
    /// Every item is measured once, up front. For small lists.
    #[default]
    MeasureAll,
    /// Every row takes the size of the first one: no measure on scroll or on append. For uniform rows.
    MeasureFirst,
    /// Rows are measured when they become visible and, inside a budget per frame, ahead of that;
    /// the rest is estimated by the average of the measured ones. For uneven rows.
    MeasureVisible,
}

/// What a MeasureVisible list may spend per frame on rows that are not on screen yet.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MeasureBudget {
    /// Milliseconds; at least one row per frame.
    Millis(f32),
    /// Rows per frame; 0 = only what is visible gets measured.
    Items(u32),
}

props!(LayoutProps, LayoutBuild, LayoutSet {
    /// DrawnUI `Type`.
    layout_type / set_layout_type: LayoutType = LayoutType::Absolute, MEASURE;
    /// Points between children of a Column or a Row, and between the children and the lines of a Wrap.
    spacing / set_spacing: f32 = 8.0, MEASURE;
    /// Wrap: a fixed number of columns, a new line after that many children; 0 = as many as fit.
    /// Column: more than 1 lays the children out in rows of that many equal columns. Grid with
    /// `items`: the columns, item `i` in column `i % split` of row `i / split`.
    split / set_split: i32 = 0, MEASURE;
    /// Wrap or Column with `split`: a last row with fewer children shares the whole width among them.
    dynamic_columns / set_dynamic_columns: bool = false, MEASURE;
    /// Grid with `items` and `split`: the items go down the columns instead of along the rows.
    invert / set_invert: bool = false, MEASURE;
    /// Wrap with `split`: every column is equally wide (true), or the next child follows right
    /// after the width of this one (false).
    split_align / set_split_align: bool = true, MEASURE;
    /// Grid: the columns, as `"*, 2*, Auto, 100"` or a list of `GridLength`. A child beyond them
    /// gets columns of `default_column_definition`. With `split` and no definitions: that many `*`.
    column_definitions / set_column_definitions: Vec<GridLength> = Vec::new(), MEASURE;
    /// Grid: the rows.
    row_definitions / set_row_definitions: Vec<GridLength> = Vec::new(), MEASURE;
    default_column_definition / set_default_column_definition: GridLength = GridLength::Auto, MEASURE;
    default_row_definition / set_default_row_definition: GridLength = GridLength::Auto, MEASURE;
    /// Grid: points between columns.
    column_spacing / set_column_spacing: f32 = 1.0, MEASURE;
    /// Grid: points between rows.
    row_spacing / set_row_spacing: f32 = 1.0, MEASURE;
    recycling_template / set_recycling_template: RecyclingTemplate = RecyclingTemplate::Enabled, MEASURE;
    measure_items_strategy / set_measure_items_strategy: MeasuringStrategy = MeasuringStrategy::MeasureAll, MEASURE;
    /// Most cells a recycled list keeps, on screen and spare; -1 = four times the visible rows.
    item_template_pool_size / set_item_template_pool_size: i32 = -1, MEASURE;
    /// Upstream measures on a background thread; here it is a slice of the frame (4 ms, as its
    /// single-threaded browser build gives to cell preparation).
    measure_budget / set_measure_budget: MeasureBudget = MeasureBudget::Millis(4.0), NONE;
});

#[derive(Default)]
pub struct SkiaLayout {
    pub p: LayoutProps,
    /// The templated part: set by `items`.
    pub(crate) items: Option<Box<list::Items>>,
    /// Wrap: where the last measure put each child, pixels from the padded corner.
    slots: Vec<(ControlId, Rect)>,
    /// Grid: the tracks and cells of the last measure.
    grid: grid::Grid,
    /// From the last measure: the box the children were measured in, the size of the content,
    /// and what each child that fills the main axis of a stack got (endless: nothing to share).
    inner: Size,
    content: Size,
    share: f32,
}

impl SkiaLayout {
    /// Absolute layout that does not fill, like a bare DrawnUI SkiaLayout.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        Build::new(SkiaLayout::default())
    }
    /// Vertical stack filling the width (SkiaStack).
    pub fn column() -> Build<SkiaLayout> {
        Self::new().layout_type(LayoutType::Column).horizontal_options(LayoutOptions::Fill)
    }
    /// Horizontal stack (SkiaRow).
    pub fn row() -> Build<SkiaLayout> {
        Self::new().layout_type(LayoutType::Row)
    }
    /// Overlay filling the width (SkiaLayer).
    pub fn layer() -> Build<SkiaLayout> {
        Self::new().horizontal_options(LayoutOptions::Fill)
    }
    /// Flow of children filling the width (SkiaWrap).
    pub fn wrap() -> Build<SkiaLayout> {
        Self::new().layout_type(LayoutType::Wrap).horizontal_options(LayoutOptions::Fill)
    }
    /// Grid filling the width (SkiaGrid).
    pub fn grid() -> Build<SkiaLayout> {
        Self::new().layout_type(LayoutType::Grid).horizontal_options(LayoutOptions::Fill)
    }

    /// Wrap: measures the children line by line and keeps their slots for `arrange`. `w` and `h`
    /// are the box inside the padding. Returns the widest line and the height of all lines.
    fn measure_wrap(&mut self, cx: &mut LayoutCx, kids: &mut Kids, w: f32, h: f32) -> Size {
        let scale = cx.scale;
        let (raw_gap, gap) = (self.p.spacing * scale, snap(self.p.spacing * scale));
        let split = self.p.split.max(0) as usize;
        // Width of one column of a split wrap.
        let chunk = if split > 1 { column_width(w, split, raw_gap) } else { w };
        let aligned = self.p.split_align && chunk.is_finite();
        let dynamic = split > 1 && self.p.dynamic_columns && w.is_finite();
        let slots = &mut self.slots;
        slots.clear();
        // The line being filled: its first slot, its children so far, where the last one ends in
        // pixels (`x`) and before rounding (`exact`).
        let (mut first, mut columns, mut x, mut exact, mut top, mut line_height) = (0, 0, 0f32, 0f32, 0f32, 0f32);
        let mut content = Size::default();
        // Gives the slots of the line their height and starts the next line.
        let next_line = |slots: &mut Vec<(ControlId, Rect)>, first: &mut usize, top: &mut f32, height: &mut f32| {
            for (_, slot) in &mut slots[*first..] {
                slot.bottom = slot.top + *height;
            }
            *first = slots.len();
            *top += *height + gap;
            *height = 0.0;
        };
        for i in 0..cx.child_count() {
            let child = cx.child(i);
            let cp = &cx.child_base(child).p;
            if !cp.is_visible {
                continue;
            }
            // A Fill child is measured with the whole line (React MeasureWrap; Nick 2026-10-02: React
            // wins over C#'s flex-fill, where it took the rest of its line), so after other children
            // it starts a line of its own. In a split wrap it fills its column at arrange.
            let fill = split == 0 && fills_width(cp);
            let (request, margins_x) = (cp.width_request, margins(cp, scale).horizontal());
            let mut left = if columns > 0 { x + gap } else { 0.0 };
            // The fit is decided before pixel rounding (C# 6cc876f4): children whose widths add up
            // to exactly the line share it at any scale; a rounding pixel past its end is cut at
            // the slot below. A line is never broken while it is empty.
            let mut exact_left = if columns > 0 { exact + raw_gap } else { 0.0 };
            if columns > 0 && if split > 0 { columns >= split } else { exact_left >= w - FIT } {
                next_line(slots, &mut first, &mut top, &mut line_height);
                (columns, left, exact_left) = (0, 0.0, 0.0);
            }
            let offered = |_left: f32| if split > 0 { chunk } else { w };
            let mut size = measure_kid(kids, cx, child, offered(left), (h - top).max(0.0));
            // An empty child takes no place in the flow.
            if size.width == 0.0 && size.height == 0.0 {
                slots.push((child, Rect::from_xywh(left, top, 0.0, 0.0)));
                continue;
            }
            // A fixed width counts with its request; a content-sized child with what it measured
            // (only it can be squeezed: an overflowing child of a fixed box does not count).
            let exact_width = match request >= 0.0 && !fill {
                true => request * scale + margins_x,
                false => size.width,
            };
            if split == 0 && columns > 0 && exact_width > w - exact_left + FIT {
                next_line(slots, &mut first, &mut top, &mut line_height);
                (columns, left, exact_left) = (0, 0.0, 0.0);
                // Measured once, unless the child takes what the new line offers or is cut by it.
                if fill || size.height > h - top {
                    size = measure_kid(kids, cx, child, offered(left), (h - top).max(0.0));
                }
            }
            // The slot: the column of a split wrap, the rest of the line for a Fill child, else
            // the child's own width, so Center and End stay in the flow.
            let room = (left + offered(left)).min(w) - left;
            let own = wanted(cx.child_base(child), scale).width;
            let slot_width = if split > 0 || fill || room <= own { room } else { own };
            slots.push((child, Rect::from_xywh(left, top, slot_width, size.height)));
            x = left + if split > 0 && aligned { chunk } else { size.width };
            exact = exact_left + if split > 0 && aligned { chunk } else { exact_width };
            columns += 1;
            line_height = line_height.max(size.height);
            content.width = content.width.max(x.min(w));
            content.height = top + line_height;
        }
        // A last line with fewer than `split` children shares the whole width among them (React
        // DynamicColumns on a Wrap; upstream leaves the slots empty).
        if dynamic && columns > 0 && columns < split {
            let width = column_width(w, columns, self.p.spacing * scale);
            let mut left = 0.0;
            line_height = 0.0;
            for (child, slot) in &mut slots[first..] {
                let size = measure_kid(kids, cx, *child, width, (h - top).max(0.0));
                *slot = Rect::from_xywh(left, top, width, size.height);
                left += width + gap;
                line_height = line_height.max(size.height);
            }
            content.width = content.width.max(left - gap);
            content.height = top + line_height;
        }
        next_line(slots, &mut first, &mut top, &mut line_height);
        content
    }

    /// Column with `split` > 1: rows of that many equal columns, each row as tall as its tallest
    /// child (upstream MeasureStack with Split). Keeps the slots for `arrange` like a Wrap.
    // ponytail: a child that fills the height gets what is left under its row, not a share of it.
    fn measure_columns(&mut self, cx: &mut LayoutCx, kids: &mut Kids, w: f32, h: f32) -> Size {
        let scale = cx.scale;
        let (gap, raw_gap) = (snap(self.p.spacing * scale), self.p.spacing * scale);
        let split = self.p.split as usize;
        let visible = (0..cx.child_count()).filter(|i| cx.child_base(cx.child(*i)).p.is_visible).count();
        let (mut index, mut top, mut row_height, mut chunk) = (0, 0f32, 0f32, w);
        // Per row: the children that do not fill their column, added up. The widest row is the
        // width of a column that takes the size of its content.
        let (mut row_width, mut widest, mut filled) = (0f32, 0f32, false);
        for i in 0..cx.child_count() {
            let child = cx.child(i);
            if !cx.child_base(child).p.is_visible {
                continue;
            }
            let column = index % split;
            if column == 0 {
                top += if index > 0 { row_height + gap } else { 0.0 };
                let columns = if self.p.dynamic_columns { split.min(visible - index) } else { split };
                (row_height, row_width, chunk) = (0.0, 0.0, column_width(w, columns, raw_gap));
            }
            let left = column as f32 * (chunk + gap);
            let size = measure_kid(kids, cx, child, chunk, (h - top).max(0.0));
            let base = cx.child_base(child);
            let (own, fill) = (wanted(base, scale), fills_width(&base.p));
            let (start, end) = across(base.p.horizontal_options, fill, left, left + chunk, own.width);
            self.slots.push((child, Rect::new(start, top, end, top + own.height)));
            row_height = row_height.max(size.height);
            if fill {
                filled = true;
            } else {
                row_width += size.width + if column > 0 { gap } else { 0.0 };
                widest = widest.max(row_width);
            }
            index += 1;
        }
        Size::new(if widest > 0.0 || !filled { widest } else { w }, top + row_height)
    }

    /// Absolute, Column and Row: measures the children in the box `w` x `h` inside the padding.
    /// Returns the size of the content. `auto` says which sides take the size of the content.
    ///
    /// Column and Row as DrawnUi.React (SkiaLayout.MeasureAbsolute): every child is measured
    /// unbounded along the stack, so it keeps its content size there and is never cut, and in the
    /// stack's own box across it, where a Fill child reports that whole box; an auto side across is
    /// as large as the largest child, a Fill one included. Kept from C#: a child that fills the main
    /// axis of a stack whose main axis is not auto-sized takes a share of what the others leave
    /// (React gives it its content size there too). Absolute stays C#: an auto side is as large as
    /// the children that do not fill it.
    fn measure_stack(&mut self, cx: &mut LayoutCx, kids: &mut Kids, w: f32, h: f32, auto: (bool, bool)) -> Size {
        let (scale, kind, (auto_w, auto_h)) = (cx.scale, self.p.layout_type, auto);
        let gap = snap(self.p.spacing * scale);
        let stack = matches!(kind, LayoutType::Column | LayoutType::Row);
        // Main-axis Fill children share only on a main axis of a set size. Never in a templated
        // Row: its cells keep their content size (C# Templated_Row_FillXCells_FiniteWidth_AreContentSized).
        let main_auto = if kind == LayoutType::Row { auto_w } else { auto_h };
        let shares = !main_auto && kids.is_none();
        // Per side: the largest child that does not fill it, and the largest that does.
        let (mut own, mut filling) = (Size::default(), Size::default());
        // Main axis of a stack: the sizes added up, what the children that do not fill it take
        // (upstream counts a size request with its raw margins there), the visible children, the
        // ones that fill it.
        let (mut stacked, mut fixed, mut count, mut sharing) = (0f32, 0f32, 0, 0);
        let mut share = f32::INFINITY;
        let along = |size: Size| match kind {
            LayoutType::Column => size.height,
            LayoutType::Row => size.width,
            LayoutType::Absolute | LayoutType::Wrap | LayoutType::Grid => 0.0,
        };

        // Pass 0: children sized by themselves. Pass 1: children filling an auto side, which has a
        // size by now. Pass 2: children filling the main axis of a stack, or both sides of an Absolute.
        for pass in 0..3 {
            // What is left of the main axis, shared. Endless on an unbounded axis: those children
            // measure to their content.
            let left = if kind == LayoutType::Row { w } else { h } - fixed - gap * (count - 1).max(0) as f32;
            let slot = left.max(0.0) / sharing.max(1) as f32;
            if pass == 2 && sharing > 0 {
                share = slot;
            }
            for i in 0..cx.child_count() {
                let child = cx.child(i);
                let cp = &cx.child_base(child).p;
                if !cp.is_visible {
                    continue;
                }
                let (fill_x, fill_y) = (fills_width(cp), fills_height(cp));
                let order = match kind {
                    LayoutType::Absolute | LayoutType::Wrap | LayoutType::Grid => fill_x as i32 + fill_y as i32,
                    LayoutType::Column => (fill_y && shares) as i32 * 2,
                    LayoutType::Row => (fill_x && shares) as i32 * 2,
                };
                if pass == 0 {
                    count += 1;
                    sharing += (order == 2) as i32;
                }
                if order != pass {
                    continue;
                }
                // A size request with its margins, as upstream reserves it before it shares the rest.
                let (request, raw_margins) = match kind {
                    LayoutType::Column => (cp.height_request, cp.margin.vertical()),
                    _ => (cp.width_request, cp.margin.horizontal()),
                };
                let least = (request > 0.0) as i32 as f32;
                let reserved = (request >= 0.0).then(|| snap(request * scale + raw_margins * scale).max(least));
                // Absolute: a child that fills an auto side gets the size the other children gave it.
                let mut child_w = if !stack && fill_x && auto_w && own.width > 0.0 { own.width } else { w };
                let mut child_h = if !stack && fill_y && auto_h && own.height > 0.0 { own.height } else { h };
                // Main axis of a stack: a child with a share gets it, every other child is unbounded.
                match kind {
                    LayoutType::Column => child_h = if order == 2 { slot } else { f32::INFINITY },
                    LayoutType::Row => child_w = if order == 2 { slot } else { f32::INFINITY },
                    LayoutType::Absolute | LayoutType::Wrap | LayoutType::Grid => {}
                }
                let s = measure_kid(kids, cx, child, child_w, child_h);
                // A child with a share takes all of it, fraction included.
                stacked += if pass == 2 && slot.is_finite() { along(s).max(slot) } else { along(s) };
                fixed += reserved.unwrap_or(along(s));
                if kind != LayoutType::Row {
                    let side = if fill_x { &mut filling.width } else { &mut own.width };
                    *side = side.max(s.width);
                }
                if kind != LayoutType::Column {
                    let side = if fill_y { &mut filling.height } else { &mut own.height };
                    *side = side.max(s.height);
                }
            }
        }
        self.share = share;

        // Shares are whole pixels once measured, so together they can take a pixel more than was
        // shared. A child after them that does not fit any more is cut, as upstream's in-order
        // measure cuts it: measured again, with what is really left.
        if sharing > 0 && share.is_finite() && kind != LayoutType::Absolute {
            let column = kind == LayoutType::Column;
            let (mut at, mut index, mut after_fill) = (0f64, 0, false);
            for i in 0..cx.child_count() {
                let child = cx.child(i);
                let base = cx.child_base(child);
                let cp = &base.p;
                if !cp.is_visible {
                    continue;
                }
                at += if index > 0 { gap as f64 } else { 0.0 };
                index += 1;
                let (start, mut size) = (snap(at as f32), base.measured);
                let (fill_x, fill_y) = (fills_width(cp), fills_height(cp));
                let fill = if column { fill_y } else { fill_x };
                let room = (if column { h } else { w } - start).max(0.0);
                if !fill && after_fill && along(size) > room {
                    let was = along(size);
                    size = match column {
                        true => measure_kid(kids, cx, child, w, room),
                        false => measure_kid(kids, cx, child, room, h),
                    };
                    stacked += along(size) - was;
                }
                after_fill |= fill;
                // The next child starts where upstream's running position gets to (see `arrange_stack`).
                let snapped = snap((at + size.height as f64) as f32) - start;
                at += match column {
                    true => (if fill { size.height.max(share) } else { size.height }).max(snapped),
                    false => size.width,
                } as f64;
            }
        }

        // Absolute: a side that only has fill children takes what they measured: the constraint, or
        // their content on an unbounded axis. A stack: the largest child across, Fill or not.
        let across = |own: f32, filling: f32| if stack { own.max(filling) } else if own > 0.0 { own } else { filling };
        let mut content_w = across(own.width, filling.width);
        let mut content_h = across(own.height, filling.height);
        let gaps = gap * (count - 1).max(0) as f32;
        match kind {
            LayoutType::Column => content_h = snap(stacked + gaps),
            LayoutType::Row => content_w = snap(stacked + gaps),
            LayoutType::Absolute | LayoutType::Wrap | LayoutType::Grid => {}
        }
        Size::new(content_w, content_h)
    }

    /// Column and Row: places the children. Upstream lays a stack out in the box it measured the
    /// children in and only moves the result to where the stack is drawn, so the positions do not
    /// depend on where the stack sits.
    fn arrange_stack(&self, cx: &mut LayoutCx, kids: &mut Kids, origin: (f32, f32), auto: (bool, bool)) {
        let (scale, column) = (cx.scale, self.p.layout_type == LayoutType::Column);
        let gap = snap(self.p.spacing * scale);
        // The box the children align in: a side that takes the size of its content is that large.
        let side = Size::new(
            if auto.0 { self.content.width } else { self.inner.width },
            if auto.1 { self.content.height } else { self.inner.height },
        );
        // Where the next child starts on the main axis. A fraction stays in it when Fill children
        // shared a remainder: every child starts on the pixel nearest to it.
        let mut along = 0f64;
        let mut index = 0;
        for i in 0..cx.child_count() {
            let child = cx.child(i);
            let base = cx.child_base(child);
            let cp = &base.p;
            if !cp.is_visible {
                continue;
            }
            if index > 0 {
                along += gap as f64;
            }
            index += 1;
            let (size, own, start) = (base.measured, wanted(base, scale), snap(along as f32));
            let (horizontal, vertical) = (cp.horizontal_options, cp.vertical_options);
            let (main, main_fill) = if column { (vertical, fills_height(cp)) } else { (horizontal, fills_width(cp)) };
            let (length, own_length, stack_end) =
                if column { (size.height, own.height, side.height) } else { (size.width, own.width, side.width) };
            let shared = main_fill && self.share.is_finite();
            // Main axis: a child with a share gets it; a Center child, and a Fill child with
            // nothing to share, its own size; an End child everything up to the end of the stack;
            // a Start child what is left of the box the stack measured in.
            let box_end = if column { self.inner.height } else { self.inner.width };
            let end = if shared {
                start + self.share
            } else if main == LayoutOptions::Center || main_fill {
                start + own_length
            } else if main == LayoutOptions::End {
                stack_end.max(start + length)
            } else {
                box_end.max(start)
            };
            let area = if column {
                let (left, right) = across(horizontal, fills_width(cp), 0.0, side.width, own.width);
                Rect::new(left, start, right, end)
            } else {
                let (top, bottom) = across(vertical, fills_height(cp), 0.0, side.height, own.height);
                Rect::new(start, top, end, bottom)
            };
            let mut cell = cell(base, area, scale);
            // The next child starts after the measured size (a Column: or after the whole share).
            let taken = if column {
                let snapped = snap((along + size.height as f64) as f32) - start;
                if horizontal == LayoutOptions::Start {
                    cell.bottom = cell.top + snapped;
                }
                (if shared { size.height.max(self.share) } else { size.height }).max(snapped)
            } else {
                let snapped = snap((along + size.width as f64) as f32) - start;
                if vertical == LayoutOptions::Start {
                    cell.right = cell.left + snapped;
                }
                size.width
            };
            // Upstream does not place a child that measured empty: it takes no room and its rect
            // stays empty.
            if size.width == 0.0 && size.height == 0.0 {
                arrange_kid(kids, cx, child, Rect::default());
                continue;
            }
            along += taken as f64;
            arrange_kid(kids, cx, child, cell.with_offset(origin));
        }
    }
}

/// The templated part of a layout, taken out of it while it lays its children out.
type Kids = Option<Box<list::Items>>;

/// Measures a child: a slot of a recycled templated layout through its item (`list.rs`), any
/// other child itself.
fn measure_kid(kids: &mut Kids, cx: &mut LayoutCx, child: ControlId, width: f32, height: f32) -> Size {
    match kids.as_deref_mut() {
        Some(items) => items.measure_child(cx, child, width, height),
        None => cx.measure_child(child, width, height),
    }
}

/// Places a child: a slot of a recycled templated layout records where its item goes and moves
/// the view drawn in it there.
fn arrange_kid(kids: &mut Kids, cx: &mut LayoutCx, child: ControlId, destination: Rect) {
    match kids.as_deref_mut() {
        Some(items) => items.arrange_child(cx, child, destination),
        None => cx.arrange_child(child, destination),
    }
}

/// The width of one of `columns` equal columns in `width`, rounded to a pixel as upstream does
/// (halves go to the even side).
fn column_width(width: f32, columns: usize, gap: f32) -> f32 {
    if columns > 1 { snap((width - (columns - 1) as f32 * gap) / columns as f32) } else { width }
}

/// The size upstream's stack gives the slot of a child (C# LayoutCell desiredWidth /
/// desiredHeight): a size request rounded up, else the measured size.
// Upstream leaves the margins out of a requested size, and the child is squeezed by them. Here
// the margins are in.
/// What a wrap's line may be passed by when it decides whether children fill it exactly: the
/// line's own rounding to whole pixels (at most half a pixel), cut at the slot.
const FIT: f32 = 0.5;

fn wanted(base: &Base, scale: f32) -> Size {
    let (p, m) = (&base.p, margins(&base.p, scale));
    let side = |request: f32, margins: f32, measured: f32| match request >= 0.0 {
        true => (request * scale).ceil() + margins,
        false => measured,
    };
    Size::new(
        side(p.width_request, m.horizontal(), base.measured.width),
        side(p.height_request, m.vertical(), base.measured.height),
    )
}

/// The margin box of a child placed in `area`, in whole pixels, in the coordinates of `area`
/// (C# LayoutCell: Arrange, then Destination).
fn cell(base: &Base, area: Rect, scale: f32) -> Rect {
    let placed = place(base, area.size(), scale);
    Rect::new(
        snap(placed.left + area.left),
        snap(placed.top + area.top),
        snap(placed.right + area.left),
        snap(placed.bottom + area.top),
    )
}

/// Cross axis of a stack child (C# LayoutCell): with room to spare a child that fills takes the
/// whole side, any other child a slot of its own size at the start, in the middle or at the end.
fn across(options: LayoutOptions, fills: bool, start: f32, end: f32, own: f32) -> (f32, f32) {
    if end - start <= own {
        return (start, end);
    }
    match options {
        LayoutOptions::Fill if fills => (start, end),
        LayoutOptions::Center => {
            let centered = start + ((end - start - own) / 2.0).ceil();
            (centered, centered + own)
        }
        LayoutOptions::End => (end - own, end),
        _ => (start, start + own),
    }
}

/// Templated Grid: cell `i` goes to column `i % split` of row `i / split`, or with `invert` down
/// the columns, `ceil(count / split)` rows each (React MeasureGrid, C# BuildGrid with a template).
fn place_items(cx: &mut LayoutCx, split: usize, invert: bool) {
    let count = (0..cx.child_count()).filter(|i| cx.child_base(cx.child(*i)).p.is_visible).count();
    let down = if invert && split > 1 { count.div_ceil(split) } else { 0 };
    let mut index = 0;
    for i in 0..cx.child_count() {
        let child = cx.child(i);
        let Some(node) = cx.tree.node_mut(child) else { continue };
        if !node.base.p.is_visible {
            continue;
        }
        let (column, row) = if down > 0 { (index / down, index % down) } else { (index % split, index / split) };
        (node.base.p.column, node.base.p.row) = (column as i32, row as i32);
        index += 1;
    }
}

impl SkiaLayout {
    /// The tracks of the last measure of a Grid.
    pub(crate) fn grid_tracks(&self) -> &grid::Grid {
        &self.grid
    }
}

/// DrawnUI alias: a Grid layout filling the width.
pub struct SkiaGrid;
impl SkiaGrid {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        SkiaLayout::grid()
    }
}

/// DrawnUI alias: a Wrap layout filling the width.
pub struct SkiaWrap;
impl SkiaWrap {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        SkiaLayout::wrap()
    }
}

/// DrawnUI alias: a Column layout filling the width.
pub struct SkiaStack;
impl SkiaStack {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        SkiaLayout::column()
    }
}

/// DrawnUI alias: a Row layout.
pub struct SkiaRow;
impl SkiaRow {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        SkiaLayout::row()
    }
}

/// DrawnUI alias: an Absolute layout filling the width.
pub struct SkiaLayer;
impl SkiaLayer {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLayout> {
        SkiaLayout::layer()
    }
}

impl Has<LayoutProps> for SkiaLayout {
    fn part(&self) -> &LayoutProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.p
    }
}

impl Container for SkiaLayout {}

/// Cross axis of a list row: a Center cell gets a slot of its own size in the middle, every other
/// cell the whole side, where it aligns itself.
fn cross_slot(options: LayoutOptions, start: f32, end: f32, size: f32) -> (f32, f32) {
    if options == LayoutOptions::Center && end - start > size {
        let centered = start + ((end - start - size) / 2.0).ceil();
        (centered, centered + size)
    } else {
        (start, end)
    }
}

/// The sides of a layout that take the size of its content (C# NeedAutoWidth / NeedAutoHeight).
fn auto_sides(p: &ControlProps) -> (bool, bool) {
    (
        p.lock_ratio == 0.0 && p.width_request < 0.0 && p.horizontal_options != LayoutOptions::Fill,
        p.lock_ratio == 0.0 && p.height_request < 0.0 && p.vertical_options != LayoutOptions::Fill,
    )
}

impl Control for SkiaLayout {
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let mut kids = self.items.take();
        let size = match kids.as_deref_mut() {
            Some(items) if list::is_list(&self.p) => items.measure(cx, &self.p, width, height),
            Some(items) => {
                items.realize_all(cx, &self.p, true);
                self.measure_children(cx, &mut kids, width, height)
            }
            None => self.measure_children(cx, &mut kids, width, height),
        };
        self.items = kids;
        size
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let mut kids = self.items.take();
        match kids.as_deref_mut() {
            Some(items) if list::is_list(&self.p) => items.arrange(cx, &self.p),
            _ => {
                // A recycled layout arranged again only because an ancestor scrolled places nothing:
                // it binds views to the slots that can be seen now.
                if kids.as_deref().is_none_or(|items| items.lays_out(cx.base().rect)) {
                    self.arrange_children(cx, &mut kids);
                }
                if let Some(items) = kids.as_deref_mut() {
                    items.after_arrange(cx, &self.p);
                }
            }
        }
        self.items = kids;
    }
}

impl SkiaLayout {
    /// Measures the children of a Wrap, a split Column, a Grid, a stack or an Absolute layout.
    fn measure_children(&mut self, cx: &mut LayoutCx, kids: &mut Kids, width: f32, height: f32) -> Size {
        let (scale, kind) = (cx.scale, self.p.layout_type);
        let p = &cx.base().p;
        // An auto side is as large as the children that do not fill it.
        let auto = auto_sides(p);
        // The box for the children, and what the padding adds to the content of an auto side.
        let (inner, padding) = content_box(p, width, height, scale);
        let (w, h) = (inner.width, inner.height);
        self.inner = inner;
        // A recycled templated layout learns what the template decides for an item (its layout
        // properties) when it measures it: when that differs from what the pass read, once more.
        for _ in 0..2 {
            if kids.is_some() && kind == LayoutType::Grid {
                place_items(cx, self.p.split.max(1) as usize, self.p.invert);
            }
            self.slots.clear();
            self.content = if kind == LayoutType::Wrap {
                self.measure_wrap(cx, kids, w, h)
            } else if kind == LayoutType::Column && self.p.split > 1 && w.is_finite() {
                self.measure_columns(cx, kids, w, h)
            } else if kind == LayoutType::Grid {
                self.grid.measure(cx, &self.p, kids, w, h, auto)
            } else {
                self.measure_stack(cx, kids, w, h, auto)
            };
            if !kids.as_deref_mut().is_some_and(|items| items.take_props_changed()) {
                break;
            }
        }
        Size::new(self.content.width + padding.width, self.content.height + padding.height)
    }

    /// Places the children of a Wrap, a split Column, a Grid, a stack or an Absolute layout.
    fn arrange_children(&mut self, cx: &mut LayoutCx, kids: &mut Kids) {
        let scale = cx.scale;
        let (inner, auto) = (content_rect(cx.base(), scale), auto_sides(&cx.base().p));
        let origin = (inner.left, inner.top);
        // A Wrap and a Column with `split`: the measure left a slot for every child.
        if !self.slots.is_empty() {
            for (child, slot) in &self.slots {
                let cell = cell(cx.child_base(*child), *slot, scale);
                arrange_kid(kids, cx, *child, cell.with_offset(origin));
            }
            return;
        }
        match self.p.layout_type {
            LayoutType::Grid => self.grid.arrange(cx, &self.p, kids, inner.left, inner.top),
            LayoutType::Column | LayoutType::Row => self.arrange_stack(cx, kids, origin, auto),
            LayoutType::Absolute | LayoutType::Wrap => {
                for i in 0..cx.child_count() {
                    let child = cx.child(i);
                    if cx.child_base(child).p.is_visible {
                        arrange_kid(kids, cx, child, inner);
                    }
                }
            }
        }
    }
}
