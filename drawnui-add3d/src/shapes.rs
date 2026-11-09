//! 3D made in code: smooth shapes (turned profiles, spheres, rings, boxes, any 2D path pushed into
//! depth) put together as named, colored parts. A part is a node and a material of its own name, so
//! `set_node_*` and `set_material_color` drive it like a part of a model file.

use std::f32::consts::TAU;

use glam::{Quat, Vec2, Vec3};
use drawnui::skia::{Color, Path, PathVerb};

use super::scene::{Material, Node, Primitive, Scene};

/// A mesh made in code (see the constructors).
#[derive(Clone, Default)]
pub struct Mesh3D {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    /// Outline directions per vertex; empty = the normals averaged per position.
    outline: Vec<[f32; 3]>,
}

impl Mesh3D {
    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, normal: Vec3) {
        let base = self.positions.len() as u32;
        for p in [a, b, c, d] {
            self.positions.push(p.to_array());
            self.normals.push(normal.to_array());
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A profile turned around the vertical axis: `(radius, height)` points from bottom to top (a
    /// cup, a vase, a coin's rim). Each step of the profile is its own band, so corners stay sharp.
    pub fn lathe(profile: &[(f32, f32)], segments: usize) -> Mesh3D {
        let segments = segments.max(3);
        let mut mesh = Mesh3D::default();
        for pair in profile.windows(2) {
            let ((r0, y0), (r1, y1)) = (pair[0], pair[1]);
            // The band's outward normal in the profile plane.
            let (dr, dy) = (r1 - r0, y1 - y0);
            let (nr, ny) = Vec2::new(dy, -dr).normalize_or_zero().into();
            let base = mesh.positions.len() as u32;
            for s in 0..=segments {
                let a = s as f32 / segments as f32 * TAU;
                let (sin, cos) = a.sin_cos();
                for (r, y) in [(r0, y0), (r1, y1)] {
                    mesh.positions.push([r * cos, y, r * sin]);
                    mesh.normals.push([nr * cos, ny, nr * sin]);
                }
            }
            for s in 0..segments as u32 {
                let (a, b, c, d) = (base + s * 2, base + s * 2 + 1, base + s * 2 + 3, base + s * 2 + 2);
                mesh.indices.extend([a, b, c, a, c, d]);
            }
        }
        mesh
    }

    /// A disc or a short cylinder (a coin, a button, a medal), `height` thick, centered.
    pub fn cylinder(radius: f32, height: f32) -> Mesh3D {
        let h = height / 2.0;
        Mesh3D::lathe(&[(0.0, -h), (radius, -h), (radius, h), (0.0, h)], 64)
    }

    /// A sphere (a ball, a dot of a loader, a gem's round cousin).
    pub fn sphere(radius: f32) -> Mesh3D {
        let steps = 24;
        let profile: Vec<(f32, f32)> = (0..=steps).map(|i| std::f32::consts::PI * (i as f32 / steps as f32 - 0.5)).map(|a| (radius * a.cos(), radius * a.sin())).collect();
        Mesh3D::lathe(&profile, 48)
    }

    /// A ring (a torus): `radius` to the middle of the tube, `tube` the tube's radius.
    pub fn torus(radius: f32, tube: f32) -> Mesh3D {
        let steps = 24;
        let profile: Vec<(f32, f32)> = (0..=steps).map(|i| i as f32 / steps as f32 * TAU).map(|a| (radius + tube * a.cos(), -tube * a.sin())).collect();
        Mesh3D::lathe(&profile, 64)
    }

    /// A box, centered.
    pub fn cuboid(width: f32, height: f32, depth: f32) -> Mesh3D {
        let (x, y, z) = (width / 2.0, height / 2.0, depth / 2.0);
        let v = |a: f32, b: f32, c: f32| Vec3::new(a * x, b * y, c * z);
        let mut mesh = Mesh3D::default();
        mesh.quad(v(-1., -1., 1.), v(1., -1., 1.), v(1., 1., 1.), v(-1., 1., 1.), Vec3::Z);
        mesh.quad(v(1., -1., -1.), v(-1., -1., -1.), v(-1., 1., -1.), v(1., 1., -1.), -Vec3::Z);
        mesh.quad(v(1., -1., 1.), v(1., -1., -1.), v(1., 1., -1.), v(1., 1., 1.), Vec3::X);
        mesh.quad(v(-1., -1., -1.), v(-1., -1., 1.), v(-1., 1., 1.), v(-1., 1., -1.), -Vec3::X);
        mesh.quad(v(-1., 1., 1.), v(1., 1., 1.), v(1., 1., -1.), v(-1., 1., -1.), Vec3::Y);
        mesh.quad(v(-1., -1., -1.), v(1., -1., -1.), v(1., -1., 1.), v(-1., -1., 1.), -Vec3::Y);
        mesh
    }

    /// A 2D path pushed into depth (a star, a heart, a check mark, a logo, a digit), `depth` thick,
    /// facing the viewer, centered on the path's bounds; path units, y down as in Skia. Each closed
    /// contour is its own solid.
    // ponytail: contours are filled as solids, holes (an O, a ring) are not cut; draw a ring with
    // `torus` or `lathe` until a shape needs a hole.
    pub fn extrude(path: &Path, depth: f32) -> Mesh3D {
        let bounds = path.bounds();
        let center = Vec2::new(bounds.center_x(), bounds.center_y());
        let mut mesh = Mesh3D::default();
        for mut contour in contours(path) {
            for p in &mut contour {
                *p = Vec2::new(p.x - center.x, center.y - p.y);
            }
            if signed_area(&contour) < 0.0 {
                contour.reverse();
            }
            let n = contour.len();
            if n < 3 {
                continue;
            }
            // The outline follows the 2D shape: at each corner, between its two edges' normals.
            let edge_normal = |i: usize| {
                let e = contour[(i + 1) % n] - contour[i];
                Vec2::new(e.y, -e.x).normalize_or_zero()
            };
            let corner: Vec<[f32; 3]> = (0..n).map(|i| (edge_normal((i + n - 1) % n) + edge_normal(i)).normalize_or_zero()).map(|v| [v.x, v.y, 0.0]).collect();
            let (front, back) = (depth / 2.0, -depth / 2.0);
            for (z, normal, flip) in [(front, Vec3::Z, false), (back, -Vec3::Z, true)] {
                let base = mesh.positions.len() as u32;
                for (i, p) in contour.iter().enumerate() {
                    mesh.positions.push([p.x, p.y, z]);
                    mesh.normals.push(normal.to_array());
                    mesh.outline.push(corner[i]);
                }
                for [a, b, c] in ear_clip(&contour) {
                    let (b, c) = if flip { (c, b) } else { (b, c) };
                    mesh.indices.extend([base + a, base + b, base + c]);
                }
            }
            for i in 0..n {
                let (a, b) = (contour[i], contour[(i + 1) % n]);
                let edge = b - a;
                if edge.length_squared() < 1e-12 {
                    continue;
                }
                let normal = Vec3::new(edge.y, -edge.x, 0.0).normalize();
                mesh.quad(Vec3::new(a.x, a.y, back), Vec3::new(b.x, b.y, back), Vec3::new(b.x, b.y, front), Vec3::new(a.x, a.y, front), normal);
                let (ca, cb) = (corner[i], corner[(i + 1) % n]);
                mesh.outline.extend([ca, cb, cb, ca]);
            }
        }
        mesh
    }
}

/// The path's contours as polygons: lines as they are, curves in short straight steps.
fn contours(path: &Path) -> Vec<Vec<Vec2>> {
    let (mut out, mut current): (Vec<Vec<Vec2>>, Vec<Vec2>) = (Vec::new(), Vec::new());
    let v = |p: drawnui::skia::Point| Vec2::new(p.x, p.y);
    // Each record's last points are its own (the start of a segment is the contour's last point).
    for rec in path.iter() {
        let points = rec.points();
        let last = |k: usize| v(points[points.len() - k]);
        match rec.verb() {
            PathVerb::Move => {
                if current.len() > 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                current.push(last(1));
            }
            PathVerb::Line => current.push(last(1)),
            PathVerb::Quad | PathVerb::Conic => {
                let (a, b, c) = (current.last().copied().unwrap_or(last(2)), last(2), last(1));
                current.extend((1..=12).map(|i| i as f32 / 12.0).map(|t| a * (1.0 - t) * (1.0 - t) + b * 2.0 * t * (1.0 - t) + c * t * t));
            }
            PathVerb::Cubic => {
                let (a, b, c, d) = (current.last().copied().unwrap_or(last(3)), last(3), last(2), last(1));
                current.extend((1..=16).map(|i| i as f32 / 16.0).map(|t| {
                    let u = 1.0 - t;
                    a * u * u * u + b * 3.0 * u * u * t + c * 3.0 * u * t * t + d * t * t * t
                }));
            }
            PathVerb::Close => {
                if current.len() > 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() > 2 {
        out.push(current);
    }
    // A closing point equal to the first one is dropped.
    for c in &mut out {
        if c.len() > 3 && c[0].distance_squared(c[c.len() - 1]) < 1e-8 {
            c.pop();
        }
    }
    out
}

fn signed_area(polygon: &[Vec2]) -> f32 {
    polygon.iter().zip(polygon.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() / 2.0
}

/// Triangles of a simple counter-clockwise polygon, by clipping ears.
fn ear_clip(polygon: &[Vec2]) -> Vec<[u32; 3]> {
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    // Strictly inside: a point on an edge (two points of the outline in a line) does not block an ear.
    let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| cross(a, b, p) > 1e-9 && cross(b, c, p) > 1e-9 && cross(c, a, p) > 1e-9;
    let mut left: Vec<usize> = (0..polygon.len()).collect();
    let mut out = Vec::new();
    let mut guard = 0;
    while left.len() > 3 && guard < polygon.len() * polygon.len() {
        guard += 1;
        let n = left.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (polygon[left[(i + n - 1) % n]], polygon[left[i]], polygon[left[(i + 1) % n]]);
            cross(a, b, c) > 1e-9 && left.iter().all(|&j| [left[(i + n - 1) % n], left[i], left[(i + 1) % n]].contains(&j) || !inside(polygon[j], a, b, c))
        });
        // None found (a pinched outline): the most convex corner goes anyway, so no hole is left.
        let Some(i) = ear.or_else(|| {
            (0..n).max_by(|&i, &j| {
                let c = |i: usize| cross(polygon[left[(i + n - 1) % n]], polygon[left[i]], polygon[left[(i + 1) % n]]);
                c(i).total_cmp(&c(j))
            })
        }) else {
            break;
        };
        out.push([left[(i + n - 1) % n] as u32, left[i] as u32, left[(i + 1) % n] as u32]);
        left.remove(i);
    }
    if left.len() == 3 {
        out.push([left[0] as u32, left[1] as u32, left[2] as u32]);
    }
    out
}

struct Part {
    name: String,
    mesh: Mesh3D,
    color: Color,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    parent: Option<String>,
}

/// A 3D model made in code: named, colored parts, each a mesh with its own place (`at`, `turned`,
/// `scaled`) and optionally hung under another part (`inside`), for `SkiaModel3D::from_model` and
/// `SkiaScene3D`'s `model_from`. The calls after `part` change the part just added.
#[derive(Default)]
pub struct Model3D {
    parts: Vec<Part>,
}

impl Model3D {
    pub fn new() -> Model3D {
        Model3D::default()
    }

    /// Adds a part: a mesh in one color. Its name is its node's and its material's.
    pub fn part(mut self, name: impl Into<String>, mesh: Mesh3D, color: Color) -> Model3D {
        self.parts.push(Part { name: name.into(), mesh, color, translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE, parent: None });
        self
    }

    fn last(&mut self) -> Option<&mut Part> {
        self.parts.last_mut()
    }

    /// Places the last part (in its parent's units).
    pub fn at(mut self, x: f32, y: f32, z: f32) -> Model3D {
        if let Some(p) = self.last() {
            p.translation = Vec3::new(x, y, z);
        }
        self
    }

    /// Turns the last part: degrees around x, y, z (in that order).
    pub fn turned(mut self, x: f32, y: f32, z: f32) -> Model3D {
        if let Some(p) = self.last() {
            p.rotation = Quat::from_euler(glam::EulerRot::XYZ, x.to_radians(), y.to_radians(), z.to_radians());
        }
        self
    }

    /// Scales the last part.
    pub fn scaled(mut self, x: f32, y: f32, z: f32) -> Model3D {
        if let Some(p) = self.last() {
            p.scale = Vec3::new(x, y, z);
        }
        self
    }

    /// Hangs the last part under another one by name: it moves, turns and hides with it.
    pub fn inside(mut self, parent: impl Into<String>) -> Model3D {
        if let Some(p) = self.last() {
            p.parent = Some(parent.into());
        }
        self
    }

    pub(super) fn to_scene(&self) -> Scene {
        let linear = |v: u8| (v as f32 / 255.0).powf(2.2);
        let nodes = self
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| Node {
                name: p.name.clone(),
                parent: p.parent.as_ref().and_then(|name| self.parts.iter().position(|q| &q.name == name)).filter(|j| *j != i),
                translation: p.translation,
                rotation: p.rotation,
                scale: p.scale,
                mesh: Some(i),
                skin: None,
            })
            .collect();
        let meshes = self
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let n = p.mesh.positions.len();
                vec![Primitive {
                    positions: p.mesh.positions.clone(),
                    normals: p.mesh.normals.clone(),
                    // Only a mesh that gave one for every vertex keeps its own outline directions.
                    outline_normals: if p.mesh.outline.len() == n { p.mesh.outline.clone() } else { Vec::new() },
                    uvs: vec![[0.0, 0.0]; n],
                    joints: Vec::new(),
                    weights: Vec::new(),
                    indices: p.mesh.indices.clone(),
                    material: Some(i),
                }]
            })
            .collect();
        let materials = self
            .parts
            .iter()
            .map(|p| Material {
                name: p.name.clone(),
                color: [linear(p.color.r()), linear(p.color.g()), linear(p.color.b()), p.color.a() as f32 / 255.0],
                texture: None,
                double_sided: false,
                blend: p.color.a() < 255,
            })
            .collect();
        Scene::assemble(nodes, meshes, materials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star() -> Path {
        let mut b = drawnui::skia::PathBuilder::new();
        for i in 0..10 {
            let (r, a) = (if i % 2 == 0 { 50.0 } else { 20.0 }, i as f32 * TAU / 10.0 - TAU / 4.0);
            let p = (50.0 + r * a.cos(), 50.0 + r * a.sin());
            if i == 0 { b.move_to(p) } else { b.line_to(p) };
        }
        b.close();
        b.detach()
    }

    /// Every triangle faces the way its vertices' normals say (outward, seen from outside).
    fn faces_outward(mesh: &Mesh3D) -> bool {
        mesh.indices.chunks(3).all(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.positions[i as usize]));
            let face = (b - a).cross(c - a);
            face.length() < 1e-9 || face.dot(Vec3::from(mesh.normals[t[0] as usize])) > 0.0
        })
    }

    #[test]
    fn shapes_face_outward() {
        for (name, mesh) in [("sphere", Mesh3D::sphere(1.0)), ("cylinder", Mesh3D::cylinder(1.0, 0.3)), ("torus", Mesh3D::torus(1.0, 0.2)), ("box", Mesh3D::cuboid(1.0, 2.0, 0.5)), ("star", Mesh3D::extrude(&star(), 10.0))] {
            assert!(!mesh.indices.is_empty(), "{name}");
            assert!(faces_outward(&mesh), "{name}");
        }
    }

    #[test]
    fn a_shape_with_points_in_a_line_is_cut_without_holes() {
        // A heart: its top dip and its bottom tip share x = 50.
        let mut b = drawnui::skia::PathBuilder::new();
        b.move_to((50.0, 88.0));
        b.cubic_to((20.0, 65.0), (0.0, 45.0), (0.0, 28.0));
        b.cubic_to((0.0, 12.0), (12.0, 0.0), (27.0, 0.0));
        b.cubic_to((38.0, 0.0), (46.0, 7.0), (50.0, 15.0));
        b.cubic_to((54.0, 7.0), (62.0, 0.0), (73.0, 0.0));
        b.cubic_to((88.0, 0.0), (100.0, 12.0), (100.0, 28.0));
        b.cubic_to((100.0, 45.0), (80.0, 65.0), (50.0, 88.0));
        b.close();
        let outline = contours(&b.detach()).remove(0);
        let triangles = ear_clip(&outline);
        assert_eq!(triangles.len(), outline.len() - 2, "every corner but two is cut off once");
        let area: f32 = triangles.iter().map(|[a, b, c]| (outline[*b as usize] - outline[*a as usize]).perp_dot(outline[*c as usize] - outline[*a as usize]).abs() / 2.0).sum();
        assert!((area - signed_area(&outline).abs()).abs() < 1.0, "the triangles cover the shape: {area}");
        let flipped: Vec<_> = triangles.iter().filter(|[a, b, c]| (outline[*b as usize] - outline[*a as usize]).perp_dot(outline[*c as usize] - outline[*a as usize]) < 0.0).collect();
        assert!(flipped.is_empty(), "triangles turned the wrong way: {flipped:?} of {}", outline.len());
    }

    #[test]
    fn a_star_extrudes_into_caps_and_walls() {
        let mesh = Mesh3D::extrude(&star(), 10.0);
        // Two caps of 10 - 2 triangles, ten walls of two triangles.
        assert_eq!(mesh.indices.len() / 3, 2 * 8 + 10 * 2);
    }

    #[test]
    fn parts_become_named_nodes_and_materials() {
        let scene = Model3D::new()
            .part("disc", Mesh3D::cylinder(1.0, 0.2), Color::from_rgb(245, 184, 46))
            .turned(90.0, 0.0, 0.0)
            .part("star", Mesh3D::extrude(&star(), 10.0), Color::WHITE)
            .inside("disc")
            .to_scene();
        assert_eq!(scene.nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["disc", "star"]);
        assert_eq!(scene.nodes[1].parent, Some(0));
        assert_eq!(scene.materials[1].name, "star");
        let (lo, hi) = scene.bounds;
        assert!((hi - lo).min_element() > 0.0);
    }
}
