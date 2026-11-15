//! A glTF scene on the CPU: read from a `.glb`, posed by its animations, hit by rays. One read
//! per file, shared by every control that shows it.
// Some of it is read by the GPU side only, which is the browser's for now.
#![cfg_attr(not(target_os = "emscripten"), allow(dead_code))]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use glam::{Mat4, Quat, Vec3};
use drawnui::skia::Data;

/// An animation playing: its index and seconds into it.
#[derive(Clone, Copy)]
pub(super) struct Track {
    pub(super) animation: usize,
    pub(super) time: f32,
    pub(super) looped: bool,
}

/// A material changed from code (`set_material_color`, `set_material_visible`, `set_material_image`).
#[derive(Clone, Default)]
pub(super) struct MaterialOverride {
    pub(super) color: Option<[f32; 4]>,
    pub(super) hidden: bool,
    pub(super) image: Option<Texture3D>,
    pub(super) roughness: Option<f32>,
}

/// An image to put on a material (`set_material_image`): a PNG / JPEG / WebP file, or any Skia
/// image (a photo, a pattern drawn in code). Cheap to clone: the GPU keeps one copy of it however
/// many materials and controls show it. It wraps by the surface's texture coordinates (glTF's, or
/// those of the shapes made in code) and is multiplied by the material's color.
#[derive(Clone)]
pub struct Texture3D(pub(super) Rc<(Vec<u8>, u32, u32)>);

impl Texture3D {
    /// From encoded bytes (PNG, JPEG, WebP); `None` when they are not an image.
    pub fn decode(encoded: &[u8]) -> Option<Texture3D> {
        decode_image(encoded).map(|i| Texture3D(Rc::new(i)))
    }

    /// From a Skia image in memory (decoded, or drawn on a raster surface); `None` for an image that
    /// lives on the GPU only.
    pub fn from_image(image: &drawnui::skia::Image) -> Option<Texture3D> {
        pixels(image).map(|i| Texture3D(Rc::new(i)))
    }

    /// Width and height in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.0.1, self.0.2)
    }
}

thread_local! {
    /// Files read, by their bytes' address and length: every control showing the same bytes
    /// (an `include_bytes!` constant) shares one scene, and the GPU keeps one copy of it.
    static SCENES: RefCell<HashMap<(usize, usize), Weak<Scene>>> = RefCell::new(HashMap::new());
}

/// The scene of these bytes, read once while any control holds it.
/// What a control does with a scene from a url once it is read (or could not be).
pub(super) type UrlDone = Box<dyn FnOnce(&mut drawnui::Cx<'_>, Result<Rc<Scene>, String>)>;

enum UrlEntry {
    Loaded(Weak<Scene>),
    Loading(Vec<UrlDone>),
}

thread_local! {
    /// Scenes from urls: one fetch and one copy per url while any control holds it.
    static URLS: RefCell<HashMap<String, UrlEntry>> = RefCell::new(HashMap::new());
}

/// The scene of `url`: at once when a control holds it already (`Some`), else `done` runs when it
/// arrives. Every control asking for a url while it loads waits for the same fetch, and they
/// share the one copy read from it. A failure is not remembered: the next request fetches again.
pub(super) fn load_url(cx: &mut drawnui::Cx<'_>, url: &str, done: UrlDone) -> Option<Result<Rc<Scene>, String>> {
    let fetch = URLS.with_borrow_mut(|urls| {
        if let Some(UrlEntry::Loaded(scene)) = urls.get(url)
            && let Some(scene) = scene.upgrade()
        {
            return Err(scene);
        }
        match urls.get_mut(url) {
            Some(UrlEntry::Loading(waiting)) => {
                waiting.push(done);
                Ok(false)
            }
            _ => {
                urls.retain(|_, entry| !matches!(entry, UrlEntry::Loaded(scene) if scene.strong_count() == 0));
                urls.insert(url.to_owned(), UrlEntry::Loading(vec![done]));
                Ok(true)
            }
        }
    });
    match fetch {
        Err(scene) => return Some(Ok(scene)),
        Ok(false) => return None,
        Ok(true) => {}
    }
    let key = url.to_owned();
    cx.fetch_bytes(url, move |cx, bytes| {
        let read = if bytes.is_empty() { Err("the url could not be loaded".to_owned()) } else { Scene::load(&bytes).map(Rc::new) };
        let waiting = URLS.with_borrow_mut(|urls| {
            let waiting = match urls.remove(&key) {
                Some(UrlEntry::Loading(waiting)) => waiting,
                _ => Vec::new(),
            };
            if let Ok(scene) = &read {
                urls.insert(key, UrlEntry::Loaded(Rc::downgrade(scene)));
            }
            waiting
        });
        for done in waiting {
            done(cx, read.clone());
        }
    });
    None
}

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
    /// The directions an outline pushes each vertex along; empty = the normals averaged per position.
    pub(super) outline_normals: Vec<[f32; 3]>,
    pub(super) uvs: Vec<[f32; 2]>,
    pub(super) joints: Vec<[f32; 4]>,
    pub(super) weights: Vec<[f32; 4]>,
    pub(super) indices: Vec<u32>,
    pub(super) material: Option<usize>,
}

