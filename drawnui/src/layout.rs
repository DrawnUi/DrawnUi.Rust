//! Measure and arrange, the same contract as DrawnUI SkiaControl: `measure` applies margin,
//! size requests and LockRatio around the control's own content measure; `arrange` places the
//! measured size inside a destination by margin and layout options.

use std::any::Any;

use skia_safe::{Rect, Size};

use crate::control::LayoutCx;
use crate::fonts::Fonts;
use crate::tree::{Base, ControlId, ControlProps, Cx, Tree};
use crate::types::{CacheType, Dirty, LayoutOptions, Thickness};

/// The control takes the width its parent gives it (C# NeedFillX).
pub(crate) fn fills_width(p: &ControlProps) -> bool {
    p.horizontal_options == LayoutOptions::Fill && p.width_request < 0.0
}

/// The control takes the height its parent gives it (C# NeedFillY).
pub(crate) fn fills_height(p: &ControlProps) -> bool {
    p.vertical_options == LayoutOptions::Fill && p.height_request < 0.0
}

/// Larger of two sizes; an infinite one loses.
fn smart_max(a: f32, b: f32) -> f32 {
    if !a.is_finite() || (b.is_finite() && b > a) { b } else { a }
}

/// Smaller of two sizes; an infinite one loses.
fn smart_min(a: f32, b: f32) -> f32 {
    if !a.is_finite() || (b.is_finite() && b < a) { b } else { a }
}

/// C# `RoundCenterAlignment`, on as upstream: a centered control grows by one pixel when the free
/// space around it is odd, so that both gaps are equal. Off: the control keeps its size and the
/// odd pixel goes to the leading gap.
pub(crate) const ROUND_CENTER_ALIGNMENT: bool = true;

/// A whole pixel, as C# `Math.Round` gives it: a half goes to the even side.
pub(crate) fn snap(pixels: f32) -> f32 {
    pixels.round_ties_even()
}

/// The margins in pixels, each side rounded on its own (C# GetMarginsInPixels).
pub(crate) fn margins(p: &ControlProps, scale: f32) -> Thickness {
    let m = p.margin;
    Thickness::new(snap(m.left * scale), snap(m.top * scale), snap(m.right * scale), snap(m.bottom * scale))
}

/// For the content measure of a control with padding, given the constraints its `measure` got:
/// the box its children are measured in, and what the padding adds to a side that takes the size
/// of its content. Upstream reserves the raw insets in the first (GetMeasuringRectForChildren) and
/// insets rounded per side in the second (GetAllMarginsInPixels).
pub(crate) fn content_box(p: &ControlProps, width: f32, height: f32, scale: f32) -> (Size, Size) {
    let (m, pad, scale64) = (p.margin, p.padding, scale as f64);
    let rounded = margins(p, scale);
    let raw = |lead: f32, trail: f32| ((lead as f64 * scale64) + (trail as f64 * scale64)) as f32;
    let inner = Size::new(
        snap(width + rounded.horizontal() - raw(pad.left + m.left, pad.right + m.right)),
        snap(height + rounded.vertical() - raw(pad.top + m.top, pad.bottom + m.bottom)),
    );
    let both = |lead: f32, trail: f32| snap(lead * scale) + snap(trail * scale);
    let added = Size::new(
        both(m.left + pad.left, m.right + pad.right) - rounded.horizontal(),
        both(m.top + pad.top, m.bottom + pad.bottom) - rounded.vertical(),
    );
    (inner, added)
}

/// The rect the children of a control are arranged in: its own rect without the padding, where
/// each side takes the smaller of the raw and the rounded inset, so the content never gets less
/// room than it was measured for (C# ContractPixelsRectForContent).
pub(crate) fn content_rect(base: &Base, scale: f32) -> Rect {
    let (r, pad) = (base.rect, base.p.padding);
    let inset = |points: f32| (points * scale).min(snap(points * scale));
    let (left, top, right, bottom) = (inset(pad.left), inset(pad.top), inset(pad.right), inset(pad.bottom));
    Rect::new(r.left + left, r.top + top, r.right - right, r.bottom - bottom)
}

/// The size the last measure came to before it was rounded to pixels, margins included. Arrange
/// takes its ceiling, as upstream does with the measured size in points. A fraction can only come
/// from a Minimum or a Maximum request, so it is read back from those.
// ponytail: content that happens to be exactly round(Maximum) pixels large is taken for capped,
// which can make its box 1 px larger than upstream. Keep the raw size in `Base` if that shows.
pub(crate) fn desired(base: &Base) -> Size {
    let (p, scale) = (&base.p, base.scale);
    let m = margins(p, scale);
    let side = |measured: f32, margins: f32, minimum: f32, maximum: f32, request: f32| {
        if minimum >= 0.0 && measured == snap(minimum * scale + margins) {
            minimum * scale + margins
        } else if maximum >= 0.0 && request < 0.0 && measured == snap(maximum * scale + margins) {
            maximum * scale + margins
        } else {
            measured
        }
    };
    Size::new(
        side(base.measured.width, m.horizontal(), p.minimum_width_request, p.maximum_width_request, p.width_request),
        side(base.measured.height, m.vertical(), p.minimum_height_request, p.maximum_height_request, p.height_request),
    )
}

