//! SkiaSvg: an SVG from a file (`source`, loaded through the tree's asset channel) or inline
//! markup (`svg_string`), drawn with Skia's SVG module (cargo feature `svg`). As in DrawnUi.React it
//! is rasterized at the pixel size it is displayed at, once per size, and blitted from then on; C#
//! replays the vector picture on every paint. `tint_color` recolors it (SrcIn).
//!
//! A file is loaded once however many controls show it: a control finds the picture, or the load
//! in flight, on the other controls showing that file; the arriving bytes go to every control
//! still waiting for them. Nothing is kept once no control shows the file.
// ponytail: no Success / Error handlers (the demo uses none), no font for text inside an SVG on
// the web (Skia's default font manager is empty there).

use skia_safe::{
    BlendMode, ClipOp, Color, Contains, FilterMode, ISize, Image, ImageInfo, MipmapMode, Paint, Rect, SamplingOptions,
    Size, color_filters, surfaces,
    svg::{
        Dom, Length, LengthUnit, PreserveAspectRatio,
        preserve_aspect_ratio::{Align, Scale},
    },
};

use crate::control::{Control, Has, LayoutCx, PaintCx, part, part_mut};
use crate::controls::image::{DrawImageAlignment, TransformAspect, rescale_aspect};
use crate::controls::sprite::picture_size;
use crate::props;
use crate::tree::{Build, Cx, Tree};
use crate::types::{CacheType, Dirty, IntoProp};

props!(SvgProps, SvgBuild, SvgSet {
    /// Path or URL of an .svg file, loaded by the host. `svg_string` wins when both are set.
    source / set_source: String = String::new(), MEASURE_APPLY;
    /// Inline SVG markup.
    svg_string / set_svg_string: String = String::new(), MEASURE_APPLY;
    /// Recolors the picture; transparent keeps its own colors.
    tint_color / set_tint_color: Color = Color::TRANSPARENT, DRAW_APPLY;
    /// How the picture fits the box, as SkiaImage.
    aspect / set_aspect: TransformAspect = TransformAspect::AspectFitFill, DRAW;
    /// Where the fitted picture sits across the box.
    horizontal_alignment / set_horizontal_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
    /// Where the fitted picture sits down the box.
    vertical_alignment / set_vertical_alignment: DrawImageAlignment = DrawImageAlignment::Center, DRAW;
});

/// Draws an SVG (DrawnUI SkiaSvg).
#[derive(Default)]
pub struct SkiaSvg {
    /// Its own properties.
    pub p: SvgProps,
    /// The parsed picture; controls showing the same file share it.
    dom: Option<Dom>,
    /// The picture's own size: its viewBox, else its width and height.
    intrinsic: Size,
    /// The file or markup the picture (or the load in flight) belongs to.
    resolved: String,
    /// The picture at the pixel size it was last displayed at.
    raster: Option<(ISize, Image)>,
    loading: bool,
    error: bool,
    /// Anti-aliased, with the tint.
    paint: Paint,
}

impl SkiaSvg {
    /// An SVG file, loaded by the host. Cached as Operations, as upstream.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: impl IntoProp<String>) -> Build<SkiaSvg> {
        Build::new(SkiaSvg::default()).source(source).use_cache(CacheType::Operations)
    }

    /// Inline markup instead of a file.
    pub fn from_string(svg: impl IntoProp<String>) -> Build<SkiaSvg> {
        Build::new(SkiaSvg::default()).svg_string(svg).use_cache(CacheType::Operations)
    }

    /// The file is on its way.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// The file could not be loaded or the markup not parsed.
    pub fn has_error(&self) -> bool {
        self.error
    }

    /// The picture's own size in its units (its viewBox, else its width and height); zero
    /// until it is there.
    pub fn intrinsic_size(&self) -> Size {
        self.intrinsic
    }

    /// The picture arrived (`None` = the load or the parse failed).
    fn take(&mut self, dom: Option<Dom>) {
        self.intrinsic = dom.as_ref().map_or(Size::default(), fill_container);
        (self.loading, self.error, self.raster) = (false, dom.is_none(), None);
        self.dom = dom;
    }

    /// Whether this control waits for the file `url`.
    fn waits_for(&self, url: &str) -> bool {
        self.p.svg_string.is_empty() && self.resolved == url
    }

    /// Where the picture lands inside `dest` (DrawnUI DisplayRect): aspect scale, then alignment.
    fn display_rect(&self, dest: Rect) -> Rect {
        let (width, height) = (self.intrinsic.width, self.intrinsic.height);
        let (sx, sy) = rescale_aspect(width, height, dest.size(), self.p.aspect);
        let (w, h) = (width * sx, height * sy);
        let along = |alignment: DrawImageAlignment, room: f32| match alignment {
            DrawImageAlignment::Start => 0.0,
            DrawImageAlignment::Center => room / 2.0,
            DrawImageAlignment::End => room,
        };
        let x = dest.left + along(self.p.horizontal_alignment, dest.width() - w);
        let y = dest.top + along(self.p.vertical_alignment, dest.height() - h);
        Rect::from_xywh(x, y, w, h)
    }
}

