//! 3D models in the UI (feature `model3d`): glTF 2.0 scenes (`.glb`) drawn in real 3D with a
//! depth buffer, textures, a light and skeletal animation, through the GPU context Skia uses, into
//! a texture the control then shows as an image. Layout, gestures and everything drawn around it
//! work as for any control.
//! - `SkiaModel3D`: one scene, framed by its own camera; drag to turn it.
//! - `SkiaScene3D`: a 3D layer over a 2D game field: many objects placed in the layer's own points
//!   like sprites, each with its own animations, all drawn in one pass, each model loaded once.
//!
//! The app drives them: animations looped, played once and back, held, layered, blended into
//! each other; nodes and materials by name; taps that tell what was hit. WebGL2 (the browser) for
//! now; elsewhere nothing is drawn yet.
// ponytail: one directional light + sky light, base color texture and factor only (no normal,
// metallic-roughness or emissive maps), no morph targets, 64 joints per skin, alpha blend drawn in
// tree order; CUBICSPLINE samplers read as linear. Add them when a delivered model needs them.

mod layer;
mod player;
mod render;
mod scene;
mod shapes;

pub use layer::{SceneHit, SkiaScene3D};
pub use shapes::{Mesh3D, Model3D};

/// How 3D is drawn (`look` on `SkiaModel3D` and `SkiaScene3D`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// Shaded by a light and the sky (the default).
    #[default]
    Lit,
    /// Two tones, lit and shade, like a cartoon.
    Toon,
    /// Each color as it is, no shading: an illustration (with `outline`).
    Flat,
}

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use glam::{Mat4, Quat, Vec3};
use skia_safe::{Color, ISize, Paint, Rect, SamplingOptions};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, PaintCx, part_mut};
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{Build, ControlId, Cx, Mut, Raw, Tree, wrong_state};
use crate::types::Dirty;
use player::Player;
use scene::{MaterialOverride, Scene};

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

/// A node's own transform set from code (`set_node_translation` and the rest): it replaces what
/// the file and the animations give that part.
#[derive(Clone, Copy, Default)]
struct NodeOverride {
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<Vec3>,
    hidden: bool,
}

enum Event {
    Ended(String),
    Tapped(ModelHit),
}

/// An outline color for the shader: sRGB, as given.
fn outline_color(c: Color) -> [f32; 4] {
    [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]
}

/// A UI color (sRGB) as a glTF material color (linear).
fn linear(c: Color) -> [f32; 4] {
    let l = |v: u8| (v as f32 / 255.0).powf(2.2);
    [l(c.r()), l(c.g()), l(c.b()), c.a() as f32 / 255.0]
}

/// A glTF scene drawn in 3D (see the module docs).
pub struct SkiaModel3D {
    id: ControlId,
    scene: Option<Rc<Scene>>,
    /// The file could not be read: why.
    error: Option<String>,
    /// The texture it draws into, made at the first paint on a GL context.
    target: RefCell<Option<render::Target>>,
    /// The animation asked for by name before the first frame (`Build::animation`).
    start_animation: Option<String>,
    player: Player,
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
    look: Look,
    /// Outline width in points (0 = none) and color.
    outline: (f32, Color),
}

impl SkiaModel3D {
    /// A model made in code (`Model3D`): smooth shapes, flat colors. Same as `new` otherwise.
    pub fn from_model(model: &Model3D) -> Build<SkiaModel3D> {
        SkiaModel3D::with_scene(Ok(Rc::new(model.to_scene())))
    }

