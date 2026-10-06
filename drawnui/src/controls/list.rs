//! Templated children (DrawnUI ItemsSource + ItemTemplate): cells are real nodes made from a
//! template. A single Column is a list: cells exist for the visible rows only and are recycled,
//! row sizes live in arrays, not in a structure of cells, so a frame costs what enters the
//! viewport, not what the list holds. Any other layout (Row, Wrap, Grid, a Column with `split`)
//! follows RecyclingTemplate, as DrawnUi.React does: Enabled lays out one slot per item, measured
//! through views from a pool, and binds views only to the slots that can be seen; Disabled
//! realizes one cell per item and lays them out as its children.

use std::any::Any;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

use skia_safe::{Rect, Size};

use super::{LayoutProps, LayoutType, MeasureBudget, MeasuringStrategy, RecyclingTemplate, SkiaLayout, cell, cross_slot};
use crate::control::{Control, LayoutCx, part_mut};
use crate::controls::scroll::SkiaScroll;
use crate::layout::{self, fills_height};
use crate::tree::{Base, Build, ControlId, ControlProps, Cx, Detached, Mut, Tree, wrong_state};
use crate::types::{CacheType, Dirty, LayoutOptions, Thickness};

/// DrawnUI ReserveTemplates default: spare cells a recycled pool may keep above the visible ones.
const RESERVE_TEMPLATES: usize = 2;

// ---------------------------------------------------------------- row sizes

/// Row heights in pixels. Measured rows are exact, the others take the average of the measured
/// ones. Two Fenwick trees over the measured rows (their heights, how many) give the offset of any
/// row and the row at any offset in O(log n).
#[derive(Default)]
struct Sizes {
    len: usize,
    /// MeasureFirst: no arrays, every row has the height of the first one (`uniform`, NaN until
    /// it was measured).
    first_only: bool,
    uniform: f32,
    /// NaN = not measured.
    heights: Vec<f32>,
    sums: Vec<f64>,
    counts: Vec<u32>,
    sum: f64,
    measured: usize,
}

impl Sizes {
    fn reset(&mut self, len: usize, first_only: bool) {
        (self.len, self.first_only, self.uniform) = (len, first_only, f32::NAN);
        self.heights.clear();
        if !first_only {
            self.heights.resize(len, f32::NAN);
        }
        self.rebuild();
    }

    // ponytail: every insert, remove and move rebuilds the trees, O(n) additions per collection
    // change (an append and a drag step included). Push to the trees in O(log n) when appends to
    // huge lists or drags in them show up.
    fn insert(&mut self, index: usize, count: usize) {
        self.len += count;
        if !self.first_only {
            self.heights.splice(index..index, std::iter::repeat_n(f32::NAN, count));
        }
        self.rebuild();
    }

    fn remove(&mut self, index: usize, count: usize) {
        self.len -= count;
        if !self.first_only {
            self.heights.drain(index..index + count);
        }
        self.rebuild();
    }

    /// The row `from` goes to `to`; its height goes with it.
    fn move_row(&mut self, from: usize, to: usize) {
        if !self.first_only {
            let height = self.heights.remove(from);
            self.heights.insert(to, height);
            self.rebuild();
        }
    }

    /// Row `i` now holds what row `order[i]` held; the heights follow.
    fn reorder(&mut self, order: &[usize]) {
        if !self.first_only {
            let was = std::mem::take(&mut self.heights);
            self.heights = order.iter().map(|old| was[*old]).collect();
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        let n = self.heights.len();
        self.sums.clear();
        self.sums.resize(n + 1, 0.0);
        self.counts.clear();
        self.counts.resize(n + 1, 0);
        (self.sum, self.measured) = (0.0, 0);
        for i in 1..=n {
            let h = self.heights[i - 1];
            if !h.is_nan() {
                self.sums[i] += h as f64;
                self.counts[i] += 1;
                self.sum += h as f64;
                self.measured += 1;
            }
            let parent = i + (i & i.wrapping_neg());
            if parent <= n {
                self.sums[parent] += self.sums[i];
                self.counts[parent] += self.counts[i];
            }
        }
    }

    fn known(&self, index: usize) -> bool {
        if self.first_only { !self.uniform.is_nan() } else { !self.heights[index].is_nan() }
    }

    /// What a row that was not measured counts as: whole pixels, so estimated rows do not put
    /// the rows after them between pixels.
    fn average(&self) -> f64 {
        if self.measured > 0 { (self.sum / self.measured as f64).round() } else { 0.0 }
    }

    fn height(&self, index: usize) -> f32 {
        let h = if self.first_only { self.uniform } else { self.heights[index] };
        if !h.is_nan() {
            h
        } else if self.first_only {
            0.0
        } else {
            self.average() as f32
        }
    }

    /// Records a measured height, or forgets it with NaN. True when it changed. Under MeasureFirst
    /// only the first measure counts: every row keeps that size.
    fn set(&mut self, index: usize, height: f32) -> bool {
        if self.first_only {
            if !self.uniform.is_nan() || height.is_nan() {
                return false;
            }
            self.uniform = height;
        } else {
            let old = self.heights[index];
            if (old.is_nan() && height.is_nan()) || (old - height).abs() <= 0.01 {
                return false;
            }
            self.heights[index] = height;
            let value = |h: f32| if h.is_nan() { 0.0 } else { h as f64 };
            let (by, more) = (value(height) - value(old), height.is_nan() as i32 - old.is_nan() as i32);
            self.sum += by;
            self.measured = (self.measured as i32 - more) as usize;
            let mut i = index + 1;
            while i <= self.len {
                self.sums[i] += by;
                self.counts[i] = (self.counts[i] as i32 - more) as u32;
                i += i & i.wrapping_neg();
            }
        }
        true
    }

    /// Pixels from the first row's top to the top of row `index` (`len` = one gap past the end).
    fn offset(&self, index: usize, gap: f32) -> f32 {
        if self.first_only {
            return index as f32 * (self.height(0) + gap);
        }
        let (mut sum, mut count, mut i) = (0f64, 0usize, index);
        while i > 0 {
            sum += self.sums[i];
            count += self.counts[i] as usize;
            i -= i & i.wrapping_neg();
        }
        (sum + (index - count) as f64 * self.average() + index as f64 * gap as f64) as f32
    }

    /// The row at `y` pixels from the first row's top; the gap below a row belongs to it.
    /// `len` must not be 0.
    fn index_at(&self, y: f32, gap: f32) -> usize {
        if self.first_only {
            let stride = self.height(0) + gap;
            return if stride > 0.0 { ((y / stride).max(0.0) as usize).min(self.len - 1) } else { 0 };
        }
        let (average, gap, y) = (self.average(), gap as f64, y as f64);
        let (mut at, mut sum, mut count) = (0usize, 0f64, 0usize);
        let mut step = 1usize << self.len.ilog2();
        while step > 0 {
            let next = at + step;
            if next <= self.len {
                let (s, c) = (sum + self.sums[next], count + self.counts[next] as usize);
                if s + (next - c) as f64 * average + next as f64 * gap <= y {
                    (at, sum, count) = (next, s, c);
                }
            }
            step >>= 1;
        }
        at.min(self.len - 1)
    }

    /// First and last row that show between `top` and `bottom`, by the sizes as they are known.
    /// A row that starts exactly at `bottom` does not show.
    fn range(&self, top: f32, bottom: f32, gap: f32) -> (usize, usize) {
        let (first, last) = (self.index_at(top, gap), self.index_at(bottom, gap));
        (first, if last > first && self.offset(last, gap) >= bottom { last - 1 } else { last })
    }

    /// Height of all rows with the gaps between them.
    fn total(&self, gap: f32) -> f32 {
        if self.len == 0 { 0.0 } else { self.offset(self.len, gap) - gap }
    }
}

// ---------------------------------------------------------------- the list

/// What a MeasureVisible list measures in one frame besides the rows on screen, against its
/// `measure_budget`: rows that came in above the viewport, then rows ahead. A frame starts at the
/// list's measure, or at its arrange when it was not measured.
#[derive(Clone, Copy)]
struct Work {
    budget: MeasureBudget,
    started: Option<Instant>,
    rows: u32,
}

impl Work {
    fn new(budget: MeasureBudget) -> Self {
        Self { budget, started: None, rows: 0 }
    }

