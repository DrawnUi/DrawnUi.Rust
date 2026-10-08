//! Lottie (bodymovin JSON) drawn with Skia, for SkiaLottie. DrawnUi.React draws it with CanvasKit
//! Skottie; skia-safe has Skottie only in a Skia build with text layout, so this is an own renderer
//! of the subset UI animations use, with Skottie's semantics (skia `modules/skottie`): the same
//! time mapping, keyframe interpolation, spatial position curves, shape stack order, trim paths,
//! gradient stops and group isolation.
//!
//! In: shape layers (group, rectangle, ellipse, path, polystar, fill, stroke with dashes, gradient
//! fill and stroke, trim paths, group transforms), solid and null layers, precomps with start
//! time, time stretch and time remap, parenting, layer and group opacity, hold and bezier
//! keyframes, split position, colors replaced before parsing (`apply_tint`), layer masks as
//! Skottie's geometric path (every mode, inverted masks).
// ponytail: masks clip whole (their opacity, feather and expansion are not applied, as Skottie
// does only for opaque masks); no mattes (a matted layer draws unmasked, a matte source stays
// hidden as in Skottie), effects, layer styles, text, images, merge paths, repeaters, round
// corners, offset paths, pucker / bloat, blend modes, 3D, auto-orient, expressions (the static
// value is used). Each is a new arm in `shape_type` / `Content` when a file needs it.

use std::collections::HashMap;
use std::rc::Rc;

use serde_json::{Map, Value};
use skia_safe::{
    Canvas, Color, Color4f, ContourMeasure, ContourMeasureIter, CubicMap, Matrix, Paint, PaintCap, PaintJoin,
    PaintStyle, Path, PathBuilder, PathDirection, PathEffect, PathFillType, Point, RRect, Rect, Shader, StrokeRec,
    TileMode,
    gradient::{self, Gradient, Interpolation},
    matrix::ScaleToFit,
    trim_path_effect,
    PathOp,
};

type Obj = Map<String, Value>;

// ---------------------------------------------------------------- json

/// Skottie `Parse<SkScalar>`: a number, or the first element of an array.
fn scalar_of(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => n.as_f64().map(|n| n as f32),
        Value::Array(a) => a.first().and_then(scalar_of),
        _ => None,
    }
}

fn float(o: &Obj, key: &str, default: f32) -> f32 {
    o.get(key).and_then(scalar_of).unwrap_or(default)
}

/// Skottie `Parse<bool>`: a bool or a number.
fn flag(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|n| n != 0.0),
        _ => None,
    }
}

/// Skottie `ParseDefault<int>`: a plain number, else `default`.
fn int(o: &Obj, key: &str, default: i64) -> i64 {
    o.get(key).and_then(Value::as_f64).map_or(default, |n| n as i64)
}

/// A 1-based enum of the JSON as an index into a table of `len` (Skottie
/// `min(ParseDefault<size_t>(v, 1) - 1, len - 1)`: 0 wraps to the last entry).
fn table(o: &Obj, key: &str, len: usize) -> usize {
    let n = o.get(key).and_then(Value::as_f64).filter(|n| *n >= 0.0).map_or(1, |n| n as u64);
    (n.wrapping_sub(1) as usize).min(len - 1)
}

/// Skottie `Parse<SkPoint>`: `{"x": .., "y": ..}` (arrays give their first element).
fn point_of(v: Option<&Value>) -> Option<Point> {
    let o = v?.as_object()?;
    Some(Point::new(scalar_of(o.get("x")?)?, scalar_of(o.get("y")?)?))
}

/// Skottie `Parse<SkV2>`: an array of at least two numbers.
fn vec2_of(v: Option<&Value>) -> Option<Point> {
    let a = v?.as_array().filter(|a| a.len() >= 2)?;
    Some(Point::new(scalar_of(&a[0])?, scalar_of(&a[1])?))
}

// ---------------------------------------------------------------- properties

/// Keyframe mappings (Skottie `Keyframe::mapping`): hold, linear, else a bezier map index + 2.
const HOLD: u32 = 0;
const LINEAR: u32 = 1;
const CUBIC: u32 = 2;

#[derive(Clone, Copy)]
struct Kf {
    t: f32,
    /// Index of the value, in values of `Prop::len` floats.
    v: u32,
    map: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Scalar,
    /// Positions, sizes, scales, anchors: spatial curves between keyframes with tangents.
    Vec2,
    /// Colors and gradient stops: every value as long as the first.
    Vector,
    /// Bezier path vertices, 6 floats per vertex plus the closed flag.
    Shape,
}

/// An animatable property (Skottie KeyframeAnimator): values, the keyframes pointing into them,
/// the bezier maps of eased segments and, for a Vec2, the curve of each spatial segment.
struct Prop {
    len: usize,
    values: Vec<f32>,
    kfs: Vec<Kf>,
    cubics: Vec<CubicMap>,
    /// Per value (Vec2 only): the path to the next value when the segment has tangents.
    curves: Vec<Option<ContourMeasure>>,
}

/// Appends a value of `kind` to `out`; false (and `out` untouched) when it does not parse.
/// `len`: the length every value of a Vector or Shape property must have.
fn parse_value(v: &Value, kind: Kind, len: Option<usize>, out: &mut Vec<f32>) -> bool {
    let start = out.len();
    let ok = match kind {
        Kind::Scalar => scalar_of(v).map(|x| out.push(x)).is_some(),
        Kind::Vec2 => vec2_of(Some(v)).map(|p| out.extend([p.x, p.y])).is_some(),
        Kind::Vector => match v.as_array() {
            Some(a) if len.is_none_or(|l| l == a.len()) => a.iter().all(|x| scalar_of(x).map(|x| out.push(x)).is_some()),
            _ => false,
        },
        Kind::Shape => shape_value(v, len, out),
    };
    if !ok {
        out.truncate(start);
    }
    ok
}

/// Skottie `parse_encoding_data`: `{"v", "i", "o", "c"}` (or a one-element array of it) as
/// x, y, in x, in y, out x, out y per vertex and the closed flag last.
fn shape_value(v: &Value, len: Option<usize>, out: &mut Vec<f32>) -> bool {
    let root = match v {
        Value::Array(a) if a.len() == 1 => &a[0],
        _ => v,
    };
    let Some(o) = root.as_object() else { return false };
    let Some(vs) = o.get("v").and_then(Value::as_array) else { return false };
    if len.is_some_and(|l| l != vs.len() * 6 + 1) {
        return false;
    }
    let pair = |p: &Value| -> Option<(f32, f32)> {
        let a = p.as_array().filter(|a| a.len() == 2)?;
        Some((scalar_of(&a[0])?, scalar_of(&a[1])?))
    };
    let optional = |key: &str, i: usize| -> Option<(f32, f32)> {
        match o.get(key).and_then(Value::as_array) {
            Some(a) if i < a.len() => pair(&a[i]),
            _ => Some((0.0, 0.0)),
        }
    };
    for (i, vertex) in vs.iter().enumerate() {
        let (Some(p), Some(tin), Some(tout)) = (pair(vertex), optional("i", i), optional("o", i)) else { return false };
        out.extend([p.0, p.1, tin.0, tin.1, tout.0, tout.1]);
    }
    out.push(flag(o.get("c")).unwrap_or(false) as u8 as f32);
    true
}

