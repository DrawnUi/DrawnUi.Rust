//! SkiaModel3D (feature `model3d`): a glTF 2.0 scene (`.glb`) drawn in real 3D inside the UI: its
//! meshes with a depth buffer, textures, a light, skeletal animation. The scene is drawn with the
//! GPU context Skia uses into a texture of the control's size, which the control then shows as an
//! image: layout, gestures and everything drawn around it work as for any control. Drag to turn it.
//! The app drives the scene: animations looped, played once or layered, blended into each other;
//! nodes and materials by name (shown, moved, recolored); taps tell which part was hit.
//! WebGL2 (the browser) for now; elsewhere nothing is drawn yet.
// The GL side is the browser's for now: elsewhere the scene is read but not drawn.
#![cfg_attr(not(target_os = "emscripten"), allow(dead_code))]
// ponytail: one directional light + sky light, base color texture and factor only (no normal,
// metallic-roughness or emissive maps), no morph targets, 64 joints per skin, alpha blend drawn in
// tree order; CUBICSPLINE samplers read as linear. Add them when a delivered model needs them.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use glam::{Mat4, Quat, Vec3};
use skia_safe::{Color, Data, ISize, Paint, Rect, SamplingOptions};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, PaintCx, part_mut};
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{Build, ControlId, Cx, Mut, Raw, Tree, wrong_state};
use crate::types::Dirty;

/// Joint matrices a skinned mesh may use (WebGL2 guarantees 256 vertex uniform vectors).
const MAX_JOINTS: usize = 64;

/// What a tap on the scene hit (`on_tapped_3d`): the node and the material under the finger, and
/// the point in the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelHit {
    /// The glTF node's name ("" when it has none).
    pub node: String,
    /// The material's name ("" when it has none).
    pub material: String,
    /// Where the ray met the surface, in scene units.
    pub point: [f32; 3],
}

type EndedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &str)>;
type TappedHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &ModelHit)>;

/// An animation playing: its index and seconds into it.
#[derive(Clone, Copy)]
struct Track {
    animation: usize,
    time: f32,
    looped: bool,
}

/// A node's own transform set from code (`set_node_translation` and the rest): it replaces what
/// the file and the animations give that part.
#[derive(Clone, Copy, Default)]
struct NodeOverride {
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<Vec3>,
    hidden: bool,
}

/// A material changed from code (`set_material_color`, `set_material_visible`).
#[derive(Clone, Copy, Default)]
struct MaterialOverride {
    color: Option<[f32; 4]>,
    hidden: bool,
}

enum Event {
    Ended(String),
    Tapped(ModelHit),
}

/// A glTF scene drawn in 3D (see the module docs).
pub struct SkiaModel3D {
    id: ControlId,
    scene: Option<Rc<Scene>>,
    /// The file could not be read: why.
    error: Option<String>,
    /// GL objects, made at the first paint on a GL context.
    gl: RefCell<Option<GlScene>>,
    /// The animation asked for by name before the first frame (`Build::animation`).
    start_animation: Option<String>,
    /// The main animation.
    main: Option<Track>,
    /// The animation the main one fades from, and the seconds the fade has run.
    fading: Option<(Track, f32)>,
    /// Cross-fade length, seconds.
    fade: f32,
    /// A once-only animation goes back to this looping one when it ends.
    back_to: Option<usize>,
    /// Animations playing at the same time as the main one (other objects of the scene).
    layers: Vec<Track>,
    playing: bool,
    speed: f32,
    last_ms: Option<f64>,
    ticking: bool,
    nodes: Vec<NodeOverride>,
    materials: Vec<MaterialOverride>,
    events: Vec<Event>,
    on_ended: Option<EndedHandler>,
    on_tapped: Option<TappedHandler>,
    /// Turn around the vertical axis, degrees; tilt, degrees.
    yaw: f32,
    pitch: f32,
    /// Degrees per second the scene turns by itself (0 = still).
    auto_rotate: f32,
    /// Drags turn the scene.
    orbit: bool,
    /// Distance to the scene, as a factor of the distance that fits it (1 = fits).
    zoom: f32,
}