pub(super) struct Material {
    pub(super) name: String,
    pub(super) color: [f32; 4],
    /// glTF roughness: 1 = matte (no shine), toward 0 = a tighter, brighter highlight.
    pub(super) roughness: f32,
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
    /// The box and the outline turned around the vertical (`Profile`) at rest (0) and through each
    /// animation (1 + its index), sampled, worked out when a viewer first frames them.
    extents: Vec<std::cell::OnceCell<Extent>>,
}

/// The box around a model at rest or through one animation, and its outline turned around the vertical.
pub(super) struct Extent {
    pub(super) min: Vec3,
    pub(super) max: Vec3,
    pub(super) profile: Profile,
}

/// A model's outline turned around its vertical axis: for each height band, the farthest it reaches
/// from the axis. What a camera that may turn the model any way around the vertical keeps in view
/// (tighter than the box: the top of a head and the tip of a punch are not one corner).
pub(super) struct Profile {
    pub(super) axis: glam::Vec2,
    y0: f32,
    step: f32,
    /// Per band: reach, lowest and highest point (reach < 0: empty).
    bands: [(f32, f32, f32); PROFILE_BANDS],
}

const PROFILE_BANDS: usize = 48;

impl Profile {
    fn new(axis: glam::Vec2, y0: f32, y1: f32) -> Profile {
        Profile { axis, y0, step: (y1 - y0).max(1e-3) / PROFILE_BANDS as f32, bands: [(-1.0, 0.0, 0.0); PROFILE_BANDS] }
    }

    fn add(&mut self, p: Vec3) {
        let band = (((p.y - self.y0) / self.step) as i32).clamp(0, PROFILE_BANDS as i32 - 1) as usize;
        let reach = glam::Vec2::new(p.x - self.axis.x, p.z - self.axis.y).length();
        let b = &mut self.bands[band];
        *b = if b.0 < 0.0 { (reach, p.y, p.y) } else { (b.0.max(reach), b.1.min(p.y), b.2.max(p.y)) };
    }

    /// Its points (reach, height): each band's reach at the band's lowest and highest point.
    pub(super) fn points(&self, into: &mut Vec<glam::Vec2>) {
        for &(reach, low, high) in self.bands.iter().filter(|b| b.0 >= 0.0) {
            into.push(glam::Vec2::new(reach, low));
            into.push(glam::Vec2::new(reach, high));
        }
    }
}

