//! Rendering a control: opacity layer, transform, clip, then its cache or its content.
//! Same order as DrawnUI SkiaControl.Render.

use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use skia_safe::{
    BlendMode, ClipOp, Color, Color4f, FilterMode, Image, ImageFilter, Matrix, MipmapMode, Paint, PathBuilder, Picture,
    PictureRecorder, Point, Rect, SamplingOptions, Shader, Surface,
    canvas::SaveLayerRec,
    gradient::{self, Gradient, Interpolation},
    image_filters,
};

use skia_safe::QuickReject as _;

use crate::control::{PaintCx, Target};
use crate::controls::backdrop::BackdropPaint;
use crate::controls::layout::SkiaLayout;
use crate::controls::shape::ShapeCache;
use crate::effects::{self, CachedTexture};
use crate::tree::{Base, ControlId, ControlProps, Cx, RenderSlot, Tree};
use crate::types::{CacheType, GradientType, SkiaGradient, SkiaShadow, Thickness};

/// What a cached control replays instead of painting again.
pub(crate) struct CachedObject {
    content: CachedContent,
    /// What was recorded, canvas pixels at recording time: the control's rect grown by `margin`
    /// (DrawnUI keeps the two apart as RecordingArea and Bounds). Only the blit uses it; layout
    /// and hit testing know the control's own rect alone.
    bounds: Rect,
    /// The effects margin of the control and its descendants the cache was recorded with.
    margin: Thickness,
    scale: f32,
    /// The `use_cache` it was recorded for.
    kind: CacheType,
}

enum CachedContent {
    Picture(Picture),
    Image(Image),
}

impl CachedObject {
    fn cache_type(&self) -> CacheType {
        self.kind
    }

    /// Lives on the GPU context: a texture, or a picture that may draw one.
    fn on_gpu(&self) -> bool {
        match &self.content {
            CachedContent::Image(image) => image.is_texture_backed(),
            CachedContent::Picture(_) => true,
        }
    }

    /// The CPU bitmap of an ImageDoubleBuffered cache, made for `bounds`.
    fn double_buffered(image: Image, bounds: Rect, margin: Thickness, scale: f32) -> Self {
        Self { content: CachedContent::Image(image), bounds, margin, scale, kind: CacheType::ImageDoubleBuffered }
    }

    /// The bitmap of an Image cache and the canvas rect it was recorded for.
    pub(crate) fn image(&self) -> Option<(&Image, Rect)> {
        match &self.content {
            CachedContent::Image(image) => Some((image, self.bounds)),
            CachedContent::Picture(_) => None,
        }
    }

    /// The effects margin it was recorded with: its bounds are the control's rect grown by it.
    pub(crate) fn margin(&self) -> Thickness {
        self.margin
    }

    /// Replays the cache with its top-left moved to (left, top).
    fn draw(&self, canvas: &skia_safe::Canvas, left: f32, top: f32) {
        match &self.content {
            // Nearest sampling: a cached bitmap is blitted 1:1, never resampled.
            CachedContent::Image(image) => {
                let sampling = SamplingOptions::new(FilterMode::Nearest, MipmapMode::None);
                canvas.draw_image_with_sampling_options(image, (left, top), sampling, None);
            }
            CachedContent::Picture(picture) => {
                canvas.save();
                canvas.translate((left - self.bounds.left, top - self.bounds.top));
                canvas.draw_picture(picture, None, None);
                canvas.restore();
            }
        }
    }
}

/// `rect` with a margin in pixels added on every side.
fn grow(rect: Rect, margin: &Thickness) -> Rect {
    Rect::new(rect.left - margin.left, rect.top - margin.top, rect.right + margin.right, rect.bottom + margin.bottom)
}

/// Pixels a control and its descendants paint outside the control's rect, per side, on whole
/// pixels (DrawnUI AggregatedEffectsMarginPixels). Position-agnostic as upstream: the overflow of
/// a child counts on the same side of every ancestor, wherever the child sits. Kept per control
/// until something below it changes, so an unchanged subtree is never walked again.
fn effects_margin(cx: &mut PaintCx, id: ControlId) -> Thickness {
    let Some(node) = cx.node(id) else { return Thickness::ZERO };
    let (slot, epoch) = (id.index as usize, node.base.content_epoch);
    if let Some((for_epoch, for_scale, margin)) = cx.render[slot].effects
        && for_epoch == epoch
        && for_scale == cx.scale
    {
        return margin;
    }
    let mut own = node.kind.as_deref().map_or(Thickness::ZERO, |kind| kind.effects_margin(cx.scale));
    if !node.base.visual_effects.is_empty() {
        own = own.max(effects::margin(&node.base, cx.scale));
    }
    let round = f32::round_ties_even;
    let mut margin = Thickness::new(round(own.left), round(own.top), round(own.right), round(own.bottom));
    for &child in &node.children {
        margin = margin.max(effects_margin(cx, child));
    }
    cx.render[slot].effects = Some((epoch, cx.scale, margin));
    margin
}

fn has_matrix_transform(p: &ControlProps) -> bool {
    p.rotation != 0.0 || p.scale_x != 1.0 || p.scale_y != 1.0 || p.skew_x != 0.0 || p.skew_y != 0.0
}

