//! SkiaShape: a layout that draws an outline (shadows, fill, stroke) and clips its children to it.
//! Same geometry as C# SkiaShape: the outline is inset so that the stroke stays inside the box,
//! the fill and the children clip sit a third of the stroke further in, the stroke goes on top.
// ponytail: no SmoothPoints, Squircle (upstream draws none either).

use skia_safe::{
    BlendMode, Canvas, ClipOp, Color, ContourMeasureIter, ImageFilter, Matrix, Paint, PaintCap, PaintJoin, PaintStyle,
    Path, PathBuilder, PathEffect, Point, RRect, Rect, Size, Vector, matrix::ScaleToFit,
};

use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::tree::{Build, Container, Cx};
use crate::types::{CacheType, CornerRadius, IntoProp, SkiaGradient, SkiaShadow, Thickness};
use crate::{paint, props};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ShapeType {
    #[default]
    Rectangle,
    /// The largest circle that fits, centered.
    Circle,
    Ellipse,
    /// A stroked piece of the ellipse: from `value1` degrees, over `value2` degrees, clockwise from 3 o'clock.
    Arc,
    /// `path_data`, stretched over the box.
    Path,
    /// `points`, closed.
    Polygon,
    /// `points`, stroked.
    Line,
}

/// Raised or pressed-in edges of a shape (DrawnUI BevelType).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BevelType {
    #[default]
    None,
    /// Raised: light on the top and left edges, shadow on the bottom and right ones.
    Bevel,
    /// Pressed in: shadow on the top and left edges, light on the bottom and right ones.
    Emboss,
}

/// The edges of a bevel or emboss (DrawnUI SkiaBevel): `SkiaBevel::new(4).opacity(0.7)`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SkiaBevel {
    /// Width of the edges, points.
    pub depth: f32,
    /// Color of the lit edges.
    pub light_color: Color,
    /// Color of the shaded edges.
    pub shadow_color: Color,
    /// Alpha of both colors, replacing their own.
    pub opacity: f32,
}

impl Default for SkiaBevel {
    fn default() -> Self {
        Self { depth: 2.0, light_color: Color::WHITE, shadow_color: Color::BLACK, opacity: 0.5 }
    }
}

impl SkiaBevel {
    /// Edges `depth` points wide, white and black at opacity 0.5 (the upstream defaults).
    pub fn new(depth: impl IntoProp<f32>) -> Self {
        Self { depth: depth.into_prop(), ..Self::default() }
    }

    /// Sets the color of the lit edges.
    pub fn light_color(mut self, color: Color) -> Self {
        self.light_color = color;
        self
    }

    /// Sets the color of the shaded edges.
    pub fn shadow_color(mut self, color: Color) -> Self {
        self.shadow_color = color;
        self
    }

    /// Sets the alpha of both colors.
    pub fn opacity(mut self, opacity: impl IntoProp<f32>) -> Self {
        self.opacity = opacity.into_prop();
        self
    }
}

impl IntoProp<Option<SkiaBevel>> for SkiaBevel {
    fn into_prop(self) -> Option<SkiaBevel> {
        Some(self)
    }
}