    /// Whether one more row fits in the frame. A time budget takes at least one row; under a
    /// synthetic clock (the headless host) a row counts as a millisecond, so a test gives the same
    /// frames on every machine.
    fn fits(&mut self, synthetic: bool) -> bool {
        match self.budget {
            MeasureBudget::Items(items) => self.rows < items,
            MeasureBudget::Millis(_) if self.rows == 0 => {
                self.started = Some(Instant::now());
                true
            }
            MeasureBudget::Millis(ms) if synthetic => (self.rows as f32) < ms,
            MeasureBudget::Millis(ms) => self.started.is_none_or(|at| at.elapsed().as_secs_f32() * 1000.0 < ms),
        }
    }
}

/// One template instance and the handles its template returned.
struct Cell {
    id: ControlId,
    handles: Box<dyn Any>,
    /// The item it shows, by its index now; `None` when that item is gone or changed. A row gets
    /// the cell that shows it already back without a bind.
    bound: Option<usize>,
}

enum Change {
    Reset,
    Inserted(usize, usize),
    Removed(usize, usize),
    Changed(usize),
    Moved(usize, usize),
    /// For every new index, the old index of the item now there.
    Reordered(Vec<usize>),
}

/// What the layout reads from a child to place it: the part of its properties a recycled layout's
/// slot takes over from the view its item was measured with (what the template decided for the item).
#[derive(Clone, Copy, PartialEq)]
struct Placement {
    horizontal_options: LayoutOptions,
    vertical_options: LayoutOptions,
    requests: [f32; 6],
    margin: Thickness,
    lock_ratio: f32,
    fill_ratios: (f32, f32),
    spans: (i32, i32),
    is_visible: bool,
    z_index: i32,
}

impl Placement {
    fn of(p: &ControlProps) -> Self {
        Self {
            horizontal_options: p.horizontal_options,
            vertical_options: p.vertical_options,
            requests: [
                p.width_request,
                p.height_request,
                p.minimum_width_request,
                p.minimum_height_request,
                p.maximum_width_request,
                p.maximum_height_request,
            ],
            margin: p.margin,
            lock_ratio: p.lock_ratio,
            fill_ratios: (p.horizontal_fill_ratio, p.vertical_fill_ratio),
            spans: (p.column_span, p.row_span),
            is_visible: p.is_visible,
            z_index: p.z_index,
        }
    }

