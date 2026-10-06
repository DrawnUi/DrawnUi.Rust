//! SkiaGif: plays the frames of a GIF (DrawnUI SkiaGif, React `SkiaGif.ts`). The frames come
//! decoded from the host through the image manager, once per source; a `FramePlayer` picks the
//! frame by time on the frame clock; the frame is drawn like SkiaImage draws its bitmap.

use std::any::Any;
use std::sync::Arc;

use skia_safe::{ClipOp, FilterMode, MipmapMode, Paint, SamplingOptions, Size};

use crate::animated_frames::{self, FramePlayer, FramesProps};
use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::image::{DrawImageAlignment, Loaded, TransformAspect, place};
use crate::images::Frames;
use crate::props;
use crate::tree::{Build, ControlId, Cx, Handle, Tree, wrong_state};
use crate::types::{CacheType, Dirty, IntoProp};

props!(GifProps, GifBuild, GifSet {
    /// Path or URL of the GIF, loaded by the host like an image. Empty = nothing.
    source / set_source: String = String::new(), MEASURE;
    aspect / set_aspect: TransformAspect = TransformAspect::AspectFitFill, MEASURE;
    horizontal_alignment / set_horizontal_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
    vertical_alignment / set_vertical_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
});

#[derive(Default)]
pub struct SkiaGif {
    pub p: GifProps,
    player: FramePlayer,
    /// The frames of `resolved`, shared with the manager's cache.
    frames: Option<Arc<Frames>>,
    /// The source the manager was last asked for.
    pub(crate) resolved: String,
    loading: bool,
    error: bool,
    paint: Paint,
    pub(crate) on_success: Option<Loaded>,
    pub(crate) on_error: Option<Loaded>,
}

impl SkiaGif {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaGif> {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        // DrawnUI's ImageDoubleBuffered where workers make the bitmap (the desktop); in the browser
        // a frame changing every 100 ms or less is drawn as it is, without one more offscreen pass
        // (PARITY.md, SkiaGif).
        let cache = if cfg!(target_os = "emscripten") { CacheType::None } else { CacheType::ImageDoubleBuffered };
        Build::new(SkiaGif { paint, ..SkiaGif::default() }).source(source).use_cache(cache)
    }

    /// The frames of the current source (DrawnUI Animation): count, durations, size.
    pub fn animation(&self) -> Option<&Frames> {
        self.frames.as_deref().filter(|_| self.resolved == self.p.source)
    }

    /// The player: `current()`, `is_playing()`, `position()`.
    pub fn player(&self) -> &FramePlayer {
        &self.player
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn has_error(&self) -> bool {
        self.error
    }

    /// Asks the manager for the frames of the current source, once per source.
    fn resolve(&mut self, cx: &mut LayoutCx) {
        if self.resolved == self.p.source {
            return;
        }
        let images = &mut cx.tree.images;
        images.release(&self.resolved, cx.id);
        self.resolved.clone_from(&self.p.source);
        (self.frames, self.error) = (None, false);
        if !self.resolved.is_empty()
            && let Some(frames) = images.request_frames(&self.resolved, cx.id)
        {
            if self.on_success.is_some() {
                images.events.push((cx.id, self.resolved.clone(), true));
            }
            self.take(frames);
        }
        self.loading = self.frames.is_none() && !self.resolved.is_empty();
    }

    fn take(&mut self, frames: Arc<Frames>) {
        self.player.set_frames(frames.durations.iter().map(|ms| *ms as f32));
        self.frames = Some(frames);
    }

    /// The manager's answer for a control waiting for `source`. Returns the handler events due.
    pub(crate) fn arrived(
        tree: &mut Tree,
        id: ControlId,
        source: &str,
        frames: Option<Arc<Frames>>,
    ) -> Vec<(ControlId, String, bool)> {
        let Some(mut me) = tree.find_mut::<SkiaGif>(id) else { return Vec::new() };
        let gif = me.control_mut();
        let mut events = Vec::new();
        match frames {
            Some(frames) => {
                gif.take(frames);
                (gif.loading, gif.error) = (false, false);
                if gif.on_success.is_some() {
                    events.push((id, source.to_owned(), true));
                }
                // Laid out again: the arrange plays it when it is set to, so Started comes after
                // Success, as React (and no control is searched for players on every arrival).
                me.mark(Dirty::MEASURE);
            }
            None => {
                (gif.loading, gif.error) = (false, true);
                if gif.on_error.is_some() {
                    events.push((id, source.to_owned(), false));
                }
            }
        }
        events
    }
}

impl Has<GifProps> for SkiaGif {
    fn part(&self) -> &GifProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut GifProps {
        &mut self.p
    }
}

impl Has<FramesProps> for SkiaGif {
    fn part(&self) -> &FramesProps {
        &self.player.p
    }
    fn part_mut(&mut self) -> &mut FramesProps {
        &mut self.player.p
    }
}

impl Control for SkiaGif {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.player)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.player)
    }

    /// React `SkiaGif.MeasureAbsolute`: the offered box; an unbounded side follows the frames'
    /// aspect, and is 0 until they are there.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        self.resolve(cx);
        let (mut w, mut h) = (width, height);
        if let Some(size) = self.animation().map(|frames| frames.size).filter(|size| !size.is_empty()) {
            let aspect = size.width as f32 / size.height as f32;
            if !w.is_finite() && h.is_finite() {
                w = h * aspect;
            } else if !h.is_finite() && w.is_finite() {
                h = w / aspect;
            }
        }
        if !w.is_finite() || !h.is_finite() {
            return Size::default();
        }
        Size::new(w, h)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        animated_frames::placed(&mut self.player, cx.tree, cx.id);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let Some(frames) = self.animation() else { return };
        let Some(image) = frames.images.get(self.player.current()) else { return };
        let (h, v) = (self.p.horizontal_alignment, self.p.vertical_alignment);
        let display = place(frames.size, cx.rect, self.p.aspect, h, v, (1.0, 1.0), (0.0, 0.0));
        cx.canvas.save();
        cx.canvas.clip_rect(cx.rect, ClipOp::Intersect, true);
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
        cx.canvas.draw_image_rect_with_sampling_options(image, None, display, sampling, &self.paint);
        cx.canvas.restore();
    }
}

fn handler<T: 'static, S: Any>(
    me: Handle<T>,
    mut f: impl FnMut(Handle<T>, &mut S, &mut Cx<'_>, &str) + 'static,
) -> Loaded {
    Box::new(move |state, cx, source| {
        let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
        f(me, state, cx, source)
    })
}

// ponytail: on SkiaGif itself, not on every control that embeds one: a generic impl would
// overlap SkiaImage's handlers of the same names.
impl Build<SkiaGif> {
    /// Runs when the frames of the current source are there (DrawnUI Success).
    pub fn on_success<S: Any>(
        mut self,
        f: impl FnMut(Handle<SkiaGif>, &mut S, &mut Cx<'_>, &str) + 'static,
    ) -> Self {
        let me = self.handle();
        self.control_mut().on_success = Some(handler(me, f));
        self
    }

    /// Runs when the current source could not be loaded or decoded (DrawnUI Error).
    pub fn on_error<S: Any>(mut self, f: impl FnMut(Handle<SkiaGif>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        self.control_mut().on_error = Some(handler(me, f));
        self
    }
}
