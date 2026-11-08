//! A glTF scene on the CPU: read from a `.glb`, posed by its animations, hit by rays. One read
//! per file, shared by every control that shows it.
// Some of it is read by the GPU side only, which is the browser's for now.
#![cfg_attr(not(target_os = "emscripten"), allow(dead_code))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use glam::{Mat4, Quat, Vec3};
use skia_safe::Data;

/// An animation playing: its index and seconds into it.
#[derive(Clone, Copy)]
pub(super) struct Track {
    pub(super) animation: usize,
    pub(super) time: f32,
    pub(super) looped: bool,
}

/// A material changed from code (`set_material_color`, `set_material_visible`).
#[derive(Clone, Copy, Default)]
pub(super) struct MaterialOverride {
    pub(super) color: Option<[f32; 4]>,
    pub(super) hidden: bool,
}

thread_local! {
    /// Files read, by their bytes' address and length: every control showing the same bytes
    /// (an `include_bytes!` constant) shares one scene, and the GPU keeps one copy of it.
    static SCENES: RefCell<HashMap<(usize, usize), Weak<Scene>>> = RefCell::new(HashMap::new());
}

/// The scene of these bytes, read once while any control holds it.
pub(super) fn shared(bytes: &[u8]) -> Result<Rc<Scene>, String> {
    let key = (bytes.as_ptr() as usize, bytes.len());
    if let Some(scene) = SCENES.with_borrow(|s| s.get(&key).and_then(Weak::upgrade)) {
        return Ok(scene);
    }
    let scene = Rc::new(Scene::load(bytes)?);
    SCENES.with_borrow_mut(|s| {
        s.retain(|_, w| w.strong_count() > 0);
        s.insert(key, Rc::downgrade(&scene));
    });
    Ok(scene)
}


pub(super) struct Node {
    pub(super) name: String,
    pub(super) parent: Option<usize>,
    pub(super) translation: Vec3,
    pub(super) rotation: Quat,
    pub(super) scale: Vec3,
    pub(super) mesh: Option<usize>,
    pub(super) skin: Option<usize>,
}

pub(super) struct Primitive {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) normals: Vec<[f32; 3]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) joints: Vec<[f32; 4]>,
    pub(super) weights: Vec<[f32; 4]>,
    pub(super) indices: Vec<u32>,
    pub(super) material: Option<usize>,
}

pub(super) struct Material {
    pub(super) name: String,
    pub(super) color: [f32; 4],
    pub(super) texture: Option<usize>,
    pub(super) double_sided: bool,
    pub(super) blend: bool,
}

pub(super) struct Skin {
    pub(super) joints: Vec<usize>,
    pub(super) inverse_bind: Vec<Mat4>,
}

pub(super) enum Path {
    Translation,
    Rotation,
    Scale,
}

pub(super) struct Channel {
    pub(super) node: usize,
    pub(super) path: Path,
    pub(super) step: bool,
    pub(super) times: Vec<f32>,
    /// 3 or 4 floats per key.
    pub(super) values: Vec<f32>,
}

pub(super) struct Animation {
    pub(super) name: String,
    pub(super) duration: f32,
    pub(super) channels: Vec<Channel>,
}

pub(super) struct Scene {
    pub(super) nodes: Vec<Node>,
    pub(super) meshes: Vec<Vec<Primitive>>,
    pub(super) materials: Vec<Material>,
    /// Decoded RGBA, width, height.
    pub(super) images: Vec<(Vec<u8>, u32, u32)>,
    pub(super) skins: Vec<Skin>,
    pub(super) animations: Vec<Animation>,
    /// The scene's box at rest.
    pub(super) bounds: (Vec3, Vec3),
}

impl Scene {
    pub(super) fn load(bytes: &[u8]) -> Result<Scene, String> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
        let blob = gltf.blob.clone();
        let buffers: Vec<Vec<u8>> = gltf
            .buffers()
            .map(|b| match b.source() {
                gltf::buffer::Source::Bin => blob.clone().ok_or_else(|| "no binary chunk".to_owned()),
                gltf::buffer::Source::Uri(uri) => data_uri(uri).ok_or_else(|| format!("external buffer {uri} (use a .glb)")),
            })
            .collect::<Result<_, _>>()?;
        let get = |b: gltf::Buffer| buffers.get(b.index()).map(|v| v.as_slice());

        let images = gltf
            .images()
            .map(|image| {
                let encoded: Vec<u8> = match image.source() {
                    gltf::image::Source::View { view, .. } => {
                        let data = &buffers[view.buffer().index()];
                        data[view.offset()..view.offset() + view.length()].to_vec()
                    }
                    gltf::image::Source::Uri { uri, .. } => data_uri(uri).unwrap_or_default(),
                };
                decode_image(&encoded).unwrap_or_else(|| (vec![255; 4], 1, 1))
            })
            .collect();