/// Poses sampled along each animation for its box: the view fits what the model does, not only its rest.
const BOUNDS_SAMPLES: usize = 8;
/// The vertical field of view of a viewer.
const FOV_DEGREES: f32 = 35.0;
/// A little air around a fitted model: it never touches the edges of its view.
const AIR: f32 = 1.1;

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
                    roughness: pbr.roughness_factor(),
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
                        let mut primitive = Primitive { positions, normals, outline_normals: Vec::new(), uvs, joints, weights, indices, material: p.material().index() };
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

        let animations: Vec<Animation> = gltf
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

        let extents = (0..=animations.len()).map(|_| Default::default()).collect();
        let mut scene = Scene { nodes, meshes, materials, images, skins, animations, bounds: (Vec3::ZERO, Vec3::ZERO), extents };
        scene.bounds = scene.rest_bounds();
        Ok(scene)
    }

    /// A scene from parts made in code (`Model3D`): no skins, no animations, no images.
    pub(super) fn assemble(nodes: Vec<Node>, meshes: Vec<Vec<Primitive>>, materials: Vec<Material>) -> Scene {
        let extents = vec![Default::default()];
        let mut scene = Scene { nodes, meshes, materials, images: Vec::new(), skins: Vec::new(), animations: Vec::new(), bounds: (Vec3::ZERO, Vec3::ZERO), extents };
        scene.bounds = scene.rest_bounds();
        scene
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
        let (lo, hi) = self.sweep(None, &mut |_| {});
        if lo.x > hi.x { (Vec3::splat(-1.0), Vec3::splat(1.0)) } else { (lo, hi) }
    }

    /// Every vertex of the model at rest (`None`) or through one animation (sampled) to `each`;
    /// returns their box (empty: min above max).
    fn sweep(&self, animation: Option<usize>, each: &mut dyn FnMut(Vec3)) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut add = |pose: Vec<Mat4>| {
            for (i, node) in self.nodes.iter().enumerate() {
                let Some(mesh) = node.mesh.and_then(|m| self.meshes.get(m)) else { continue };
                for p in mesh {
                    for v in self.posed_positions(i, p, &pose) {
                        (lo, hi) = (lo.min(v), hi.max(v));
                        each(v);
                    }
                }
            }
        };
        match animation {
            None => add(self.pose(None, 0.0)),
            Some(a) => {
                let duration = self.animations[a].duration;
                for s in 0..=BOUNDS_SAMPLES {
                    add(self.pose(Some(a), duration * s as f32 / BOUNDS_SAMPLES as f32 * 0.999));
                }
            }
        }
        (lo, hi)
    }

    /// The box and outline at rest (`None`) or through one animation, worked out once.
    pub(super) fn extent(&self, animation: Option<usize>) -> &Extent {
        let slot = animation.map_or(0, |a| a + 1);
        self.extents[slot].get_or_init(|| {
            let (lo, hi) = self.bounds;
            let height = (hi.y - lo.y).max(1e-3);
            let mut profile = Profile::new(glam::Vec2::new((lo.x + hi.x) / 2.0, (lo.z + hi.z) / 2.0), lo.y - height, hi.y + height);
            let (min, max) = self.sweep(animation, &mut |v| profile.add(v));
            let (min, max) = if min.x > max.x { self.bounds } else { (min, max) };
            Extent { min, max, profile }
        })
    }

    /// View and projection for a view nobody turns: the box `min..max` (`margin` scene units
    /// around it) fitted exactly as it is seen from this yaw and pitch.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn camera_as_seen(min: Vec3, max: Vec3, yaw: f32, pitch: f32, zoom: f32, aspect: f32, margin: f32) -> Mat4 {
        let center = (min + max) * 0.5;
        let half = (max - min) * 0.5 + Vec3::splat(margin.max(0.0));
        let tan_v = (FOV_DEGREES.to_radians() * 0.5).tan();
        let tan_h = tan_v * aspect;
        let rotation = turn(yaw, pitch);
        let (right, upward, back) = (rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z);
        let mut fit = 1e-3f32;
        for i in 0..8 {
            let corner = Vec3::new(if i & 1 == 0 { -half.x } else { half.x }, if i & 2 == 0 { -half.y } else { half.y }, if i & 4 == 0 { -half.z } else { half.z });
            let near = corner.dot(back);
            fit = fit.max((near + corner.dot(right).abs() / tan_h).max(near + corner.dot(upward).abs() / tan_v));
        }
        Scene::camera_at(center, half.length(), fit * AIR * zoom, yaw, pitch, aspect)
    }

    /// How far the camera stands (before zoom) so an outline turned around the vertical (`Profile`
    /// points: reach from the axis, height) stays in view at any yaw and at every pitch from
    /// `pitch_from` to `pitch_to`: across, each band's widest turn; up and down, the band tilted.
    pub(super) fn outline_fit(outline: &[glam::Vec2], center_y: f32, pitch_from: f32, pitch_to: f32, aspect: f32, margin: f32) -> f32 {
        let tan_v = (FOV_DEGREES.to_radians() * 0.5).tan();
        let tan_h = tan_v * aspect;
        let mut fit = 1e-3f32;
        let steps = (((pitch_to - pitch_from) / 10.0).ceil() as i32).max(1);
        for k in 0..=steps {
            let tilt = (pitch_from + (pitch_to - pitch_from) * k as f32 / steps as f32).to_radians();
            let (sin, cos) = tilt.sin_cos();
            let across = (cos * cos + 1.0 / (tan_h * tan_h)).sqrt();
            for point in outline {
                // A margin around the box widens the reach by up to the margin's diagonal.
                let reach = point.x + margin * 1.415;
                let mut y = point.y - center_y;
                y += y.signum() * margin;
                // Up and down: the point swung to the front and to the back of the axis.
                for side in [-1.0, 1.0] {
                    let c = side * reach;
                    let (near, up) = (c * cos + y * sin, y * cos - c * sin);
                    fit = fit.max(near + up.abs() / tan_v);
                }
                // Across: swung to the side, at its nearest.
                fit = fit.max(reach * across + y * sin);
            }
        }
        fit * AIR
    }

    /// View and projection looking at `center` from `distance`, turned by yaw / pitch.
    pub(super) fn camera_at(center: Vec3, radius: f32, distance: f32, yaw: f32, pitch: f32, aspect: f32) -> Mat4 {
        let radius = radius.max(1e-3);
        let eye = center + turn(yaw, pitch) * Vec3::new(0.0, 0.0, distance);
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let proj = Mat4::perspective_rh_gl(FOV_DEGREES.to_radians(), aspect, (distance - radius * 2.0).max(distance * 0.01), distance + radius * 2.0);
        proj * view
    }

}

/// glam: from_rotation_y(yaw) * from_rotation_x(-pitch), applied to (0, 0, distance).
fn turn(yaw: f32, pitch: f32) -> Quat {
    Quat::from_rotation_y(yaw.to_radians()) * Quat::from_rotation_x(-pitch.to_radians())
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
    pixels(&drawnui::skia::images::deferred_from_encoded_data(Data::new_copy(encoded), None)?)
}

/// An image's pixels as unpremultiplied RGBA.
fn pixels(image: &drawnui::skia::Image) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = (image.width(), image.height());
    let info = drawnui::skia::ImageInfo::new((w, h), drawnui::skia::ColorType::RGBA8888, drawnui::skia::AlphaType::Unpremul, None);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    image.read_pixels(&info, &mut pixels, (w * 4) as usize, (0, 0), drawnui::skia::image::CachingHint::Disallow).then_some((pixels, w as u32, h as u32))
}