props!(ShapeProps, ShapeBuild, ShapeSet {
    /// DrawnUI `Type`.
    shape_type / set_shape_type: ShapeType = ShapeType::Rectangle, DRAW_APPLY;
    /// Points, per corner; a number sets all four.
    corner_radius / set_corner_radius: CornerRadius = CornerRadius::uniform(0.0), DRAW;
    /// A transparent color means no stroke.
    stroke_color / set_stroke_color: Color = Color::from_rgb(0x80, 0x80, 0x80), DRAW;
    /// Points; a negative value is pixels. Children are laid out inside the stroke. On rounded
    /// rectangles and curves a stroke of 1 point or less is drawn at 0.55 of its width.
    stroke_width / set_stroke_width: f32 = 0.0, MEASURE;
    /// Line ends; also picks the join: Round -> Round, Square -> Bevel, Butt -> Miter.
    stroke_cap / set_stroke_cap: PaintCap = PaintCap::Round, DRAW;
    stroke_blend_mode / set_stroke_blend_mode: BlendMode = BlendMode::SrcOver, DRAW;
    /// Painted along the stroke instead of `stroke_color`, whose alpha still applies.
    stroke_gradient / set_stroke_gradient: Option<Box<SkiaGradient>> = None, DRAW;
    /// Dashes in points: on, off, on, off... (DrawnUI StrokePath). Empty = a solid line.
    stroke_path / set_stroke_path: Vec<f32> = Vec::new(), DRAW_APPLY;
    /// Drawn with the fill, so a shape without a background has none. A shadow is a blur: it
    /// runs on every paint, also when an Operations cache is replayed. `CacheType::Image` keeps
    /// the result.
    shadows / set_shadows: Vec<SkiaShadow> = Vec::new(), DRAW_APPLY;
    /// A shape with shadows keeps only what falls outside its outline: the shadows.
    clip_background_color / set_clip_background_color: bool = false, DRAW;
    /// Polygon and Line: the corners as ratios of the box, (0, 0) = top left, (1, 1) = bottom right.
    points / set_points: Vec<Point> = Vec::new(), DRAW_APPLY;
    /// Path: SVG path data.
    path_data / set_path_data: String = String::new(), DRAW_APPLY;
    /// Degrees: where an Arc and a Sweep gradient start.
    value1 / set_value1: f32 = 0.0, DRAW;
    /// Degrees: how far an Arc and a Sweep gradient go.
    value2 / set_value2: f32 = 0.0, DRAW;
    /// Raised or pressed-in edges over the fill (Rectangle, Circle, Ellipse, Path, Polygon); drawn
    /// only with a `bevel`, as upstream.
    bevel_type / set_bevel_type: BevelType = BevelType::None, DRAW_APPLY;
    /// Depth, colors and opacity of the edges.
    bevel / set_bevel: Option<SkiaBevel> = None, DRAW_APPLY;
});

/// The rects of a shape inside a drawing rect, pixels (C# CalculateSizeForStroke).
struct Geometry {
    /// Where the outline runs: the rect, inset so that the stroke stays inside it.
    outline: Rect,
    /// What the fill covers and the children are clipped to: a third of the stroke further in.
    inner: Rect,
    /// Stroke width in pixels; 0 = no stroke.
    stroke: f32,
}

/// What a shape's paint reuses between frames. Built again when a property it is made from, the
/// outline rect or the scale changes.
pub(crate) struct ShapeCache {
    /// (the shape's version, outline rect, scale)
    key: (u32, Rect, f32),
    /// The outline of a Path, Polygon or Line.
    path: Path,
    /// One drop-shadow filter per shadow.
    shadows: Vec<Option<ImageFilter>>,
    /// The dashes of `stroke_path`.
    dash: Option<PathEffect>,
    /// The top / left and the bottom / right edges of a bevel.
    bevel: Option<(Path, Path)>,
}

#[derive(Default)]
pub struct SkiaShape {
    layout: SkiaLayout,
    pub p: ShapeProps,
    /// Grows when a property the cached paint objects are made from changes.
    version: u32,
}

impl SkiaShape {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaShape> {
        Build::new(SkiaShape::default()).use_cache(CacheType::Operations)
    }

    /// Stroke width in pixels, 0 when nothing is stroked.
    fn stroke_pixels(&self, scale: f32) -> f32 {
        let width = self.p.stroke_width;
        if self.p.stroke_color.a() == 0 {
            0.0
        } else if width > 0.0 {
            width * scale
        } else {
            -width
        }
    }

    /// Pixels the outline sits inside the rect so that the stroke never leaves it: half the
    /// stroke on whole pixels, at least 1 (C# GetInflationForStroke).
    fn stroke_inset(&self, scale: f32) -> f32 {
        let half = self.stroke_pixels(scale) / 2.0;
        if half == 0.0 { 0.0 } else { half.ceil().max(1.0) }
    }