    fn apply(self, p: &mut ControlProps) {
        (p.horizontal_options, p.vertical_options) = (self.horizontal_options, self.vertical_options);
        let [w, h, min_w, min_h, max_w, max_h] = self.requests;
        (p.width_request, p.height_request, p.minimum_width_request, p.minimum_height_request) = (w, h, min_w, min_h);
        (p.maximum_width_request, p.maximum_height_request) = (max_w, max_h);
        (p.margin, p.lock_ratio) = (self.margin, self.lock_ratio);
        (p.horizontal_fill_ratio, p.vertical_fill_ratio) = self.fill_ratios;
        (p.column_span, p.row_span) = self.spans;
        (p.is_visible, p.z_index) = (self.is_visible, self.z_index);
    }
}

/// The node that holds the place of one item in a recycled templated layout: the layout measures
/// and places it like a child, it draws nothing itself, and the view of its item is its child
/// while the item can be seen.
struct SlotControl;

impl Control for SlotControl {}

/// One item of a recycled templated layout (not a list).
struct Slot {
    node: ControlId,
    /// The view drawn in the slot; `None` while the item cannot be seen.
    view: Option<Cell>,
    /// The constraints and scale (bits) the item was last measured with; `None`: measure it.
    measured_for: Option<(u32, u32, u32)>,
}

/// The rect `layout::arrange` gives a control arranged into `destination`.
fn arranged_rect(base: &Base, destination: Rect, scale: f32) -> Rect {
    let (placed, m) = (layout::place(base, destination.size(), scale), layout::margins(&base.p, scale));
    Rect::new(
        layout::snap(placed.left + destination.left + m.left),
        layout::snap(placed.top + destination.top + m.top),
        layout::snap(placed.right + destination.left - m.right),
        layout::snap(placed.bottom + destination.top - m.bottom),
    )
}

/// The caches above a control show something else now: they record again.
fn stale_above(tree: &mut Tree, from: ControlId) {
    let mut current = Some(from);
    while let Some(node) = current.and_then(|c| tree.node_mut(c)) {
        node.base.content_epoch = node.base.content_epoch.wrapping_add(1);
        current = node.parent;
    }
}

/// A templated Column without `split` is a list of recycled cells; every other templated layout
/// lays out every item (DrawnUi.React `IsTemplatedList`).
pub(crate) fn is_list(p: &LayoutProps) -> bool {
    p.layout_type == LayoutType::Column && p.split <= 1
}

type Bind = Box<dyn FnMut(&dyn Any, &dyn Any, usize, &mut Cx<'_>)>;

/// The templated part of a SkiaLayout.
pub(crate) struct Items {
    count_of: Rc<dyn Fn(&dyn Any) -> usize>,
    template: Box<dyn FnMut() -> (Detached, Box<dyn Any>)>,
    bind: Bind,
    /// Changes the app reported since the last measure.
    pending: Vec<Change>,
    sizes: Sizes,
    /// What the sizes were measured for: width, scale, and the two modes.
    key: Option<(f32, f32, MeasuringStrategy, RecyclingTemplate)>,
    /// Spacing and top padding of the last measure, pixels.
    gap: f32,
    lead: f32,
    /// Widest cell measured: the width of an auto-width list.
    max_width: f32,
    /// The realized rows `first..first + cells.len()`, in order.
    cells: VecDeque<Cell>,
    first: usize,
    /// Released cells, hidden. Enabled: the next row that needs a cell takes one. Disabled: each
    /// waits for its own item.
    pool: Vec<Cell>,
    /// Cells of rows entering the window, made while the visible rows were being found.
    fresh: Vec<(usize, Cell)>,
    /// Every row before this one is measured (MeasureAll, MeasureVisible).
    frontier: usize,
    /// What this frame measured so far besides the rows on screen.
    work: Work,
    /// A realized cell may need its measure: the list was measured since the last arrange.
    cells_dirty: bool,
    /// Pixels the anchor row moved by since the scroll last took the shift.
    shift: f32,
    /// Content height as the parents know it.
    total: f32,
    /// What the cells were last placed for: first row, rows, its top, the list's left, top and
    /// width. Nothing is touched while it holds.
    placed: (usize, usize, f32, f32, f32, f32),
    /// Cells for the visible rows only (a list), or one per item laid out by the layout.
    list: bool,
    /// Not a list: where the layout put each item at the last arrange, (top, height) in pixels
    /// from the layout's top.
    laid: Vec<(f32, f32)>,
    /// Cells made from the template so far (diagnostics).
    created: u32,
    /// A cell was made, bound, shown or hidden since `realize_all` last reported it.
    touched: bool,
    /// Not a list, RecyclingTemplate Enabled: the layout's children are slots, one per item, and
    /// the views are bound to the slots that can be seen.
    slotted: bool,
    slots: Vec<Slot>,
    /// What the template decided for the item measured last: what a slot not measured yet is
    /// laid out with.
    model: Option<Placement>,
    /// A slot measured in this pass took other layout properties than the layout read for it.
    props_changed: bool,
    /// Slots: measured since the last arrange, and the layout's rect at that arrange.
    relaid: bool,
    arranged_at: Rect,
}

/// Every index once: a reorder that keeps every item.
fn is_permutation(order: &[usize]) -> bool {
    let mut seen = vec![false; order.len()];
    order.iter().all(|i| *i < seen.len() && !std::mem::replace(&mut seen[*i], true))
}

/// The app state as the type the list's closures were written for.
fn state<S: Any>(state: &dyn Any) -> &S {
    state.downcast_ref().unwrap_or_else(|| wrong_state::<S>())
}

fn bind_cell(bind: &mut Bind, cell: &mut Cell, cx: &mut LayoutCx, index: usize) -> bool {
    cell.bound = Some(index);
    bind(&*cell.handles, cx.state, index, &mut Cx { tree: cx.tree });
    // Setters only queue: the cell is measured and painted in this frame, so they apply now.
    layout::flush(cx.tree, Some(cx.id));
    true
}

/// Shows a cell for a row, or hides a released one from paint and hit testing.
fn show(cx: &mut LayoutCx, id: ControlId, index: Option<usize>) {
    if let Some(node) = cx.tree.node_mut(id) {
        node.base.p.is_visible = index.is_some();
        node.base.context_index = index;
    }
}

impl Items {
    fn recycles(&self) -> bool {
        self.key.is_none_or(|k| k.3 == RecyclingTemplate::Enabled)
    }

    fn realized(&self, index: usize) -> Option<ControlId> {
        match self.slotted {
            true => self.slots.get(index)?.view.as_ref().map(|c| c.id),
            false => self.cells.get(index.checked_sub(self.first)?).map(|c| c.id),
        }
    }

    /// The visible part of the rows plus the inflation, in pixels from the first row's top, as it
    /// will be once the scroll took the pending shift. `None` when nothing shows.
    fn window(&self, cx: &LayoutCx) -> Option<(f32, f32)> {
        let visible = cx.visible_rect();
        if visible.is_empty() || self.sizes.len == 0 {
            return None;
        }
        let base = cx.base();
        let top = layout::content_rect(base, cx.scale).top - self.shift;
        let inflate = base.p.virtualisation_inflated * cx.scale;
        Some((visible.top - inflate - top, visible.bottom + inflate - top))
    }

    /// The row that keeps its place on screen while sizes above it change: the first measured
    /// row in the window, with its offset.
    fn anchor(&self, cx: &LayoutCx) -> Option<(usize, f32)> {
        let (top, bottom) = self.window(cx)?;
        let (first, last) = self.sizes.range(top, bottom, self.gap);
        let anchor = (first..=last).find(|i| self.sizes.known(*i)).unwrap_or(first);
        Some((anchor, self.sizes.offset(anchor, self.gap)))
    }

    fn create(&mut self, cx: &mut LayoutCx) -> Cell {
        let (detached, handles) = (self.template)();
        // Mounting asks for another frame; the cell is laid out and painted in this one.
        let pending = cx.tree.needs_frame;
        let id = cx.tree.mount(Some(cx.id), detached);
        cx.tree.needs_frame = pending;
        cx.tree.run_observers_under(id, cx.state);
        self.created += 1;
        Cell { id, handles, bound: None }
    }

    /// A visible cell showing item `index`, measured: the one made for it while the rows were
    /// found, the spare one that shows it already, another spare one bound to it, or a new one.
    fn acquire(&mut self, cx: &mut LayoutCx, index: usize) -> Cell {
        let cell = if let Some(at) = self.fresh.iter().position(|f| f.0 == index) {
            self.fresh.swap_remove(at).1
        } else {
            // ponytail: a scan of the spare cells per entering row. Few under Enabled; under
            // Disabled one per item seen, which is for small lists. Index them when that hurts.
            let spare = self.pool.iter().position(|c| c.bound == Some(index)).or_else(|| {
                // No cell shows it: one whose item is gone, else the one whose row is farthest
                // away, the least likely to be wanted back. Disabled never takes a cell from an
                // item that still exists.
                let away = |c: &Cell| c.bound.map_or(usize::MAX, |bound| bound.abs_diff(index));
                let (at, cell) = self.pool.iter().enumerate().max_by_key(|(_, c)| away(c))?;
                (self.recycles() || cell.bound.is_none()).then_some(at)
            });
            let mut cell = match spare {
                Some(at) => self.pool.swap_remove(at),
                None => self.create(cx),
            };
            if cell.bound != Some(index) {
                bind_cell(&mut self.bind, &mut cell, cx, index);
            }
            cell
        };
        self.touched = true;
        show(cx, cell.id, Some(index));
        if self.list {
            self.measure_cell(cx, cell.id, index);
        }
        cell
    }

    fn release(&mut self, cx: &mut LayoutCx, cell: Cell) {
        self.touched = true;
        show(cx, cell.id, None);
        // An overlay effect still playing on the cell (a ripple) belongs to the row that left.
        let mut at = 0;
        while let Some(animator) = cx.tree.animators.get(at) {
            let inside = animator.control == cell.id || cx.tree.is_ancestor(cell.id, animator.control);
            if animator.overlay.is_some() && inside {
                cx.tree.animators.remove(at);
            } else {
                at += 1;
            }
        }
        self.pool.push(cell);
    }

    fn release_all(&mut self, cx: &mut LayoutCx) {
        while let Some(cell) = self.cells.pop_back() {
            self.release(cx, cell);
        }
    }

    fn release_outside(&mut self, cx: &mut LayoutCx, first: usize, last: usize) {
        while self.first < first
            && let Some(cell) = self.cells.pop_front()
        {
            self.release(cx, cell);
            self.first += 1;
        }
        while self.first + self.cells.len() > last + 1
            && let Some(cell) = self.cells.pop_back()
        {
            self.release(cx, cell);
        }
    }

    /// Makes `first..=last` the realized rows: cells outside go to the pool, entering rows get one.
    fn realize(&mut self, cx: &mut LayoutCx, first: usize, last: usize) {
        self.release_outside(cx, first, last);
        if self.cells.is_empty() {
            self.first = first;
        }
        while self.first > first {
            let cell = self.acquire(cx, self.first - 1);
            self.first -= 1;
            self.cells.push_front(cell);
        }
        while self.first + self.cells.len() <= last {
            let cell = self.acquire(cx, self.first + self.cells.len());
            self.cells.push_back(cell);
        }
    }

    /// Removes every cell node: the cells cannot be used for what comes next. The slots go with
    /// the views drawn in them.
    fn drop_cells(&mut self, cx: &mut LayoutCx) {
        for cell in self.cells.drain(..).chain(self.pool.drain(..)).chain(self.fresh.drain(..).map(|f| f.1)) {
            cx.tree.remove_now(cell.id);
        }
        for slot in self.slots.drain(..) {
            cx.tree.remove_now(slot.node);
        }
    }

    /// Measures the cell of a row (a clean cell costs nothing) and records the size.
    fn measure_cell(&mut self, cx: &mut LayoutCx, id: ControlId, index: usize) {
        let width = self.key.map_or(0.0, |k| k.0);
        let size = cx.measure_child(id, width, f32::INFINITY);
        let first = self.sizes.set(index, size.height);
        if !self.sizes.first_only || first {
            self.max_width = if self.sizes.first_only { size.width } else { self.max_width.max(size.width) };
        }
    }

    /// Measures a row wherever it is: with its own cell when it has one, else with a spare one.
    fn measure_item(&mut self, cx: &mut LayoutCx, index: usize) {
        match self.realized(index) {
            Some(id) => self.measure_cell(cx, id, index),
            None => {
                let cell = self.acquire(cx, index);
                self.release(cx, cell);
            }
        }
    }

    /// Makes the size of a row in the window exact: a row without a cell gets the one it will
    /// be shown with.
    fn ensure(&mut self, cx: &mut LayoutCx, index: usize) {
        if let Some(id) = self.realized(index) {
            if self.cells_dirty {
                self.measure_cell(cx, id, index);
            }
        } else {
            let cell = self.acquire(cx, index);
            self.fresh.push((index, cell));
        }
    }

    /// Applies what the app reported, then what the item count still says: more items = appended,
    /// fewer = everything is new. `anchor` follows its item.
    fn sync(&mut self, cx: &mut LayoutCx, anchor: &mut Option<(usize, f32)>) {
        let mut pending = std::mem::take(&mut self.pending);
        for change in pending.drain(..) {
            self.apply(cx, change, anchor);
        }
        self.pending = pending;
        let (len, count) = (self.sizes.len, (self.count_of)(cx.state));
        if count > len {
            self.apply(cx, Change::Inserted(len, count - len), anchor);
        } else if count < len {
            self.apply(cx, Change::Reset, anchor);
        }
    }

    fn apply(&mut self, cx: &mut LayoutCx, change: Change, anchor: &mut Option<(usize, f32)>) {
        if self.slotted {
            return self.apply_to_slots(cx, change);
        }
        let len = self.sizes.len;
        match change {
            Change::Inserted(index, count) if index <= len => {
                self.sizes.insert(index, count);
                let above = anchor.as_mut().filter(|(a, _)| index <= *a).map(|(a, _)| *a += count).is_some();
                self.reindex(cx, index, index, |i| Some(if i >= index { i + count } else { i }));
                // MeasureVisible, as DrawnUi.React: rows that come in above the rows on screen are
                // measured now, as many as the frame's budget takes, so the scroll takes their real
                // extent in this frame. The rest is measured ahead in the next frames; the rows on
                // screen stay where they are either way.
                if above && self.list && self.key.is_some_and(|k| k.2 == MeasuringStrategy::MeasureVisible) {
                    for row in index..index + count {
                        if !self.work.fits(cx.tree.synthetic_clock) {
                            break;
                        }
                        self.measure_item(cx, row);
                        self.work.rows += 1;
                    }
                }
            }
            Change::Removed(index, count) if index + count <= len => {
                self.sizes.remove(index, count);
                *anchor = match *anchor {
                    Some((a, offset)) if index + count <= a => Some((a - count, offset)),
                    Some(kept) if index > kept.0 => Some(kept),
                    _ => None,
                };
                self.reindex(cx, index, index + count, |i| match i {
                    i if i >= index + count => Some(i - count),
                    i if i >= index => None,
                    i => Some(i),
                });
            }
            Change::Moved(from, to) if from < len && to < len => {
                if from == to {
                    return;
                }
                self.sizes.move_row(from, to);
                let (low, high) = (from.min(to), from.max(to));
                let moved = move |i: usize| match i {
                    i if i == from => to,
                    i if from < to && i > from && i <= to => i - 1,
                    i if to < from && i >= to && i < from => i + 1,
                    i => i,
                };
                // The scroll keeps its offset, as upstream and DrawnUi.React do on a Move.
                *anchor = None;
                self.reindex(cx, low, high + 1, |i| Some(moved(i)));
            }
            Change::Reordered(order) if order.len() == len && is_permutation(&order) => {
                self.sizes.reorder(&order);
                // Where each item went: old index to new index.
                let mut now = vec![0; len];
                for (new, old) in order.iter().enumerate() {
                    now[*old] = new;
                }
                *anchor = None;
                self.reindex(cx, 0, len, |i| Some(now[i]));
            }
            Change::Changed(index) if index < len => {
                self.sizes.set(index, f32::NAN);
                self.frontier = self.frontier.min(index);
                if let Some(cell) = index.checked_sub(self.first).and_then(|at| self.cells.get_mut(at)) {
                    self.touched = bind_cell(&mut self.bind, cell, cx, index);
                }
                self.pool.iter_mut().filter(|cell| cell.bound == Some(index)).for_each(|cell| cell.bound = None);
            }
            // A reset, or a change that does not fit the list: everything is bound and measured again.
            _ => {
                *anchor = None;
                self.release_all(cx);
                self.pool.iter_mut().for_each(|cell| cell.bound = None);
                self.sizes.reset((self.count_of)(cx.state), self.sizes.first_only);
                (self.frontier, self.max_width) = (0, 0.0);
            }
        }
    }

    /// Rows from `index` on got other indices; the rows from `end` on kept their item. Cells
    /// keep their item too: below the change they stay where they are under the new index, and
    /// the ones the change went through are released and come back to their rows without a bind.
    fn reindex(&mut self, cx: &mut LayoutCx, index: usize, end: usize, new_index: impl Fn(usize) -> Option<usize>) {
        self.frontier = self.frontier.min(index);
        if end <= self.first {
            self.first = new_index(self.first).unwrap_or(self.first);
            for (at, cell) in self.cells.iter_mut().enumerate() {
                cell.bound = Some(self.first + at);
                show(cx, cell.id, cell.bound);
            }
        } else if index < self.first + self.cells.len() {
            self.release_all(cx);
        }
        for cell in &mut self.pool {
            cell.bound = cell.bound.and_then(&new_index);
        }
    }

    pub(crate) fn measure(&mut self, cx: &mut LayoutCx, p: &LayoutProps, width: f32, _height: f32) -> Size {
        cx.track_viewport();
        self.list = true;
        self.work = Work::new(p.measure_budget);
        let scale = cx.scale;
        // Whole pixels as a stack makes them: the box of the cells, what the padding adds, the gap.
        let (inner, padding) = layout::content_box(&cx.base().p, width, f32::INFINITY, scale);
        let mut anchor = self.anchor(cx);
        let lead = cx.base().p.padding.top * scale;
        (self.gap, self.lead) = (layout::snap(p.spacing * scale), lead.min(layout::snap(lead)));

        let key = (inner.width, scale, p.measure_items_strategy, p.recycling_template);
        if self.key != Some(key) {
            // Another width or scale: every size is void. Another mode: the cells too.
            if self.key.is_some_and(|k| k.3 != key.3) {
                self.drop_cells(cx);
            }
            self.key = Some(key);
            self.sizes.reset(self.sizes.len, key.2 == MeasuringStrategy::MeasureFirst);
            (self.frontier, self.max_width, anchor) = (0, 0.0, None);
        }
        self.sync(cx, &mut anchor);

        let len = self.sizes.len;
        if key.2 == MeasuringStrategy::MeasureAll {
            while self.frontier < len {
                if !self.sizes.known(self.frontier) {
                    self.measure_item(cx, self.frontier);
                }
                self.frontier += 1;
            }
        } else if len > 0 && !self.sizes.known(0) && self.sizes.measured == 0 {
            // MeasureFirst: the size of every row. MeasureVisible: the first estimate.
            self.measure_item(cx, 0);
        }

        if let Some((index, before)) = anchor {
            self.shift += self.sizes.offset(index, self.gap) - before;
        }
        self.cells_dirty = true;
        self.total = self.sizes.total(self.gap);
        Size::new(self.max_width + padding.width, self.total + padding.height)
    }

    /// Not a list (a templated Row, Wrap, Grid or split Column): what the layout measures and
    /// places like children, in item order (a templated layout has no other children). With
    /// RecyclingTemplate Enabled a slot per item (`realize_slots`); Disabled a visible cell per
    /// item. `slots`: the caller binds views to the slots in its arrange (`after_arrange`); one
    /// that does not (a carousel) gets a cell per item. Runs at the start of the layout's measure.
    /// True when a cell or slot was made, bound, shown or hidden.
    pub(crate) fn realize_all(&mut self, cx: &mut LayoutCx, p: &LayoutProps, slots: bool) -> bool {
        self.list = false;
        let key = (0.0, cx.scale, p.measure_items_strategy, p.recycling_template);
        if self.key != Some(key) {
            if self.key.is_some_and(|k| k.3 != key.3) {
                self.drop_cells(cx);
            }
            self.key = Some(key);
            // No row sizes to keep: the layout measures the cells.
            self.sizes.reset(self.sizes.len, true);
        }
        self.slotted = slots && p.recycling_template == RecyclingTemplate::Enabled;
        if self.slotted {
            return self.realize_slots(cx);
        }
        self.sync(cx, &mut None);
        match self.sizes.len {
            0 => self.release_all(cx),
            len => self.realize(cx, 0, len - 1),
        }
        // Fewer items than before: the spare cells above the pool limit go.
        self.trim_pool(cx, self.cells.len(), p.item_template_pool_size);
        // The layout takes its children in order: the cells in item order, the spare ones after.
        if let Some(node) = cx.tree.node_mut(cx.id) {
            node.children.clear();
            node.children.extend(self.cells.iter().chain(&self.pool).map(|c| c.id));
        }
        std::mem::take(&mut self.touched)
    }

    /// Not a list: remembers where the layout put each item, for `item_offset_pixels` and
    /// `item_at_pixels`, and binds views to the slots that can be seen. Runs at the end of the
    /// layout's arrange.
    pub(crate) fn after_arrange(&mut self, cx: &mut LayoutCx, p: &LayoutProps) {
        let rect = cx.base().rect;
        if self.lays_out(rect) {
            let ids = self.slots.iter().map(|s| s.node).chain(self.cells.iter().map(|c| c.id));
            self.laid.clear();
            self.laid.extend(ids.map(|id| {
                let r = cx.child_base(id).rect;
                (r.top - rect.top, r.height())
            }));
        }
        if self.slotted {
            (self.relaid, self.arranged_at) = (false, rect);
            self.show_slots(cx, p.item_template_pool_size);
        }
    }

    /// Not a list: whether the arrange must place the children again. A recycled layout that was
    /// not measured and did not move since its last arrange (an ancestor scrolled) does not.
    pub(crate) fn lays_out(&self, rect: Rect) -> bool {
        !self.slotted || self.relaid || self.arranged_at != rect
    }

    pub(crate) fn arrange(&mut self, cx: &mut LayoutCx, p: &LayoutProps) {
        let (gap, len) = (self.gap, self.sizes.len);
        if !self.cells_dirty {
            // Not measured in this frame: its budget starts here.
            self.work = Work::new(p.measure_budget);
        }
        let window = self.window(cx);
        let mut shift = std::mem::take(&mut self.shift);
        if let Some((top, bottom)) = window {
            let (mut first, mut last) = self.sizes.range(top, bottom, gap);
            // By the estimate first, so the rows that enter find cells in the pool.
            self.release_outside(cx, first, last);
            let anchor = (first..=last).find(|i| self.sizes.known(*i)).unwrap_or(first);
            let before = self.sizes.offset(anchor, gap);
            if self.cells_dirty || !(first..=last).all(|i| self.sizes.known(i)) {
                // Rows of unknown size are measured as they are met, going up and down from the
                // anchor row, which stays where it is on screen whatever the rows above turn out to be.
                let lead = before - top;
                let from_top = |sizes: &Sizes, row: usize| sizes.offset(row, gap) - sizes.offset(anchor, gap) + lead;
                self.ensure(cx, anchor);
                (first, last) = (anchor, anchor);
                while first > 0 && from_top(&self.sizes, first) > 0.0 {
                    first -= 1;
                    self.ensure(cx, first);
                }
                while last + 1 < len && from_top(&self.sizes, last + 1) < bottom - top {
                    last += 1;
                    self.ensure(cx, last);
                }
            }
            self.realize(cx, first, last);
            debug_assert!(self.fresh.is_empty(), "every row met on the way is in the window");
            self.trim_pool(cx, self.cells.len(), p.item_template_pool_size);
            self.measure_ahead(cx);
            shift += self.sizes.offset(anchor, gap) - before;
        } else {
            self.release_all(cx);
        }
        let dirty = std::mem::take(&mut self.cells_dirty);
        self.place(cx, dirty);

        // The scroll takes the shift right after this arrange. When nobody does, the content has
        // moved instead: the next arrange realizes the rows for where it is.
        cx.base_mut().viewport_shift = shift;
        let total = self.sizes.total(gap);
        let grew = total - self.total;
        if grew != 0.0 {
            self.total = total;
            let base = cx.base_mut();
            let bp = &base.p;
            // Measured unbounded (the content of a scroll) and sized by its rows: the new height is
            // set as a measure would set it, and the parents read it at their next arrange.
            if base.last_constraints.1 == f32::INFINITY
                && !fills_height(bp)
                && bp.height_request < 0.0
                && bp.minimum_height_request < 0.0
                && bp.maximum_height_request < 0.0
                && bp.lock_ratio == 0.0
            {
                base.measured.height += grew;
                base.rect.bottom += grew;
            } else {
                cx.tree.invalidate(cx.id, Dirty::MEASURE);
            }
        }
        let measuring = self.frontier < len && p.measure_budget != MeasureBudget::Items(0);
        if grew != 0.0 || shift != 0.0 || (measuring && p.measure_items_strategy == MeasuringStrategy::MeasureVisible) {
            // This arrange runs again on the next frame.
            cx.tree.needs_frame = true;
            let mut current = Some(cx.id);
            while let Some(node) = current.and_then(|c| cx.tree.node_mut(c)) {
                node.base.need_arrange = true;
                current = node.parent;
            }
        }
    }

    /// DrawnUI GetTemplatesPoolLimit: the pool follows the `visible` cells in use,
    /// `item_template_pool_size` caps it. Rows on screen always get a cell.
    fn trim_pool(&mut self, cx: &mut LayoutCx, visible: usize, pool_size: i32) {
        if !self.recycles() {
            return;
        }
        let limit = if pool_size > 0 { pool_size as usize } else { (visible * 4).max(visible + RESERVE_TEMPLATES) };
        while self.pool.len() > limit.saturating_sub(visible)
            && let Some(cell) = self.pool.pop()
        {
            cx.tree.remove_now(cell.id);
        }
    }

    /// MeasureVisible: measures rows nobody has seen yet, in index order, inside the frame's
    /// budget (upstream measures them on a background thread).
    fn measure_ahead(&mut self, cx: &mut LayoutCx) {
        if self.key.is_none_or(|k| k.2 != MeasuringStrategy::MeasureVisible) {
            return;
        }
        while self.frontier < self.sizes.len {
            if !self.sizes.known(self.frontier) {
                if !self.work.fits(cx.tree.synthetic_clock) {
                    break;
                }
                self.measure_item(cx, self.frontier);
                self.work.rows += 1;
            }
            self.frontier += 1;
        }
    }

    /// Arranges the realized cells at their rows. Nothing is touched when the same rows are at
    /// the same place (sizes that changed below them do not move them), unless `force` says a
    /// cell may have changed by itself.
    fn place(&mut self, cx: &mut LayoutCx, force: bool) {
        let (scale, gap) = (cx.scale, self.gap);
        let (rect, inner) = (cx.base().rect, layout::content_rect(cx.base(), scale));
        let top = if self.cells.is_empty() { 0.0 } else { self.sizes.offset(self.first, gap) };
        let placed = (self.first, self.cells.len(), top, rect.left, rect.top, rect.width());
        if placed == self.placed && !force {
            return;
        }
        self.placed = placed;
        // As upstream's stack: the cells are placed in the box they were measured in, in whole
        // pixels from its corner, and the result moves to where the list is.
        let width = inner.width().min(self.key.map_or(0.0, |k| k.0));
        let mut y = top;
        for (at, row) in self.cells.iter().enumerate() {
            let height = self.sizes.height(self.first + at);
            let base = cx.child_base(row.id);
            let bp = &base.p;
            let (left, right) = cross_slot(bp.horizontal_options, 0.0, width, base.measured.width);
            // A row that starts at the top of its slot is as tall as `place` makes it: a pixel more
            // than measured where the way to points and back ends above it (see `layout::place`).
            let own = bp.vertical_options == LayoutOptions::Start && bp.horizontal_options != LayoutOptions::Start;
            let slot = Rect::new(left, y, right, if own { f32::INFINITY } else { y + height });
            cx.arrange_child(row.id, cell(base, slot, scale).with_offset((inner.left, inner.top)));
            y += height + gap;
        }
        // Other rows, or the same rows elsewhere: the caches above show the old ones.
        let mut current = Some(cx.id);
        while let Some(node) = current.and_then(|c| cx.tree.node_mut(c)) {
            node.base.content_epoch = node.base.content_epoch.wrapping_add(1);
            current = node.parent;
        }
    }
}

// ---------------------------------------------------------------- recycled layouts

/// A templated Row, Wrap, Grid or split Column with RecyclingTemplate Enabled (DrawnUi.React
/// `TemplatedSlot`, C# DrawStack): the layout's children are slots, one per item, which it
/// measures and places like any child. A slot is measured through a view from the pool bound to
/// its item, which goes back right after unless the item can be seen, and keeps the size for the
/// same item and constraints. After the arrange, views are bound to the slots that can be seen.
impl Items {
    /// Makes one slot per item, applies what the app reported, and gives the layout its children:
    /// the slots in item order, the spare views (hidden) after them.
    fn realize_slots(&mut self, cx: &mut LayoutCx) -> bool {
        // Bound to what can be seen: arranged again when an ancestor scrolls.
        cx.track_viewport();
        self.sync(cx, &mut None);
        let len = self.sizes.len;
        while self.slots.len() > len
            && let Some(slot) = self.slots.pop()
        {
            self.drop_slot(cx, slot);
        }
        while self.slots.len() < len {
            let slot = self.mount_slot(cx, self.slots.len());
            self.slots.push(slot);
        }
        (self.relaid, self.props_changed) = (true, false);
        self.write_children(cx);
        std::mem::take(&mut self.touched)
    }

    fn mount_slot(&mut self, cx: &mut LayoutCx, index: usize) -> Slot {
        self.touched = true;
        if self.model.is_none() {
            // The first view, before any bind: the layout properties the first slots start from.
            let cell = self.create(cx);
            self.model = cx.tree.node(cell.id).map(|n| Placement::of(&n.base.p));
            self.release(cx, cell);
        }
        // Mounting asks for another frame; the slot is laid out in this one.
        let pending = cx.tree.needs_frame;
        let node = cx.tree.mount(Some(cx.id), Build::new(SlotControl));
        cx.tree.needs_frame = pending;
        if let Some(n) = cx.tree.node_mut(node) {
            if let Some(model) = self.model {
                model.apply(&mut n.base.p);
            }
            n.base.context_index = Some(index);
            // Room for its view now: a scrolled frame that binds one allocates nothing.
            n.children.reserve_exact(1);
        }
        Slot { node, view: None, measured_for: None }
    }

    /// Removes a slot whose item is gone; its view goes back to the pool, free for any item.
    fn drop_slot(&mut self, cx: &mut LayoutCx, mut slot: Slot) {
        if let Some(mut cell) = Self::detach(cx, &mut slot) {
            cell.bound = None;
            self.release(cx, cell);
        }
        cx.tree.remove_now(slot.node);
        self.touched = true;
    }

    /// Takes the view out of a slot: it is a child of the layout again.
    fn detach(cx: &mut LayoutCx, slot: &mut Slot) -> Option<Cell> {
        let cell = slot.view.take()?;
        if let Some(n) = cx.tree.node_mut(slot.node) {
            n.children.clear();
        }
        if let Some(n) = cx.tree.node_mut(cell.id) {
            n.parent = Some(cx.id);
        }
        stale_above(cx.tree, slot.node);
        Some(cell)
    }

    /// The layout's children: the slots in item order, the spare views after them.
    fn write_children(&self, cx: &mut LayoutCx) {
        if let Some(node) = cx.tree.node_mut(cx.id) {
            node.children.clear();
            node.children.extend(self.slots.iter().map(|s| s.node).chain(self.pool.iter().map(|c| c.id)));
        }
    }

    /// The slots from `from` on hold other items now: their index and their view's.
    fn reindex_slots(&mut self, cx: &mut LayoutCx, from: usize) {
        for (index, slot) in self.slots.iter_mut().enumerate().skip(from) {
            if let Some(n) = cx.tree.node_mut(slot.node) {
                n.base.context_index = Some(index);
            }
            if let Some(cell) = &mut slot.view {
                cell.bound = Some(index);
                show(cx, cell.id, Some(index));
            }
        }
    }

    /// What the app reported, for slots. Slots and the views in them keep their items: an insert
    /// or a move measures and binds nothing that was there.
    fn apply_to_slots(&mut self, cx: &mut LayoutCx, change: Change) {
        let len = self.sizes.len;
        self.touched = true;
        let remap = |pool: &mut Vec<Cell>, new_index: &dyn Fn(usize) -> Option<usize>| {
            for cell in pool {
                cell.bound = cell.bound.and_then(new_index);
            }
        };
        match change {
            Change::Inserted(index, count) if index <= len => {
                self.sizes.insert(index, count);
                let mounted: Vec<Slot> = (index..index + count).map(|at| self.mount_slot(cx, at)).collect();
                self.slots.splice(index..index, mounted);
                self.reindex_slots(cx, index + count);
                remap(&mut self.pool, &|i| Some(if i >= index { i + count } else { i }));
            }
            Change::Removed(index, count) if index + count <= len => {
                self.sizes.remove(index, count);
                let gone: Vec<Slot> = self.slots.drain(index..index + count).collect();
                for slot in gone {
                    self.drop_slot(cx, slot);
                }
                self.reindex_slots(cx, index);
                remap(&mut self.pool, &|i| match i {
                    i if i >= index + count => Some(i - count),
                    i if i >= index => None,
                    i => Some(i),
                });
            }
            Change::Moved(from, to) if from < len && to < len => {
                let slot = self.slots.remove(from);
                self.slots.insert(to, slot);
                self.reindex_slots(cx, from.min(to));
                remap(&mut self.pool, &|i| {
                    Some(match i {
                        i if i == from => to,
                        i if from < to && i > from && i <= to => i - 1,
                        i if to < from && i >= to && i < from => i + 1,
                        i => i,
                    })
                });
            }
            Change::Reordered(order) if order.len() == len && is_permutation(&order) => {
                let mut was: Vec<Option<Slot>> = self.slots.drain(..).map(Some).collect();
                self.slots.extend(order.iter().filter_map(|old| was[*old].take()));
                self.reindex_slots(cx, 0);
                let mut now = vec![0; len];
                for (new, old) in order.iter().enumerate() {
                    now[*old] = new;
                }
                remap(&mut self.pool, &|i| now.get(i).copied());
            }
            Change::Changed(index) if index < len => {
                let slot = &mut self.slots[index];
                slot.measured_for = None;
                if let Some(cell) = &mut slot.view {
                    bind_cell(&mut self.bind, cell, cx, index);
                }
                remap(&mut self.pool, &|i| (i != index).then_some(i));
            }
            // A reset, or a change that does not fit: every item is bound and measured again.
            _ => {
                for index in 0..self.slots.len() {
                    self.slots[index].measured_for = None;
                    if let Some(mut cell) = Self::detach(cx, &mut self.slots[index]) {
                        cell.bound = None;
                        self.release(cx, cell);
                    }
                }
                remap(&mut self.pool, &|_| None);
                self.sizes.reset((self.count_of)(cx.state), true);
            }
        }
    }

    /// The slot a child of the layout is, if it is one.
    fn slot_of(&self, cx: &LayoutCx, child: ControlId) -> Option<usize> {
        if !self.slotted {
            return None;
        }
        let index = cx.child_base(child).context_index?;
        (self.slots.get(index)?.node == child).then_some(index)
    }

    /// Measures a child of the layout: a slot through its item, any other child itself.
    pub(crate) fn measure_child(&mut self, cx: &mut LayoutCx, child: ControlId, width: f32, height: f32) -> Size {
        match self.slot_of(cx, child) {
            Some(index) => self.measure_slot(cx, index, width, height),
            None => cx.measure_child(child, width, height),
        }
    }

    /// Places a child of the layout: a slot records where its item goes and moves its view there.
    pub(crate) fn arrange_child(&mut self, cx: &mut LayoutCx, child: ControlId, destination: Rect) {
        let Some(index) = self.slot_of(cx, child) else { return cx.arrange_child(child, destination) };
        let rect = arranged_rect(cx.child_base(child), destination, cx.scale);
        if let Some(n) = cx.tree.node_mut(child) {
            (n.base.rect, n.base.last_destination, n.base.need_arrange) = (rect, destination, false);
        }
        // Views on screen follow their slots, also when a cache above skips the drawing pass:
        // taps hit them where they are.
        if let Some(cell) = &self.slots[index].view {
            cx.arrange_child(cell.id, destination);
        }
    }

    /// True once after a pass in which a slot took other layout properties than the layout read
    /// for it: the layout lays its children out once more.
    pub(crate) fn take_props_changed(&mut self) -> bool {
        std::mem::take(&mut self.props_changed)
    }

    /// Measures item `index` for its slot: the same item with the same constraints keeps its size,
    /// else the view drawn in the slot, or a spare one bound to the item, is measured, and the slot
    /// takes its size and what the template decided for the item.
    fn measure_slot(&mut self, cx: &mut LayoutCx, index: usize, width: f32, height: f32) -> Size {
        let node = self.slots[index].node;
        let key = (width.to_bits(), height.to_bits(), cx.scale.to_bits());
        let base = cx.child_base(node);
        if !base.need_measure && self.slots[index].measured_for == Some(key) {
            return base.measured;
        }
        let (cell, spare) = match self.slots[index].view.take() {
            Some(cell) => (cell, false),
            None => (self.acquire(cx, index), true),
        };
        let size = cx.measure_child(cell.id, width, height);
        let (placement, measured, scale) = {
            let view = cx.child_base(cell.id);
            (Placement::of(&view.p), view.measured, view.scale)
        };
        if let Some(n) = cx.tree.node_mut(node) {
            self.props_changed |= Placement::of(&n.base.p) != placement;
            placement.apply(&mut n.base.p);
            (n.base.measured, n.base.scale, n.base.need_measure) = (measured, scale, false);
        }
        self.model = Some(placement);
        self.slots[index].measured_for = Some(key);
        match spare {
            true => self.release(cx, cell),
            false => self.slots[index].view = Some(cell),
        }
        size
    }

    /// Where slots are drawn (C# GetOnScreenVisibleArea): what can be seen of the layout, grown by
    /// `virtualisation_inflated`. Under a cache (the layout's own, or an ancestor's below the
    /// nearest scroll) every slot: the cache is blitted later at other offsets. `None` = all.
    fn slot_area(&self, cx: &LayoutCx) -> Option<Rect> {
        let mut current = Some(cx.id);
        while let Some(node) = current.and_then(|id| cx.tree.node(id)) {
            let scroll = node.id != cx.id && (!node.base.content_offset.is_zero() || cx.tree.find::<SkiaScroll>(node.id).is_some());
            if scroll {
                break;
            }
            if node.base.p.use_cache.resolved() != CacheType::None {
                return None;
            }
            current = node.parent;
        }
        let (visible, inflate) = (cx.visible_rect(), cx.base().p.virtualisation_inflated * cx.scale);
        Some(Rect::new(visible.left - inflate, visible.top - inflate, visible.right + inflate, visible.bottom + inflate))
    }

    /// Binds a view to every slot that can be seen and gives the views of the others back to the
    /// pool (C# DrawStack). Nothing changes, and nothing is allocated, while the same slots show.
    fn show_slots(&mut self, cx: &mut LayoutCx, pool_size: i32) {
        let area = self.slot_area(cx);
        let mut changed = false;
        for index in 0..self.slots.len() {
            let base = cx.child_base(self.slots[index].node);
            let r = base.rect;
            let seen = base.p.is_visible
                && area.is_none_or(|a| r.right > a.left && r.left < a.right && r.bottom > a.top && r.top < a.bottom);
            match (seen, self.slots[index].view.is_some()) {
                (true, false) => {
                    self.attach(cx, index);
                    changed = true;
                }
                (false, true) => {
                    if let Some(cell) = Self::detach(cx, &mut self.slots[index]) {
                        self.release(cx, cell);
                    }
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            let in_use = self.slots.iter().filter(|s| s.view.is_some()).count();
            self.trim_pool(cx, in_use, pool_size);
            self.write_children(cx);
        }
    }

    /// Puts a view bound to item `index` in its slot, measured for it and placed there. A view
    /// last measured for another item is measured again: its bind made it dirty, or the slot's
    /// constraints differ. The measure keeps a size only for the same constraints of a control
    /// nothing changed in (the whole box), so C#'s Wrap bug (a size key that was 0 on both sides,
    /// 3d7bd78f) has no counterpart here.
    fn attach(&mut self, cx: &mut LayoutCx, index: usize) {
        let cell = self.acquire(cx, index);
        let node = self.slots[index].node;
        if let Some((w, h, _)) = self.slots[index].measured_for {
            cx.measure_child(cell.id, f32::from_bits(w), f32::from_bits(h));
        }
        if let Some(n) = cx.tree.node_mut(cell.id) {
            n.parent = Some(node);
        }
        if let Some(n) = cx.tree.node_mut(node) {
            n.children.push(cell.id);
        }
        let destination = cx.child_base(node).last_destination;
        cx.arrange_child(cell.id, destination);
        stale_above(cx.tree, node);
        self.slots[index].view = Some(cell);
    }
}

// ---------------------------------------------------------------- public surface

impl<T: Control> Build<T> {
    /// Gives the layout templated children (DrawnUI ItemsSource + ItemTemplate): `count` reads the
    /// number of items from the app state, `template` makes one cell and the handles into it,
    /// `bind` puts item `index` into a cell and runs whenever a cell gets an item. A Column is a
    /// list: cells exist for the visible rows only. A Row, Wrap, Grid or split Column lays out every
    /// item; with `recycling_template` Enabled (the default) cells from a pool are bound to the items
    /// that can be seen, Disabled gives every item a cell of its own (a templated layout has no
    /// other children). Tell the
    /// layout what changed with `Cx::items_inserted`, `items_removed`, `items_changed`,
    /// `items_moved`, `items_reset`; a count that changed without such a call is read as
    /// "appended" when it grew and as a reset when it shrank. The control is a SkiaLayout or
    /// embeds one (a SkiaShape, a decorated grid).
    pub fn items<S: Any, C: Into<Detached>, H: 'static>(
        mut self,
        count: impl Fn(&S) -> usize + 'static,
        mut template: impl FnMut() -> (C, H) + 'static,
        mut bind: impl FnMut(&H, &S, usize, &mut Cx<'_>) + 'static,
    ) -> Self {
        let count = Rc::new(count);
        let counted = count.clone();
        let layout = part_mut::<SkiaLayout>(self.control_mut()).expect("items: the control embeds no SkiaLayout");
        layout.items = Some(Box::new(Items {
            count_of: Rc::new(move |s: &dyn Any| counted(state::<S>(s))),
            template: Box::new(move || {
                let (cell, handles) = template();
                (cell.into(), Box::new(handles) as Box<dyn Any>)
            }),
            bind: Box::new(move |handles: &dyn Any, s: &dyn Any, index: usize, cx: &mut Cx<'_>| {
                bind(handles.downcast_ref::<H>().expect("handles of this template"), state::<S>(s), index, cx)
            }),
            pending: Vec::new(),
            sizes: Sizes::default(),
            key: None,
            gap: 0.0,
            lead: 0.0,
            max_width: 0.0,
            cells: VecDeque::new(),
            first: 0,
            pool: Vec::new(),
            fresh: Vec::new(),
            frontier: 0,
            work: Work::new(MeasureBudget::Items(0)),
            cells_dirty: false,
            shift: 0.0,
            total: 0.0,
            placed: (0, 0, 0.0, 0.0, 0.0, 0.0),
            list: true,
            laid: Vec::new(),
            created: 0,
            touched: false,
            slotted: false,
            slots: Vec::new(),
            model: None,
            props_changed: false,
            relaid: false,
            arranged_at: Rect::default(),
        }));
        // Another count: the list is measured again, and takes it as appended or as a reset.
        self.observe(move |me: &mut Mut<'_, T>, app: &S| {
            let layout = part_mut::<SkiaLayout>(me.control_mut());
            if layout.is_some_and(|l| l.items.as_ref().is_some_and(|items| items.sizes.len != count(app))) {
                me.mark(Dirty::MEASURE);
            }
        })
    }
}

impl SkiaLayout {
    /// Number of items the list is laid out for; 0 for a layout without items.
    pub fn items_count(&self) -> usize {
        self.items.as_ref().map_or(0, |items| items.sizes.len)
    }

    /// Pixels from the layout's top to the row of item `index`: what a scroll needs for
    /// ScrollToIndex. Exact when every row above is measured, otherwise estimated. Not a list:
    /// where the last arrange put the item.
    pub fn item_offset_pixels(&self, index: usize) -> f32 {
        let Some(items) = self.items.as_ref() else { return 0.0 };
        match items.list {
            true => items.lead + items.sizes.offset(index.min(items.sizes.len), items.gap),
            false => items.laid.get(index).map_or(0.0, |laid| laid.0),
        }
    }

    /// Height of the row of item `index` in pixels, estimated until it was measured.
    pub fn item_height_pixels(&self, index: usize) -> f32 {
        let Some(items) = self.items.as_ref().filter(|items| index < items.sizes.len) else { return 0.0 };
        match items.list {
            true => items.sizes.height(index),
            false => items.laid.get(index).map_or(0.0, |laid| laid.1),
        }
    }

    /// The item at `y` pixels from the layout's top (the gap below a row belongs to it), `None`
    /// above the first row or below the last one. For a drag: the slot under the pointer.
    pub fn item_at_pixels(&self, y: f32) -> Option<usize> {
        let items = self.items.as_ref().filter(|items| items.sizes.len > 0)?;
        match items.list {
            true => {
                let y = y - items.lead;
                (y >= 0.0 && y < items.sizes.total(items.gap)).then(|| items.sizes.index_at(y, items.gap))
            }
            false => items.laid.iter().position(|(top, height)| y >= *top && y < top + height),
        }
    }

    /// The cell showing item `index` right now, if the item is on screen (DrawnUI
    /// GetCellInUseOrNull): its rect is where the row is drawn.
    pub fn cell_in_use(&self, index: usize) -> Option<ControlId> {
        self.items.as_ref()?.realized(index)
    }

    /// The cells on screen with the item each shows, in item order (DrawnUI Views of a templated
    /// layout). A recycled layout that is not a list: the views bound to the slots that can be seen.
    pub fn cells_in_use(&self) -> impl Iterator<Item = (usize, ControlId)> + '_ {
        let items = self.items.as_deref();
        let first = items.map_or(0, |items| items.first);
        let cells = items.into_iter().flat_map(|items| items.cells.iter()).enumerate().map(move |(at, cell)| (first + at, cell.id));
        let slots = items.into_iter().flat_map(|items| items.slots.iter()).enumerate();
        cells.chain(slots.filter_map(|(index, slot)| Some((index, slot.view.as_ref()?.id))))
    }

    /// One line of diagnostics as DrawnUi.React DebugString: items, the rows with a cell, the
    /// measured rows (MeasureVisible), cells in use, spare cells, cells made.
    pub fn debug_string(&self) -> String {
        let Some(items) = self.items.as_ref() else { return "no items".to_owned() };
        let (n, m) = (items.sizes.len, self.measured_items());
        let visible = self.visible_items().map_or("-".to_owned(), |(first, last)| format!("{first}-{last}"));
        let estimating = items.list && items.key.is_some_and(|k| k.2 == MeasuringStrategy::MeasureVisible);
        let measured = if estimating { format!(" measured {m}/{n}") } else { String::new() };
        let in_use = items.cells.len() + items.slots.iter().filter(|s| s.view.is_some()).count();
        format!("items {n} visible {visible}{measured} inuse {in_use} pool {} created {}", items.pool.len(), items.created)
    }

    /// False while the row's height is an estimate (MeasureVisible).
    pub fn is_item_measured(&self, index: usize) -> bool {
        self.items.as_ref().is_some_and(|items| index < items.sizes.len && items.sizes.known(index))
    }

    /// How many rows have a measured height.
    pub fn measured_items(&self) -> usize {
        let Some(sizes) = self.items.as_ref().map(|items| &items.sizes) else { return 0 };
        if sizes.first_only { if sizes.uniform.is_nan() { 0 } else { sizes.len } } else { sizes.measured }
    }

    /// First and last item that have a cell right now (DrawnUI FirstVisibleIndex / LastVisibleIndex):
    /// the rows in the viewport plus `virtualisation_inflated`.
    pub fn visible_items(&self) -> Option<(usize, usize)> {
        let items = self.items.as_ref()?;
        if items.slotted {
            let mut seen = items.slots.iter().enumerate().filter(|(_, s)| s.view.is_some()).map(|(index, _)| index);
            let first = seen.next()?;
            return Some((first, seen.last().unwrap_or(first)));
        }
        (!items.cells.is_empty()).then(|| (items.first, items.first + items.cells.len() - 1))
    }
}

impl Cx<'_> {
    /// Where the row of item `index` is drawn right now, in canvas pixels (the scroll offsets of
    /// the ancestors applied, render transforms not), while the row has a cell (DrawnUi.React
    /// `GetExistingViewAtIndex(index).DrawingRect`). For a drag: where a floating copy of the row
    /// starts, and the slot it glides into.
    pub fn item_rect(&self, list: impl Into<ControlId>, index: usize) -> Option<Rect> {
        let tree = &*self.tree;
        let cell = tree.find::<SkiaLayout>(list)?.cell_in_use(index)?;
        let mut rect = tree.base(cell)?.rect;
        let mut current = tree.parent(cell);
        while let Some(ancestor) = current {
            rect.offset(tree.base(ancestor)?.content_offset);
            current = tree.parent(ancestor);
        }
        Some(rect)
    }