/// Measures a control inside constraints in pixels. Returns its size in whole pixels, margin
/// included. A control that was not invalidated and gets the same constraints and scale returns
/// its previous size without measuring anything. `state` is the app state.
pub(crate) fn measure(
    tree: &mut Tree,
    fonts: &Fonts,
    state: &dyn Any,
    id: ControlId,
    width: f32,
    height: f32,
    scale: f32,
) -> Size {
    let Some(node) = tree.node_mut(id) else { return Size::default() };
    let b = &mut node.base;
    if !b.need_measure && b.scale == scale && b.last_constraints.0.to_bits() == width.to_bits() && b.last_constraints.1.to_bits() == height.to_bits() {
        return b.measured;
    }
    b.last_constraints = (width, height);
    b.scale = scale;
    // No room on a side: the control is empty (C# MeasureInternal).
    if width == 0.0 || height == 0.0 {
        b.measured = Size::default();
        b.need_measure = false;
        b.need_arrange = true;
        return b.measured;
    }
    let p = &b.p;
    let m = margins(p, scale);
    let (mx, my) = (m.horizontal(), m.vertical());

    // With LockRatio one request drives both sides; requests win over locked constraints.
    let (mut req_w, mut req_h) = (p.width_request, p.height_request);
    if p.lock_ratio != 0.0 && (req_w >= 0.0 || req_h >= 0.0) {
        let both = req_w >= 0.0 && req_h >= 0.0;
        let side = if both && p.lock_ratio < 0.0 { req_w.min(req_h) } else { req_w.max(req_h) };
        (req_w, req_h) = (side, side);
    }

    // A request and its margins make whole pixels together, and no more than the constraint
    // (C# AdaptWidthConstraintToRequest).
    let mut w = width - mx;
    let mut h = height - my;
    if req_w >= 0.0 {
        w = snap(req_w * scale + mx).min(width) - mx;
    } else if p.maximum_width_request >= 0.0 {
        w = w.min(p.maximum_width_request * scale);
    }
    if req_h >= 0.0 {
        h = snap(req_h * scale + my).min(height) - my;
    } else if p.maximum_height_request >= 0.0 {
        h = h.min(p.maximum_height_request * scale);
    }

    // No request: the constraints themselves are locked.
    let mut locked = false;
    if p.lock_ratio != 0.0 && req_w < 0.0 && req_h < 0.0 {
        let side = if p.lock_ratio > 0.0 { smart_max(w, h) } else { smart_min(w, h) } * p.lock_ratio.abs();
        if side > 0.0 && side.is_finite() {
            (w, h) = (side, side);
            locked = true;
        }
    }

    let content = match node.kind.take() {
        Some(mut kind) => {
            let size = kind.measure(&mut LayoutCx { tree, fonts, state, id, scale }, w, h);
            if let Some(node) = tree.node_mut(id) {
                node.kind = Some(kind);
            }
            size
        }
        None => Size::default(),
    };

    // Read after the content measure: a control may set style defaults (minimum size) in it.
    let Some(node) = tree.node_mut(id) else { return Size::default() };
    let p = &node.base.p;
    let (h_fill, v_fill) = (p.horizontal_options == LayoutOptions::Fill, p.vertical_options == LayoutOptions::Fill);
    let (min_w, min_h, max_w, max_h) =
        (p.minimum_width_request, p.minimum_height_request, p.maximum_width_request, p.maximum_height_request);

    let mut rw = if locked || req_w >= 0.0 || (h_fill && w.is_finite()) { w } else { content.width };
    let mut rh = if locked || req_h >= 0.0 || (v_fill && h.is_finite()) { h } else { content.height };
    // Never larger than the constraint: what does not fit is cut there. A minimum still wins.
    if !locked {
        rw = rw.min((width - mx).max(0.0));
        rh = rh.min((height - my).max(0.0));
    }
    if min_w >= 0.0 {
        rw = rw.max(min_w * scale);
    }
    if min_h >= 0.0 {
        rh = rh.max(min_h * scale);
    }
    if max_w >= 0.0 && req_w < 0.0 {
        rw = rw.min(max_w * scale);
    }
    if max_h >= 0.0 && req_h < 0.0 {
        rh = rh.min(max_h * scale);
    }

    // Whole pixels. What a Minimum or Maximum leaves as a fraction comes back in `desired`.
    let measured = Size::new(snap(rw + mx), snap(rh + my));
    node.base.measured = measured;
    node.base.need_measure = false;
    node.base.need_arrange = true;
    measured
}

