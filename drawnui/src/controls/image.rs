//! SkiaImage: a bitmap from the image manager (`images.rs`), scaled into the control's box by
//! `aspect` and placed by the two alignments. Same rules as DrawnUI SkiaImage.

use std::any::Any;
use std::cell::RefCell;
use std::sync::Arc;

use skia_safe::{
    BlendMode, ClipOp, Color, ColorFilter, Contains, CubicResampler, FilterMode, ISize, Image, ImageFilter, ImageInfo,
    MipmapMode, Paint, Rect, SamplingOptions, Size, TileMode, color_filters, image_filters, surfaces,
};

use crate::control::{Control, Has, LayoutCx, PaintCx, part_mut};
use crate::images::{Want, grow, within};
use crate::props;
use crate::tree::{Build, ControlProps, Cx, Handle, wrong_state};
use crate::types::{Dirty, IntoProp, LayoutOptions};

/// How the bitmap is scaled into the box (DrawnUI TransformAspect).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TransformAspect {
    /// Bitmap pixels as they are.
    None,
    /// Enlarged to the box without keeping the aspect; never reduced.
    Fill,
    /// Reduced to the box without keeping the aspect; never enlarged.
    Fit,
    /// Inside the box with the aspect kept; may leave room.
    AspectFit,
    /// Enlarged to cover the box with the aspect kept; never reduced.
    AspectFill,
    /// AspectFill, but reduced to fit when that would stick out.
    AspectFitFill,
    /// Fill, but Fit when that would stick out.
    FitFill,
    /// Stretched to the box on both axes.
    Cover,
    /// Covers the box with the aspect kept; what sticks out is cropped.
    #[default]
    AspectCover,
    /// Not implemented upstream either: draws as `None`.
    Tile,
}

/// Where the scaled bitmap sits inside the box on one axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DrawImageAlignment {
    Start,
    #[default]
    Center,
    End,
}

/// Sampling of the scaled bitmap (DrawnUI FilterQuality). The default is `High`, the sampling
/// React draws every bitmap with (`SkiaImage.ts`: linear between mip levels); upstream's default
/// is `Low`, a linear filter that speckles a photo shrunk past half its size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FilterQuality {
    /// Nearest neighbor.
    None,
    /// Linear.
    Low,
    /// Linear with the nearest mip level.
    Medium,
    /// Linear between mip levels.
    #[default]
    High,
    /// Mitchell cubic when enlarging, `High` when reducing.
    Ultra,
}

/// The one color effect an image has (DrawnUI SkiaImageEffect); its amount is the property of the same name.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SkiaImageEffect {
    #[default]
    None,
    BlackAndWhite,
    Pastel,
    /// `color_tint` blended by `effect_blend_mode`.
    Tint,
    Darken,
    Lighten,
    Grayscale,
    Sepia,
    InvertColors,
    Contrast,
    Saturation,
    Brightness,
    Gamma,
    /// Saturation and lightness (`saturation`, `brightness`), then `background_color` blended in.
    TSL,
    /// Saturation and lightness (`saturation`, `brightness`), then the color at hue `gamma` blended in.
    HSL,
    /// `paint_color_filter` as it is.
    Custom,
}