    fn geometry(&self, rect: Rect, scale: f32) -> Geometry {
        let stroke = self.stroke_pixels(scale);
        if stroke == 0.0 {
            return Geometry { outline: rect, inner: rect, stroke };
        }
        let inset = self.stroke_inset(scale);
        let (left, top) = ((rect.left + inset).ceil(), (rect.top + inset).ceil());
        let outline = Rect::new(left, top, (rect.right - inset).floor(), (rect.bottom - inset).floor());
        Geometry { outline, inner: outline.with_inset((stroke / 3.0, stroke / 3.0)), stroke }
    }

    /// Corner radii in pixels, each smaller by `by`: top left, top right, bottom right, bottom left.
    fn radii(&self, scale: f32, by: f32) -> [Vector; 4] {
        let r = &self.p.corner_radius;
        [r.top_left, r.top_right, r.bottom_right, r.bottom_left].map(|radius| {
            let radius = (radius * scale - by).max(0.0);
            Vector::new(radius, radius)
        })
    }

    /// The circle the children of a Circle are clipped to: whole pixels, as upstream.
    fn inner_circle(inner: Rect) -> Rect {
        let radius = (inner.width().min(inner.height()) / 2.0).floor();
        Rect::from_xywh(inner.center_x() - radius, inner.center_y() - radius, radius * 2.0, radius * 2.0)
    }

    /// The outline of a Path, Polygon or Line over `rect`.
    fn outline_path(&self, rect: Rect) -> Path {
        if self.p.shape_type == ShapeType::Path {
            // The path's own box is stretched over the rect.
            let Some(path) = Path::from_svg(&self.p.path_data) else { return Path::default() };
            return match Matrix::rect_2_rect(path.compute_tight_bounds(), rect, ScaleToFit::Fill) {
                Some(matrix) => path.with_transform(&matrix),
                None => Path::default(),
            };
        }
        let points = &self.p.points;
        if points.len() < 2 {
            return Path::default();
        }
        // Corners land on whole pixels (C# ScalePoint).
        let round = f32::round_ties_even;
        let at = |p: &Point| Point::new(round(rect.left + p.x * rect.width()), round(rect.top + p.y * rect.height()));
        let mut builder = PathBuilder::new();
        builder.move_to(at(&points[0]));
        for point in &points[1..] {
            builder.line_to(at(point));
        }
        if self.p.shape_type == ShapeType::Polygon {
            builder.close();
        }
        builder.detach()
    }

    /// Takes the shape's paint objects out of its render slot, built again when stale; `keep`
    /// puts them back. `None` for a shape that needs none: no path, shadow or dash.
    fn take_cache(&self, cx: &mut PaintCx, outline: Rect) -> Option<ShapeCache> {
        let p = &self.p;
        let has_path = matches!(p.shape_type, ShapeType::Path | ShapeType::Polygon | ShapeType::Line);
        let bevel = p.bevel.filter(|_| p.bevel_type != BevelType::None);
        if !has_path && p.shadows.is_empty() && p.stroke_path.is_empty() && bevel.is_none() {
            return None;
        }
        let (scale, key) = (cx.scale, (self.version, outline, cx.scale));
        match cx.render[cx.id.index as usize].paints.as_mut().and_then(|paints| paints.shape.take()) {
            Some(cache) if cache.key == key => Some(cache),
            _ => {
                let path = if has_path { self.outline_path(outline) } else { Path::default() };
                Some(ShapeCache {
                    key,
                    bevel: bevel.map(|bevel| self.bevel_paths(outline, scale, bevel.depth * scale, &path)),
                    path,
                    shadows: p.shadows.iter().map(|shadow| paint::create_shadow(shadow, scale)).collect(),
                    dash: {
                        let intervals: Vec<f32> = p.stroke_path.iter().map(|v| (v * scale).round_ties_even()).collect();
                        PathEffect::dash(&intervals, 0.0)
                    },
                })
            }
        }
    }