/// One axis of C# `CalculateLayout`: where the margin box starts and ends inside an area
/// `available` long, from 0. `wanted` is the measured size before rounding, `shift` the leading
/// margin minus the trailing one, `cap` the pixels a Maximum request leaves a Fill control
/// (negative: none).
fn place_axis(
    options: LayoutOptions,
    fill: bool,
    available: f32,
    wanted: f32,
    shift: f32,
    cap: f32,
    ratio: f32,
) -> (f32, f32) {
    // C# DefineAvailableSize: the box is a whole number of pixels, rounded up.
    let mut size = match fill {
        true if cap >= 0.0 => available.min(cap),
        true => available,
        false => wanted.min(available),
    };
    if ratio != 1.0 {
        size *= ratio;
    }
    let mut size = if size.is_finite() { size.ceil() } else { f32::MAX };
    // Center and End need room to move; without it the control starts at the edge and is cut.
    if !available.is_finite() || available <= size {
        return (0.0, size.min(available));
    }
    match options {
        // The content is centered in the whole area (rounded up), its margin box is moved back
        // inside when it sticks out, then both move by the difference of the two margins.
        LayoutOptions::Center => {
            let shift = if wanted > 0.0 { shift } else { 0.0 };
            let mut real = size + shift;
            if ROUND_CENTER_ALIGNMENT && (snap(available) - snap(real)) % 2.0 != 0.0 {
                size += 1.0;
                real += 1.0;
            }
            let start = (available / 2.0 - real / 2.0).ceil();
            let (start, end) = if start < 0.0 {
                (0.0, size)
            } else if start + size > available {
                (available - size, available)
            } else {
                (start, start + size)
            };
            (start + shift, end + shift)
        }
        LayoutOptions::End => ((available - size).max(0.0), available),
        _ => (0.0, size),
    }
}

/// The margin box of a control inside an area of that size, in whole pixels from the corner of
/// the area (C# CalculateLayout). Stacks place a child with it before they know where they are.
pub(crate) fn place(base: &Base, area: Size, scale: f32) -> Rect {
    let p = &base.p;
    // Upstream keeps the measured size in points and comes back to pixels here: at a scale that
    // is not a power of two the way there and back can end just above a whole pixel, and the box,
    // rounded up, is then 1 px larger than measured. Same arithmetic, same pixel.
    let (m, wanted) = (margins(p, scale), desired(base));
    let wanted = Size::new(wanted.width / scale * scale, wanted.height / scale * scale);
    let cap = |maximum: f32, margins: f32| if maximum >= 0.0 { maximum * scale + margins } else { -1.0 };
    let (left, right) = place_axis(
        p.horizontal_options,
        fills_width(p),
        area.width,
        wanted.width,
        (p.margin.left - p.margin.right) * scale,
        cap(p.maximum_width_request, m.horizontal()),
        p.horizontal_fill_ratio,
    );
    let (top, bottom) = place_axis(
        p.vertical_options,
        fills_height(p),
        area.height,
        wanted.height,
        (p.margin.top - p.margin.bottom) * scale,
        cap(p.maximum_height_request, m.vertical()),
        p.vertical_fill_ratio,
    );
    Rect::new(snap(left), snap(top), snap(right), snap(bottom))
}