impl SkiaModel3D {
    /// A scene from the bytes of a `.glb` (or a `.gltf` with its buffers embedded). It plays its
    /// first animation in a loop; drags turn it.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(glb: &[u8]) -> Build<SkiaModel3D> {
        let (scene, error) = match Scene::load(glb) {
            Ok(scene) => (Some(Rc::new(scene)), None),
            Err(why) => (None, Some(why)),
        };
        let (nodes, materials) = scene.as_ref().map_or((0, 0), |s| (s.nodes.len(), s.materials.len()));
        let main = scene.as_ref().filter(|s| !s.animations.is_empty()).map(|_| Track { animation: 0, time: 0.0, looped: true });
        let model = SkiaModel3D {
            id: ControlId { index: 0, generation: 0 },
            scene,
            error,
            gl: RefCell::new(None),
            start_animation: None,
            main,
            fading: None,
            fade: 0.25,
            back_to: None,
            layers: Vec::new(),
            playing: true,
            speed: 1.0,
            last_ms: None,
            ticking: false,
            nodes: vec![NodeOverride::default(); nodes],
            materials: vec![MaterialOverride::default(); materials],
            events: Vec::new(),
            on_ended: None,
            on_tapped: None,
            yaw: 20.0,
            pitch: 10.0,
            auto_rotate: 0.0,
            orbit: true,
            zoom: 1.0,
        };
        let mut build = Build::new(model);
        let id = build.id();
        build.control_mut().id = id;
        build
    }

    /// The names of the scene's animations, in file order.
    pub fn animations(&self) -> Vec<String> {
        self.scene.as_ref().map_or(Vec::new(), |s| s.animations.iter().map(|a| a.name.clone()).collect())
    }

    /// The names of the scene's nodes (objects, bones, cameras), in file order.
    pub fn node_names(&self) -> Vec<String> {
        self.scene.as_ref().map_or(Vec::new(), |s| s.nodes.iter().map(|n| n.name.clone()).collect())
    }

    /// The names of the scene's materials (its parts by look), in file order.
    pub fn material_names(&self) -> Vec<String> {
        self.scene.as_ref().map_or(Vec::new(), |s| s.materials.iter().map(|m| m.name.clone()).collect())
    }

    /// The main animation playing, by name.
    pub fn current_animation(&self) -> Option<&str> {
        Some(self.scene.as_ref()?.animations.get(self.main?.animation)?.name.as_str())
    }

    /// The file could not be read: why (`None` when it was).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn animation_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.animations.iter().position(|a| a.name == name)
    }

    fn node_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.nodes.iter().position(|n| n.name == name)
    }

    fn material_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.materials.iter().position(|m| m.name == name)
    }

    /// Starts `animation` as the main one, fading from what played.
    fn start(&mut self, animation: usize, looped: bool) {
        if let Some(old) = self.main.filter(|_| self.fade > 0.0) {
            self.fading = Some((old, 0.0));
        }
        self.main = Some(Track { animation, time: 0.0, looped });
        self.playing = true;
    }

    fn wants_frames(&self) -> bool {
        self.scene.is_some()
            && (!self.events.is_empty() || self.fading.is_some() || self.auto_rotate != 0.0 || (self.playing && (self.main.is_some() || !self.layers.is_empty())))
    }

    fn update_ticking(&mut self, tree: &mut Tree) {
        if self.wants_frames() {
            if !std::mem::replace(&mut self.ticking, true) {
                self.last_ms = None;
                animators::start_frame(tree, self.id, tick);
            }
        } else if std::mem::take(&mut self.ticking) {
            animators::stop_frames(tree, self.id);
        }
    }

    /// One frame of time: tracks advance, a fade goes on, a once-only animation that ended is
    /// reported and goes back to the looping one.
    fn advance(&mut self, delta: f32) {
        let Some(scene) = self.scene.clone() else { return };
        self.yaw = (self.yaw + self.auto_rotate * delta) % 360.0;
        if !self.playing {
            return;
        }
        let step = delta * self.speed;
        if let Some((track, elapsed)) = &mut self.fading {
            track.time += step;
            *elapsed += delta;
            if *elapsed >= self.fade {
                self.fading = None;
            }
        }
        for layer in &mut self.layers {
            layer.time += step;
        }
        let Some(main) = &mut self.main else { return };
        main.time += step;
        let duration = scene.animations.get(main.animation).map_or(0.0, |a| a.duration);
        if !main.looped && main.time >= duration {
            main.time = duration;
            let ended = scene.animations[main.animation].name.clone();
            self.events.push(Event::Ended(ended));
            match self.back_to.take() {
                Some(back) => self.start(back, true),
                // Held on its last frame (a death).
                None => self.playing = !self.layers.is_empty(),
            }
        }
    }

    /// The local transform of every node for this frame: the file's, the animations' (blended
    /// while fading), then what code set.
    fn locals(&self, scene: &Scene) -> Vec<(Vec3, Quat, Vec3)> {
        let mut local = scene.rest();
        if let Some(main) = self.main {
            match self.fading {
                Some((from, elapsed)) => {
                    let mut old = local.clone();
                    scene.apply(from, &mut old);
                    scene.apply(main, &mut local);
                    let w = (elapsed / self.fade).clamp(0.0, 1.0);
                    for (l, o) in local.iter_mut().zip(&old) {
                        *l = (o.0.lerp(l.0, w), o.1.slerp(l.1, w), o.2.lerp(l.2, w));
                    }
                }
                None => scene.apply(main, &mut local),
            }
        }
        for layer in &self.layers {
            scene.apply(*layer, &mut local);
        }
        for (l, o) in local.iter_mut().zip(&self.nodes) {
            if let Some(t) = o.translation {
                l.0 = t;
            }
            if let Some(r) = o.rotation {
                l.1 = r;
            }
            if let Some(s) = o.scale {
                l.2 = s;
            }
        }
        local
    }

    /// Hidden nodes, a hidden parent hiding its children.
    fn hidden(&self, scene: &Scene) -> Vec<bool> {
        (0..scene.nodes.len())
            .map(|i| std::iter::successors(Some(i), |n| scene.nodes[*n].parent).any(|n| self.nodes.get(n).is_some_and(|o| o.hidden)))
            .collect()
    }

    fn camera(&self, scene: &Scene, size: (f32, f32)) -> Mat4 {
        scene.camera(self.yaw, self.pitch, self.zoom, size.0 / size.1.max(1.0))
    }

    /// What the ray through this point of the control (pixels from its corner) meets first.
    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> Option<ModelHit> {
        let scene = self.scene.as_ref()?;
        let pose = scene.world(&self.locals(scene));
        let inverse = self.camera(scene, size).inverse();
        let (nx, ny) = (x / size.0 * 2.0 - 1.0, 1.0 - y / size.1 * 2.0);
        let (near, far) = (inverse.project_point3(Vec3::new(nx, ny, -1.0)), inverse.project_point3(Vec3::new(nx, ny, 1.0)));
        let (node, primitive, t) = scene.ray_hit(&pose, &self.hidden(scene), &self.materials, near, (far - near).normalize())?;
        let material = scene.meshes[scene.nodes[node].mesh?][primitive].material.and_then(|m| scene.materials.get(m)).map_or(String::new(), |m| m.name.clone());
        let point = near + (far - near).normalize() * t;
        Some(ModelHit { node: scene.nodes[node].name.clone(), material, point: point.to_array() })
    }
}