    /// A scene from the bytes of a `.glb` (or a `.gltf` with its buffers embedded). It plays its
    /// first animation in a loop; drags turn it. Controls showing the same bytes share one copy.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(glb: &[u8]) -> Build<SkiaModel3D> {
        SkiaModel3D::with_scene(scene::shared(glb))
    }

    fn with_scene(read: Result<Rc<Scene>, String>) -> Build<SkiaModel3D> {
        let (scene, error) = match read {
            Ok(scene) => (Some(scene), None),
            Err(why) => (None, Some(why)),
        };
        let (nodes, materials) = scene.as_ref().map_or((0, 0), |s| (s.nodes.len(), s.materials.len()));
        let model = SkiaModel3D {
            id: ControlId { index: 0, generation: 0 },
            player: Player::new(scene.as_deref()),
            scene,
            error,
            target: RefCell::new(None),
            start_animation: None,
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
            look: Look::Lit,
            outline: (0.0, Color::BLACK),
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
        Some(self.scene.as_ref()?.animations.get(self.player.main?.animation)?.name.as_str())
    }

    /// The file could not be read: why (`None` when it was).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn animation_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.animation_named(name)
    }

    fn node_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.nodes.iter().position(|n| n.name == name)
    }

    fn material_named(&self, name: &str) -> Option<usize> {
        self.scene.as_ref()?.materials.iter().position(|m| m.name == name)
    }

    fn wants_frames(&self) -> bool {
        self.scene.is_some() && (!self.events.is_empty() || self.auto_rotate != 0.0 || self.player.busy())
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

    /// One frame of time: the animations advance, an animation played once that ended is reported.
    fn advance(&mut self, delta: f32) {
        let Some(scene) = self.scene.clone() else { return };
        self.yaw = (self.yaw + self.auto_rotate * delta) % 360.0;
        if let Some(ended) = self.player.advance(&scene, delta) {
            self.events.push(Event::Ended(scene.animations[ended].name.clone()));
        }
    }

    /// The local transform of every node for this frame: the animations', then what code set.
    fn locals(&self, scene: &Scene) -> Vec<(Vec3, Quat, Vec3)> {
        let mut local = self.player.locals(scene);
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
        scene.hidden_by(|n| self.nodes.get(n).is_some_and(|o| o.hidden))
    }

    fn camera(&self, scene: &Scene, size: (f32, f32)) -> Mat4 {
        scene.camera(self.yaw, self.pitch, self.zoom, size.0 / size.1.max(1.0))
    }

    /// What the ray through this point of the control (pixels from its corner) meets first.
    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> Option<ModelHit> {
        let scene = self.scene.as_ref()?;
        let pose = scene.world(&self.locals(scene));
        let (from, dir) = scene::ray(self.camera(scene, size), x / size.0, y / size.1);
        let (node, primitive, t) = scene.ray_hit(&pose, &self.hidden(scene), &self.materials, from, dir)?;
        Some(ModelHit { node: scene.nodes[node].name.clone(), material: scene.material_of(node, primitive), point: (from + dir * t).to_array() })
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
            let model = node.kind.as_deref_mut().and_then(part_mut::<SkiaModel3D>).expect("checked above");
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
            // The start pose: playing only when the app did not say otherwise.
            let playing = self.player.playing;
            self.player.play(animation);
            self.player.fade_now();
            self.player.playing = playing;
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
        let Some(scene) = self.scene.as_ref() else { return };
        let pose = scene.world(&self.locals(scene));
        let view_proj = self.camera(scene, (size.width as f32, size.height as f32));
        let hidden = self.hidden(scene);
        let style = render::Style { look: self.look, outline: self.outline.0 * cx.scale, outline_color: outline_color(self.outline.1) };
        let canvas = cx.canvas;
        let Some(gpu) = cx.gpu() else { return };
        let items = [render::Item { scene, pose: &pose, hidden: &hidden, materials: &self.materials }];
        let Some(image) = render::draw(&mut self.target.borrow_mut(), gpu, &items, &style, view_proj, size) else { return };
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
        self.control_mut().player.playing = on;
        self
    }

    /// Animation speed, 1 = as authored.
    pub fn speed(mut self, speed: f32) -> Self {
        self.control_mut().player.speed = speed;
        self
    }

    /// How long one animation blends into the next, milliseconds (250 by default, 0 = cut).
    pub fn crossfade_ms(mut self, ms: f32) -> Self {
        self.control_mut().player.fade = ms.max(0.0) / 1000.0;
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

    /// How it is drawn: `Look::Lit` (default), `Look::Toon` or `Look::Flat`.
    pub fn look(mut self, look: Look) -> Self {
        self.control_mut().look = look;
        self
    }

    /// An outline around every shape, `width` points, like an illustration's stroke.
    pub fn outline(mut self, width: f32, color: Color) -> Self {
        self.control_mut().outline = (width.max(0.0), color);
        self
    }

    /// Runs when an animation played with `play_once` or `hold` ended, with its name.
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
    fn with_animation(&mut self, name: &str, f: impl FnOnce(&mut Player, usize)) -> bool {
        let model = self.control_mut();
        let Some(animation) = model.animation_named(name) else { return false };
        f(&mut model.player, animation);
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    /// Loops the animation with this name, blending into it; false when the scene has none such.
    pub fn play(&mut self, name: &str) -> bool {
        self.with_animation(name, Player::play)
    }

    /// Plays the animation with this name once, then blends back into the looping one that
    /// played before it (an attack, then idle again); `on_animation_ended` hears its end. With
    /// nothing looping before, it holds its last frame (a death). False when there is none such.
    pub fn play_once(&mut self, name: &str) -> bool {
        self.with_animation(name, Player::play_once)
    }

    /// Plays the animation once and stays on its last frame; nothing goes back.
    pub fn hold(&mut self, name: &str) -> bool {
        self.with_animation(name, Player::hold)
    }

    /// Loops another animation together with the main one (another object of the scene: a coin
    /// spinning while the boss attacks). False when the scene has none such.
    pub fn play_layer(&mut self, name: &str) -> bool {
        self.with_animation(name, Player::add_layer)
    }

    /// Stops an animation started with `play_layer`.
    pub fn stop_layer(&mut self, name: &str) {
        self.with_animation(name, Player::remove_layer);
    }

    /// Stops or resumes all animations where they are.
    pub fn set_playing(&mut self, on: bool) {
        self.control_mut().player.playing = on;
        self.mark(Dirty::DRAW_APPLY);
    }

    /// Animation speed, 1 = as authored.
    pub fn set_speed(&mut self, speed: f32) {
        self.control_mut().player.speed = speed;
    }

    /// The view's angles: around the vertical axis and tilt, degrees (a spin driven by the app).
    pub fn set_view_angles(&mut self, yaw: f32, pitch: f32) {
        let model = self.control_mut();
        (model.yaw, model.pitch) = (yaw, pitch);
        self.mark(Dirty::DRAW);
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
        model.materials[i].color = color.map(linear);
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

#[cfg(test)]
mod tests {
    use super::*;

    const DRAGON: &[u8] = include_bytes!("../../../../examples/model3d/assets/dragon.glb");

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

    #[test]
    fn the_same_bytes_are_read_once() {
        let (a, b) = (scene::shared(DRAGON).unwrap(), scene::shared(DRAGON).unwrap());
        assert!(Rc::ptr_eq(&a, &b));
    }
}