        let materials = gltf
            .materials()
            .map(|m| {
                let pbr = m.pbr_metallic_roughness();
                Material {
                    name: m.name().unwrap_or_default().to_owned(),
                    color: pbr.base_color_factor(),
                    texture: pbr.base_color_texture().map(|t| t.texture().source().index()),
                    double_sided: m.double_sided(),
                    blend: m.alpha_mode() == gltf::material::AlphaMode::Blend,
                }
            })
            .collect();

        let meshes = gltf
            .meshes()
            .map(|mesh| {
                mesh.primitives()
                    .filter(|p| p.mode() == gltf::mesh::Mode::Triangles)
                    .filter_map(|p| {
                        let reader = p.reader(get);
                        let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
                        let n = positions.len();
                        let normals = reader.read_normals().map_or_else(|| vec![[0.0, 1.0, 0.0]; n], |r| r.collect());
                        let uvs = reader.read_tex_coords(0).map_or_else(|| vec![[0.0, 0.0]; n], |r| r.into_f32().collect());
                        let joints = reader.read_joints(0).map_or_else(Vec::new, |r| r.into_u16().map(|j| j.map(f32::from)).collect());
                        let weights = reader.read_weights(0).map_or_else(Vec::new, |r| r.into_f32().collect());
                        let indices = reader.read_indices().map_or_else(|| (0..n as u32).collect(), |r| r.into_u32().collect());
                        let mut primitive = Primitive { positions, normals, uvs, joints, weights, indices, material: p.material().index() };
                        // Vertex colors are folded into the material color only when uniform; a
                        // per-vertex color set is left out (ponytail above).
                        if primitive.joints.len() != n || primitive.weights.len() != n {
                            (primitive.joints, primitive.weights) = (Vec::new(), Vec::new());
                        }
                        Some(primitive)
                    })
                    .collect()
            })
            .collect();

        let skins = gltf
            .skins()
            .map(|skin| {
                let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
                let inverse_bind = skin
                    .reader(get)
                    .read_inverse_bind_matrices()
                    .map_or_else(|| vec![Mat4::IDENTITY; joints.len()], |r| r.map(|m| Mat4::from_cols_array_2d(&m)).collect());
                Skin { joints, inverse_bind }
            })
            .collect();