/// One frame: time moves on, then the app hears what happened (an animation ended, a tap).
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut result = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return result };
    let mut queue = Vec::new();
    if let Some(model) = node.kind.as_deref_mut().and_then(part_mut::<SkiaModel3D>) {
        let delta = model.last_ms.map_or(0.0, |last| ((time_ms - last) / 1000.0) as f32);
        model.last_ms = Some(time_ms);
        model.advance(delta);
        let events = std::mem::take(&mut model.events);
        for event in events {
            let control = node.kind.as_deref_mut().expect("checked above");
            let model = part_mut::<SkiaModel3D>(control).expect("checked above");
            match event {
                Event::Ended(name) => {
                    if let Some(mut handler) = model.on_ended.take() {
                        let control = node.kind.as_deref_mut().expect("checked above");
                        handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, &name);
                        result.state_touched = true;
                        let model = node.kind.as_deref_mut().and_then(part_mut::<SkiaModel3D>).expect("checked above");
                        model.on_ended.get_or_insert(handler);
                    }
                }
                Event::Tapped(hit) => {
                    if let Some(mut handler) = model.on_tapped.take() {
                        let control = node.kind.as_deref_mut().expect("checked above");
                        handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, &hit);
                        result.state_touched = true;
                        let model = node.kind.as_deref_mut().and_then(part_mut::<SkiaModel3D>).expect("checked above");
                        model.on_tapped.get_or_insert(handler);
                    }
                }
            }
        }
        let model = node.kind.as_deref_mut().and_then(part_mut::<SkiaModel3D>).expect("checked above");
        result.keep = model.wants_frames();
        model.ticking = result.keep;
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    cx.tree.invalidate(id, Dirty::DRAW);
    result
}

impl Control for SkiaModel3D {
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if let Some(name) = self.start_animation.take()
            && let Some(animation) = self.animation_named(&name)
        {
            self.main = Some(Track { animation, time: 0.0, looped: true });
        }
        self.update_ticking(cx.tree);
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match gesture.kind {
            GestureKind::Down => Handled::Yes,
            GestureKind::Panning if self.orbit => {
                let scale = cx.base().scale.max(0.1);
                self.yaw += gesture.delta.x / scale * 0.5;
                self.pitch = (self.pitch + gesture.delta.y / scale * 0.3).clamp(-60.0, 80.0);
                cx.invalidate(Dirty::DRAW);
                Handled::Yes
            }
            GestureKind::Tapped if self.on_tapped.is_some() => {
                let rect = cx.base().rect;
                match self.hit(cx.point.x - rect.left, cx.point.y - rect.top, (rect.width(), rect.height())) {
                    Some(hit) => {
                        self.events.push(Event::Tapped(hit));
                        self.update_ticking(cx.tree);
                        Handled::Tapped
                    }
                    None => Handled::No,
                }
            }
            _ => Handled::No,
        }
    }

    fn paint(&self, cx: &mut PaintCx) {
        let rect = cx.rect;
        let size = ISize::new(rect.width().round() as i32, rect.height().round() as i32);
        if size.width <= 0 || size.height <= 0 {
            return;
        }
        let Some(scene) = self.scene.clone() else { return };
        let pose = scene.world(&self.locals(&scene));
        let view_proj = self.camera(&scene, (size.width as f32, size.height as f32));
        let hidden = self.hidden(&scene);
        let canvas = cx.canvas;
        let Some(gpu) = cx.gpu() else { return };
        let mut gl = self.gl.borrow_mut();
        if gl.as_ref().is_some_and(|g| g.epoch != gpu.epoch()) {
            // A new GL context: the old objects are gone with the old one.
            *gl = None;
        }
        let Some(image) = render::draw(&mut gl, gpu, &scene, &pose, &hidden, &self.materials, view_proj, size) else { return };
        canvas.draw_image_rect_with_sampling_options(&image, None, Rect::from_xywh(rect.left, rect.top, size.width as f32, size.height as f32), SamplingOptions::default(), &Paint::default());
    }
}

impl Build<SkiaModel3D> {
    /// The animation it loops from the start, by name (glTF animation name); the first one when
    /// not set.
    pub fn animation(mut self, name: impl Into<String>) -> Self {
        self.control_mut().start_animation = Some(name.into());
        self
    }

    /// Plays the animations (on by default).
    pub fn playing(mut self, on: bool) -> Self {
        self.control_mut().playing = on;
        self
    }

    /// Animation speed, 1 = as authored.
    pub fn speed(mut self, speed: f32) -> Self {
        self.control_mut().speed = speed;
        self
    }

    /// How long one animation blends into the next, milliseconds (250 by default, 0 = cut).
    pub fn crossfade_ms(mut self, ms: f32) -> Self {
        self.control_mut().fade = ms.max(0.0) / 1000.0;
        self
    }

    /// Turns the scene by itself, degrees per second.
    pub fn auto_rotate(mut self, degrees_per_second: f32) -> Self {
        self.control_mut().auto_rotate = degrees_per_second;
        self
    }

    /// Start angles: around the vertical axis and tilt, degrees.
    pub fn view_angles(mut self, yaw: f32, pitch: f32) -> Self {
        (self.control_mut().yaw, self.control_mut().pitch) = (yaw, pitch);
        self
    }

    /// Distance as a factor of the one that fits the scene (below 1: closer).
    pub fn zoom(mut self, zoom: f32) -> Self {
        self.control_mut().zoom = zoom.max(0.05);
        self
    }

    /// Drags turn the scene (on by default).
    pub fn orbit(mut self, on: bool) -> Self {
        self.control_mut().orbit = on;
        self
    }