props!(ImageProps, ImageBuild, ImageSet {
    /// Path or URL, loaded by the host like a font. Empty = no image. Setting another one clears the picture at once.
    source / set_source: String = String::new(), MEASURE;
    aspect / set_aspect: TransformAspect = TransformAspect::AspectCover, MEASURE;
    horizontal_alignment / set_horizontal_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
    vertical_alignment / set_vertical_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
    /// The load starts when the control is placed for drawing, not when it is first measured:
    /// cells a list only measures load nothing.
    load_source_on_first_draw / set_load_source_on_first_draw: bool = false, NONE;
    rescaling_quality / set_rescaling_quality: FilterQuality = FilterQuality::High, DRAW;
    zoom_x / set_zoom_x: f32 = 1.0, DRAW;
    zoom_y / set_zoom_y: f32 = 1.0, DRAW;
    /// Points the picture is moved by inside the box.
    horizontal_offset / set_horizontal_offset: f32 = 0.0, DRAW;
    vertical_offset / set_vertical_offset: f32 = 0.0, DRAW;
    add_effect / set_add_effect: SkiaImageEffect = SkiaImageEffect::None, DRAW_APPLY;
    color_tint / set_color_tint: Color = Color::TRANSPARENT, DRAW_APPLY;
    effect_blend_mode / set_effect_blend_mode: BlendMode = BlendMode::SrcIn, DRAW_APPLY;
    /// Steps of 255 taken off every color channel.
    darken / set_darken: f32 = 5.0, DRAW_APPLY;
    /// Steps of 255 added to every color channel.
    lighten / set_lighten: f32 = 5.0, DRAW_APPLY;
    contrast / set_contrast: f32 = 1.0, DRAW_APPLY;
    brightness / set_brightness: f32 = 1.0, DRAW_APPLY;
    gamma / set_gamma: f32 = 1.0, DRAW_APPLY;
    saturation / set_saturation: f32 = 0.0, DRAW_APPLY;
    /// Blur sigma in points; 0 = off.
    blur / set_blur: f32 = 0.0, DRAW_APPLY;
    /// A color filter of the app's own, used instead of the one of `add_effect` (DrawnUI PaintColorFilter).
    paint_color_filter / set_paint_color_filter: AppFilter<ColorFilter> = AppFilter::default(), DRAW_APPLY;
    /// An image filter of the app's own, used instead of `blur` (DrawnUI PaintImageFilter).
    paint_image_filter / set_paint_image_filter: AppFilter<ImageFilter> = AppFilter::default(), DRAW_APPLY;
});

/// A Skia filter the app made, or none (`paint_color_filter`, `paint_image_filter`): set from
/// the filter or from what Skia's factories return. Skia filters have no equality: each setting
/// is a new value.
#[derive(Clone, Debug)]
pub struct AppFilter<F>(Option<Arc<F>>);

impl<F> AppFilter<F> {
    /// The filter, if one is set.
    pub fn get(&self) -> Option<&F> {
        self.0.as_deref()
    }
}

impl<F> Default for AppFilter<F> {
    fn default() -> Self {
        Self(None)
    }
}

impl<F> PartialEq for AppFilter<F> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        }
    }
}

impl IntoProp<AppFilter<ColorFilter>> for ColorFilter {
    fn into_prop(self) -> AppFilter<ColorFilter> {
        AppFilter(Some(Arc::new(self)))
    }
}

impl IntoProp<AppFilter<ColorFilter>> for Option<ColorFilter> {
    fn into_prop(self) -> AppFilter<ColorFilter> {
        AppFilter(self.map(Arc::new))
    }
}

impl IntoProp<AppFilter<ImageFilter>> for ImageFilter {
    fn into_prop(self) -> AppFilter<ImageFilter> {
        AppFilter(Some(Arc::new(self)))
    }
}

impl IntoProp<AppFilter<ImageFilter>> for Option<ImageFilter> {
    fn into_prop(self) -> AppFilter<ImageFilter> {
        AppFilter(self.map(Arc::new))
    }
}