/// Left/Top on a cached control with no other transform: the cache is blitted at an offset, no
/// matrix on the canvas.
fn offset_only(p: &ControlProps) -> bool {
    (p.left != 0.0 || p.top != 0.0)
        && p.use_cache != CacheType::None
        && p.translation_x == 0.0
        && p.translation_y == 0.0
        && !has_matrix_transform(p)
}

/// The matrix a control is drawn with, when it has one (see `render`).
fn control_matrix(base: &Base, scale: f32) -> Option<Matrix> {
    let p = &base.p;
    if offset_only(p) {
        return Some(Matrix::translate((p.left * scale, p.top * scale)));
    }
    let moved = p.translation_x != 0.0 || p.translation_y != 0.0 || p.left != 0.0 || p.top != 0.0;
    (moved || has_matrix_transform(p)).then(|| render_transform(base, scale))
}

/// T(pivot + move) · scale/skew · rotation · T(-pivot), in canvas pixels.
fn render_transform(base: &Base, scale: f32) -> Matrix {
    let p = &base.p;
    let shift = Point::new((p.translation_x + p.left) * scale, (p.translation_y + p.top) * scale);
    if !has_matrix_transform(p) {
        return Matrix::translate(shift);
    }
    let pivot = Point::new(base.rect.left + base.rect.width() * p.anchor_x, base.rect.top + base.rect.height() * p.anchor_y);
    let kx = if p.skew_x != 0.0 { p.skew_x.to_radians().tan() } else { 0.0 };
    let ky = if p.skew_y != 0.0 { p.skew_y.to_radians().tan() } else { 0.0 };
    let mut m = Matrix::translate(pivot + shift);
    m.pre_concat(&Matrix::new_all(p.scale_x, kx, 0.0, ky, p.scale_y, 0.0, 0.0, 0.0, 1.0));
    if p.rotation != 0.0 {
        m.pre_concat(&Matrix::rotate_deg(p.rotation));
    }
    m.pre_concat(&Matrix::translate(-pivot));
    m
}

/// Draws one control with the canvas of `cx`. `cx` describes the parent; it is restored on return.
pub(crate) fn render(cx: &mut PaintCx, id: ControlId) {
    let Some(node) = cx.node(id) else { return };
    let base = &node.base;
    let p = &base.p;
    if !p.is_visible || p.opacity <= 0.0 {
        return;
    }
    let canvas = cx.canvas;
    let scale = cx.scale;
    let moved = p.translation_x != 0.0 || p.translation_y != 0.0 || p.left != 0.0 || p.top != 0.0;
    let offset_only = offset_only(p);
    let need_transform = (moved || has_matrix_transform(p)) && !offset_only;

    let mut offset = Point::default();
    let slot = id.index as usize;
    let matrix = need_transform.then(|| render_transform(base, scale));
    if offset_only {
        offset = Point::new(p.left * scale, p.top * scale);
        // Gestures still map through it.
        cx.render[slot].matrix = Some(Matrix::translate(offset));
    } else {
        cx.render[slot].matrix = matrix;
    }

    // The control lies outside the clip (content scrolled out of its scroll, a cache being
    // recorded for another area): its subtree is not painted and its caches are not recorded
    // (DrawnUI Virtualisation: by the control's own rect, whatever its children overflow by).
    let painted = grow(base.rect, &effects_margin(cx, id)).with_offset(offset);
    if canvas.quick_reject(&matrix.map_or(painted, |m| m.map_rect(painted).0)) {
        return;
    }

    let mut saved = false;
    if p.opacity < 1.0 {
        let mut layer = Paint::default();
        layer.set_alpha_f(p.opacity.clamp(0.0, 1.0));
        canvas.save_layer(&SaveLayerRec::default().paint(&layer));
        saved = true;
    } else if need_transform {
        canvas.save();
        saved = true;
    }
    if let Some(matrix) = &matrix {
        canvas.concat(matrix);
    }

    if p.is_clipped_to_bounds {
        if !saved {
            canvas.save();
            saved = true;
        }
        // ClipEffects, as DrawnUi.React: true cuts at the rect; false lets through what the
        // subtree paints outside on purpose (its shadows), grown by the effects margin (C# always
        // grows it).
        let clip = match p.clip_effects {
            true => base.rect,
            false => grow(base.rect, &effects_margin(cx, id)),
        };
        canvas.clip_rect(clip.with_offset(offset), ClipOp::Intersect, true);
    }

    render_content(cx, id, base, offset);
    paint_overlays(cx, id, base.rect.with_offset(offset));

    if saved {
        canvas.restore();
    }
}