        let mut nodes: Vec<Node> = gltf
            .nodes()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                Node {
                    name: n.name().unwrap_or_default().to_owned(),
                    parent: None,
                    translation: Vec3::from(t),
                    rotation: Quat::from_array(r),
                    scale: Vec3::from(s),
                    mesh: n.mesh().map(|m| m.index()),
                    skin: n.skin().map(|s| s.index()),
                }
            })
            .collect();
        for n in gltf.nodes() {
            for child in n.children() {
                nodes[child.index()].parent = Some(n.index());
            }
        }

        let animations = gltf
            .animations()
            .enumerate()
            .map(|(i, a)| {
                let mut duration: f32 = 0.0;
                let channels = a
                    .channels()
                    .filter_map(|c| {
                        let reader = c.reader(get);
                        let times: Vec<f32> = reader.read_inputs()?.collect();
                        duration = duration.max(times.last().copied().unwrap_or(0.0));
                        let (path, values): (Path, Vec<f32>) = match reader.read_outputs()? {
                            gltf::animation::util::ReadOutputs::Translations(v) => (Path::Translation, v.flatten().collect()),
                            gltf::animation::util::ReadOutputs::Rotations(v) => (Path::Rotation, v.into_f32().flatten().collect()),
                            gltf::animation::util::ReadOutputs::Scales(v) => (Path::Scale, v.flatten().collect()),
                            gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => return None,
                        };
                        let interpolation = c.sampler().interpolation();
                        let cubic = interpolation == gltf::animation::Interpolation::CubicSpline;
                        // Cubic spline keys hold (in tangent, value, out tangent): keep the values.
                        let width = if matches!(path, Path::Rotation) { 4 } else { 3 };
                        let values = if cubic { values.chunks(width * 3).flat_map(|k| k[width..width * 2].to_vec()).collect() } else { values };
                        Some(Channel { node: c.target().node().index(), path, step: interpolation == gltf::animation::Interpolation::Step, times, values })
                    })
                    .collect();
                Animation { name: a.name().map_or_else(|| format!("Animation {}", i + 1), str::to_owned), duration, channels }
            })
            .collect();

        let mut scene = Scene { nodes, meshes, materials, images, skins, animations, bounds: (Vec3::ZERO, Vec3::ZERO) };
        scene.bounds = scene.rest_bounds();
        Ok(scene)
    }

    /// The animation with this name.
    pub(super) fn animation_named(&self, name: &str) -> Option<usize> {
        self.animations.iter().position(|a| a.name == name)
    }

    /// Nodes hidden: those `hidden` says, with everything under them.
    pub(super) fn hidden_by(&self, hidden: impl Fn(usize) -> bool) -> Vec<bool> {
        (0..self.nodes.len()).map(|i| std::iter::successors(Some(i), |n| self.nodes[*n].parent).any(&hidden)).collect()
    }

    /// The name of the material a node's primitive is drawn with ("" when none).
    pub(super) fn material_of(&self, node: usize, primitive: usize) -> String {
        let material = self.nodes[node].mesh.and_then(|m| self.meshes[m].get(primitive)).and_then(|p| p.material);
        material.and_then(|m| self.materials.get(m)).map_or(String::new(), |m| m.name.clone())
    }

    /// Every node's own transform as the file gives it.
    pub(super) fn rest(&self) -> Vec<(Vec3, Quat, Vec3)> {
        self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect()
    }

    /// Writes an animation's values at its time into the nodes it moves; a looped one wraps, a
    /// once-only one stops at its end.
    pub(super) fn apply(&self, track: Track, local: &mut [(Vec3, Quat, Vec3)]) {
        let Some(a) = self.animations.get(track.animation) else { return };
        let t = match (a.duration > 0.0, track.looped) {
            (false, _) => 0.0,
            (true, true) => track.time.rem_euclid(a.duration),
            (true, false) => track.time.clamp(0.0, a.duration),
        };
        for c in &a.channels {
            let Some(l) = local.get_mut(c.node) else { continue };
            match c.path {
                Path::Translation => l.0 = Vec3::from_slice(&sample(c, t, 3)),
                Path::Rotation => l.1 = Quat::from_slice(&sample(c, t, 4)).normalize(),
                Path::Scale => l.2 = Vec3::from_slice(&sample(c, t, 3)),
            }
        }
    }

    /// Every node's world matrix from the local transforms.
    pub(super) fn world(&self, local: &[(Vec3, Quat, Vec3)]) -> Vec<Mat4> {
        let mut world = vec![None; self.nodes.len()];
        fn resolve(i: usize, nodes: &[Node], local: &[(Vec3, Quat, Vec3)], world: &mut [Option<Mat4>]) -> Mat4 {
            if let Some(m) = world[i] {
                return m;
            }
            let (t, r, s) = local[i];
            let own = Mat4::from_scale_rotation_translation(s, r, t);
            let m = match nodes[i].parent {
                Some(p) => resolve(p, nodes, local, world) * own,
                None => own,
            };
            world[i] = Some(m);
            m
        }
        (0..self.nodes.len()).map(|i| resolve(i, &self.nodes, local, &mut world)).collect()
    }

    /// The world matrices with one looped animation at this time (bounds, tests).
    pub(super) fn pose(&self, animation: Option<usize>, time: f32) -> Vec<Mat4> {
        let mut local = self.rest();
        if let Some(animation) = animation {
            self.apply(Track { animation, time, looped: true }, &mut local);
        }
        self.world(&local)
    }

    /// A primitive's vertices in the scene as posed: skinned by its joints, else moved by its node.
    pub(super) fn posed_positions(&self, node: usize, primitive: &Primitive, pose: &[Mat4]) -> Vec<Vec3> {
        let joints: Option<Vec<Mat4>> = self.nodes[node].skin.and_then(|s| self.skins.get(s)).map(|skin| skin.joints.iter().zip(&skin.inverse_bind).map(|(j, ibm)| pose[*j] * *ibm).collect());
        primitive
            .positions
            .iter()
            .enumerate()
            .map(|(v, position)| {
                let at = Vec3::from(*position);
                match (&joints, primitive.joints.get(v), primitive.weights.get(v)) {
                    (Some(joints), Some(j), Some(w)) => (0..4).fold(Vec3::ZERO, |sum, k| match joints.get(j[k] as usize) {
                        Some(m) => sum + m.transform_point3(at) * w[k],
                        None => sum,
                    }),
                    _ => pose[node].transform_point3(at),
                }
            })
            .collect()
    }

    /// The nearest surface a ray meets: node, primitive, distance. Hidden nodes and materials are
    /// passed through.
    pub(super) fn ray_hit(&self, pose: &[Mat4], hidden: &[bool], materials: &[MaterialOverride], from: Vec3, dir: Vec3) -> Option<(usize, usize, f32)> {
        let mut best: Option<(usize, usize, f32)> = None;
        for (i, node) in self.nodes.iter().enumerate() {
            let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)).filter(|_| !hidden[i]) else { continue };
            for (p, primitive) in mesh.iter().enumerate() {
                if primitive.material.and_then(|m| materials.get(m)).is_some_and(|m| m.hidden) {
                    continue;
                }
                let at = self.posed_positions(i, primitive, pose);
                for tri in primitive.indices.chunks_exact(3) {
                    let (Some(a), Some(b), Some(c)) = (at.get(tri[0] as usize), at.get(tri[1] as usize), at.get(tri[2] as usize)) else { continue };
                    if let Some(t) = ray_triangle(from, dir, *a, *b, *c)
                        && best.is_none_or(|(_, _, d)| t < d)
                    {
                        best = Some((i, p, t));
                    }
                }
            }
        }
        best
    }

    pub(super) fn rest_bounds(&self) -> (Vec3, Vec3) {
        let pose = self.pose(None, 0.0);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (i, node) in self.nodes.iter().enumerate() {
            let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)) else { continue };
            for p in mesh {
                for v in self.posed_positions(i, p, &pose) {
                    (lo, hi) = (lo.min(v), hi.max(v));
                }
            }
        }
        if lo.x > hi.x { (Vec3::splat(-1.0), Vec3::splat(1.0)) } else { (lo, hi) }
    }

    /// View and projection that fit the scene's box, turned by yaw / pitch.
    pub(super) fn camera(&self, yaw: f32, pitch: f32, zoom: f32, aspect: f32) -> Mat4 {
        let (lo, hi) = self.bounds;
        let center = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1e-3);
        let fov = 35f32.to_radians();
        let fit = radius / (fov * 0.5).sin() * if aspect < 1.0 { 1.0 / aspect } else { 1.0 };
        let distance = fit * zoom;
        let rotation = Quat::from_rotation_y(yaw.to_radians()) * Quat::from_rotation_x(-pitch.to_radians());
        let eye = center + rotation * Vec3::new(0.0, 0.0, distance);
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let proj = Mat4::perspective_rh_gl(fov, aspect, (distance - radius * 2.0).max(distance * 0.01), distance + radius * 2.0);
        proj * view
    }
}

