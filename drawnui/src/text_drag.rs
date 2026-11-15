//! Desktop text selection as in browsers (drawnui-cross 6cc): a main-button mouse drag selects the
//! text of the selectable labels (`accessibility_text_selectable`) it goes over, in page order,
//! also when it starts outside any text; inside a scroll that holds selectable text the drag
//! selects instead of scrolling and scrolls on while held past the scroll's edge. Touch keeps its
//! own selection (a long press). Nothing runs between presses.

use skia_safe::Point;

use super::{Ui, drawn_rect, to_local};
use crate::controls::label::SkiaLabel;
use crate::controls::scroll::SkiaScroll;
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{ControlId, Cx, Tree};
use crate::types::Dirty;

/// A mouse press that may select text.
pub(super) struct TextDrag {
    /// The selectable labels at the press, in page order.
    labels: Vec<ControlId>,
    /// Where the selection starts: label, and the range the press took there (one boundary, or
    /// the double-clicked word).
    anchor: (usize, usize, usize),
    /// The pointer, canvas pixels.
    point: Point,
    /// The drag went on past the press: the selection follows the pointer.
    moved: bool,
    /// The innermost scroll around the anchor that holds selectable text: it scrolls on while the
    /// pointer is held past its edge.
    scroll: Option<ControlId>,
}

/// A text position over the labels in page order: label, character boundary.
type Position = (usize, usize);

/// The selectable labels of the tree in page order (children order), visible ones only.
fn selectable_labels(tree: &Tree) -> Vec<ControlId> {
    let mut out = Vec::new();
    let mut stack: Vec<ControlId> = tree.root().into_iter().collect();
    while let Some(id) = stack.pop() {
        let Some(base) = tree.base(id) else { continue };
        if !base.p.is_visible {
            continue;
        }
        if base.p.accessibility_text_selectable && tree.find::<SkiaLabel>(id).is_some() {
            out.push(id);
        }
        stack.extend(tree.children(id).iter().rev());
    }
    out
}

fn scrolls_above(tree: &Tree, id: ControlId) -> impl Iterator<Item = ControlId> + '_ {
    std::iter::successors(tree.parent(id), |c| tree.parent(*c)).filter(|c| tree.find::<SkiaScroll>(*c).is_some())
}

/// Sets a label's selection; true when it changed.
fn set(tree: &mut Tree, id: ControlId, from: usize, to: usize) -> bool {
    let label = tree.node_mut(id).and_then(|n| crate::control::part_mut::<SkiaLabel>(n.kind.as_deref_mut()?));
    let changed = label.is_some_and(|l| l.set_drag_selection(from, to));
    if changed {
        tree.invalidate(id, Dirty::DRAW);
    }
    changed
}

fn len(tree: &Tree, id: ControlId) -> usize {
    tree.find::<SkiaLabel>(id).map_or(0, |l| l.selection_len())
}

/// The text position of a canvas point: the character boundary under it in a label, else the
/// start of the first label after it in reading order (above it, or on its line further right),
/// else the end of the last label.
fn position(tree: &Tree, labels: &[ControlId], point: Point) -> Position {
    for (k, &id) in labels.iter().enumerate() {
        let Some(rect) = drawn_rect(tree, id) else { continue };
        let on_line = point.y >= rect.top && point.y < rect.bottom;
        let before = rect.bottom <= point.y || (on_line && point.x >= rect.right);
        if before {
            continue;
        }
        if on_line && point.x >= rect.left {
            let index = tree.find::<SkiaLabel>(id).map_or(0, |l| l.selection_index_at(to_local(tree, id, point)));
            return (k, index);
        }
        return (k, 0);
    }
    labels.len().checked_sub(1).map_or((0, 0), |last| (last, len(tree, labels[last])))
}