/// Cache blit or live paint, then the post renderer effects: the part a transform or an opacity
/// layer wraps.
fn render_content(cx: &mut PaintCx, id: ControlId, base: &Base, offset: Point) {
    let cache_type = base.p.use_cache.resolved();
    let slot = id.index as usize;
    let rect = base.rect.with_offset(offset);
    let post = effects::has_post_renderer(base);
    let baked = cache_type == CacheType::ImageDoubleBuffered && cx.bakes.enabled;
    // A worker's picture takes no GPU texture: a cache on the GPU is painted live into it and left
    // as it is. An ImageDoubleBuffered bitmap is in CPU memory: it is drawn.
    let live = cx.offthread && !baked;
    if cache_type == CacheType::None || live {
        if !live && let Some(old) = cx.render[slot].cache.take() {
            cx.drops.push(old);
        }
        paint_content(cx, id, cx.canvas, rect);
        // DrawnUI DrawDirectInternal: the effects take what was painted.
        if post {
            effects::post_render(cx, id, base, rect, None);
        }
        return;
    }

    // The cache holds what the subtree paints outside the rect too. A cache that is still valid
    // was recorded with the margin the subtree has now.
    let margin = match &cx.render[slot].cache {
        Some(c) if cx.render[slot].cache_epoch == base.content_epoch && c.scale == cx.scale => c.margin,
        _ => effects_margin(cx, id),
    };
    let r = match cache_type {
        // The area the canvas shows, in the control's own coordinates (before its offset).
        CacheType::OperationsFull => cx.canvas.local_clip_bounds().unwrap_or(base.rect).with_offset((-offset.x, -offset.y)),
        _ => cache_rect(base.rect, &margin, cache_type),
    };
    let mut stale = cx.render[slot].cache.as_ref().is_none_or(|c| {
        cx.render[slot].cache_epoch != base.content_epoch
            || c.cache_type() != cache_type
            || c.scale != cx.scale
            || c.bounds.width().round() != r.width().round()
            || c.bounds.height().round() != r.height().round()
    });
    // A picture holding a copy of what is under it: also stale when that changed. Asked inside
    // another picture being recorded, that picture now holds the copy too (`surface_under`).
    if cx.render[slot].reads_below && !baked {
        let below = below_key(cx, id, rect);
        stale |= below.is_none() || below != cx.render[slot].below;
    }
    let missing = if baked {
        double_buffered(cx, id, base, r, margin)
    } else {
        if stale {
            record_stale(cx, id, base, cache_type, r, margin, offset);
        }
        Missing::Live
    };
    // A double-buffered cache kept after a failed record was made with its own, older margin.
    let r = match &cx.render[slot].cache {
        Some(c) if c.margin != margin => cache_rect(base.rect, &c.margin, cache_type),
        _ => r,
    };
    draw_cache(cx, id, base, rect, r, offset, post, missing);
}

/// A new cache for a stale one; a failed ImageDoubleBuffered record keeps the last one.
#[allow(clippy::too_many_arguments)]
fn record_stale(cx: &mut PaintCx, id: ControlId, base: &Base, cache_type: CacheType, r: Rect, margin: Thickness, offset: Point) {
    let slot = id.index as usize;
    cx.render[slot].reads_below = false;
    let recorded = record(cx, id, base, cache_type, r, margin, offset);
    if cx.render[slot].reads_below {
        cx.render[slot].below = below_key(cx, id, base.rect.with_offset(offset));
    }
    match recorded {
        // DrawnUI ImageDoubleBuffered: while a new cache cannot be made, the last one is drawn.
        None if cache_type == CacheType::ImageDoubleBuffered && cx.render[slot].cache.is_some() => {}
        recorded => {
            if let Some(old) = std::mem::replace(&mut cx.render[slot].cache, recorded) {
                cx.drops.push(old);
            }
            cx.render[slot].cache_epoch = base.content_epoch;
            cx.render[slot].records += 1;
        }
    }
}

/// What a control shows this frame while it has no cache.
#[derive(Clone, Copy, PartialEq)]
enum Missing {
    /// Its content, painted live (no surface for a cache).
    Live,
    /// DrawnUI DrawPlaceholder: every frame until its first bitmap is back.
    Placeholder,
    /// Nothing (it has a bitmap, which is drawn).
    Nothing,
}

/// Blits the cache (recorded for `r`, the control's rect grown by the cache's margin) or draws
/// what a missing one shows, then the post renderers.
#[allow(clippy::too_many_arguments)]
fn draw_cache(cx: &mut PaintCx, id: ControlId, base: &Base, rect: Rect, r: Rect, offset: Point, post: bool, missing: Missing) {
    let slot = id.index as usize;
    let (left, top) = (r.left + offset.x, r.top + offset.y);
    // DrawnUI DrawRenderObject: an Image cache is not blitted under post renderers, they sample
    // it and draw in its place; a picture is replayed and they take what it painted.
    let texture = match &cx.render[slot].cache {
        Some(cache) if post => cache.image().map(|(image, _)| {
            let bounds = Rect::from_xywh(left, top, image.width() as f32, image.height() as f32);
            CachedTexture { image: image.clone(), bounds }
        }),
        _ => None,
    };
    if let Some(texture) = &texture
        && effects::post_render(cx, id, base, rect, Some(texture))
    {
        return;
    }
    match &cx.render[slot].cache {
        Some(cache) => cache.draw(cx.canvas, left, top),
        None => match missing {
            // No surface for the cache: draw live.
            Missing::Live => paint_content(cx, id, cx.canvas, rect),
            Missing::Placeholder => placeholder(cx, base, rect),
            Missing::Nothing => {}
        },
    }
    if post && texture.is_none() {
        effects::post_render(cx, id, base, rect, None);
    }
}

/// What a cache of the control records: its rect grown by `margin`. Image caches are rasterized
/// over whole device pixels, so the blit lands 1:1.
fn cache_rect(rect: Rect, margin: &Thickness, cache_type: CacheType) -> Rect {
    let grown = grow(rect, margin);
    match cache_type {
        CacheType::Operations => grown,
        _ => Rect::new(grown.left.floor(), grown.top.floor(), grown.right.ceil(), grown.bottom.ceil()),
    }
}