/// Skottie `backfill_spatial` check: a tangent along the chord and not longer keeps the
/// segment a straight line.
fn along(v0: Point, v1: Point) -> bool {
    let (l0, l1) = (v0.length() * v0.length(), v1.length() * v1.length());
    if l0 < l1 {
        return false;
    }
    let dot = v0.x * v1.x + v0.y * v1.y;
    (dot * dot - l0 * l1).abs() <= 1.0 / 4096.0
}

impl Prop {
    fn constant(values: Vec<f32>) -> Prop {
        Prop { len: values.len(), values, kfs: vec![Kf { t: 0.0, v: 0, map: HOLD }], cubics: Vec::new(), curves: Vec::new() }
    }

    /// Skottie `bindImpl`: `{"a", "k"}`, a static value or keyframes. `None` for a missing or
    /// broken property (the caller's default stays, as in Skottie).
    fn parse(v: Option<&Value>, kind: Kind) -> Option<Prop> {
        let o = v?.as_object()?;
        let k = o.get("k")?;
        if !flag(o.get("a")).unwrap_or(false) {
            let mut values = Vec::new();
            if parse_value(k, kind, None, &mut values) {
                return Some(Prop::constant(values));
            }
            if o.contains_key("a") {
                return None;
            }
        }
        Prop::keyframes(k.as_array().filter(|a| !a.is_empty())?, kind)
    }

    /// Skottie `parseKeyframes` with the value dedupe of the Vector and Vec2 builders.
    fn keyframes(jkfs: &[Value], kind: Kind) -> Option<Prop> {
        let mut p = Prop { len: 0, values: Vec::new(), kfs: Vec::with_capacity(jkfs.len()), cubics: Vec::new(), curves: Vec::new() };
        let mut len = None;
        let (mut ti, mut to, mut pending_spatial) = (Point::default(), Point::default(), false);
        let (mut prev_c0, mut prev_c1) = (Point::default(), Point::default());
        let mut constant = true;
        for (i, jkf) in jkfs.iter().enumerate() {
            let kf = jkf.as_object()?;
            let t = kf.get("t").and_then(scalar_of)?;
            let start = p.values.len();
            // "s", or for the last keyframe of an old export the "e" of the one before.
            let mut ok = kf.get("s").is_some_and(|s| parse_value(s, kind, len, &mut p.values));
            if !ok && i > 0 && i == jkfs.len() - 1 {
                ok = jkfs[i - 1].get("e").is_some_and(|e| parse_value(e, kind, len, &mut p.values));
            }
            if !ok {
                return None;
            }
            let size = *len.get_or_insert(p.values.len() - start);
            if size == 0 {
                return None;
            }
            p.len = size;
            let count = start / size;
            if kind == Kind::Vec2 {
                let value = Point::new(p.values[start], p.values[start + 1]);
                if pending_spatial && count > 0 {
                    let prev = Point::new(p.values[start - 2], p.values[start - 1]);
                    if value != prev && !(along(value - prev, to) && along(prev - value, ti)) {
                        let mut path = PathBuilder::new();
                        path.move_to(prev).cubic_to(prev + to, value + ti, value);
                        p.curves[count - 1] = ContourMeasureIter::new(&path.detach(), false, None).next();
                    }
                }
                ti = vec2_of(kf.get("ti")).unwrap_or_default();
                to = vec2_of(kf.get("to")).unwrap_or_default();
                pending_spatial = ti != Point::default() || to != Point::default();
            }
            // A value equal to the one before is the same value (Skottie keeps one record).
            let same = count > 0
                && p.values[start..] == p.values[start - size..start]
                && !(kind == Kind::Vec2 && pending_spatial);
            let v = if same {
                p.values.truncate(start);
                count - 1
            } else {
                p.curves.push(None);
                count
            } as u32;
            if let Some(prev) = p.kfs.last_mut() {
                if t < prev.t {
                    return None;
                }
                if prev.v == v {
                    prev.map = HOLD;
                }
            }
            let map = if flag(kf.get("h")).unwrap_or(false) {
                HOLD
            } else {
                match (point_of(kf.get("o")), point_of(kf.get("i"))) {
                    (Some(c0), Some(c1)) if !CubicMap::is_linear(c0, c1) => {
                        if c0 != prev_c0 || c1 != prev_c1 || p.cubics.is_empty() {
                            p.cubics.push(CubicMap::new(c0, c1));
                            (prev_c0, prev_c1) = (c0, c1);
                        }
                        CUBIC + p.cubics.len() as u32 - 1
                    }
                    _ => LINEAR,
                }
            };
            constant &= v == p.kfs.first().map_or(v, |k| k.v);
            p.kfs.push(Kf { t, v, map });
        }
        if constant {
            p.kfs.truncate(1);
        }
        Some(p)
    }

    fn is_static(&self) -> bool {
        self.kfs.len() == 1
    }

    /// Skottie `getLERPInfo`: the weight between two values at `t`, as value offsets.
    fn at(&self, t: f32) -> (f32, usize, usize) {
        let (first, last) = (self.kfs[0], self.kfs[self.kfs.len() - 1]);
        let edge = |k: Kf| (0.0, k.v as usize * self.len, k.v as usize * self.len);
        if self.kfs.len() == 1 || t <= first.t {
            return edge(first);
        }
        if t >= last.t {
            return edge(last);
        }
        let i = self.kfs.partition_point(|k| k.t <= t) - 1;
        let (k0, k1) = (self.kfs[i], self.kfs[i + 1]);
        if k0.map == HOLD {
            return edge(k0);
        }
        let mut w = (t - k0.t) / (k1.t - k0.t);
        if k0.map >= CUBIC {
            w = self.cubics[(k0.map - CUBIC) as usize].compute_y_from_x(w);
        }
        (w, k0.v as usize * self.len, k1.v as usize * self.len)
    }

    fn scalar(&self, t: f32) -> f32 {
        let (w, a, b) = self.at(t);
        lerp(self.values[a], self.values[b], w)
    }

    /// Skottie Vec2KeyframeAnimator: along the segment's curve when it has one.
    fn vec2(&self, t: f32) -> Point {
        let (w, a, b) = self.at(t);
        if let Some(Some(curve)) = self.curves.get(a / self.len) {
            let length = curve.length();
            let distance = length * w;
            if let Some((mut pos, tan)) = curve.pos_tan(distance) {
                if !(0.0..=length).contains(&distance) {
                    let overshoot = (-distance).max(distance - length).copysign(distance);
                    pos += tan * overshoot;
                }
                return pos;
            }
        }
        let v = &self.values;
        Point::new(lerp(v[a], v[b], w), lerp(v[a + 1], v[b + 1], w))
    }

    /// The value at `t` into `out` (as long as `len`).
    fn vector(&self, t: f32, out: &mut [f32]) {
        let (w, a, b) = self.at(t);
        for (i, out) in out.iter_mut().enumerate().take(self.len) {
            *out = lerp(self.values[a + i], self.values[b + i], w);
        }
    }