/// `handler(app state, cx, source)`.
pub(crate) type Loaded = Box<dyn FnMut(&mut dyn Any, &mut Cx<'_>, &str)>;

#[derive(Default)]
pub struct SkiaImage {
    pub p: ImageProps,
    /// The decoded bitmap of `resolved`: a CPU image, shared with the manager's cache. It may
    /// have fewer pixels than the file: it is decoded for the box it is shown in.
    pub(crate) image: Option<Image>,
    /// Pixel size of the file: what the control measures and scales by, whatever was decoded.
    pub(crate) source_size: ISize,
    /// The largest box the manager was asked to cover with `resolved`.
    wanted: Want,
    /// The source the manager was last asked for: `image` and the load in flight belong to it.
    pub(crate) resolved: String,
    pub(crate) loading: bool,
    pub(crate) error: bool,
    /// Anti-aliasing plus the filters of the effect properties: built when they change, and at
    /// paint when the background color or the scale it was built for changed (TSL tints with the
    /// background, blur is in points: React reads both at paint).
    paint: RefCell<Built>,
    /// Measured again when the bitmap arrives, whatever the size requests: a control that builds
    /// something from the bitmap in its measure (the tiles).
    pub(crate) remeasure_on_arrival: bool,
    pub(crate) on_success: Option<Loaded>,
    pub(crate) on_error: Option<Loaded>,
}

/// The paint of a SkiaImage and what it was built for.
#[derive(Default)]
struct Built {
    paint: Paint,
    background: Option<Color>,
    scale: f32,
}

/// The control takes its size from the bitmap on at least one axis (C# NeedAutoSize).
pub(crate) fn auto_sized(p: &ControlProps) -> bool {
    (p.width_request < 0.0 && p.horizontal_options != LayoutOptions::Fill)
        || (p.height_request < 0.0 && p.vertical_options != LayoutOptions::Fill)
}

/// DrawnUI RescaleAspect: the scale per axis that puts a `width` x `height` bitmap into a box.
pub fn rescale_aspect(width: f32, height: f32, dest: Size, aspect: TransformAspect) -> (f32, f32) {
    let (s1, s2) = (dest.width / width, dest.height / height);
    let fit = (if dest.width < width { s1 } else { 1.0 }, if dest.height < height { s2 } else { 1.0 });
    let fill = (if width < dest.width { s1 } else { 1.0 }, if height < dest.height { s2 } else { 1.0 });
    let aspect_fill = if width < dest.width { s1.max(s2) } else { 1.0 };
    let sticks_out = |(x, y): (f32, f32)| width * x > dest.width || height * y > dest.height;
    match aspect {
        TransformAspect::None | TransformAspect::Tile => (1.0, 1.0),
        TransformAspect::Fit => fit,
        TransformAspect::Fill => fill,
        TransformAspect::FitFill => if sticks_out(fill) { fit } else { fill },
        TransformAspect::Cover => (s1, s2),
        TransformAspect::AspectCover => (s1.max(s2), s1.max(s2)),
        TransformAspect::AspectFill => (aspect_fill, aspect_fill),
        TransformAspect::AspectFit => (s1.min(s2), s1.min(s2)),
        TransformAspect::AspectFitFill if sticks_out((aspect_fill, aspect_fill)) => (s1.min(s2), s1.min(s2)),
        TransformAspect::AspectFitFill => (aspect_fill, aspect_fill),
    }
}

/// Where a bitmap of `size` lands in `dest` (DrawnUI CalculateDisplayRect): aspect scale,
/// alignment, then `zoom` around the center of that and `moved` pixels (DrawnUi.React
/// `SkiaImage.Paint`; upstream zooms before it aligns, which differs for Start and End).
pub(crate) fn place(
    size: ISize,
    dest: Rect,
    aspect: TransformAspect,
    horizontal: DrawImageAlignment,
    vertical: DrawImageAlignment,
    zoom: (f32, f32),
    moved: (f32, f32),
) -> Rect {
    let (width, height) = (size.width as f32, size.height as f32);
    let (sx, sy) = rescale_aspect(width, height, dest.size(), aspect);
    let (w, h) = (width * sx, height * sy);
    let along = |alignment: DrawImageAlignment, room: f32| match alignment {
        DrawImageAlignment::Start => 0.0,
        DrawImageAlignment::Center => room / 2.0,
        DrawImageAlignment::End => room,
    };
    // Zoomed around the center of the aligned picture, then moved.
    let (zw, zh) = (w * zoom.0, h * zoom.1);
    let x = dest.left + along(horizontal, dest.width() - w) + (w - zw) / 2.0 + moved.0;
    let y = dest.top + along(vertical, dest.height() - h) + (h - zh) / 2.0 + moved.1;
    Rect::from_xywh(x, y, zw, zh)
}

/// DrawnUI SkiaSamplingOptions.GetSamplingOptions.
fn sampling(quality: FilterQuality, enlarging: bool) -> SamplingOptions {
    match quality {
        FilterQuality::None => SamplingOptions::new(FilterMode::Nearest, MipmapMode::None),
        FilterQuality::Low => SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        FilterQuality::Medium => SamplingOptions::new(FilterMode::Linear, MipmapMode::Nearest),
        FilterQuality::Ultra if enlarging => CubicResampler::mitchell().into(),
        FilterQuality::High | FilterQuality::Ultra => SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear),
    }
}

/// The color at (hue, saturation 1, lightness), each 0..1 (MAUI Color.FromHsla as ImageEffects.ts does it).
fn hsl(hue: f32, lightness: f32) -> Color {
    let (h, l) = (hue.rem_euclid(1.0), lightness.clamp(0.0, 1.0));
    let q = if l < 0.5 { l * 2.0 } else { 1.0 };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round() as u8
    };
    Color::from_rgb(channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
}