/// Places the measured control inside `destination` (pixels) and lets it arrange its children.
/// The rect is whole pixels. Nothing happens when neither the destination nor the measure changed
/// since the last arrange.
pub(crate) fn arrange(
    tree: &mut Tree,
    fonts: &Fonts,
    state: &dyn Any,
    id: ControlId,
    destination: Rect,
    scale: f32,
) {
    let Some(node) = tree.node_mut(id) else { return };
    let b = &node.base;
    if !b.need_arrange && b.last_destination == destination {
        return;
    }
    let (fill_w, fill_h) = (fills_width(&b.p), fills_height(&b.p));
    // Arranged into another box than it was measured for on a Fill axis: measure again with the
    // final box, so the inner layout matches the size the control is drawn at (C# Arrange).
    // A size measured to content on an unbounded axis is compared itself.
    if !b.need_measure && b.scale == scale {
        let (for_w, for_h) = b.last_constraints;
        let differs = |side: f32, measured_for: f32, measured: f32| {
            side.is_finite() && (side - if measured_for.is_finite() { measured_for } else { measured }).abs() > 1.0
        };
        let again_w = fill_w && differs(destination.width(), for_w, b.measured.width);
        let again_h = fill_h && differs(destination.height(), for_h, b.measured.height);
        if again_w || again_h {
            let width = if again_w { destination.width() } else { for_w };
            let height = if again_h { destination.height() } else { for_h };
            measure(tree, fonts, state, id, width, height, scale);
        }
    }
    let Some(node) = tree.node_mut(id) else { return };
    let b = &mut node.base;
    b.last_destination = destination;
    b.need_arrange = false;
    // The margin box in whole pixels from the corner of the destination, then moved there and
    // taken in by the margins, and whole pixels again (C# GetDrawingRectWithMargins).
    let (placed, m) = (place(b, destination.size(), scale), margins(&b.p, scale));
    let rect = Rect::new(
        snap(placed.left + destination.left + m.left),
        snap(placed.top + destination.top + m.top),
        snap(placed.right + destination.left - m.right),
        snap(placed.bottom + destination.top - m.bottom),
    );
    let moved = b.rect != rect;
    b.rect = rect;
    // Another box: a list below sees another part of itself through it (a resized scroll).
    if moved {
        tree.rearrange_trackers(id, Some(id));
    }
    let Some(node) = tree.node_mut(id) else { return };

    if let Some(mut kind) = node.kind.take() {
        kind.arrange(&mut LayoutCx { tree, fonts, state, id, scale });
        if let Some(node) = tree.node_mut(id) {
            node.kind = Some(kind);
        }
    }
}

/// The commit step of a frame: applies pending removals and turns the dirty flags set since the
/// last frame into invalidation. Measure invalidation goes up to the root (every ancestor
/// measures again and re-records); draw and repaint only stale the caches on the way up. The
/// frame that commits lays out and paints what it flushed, so it asks for no other frame: what is
/// invalidated later in it (during layout) stays queued, and that asks for the next one.
pub(crate) fn commit(tree: &mut Tree) {
    flush(tree, None);
}

/// Removes the controls whose removal was asked for; their parents measure again.
fn remove_pending(tree: &mut Tree) {
    for id in std::mem::take(&mut tree.removals) {
        let parent = tree.parent(id);
        tree.remove_now(id);
        if let Some(parent) = parent {
            tree.invalidate(parent, Dirty::MEASURE);
        }
    }
}

/// Applies the removals and the dirty flags queued so far. `within` is a control that is laying
/// out its own subtree right now (a list that just bound a cell): it measures the dirty controls
/// below it in this same pass, so measure invalidation stops there; the caches above still go stale.
pub(crate) fn flush(tree: &mut Tree, within: Option<ControlId>) {
    remove_pending(tree);
    while !tree.queue.is_empty() {
        // The two lists trade places, so a frame that marks controls dirty allocates nothing.
        let mut batch = std::mem::take(&mut tree.queue_spare);
        std::mem::swap(&mut batch, &mut tree.queue);
        for id in batch.drain(..) {
            let Some(node) = tree.node_mut(id) else { continue };
            let dirty = std::mem::take(&mut node.base.dirty);
            if dirty.contains(Dirty::APPLY)
                && let Some(mut kind) = node.kind.take()
            {
                kind.on_props_changed(&mut Cx { tree });
                if let Some(node) = tree.node_mut(id) {
                    node.kind = Some(kind);
                }
            }
            let mut measure = dirty.contains(Dirty::MEASURE);
            let own = measure || dirty.contains(Dirty::DRAW);
            if !own && !dirty.contains(Dirty::REPAINT) {
                continue;
            }
            let mut current = Some(id);
            let mut is_self = true;
            // The child the change came through, for an ImageComposite ancestor.
            let mut through = None;
            while let Some(node) = current.and_then(|c| tree.node_mut(c)) {
                if !is_self && Some(node.id) == within {
                    measure = false;
                }
                if measure {
                    node.base.need_measure = true;
                }
                if own || !is_self {
                    node.base.content_epoch = node.base.content_epoch.wrapping_add(1);
                }
                // The control itself changed, not only what is below it.
                if own && is_self {
                    node.base.own_epoch = node.base.own_epoch.wrapping_add(1);
                }
                let (at, composite) = (node.id, node.base.p.use_cache.resolved() == CacheType::ImageComposite);
                current = node.parent;
                // An ImageComposite control draws again only the children a change came through;
                // its own change or a new layout draws it whole.
                if composite && (own || !is_self) {
                    crate::paint::composite_changed(&mut tree.render, at, if is_self || measure { None } else { through });
                }
                through = Some(at);
                is_self = false;
            }
            // Dirty outside of what `within` lays out: this frame's layout is over for it.
            if measure && within.is_some() {
                tree.needs_frame = true;
            }
        }
        tree.queue_spare = batch;
    }
}