    /// Skottie `ColorValue -> SkColor4f`: channels pinned to 0..1, alpha 1 when absent.
    fn color(&self, t: f32) -> Color4f {
        let mut c = [0.0, 0.0, 0.0, 1.0];
        self.vector(t, &mut c[..self.len.min(4)]);
        let pin = |v: f32| v.clamp(0.0, 1.0);
        Color4f::new(pin(c[0]), pin(c[1]), pin(c[2]), pin(c[3]))
    }

    /// Skottie `ShapeValue -> SkPath`: a line where both tangents are empty, else a cubic.
    fn path(&self, t: f32) -> Path {
        let (w, a, b) = self.at(t);
        let v = |i: usize| lerp(self.values[a + i], self.values[b + i], w);
        let count = self.len / 6;
        let mut path = PathBuilder::new();
        if count == 0 {
            return path.detach();
        }
        let point = |i: usize, at: usize| Point::new(v(i * 6 + at), v(i * 6 + at + 1));
        path.move_to(point(0, 0));
        let mut cubic = |from: usize, to: usize| {
            let (p0, p1) = (point(from, 0), point(to, 0));
            let (c0, c1) = (point(from, 4) + p0, point(to, 2) + p1);
            if c0 == p0 && c1 == p1 {
                path.line_to(p1);
            } else {
                path.cubic_to(c0, c1, p1);
            }
        };
        for i in 1..count {
            cubic(i - 1, i);
        }
        if v(self.len - 1) != 0.0 {
            cubic(count - 1, 0);
            path.close();
        }
        path.detach()
    }
}

fn lerp(a: f32, b: f32, w: f32) -> f32 {
    a + (b - a) * w
}

fn scalar(p: &Option<Prop>, t: f32, default: f32) -> f32 {
    p.as_ref().map_or(default, |p| p.scalar(t))
}

fn vec2(p: &Option<Prop>, t: f32, default: Point) -> Point {
    p.as_ref().map_or(default, |p| p.vec2(t))
}

fn is_static(p: &Option<Prop>) -> bool {
    p.as_ref().is_none_or(Prop::is_static)
}

// ---------------------------------------------------------------- transforms

enum Position {
    Joined(Option<Prop>),
    /// `"s": true`: x and y animate on their own.
    Split(Option<Prop>, Option<Prop>),
}

/// A layer's `ks` or a group's `tr` (Skottie TransformAdapter2D), with its opacity.
struct Transform {
    anchor: Option<Prop>,
    position: Position,
    scale: Option<Prop>,
    rotation: Option<Prop>,
    skew: Option<Prop>,
    skew_axis: Option<Prop>,
    opacity: Option<Prop>,
}

impl Transform {
    fn parse(o: &Obj) -> Transform {
        let p = o.get("p");
        let position = if p.and_then(Value::as_object).is_some_and(|p| flag(p.get("s")).unwrap_or(false)) {
            let p = p.and_then(Value::as_object);
            Position::Split(Prop::parse(p.and_then(|p| p.get("x")), Kind::Scalar), Prop::parse(p.and_then(|p| p.get("y")), Kind::Scalar))
        } else {
            Position::Joined(Prop::parse(p, Kind::Vec2))
        };
        let rotation = match o.get("r") {
            Some(r) if !r.is_null() => Some(r),
            _ => o.get("rz"),
        };
        Transform {
            anchor: Prop::parse(o.get("a"), Kind::Vec2),
            position,
            scale: Prop::parse(o.get("s"), Kind::Vec2),
            rotation: Prop::parse(rotation, Kind::Scalar),
            skew: Prop::parse(o.get("sk"), Kind::Scalar),
            skew_axis: Prop::parse(o.get("sa"), Kind::Scalar),
            opacity: Prop::parse(o.get("o"), Kind::Scalar),
        }
    }

    fn is_static(&self) -> bool {
        let position = match &self.position {
            Position::Joined(p) => is_static(p),
            Position::Split(x, y) => is_static(x) && is_static(y),
        };
        position && [&self.anchor, &self.scale, &self.rotation, &self.skew, &self.skew_axis].into_iter().all(is_static)
    }

    /// T(position) R(rotation) Skew S(scale / 100) T(-anchor).
    fn matrix(&self, t: f32) -> Matrix {
        let position = match &self.position {
            Position::Joined(p) => vec2(p, t, Point::default()),
            Position::Split(x, y) => Point::new(scalar(x, t, 0.0), scalar(y, t, 0.0)),
        };
        let anchor = vec2(&self.anchor, t, Point::default());
        let scale = vec2(&self.scale, t, Point::new(100.0, 100.0));
        let mut m = Matrix::translate(position);
        m.pre_rotate(scalar(&self.rotation, t, 0.0), None);
        let skew = scalar(&self.skew, t, 0.0);
        if skew != 0.0 {
            let sk = -skew.clamp(-85.0, 85.0).to_radians();
            let sa = scalar(&self.skew_axis, t, 0.0);
            let mut s = Matrix::rotate_deg(sa);
            s.pre_skew((sk.tan(), 0.0), None);
            s.pre_rotate(-sa, None);
            m.pre_concat(&s);
        }
        m.pre_scale((scale.x / 100.0, scale.y / 100.0), None);
        m.pre_translate((-anchor.x, -anchor.y));
        m
    }

    /// 0..1 (Skottie OpacityEffect: `o / 100`, nothing drawn at 0 or below, as is above 1).
    fn opacity(&self, t: f32) -> f32 {
        (scalar(&self.opacity, t, 100.0) * 0.01).min(1.0)
    }
}

// ---------------------------------------------------------------- geometry

enum GeoKind {
    Path(Prop),
    Rect { size: Option<Prop>, position: Option<Prop>, radius: Option<Prop>, direction: PathDirection },
    Ellipse { size: Option<Prop>, position: Option<Prop>, direction: PathDirection },
    Star { polygon: bool, points: Option<Prop>, position: Option<Prop>, rotation: Option<Prop>, inner: Option<Prop>, outer: Option<Prop> },
    /// A group's geometry used by a paint outside the group: in the group's transform.
    Transformed(Rc<Geo>, Rc<Transform>),
    Trim(Rc<Geo>, Rc<Trim>),
    /// Several geometries under one paint, one path.
    Merge(Vec<Rc<Geo>>),
}

/// A geometry node; `fixed` is its path when nothing in it animates, built once.
struct Geo {
    kind: GeoKind,
    fixed: Option<Path>,
}

struct Trim {
    start: Option<Prop>,
    end: Option<Prop>,
    offset: Option<Prop>,
}