/// The color filter of `add_effect`, with the matrices of DrawnUI SkiaImageEffects (row-major
/// 4 x 5). Where upstream's offset is on the wrong scale (Darken, Lighten, Brightness,
/// InvertColors, Contrast) the offset is the one of DrawnUi.React's ImageEffects.ts.
fn color_filter(p: &ImageProps, background: Option<Color>) -> Option<ColorFilter> {
    let rows = |r: [f32; 3], g: [f32; 3], b: [f32; 3], t: f32| {
        [r[0], r[1], r[2], 0.0, t, g[0], g[1], g[2], 0.0, t, b[0], b[1], b[2], 0.0, t, 0.0, 0.0, 0.0, 1.0, 0.0]
    };
    let shift = |by: f32| rows([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], by);
    let saturation = |amount: f32| {
        let rest = 1.0 - amount;
        let (r, g, b) = (0.213 * rest, 0.715 * rest, 0.072 * rest);
        color_filters::matrix_row_major(&rows([r + amount, g, b], [r, g + amount, b], [r, g, b + amount], 0.0), None)
    };
    // ImageEffects.ts `Lightness`: the amount in steps of 255.
    let lightness = |amount: f32| color_filters::matrix_row_major(&shift(amount / 255.0), None);
    // ImageEffects.ts: TSL tints after lightness and saturation, HSL before them.
    let tinted = |tint: Color, tint_first: bool| -> Option<ColorFilter> {
        let blend = color_filters::blend(tint, p.effect_blend_mode)?;
        match tint_first {
            true => color_filters::compose(color_filters::compose(saturation(p.saturation), lightness(p.brightness))?, blend),
            false => color_filters::compose(blend, color_filters::compose(lightness(p.brightness), saturation(p.saturation))?),
        }
    };
    let matrix = match p.add_effect {
        SkiaImageEffect::Tint if p.color_tint != Color::TRANSPARENT => {
            return color_filters::blend(p.color_tint, p.effect_blend_mode);
        }
        // TSL blends the background color in; HSL the color at hue `gamma`, full saturation,
        // lightness `brightness`. Both need a background color, as upstream.
        SkiaImageEffect::TSL if background.is_some_and(|c| c != Color::TRANSPARENT) => {
            return tinted(background?, false);
        }
        SkiaImageEffect::HSL if background.is_some_and(|c| c != Color::TRANSPARENT) => {
            return tinted(hsl(p.gamma, p.brightness), true);
        }
        // The amounts of Darken and Lighten are steps of 255, as the TypeScript engine reads them.
        // Upstream hands them to Skia as they are, where 1 is full white: its own default of 5
        // gives a plain black or white picture.
        SkiaImageEffect::Darken if p.darken != 0.0 => shift(-p.darken / 255.0),
        SkiaImageEffect::Lighten if p.lighten != 0.0 => shift(p.lighten / 255.0),
        SkiaImageEffect::Brightness if p.brightness >= 1.0 => shift(p.brightness),
        SkiaImageEffect::BlackAndWhite | SkiaImageEffect::Grayscale => {
            rows([0.2989, 0.587, 0.114], [0.2989, 0.587, 0.114], [0.2989, 0.587, 0.114], 0.0)
        }
        SkiaImageEffect::Pastel => rows([0.75, 0.25, 0.25], [0.25, 0.75, 0.25], [0.25, 0.25, 0.75], 0.0),
        SkiaImageEffect::Sepia => rows([0.393, 0.769, 0.189], [0.349, 0.686, 0.168], [0.272, 0.534, 0.131], 0.0),
        // Upstream's offset is 255 ("NOT WORKING" by its own comment: a white picture).
        SkiaImageEffect::InvertColors => rows([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0], 1.0),
        // ImageEffects.ts: the offset in steps of 255 (upstream hands 0.5 x (1 - amount) to Skia
        // as it is, where 1 is full white: a quarter darker at 1.5).
        SkiaImageEffect::Contrast if p.contrast >= 1.0 => {
            let c = p.contrast + 1.0;
            rows([c, 0.0, 0.0], [0.0, c, 0.0], [0.0, 0.0, c], 0.5 * (1.0 - p.contrast) / 255.0)
        }
        SkiaImageEffect::Saturation if p.saturation >= 0.0 => {
            let (s, rest) = (p.saturation, 1.0 - p.saturation);
            let (r, g, b) = (0.213 * rest, 0.715 * rest, 0.072 * rest);
            rows([r + s, g, b], [r, g + s, b], [r, g, b + s], 0.0)
        }
        SkiaImageEffect::Gamma if p.gamma >= 0.0 => {
            // Upstream: a value above 1 darkens, one below 1 brightens, by its distance from 1.
            let gamma = if p.gamma < 1.0 { p.gamma + 1.0 } else if p.gamma > 1.0 { p.gamma - 1.0 } else { 1.0 };
            let curve: [u8; 256] = std::array::from_fn(|i| ((i as f32 / 255.0).powf(gamma) * 255.0) as u8);
            return color_filters::table(&curve);
        }
        _ => return None,
    };
    Some(color_filters::matrix_row_major(&matrix, None))
}