/// DrawnUI ImageDoubleBuffered with the host's workers (`bakes`): new content is recorded into a
/// picture a worker draws into a CPU bitmap; meanwhile the last bitmap shows as it was made (its
/// size, at the control's place). The same content at a new size or scale is made in the frame
/// (DrawnUI TrySyncRebuildStaleSize), unless a bake is out. At most one bake per control is out:
/// a change meanwhile is recorded once it is back.
fn double_buffered(cx: &mut PaintCx, id: ControlId, base: &Base, r: Rect, margin: Thickness) -> Missing {
    let slot = id.index as usize;
    // A cache of another type (use_cache changed) may be on the GPU: it goes.
    if cx.render[slot].cache.as_ref().is_some_and(|c| c.kind != CacheType::ImageDoubleBuffered) {
        let old = cx.render[slot].cache.take().expect("just seen");
        cx.drops.push(old);
    }
    // A bake is out: the bitmap it replaces shows, or the placeholder until the first one is back
    // (C# 23b52ad0: on every frame, never over a bitmap; it used to show one frame, then a hole).
    if cx.bakes.flying(id) {
        return Missing::Placeholder;
    }
    let epoch = base.content_epoch;
    let failed = cx.bakes.failed(id) == Some(epoch);
    let Some(cache) = &cx.render[slot].cache else {
        if failed {
            return Missing::Live;
        }
        let Some(picture) = record_offthread(cx, id, base, r) else {
            // Nothing to bake: tried once per content, not on every frame.
            cx.bakes.set_failed(id, epoch);
            return Missing::Live;
        };
        cx.bakes.send(id, picture, epoch, r, margin, cx.scale);
        return Missing::Placeholder;
    };
    let content = cx.render[slot].cache_epoch != epoch;
    let resized = (cache.bounds.width() - r.width()).abs() > 1.0
        || (cache.bounds.height() - r.height()).abs() > 1.0
        || cache.scale != cx.scale;
    if content && !failed {
        match record_offthread(cx, id, base, r) {
            Some(picture) => cx.bakes.send(id, picture, epoch, r, margin, cx.scale),
            // Nothing to bake: the last bitmap stays, tried again only for other content.
            None => cx.bakes.set_failed(id, epoch),
        }
    } else if resized && !content {
        let made = record_offthread(cx, id, base, r).and_then(|picture| crate::bakes::rasterize(&picture, r));
        match made {
            Some(image) => {
                let cache = CachedObject::double_buffered(image, r, margin, cx.scale);
                if let Some(old) = cx.render[slot].cache.replace(cache) {
                    cx.drops.push(old);
                }
                cx.render[slot].records += 1;
            }
            None => cx.bakes.set_failed(id, epoch),
        }
    }
    Missing::Nothing
}

/// What `id` paints, recorded for a worker: no GPU texture goes in (`PaintCx::offthread`).
fn record_offthread(cx: &mut PaintCx, id: ControlId, base: &Base, r: Rect) -> Option<Picture> {
    let mut recorder = PictureRecorder::new();
    let canvas = recorder.begin_recording(r, false);
    let was = std::mem::replace(&mut cx.offthread, true);
    let outer = cx.target.take();
    paint_content(cx, id, canvas, base.rect);
    cx.target = outer;
    cx.offthread = was;
    recorder.finish_recording_as_picture(None)
}

/// DrawnUI DrawPlaceholder: the control's background color, else a faint gray.
fn placeholder(cx: &mut PaintCx, base: &Base, rect: Rect) {
    let color = base.p.background_color.filter(|c| c.a() > 2).unwrap_or(Color::from_argb(32, 128, 128, 128));
    let mut paint = Paint::default();
    paint.set_color(color);
    cx.canvas.draw_rect(rect, &paint);
}

/// The GPU context changed (DrawnUI GraphicContextMismatch, found for every control at once
/// instead of at each one's next draw): caches, kept surfaces and effect textures of the old one go
/// and are made again as they are painted. CPU bitmaps stay (ImageDoubleBuffered on the desktop,
/// decoded pictures, SVG rasters).
pub(crate) fn gpu_changed(tree: &mut Tree) {
    for render in &mut tree.render {
        if render.cache.as_ref().is_some_and(CachedObject::on_gpu) {
            render.cache = None;
        }
        render.paints = None;
    }
    for node in tree.nodes.iter_mut().flatten() {
        for effect in &mut node.base.visual_effects {
            effect.gpu_lost();
        }
    }
    tree.needs_frame = true;
}

/// A worker's bitmap of an ImageDoubleBuffered cache is back: it becomes the control's cache and
/// the controls above draw it again. A failed one is not tried again until the content changes.
pub(crate) fn bake_arrived(tree: &mut Tree, request: u32, image: Option<Image>) {
    let Some((slot, flying)) = tree.bakes.arrived(request) else { return };
    if tree.node(flying.control).is_none() {
        return;
    }
    match image {
        Some(image) => {
            let cache = CachedObject::double_buffered(image, flying.bounds, flying.margin, flying.scale);
            let render = &mut tree.render[slot];
            if let Some(old) = render.cache.replace(cache) {
                tree.drops.push(old);
            }
            render.cache_epoch = flying.epoch;
            render.records += 1;
        }
        None => tree.bakes.set_failed(flying.control, flying.epoch),
    }
    tree.invalidate(flying.control, crate::types::Dirty::REPAINT);
    tree.needs_frame = true;
}