    /// The edges of a bevel (C# PaintBevelEffect): strokes `depth` pixels wide, half a depth
    /// inside the outline, the top and left ones and the bottom and right ones. A rounded
    /// rectangle splits its top right and bottom left corners at 45 degrees, a circle or an
    /// ellipse is split at 135 and 315 degrees; a path gives the first half of its first contour
    /// to the light and the rest to the shadow; a polygon's edge is light unless its middle lies
    /// right of and below the center.
    fn bevel_paths(&self, rect: Rect, scale: f32, depth: f32, outline: &Path) -> (Path, Path) {
        let (mut tl, mut br) = (PathBuilder::new(), PathBuilder::new());
        let half = depth / 2.0;
        let ltrb = Rect::new;
        match self.p.shape_type {
            ShapeType::Rectangle if self.p.corner_radius.is_zero() => {
                tl.move_to((rect.left, rect.top + half)).line_to((rect.right, rect.top + half));
                tl.move_to((rect.left + half, rect.top)).line_to((rect.left + half, rect.bottom));
                br.move_to((rect.left, rect.bottom - half)).line_to((rect.right, rect.bottom - half));
                br.move_to((rect.right - half, rect.top)).line_to((rect.right - half, rect.bottom));
            }
            ShapeType::Rectangle => {
                let [a, b, c, d] = self.radii(scale, 0.0);
                let (r_tl, r_tr, r_br, r_bl) = (a.x, b.x, c.x, d.x);
                let (l, t, r, b) = (rect.left, rect.top, rect.right, rect.bottom);
                let top_right = ltrb(r - 2.0 * r_tr + half, t + half, r - half, t + 2.0 * r_tr - half);
                let bottom_left = ltrb(l + half, b - 2.0 * r_bl + half, l + 2.0 * r_bl - half, b - half);
                if r_tl > half {
                    tl.add_arc(ltrb(l + half, t + half, l + 2.0 * r_tl - half, t + 2.0 * r_tl - half), 180.0, 90.0);
                }
                tl.move_to((l + r_tl, t + half)).line_to((r - r_tr, t + half));
                if r_tr > half {
                    tl.add_arc(top_right, 270.0, 45.0);
                    br.add_arc(top_right, 315.0, 45.0);
                }
                if r_bl > half {
                    tl.add_arc(bottom_left, 135.0, 45.0);
                    br.add_arc(bottom_left, 90.0, 45.0);
                }
                tl.move_to((l + half, t + r_tl)).line_to((l + half, b - r_bl));
                br.move_to((r - half, t + r_tr)).line_to((r - half, b - r_br));
                if r_br > half {
                    br.add_arc(ltrb(r - 2.0 * r_br + half, b - 2.0 * r_br + half, r - half, b - half), 0.0, 90.0);
                }
                br.move_to((l + r_bl, b - half)).line_to((r - r_br, b - half));
            }
            ShapeType::Circle | ShapeType::Ellipse => {
                // A circle is the largest one that fits, centered (upstream strokes the rect's ellipse).
                let oval = if self.p.shape_type == ShapeType::Circle {
                    let d = rect.width().min(rect.height());
                    Rect::from_xywh(rect.center_x() - d / 2.0, rect.center_y() - d / 2.0, d, d)
                } else {
                    rect
                };
                let oval = oval.with_inset((half, half));
                tl.add_arc(oval, 135.0, 180.0);
                br.add_arc(oval, 315.0, 180.0);
            }
            ShapeType::Path => {
                if let Some(contour) = ContourMeasureIter::new(outline, false, None).next() {
                    let length = contour.length();
                    let _ = contour.get_segment(0.0, length / 2.0, &mut tl, true);
                    let _ = contour.get_segment(length / 2.0, length, &mut br, true);
                }
            }
            ShapeType::Polygon if self.p.points.len() >= 3 => {
                let round = f32::round_ties_even;
                let at = |p: &Point| Point::new(round(rect.left + p.x * rect.width()), round(rect.top + p.y * rect.height()));
                let (points, center) = (&self.p.points, rect.center());
                for (i, point) in points.iter().enumerate() {
                    let (from, to) = (at(point), at(&points[(i + 1) % points.len()]));
                    let middle = Point::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
                    let edge = if middle.x > center.x && middle.y > center.y { &mut br } else { &mut tl };
                    edge.move_to(from).line_to(to);
                }
            }
            _ => {}
        }
        (tl.detach(), br.detach())
    }