/// The picture's own size (its viewBox first: a file without width and height still has one,
/// else its width and height), and its root made to fill whatever container it is rendered into:
/// 100% x 100%, a viewBox from that size when it has none, stretched to the container (the display
/// rect has the picture's aspect already; Fill stretches it, as the browser does in React). Root
/// sizes such as `width="800px"` would otherwise win over the container. Idempotent: controls
/// showing the same file share the DOM.
fn fill_container(dom: &Dom) -> Size {
    let mut root = dom.root();
    let size = match root.view_box() {
        Some(view_box) if view_box.width() > 0.0 && view_box.height() > 0.0 => view_box.size(),
        _ => root.intrinsic_size(),
    };
    if size.width > 0.0 && size.height > 0.0 {
        if root.view_box().is_none() {
            root.set_view_box(Rect::from_size(size));
        }
        root.set_width(Length::new(100.0, LengthUnit::Percentage));
        root.set_height(Length::new(100.0, LengthUnit::Percentage));
        root.set_preserve_aspect_ratio(PreserveAspectRatio::new(Align::None, Scale::Meet));
    }
    size
}

/// The picture of `url` held by another control, and whether a load of it is in flight.
fn shared(tree: &Tree, url: &str) -> (Option<Dom>, bool) {
    let mut flying = false;
    for other in tree.nodes.iter().flatten().filter_map(|node| part::<SkiaSvg>(node.kind.as_deref()?)) {
        if other.waits_for(url) {
            if other.dom.is_some() {
                return (other.dom.clone(), false);
            }
            flying |= other.loading;
        }
    }
    (None, flying)
}

/// The bytes of `url` arrived (empty = failed): every control still waiting for them parses the
/// one picture and measures again.
fn deliver(tree: &mut Tree, url: &str, bytes: &[u8]) {
    let dom = (!bytes.is_empty()).then(|| Dom::from_bytes(bytes, crate::fonts::font_mgr()).ok()).flatten();
    if dom.is_none() {
        eprintln!("drawnui: svg {url} did not load");
    }
    let mut arrived = Vec::new();
    for node in tree.nodes.iter_mut().flatten() {
        let Some(svg) = node.kind.as_deref_mut().and_then(part_mut::<SkiaSvg>) else { continue };
        if svg.loading && svg.waits_for(url) {
            svg.take(dom.clone());
            arrived.push(node.id);
        }
    }
    for id in arrived {
        tree.invalidate(id, Dirty::MEASURE);
    }
}

impl Has<SvgProps> for SkiaSvg {
    fn part(&self) -> &SvgProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut SvgProps {
        &mut self.p
    }
}

impl Control for SkiaSvg {
    /// The tint goes into the paint; markup is parsed here, a file is taken from another control
    /// showing it, waits for the load in flight, or is asked for.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.paint.set_anti_alias(true);
        let tint = self.p.tint_color;
        self.paint.set_color_filter((tint.a() > 0).then(|| color_filters::blend(tint, BlendMode::SrcIn)).flatten());
        let wanted = if self.p.svg_string.is_empty() { &self.p.source } else { &self.p.svg_string };
        if self.resolved == *wanted {
            return;
        }
        self.resolved.clone_from(wanted);
        if !self.p.svg_string.is_empty() {
            let dom = Dom::from_bytes(self.p.svg_string.as_bytes(), crate::fonts::font_mgr()).ok();
            return self.take(dom);
        }
        self.take(None);
        self.error = false;
        if self.resolved.is_empty() {
            return;
        }
        let (dom, flying) = shared(cx.tree, &self.resolved);
        if dom.is_some() {
            return self.take(dom);
        }
        self.loading = true;
        if !flying {
            let url = self.resolved.clone();
            cx.tree.assets.fetch(&self.resolved, move |tree, bytes| deliver(tree, &url, &bytes));
        }
    }

    /// Sized by the picture's aspect (`sprite::picture_size`).
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let (w, h) = (self.intrinsic.width, self.intrinsic.height);
        let aspect = if w > 0.0 && h > 0.0 { w / h } else { 0.0 };
        picture_size(&cx.base().p, aspect, width, height)
    }

    /// The rect is known: the picture is rasterized at the size it will show at, when that changed.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        if self.dom.is_none() || self.intrinsic.width <= 0.0 || self.intrinsic.height <= 0.0 {
            return;
        }
        let display = self.display_rect(cx.base().rect);
        let size = ISize::new(display.width().round() as i32, display.height().round() as i32);
        if size.width < 1 || size.height < 1 || self.raster.as_ref().is_some_and(|(have, _)| *have == size) {
            return;
        }
        let Some(mut surface) = surfaces::raster(&ImageInfo::new_n32_premul(size, None), None, None) else { return };
        let Some(dom) = self.dom.as_mut() else { return };
        dom.set_container_size(Size::new(size.width as f32, size.height as f32));
        dom.render(surface.canvas());
        self.raster = Some((size, surface.image_snapshot()));
    }

    fn paint(&self, cx: &mut PaintCx) {
        let Some((_, raster)) = &self.raster else { return };
        let display = self.display_rect(cx.rect);
        // What sticks out of the box is cropped.
        let clip = !cx.rect.contains(display);
        if clip {
            cx.canvas.save();
            cx.canvas.clip_rect(cx.rect, ClipOp::Intersect, true);
        }
        let (w, h) = (raster.width() as f32, raster.height() as f32);
        if (display.width() - w).abs() < 0.5 && (display.height() - h).abs() < 0.5 {
            // The raster has the displayed size: a plain blit on whole pixels, crisp and about 9
            // times cheaper on the CPU canvas than a filtered one (five logos: 2.8 ms to 0.3 ms).
            cx.canvas.draw_image(raster, (display.left.round(), display.top.round()), Some(&self.paint));
        } else {
            let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
            cx.canvas.draw_image_rect_with_sampling_options(raster, None, display, sampling, &self.paint);
        }
        if clip {
            cx.canvas.restore();
        }
    }
}