impl Geo {
    fn new(kind: GeoKind) -> Rc<Geo> {
        let fixed = match &kind {
            GeoKind::Path(p) => p.is_static(),
            GeoKind::Rect { size, position, radius, .. } => [size, position, radius].into_iter().all(is_static),
            GeoKind::Ellipse { size, position, .. } => is_static(size) && is_static(position),
            GeoKind::Star { points, position, rotation, inner, outer, .. } => {
                [points, position, rotation, inner, outer].into_iter().all(is_static)
            }
            GeoKind::Transformed(geo, transform) => geo.fixed.is_some() && transform.is_static(),
            GeoKind::Trim(geo, trim) => geo.fixed.is_some() && [&trim.start, &trim.end, &trim.offset].into_iter().all(is_static),
            GeoKind::Merge(geos) => geos.iter().all(|g| g.fixed.is_some()),
        };
        let mut geo = Geo { kind, fixed: None };
        if fixed {
            geo.fixed = Some(geo.build(0.0));
        }
        Rc::new(geo)
    }

    fn path(&self, t: f32) -> Path {
        match &self.fixed {
            Some(path) => path.clone(),
            None => self.build(t),
        }
    }

    fn build(&self, t: f32) -> Path {
        let centered = |size: &Option<Prop>, position: &Option<Prop>| {
            let (s, p) = (vec2(size, t, Point::default()), vec2(position, t, Point::default()));
            Rect::from_xywh(p.x - s.x / 2.0, p.y - s.y / 2.0, s.x, s.y)
        };
        match &self.kind {
            GeoKind::Path(p) => p.path(t),
            GeoKind::Rect { size, position, radius, direction } => {
                let r = scalar(radius, t, 0.0);
                let rrect = RRect::new_rect_xy(centered(size, position), r, r);
                PathBuilder::new().add_rrect(rrect, *direction, 2).detach()
            }
            GeoKind::Ellipse { size, position, direction } => {
                PathBuilder::new().add_rrect(RRect::new_oval(centered(size, position)), *direction, 1).detach()
            }
            GeoKind::Star { polygon, points, position, rotation, inner, outer } => {
                let count = scalar(points, t, 0.0).round().clamp(0.0, 100_000.0) as u32;
                let arc = std::f32::consts::TAU / count as f32;
                let c = vec2(position, t, Point::default());
                let (inner, outer) = (scalar(inner, t, 0.0), scalar(outer, t, 0.0));
                let on = |r: f32, a: f32| Point::new(c.x + r * a.cos(), c.y + r * a.sin());
                let mut angle = (scalar(rotation, t, 0.0) - 90.0).to_radians();
                let mut path = PathBuilder::new();
                path.move_to(on(outer, angle));
                for _ in 0..count {
                    if !polygon {
                        path.line_to(on(inner, angle + arc * 0.5));
                    }
                    angle += arc;
                    path.line_to(on(outer, angle));
                }
                path.close();
                path.detach()
            }
            GeoKind::Transformed(geo, transform) => geo.path(t).make_transform(&transform.matrix(t)),
            GeoKind::Trim(geo, trim) => {
                let path = geo.path(t);
                // Skottie TrimEffectAdapter.
                let (start, end) = (scalar(&trim.start, t, 0.0) / 100.0, scalar(&trim.end, t, 100.0) / 100.0);
                let offset = scalar(&trim.offset, t, 0.0) / 360.0;
                let (mut from, mut to) = (start.min(end) + offset, start.max(end) + offset);
                let mut mode = trim_path_effect::Mode::Normal;
                if to - from < 1.0 {
                    from -= from.floor();
                    to -= to.floor();
                    if from > to {
                        std::mem::swap(&mut from, &mut to);
                        mode = trim_path_effect::Mode::Inverted;
                    }
                } else {
                    (from, to) = (0.0, 1.0);
                }
                match PathEffect::trim(from, to, mode) {
                    Some(effect) => effect
                        .filter_path(&path, &StrokeRec::new_hairline(), path.bounds())
                        .map_or(path, |(mut builder, _)| builder.detach()),
                    None => path,
                }
            }
            GeoKind::Merge(geos) => {
                let mut path = PathBuilder::new();
                for geo in geos {
                    path.add_path(&geo.path(t), None);
                }
                path.detach()
            }
        }
    }
}

fn direction(o: &Obj) -> PathDirection {
    if int(o, "d", -1) == 3 { PathDirection::CCW } else { PathDirection::CW }
}

/// Skottie `AttachTrimGeometryEffect`: each geometry trimmed ("m": 1), or all of them as one ("m": 2).
fn trimmed(trim: &Rc<Trim>, serial: bool, geos: Vec<Rc<Geo>>) -> Vec<Rc<Geo>> {
    let inputs = if serial { vec![Geo::new(GeoKind::Merge(geos))] } else { geos };
    inputs.into_iter().map(|g| Geo::new(GeoKind::Trim(g, trim.clone()))).collect()
}

// ---------------------------------------------------------------- paints

struct GradientSpec {
    radial: bool,
    /// Color stops; opacity stops follow them in `stops`.
    count: usize,
    stops: Option<Prop>,
    start: Option<Prop>,
    end: Option<Prop>,
    highlight_length: Option<Prop>,
    highlight_angle: Option<Prop>,
}

impl GradientSpec {
    fn is_static(&self) -> bool {
        [&self.stops, &self.start, &self.end, &self.highlight_length, &self.highlight_angle].into_iter().all(is_static)
    }

    /// Skottie GradientAdapter::onSync: color and opacity stops merged into one list, then the
    /// linear or two-point conical shader.
    fn shader(&self, t: f32) -> Option<Shader> {
        let stops = self.stops.as_ref()?;
        let mut values = vec![0.0; stops.len];
        stops.vector(t, &mut values);
        let (c_count, o_count) = (self.count, values.len().checked_sub(self.count * 4)? / 2);
        if values.len() != c_count * 4 + o_count * 2 {
            return None;
        }
        let (colors, opacities) = values.split_at(c_count * 4);
        let (mut ci, mut oi) = (0, 0);
        let (mut position, mut color) = (0.0, [0.0f32; 4]);
        if c_count > 0 {
            color[..3].copy_from_slice(&colors[1..4]);
        }
        color[3] = if o_count > 0 { opacities[1] } else { 1.0 };
        let (mut positions, mut colors4) = (Vec::new(), Vec::new());
        while ci < c_count || oi < o_count {
            let cs = if ci < c_count { [colors[ci * 4], colors[ci * 4 + 1], colors[ci * 4 + 2], colors[ci * 4 + 3]] } else {
                [opacities[oi * 2], color[0], color[1], color[2]]
            };
            let os = if oi < o_count { [opacities[oi * 2], opacities[oi * 2 + 1]] } else { [colors[ci * 4], color[3]] };
            let (c_pos, o_pos) = (cs[0].max(position), os[0].max(position));
            let (c_rel, o_rel) = (c_pos - position, o_pos - position);
            let t_c = (o_rel / c_rel).clamp(0.0, 1.0);
            let t_o = (c_rel / o_rel).clamp(0.0, 1.0);
            let t_c = if t_c.is_nan() { 0.0 } else { t_c };
            let t_o = if t_o.is_nan() { 0.0 } else { t_o };
            position = c_pos.min(o_pos);
            color = [lerp(color[0], cs[1], t_c), lerp(color[1], cs[2], t_c), lerp(color[2], cs[3], t_c), lerp(color[3], os[1], t_o)];
            positions.push(position);
            colors4.push(Color4f::new(color[0], color[1], color[2], color[3]));
            if c_pos <= o_pos {
                ci += 1;
            }
            if o_pos <= c_pos {
                oi += 1;
            }
        }
        if colors4.is_empty() {
            return None;
        }
        let gradient = Gradient::new(gradient::Colors::new(&colors4, Some(&positions[..]), TileMode::Clamp, None), Interpolation::default());
        let (s, e) = (vec2(&self.start, t, Point::default()), vec2(&self.end, t, Point::default()));
        if !self.radial {
            return gradient::shaders::linear_gradient((s, e), &gradient, None);
        }
        let e = Matrix::rotate_deg_pivot(scalar(&self.highlight_angle, t, 0.0), s).map_point(e);
        let eps = 2.0 / 4096.0;
        let h = (scalar(&self.highlight_length, t, 0.0) * 0.01).clamp(-1.0 + eps, 1.0 - eps);
        let focal = s + (e - s) * h;
        let radius = Point::distance(s, e);
        if focal == s {
            gradient::shaders::radial_gradient((s, radius), &gradient, None)
        } else {
            gradient::shaders::two_point_conical_gradient((focal, 0.0), (s, radius), &gradient, None)
        }
    }
}