/// Records the content of a control into a new cache for the rect `r`: its own rect grown by
/// `margin`. `offset` moves it where it is drawn.
fn record(
    cx: &mut PaintCx,
    id: ControlId,
    base: &Base,
    cache_type: CacheType,
    r: Rect,
    margin: Thickness,
    offset: Point,
) -> Option<CachedObject> {
    let scale = cx.scale;
    match cache_type {
        CacheType::Operations | CacheType::OperationsFull => {
            let mut recorder = PictureRecorder::new();
            let canvas = recorder.begin_recording(r, false);
            // The picture is replayed moved by `offset` on this canvas: the surface it lands on.
            let here = Matrix::concat(&cx.canvas.local_to_device_as_3x3(), &Matrix::translate(offset));
            // SAFETY: only asked whether the canvas draws into a surface; nothing goes through it.
            let target = match unsafe { cx.canvas.surface() } {
                Some(_) => Some(Target { canvas: cx.canvas, matrix: here, recording: id }),
                None => cx.target.map(|t| Target { canvas: t.canvas, matrix: Matrix::concat(&t.matrix, &here), recording: id }),
            };
            let outer = std::mem::replace(&mut cx.target, target);
            paint_content(cx, id, canvas, base.rect);
            cx.target = outer;
            let picture = recorder.finish_recording_as_picture(None)?;
            Some(CachedObject { content: CachedContent::Picture(picture), bounds: r, margin, scale, kind: cache_type })
        }
        CacheType::Image | CacheType::ImageDoubleBuffered => {
            let (w, h) = ((r.width().round() as i32).max(1), (r.height().round() as i32).max(1));
            let mut offscreen = cx.gpu.offscreen(w, h)?;
            let canvas = offscreen.canvas();
            canvas.clear(Color::TRANSPARENT);
            canvas.translate((-r.left, -r.top));
            paint_content(cx, id, canvas, base.rect);
            let image = offscreen.image_snapshot();
            Some(CachedObject { content: CachedContent::Image(image), bounds: r, margin, scale, kind: cache_type })
        }
        CacheType::ImageComposite => {
            let image = record_composite(cx, id, base, r)?;
            Some(CachedObject { content: CachedContent::Image(image), bounds: r, margin, scale, kind: cache_type })
        }
        // `resolved` turned the GPU names into the caches above.
        CacheType::None | CacheType::GPU | CacheType::ImageCompositeGPU => None,
    }
}

// ---------------------------------------------------------------- what is under a control

/// What the canvas shows under control `id` and where `rect` (its drawing rect on the canvas of
/// `cx`) lands on the surface: a picture holding a copy of what is under it records again when
/// this changes. `None` when that cannot be told.
fn below_key(cx: &mut PaintCx, id: ControlId, rect: Rect) -> Option<u64> {
    let below = painted_below(cx, id)?;
    let device = effects::device_bounds(cx, rect);
    let mut h = DefaultHasher::new();
    (below, [device.left, device.top, device.right, device.bottom]).hash(&mut h);
    Some(h.finish())
}

/// A key of what the canvas shows under control `id` when it paints (SkiaBackdrop): the own paint
/// of its ancestors and every control drawn before it, up to the root. It changes when any of
/// that may have changed. `None` when that cannot be told: a control below with a visual effect or
/// an overlay may draw something new without a change of its own.
pub(crate) fn painted_below(cx: &PaintCx, id: ControlId) -> Option<u64> {
    let mut h = DefaultHasher::new();
    let mut child = cx.node(id)?;
    while let Some(parent) = child.parent.and_then(|p| cx.node(p)) {
        let b = &parent.base;
        (b.own_epoch, b.content_offset.x.to_bits(), b.content_offset.y.to_bits()).hash(&mut h);
        let (z, at) = (child.base.p.z_index, parent.children.iter().position(|c| *c == child.id)?);
        for (i, &sibling) in parent.children.iter().enumerate() {
            let Some(s) = cx.node(sibling) else { continue };
            let p = &s.base.p;
            // Drawn before `child`: lower z, or the same z and earlier (as `paint_children`).
            if !p.is_visible || !(p.z_index < z || (p.z_index == z && i < at)) {
                continue;
            }
            if !s.base.visual_effects.is_empty() || cx.animators.iter().any(|a| a.control == sibling && a.overlay.is_some()) {
                return None;
            }
            let r = s.base.rect;
            (sibling.index, s.base.content_epoch, p.opacity.to_bits(), [r.left, r.top, r.right, r.bottom].map(f32::to_bits)).hash(&mut h);
            // Its transform, as drawn this frame (it was drawn before `child`).
            if let Some(m) = cx.render[sibling.index as usize].matrix {
                let mut values = [0.0; 9];
                m.get_9(&mut values);
                values.map(f32::to_bits).hash(&mut h);
            }
        }
        child = parent;
    }
    Some(h.finish())
}

// ---------------------------------------------------------------- ImageComposite