impl<S: 'static> Ui<S> {
    /// Before a pointer gesture is routed: a main-button mouse press over a canvas with
    /// selectable text starts a possible text drag; the scrolls holding that text leave mouse
    /// drags to it (`Tree::text_scrolls`).
    pub(super) fn text_drag_before(&mut self, gesture: &Gesture) {
        if gesture.kind != GestureKind::Down {
            return;
        }
        self.text_drag = None;
        self.tree.text_scrolls.clear();
        if gesture.touch || !gesture.is_primary() {
            return;
        }
        let labels = selectable_labels(&self.tree);
        if labels.is_empty() {
            self.clear_text_selection(None);
            return;
        }
        let mut scrolls: Vec<ControlId> = labels.iter().flat_map(|id| scrolls_above(&self.tree, *id)).collect();
        scrolls.sort_unstable_by_key(|s| s.index);
        scrolls.dedup();
        self.tree.text_scrolls = scrolls;
        self.text_drag = Some(TextDrag { labels, anchor: (0, 0, 0), point: gesture.location, moved: false, scroll: None });
    }

    /// After a pointer gesture was routed (`consumed` took it).
    pub(super) fn text_drag_after(&mut self, gesture: &Gesture, consumed: Option<ControlId>) {
        let Some(drag) = self.text_drag.as_mut() else { return };
        match gesture.kind {
            GestureKind::Down => {
                // On a selectable label: its press decides the start (a word after a double click).
                let pressed = consumed.and_then(|c| drag.labels.iter().position(|l| *l == c));
                let anchor = pressed.and_then(|k| self.tree.find::<SkiaLabel>(drag.labels[k])?.mouse_press_anchor().map(|(a, b)| (k, a, b)));
                drag.anchor = anchor.unwrap_or_else(|| {
                    let (k, at) = position(&self.tree, &drag.labels, gesture.location);
                    (k, at, at)
                });
                let label = drag.labels[drag.anchor.0];
                drag.scroll = scrolls_above(&self.tree, label).next();
                // A click elsewhere drops the selection; the pressed label keeps what its press took.
                self.clear_text_selection(anchor.map(|_| label));
            }
            GestureKind::Panning => {
                if gesture.touch || !gesture.is_primary() || consumed.is_some_and(|c| !drag.labels.contains(&c)) {
                    // Another control took the drag (a slider, a carousel): no text selection.
                    self.text_drag = None;
                    self.tree.text_scrolls.clear();
                    return;
                }
                (drag.point, drag.moved) = (gesture.location, true);
                self.select_text_to(gesture.location);
            }
            GestureKind::Up => {
                self.text_drag = None;
                self.tree.text_scrolls.clear();
            }
            _ => {}
        }
    }

    /// The labels from the drag's anchor to the text position at `point` take their parts.
    fn select_text_to(&mut self, point: Point) {
        let Some(drag) = self.text_drag.as_ref() else { return };
        let focus = position(&self.tree, &drag.labels, point);
        let (k, a, b) = drag.anchor;
        let (start, end) = if focus >= (k, a) { ((k, a), focus.max((k, b))) } else { (focus, (k, b)) };
        let labels = drag.labels.clone();
        self.tree.text_selection.clear();
        for (i, id) in labels.into_iter().enumerate() {
            let n = len(&self.tree, id);
            let from = if i == start.0 { start.1 } else if i > start.0 { 0 } else { n };
            let to = if i == end.0 { end.1 } else if i < end.0 { n } else { 0 };
            let (from, to) = if to > from { (from, to) } else { (0, 0) };
            set(&mut self.tree, id, from, to);
            if to > from {
                self.tree.text_selection.push(id);
            }
        }
    }

    /// Drops the selection of every label on the page, but `keep`.
    fn clear_text_selection(&mut self, keep: Option<ControlId>) {
        clear_selection(&mut self.tree, keep);
    }

    /// Each frame while a drag is held past the edge of its scroll: the scroll moves on, faster
    /// the farther the pointer is, and the selection follows.
    pub(super) fn text_drag_frame(&mut self) {
        let Some(drag) = self.text_drag.as_ref().filter(|d| d.moved) else { return };
        let (Some(scroll), point) = (drag.scroll, drag.point) else { return };
        let Some(rect) = drawn_rect(&self.tree, scroll) else { return };
        let past = if point.y > rect.bottom { point.y - rect.bottom } else if point.y < rect.top { point.y - rect.top } else { return };
        let scale = self.scale.max(0.1);
        // Points per frame: a tenth of the distance past the edge, at least 2.
        let step = (past / scale * 0.1).abs().clamp(2.0, 40.0).copysign(past);
        let Some(offset) = self.tree.find::<SkiaScroll>(scroll).map(|s| s.viewport_offset_y()) else { return };
        Cx { tree: &mut self.tree }.set_viewport_offset_y(scroll, offset - step);
        self.select_text_to(point);
        self.tree.needs_frame = true;
    }

    /// Ctrl+C (Cmd+C) with text selected on the page: the selected text of every label, joined by
    /// new lines. False when nothing is selected.
    pub(super) fn copy_text_selection(&mut self) -> bool {
        let text = selected_text(&self.tree);
        if text.is_empty() {
            return false;
        }
        Cx { tree: &mut self.tree }.set_clipboard(text);
        true
    }
}

/// The text selected on the page: every label's part in page order, joined by new lines.
pub(crate) fn selected_text(tree: &Tree) -> String {
    let parts: Vec<String> = tree.text_selection.iter().filter_map(|id| tree.find::<SkiaLabel>(*id).map(|l| l.selected_text())).filter(|t| !t.is_empty()).collect();
    parts.join("\n")
}

/// Drops the selection of every label on the page, but `keep`.
pub(crate) fn clear_selection(tree: &mut Tree, keep: Option<ControlId>) {
    for id in std::mem::take(&mut tree.text_selection) {
        if Some(id) == keep {
            tree.text_selection.push(id);
            continue;
        }
        set(tree, id, 0, 0);
    }
}