/// Stroke dashes: intervals, then the offset (Skottie DashAdapter).
struct Dashes {
    intervals: Vec<Option<Prop>>,
    offset: Option<Prop>,
}

impl Dashes {
    fn effect(&self, t: f32) -> Option<PathEffect> {
        let intervals: Vec<f32> = self.intervals.iter().map(|p| scalar(p, t, 0.0)).collect();
        PathEffect::dash(&intervals, scalar(&self.offset, t, 0.0))
    }
}

/// A fill or a stroke with a color or a gradient (Skottie FillStrokeAdapter).
struct PaintSpec {
    stroke: bool,
    color: Option<Prop>,
    gradient: Option<GradientSpec>,
    opacity: Option<Prop>,
    width: Option<Prop>,
    cap: PaintCap,
    join: PaintJoin,
    miter: f32,
    dashes: Option<Dashes>,
    fill_type: PathFillType,
    /// The shader of a gradient that does not animate, built once.
    fixed_shader: Option<Shader>,
    fixed_dashes: Option<PathEffect>,
}

impl PaintSpec {
    fn parse(o: &Obj, stroke: bool, gradient: bool) -> Option<PaintSpec> {
        let gradient = if gradient {
            let g = o.get("g")?.as_object()?;
            let count = int(g, "p", -1);
            if count < 0 {
                return None;
            }
            Some(GradientSpec {
                radial: int(o, "t", 1) != 1,
                count: count as usize,
                stops: Prop::parse(g.get("k"), Kind::Vector),
                start: Prop::parse(o.get("s"), Kind::Vec2),
                end: Prop::parse(o.get("e"), Kind::Vec2),
                highlight_length: Prop::parse(o.get("h"), Kind::Scalar),
                highlight_angle: Prop::parse(o.get("a"), Kind::Scalar),
            })
        } else {
            None
        };
        let dashes = o.get("d").and_then(Value::as_array).filter(|d| stroke && d.len() > 1).map(|d| {
            let mut props: Vec<Option<Prop>> = d.iter().map(|i| Prop::parse(i.as_object().and_then(|i| i.get("v")), Kind::Scalar)).collect();
            let offset = props.pop().flatten();
            Dashes { intervals: props, offset }
        });
        let fill_type = if table(o, "r", 2) == 1 { PathFillType::EvenOdd } else { PathFillType::Winding };
        let mut spec = PaintSpec {
            stroke,
            color: if gradient.is_none() { Prop::parse(o.get("c"), Kind::Vector) } else { None },
            opacity: Prop::parse(o.get("o"), Kind::Scalar),
            width: Prop::parse(o.get("w"), Kind::Scalar),
            cap: [PaintCap::Butt, PaintCap::Round, PaintCap::Square][table(o, "lc", 3)],
            join: [PaintJoin::Miter, PaintJoin::Round, PaintJoin::Bevel][table(o, "lj", 3)],
            miter: float(o, "ml", 4.0),
            gradient,
            dashes,
            fill_type,
            fixed_shader: None,
            fixed_dashes: None,
        };
        if let Some(g) = spec.gradient.as_ref().filter(|g| g.is_static()) {
            spec.fixed_shader = g.shader(0.0);
        }
        if let Some(d) = spec.dashes.as_ref().filter(|d| d.intervals.iter().all(is_static) && is_static(&d.offset)) {
            spec.fixed_dashes = d.effect(0.0);
        }
        Some(spec)
    }

    /// The paint at `t` under the inherited `opacity`; `None` when it draws nothing.
    fn paint(&self, t: f32, opacity: f32) -> Option<Paint> {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let alpha = match (&self.gradient, &self.color) {
            (Some(g), _) => {
                let shader = match &self.fixed_shader {
                    Some(shader) => shader.clone(),
                    None => g.shader(t)?,
                };
                paint.set_shader(shader);
                1.0
            }
            (None, Some(color)) => {
                let c = color.color(t);
                paint.set_color4f(Color4f::new(c.r, c.g, c.b, 1.0), None);
                c.a
            }
            // No color: black (Skottie's Color node default).
            (None, None) => 1.0,
        };
        let alpha = alpha * (scalar(&self.opacity, t, 100.0) * 0.01) * opacity;
        if alpha <= 0.0 {
            return None;
        }
        paint.set_alpha_f(alpha.min(1.0));
        if self.stroke {
            let width = scalar(&self.width, t, 1.0);
            if width <= 0.0 {
                return None;
            }
            paint.set_style(PaintStyle::Stroke).set_stroke_width(width);
            paint.set_stroke_cap(self.cap).set_stroke_join(self.join).set_stroke_miter(self.miter);
            if let Some(dashes) = &self.dashes {
                paint.set_path_effect(match &self.fixed_dashes {
                    Some(effect) => Some(effect.clone()),
                    None => dashes.effect(t),
                });
            }
        }
        Some(paint)
    }
}

// ---------------------------------------------------------------- shape tree

/// What a shape layer draws: the render tree Skottie's `attachShape` builds.
enum Node {
    Draw { geo: Rc<Geo>, paint: PaintSpec },
    /// Drawn first to last; the transform and its opacity apply to all of them.
    Group { children: Vec<Node>, transform: Option<Rc<Transform>> },
}

#[derive(Clone, Copy, PartialEq)]
enum ShapeType {
    Geometry,
    Trim,
    Paint { stroke: bool, gradient: bool },
    Group,
    Transform,
}

fn shape_type(o: &Obj) -> Option<ShapeType> {
    Some(match o.get("ty")?.as_str()? {
        "el" | "rc" | "sh" | "sr" => ShapeType::Geometry,
        "tm" => ShapeType::Trim,
        "fl" => ShapeType::Paint { stroke: false, gradient: false },
        "st" => ShapeType::Paint { stroke: true, gradient: false },
        "gf" => ShapeType::Paint { stroke: false, gradient: true },
        "gs" => ShapeType::Paint { stroke: true, gradient: true },
        "gr" => ShapeType::Group,
        "tr" => ShapeType::Transform,
        _ => return None,
    })
}