    /// Runs when an animation played with `play_once` ended, with its name.
    pub fn on_animation_ended<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        self.control_mut().on_ended = Some(Box::new(move |raw, state, cx, name| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx, name)
        }));
        self
    }

    /// Runs when a tap lands on the scene's surface (not on empty space around it), with what it
    /// hit: the node, the material and the point.
    pub fn on_tapped_3d<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &ModelHit) + 'static) -> Self {
        self.control_mut().on_tapped = Some(Box::new(move |raw, state, cx, hit| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx, hit)
        }));
        self
    }
}

impl Mut<'_, SkiaModel3D> {
    /// Loops the animation with this name, blending into it; false when the scene has none such.
    pub fn play(&mut self, name: &str) -> bool {
        let model = self.control_mut();
        let Some(animation) = model.animation_named(name) else { return false };
        model.back_to = None;
        model.start(animation, true);
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    /// Plays the animation with this name once, then blends back into the looping one that
    /// played before it (an attack, then idle again); `on_animation_ended` hears its end. With
    /// nothing looping before, it holds its last frame (a death). False when there is none such.
    pub fn play_once(&mut self, name: &str) -> bool {
        let model = self.control_mut();
        let Some(animation) = model.animation_named(name) else { return false };
        if let Some(main) = model.main.filter(|m| m.looped) {
            model.back_to = Some(main.animation);
        }
        model.start(animation, false);
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    /// Holds the animation where it is: a once-only one stays on its frame, nothing goes back.
    pub fn hold(&mut self, name: &str) -> bool {
        let model = self.control_mut();
        let Some(animation) = model.animation_named(name) else { return false };
        model.back_to = None;
        model.start(animation, false);
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    /// Loops another animation together with the main one (another object of the scene: a coin
    /// spinning while the boss attacks). False when the scene has none such.
    pub fn play_layer(&mut self, name: &str) -> bool {
        let model = self.control_mut();
        let Some(animation) = model.animation_named(name) else { return false };
        if !model.layers.iter().any(|l| l.animation == animation) {
            model.layers.push(Track { animation, time: 0.0, looped: true });
        }
        model.playing = true;
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    /// Stops an animation started with `play_layer`.
    pub fn stop_layer(&mut self, name: &str) {
        let model = self.control_mut();
        if let Some(animation) = model.animation_named(name) {
            model.layers.retain(|l| l.animation != animation);
        }
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Stops or resumes all animations where they are.
    pub fn set_playing(&mut self, on: bool) {
        self.control_mut().playing = on;
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Animation speed, 1 = as authored.
    pub fn set_speed(&mut self, speed: f32) {
        self.control_mut().speed = speed;
    }

    /// Turns the scene by itself, degrees per second (0 = still).
    pub fn set_auto_rotate(&mut self, degrees_per_second: f32) {
        self.control_mut().auto_rotate = degrees_per_second;
        self.mark(Dirty::APPLY);
    }

    fn with_node(&mut self, name: &str, f: impl FnOnce(&mut NodeOverride)) -> bool {
        let model = self.control_mut();
        let Some(i) = model.node_named(name) else { return false };
        f(&mut model.nodes[i]);
        self.mark(Dirty::DRAW);
        true
    }

    /// Shows or hides a node by name, with what hangs under it. False when there is none such.
    pub fn set_node_visible(&mut self, name: &str, visible: bool) -> bool {
        self.with_node(name, |n| n.hidden = !visible)
    }

    /// Moves a node by name: its own position, in its parent's units. It replaces what the file
    /// and the animations give it.
    pub fn set_node_translation(&mut self, name: &str, xyz: [f32; 3]) -> bool {
        self.with_node(name, |n| n.translation = Some(Vec3::from(xyz)))
    }

    /// Turns a node by name: rotations around x, y, z in degrees (applied in that order).
    pub fn set_node_rotation(&mut self, name: &str, degrees: [f32; 3]) -> bool {
        let [x, y, z] = degrees.map(f32::to_radians);
        self.with_node(name, |n| n.rotation = Some(Quat::from_euler(glam::EulerRot::XYZ, x, y, z)))
    }

    /// Scales a node by name.
    pub fn set_node_scale(&mut self, name: &str, xyz: [f32; 3]) -> bool {
        self.with_node(name, |n| n.scale = Some(Vec3::from(xyz)))
    }

    /// Gives a node back what the file and the animations say: position, turn, scale, shown.
    pub fn reset_node(&mut self, name: &str) -> bool {
        self.with_node(name, |n| *n = NodeOverride::default())
    }

    /// Recolors a material by name (a part of the model: skin, armor); `None` = the file's color.
    /// A texture is tinted by it. False when there is none such.
    pub fn set_material_color(&mut self, name: &str, color: Option<Color>) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].color = color.map(|c| {
            // The file's colors are linear; a UI color is sRGB.
            let linear = |v: u8| (v as f32 / 255.0).powf(2.2);
            [linear(c.r()), linear(c.g()), linear(c.b()), c.a() as f32 / 255.0]
        });
        self.mark(Dirty::DRAW);
        true
    }

    /// Shows or hides every surface with this material. False when there is none such.
    pub fn set_material_visible(&mut self, name: &str, visible: bool) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].hidden = !visible;
        self.mark(Dirty::DRAW);
        true
    }
}

// ---------------------------------------------------------------- the scene on the CPU

struct Node {
    name: String,
    parent: Option<usize>,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    mesh: Option<usize>,
    skin: Option<usize>,
}

struct Primitive {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    joints: Vec<[f32; 4]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
    material: Option<usize>,
}

struct Material {
    name: String,
    color: [f32; 4],
    texture: Option<usize>,
    double_sided: bool,
    blend: bool,
}

struct Skin {
    joints: Vec<usize>,
    inverse_bind: Vec<Mat4>,
}

enum Path {
    Translation,
    Rotation,
    Scale,
}

struct Channel {
    node: usize,
    path: Path,
    step: bool,
    times: Vec<f32>,
    /// 3 or 4 floats per key.
    values: Vec<f32>,
}

struct Animation {
    name: String,
    duration: f32,
    channels: Vec<Channel>,
}

struct Scene {
    nodes: Vec<Node>,
    meshes: Vec<Vec<Primitive>>,
    materials: Vec<Material>,
    /// Decoded RGBA, width, height.
    images: Vec<(Vec<u8>, u32, u32)>,
    skins: Vec<Skin>,
    animations: Vec<Animation>,
    /// The scene's box at rest.
    bounds: (Vec3, Vec3),
}

impl Scene {
    fn load(bytes: &[u8]) -> Result<Scene, String> {
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

    /// Every node's own transform as the file gives it.
    fn rest(&self) -> Vec<(Vec3, Quat, Vec3)> {
        self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect()
    }

    /// Writes an animation's values at its time into the nodes it moves; a looped one wraps, a
    /// once-only one stops at its end.
    fn apply(&self, track: Track, local: &mut [(Vec3, Quat, Vec3)]) {
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
    fn world(&self, local: &[(Vec3, Quat, Vec3)]) -> Vec<Mat4> {
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
    fn pose(&self, animation: Option<usize>, time: f32) -> Vec<Mat4> {
        let mut local = self.rest();
        if let Some(animation) = animation {
            self.apply(Track { animation, time, looped: true }, &mut local);
        }
        self.world(&local)
    }

    /// A primitive's vertices in the scene as posed: skinned by its joints, else moved by its node.
    fn posed_positions(&self, node: usize, primitive: &Primitive, pose: &[Mat4]) -> Vec<Vec3> {
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
    fn ray_hit(&self, pose: &[Mat4], hidden: &[bool], materials: &[MaterialOverride], from: Vec3, dir: Vec3) -> Option<(usize, usize, f32)> {
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

    fn rest_bounds(&self) -> (Vec3, Vec3) {
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
    fn camera(&self, yaw: f32, pitch: f32, zoom: f32, aspect: f32) -> Mat4 {
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

/// Where a ray meets a triangle, both sides (Moller-Trumbore): the distance along the ray.
fn ray_triangle(from: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
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
fn sample(c: &Channel, t: f32, width: usize) -> Vec<f32> {
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
fn data_uri(uri: &str) -> Option<Vec<u8>> {
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
fn decode_image(encoded: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let image = skia_safe::images::deferred_from_encoded_data(Data::new_copy(encoded), None)?;
    let (w, h) = (image.width(), image.height());
    let info = skia_safe::ImageInfo::new((w, h), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Unpremul, None);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    image.read_pixels(&info, &mut pixels, (w * 4) as usize, (0, 0), skia_safe::image::CachingHint::Disallow).then_some((pixels, w as u32, h as u32))
}

// ---------------------------------------------------------------- the scene on the GPU

/// GL objects of the scene and the frame it is drawn into.
struct GlScene {
    epoch: u32,
    program: u32,
    uniforms: Uniforms,
    /// Per mesh, per primitive: vertex array, index count.
    meshes: Vec<Vec<(u32, i32)>>,
    textures: Vec<u32>,
    frame: u32,
    color: u32,
    depth: u32,
    size: ISize,
}

struct Uniforms {
    model: i32,
    view_proj: i32,
    joints: i32,
    skinned: i32,
    color: i32,
    textured: i32,
    texture: i32,
    light: i32,
}

#[cfg(target_os = "emscripten")]
mod render {
    use super::*;
    use crate::gpu::{GlTarget, Gpu};
    use skia_safe::{ColorType, Image, gpu::SurfaceOrigin};

    const VS: &str = "#version 300 es
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_nrm;
layout(location=2) in vec2 a_uv;
layout(location=3) in vec4 a_joints;
layout(location=4) in vec4 a_weights;
uniform mat4 u_model;
uniform mat4 u_view_proj;
uniform mat4 u_joints[64];
uniform bool u_skinned;
out vec3 v_nrm;
out vec2 v_uv;
void main() {
    mat4 skin = mat4(1.0);
    if (u_skinned) {
        skin = a_weights.x * u_joints[int(a_joints.x)] + a_weights.y * u_joints[int(a_joints.y)]
             + a_weights.z * u_joints[int(a_joints.z)] + a_weights.w * u_joints[int(a_joints.w)];
    }
    mat4 m = u_model * skin;
    v_nrm = mat3(m) * a_nrm;
    v_uv = a_uv;
    gl_Position = u_view_proj * m * vec4(a_pos, 1.0);
}";

    const FS: &str = "#version 300 es
precision mediump float;
in vec3 v_nrm;
in vec2 v_uv;
uniform vec4 u_color;
uniform bool u_textured;
uniform sampler2D u_texture;
uniform vec3 u_light;
out vec4 o_color;
void main() {
    vec4 c = u_color;
    // glTF colors are linear; textures are sRGB: light in linear, show in sRGB.
    if (u_textured) { vec4 t = texture(u_texture, v_uv); c *= vec4(pow(t.rgb, vec3(2.2)), t.a); }
    if (c.a < 0.02) discard;
    vec3 n = normalize(v_nrm);
    if (!gl_FrontFacing) n = -n;
    float sun = max(dot(n, u_light), 0.0);
    float sky = 0.5 + 0.5 * n.y;
    vec3 lit = c.rgb * (0.30 + 0.25 * sky + 0.70 * sun);
    o_color = vec4(pow(lit, vec3(1.0 / 2.2)) * c.a, c.a);
}";

    #[allow(non_snake_case)]
    unsafe extern "C" {
        fn glCreateShader(kind: u32) -> u32;
        fn glShaderSource(shader: u32, count: i32, strings: *const *const u8, lengths: *const i32);
        fn glCompileShader(shader: u32);
        fn glGetShaderiv(shader: u32, name: u32, out: *mut i32);
        fn glGetShaderInfoLog(shader: u32, size: i32, length: *mut i32, log: *mut u8);
        fn glCreateProgram() -> u32;
        fn glAttachShader(program: u32, shader: u32);
        fn glLinkProgram(program: u32);
        fn glGetProgramiv(program: u32, name: u32, out: *mut i32);
        fn glGetUniformLocation(program: u32, name: *const u8) -> i32;
        fn glUseProgram(program: u32);
        fn glGenVertexArrays(n: i32, out: *mut u32);
        fn glBindVertexArray(vao: u32);
        fn glGenBuffers(n: i32, out: *mut u32);
        fn glBindBuffer(target: u32, buffer: u32);
        fn glBufferData(target: u32, size: isize, data: *const u8, usage: u32);
        fn glEnableVertexAttribArray(index: u32);
        fn glDisableVertexAttribArray(index: u32);
        fn glVertexAttribPointer(index: u32, size: i32, kind: u32, normalized: u8, stride: i32, offset: usize);
        fn glVertexAttrib4f(index: u32, x: f32, y: f32, z: f32, w: f32);
        fn glGenTextures(n: i32, out: *mut u32);
        fn glBindTexture(target: u32, texture: u32);
        fn glTexImage2D(target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, kind: u32, data: *const u8);
        fn glTexParameteri(target: u32, name: u32, value: i32);
        fn glGenerateMipmap(target: u32);
        fn glActiveTexture(unit: u32);
        fn glGenFramebuffers(n: i32, out: *mut u32);
        fn glBindFramebuffer(target: u32, frame: u32);
        fn glFramebufferTexture2D(target: u32, attachment: u32, tex_target: u32, texture: u32, level: i32);
        fn glGenRenderbuffers(n: i32, out: *mut u32);
        fn glBindRenderbuffer(target: u32, buffer: u32);
        fn glRenderbufferStorage(target: u32, format: u32, w: i32, h: i32);
        fn glFramebufferRenderbuffer(target: u32, attachment: u32, rb_target: u32, buffer: u32);
        fn glViewport(x: i32, y: i32, w: i32, h: i32);
        fn glClearColor(r: f32, g: f32, b: f32, a: f32);
        fn glClearDepthf(d: f32);
        fn glClear(mask: u32);
        fn glEnable(cap: u32);
        fn glDisable(cap: u32);
        fn glDepthMask(on: u8);
        fn glColorMask(r: u8, g: u8, b: u8, a: u8);
        fn glDepthFunc(func: u32);
        fn glCullFace(face: u32);
        fn glFrontFace(mode: u32);
        fn glBlendFunc(src: u32, dst: u32);
        fn glUniformMatrix4fv(location: i32, count: i32, transpose: u8, value: *const f32);
        fn glUniform4fv(location: i32, count: i32, value: *const f32);
        fn glUniform3fv(location: i32, count: i32, value: *const f32);
        fn glUniform1i(location: i32, value: i32);
        fn glDrawElements(mode: u32, count: i32, kind: u32, offset: usize);
        fn glPixelStorei(name: u32, value: i32);
    }

    const ARRAY_BUFFER: u32 = 0x8892;
    const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
    const STATIC_DRAW: u32 = 0x88E4;
    const FLOAT: u32 = 0x1406;
    const UNSIGNED_BYTE: u32 = 0x1401;
    const UNSIGNED_INT: u32 = 0x1405;
    const TEXTURE_2D: u32 = 0x0DE1;
    const RGBA: u32 = 0x1908;
    const RGBA8: u32 = 0x8058;
    const FRAMEBUFFER: u32 = 0x8D40;
    const RENDERBUFFER: u32 = 0x8D41;
    const COLOR_ATTACHMENT0: u32 = 0x8CE0;
    const DEPTH_ATTACHMENT: u32 = 0x8D00;
    const DEPTH_COMPONENT24: u32 = 0x81A6;
    const DEPTH_TEST: u32 = 0x0B71;
    const CULL_FACE: u32 = 0x0B44;
    const BLEND: u32 = 0x0BE2;
    const SCISSOR_TEST: u32 = 0x0C11;
    const STENCIL_TEST: u32 = 0x0B90;
    const TRIANGLES: u32 = 0x0004;
    const LESS: u32 = 0x0201;
    const BACK: u32 = 0x0405;
    const CCW: u32 = 0x0901;
    const ONE: u32 = 1;
    const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
    const TEXTURE0: u32 = 0x84C0;
    const UNPACK_ALIGNMENT: u32 = 0x0CF5;

    fn shader(kind: u32, source: &str) -> u32 {
        // SAFETY: GL calls on the current WebGL2 context, with valid pointers.
        unsafe {
            let s = glCreateShader(kind);
            let (ptr, len) = (source.as_ptr(), source.len() as i32);
            glShaderSource(s, 1, &ptr, &len);
            glCompileShader(s);
            let mut ok = 0;
            glGetShaderiv(s, 0x8B81, &mut ok);
            if ok == 0 {
                let mut log = vec![0u8; 1024];
                let mut n = 0;
                glGetShaderInfoLog(s, 1024, &mut n, log.as_mut_ptr());
                eprintln!("SkiaModel3D shader: {}", String::from_utf8_lossy(&log[..n.max(0) as usize]));
            }
            s
        }
    }

    fn buffer(target: u32, bytes: &[u8]) -> u32 {
        // SAFETY: as `shader`.
        unsafe {
            let mut b = 0;
            glGenBuffers(1, &mut b);
            glBindBuffer(target, b);
            glBufferData(target, bytes.len() as isize, bytes.as_ptr(), STATIC_DRAW);
            b
        }
    }

    fn bytes<T>(v: &[T]) -> &[u8] {
        // SAFETY: plain f32 / u32 arrays.
        unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
    }

    fn attribute(index: u32, data: &[u8], size: i32) {
        if data.is_empty() {
            return;
        }
        buffer(ARRAY_BUFFER, data);
        // SAFETY: as `shader`; the buffer just bound.
        unsafe {
            glEnableVertexAttribArray(index);
            glVertexAttribPointer(index, size, FLOAT, 0, 0, 0);
        }
    }

    fn upload(scene: &Scene, epoch: u32) -> GlScene {
        // SAFETY: as `shader`.
        unsafe {
            let program = glCreateProgram();
            glAttachShader(program, shader(0x8B31, VS));
            glAttachShader(program, shader(0x8B30, FS));
            glLinkProgram(program);
            let mut ok = 0;
            glGetProgramiv(program, 0x8B82, &mut ok);
            if ok == 0 {
                eprintln!("SkiaModel3D: the shader program did not link");
            }
            let loc = |name: &str| glGetUniformLocation(program, format!("{name}\0").as_ptr());
            let uniforms = Uniforms {
                model: loc("u_model"),
                view_proj: loc("u_view_proj"),
                joints: loc("u_joints"),
                skinned: loc("u_skinned"),
                color: loc("u_color"),
                textured: loc("u_textured"),
                texture: loc("u_texture"),
                light: loc("u_light"),
            };
            glPixelStorei(UNPACK_ALIGNMENT, 4);
            let textures = scene
                .images
                .iter()
                .map(|(rgba, w, h)| {
                    let mut t = 0;
                    glGenTextures(1, &mut t);
                    glBindTexture(TEXTURE_2D, t);
                    glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, *w as i32, *h as i32, 0, RGBA, UNSIGNED_BYTE, rgba.as_ptr());
                    glGenerateMipmap(TEXTURE_2D);
                    glTexParameteri(TEXTURE_2D, 0x2801, 0x2703); // MIN_FILTER LINEAR_MIPMAP_LINEAR
                    glTexParameteri(TEXTURE_2D, 0x2800, 0x2601); // MAG_FILTER LINEAR
                    t
                })
                .collect();
            let meshes = scene
                .meshes
                .iter()
                .map(|primitives| {
                    primitives
                        .iter()
                        .map(|p| {
                            let mut vao = 0;
                            glGenVertexArrays(1, &mut vao);
                            glBindVertexArray(vao);
                            attribute(0, bytes(&p.positions), 3);
                            attribute(1, bytes(&p.normals), 3);
                            attribute(2, bytes(&p.uvs), 2);
                            attribute(3, bytes(&p.joints), 4);
                            attribute(4, bytes(&p.weights), 4);
                            buffer(ELEMENT_ARRAY_BUFFER, bytes(&p.indices));
                            glBindVertexArray(0);
                            (vao, p.indices.len() as i32)
                        })
                        .collect()
                })
                .collect();
            GlScene { epoch, program, uniforms, meshes, textures, frame: 0, color: 0, depth: 0, size: ISize::new(0, 0) }
        }
    }

    fn ensure_frame(gl: &mut GlScene, size: ISize) {
        if gl.size == size && gl.frame != 0 {
            return;
        }
        // SAFETY: as `shader`.
        unsafe {
            if gl.frame == 0 {
                glGenFramebuffers(1, &mut gl.frame);
                glGenTextures(1, &mut gl.color);
                glGenRenderbuffers(1, &mut gl.depth);
            }
            glBindTexture(TEXTURE_2D, gl.color);
            glTexImage2D(TEXTURE_2D, 0, RGBA8 as i32, size.width, size.height, 0, RGBA, UNSIGNED_BYTE, std::ptr::null());
            glTexParameteri(TEXTURE_2D, 0x2801, 0x2601);
            glTexParameteri(TEXTURE_2D, 0x2800, 0x2601);
            glBindRenderbuffer(RENDERBUFFER, gl.depth);
            glRenderbufferStorage(RENDERBUFFER, DEPTH_COMPONENT24, size.width, size.height);
            glBindFramebuffer(FRAMEBUFFER, gl.frame);
            glFramebufferTexture2D(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, gl.color, 0);
            glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, gl.depth);
        }
        gl.size = size;
    }

    /// Draws the scene in its pose into the frame texture and hands it to Skia as an image.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(slot: &mut Option<GlScene>, gpu: &mut Gpu, scene: &Scene, pose: &[Mat4], hidden: &[bool], overrides: &[MaterialOverride], view_proj: Mat4, size: ISize) -> Option<Image> {
        let gl = slot.get_or_insert_with(|| upload(scene, gpu.epoch()));
        ensure_frame(gl, size);
        let light = Vec3::new(0.4, 0.8, 0.6).normalize();
        // SAFETY: as `shader`. Skia's GL state is reset below.
        unsafe {
            glBindFramebuffer(FRAMEBUFFER, gl.frame);
            glViewport(0, 0, size.width, size.height);
            glDisable(SCISSOR_TEST);
            glDisable(STENCIL_TEST);
            glDisable(BLEND);
            glColorMask(1, 1, 1, 1);
            glDepthMask(1);
            glClearColor(0.0, 0.0, 0.0, 0.0);
            glClearDepthf(1.0);
            glClear(0x4000 | 0x0100);
            glEnable(DEPTH_TEST);
            glDepthFunc(LESS);
            glFrontFace(CCW);
            glCullFace(BACK);
            glUseProgram(gl.program);
            glUniformMatrix4fv(gl.uniforms.view_proj, 1, 0, view_proj.to_cols_array().as_ptr());
            glUniform3fv(gl.uniforms.light, 1, light.to_array().as_ptr());
            glUniform1i(gl.uniforms.texture, 0);
            glActiveTexture(TEXTURE0);
            let mut joints = vec![0f32; MAX_JOINTS * 16];
            // Opaque first, then blended, each in tree order.
            for pass_blend in [false, true] {
                if pass_blend {
                    glEnable(BLEND);
                    glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
                    glDepthMask(0);
                }
                for (i, node) in scene.nodes.iter().enumerate() {
                    let (Some(m), Some(vaos)) = (node.mesh, node.mesh.and_then(|m| gl.meshes.get(m))) else { continue };
                    if hidden[i] {
                        continue;
                    }
                    let skin = node.skin.and_then(|s| scene.skins.get(s));
                    let model = if skin.is_some() { Mat4::IDENTITY } else { pose[i] };
                    if let Some(skin) = skin {
                        for (j, (joint, ibm)) in skin.joints.iter().zip(&skin.inverse_bind).take(MAX_JOINTS).enumerate() {
                            joints[j * 16..j * 16 + 16].copy_from_slice(&(pose[*joint] * *ibm).to_cols_array());
                        }
                        glUniformMatrix4fv(gl.uniforms.joints, skin.joints.len().min(MAX_JOINTS) as i32, 0, joints.as_ptr());
                    }
                    glUniformMatrix4fv(gl.uniforms.model, 1, 0, model.to_cols_array().as_ptr());
                    for (p, (vao, count)) in scene.meshes[m].iter().zip(vaos) {
                        let material = p.material.and_then(|i| scene.materials.get(i));
                        let changed = p.material.and_then(|i| overrides.get(i)).copied().unwrap_or_default();
                        if changed.hidden || material.is_some_and(|m| m.blend) != pass_blend {
                            continue;
                        }
                        let color = changed.color.unwrap_or(material.map_or([0.8, 0.8, 0.8, 1.0], |m| m.color));
                        let texture = material.and_then(|m| m.texture).and_then(|t| gl.textures.get(t));
                        if material.is_some_and(|m| m.double_sided) { glDisable(CULL_FACE) } else { glEnable(CULL_FACE) }
                        glUniform4fv(gl.uniforms.color, 1, color.as_ptr());
                        glUniform1i(gl.uniforms.textured, texture.is_some() as i32);
                        glBindTexture(TEXTURE_2D, texture.copied().unwrap_or(0));
                        glUniform1i(gl.uniforms.skinned, (skin.is_some() && !p.joints.is_empty()) as i32);
                        glBindVertexArray(*vao);
                        if p.joints.is_empty() {
                            glDisableVertexAttribArray(3);
                            glDisableVertexAttribArray(4);
                            glVertexAttrib4f(3, 0.0, 0.0, 0.0, 0.0);
                            glVertexAttrib4f(4, 1.0, 0.0, 0.0, 0.0);
                        }
                        glDrawElements(TRIANGLES, *count, UNSIGNED_INT, 0);
                    }
                }
            }
            glBindVertexArray(0);
            glBindTexture(TEXTURE_2D, 0);
            glUseProgram(0);
            glDisable(DEPTH_TEST);
            glDisable(CULL_FACE);
            glDisable(BLEND);
            glDepthMask(1);
            glBindFramebuffer(FRAMEBUFFER, 0);
            glBindBuffer(ARRAY_BUFFER, 0);
            gpu.reset_gl_state();
            gpu.import_gl_texture(gl.color, GlTarget::Texture2D, RGBA8, size, SurfaceOrigin::BottomLeft, ColorType::RGBA8888)
        }
    }
}

#[cfg(not(target_os = "emscripten"))]
mod render {
    use super::*;
    use crate::gpu::Gpu;
    use skia_safe::Image;

    /// Not on this platform yet: nothing is drawn.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(_slot: &mut Option<GlScene>, _gpu: &mut Gpu, _scene: &Scene, _pose: &[Mat4], _hidden: &[bool], _overrides: &[MaterialOverride], _view_proj: Mat4, _size: ISize) -> Option<Image> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAGON: &[u8] = include_bytes!("../../../examples/model3d/assets/dragon.glb");

    #[test]
    fn a_skinned_glb_loads_with_its_animations_and_moves_over_time() {
        let scene = Scene::load(DRAGON).expect("loads");
        assert_eq!(scene.animations.len(), 8);
        assert!(scene.animations.iter().any(|a| a.name == "CharacterArmature|Punch"));
        assert_eq!(scene.skins.len(), 1);
        let vertices: usize = scene.meshes.iter().flatten().map(|p| p.positions.len()).sum();
        assert_eq!(vertices, 4350);
        let (lo, hi) = scene.bounds;
        assert!((hi - lo).min_element() > 0.0, "a real box: {lo} {hi}");
        let punch = scene.animations.iter().position(|a| a.name == "CharacterArmature|Punch");
        let (a, b) = (scene.pose(punch, 0.0), scene.pose(punch, 0.4));
        assert!(a.iter().zip(&b).any(|(x, y)| !x.abs_diff_eq(*y, 1e-4)), "the bones move");
    }

    #[test]
    fn a_broken_file_reports_why() {
        let Err(why) = Scene::load(b"not a model") else { panic!("loaded garbage") };
        assert!(!why.is_empty());
    }
}