    /// Draws the edges of a bevel or an emboss. The edges of a path or a polygon run on the outline,
    /// half outside it: what leaves the control's rect is cut, as the C# engine draws it.
    fn paint_bevel(&self, canvas: &Canvas, rect: Rect, scale: f32, cache: Option<&ShapeCache>) {
        let edges = cache.and_then(|cache| cache.bevel.as_ref());
        let (Some(bevel), Some((top_left, bottom_right))) = (self.p.bevel, edges) else { return };
        let depth = bevel.depth * scale;
        if depth <= 0.0 {
            return;
        }
        // Upstream's MAUI color to SKColor cuts the alpha to a byte.
        let alpha = (bevel.opacity.clamp(0.0, 1.0) * 255.0) as u8;
        let (light, shadow) = (bevel.light_color.with_a(alpha), bevel.shadow_color.with_a(alpha));
        let (first, second) = if self.p.bevel_type == BevelType::Emboss { (shadow, light) } else { (light, shadow) };
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(depth);
        let on_outline = matches!(self.p.shape_type, ShapeType::Path | ShapeType::Polygon);
        if on_outline {
            canvas.save();
            canvas.clip_rect(rect, ClipOp::Intersect, false);
        }
        paint.set_color(first);
        canvas.draw_path(top_left, &paint);
        paint.set_color(second);
        canvas.draw_path(bottom_right, &paint);
        if on_outline {
            canvas.restore();
        }
    }

    fn keep(cx: &mut PaintCx, cache: Option<ShapeCache>) {
        if cache.is_some() {
            cx.render[cx.id.index as usize].paints.get_or_insert_default().shape = cache;
        }
    }

    /// Fills the shape (C# PaintBackground). Open shapes have no fill.
    fn fill(&self, canvas: &Canvas, g: &Geometry, scale: f32, path: Option<&Path>, paint: &Paint) {
        match self.p.shape_type {
            ShapeType::Rectangle if self.p.corner_radius.is_zero() => {
                canvas.draw_rect(g.inner, paint);
            }
            ShapeType::Rectangle => {
                // Under a stroke wider than a point the corners follow its inner edge.
                let by = if g.stroke > 0.0 && self.p.stroke_width > 1.0 { g.stroke / 3.0 } else { 0.0 };
                canvas.draw_rrect(RRect::new_rect_radii(g.inner, &self.radii(scale, by)), paint);
            }
            ShapeType::Circle => {
                canvas.draw_circle(g.inner.center(), g.outline.width().min(g.outline.height()) / 2.0, paint);
            }
            ShapeType::Ellipse => {
                canvas.draw_oval(g.inner, paint);
            }
            ShapeType::Path | ShapeType::Polygon => {
                if let Some(path) = path {
                    canvas.draw_path(path, paint);
                }
            }
            ShapeType::Arc | ShapeType::Line => {}
        }
    }