fn geometry(o: &Obj) -> Option<Rc<Geo>> {
    let v2 = |key| Prop::parse(o.get(key), Kind::Vec2);
    let s = |key| Prop::parse(o.get(key), Kind::Scalar);
    let kind = match o.get("ty")?.as_str()? {
        "sh" => GeoKind::Path(Prop::parse(o.get("ks"), Kind::Shape)?),
        "rc" => GeoKind::Rect { size: v2("s"), position: v2("p"), radius: s("r"), direction: direction(o) },
        "el" => GeoKind::Ellipse { size: v2("s"), position: v2("p"), direction: direction(o) },
        _ => {
            let polygon = match int(o, "sy", 0) {
                1 => false,
                2 => true,
                _ => return None,
            };
            GeoKind::Star { polygon, points: s("pt"), position: v2("p"), rotation: s("r"), inner: s("ir"), outer: s("or") }
        }
    };
    Some(Geo::new(kind))
}

/// Skottie `AnimationBuilder::attachShape`. Items are listed top first: a paint draws the
/// geometries above it, after the trims below it in its group and in the enclosing groups; the
/// group's geometry goes on to the enclosing group in the group's transform.
fn attach_shape(items: &[Value], geo_stack: &mut Vec<Rc<Geo>>, trims: &mut Vec<(Rc<Trim>, bool)>) -> Option<Node> {
    let mut transform = None;
    let mut recs = Vec::with_capacity(items.len());
    for item in items.iter().rev() {
        let Some(o) = item.as_object() else { continue };
        let Some(ty) = shape_type(o) else { continue };
        if flag(o.get("hd")).unwrap_or(false) {
            continue;
        }
        recs.push((o, ty));
        match ty {
            ShapeType::Transform => transform = Some(o),
            ShapeType::Trim => trims.push((
                Rc::new(Trim {
                    start: Prop::parse(o.get("s"), Kind::Scalar),
                    end: Prop::parse(o.get("e"), Kind::Scalar),
                    offset: Prop::parse(o.get("o"), Kind::Scalar),
                }),
                table(o, "m", 2) == 1,
            )),
            _ => {}
        }
    }
    let mut geos: Vec<Rc<Geo>> = Vec::new();
    let mut draws: Vec<Node> = Vec::new();
    for &(o, ty) in recs.iter().rev() {
        match ty {
            ShapeType::Geometry => geos.extend(geometry(o)),
            ShapeType::Trim => {
                let (trim, serial) = trims.pop().expect("pushed above");
                if !geos.is_empty() {
                    geos = trimmed(&trim, serial, geos);
                }
            }
            ShapeType::Group => {
                let items = o.get("it").and_then(Value::as_array).map_or(&[][..], |a| &a[..]);
                draws.extend(attach_shape(items, &mut geos, trims));
            }
            ShapeType::Paint { stroke, gradient } => {
                let Some(paint) = PaintSpec::parse(o, stroke, gradient) else { continue };
                if geos.is_empty() {
                    continue;
                }
                let mut draw_geos = geos.clone();
                for (trim, serial) in trims.iter().rev() {
                    draw_geos = trimmed(trim, *serial, draw_geos);
                }
                let geo = if draw_geos.len() > 1 { Geo::new(GeoKind::Merge(draw_geos)) } else { draw_geos.remove(0) };
                draws.push(Node::Draw { geo, paint });
            }
            ShapeType::Transform => {}
        }
    }
    let mut wrapper = match draws.len() {
        0 => None,
        1 => draws.pop(),
        _ => {
            draws.reverse();
            Some(Node::Group { children: draws, transform: None })
        }
    };
    let transform = transform.map(|o| Rc::new(Transform::parse(o)));
    if let Some(transform) = &transform {
        wrapper = wrapper.map(|node| Node::Group { children: vec![node], transform: Some(transform.clone()) });
    }
    for geo in geos {
        geo_stack.push(match &transform {
            Some(transform) => Geo::new(GeoKind::Transformed(geo, transform.clone())),
            None => geo,
        });
    }
    wrapper
}

// ---------------------------------------------------------------- layers

enum Content {
    None,
    Shapes(Node),
    Solid(Color, Rect),
    Precomp(Precomp),
}

/// Skottie `attachPrecompLayer`: the time inside is `(t - st) / sr`, or the time remap `tm`
/// (seconds, at the outer time) times the frame rate.
struct Precomp {
    comp: Rc<Comp>,
    start: f32,
    stretch: f32,
    remap: Option<Prop>,
    fps: f32,
}

impl Precomp {
    fn time(&self, t: f32) -> f32 {
        if let Some(remap) = &self.remap {
            return remap.scalar(t) * self.fps;
        }
        if (self.start).abs() <= 1.0 / 4096.0 && (self.stretch - 1.0).abs() <= 1.0 / 4096.0 {
            return t;
        }
        let scale = 1.0 / self.stretch;
        (t - self.start) * if scale.is_finite() { scale } else { 0.0 }
    }
}

struct Layer {
    parent: Option<usize>,
    in_point: f32,
    out_point: f32,
    /// `ks`; a layer without it has no transform and ignores its parent, as in Skottie.
    transform: Option<Transform>,
    hidden: bool,
    /// A precomp's `w` x `h`: its content is cut there.
    clip: Option<Rect>,
    /// `masksProperties`: the layer's content is cut to them.
    masks: Vec<Mask>,
    content: Content,
}

/// A layer mask (Skottie `AttachMask`, the geometric merge of opaque masks): its shape in the
/// layer's space, how it joins the masks before it (a = union, s = difference, i = intersect,
/// l = union, d = intersect, f = xor), inverted or not.
struct Mask {
    shape: Prop,
    op: PathOp,
    /// Subtract: the first mask of a layer leaves the layer outside it.
    subtract: bool,
    inverted: bool,
}

impl Mask {
    fn parse(o: &Obj) -> Option<Mask> {
        let (op, subtract) = match o.get("mode").and_then(Value::as_str).unwrap_or("a") {
            "a" | "l" => (PathOp::Union, false),
            "s" => (PathOp::Difference, true),
            "i" | "d" => (PathOp::Intersect, false),
            "f" => (PathOp::XOR, false),
            _ => return None, // "n": no mask
        };
        let shape = Prop::parse(o.get("pt"), Kind::Shape)?;
        Some(Mask { shape, op, subtract, inverted: flag(o.get("inv")).unwrap_or(false) })
    }
}

struct Comp {
    /// Top first, as listed.
    layers: Vec<Layer>,
}

impl Layer {
    fn active(&self, t: f32) -> bool {
        (t >= self.in_point && t < self.out_point) || (t > self.out_point && t <= self.in_point)
    }