/// The kept surface of an ImageComposite cache and what its last record knew (DrawnUI
/// SetupRenderingWithComposition, as DrawnUi.React).
pub(crate) struct Composite {
    surface: Surface,
    scale: f32,
    /// Children a change came through since the last record (`layout::flush`).
    dirty: Vec<ControlId>,
    /// The next record draws everything: the control itself or its layout changed.
    full: bool,
    /// Where each child was drawn at the last record, relative to the cache's top-left.
    bounds: Vec<(ControlId, Rect)>,
    /// What the last record drew: only some children, and which (React LastCompositeRecord).
    partial: bool,
    drawn: Vec<ControlId>,
    /// The areas erased by the last record; reused.
    rects: Vec<Rect>,
}

/// A change reached an ImageComposite control (`layout::flush`): through its child `child`, or,
/// with `None`, from the control itself or a new layout, and its next record draws everything.
pub(crate) fn composite_changed(render: &mut [RenderSlot], id: ControlId, child: Option<ControlId>) {
    let paints = render.get_mut(id.index as usize).and_then(|slot| slot.paints.as_deref_mut());
    // No state yet: the first record draws everything anyway.
    let Some(state) = paints.and_then(|p| p.composite.as_mut()) else { return };
    match child {
        None => state.full = true,
        Some(child) if !state.dirty.contains(&child) => state.dirty.push(child),
        Some(_) => {}
    }
}

/// What the last record of an ImageComposite cache did (React LastCompositeRecord).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeRecord {
    /// Only some children were drawn again (React Mode "partial"); false: everything was.
    pub partial: bool,
    /// The children drawn again, in drawing order; their count is React's Children.
    pub children: Vec<ControlId>,
}

impl Tree {
    /// What the last record of the ImageComposite cache of `id` did; `None` before the first.
    pub fn last_composite_record(&self, id: impl Into<ControlId>) -> Option<CompositeRecord> {
        let slot = self.render.get(id.into().index as usize)?;
        let state = slot.paints.as_deref()?.composite.as_ref()?;
        Some(CompositeRecord { partial: state.partial, children: state.drawn.clone() })
    }
}

impl Cx<'_> {
    /// What the last record of the ImageComposite cache of `id` did; `None` before the first.
    pub fn last_composite_record(&self, id: impl Into<ControlId>) -> Option<CompositeRecord> {
        self.tree.last_composite_record(id)
    }
}

/// Where a child is drawn inside its parent, canvas pixels: its rect grown by its effects margin,
/// through its transform, moved by the parent's content offset (DrawnUI GetTransformedDirtyBounds).
fn drawn_bounds(cx: &mut PaintCx, child: ControlId, offset: Point) -> Rect {
    let Some(node) = cx.node(child).filter(|n| n.base.p.is_visible) else { return Rect::default() };
    let rect = grow(node.base.rect, &effects_margin(cx, child));
    let rect = control_matrix(&node.base, cx.scale).map_or(rect, |m| m.map_rect(rect).0);
    rect.with_offset(offset)
}