    /// Clips to the inside of the shape, where its children live (C# CreateClip). A plain
    /// rectangle clips on whole pixels, every other shape antialiased.
    fn clip(&self, canvas: &Canvas, g: &Geometry, scale: f32, path: Option<&Path>, op: ClipOp) {
        match self.p.shape_type {
            ShapeType::Rectangle if self.p.corner_radius.is_zero() => {
                canvas.clip_rect(g.inner, op, false);
            }
            ShapeType::Rectangle => {
                canvas.clip_rrect(RRect::new_rect_radii(g.inner, &self.radii(scale, 0.0)), op, true);
            }
            ShapeType::Circle => {
                canvas.clip_rrect(RRect::new_oval(Self::inner_circle(g.inner)), op, true);
            }
            ShapeType::Ellipse => {
                canvas.clip_rrect(RRect::new_oval(g.inner), op, true);
            }
            ShapeType::Path | ShapeType::Polygon => {
                if let Some(path) = path {
                    canvas.clip_path(path, op, true);
                }
            }
            ShapeType::Arc | ShapeType::Line => {}
        }
    }

    /// Strokes the outline (C# PaintStroke).
    fn stroke(&self, cx: &mut PaintCx, g: &Geometry, cache: Option<&ShapeCache>) {
        let (p, scale, canvas, outline) = (&self.p, cx.scale, cx.canvas, g.outline);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_color(p.stroke_color);
        paint.set_blend_mode(p.stroke_blend_mode);
        paint.set_stroke_cap(p.stroke_cap);
        paint.set_stroke_join(match p.stroke_cap {
            PaintCap::Round => PaintJoin::Round,
            PaintCap::Square => PaintJoin::Bevel,
            PaintCap::Butt => PaintJoin::Miter,
        });
        if let Some(gradient) = p.stroke_gradient.as_deref() {
            let paints = cx.render[cx.id.index as usize].paints.get_or_insert_default();
            if let Some(shader) = paint::gradient_shader(&mut paints.stroke, gradient, outline, (p.value1, p.value2)) {
                paint.set_shader(shader);
                paint.set_blend_mode(gradient.blend_mode);
            }
        }
        let path = cache.map(|cache| {
            paint.set_path_effect(cache.dash.clone());
            &cache.path
        });
        // On curves a stroke of a point or less is thinned, or its antialiasing makes it look
        // heavier than the straight parts (C# GetCompensatedStrokeWidth).
        let thin = if g.stroke <= scale { g.stroke * 0.55 } else { g.stroke };
        match p.shape_type {
            ShapeType::Rectangle if p.corner_radius.is_zero() => {
                paint.set_stroke_width(g.stroke);
                canvas.draw_rect(outline, &paint);
            }
            ShapeType::Rectangle => {
                paint.set_stroke_width(thin);
                // Up to two points wide the outline runs through pixel centers.
                let round = f32::round_ties_even;
                let rect = if g.stroke <= 2.0 * scale {
                    let (left, top) = (round(outline.left) + 0.5, round(outline.top) + 0.5);
                    Rect::new(left, top, round(outline.right) - 0.5, round(outline.bottom) - 0.5)
                } else {
                    outline
                };
                canvas.draw_rrect(RRect::new_rect_radii(rect, &self.radii(scale, 0.0)), &paint);
            }
            ShapeType::Circle => {
                paint.set_stroke_width(thin);
                canvas.draw_circle(outline.center(), outline.width().min(outline.height()) / 2.0, &paint);
            }
            ShapeType::Ellipse => {
                paint.set_stroke_width(thin);
                canvas.draw_oval(outline, &paint);
            }
            ShapeType::Arc => {
                paint.set_stroke_width(thin);
                canvas.draw_arc(outline, p.value1, p.value2, false, &paint);
            }
            ShapeType::Path | ShapeType::Polygon | ShapeType::Line => {
                paint.set_stroke_width(match p.shape_type {
                    ShapeType::Path => thin,
                    ShapeType::Line => g.stroke.max(1.0),
                    _ => g.stroke,
                });
                if let Some(path) = path {
                    canvas.draw_path(path, &paint);
                }
            }
        }
    }
}