    /// What the masks leave of the layer at `t`, in its space; `None` without masks.
    fn mask(&self, t: f32) -> Option<Path> {
        let mut merged: Option<Path> = None;
        for (i, mask) in self.masks.iter().enumerate() {
            let mut path = mask.shape.path(t);
            if mask.inverted != (i == 0 && mask.subtract) {
                path.toggle_inverse_fill_type();
            }
            merged = Some(match merged {
                None => path,
                Some(before) => before.op(&path, mask.op).unwrap_or(before),
            });
        }
        merged
    }
}

impl Comp {
    /// The layer's transform after its parents'.
    fn matrix(&self, index: usize, t: f32) -> Matrix {
        let mut m = Matrix::new_identity();
        let mut at = Some(index);
        // Parents were resolved without cycles; the bound is a guard.
        for _ in 0..self.layers.len() {
            let Some(i) = at else { break };
            let Some(transform) = &self.layers[i].transform else { break };
            m = Matrix::concat(&transform.matrix(t), &m);
            at = self.layers[i].parent;
        }
        m
    }

    fn render(&self, canvas: &Canvas, t: f32, opacity: f32) {
        let shown = |l: &&Layer| !l.hidden && !matches!(l.content, Content::None) && l.active(t);
        // Skottie's layer group draws into a layer when its opacity is partial and layers overlap.
        let isolate = opacity < 1.0 && self.layers.iter().filter(shown).nth(1).is_some();
        let opacity = if isolate {
            canvas.save_layer_alpha_f(None, opacity);
            1.0
        } else {
            opacity
        };
        for (i, layer) in self.layers.iter().enumerate().rev() {
            if !shown(&layer) {
                continue;
            }
            let opacity = opacity * layer.transform.as_ref().map_or(1.0, |tr| tr.opacity(t));
            if opacity <= 0.0 {
                continue;
            }
            canvas.save();
            if layer.transform.is_some() {
                canvas.concat(&self.matrix(i, t));
            }
            if let Some(clip) = layer.clip {
                canvas.clip_rect(clip, None, true);
            }
            if let Some(mask) = layer.mask(t) {
                canvas.clip_path(&mask, None, true);
            }
            match &layer.content {
                Content::Shapes(node) => render_node(node, canvas, t, opacity),
                Content::Solid(color, rect) => {
                    let mut paint = Paint::default();
                    paint.set_color(*color);
                    paint.set_alpha_f(opacity);
                    canvas.draw_rect(rect, &paint);
                }
                Content::Precomp(precomp) => precomp.comp.render(canvas, precomp.time(t), opacity),
                Content::None => {}
            }
            canvas.restore();
        }
        if isolate {
            canvas.restore();
        }
    }
}

fn render_node(node: &Node, canvas: &Canvas, t: f32, opacity: f32) {
    match node {
        Node::Draw { geo, paint: spec } => {
            let Some(paint) = spec.paint(t, opacity) else { return };
            let mut path = geo.path(t);
            path.set_fill_type(spec.fill_type);
            canvas.draw_path(&path, &paint);
        }
        Node::Group { children, transform } => {
            let opacity = opacity * transform.as_ref().map_or(1.0, |tr| tr.opacity(t));
            if opacity <= 0.0 {
                return;
            }
            let count = canvas.save();
            if let Some(transform) = transform {
                canvas.concat(&transform.matrix(t));
            }
            let opacity = if opacity < 1.0 && children.len() > 1 {
                canvas.save_layer_alpha_f(None, opacity);
                1.0
            } else {
                opacity
            };
            for child in children {
                render_node(child, canvas, t, opacity);
            }
            canvas.restore_to_count(count);
        }
    }
}

/// Builds the layers of a composition and the precomps it refers to (each asset once).
struct Builder<'a> {
    assets: HashMap<&'a str, &'a Obj>,
    built: HashMap<&'a str, Rc<Comp>>,
    /// Asset ids being built: a precomp that contains itself stays empty.
    building: Vec<&'a str>,
    fps: f32,
}

impl<'a> Builder<'a> {
    fn comp(&mut self, jlayers: &'a [Value]) -> Comp {
        let mut layers: Vec<Layer> = Vec::with_capacity(jlayers.len());
        let mut indices: HashMap<i64, usize> = HashMap::new();
        let mut parents = Vec::with_capacity(jlayers.len());
        for jlayer in jlayers {
            let Some(o) = jlayer.as_object() else { continue };
            indices.insert(int(o, "ind", -1), layers.len());
            parents.push(int(o, "parent", -1));
            layers.push(self.layer(o));
        }
        for (i, parent) in parents.into_iter().enumerate() {
            layers[i].parent = if parent >= 0 { indices.get(&parent).copied() } else { None };
        }
        // A parent chain that comes back to a layer is cut there.
        for i in 0..layers.len() {
            let (mut at, mut steps) = (layers[i].parent, 0);
            while let Some(p) = at {
                steps += 1;
                if p == i || steps > layers.len() {
                    layers[i].parent = None;
                    break;
                }
                at = layers[p].parent;
            }
        }
        Comp { layers }
    }

    fn layer(&mut self, o: &'a Obj) -> Layer {
        let in_point = float(o, "ip", 0.0);
        let hidden = match o.get("hd") {
            Some(Value::Bool(hd)) => *hd,
            _ => flag(o.get("td")).unwrap_or(false),
        };
        let clip = match (o.get("w").and_then(scalar_of), o.get("h").and_then(scalar_of)) {
            (Some(w), Some(h)) => Some(Rect::from_wh(w, h)),
            _ => None,
        };
        let content = match int(o, "ty", -1) {
            0 => self.precomp(o),
            1 => solid(o),
            4 => {
                let shapes = o.get("shapes").and_then(Value::as_array).map_or(&[][..], |a| &a[..]);
                attach_shape(shapes, &mut Vec::new(), &mut Vec::new()).map_or(Content::None, Content::Shapes)
            }
            _ => Content::None,
        };
        let masks = match o.get("masksProperties").and_then(Value::as_array) {
            Some(masks) => masks.iter().filter_map(Value::as_object).filter_map(Mask::parse).collect(),
            None => Vec::new(),
        };
        Layer {
            parent: None,
            in_point,
            out_point: float(o, "op", 0.0),
            transform: o.get("ks").and_then(Value::as_object).map(Transform::parse),
            hidden,
            clip,
            masks,
            content,
        }
    }

    fn precomp(&mut self, o: &'a Obj) -> Content {
        let Some(id) = o.get("refId").and_then(Value::as_str) else { return Content::None };
        let comp = match self.built.get(id) {
            Some(comp) => comp.clone(),
            None => {
                let Some(asset) = self.assets.get(id).copied() else { return Content::None };
                if self.building.contains(&id) {
                    return Content::None;
                }
                self.building.push(id);
                let layers = asset.get("layers").and_then(Value::as_array).map_or(&[][..], |a| &a[..]);
                let comp = Rc::new(self.comp(layers));
                self.building.pop();
                self.built.insert(id, comp.clone());
                comp
            }
        };
        Content::Precomp(Precomp {
            comp,
            start: float(o, "st", 0.0),
            stretch: float(o, "sr", 1.0),
            remap: Prop::parse(o.get("tm"), Kind::Scalar),
            fps: self.fps,
        })
    }
}