impl SkiaImage {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaImage> {
        Build::new(SkiaImage::default()).source(source)
    }

    /// The bitmap on screen (DrawnUI LoadedSource); `None` until the current source arrived.
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref().filter(|_| self.resolved == self.p.source)
    }

    /// Pixel size of the file shown (DrawnUI SourceImageSize). The bitmap itself may have fewer
    /// pixels: it is decoded for the box it is shown in.
    pub fn source_size(&self) -> Option<ISize> {
        self.image().map(|_| self.source_size)
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// The last load of the current source failed. It is tried again when the source is set again.
    pub fn has_error(&self) -> bool {
        self.error
    }

    /// Where the scaled bitmap lands for a drawing rect `dest`, pixels (DrawnUI DisplayRect):
    /// aspect scale, zoom, alignment, then the offsets. `None` until the bitmap is there.
    pub fn display_rect(&self, dest: Rect, scale: f32) -> Option<Rect> {
        self.image()?;
        let (h, v) = (self.p.horizontal_alignment, self.p.vertical_alignment);
        let moved = (scale * self.p.horizontal_offset, scale * self.p.vertical_offset);
        Some(place(self.source_size, dest, self.p.aspect, h, v, (self.p.zoom_x, self.p.zoom_y), moved))
    }

    /// The box the bitmap must cover when the control is `width` x `height` pixels (an unbounded
    /// side does not count). Aspects that show a bitmap larger than the box pixel for pixel need
    /// every pixel of the file.
    fn want(&self, width: f32, height: f32) -> Want {
        use TransformAspect::*;
        if matches!(self.p.aspect, None | Tile | Fill | AspectFill) {
            return (0, 0);
        }
        let zoom = self.p.zoom_x.max(self.p.zoom_y).max(1.0);
        let side = |pixels: f32| if pixels.is_finite() { (pixels * zoom).ceil() as u32 } else { 0 };
        (side(width), side(height))
    }

    /// Anti-aliasing plus the filters of the effect properties. The app's own filters win.
    fn build_paint(&self, background: Option<Color>, scale: f32) -> Paint {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let own_color = self.p.paint_color_filter.get().cloned();
        paint.set_color_filter(own_color.or_else(|| color_filter(&self.p, background)));
        let sigma = self.p.blur * scale;
        let blur = || image_filters::blur((sigma, sigma), TileMode::Mirror, None, None);
        let own_image = self.p.paint_image_filter.get().cloned();
        paint.set_image_filter(own_image.or_else(|| (sigma > 0.0).then(blur).flatten()));
        paint
    }

    /// Asks the manager for the current source in a box of `width` x `height` pixels: once per
    /// source, and again when the box outgrew what was asked for. The bitmap of the previous
    /// source goes at once: a recycled cell never shows the picture of its last item. True when
    /// the source was another one.
    // ponytail: every growth is a load (a window dragged larger loads per size it passes, one
    // at a time, off the frame). Round the box up to steps when that shows.
    fn resolve(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> bool {
        let want = self.want(width, height);
        let changed = self.resolved != self.p.source;
        if !changed && within(want, self.wanted) {
            return false;
        }
        let images = &mut cx.tree.images;
        if changed {
            images.release(&self.resolved, cx.id);
            self.resolved.clone_from(&self.p.source);
            (self.image, self.wanted, self.error) = (Option::None, want, false);
        } else {
            self.wanted = grow(self.wanted, want);
        }
        if !self.resolved.is_empty()
            && let Some((image, source_size)) = images.request(&self.resolved, cx.id, self.wanted)
        {
            // Cached: success is reported in this frame, before the first paint.
            if self.image.is_none() && self.on_success.is_some() {
                images.events.push((cx.id, self.resolved.clone(), true));
            }
            (self.image, self.source_size) = (Some(image), source_size);
        }
        self.loading = self.image.is_none() && !self.resolved.is_empty();
        changed
    }
}

impl Has<ImageProps> for SkiaImage {
    fn part(&self) -> &ImageProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ImageProps {
        &mut self.p
    }
}

impl Control for SkiaImage {
    /// C# OnMeasuring. Without a bitmap: the box it is offered. With one: an unbounded side
    /// follows the bounded one by the bitmap's aspect, then an auto side is the bitmap's pixels
    /// times the aspect scale for that box, never more than the box. Padding is not applied.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        if !self.p.load_source_on_first_draw {
            self.resolve(cx, width, height);
        }
        let bounded = |side: f32| if side.is_finite() { side } else { 0.0 };
        if self.image().is_none() {
            return Size::new(bounded(width), bounded(height));
        }
        let (iw, ih) = (self.source_size.width as f32, self.source_size.height as f32);
        let (w, h) = match (width.is_finite(), height.is_finite()) {
            (true, false) => (width, width * ih / iw),
            (false, true) => (height * iw / ih, height),
            _ => (width, height),
        };
        if !w.is_finite() || iw <= 0.0 || ih <= 0.0 {
            return Size::default();
        }
        let (sx, sy) = rescale_aspect(iw, ih, Size::new(w, h), self.p.aspect);
        Size::new((iw * sx).min(w), (ih * sy).min(h))
    }

    /// The final box is known: a bitmap decoded for a smaller one is asked for again. With
    /// `load_source_on_first_draw` the load starts here: the control is placed, so it will be
    /// drawn. A cached bitmap is there for this frame's paint; an auto-sized control takes its
    /// size on the next frame.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        let rect = cx.base().rect;
        if self.resolve(cx, rect.width(), rect.height()) && self.image.is_some() && auto_sized(&cx.base().p) {
            cx.tree.invalidate(cx.id, Dirty::MEASURE);
        }
    }

    fn on_props_changed(&mut self, _cx: &mut Cx) {
        let Built { background, scale, .. } = *self.paint.get_mut();
        self.paint.get_mut().paint = self.build_paint(background, scale);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let (Some(image), Some(display)) = (self.image(), self.display_rect(cx.rect, cx.scale)) else { return };
        let background = cx.node(cx.id).and_then(|node| node.base.p.background_color);
        let stale = {
            let built = self.paint.borrow();
            (built.background, built.scale) != (background, cx.scale)
        };
        if stale {
            let paint = self.build_paint(background, cx.scale);
            *self.paint.borrow_mut() = Built { paint, background, scale: cx.scale };
        }
        let built = self.paint.borrow();
        let enlarging = display.width() > image.width() as f32 || display.height() > image.height() as f32;
        // What sticks out of the box is cropped (C# ClipSource); a filter spreads past its picture.
        let clip = self.p.blur > 0.0 || self.p.paint_image_filter.get().is_some() || !cx.rect.contains(display);
        if clip {
            cx.canvas.save();
            cx.canvas.clip_rect(cx.rect, ClipOp::Intersect, true);
        }
        cx.canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            display,
            sampling(self.p.rescaling_quality, enlarging),
            &built.paint,
        );
        if clip {
            cx.canvas.restore();
        }
    }
}