    fn items_change(&mut self, list: ControlId, change: Change) {
        let Some(mut layout) = self.tree.find_mut::<SkiaLayout>(list) else { return };
        if let Some(items) = layout.control_mut().items.as_deref_mut() {
            items.pending.push(change);
            layout.mark(Dirty::MEASURE);
        }
    }

    /// The items of a list were replaced: every row is bound and measured again.
    pub fn items_reset(&mut self, list: impl Into<ControlId>) {
        self.items_change(list.into(), Change::Reset)
    }

    /// `count` items were inserted at `index`. Every measured size is kept; rows above the
    /// viewport report their extent to the scroll, so the rows on screen stay where they are.
    pub fn items_inserted(&mut self, list: impl Into<ControlId>, index: usize, count: usize) {
        self.items_change(list.into(), Change::Inserted(index, count))
    }

    /// `count` items were removed at `index`.
    pub fn items_removed(&mut self, list: impl Into<ControlId>, index: usize, count: usize) {
        self.items_change(list.into(), Change::Removed(index, count))
    }

    /// The item at `index` has other content: its cell is bound again and only its row is measured.
    pub fn items_changed(&mut self, list: impl Into<ControlId>, index: usize) {
        self.items_change(list.into(), Change::Changed(index))
    }

    /// The item at `from` is now at `to` (a drag to reorder): its measured size goes with it, the
    /// rows between shift, the cells keep their items and nothing is measured again. The scroll
    /// keeps its offset (as upstream and DrawnUi.React): a move inside the viewport binds nothing,
    /// a move across its edge binds the one row that enters.
    pub fn items_moved(&mut self, list: impl Into<ControlId>, from: usize, to: usize) {
        self.items_change(list.into(), Change::Moved(from, to))
    }

    /// The same items in another order (a reverse, a sort): `order[i]` is the old index of the item
    /// now at `i`. Sizes and cells go with their items, as with `items_moved`. Anything that is not
    /// a permutation of every item is a reset.
    pub fn items_reordered(&mut self, list: impl Into<ControlId>, order: Vec<usize>) {
        self.items_change(list.into(), Change::Reordered(order))
    }
}