/// Skottie `attachSolidLayer`: an `sw` x `sh` rect of the `sc` hex color, always opaque.
fn solid(o: &Obj) -> Content {
    let (w, h) = (float(o, "sw", 0.0), float(o, "sh", 0.0));
    let hex = o.get("sc").and_then(Value::as_str).and_then(|s| s.strip_prefix('#')).and_then(|s| u32::from_str_radix(s, 16).ok());
    match hex {
        Some(c) if w > 0.0 && h > 0.0 => Content::Solid(Color::new(0xff00_0000 | c), Rect::from_wh(w, h)),
        _ => Content::None,
    }
}

// ---------------------------------------------------------------- animation

/// A parsed animation, shared by every control that shows the same file with the same colors.
pub(crate) struct Animation {
    pub width: f32,
    pub height: f32,
    pub fps: f32,
    pub in_point: f32,
    pub out_point: f32,
    root: Comp,
}

impl Animation {
    /// Parses bodymovin JSON; `tints` replace its colors first (see `apply_tint`).
    pub fn parse(bytes: &[u8], tints: &[Color]) -> Result<Animation, String> {
        let mut doc: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !tints.is_empty() {
            apply_tint(&mut doc, tints);
        }
        let o = doc.as_object().ok_or("not a JSON object")?;
        let (width, height) = (float(o, "w", 0.0), float(o, "h", 0.0));
        let fps = float(o, "fr", -1.0);
        let in_point = float(o, "ip", 0.0);
        let out_point = float(o, "op", f32::MAX).max(in_point);
        let version = o.get("v").and_then(Value::as_str).unwrap_or("");
        if width <= 0.0 || height <= 0.0 || version.is_empty() || fps <= 0.0 || !((out_point - in_point) / fps).is_finite() {
            return Err("invalid animation params".into());
        }
        let mut assets = HashMap::new();
        for asset in o.get("assets").and_then(Value::as_array).map_or(&[][..], |a| &a[..]) {
            if let Some(a) = asset.as_object()
                && let Some(id) = a.get("id").and_then(Value::as_str)
            {
                assets.insert(id, a);
            }
        }
        let mut builder = Builder { assets, built: HashMap::new(), building: Vec::new(), fps };
        let root = builder.comp(o.get("layers").and_then(Value::as_array).map_or(&[][..], |a| &a[..]));
        Ok(Animation { width, height, fps, in_point, out_point, root })
    }

    /// Frames from the in point to the out point, rounded (React `TotalFrames`).
    pub fn total_frames(&self) -> f32 {
        (self.out_point - self.in_point).round()
    }

    /// Draws `frame` (from the in point; Skottie `seekFrame`: the out point is exclusive) scaled
    /// to fit `dst` and centered, cut to the composition.
    pub fn render(&self, canvas: &Canvas, dst: Rect, frame: f32) {
        let last = if self.out_point > self.in_point { next_down(self.out_point) } else { self.out_point };
        let t = (self.in_point + frame).clamp(self.in_point, last.max(self.in_point));
        let src = Rect::from_wh(self.width, self.height);
        canvas.save();
        if let Some(m) = Matrix::rect_2_rect(src, dst, ScaleToFit::Center) {
            canvas.concat(&m);
        }
        canvas.clip_rect(src, None, false);
        self.root.render(canvas, t, 1.0);
        canvas.restore();
    }
}

/// The float just below a positive or negative `x` (C `nextafterf(x, -inf)`).
fn next_down(x: f32) -> f32 {
    if x > 0.0 {
        f32::from_bits(x.to_bits() - 1)
    } else if x == 0.0 {
        -f32::from_bits(1)
    } else {
        f32::from_bits(x.to_bits() + 1)
    }
}

// ---------------------------------------------------------------- colors

/// C# / React `SkiaLottie.ApplyTint`: every distinct color of the JSON (a `k` array of 3 or 4
/// numbers in 0..1, an `sc` / `fc` hex string), in document order, takes the next tint; the last
/// tint covers the rest; alpha is kept. As upstream, animated colors (keyframe `s` values) are
/// not replaced and anything else that looks like a color (an anchor `[0, 0, 0]`) is.
pub(crate) fn apply_tint(doc: &mut Value, tints: &[Color]) {
    let key = |c: [f32; 4]| format!("{:.4},{:.4},{:.4}", c[0], c[1], c[2]);
    let mut mapping: HashMap<String, [f32; 3]> = HashMap::new();
    let mut index = 0;
    visit_colors(doc, &mut |slot| {
        let Some(c) = color_in(slot) else { return };
        if let std::collections::hash_map::Entry::Vacant(e) = mapping.entry(key(c)) {
            let tint = tints[index.min(tints.len() - 1)];
            index += 1;
            e.insert([tint.r() as f32 / 255.0, tint.g() as f32 / 255.0, tint.b() as f32 / 255.0]);
        }
    });
    visit_colors(doc, &mut |slot| {
        let Some(c) = color_in(slot) else { return };
        let [r, g, b] = mapping[&key(c)];
        let h = |v: f32| (v * 255.0).round() as u8;
        *slot = match slot.as_array().map(Vec::len) {
            None => Value::String(format!("#{:02X}{:02X}{:02X}{:02X}", h(c[3]), h(r), h(g), h(b))),
            Some(4) => Value::from(vec![r as f64, g as f64, b as f64, c[3] as f64]),
            Some(_) => Value::from(vec![r as f64, g as f64, b as f64]),
        };
    });
}

fn visit_colors(node: &mut Value, visit: &mut dyn FnMut(&mut Value)) {
    match node {
        Value::Array(items) => {
            for item in items {
                visit_colors(item, visit);
            }
        }
        Value::Object(map) => {
            for (name, value) in map.iter_mut() {
                let color = match name.as_str() {
                    "k" => is_color_array(value),
                    "sc" | "fc" => value.is_string(),
                    _ => false,
                };
                if color {
                    visit(value);
                }
                visit_colors(value, visit);
            }
        }
        _ => {}
    }
}

fn is_color_array(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| (a.len() == 3 || a.len() == 4) && a.iter().all(|x| x.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x))))
}

/// The color in a slot `visit_colors` found: a `k` array, or a `#RRGGBB` / `#AARRGGBB` string.
fn color_in(slot: &Value) -> Option<[f32; 4]> {
    match slot {
        Value::Array(a) => {
            let c = |i: usize| a.get(i).and_then(Value::as_f64).map(|v| v as f32);
            Some([c(0)?, c(1)?, c(2)?, c(3).unwrap_or(1.0)])
        }
        Value::String(s) => {
            let hex = s.strip_prefix('#')?;
            let v = u32::from_str_radix(hex, 16).ok()?;
            let ch = |shift: u32| ((v >> shift) & 0xff) as f32 / 255.0;
            match hex.len() {
                6 => Some([ch(16), ch(8), ch(0), 1.0]),
                8 => Some([ch(16), ch(8), ch(0), ch(24)]),
                _ => None,
            }
        }
        _ => None,
    }
}