/// Records an ImageComposite cache into its kept surface (DrawnUI ImageComposite, as React):
/// when only children changed since the last record, their old and new places, and every
/// sibling that overlaps those, are erased and only those children are drawn again. Anything else
/// (the control itself, its layout, size or scale, a control that is not a plain layout) draws
/// everything. Returns the picture of the surface.
fn record_composite(cx: &mut PaintCx, id: ControlId, base: &Base, r: Rect) -> Option<Image> {
    let slot = id.index as usize;
    let (w, h) = ((r.width().round() as i32).max(1), (r.height().round() as i32).max(1));
    let kept = cx.render[slot].paints.as_deref_mut().and_then(|p| p.composite.take());
    let mut state = match kept {
        Some(state) if state.surface.width() == w && state.surface.height() == h && state.scale == cx.scale => state,
        kept => {
            let (bounds, drawn, rects) = kept.map_or_else(Default::default, |k| (k.bounds, k.drawn, k.rects));
            let surface = cx.gpu.offscreen(w, h)?;
            Composite { surface, scale: cx.scale, dirty: Vec::new(), full: true, bounds, partial: false, drawn, rects }
        }
    };
    let node = cx.node(id)?;
    let children: &[ControlId] = &node.children;
    // Only a plain layout paints nothing but its background and its children.
    let plain = node.kind.as_deref().is_some_and(|k| (k as &dyn Any).downcast_ref::<SkiaLayout>().is_some_and(|l| l.items.is_none()));
    state.dirty.retain(|c| children.contains(c));
    let partial = plain && !state.full && !state.dirty.is_empty();
    let offset = base.content_offset;
    let origin = Point::new(r.left, r.top);

    state.drawn.clear();
    state.rects.clear();
    if partial {
        // What changed: the reported children where they were and where they are now, then every
        // sibling that overlaps any of that, until no more does.
        let old = |state: &Composite, child: ControlId| state.bounds.iter().find(|b| b.0 == child).map(|b| b.1.with_offset(origin));
        for i in 0..state.dirty.len() {
            let child = state.dirty[i];
            let now = drawn_bounds(cx, child, offset);
            state.rects.push(now);
            state.rects.extend(old(&state, child));
            state.drawn.push(child);
        }
        let mut grew = true;
        while grew {
            grew = false;
            for &child in children {
                if state.drawn.contains(&child) {
                    continue;
                }
                let (now, was) = (drawn_bounds(cx, child, offset), old(&state, child));
                let hits = |rect: &Rect| !rect.is_empty() && state.rects.iter().any(|d| Rect::intersects2(d, rect));
                if hits(&now) || was.as_ref().is_some_and(hits) {
                    state.rects.push(now);
                    state.rects.extend(was);
                    state.drawn.push(child);
                    grew = true;
                }
            }
        }
        // Drawn in the order `paint_children` draws them.
        let z = |c: &ControlId| (cx.node(*c).map_or(0, |n| n.base.p.z_index), children.iter().position(|x| x == c));
        state.drawn.sort_unstable_by_key(z);
    }

    let canvas = state.surface.canvas();
    canvas.save();
    canvas.translate((-r.left, -r.top));
    if partial {
        let mut clip = PathBuilder::new();
        let mut erase = Paint::default();
        erase.set_blend_mode(BlendMode::Clear);
        for d in &state.rects {
            let d = Rect::new(d.left.floor(), d.top.floor(), d.right.ceil(), d.bottom.ceil());
            clip.add_rect(d, None, None);
            canvas.draw_rect(d, &erase);
        }
        canvas.clip_path(&clip.detach(), ClipOp::Intersect, false);
        let kind = node.kind.as_deref()?;
        let mut own = PaintCx {
            canvas,
            rect: base.rect,
            scale: cx.scale,
            id,
            fonts: cx.fonts,
            nodes: cx.nodes,
            animators: cx.animators,
            render: &mut *cx.render,
            drops: &mut *cx.drops,
            gpu: &mut *cx.gpu,
            bakes: &mut *cx.bakes,
            offthread: cx.offthread,
            target: cx.target,
        };
        kind.paint_background(&mut own);
        canvas.translate(offset);
        for &child in &state.drawn {
            render(&mut own, child);
        }
    } else {
        canvas.clear(Color::TRANSPARENT);
        paint_content(cx, id, canvas, base.rect);
        state.drawn.extend_from_slice(children);
    }
    canvas.restore();

    state.bounds.clear();
    for &child in children {
        let now = drawn_bounds(cx, child, offset).with_offset(-origin);
        state.bounds.push((child, now));
    }
    state.partial = partial;
    state.dirty.clear();
    state.full = false;
    let image = cx.gpu.snapshot(&mut state.surface, None);
    cx.render[slot].paints.get_or_insert_default().composite = Some(state);
    image
}

/// Background, then the control's own paint: what a cache captures.
fn paint_content(cx: &mut PaintCx, id: ControlId, canvas: &skia_safe::Canvas, rect: Rect) {
    let Some(kind) = cx.node(id).and_then(|n| n.kind.as_deref()) else { return };
    let mut own = PaintCx {
        canvas,
        rect,
        scale: cx.scale,
        id,
        fonts: cx.fonts,
        nodes: cx.nodes,
        animators: cx.animators,
        render: &mut *cx.render,
        drops: &mut *cx.drops,
        gpu: &mut *cx.gpu,
        bakes: &mut *cx.bakes,
        offthread: cx.offthread,
        target: cx.target,
    };
    kind.paint_background(&mut own);
    kind.paint(&mut own);
}

/// Overlay effects of running animators (DrawnUI ExecutePostAnimators): above the content and
/// outside its cache, clipped to the control's shape.
fn paint_overlays(cx: &mut PaintCx, id: ControlId, rect: Rect) {
    let animators = cx.animators;
    if !animators.iter().any(|a| a.control == id && a.overlay.is_some()) {
        return;
    }
    let Some(node) = cx.node(id) else { return };
    let Some(kind) = node.kind.as_deref() else { return };
    let canvas = cx.canvas;
    canvas.save();
    // ClipEffects: false leaves overlays unclipped.
    if node.base.p.clip_effects {
        canvas.clip_path(&kind.create_clip(rect, cx.scale), ClipOp::Intersect, true);
    }
    let mut own = PaintCx {
        canvas,
        rect,
        scale: cx.scale,
        id,
        fonts: cx.fonts,
        nodes: cx.nodes,
        animators,
        render: &mut *cx.render,
        drops: &mut *cx.drops,
        gpu: &mut *cx.gpu,
        bakes: &mut *cx.bakes,
        offthread: cx.offthread,
        target: cx.target,
    };
    for a in animators.iter().filter(|a| a.control == id) {
        if let Some(draw) = &a.overlay {
            draw(&mut own, a.value);
        }
    }
    canvas.restore();
}

/// The default background: `fill_gradient` or `background_color` over the drawing rect.
pub(crate) fn paint_background_rect(cx: &mut PaintCx) {
    if let Some(paint) = background_paint(cx, cx.rect, (0.0, 0.0)) {
        cx.canvas.draw_rect(cx.rect, &paint);
    }
}

// ---------------------------------------------------------------- gradients and shadows

/// Paint objects a control reuses between frames: built in paint, where the rect is known, and
/// kept in its render slot (upstream keeps them in fields of the control).
#[derive(Default)]
pub(crate) struct PaintCache {
    /// The shader of `fill_gradient`.
    pub fill: Option<GradientShader>,
    /// The shader of a shape's `stroke_gradient`.
    pub stroke: Option<GradientShader>,
    /// A shape's path, shadow filters and dash.
    pub shape: Option<ShapeCache>,
    /// A backdrop's blur and brightness filters.
    pub backdrop: Option<BackdropPaint>,
    /// The kept surface of an ImageComposite cache.
    pub composite: Option<Composite>,
}