fn image_part<T: Control>(control: &mut T) -> &mut SkiaImage {
    part_mut(control).expect("the control embeds a SkiaImage")
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

impl<T: Has<ImageProps>> Build<T> {
    /// Runs when the bitmap of the current source is there (DrawnUI Success), before it is first
    /// painted: in the frame the source was set for a cached one, else when it arrives.
    // ponytail: `me` is a handle, as in the scroll's handlers: a typed `Mut` can only be made in tree.rs.
    pub fn on_success<S: Any>(mut self, f: impl FnMut(Handle<T>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        image_part(self.control_mut()).on_success = Some(handler(me, f));
        self
    }

    /// Runs when the current source could not be loaded or decoded (DrawnUI Error).
    pub fn on_error<S: Any>(mut self, f: impl FnMut(Handle<T>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        let me = self.handle();
        image_part(self.control_mut()).on_error = Some(handler(me, f));
        self
    }
}

// ---------------------------------------------------------------- tiles

props!(TilesProps, TilesBuild, TilesSet {
    /// How the picture fits one tile.
    tile_aspect / set_tile_aspect: TransformAspect = TransformAspect::AspectCover, MEASURE;
    /// Points; nothing is drawn until both are set.
    tile_width / set_tile_width: f32 = 0.0, MEASURE;
    tile_height / set_tile_height: f32 = 0.0, MEASURE;
    /// Points the grid of tiles is shifted by; it wraps around, so animating it scrolls the pattern.
    tile_offset_x / set_tile_offset_x: f32 = 0.0, DRAW;
    tile_offset_y / set_tile_offset_y: f32 = 0.0, DRAW;
});

/// SkiaImageTiles (React `SkiaImageTiles.ts`): the picture, fitted into one tile of
/// `tile_width` x `tile_height` points by `tile_aspect`, repeated over the box and shifted by the
/// offsets. The tile is rasterized once when the picture or the tile changes; every paint blits it.
pub struct SkiaImageTiles {
    image: SkiaImage,
    pub p: TilesProps,
    /// The rasterized tile and what it was made of: the bitmap, the tile in pixels, the aspect.
    tile: Option<(Image, (u32, i32, i32, TransformAspect))>,
}

impl SkiaImageTiles {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaImageTiles> {
        let image = SkiaImage { remeasure_on_arrival: true, ..SkiaImage::default() };
        Build::new(SkiaImageTiles { image, p: TilesProps::default(), tile: None }).source(source).is_clipped_to_bounds(true)
    }

    /// Rasterizes the tile when the picture or the tile changed (React `SetupTiles`).
    fn setup(&mut self, scale: f32) {
        let (tw, th) = ((self.p.tile_width * scale).round() as i32, (self.p.tile_height * scale).round() as i32);
        let Some(bitmap) = self.image.image().filter(|_| tw > 0 && th > 0) else {
            self.tile = None;
            return;
        };
        let key = (bitmap.unique_id(), tw, th, self.p.tile_aspect);
        if self.tile.as_ref().is_some_and(|(_, made)| *made == key) {
            return;
        }
        let Some(mut surface) = surfaces::raster(&ImageInfo::new_n32_premul((tw, th), None), None, None) else { return };
        let dest = Rect::from_iwh(tw, th);
        let (center, one) = (DrawImageAlignment::Center, (1.0, 1.0));
        let display = place(self.image.source_size, dest, self.p.tile_aspect, center, center, one, (0.0, 0.0));
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        surface.canvas().clip_rect(dest, ClipOp::Intersect, true);
        surface.canvas().draw_image_rect_with_sampling_options(bitmap, None, display, sampling, &paint);
        self.tile = Some((surface.image_snapshot(), key));
    }
}

impl Has<TilesProps> for SkiaImageTiles {
    fn part(&self) -> &TilesProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut TilesProps {
        &mut self.p
    }
}

impl Has<ImageProps> for SkiaImageTiles {
    fn part(&self) -> &ImageProps {
        &self.image.p
    }
    fn part_mut(&mut self) -> &mut ImageProps {
        &mut self.image.p
    }
}

impl Control for SkiaImageTiles {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.image)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.image)
    }

    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let size = self.image.measure(cx, width, height);
        self.setup(cx.scale);
        size
    }

    /// React `Paint`: the offsets wrap inside one tile; a shifted grid starts one tile before the box.
    fn paint(&self, cx: &mut PaintCx) {
        let Some((tile, (_, tw, th, _))) = &self.tile else { return };
        let (dest, scale) = (cx.rect, cx.scale);
        let (tile_w, tile_h) = (self.p.tile_width, self.p.tile_height);
        let (use_x, use_y) = (-self.p.tile_offset_x % tile_w, -self.p.tile_offset_y % tile_h);
        let offset_x = if use_x > 0.0 { (use_x * scale).round() } else { *tw as f32 + (use_x * scale).round() };
        let offset_y = if use_y > 0.0 { (use_y * scale).round() } else { *th as f32 + (use_y * scale).round() };
        let (tw, th) = (*tw as f32, *th as f32);
        let tiles_x = ((dest.width() + offset_x) / tw).ceil() as i32;
        let tiles_y = ((dest.height() + offset_y.abs()) / th).ceil() as i32;
        let (start_x, start_y) = (dest.left - offset_x, dest.top - offset_y);
        let sampling = SamplingOptions::new(FilterMode::Nearest, MipmapMode::None);
        for x in 0..tiles_x {
            for y in 0..tiles_y {
                let at = (start_x + x as f32 * tw, start_y + y as f32 * th);
                cx.canvas.draw_image_with_sampling_options(tile, at, sampling, None);
            }
        }
    }
}
