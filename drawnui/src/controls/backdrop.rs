//! SkiaBackdrop (DrawnUI SkiaBackdrop, as DrawnUi.React): frosted glass. It draws what is already
//! painted under its box again through `blur` and `brightness`, tinted by `background_color`.
//! Its children draw first and are blurred with the rest. The copy comes from the surface being
//! drawn into: the window, or the offscreen surface of the Image cache above it being recorded.
//!
//! The blurred copy is kept and drawn again while nothing under the box changed (the controls
//! drawn before it, its ancestors' own paint, its own content, its place on the surface): a
//! popup that animates above a backdrop does not blur the screen on every frame. C# and React
//! copy and blur on every frame the backdrop is drawn in.

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use skia_safe::{ClipOp, Color, FilterMode, MipmapMode, Paint, SamplingOptions, Surface, TileMode, color_filters, image_filters};

use crate::control::{Control, Has, PaintCx};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::effects::CachedTexture;
use crate::props;
use crate::tree::{Build, Container};
use crate::{effects, paint};

props!(BackdropProps, BackdropBuild, BackdropSet {
    /// Points of blur; 0 = none (DrawnUI default 5).
    blur / set_blur: f32 = 5.0, DRAW;
    /// Gamma of what is under the box: 1 = unchanged (DrawnUI SkiaImageEffects.Gamma: a value is
    /// moved by 1 toward 1 first, so 0.5 gives 1.5 and 1.5 gives 0.5).
    brightness / set_brightness: f32 = 1.0, DRAW;
});

/// The frosted glass layer: an Absolute layout filling its parent.
#[derive(Default)]
pub struct SkiaBackdrop {
    layout: SkiaLayout,
    pub p: BackdropProps,
    /// Times it copied and blurred what is under it (diagnostics).
    copies: Cell<u32>,
}

impl SkiaBackdrop {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaBackdrop> {
        Build::new(SkiaBackdrop::default()).fill()
    }

    /// Blur or brightness changes what is under it (DrawnUI HasEffects).
    pub fn has_effects(&self) -> bool {
        self.p.blur != 0.0 || self.p.brightness != 1.0
    }

    /// How many times it copied and blurred what is under it; a frame with nothing new under it
    /// draws the kept copy.
    pub fn copies(&self) -> u32 {
        self.copies.get()
    }
}

/// What a backdrop keeps between frames: the filters and the blur (pixels) and brightness they
/// were made for, the blurred copy and the key of what was under it, and the surface the copy was
/// blurred into (reused).
pub(crate) struct BackdropPaint {
    blur: f32,
    brightness: f32,
    paint: Paint,
    blurred: Option<(u64, CachedTexture)>,
    surface: Option<Surface>,
}

/// DrawnUI SkiaImageEffects.Gamma: a table of x^gamma after moving the value by 1 toward 1.
fn gamma(value: f32) -> Option<skia_safe::ColorFilter> {
    let gamma = if value < 1.0 { value + 1.0 } else if value > 1.0 { value - 1.0 } else { 1.0 };
    if gamma <= 0.0 {
        return None;
    }
    let curve: [u8; 256] = std::array::from_fn(|i| ((i as f32 / 255.0).powf(gamma) * 255.0) as u8);
    color_filters::table(&curve)
}

impl Has<LayoutProps> for SkiaBackdrop {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Has<BackdropProps> for SkiaBackdrop {
    fn part(&self) -> &BackdropProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut BackdropProps {
        &mut self.p
    }
}

impl Container for SkiaBackdrop {}

impl Control for SkiaBackdrop {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The tint is painted in `paint`, over the children (C# order).
    fn paint_background(&self, _cx: &mut PaintCx) {}

    fn paint(&self, cx: &mut PaintCx) {
        let d = cx.rect;
        if d.width() <= 0.0 || d.height() <= 0.0 {
            return;
        }
        self.layout.paint(cx);
        if let Some(tint) = paint::background_paint(cx, d, (0.0, 0.0)) {
            cx.canvas.draw_rect(d, &tint);
        }
        // A worker's picture takes no texture: the children and the tint stay.
        if !self.has_effects() || cx.offthread {
            return;
        }
        let (blur, brightness) = (self.p.blur * cx.scale, self.p.brightness);
        // What is under the box and where the box is on the surface; `None`: not to be told.
        let key = crate::paint::painted_below(cx, cx.id).map(|below| {
            let mut h = DefaultHasher::new();
            let device = effects::device_bounds(cx, d);
            (below, cx.base().content_epoch, [device.left, device.top, device.right, device.bottom]).hash(&mut h);
            (blur.to_bits(), brightness.to_bits()).hash(&mut h);
            h.finish()
        });
        let slot = cx.id.index as usize;
        let kept = cx.render[slot].paints.as_deref().and_then(|p| p.backdrop.as_ref()).and_then(|b| b.blurred.as_ref());
        let texture = match kept {
            Some((made_for, texture)) if key == Some(*made_for) => texture.clone(),
            _ => {
                // Nothing to copy while a picture is recorded: the children and the tint stay.
                let Some(copy) = effects::snapshot(cx, d) else { return };
                self.copies.set(self.copies.get() + 1);
                let Some(texture) = self.blur(cx, copy, blur, brightness) else { return };
                if let Some(backdrop) = cx.render[slot].paints.as_deref_mut().and_then(|p| p.backdrop.as_mut()) {
                    backdrop.blurred = key.map(|key| (key, texture.clone()));
                }
                texture
            }
        };
        let canvas = cx.canvas;
        canvas.save();
        canvas.clip_rect(d, ClipOp::Intersect, true);
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
        canvas.draw_image_rect_with_sampling_options(&texture.image, None, texture.bounds, sampling, &Paint::default());
        canvas.restore();
    }
}

impl SkiaBackdrop {
    /// The copy drawn through the blur and the brightness, into the kept surface of its size.
    fn blur(&self, cx: &mut PaintCx, copy: CachedTexture, blur: f32, brightness: f32) -> Option<CachedTexture> {
        let paints = cx.render[cx.id.index as usize].paints.get_or_insert_default();
        let backdrop = match paints.backdrop.take() {
            Some(b) if b.blur == blur && b.brightness == brightness => b,
            old => {
                let mut paint = Paint::default();
                if blur > 0.0 {
                    paint.set_image_filter(image_filters::blur((blur, blur), TileMode::Mirror, None, None));
                }
                if brightness != 1.0 {
                    paint.set_color_filter(gamma(brightness));
                }
                BackdropPaint { blur, brightness, paint, blurred: None, surface: old.and_then(|o| o.surface) }
            }
        };
        let mut backdrop = backdrop;
        // The kept copy goes first: the surface is drawn again without copying its pixels.
        backdrop.blurred = None;
        let (w, h) = (copy.image.width(), copy.image.height());
        let fits = backdrop.surface.as_ref().is_some_and(|s| s.width() == w && s.height() == h);
        if !fits {
            backdrop.surface = cx.gpu.offscreen(w, h);
        }
        let result = backdrop.surface.as_mut().and_then(|surface| {
            let canvas = surface.canvas();
            canvas.clear(Color::TRANSPARENT);
            let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
            let dst = skia_safe::Rect::from_iwh(w, h);
            canvas.draw_image_rect_with_sampling_options(&copy.image, None, dst, sampling, &backdrop.paint);
            let image = cx.gpu.snapshot(surface, None)?;
            Some(CachedTexture { image, bounds: copy.bounds })
        });
        cx.render[cx.id.index as usize].paints.get_or_insert_default().backdrop = Some(backdrop);
        result
    }
}