/// A gradient's shader and what it was built for.
pub(crate) struct GradientShader {
    gradient: SkiaGradient,
    rect: Rect,
    angles: (f32, f32),
    shader: Option<Shader>,
}

/// The shader of `gradient` over `rect`, built again only when the gradient, the rect or the
/// angles changed (DrawnUI SetupGradient, the caching overload).
pub(crate) fn gradient_shader(
    cache: &mut Option<GradientShader>,
    gradient: &SkiaGradient,
    rect: Rect,
    angles: (f32, f32),
) -> Option<Shader> {
    match cache {
        Some(c) if c.rect == rect && c.angles == angles && c.gradient == *gradient => {}
        _ => {
            let shader = create_gradient(gradient, rect, angles);
            *cache = Some(GradientShader { gradient: gradient.clone(), rect, angles, shader });
        }
    }
    cache.as_ref()?.shader.clone()
}

/// DrawnUI CreateGradient: the shader of a gradient over `rect`, pixels. `angles` are the start
/// and the sweep of a Sweep gradient in degrees (the control's Value1 and Value2); a zero sweep
/// is the full circle.
pub(crate) fn create_gradient(g: &SkiaGradient, rect: Rect, angles: (f32, f32)) -> Option<Shader> {
    if g.gradient_type == GradientType::None || g.colors.is_empty() {
        return None;
    }
    // Below 1 every channel is scaled down, above 1 it moves toward white (DrawnUi.Net MakeDarker / MakeLighter).
    let lit = |channel: u8| {
        let c = channel as f32 / 255.0;
        if g.light < 1.0 { c * g.light.max(0.0) } else { c + (1.0 - c) * (g.light - 1.0).min(1.0) }
    };
    let color = |c: &Color| Color4f::new(lit(c.r()), lit(c.g()), lit(c.b()), c.a() as f32 / 255.0 * g.opacity);
    let colors: Vec<Color4f> = g.colors.iter().map(color).collect();
    let positions = (g.color_positions.len() == colors.len()).then_some(&g.color_positions[..]);
    let colors = gradient::Colors::new(&colors, positions, g.tile_mode, None);
    let gradient = Gradient::new(colors, Interpolation::default());
    let at = |x: f32, y: f32| Point::new(rect.left + rect.width() * x, rect.top + rect.height() * y);
    let (start, end) = (at(g.start_x_ratio, g.start_y_ratio), at(g.end_x_ratio, g.end_y_ratio));
    let (width, height) = (rect.width(), rect.height());
    match g.gradient_type {
        GradientType::Sweep => {
            let sweep = if angles.1 == 0.0 { 360.0 } else { angles.1 };
            gradient::shaders::sweep_gradient(rect.center(), (angles.0, angles.0 + sweep), &gradient, None)
        }
        GradientType::Circular => gradient::shaders::radial_gradient((start, width.min(height) / 2.0), &gradient, None),
        // A circle over the longer side, squeezed to the shorter one around its center.
        GradientType::Oval => {
            let squeeze = if width >= height { (1.0, height / width) } else { (width / height, 1.0) };
            let mut matrix = Matrix::new_identity();
            matrix.set_scale(squeeze, start);
            gradient::shaders::radial_gradient((start, width.max(height) / 2.0), &gradient, &matrix)
        }
        _ => gradient::shaders::linear_gradient((start, end), &gradient, None),
    }
}

/// The paint of a control's background over `rect` (DrawnUI SetupBackgroundPaint): its
/// `fill_gradient`, else its `background_color`. `None` when there is nothing to fill.
/// `angles`: see `create_gradient`.
pub(crate) fn background_paint(cx: &mut PaintCx, rect: Rect, angles: (f32, f32)) -> Option<Paint> {
    let p = &cx.base().p;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if let Some(gradient) = p.fill_gradient.as_deref() {
        let paints = cx.render[cx.id.index as usize].paints.get_or_insert_default();
        if let Some(shader) = gradient_shader(&mut paints.fill, gradient, rect, angles) {
            paint.set_shader(shader);
            paint.set_blend_mode(gradient.blend_mode);
            return Some(paint);
        }
    }
    paint.set_color(p.background_color.filter(|color| color.a() > 0)?);
    Some(paint)
}

/// DrawnUI CreateShadow: the drop-shadow filter of a shadow. A fully opaque color takes the
/// shadow's `opacity`; a color with its own alpha keeps it.
pub(crate) fn create_shadow(shadow: &SkiaShadow, scale: f32) -> Option<ImageFilter> {
    let color = match shadow.color.a() {
        255 => shadow.color.with_a((shadow.opacity.clamp(0.0, 1.0) * 255.0).round() as u8),
        _ => shadow.color,
    };
    let (offset, sigma) = ((shadow.x * scale, shadow.y * scale), (shadow.blur * scale, shadow.blur * scale));
    if shadow.shadow_only {
        image_filters::drop_shadow_only(offset, sigma, color, None, None, None)
    } else {
        image_filters::drop_shadow(offset, sigma, color, None, None, None)
    }
}