/// The ray through a point of the view (`fx`, `fy` from 0 to 1, from the top left): its start on
/// the near plane and its direction.
pub(super) fn ray(view_proj: Mat4, fx: f32, fy: f32) -> (Vec3, Vec3) {
    let inverse = view_proj.inverse();
    let (nx, ny) = (fx * 2.0 - 1.0, 1.0 - fy * 2.0);
    let (near, far) = (inverse.project_point3(Vec3::new(nx, ny, -1.0)), inverse.project_point3(Vec3::new(nx, ny, 1.0)));
    (near, (far - near).normalize())
}

/// Where a ray meets a triangle, both sides (Moller-Trumbore): the distance along the ray.
pub(super) fn ray_triangle(from: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = from - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

/// The value of a channel at `t`, `width` floats.
pub(super) fn sample(c: &Channel, t: f32, width: usize) -> Vec<f32> {
    let keys = c.times.len();
    if keys == 0 || c.values.len() < keys * width {
        return vec![0.0; width];
    }
    let next = c.times.partition_point(|k| *k <= t);
    let at = |k: usize| &c.values[k * width..k * width + width];
    if next == 0 {
        return at(0).to_vec();
    }
    if next >= keys {
        return at(keys - 1).to_vec();
    }
    let (k0, k1) = (next - 1, next);
    if c.step {
        return at(k0).to_vec();
    }
    let span = (c.times[k1] - c.times[k0]).max(1e-6);
    let f = ((t - c.times[k0]) / span).clamp(0.0, 1.0);
    if width == 4 {
        let (a, b) = (Quat::from_slice(at(k0)), Quat::from_slice(at(k1)));
        return a.slerp(b, f).to_array().to_vec();
    }
    at(k0).iter().zip(at(k1)).map(|(a, b)| a + (b - a) * f).collect()
}

/// `data:...;base64,` contents.
pub(super) fn data_uri(uri: &str) -> Option<Vec<u8>> {
    let encoded = uri.strip_prefix("data:")?.split_once(";base64,")?.1;
    let mut out = Vec::with_capacity(encoded.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in encoded.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        (acc, bits) = ((acc << 6) | v as u32, bits + 6);
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// PNG / JPEG / WebP through Skia's codecs, as unpremultiplied RGBA.
pub(super) fn decode_image(encoded: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let image = skia_safe::images::deferred_from_encoded_data(Data::new_copy(encoded), None)?;
    let (w, h) = (image.width(), image.height());
    let info = skia_safe::ImageInfo::new((w, h), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Unpremul, None);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    image.read_pixels(&info, &mut pixels, (w * 4) as usize, (0, 0), skia_safe::image::CachingHint::Disallow).then_some((pixels, w as u32, h as u32))
}