impl Has<ShapeProps> for SkiaShape {
    fn part(&self) -> &ShapeProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ShapeProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaShape {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for SkiaShape {}

impl Control for SkiaShape {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// Paint sees another version and builds its cached path, shadows and dash again.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.version = self.version.wrapping_add(1);
        self.layout.on_props_changed(cx)
    }

    /// Children live inside the stroke, so an auto size is the content plus the stroke inset
    /// (C# GetContentSizeForAutosizeInPixels).
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let inset = 2.0 * self.stroke_inset(cx.scale);
        let content = self.layout.measure(cx, (width - inset).max(0.0), (height - inset).max(0.0));
        Size::new(content.width + inset, content.height + inset)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let rect = cx.base().rect;
        let outline = self.geometry(rect, cx.scale).outline;
        // The layout places the children inside the control's rect: it gets the outline for that.
        cx.base_mut().rect = outline;
        self.layout.arrange(cx);
        cx.base_mut().rect = rect;
    }

    /// Overlay effects (the ripple) stay inside the shape; an open shape has no inside.
    fn create_clip(&self, rect: Rect, scale: f32) -> Path {
        let g = self.geometry(rect, scale);
        match self.p.shape_type {
            ShapeType::Rectangle => Path::rrect(RRect::new_rect_radii(g.inner, &self.radii(scale, 0.0)), None),
            ShapeType::Circle => Path::oval(Self::inner_circle(g.inner), None),
            ShapeType::Ellipse => Path::oval(g.inner, None),
            ShapeType::Path | ShapeType::Polygon => self.outline_path(g.outline),
            ShapeType::Arc | ShapeType::Line => Path::default(),
        }
    }

    /// A shadow reaches 3 sigma of its blur around its offset copy of the shape (C# MergeShadowMargin).
    fn effects_margin(&self, scale: f32) -> Thickness {
        let mut margin = self.layout.effects_margin(scale);
        for shadow in &self.p.shadows {
            let (spread, dx, dy) = (3.0 * shadow.blur * scale, shadow.x * scale, shadow.y * scale);
            margin = margin.max(Thickness::new(spread - dx, spread - dy, spread + dx, spread + dy));
        }
        margin
    }

    fn paint_background(&self, cx: &mut PaintCx) {
        let g = self.geometry(cx.rect, cx.scale);
        let Some(mut paint) = paint::background_paint(cx, g.inner, (self.p.value1, self.p.value2)) else { return };
        let (canvas, scale) = (cx.canvas, cx.scale);
        let cache = self.take_cache(cx, g.outline);
        let path = cache.as_ref().map(|cache| &cache.path);
        match &cache {
            // As upstream, the fill is drawn once per shadow with the shadow's filter on its paint.
            Some(cache) if !cache.shadows.is_empty() => {
                for filter in &cache.shadows {
                    paint.set_image_filter(filter.clone());
                    if self.p.clip_background_color {
                        canvas.save();
                        self.clip(canvas, &g, scale, path, ClipOp::Difference);
                        self.fill(canvas, &g, scale, path, &paint);
                        canvas.restore();
                    } else {
                        self.fill(canvas, &g, scale, path, &paint);
                    }
                }
            }
            _ => self.fill(canvas, &g, scale, path, &paint),
        }
        Self::keep(cx, cache);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let g = self.geometry(cx.rect, cx.scale);
        let cache = self.take_cache(cx, g.outline);
        // The edges go over the fill, under the children and the stroke (C# Paint).
        self.paint_bevel(cx.canvas, cx.rect, cx.scale, cache.as_ref());
        // An open shape has no inside: it shows no children (C#: its clip is empty).
        if cx.has_children() && !matches!(self.p.shape_type, ShapeType::Arc | ShapeType::Line) {
            cx.canvas.save();
            self.clip(cx.canvas, &g, cx.scale, cache.as_ref().map(|cache| &cache.path), ClipOp::Intersect);
            cx.paint_children();
            cx.canvas.restore();
        }
        // The stroke goes over the fill and the children.
        if g.stroke > 0.0 {
            self.stroke(cx, &g, cache.as_ref());
        }
        Self::keep(cx, cache);
    }
}
