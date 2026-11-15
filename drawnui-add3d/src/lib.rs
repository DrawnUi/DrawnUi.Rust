//! drawnui-add3d, an add-on for DrawnUI: glTF 2.0 scenes (`.glb`) drawn in real 3D with a
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

pub use layer::{Scene3DBuild, Scene3DSet, SceneHit, SkiaScene3D};
pub use scene::Texture3D;

pub use shapes::{Mesh3D, Model3D};

/// Everything an app uses: the controls, their build and set traits, the shapes and the looks.
pub mod prelude {
    pub use crate::{LoadState3D, Look, Mesh3D, Model3D, Model3DBuild, Model3DSet, ModelHit, PointOnScreen, Scene3DBuild, Scene3DSet, SceneHit, SkiaModel3D, SkiaScene3D, Texture3D};
}

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
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glam::{Mat4, Quat, Vec3};
use drawnui::skia::{Color, ISize, Paint, Rect, SamplingOptions};

use drawnui::{
    Build, Control, ControlId, Cx, Dirty, FrameTick, Gesture, GestureCx, GestureKind, Gpu, Handled, IdleGpu, IdleGpuCx, LayoutCx, Mut, PaintCx, VelocityAccumulator,
    downcast_state,
};
use player::Player;
use scene::{MaterialOverride, Scene};

/// Where a point of the scene is on screen (`screen_position`): in the control's points from its
/// top left, and whether the model's own surface lies in front of it (a pin on the far side of a
/// globe, a hotspot behind the model).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointOnScreen {
    pub x: f32,
    pub y: f32,
    pub occluded: bool,
}

/// Where loading a model from a url stands (`source`, `load_state`, `on_load_state`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadState3D {
    /// Asked for, not here yet.
    Loading,
    /// Read and shown.
    #[default]
    Ready,
    /// Missing or not a model; `error()` says why.
    Failed,
}

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

type EndedHandler = Box<dyn FnMut(&mut Mut<'_, SkiaModel3D>, &mut dyn Any, &mut Cx<'_>, &str)>;
type TappedHandler = Box<dyn FnMut(&mut Mut<'_, SkiaModel3D>, &mut dyn Any, &mut Cx<'_>, &ModelHit)>;
type ViewHandler = Box<dyn FnMut(&mut Mut<'_, SkiaModel3D>, &mut dyn Any, &mut Cx<'_>)>;
type LoadHandler = Box<dyn FnMut(&mut Mut<'_, SkiaModel3D>, &mut dyn Any, &mut Cx<'_>, LoadState3D)>;

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
    LoadState(LoadState3D),
}

/// A view: yaw, pitch (degrees) and zoom.
type View = (f32, f32, f32);

/// Two taps on the control closer than this reset the view (`reset_on_double_tap`).
const DOUBLE_TAP_MS: f64 = 300.0;
/// `reset_view` glides back home in this long, seconds.
const GLIDE_S: f32 = 0.3;
/// The scroll's deceleration after a fling: the speed keeps this share of itself each millisecond.
const FLING_DECELERATION: f32 = 0.998;

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
    /// It rendered once (on screen or ahead, `prefetch_near`): its model and images are on the GPU.
    gpu_ready: Cell<bool>,
    /// Waiting to render ahead (`LayoutCx::request_idle_gpu`).
    prefetch_queued: bool,
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
    /// The way the automatic turn goes: 1 as the app set it, -1 after a fling the other way (it
    /// follows the last swipe, so a fling slows into the turn instead of stopping and turning back).
    auto_direction: f32,
    /// A drag that is let go keeps the model turning (`fling`).
    fling: bool,
    /// The turn a fling left, degrees per second (yaw, pitch), slowing as a scroll does.
    fling_speed: (f32, f32),
    /// The drag's speeds, for the fling when it is let go; whether it turned the model.
    velocity: VelocityAccumulator,
    dragged: bool,
    /// Drags turn the scene.
    orbit: bool,
    /// Distance to the scene, as a factor of the distance that fits it (1 = fits).
    zoom: f32,
    /// Room kept around the model's box, in its own units, for parts that move past it.
    frame_margin: f32,
    /// The animations the view keeps in frame besides the rest (`None`: all of them).
    frame_animations: Option<Vec<String>>,
    /// What the view fits, worked out once per scene and framing (`camera`).
    framing: RefCell<Option<Framing>>,
    look: Look,
    /// Outline width in points (0 = none) and color.
    outline: (f32, Color),
    /// Smooth edges (multisampling).
    antialias: bool,
    /// The view the app set last (build, `set_view_angles`, `set_zoom`): `reset_view` goes back
    /// to it; drags and pinches never change it.
    home: View,
    /// A glide back home (`reset_view`): where it started, seconds into it.
    glide: Option<(View, f32)>,
    pinch_zoom: bool,
    /// The pinch going on: the zoom when it started and the product of its steps, so a pinch
    /// past a zoom limit and back ends where it started. Until the next press, drags do not turn.
    pinch: Option<(f32, f32)>,
    zoom_range: (f32, f32),
    reset_on_double_tap: bool,
    last_tap_ms: Option<f64>,
    lock_pitch: bool,
    /// Degrees the model turns toward the pointer or finger at the control's edge (0 = off).
    tilt_to_pointer: f32,
    /// The drag under way: `None` until its first move decides, then whether it turns the model.
    drag: Option<bool>,
    /// The view the app heard last (`on_view_changed`).
    reported_view: View,
    on_view_changed: Option<ViewHandler>,
    /// Size in pixels and pixels per point at the last layout (`screen_position`).
    laid_out: (f32, f32, f32),
    /// Nothing of it is on screen: time stands still and no frames are asked for, until a scroll
    /// brings it back.
    off_screen: bool,
    /// A url asked for (`source`), loaded at the next props change.
    source: Option<String>,
    load_state: LoadState3D,
    on_load_state: Option<LoadHandler>,
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

    /// A scene loaded from a url (the browser fetches it; desktop reads it as a file path): nothing
    /// is drawn while it loads, `load_state` and `on_load_state` tell how it went. Controls asking
    /// for the same url share one load and one copy. An empty url loads nothing until `set_source`.
    pub fn from_url(url: impl Into<String>) -> Build<SkiaModel3D> {
        let mut build = SkiaModel3D::with_scene(Err(String::new()));
        let model = build.control_mut();
        (model.error, model.source, model.load_state) = (None, Some(url.into()), LoadState3D::Loading);
        build
    }

    fn with_scene(read: Result<Rc<Scene>, String>) -> Build<SkiaModel3D> {
        let (scene, error) = match read {
            Ok(scene) => (Some(scene), None),
            Err(why) => (None, Some(why)),
        };
        let (nodes, materials) = scene.as_ref().map_or((0, 0), |s| (s.nodes.len(), s.materials.len()));
        let model = SkiaModel3D {
            id: ControlId::from_bits(0),
            player: Player::new(scene.as_deref()),
            scene,
            error,
            target: RefCell::new(None),
            gpu_ready: Cell::new(false),
            prefetch_queued: false,
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
            auto_direction: 1.0,
            fling: false,
            fling_speed: (0.0, 0.0),
            velocity: VelocityAccumulator::default(),
            dragged: false,
            orbit: true,
            zoom: 1.0,
            frame_margin: 0.0,
            frame_animations: None,
            framing: RefCell::new(None),
            look: Look::Lit,
            outline: (0.0, Color::BLACK),
            antialias: false,
            home: (20.0, 10.0, 1.0),
            glide: None,
            pinch_zoom: false,
            pinch: None,
            zoom_range: (0.5, 2.5),
            reset_on_double_tap: false,
            last_tap_ms: None,
            lock_pitch: false,
            tilt_to_pointer: 0.0,
            drag: None,
            reported_view: (20.0, 10.0, 1.0),
            on_view_changed: None,
            laid_out: (0.0, 0.0, 1.0),
            off_screen: false,
            source: None,
            load_state: LoadState3D::Ready,
            on_load_state: None,
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

    /// Smooth edges on (`antialias`).
    pub fn antialias(&self) -> bool {
        self.antialias
    }

    /// Where loading from a url stands (`Ready` for a model given as bytes or made in code).
    pub fn load_state(&self) -> LoadState3D {
        self.load_state
    }

    /// The view now: yaw, pitch (degrees) and zoom.
    pub fn view(&self) -> (f32, f32, f32) {
        (self.yaw, self.pitch, self.zoom)
    }

    /// Where a point of the scene (scene units, as `ModelHit::point`) is on screen now, in the
    /// control's points from its top left, and whether the model's surface lies in front of it.
    /// `None` before the model and the control's size are there, or for a point behind the camera.
    pub fn screen_position(&self, point: [f32; 3]) -> Option<PointOnScreen> {
        let scene = self.scene.as_ref()?;
        let (width, height, scale) = self.laid_out;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let camera = self.camera(scene, (width, height));
        let p = Vec3::from(point);
        let clip = camera * p.extend(1.0);
        if clip.w <= 1e-6 {
            return None;
        }
        let (fx, fy) = ((clip.x / clip.w + 1.0) / 2.0, (1.0 - clip.y / clip.w) / 2.0);
        let (from, dir) = scene::ray(camera, fx, fy);
        let pose = scene.world(&self.locals(scene));
        let (lo, hi) = scene.bounds;
        // A hit closer than the point by more than a hair of the scene's size covers it.
        let epsilon = (hi - lo).length().max(1e-3) * 1e-3;
        let occluded = scene.ray_hit(&pose, &self.hidden(scene), &self.materials, from, dir).is_some_and(|(_, _, t)| t < (p - from).dot(dir) - epsilon);
        Some(PointOnScreen { x: fx * width / scale, y: fy * height / scale, occluded })
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

    /// Turns the model on by itself from these speeds (degrees per second) until it slows to a stop.
    fn start_fling(&mut self, yaw: f32, pitch: f32) {
        if self.auto_rotate != 0.0 && yaw.abs() > self.auto_rotate.abs() {
            self.auto_direction = yaw.signum() * self.auto_rotate.signum();
        }
        let yaw = yaw - self.auto_direction * self.auto_rotate;
        self.fling_speed = (yaw, if self.lock_pitch { 0.0 } else { pitch });
    }

    fn wants_frames(&self) -> bool {
        let flinging = self.fling_speed != (0.0, 0.0);
        let moving = !self.off_screen && (self.auto_rotate != 0.0 || self.player.busy() || self.glide.is_some() || flinging);
        let unreported = self.on_view_changed.is_some() && self.view() != self.reported_view;
        !self.events.is_empty() || (self.scene.is_some() && (moving || unreported))
    }

    /// The scene of the url is read (or could not be).
    fn loaded(&mut self, read: Result<Rc<Scene>, String>) {
        match read {
            Ok(scene) => {
                // What the app set before the scene was there: play or not, speed, crossfade, and
                // the animation to start with.
                let (playing, speed, fade) = (self.player.playing, self.player.speed, self.player.fade);
                self.player = Player::new(Some(&scene));
                (self.player.playing, self.player.speed, self.player.fade) = (playing, speed, fade);
                self.nodes = vec![NodeOverride::default(); scene.nodes.len()];
                self.materials = vec![MaterialOverride::default(); scene.materials.len()];
                (self.scene, self.error, self.load_state) = (Some(scene), None, LoadState3D::Ready);
                self.start_named_animation();
            }
            Err(why) => (self.scene, self.error, self.load_state) = (None, Some(why), LoadState3D::Failed),
        }
        self.events.push(Event::LoadState(self.load_state));
    }

    /// The animation the app named to start with (`animation`), once the scene is there: the start
    /// pose, playing only when the app did not say otherwise.
    fn start_named_animation(&mut self) {
        if let Some(name) = &self.start_animation
            && let Some(animation) = self.animation_named(name)
        {
            self.start_animation = None;
            let playing = self.player.playing;
            self.player.play(animation);
            self.player.fade_now();
            self.player.playing = playing;
        }
    }

    /// Starts loading the url asked for, if any.
    fn load_source(&mut self, cx: &mut Cx) {
        // An empty url waits for `set_source` (an app that starts its loads in an order of its own).
        let Some(url) = self.source.take().filter(|url| !url.is_empty()) else { return };
        let id = self.id;
        let done: scene::UrlDone = Box::new(move |cx, read| {
            if let Some(mut me) = cx.find_mut::<SkiaModel3D>(id) {
                me.control_mut().loaded(read);
                me.mark(Dirty::DRAW_APPLY);
            }
        });
        match scene::load_url(cx, &url, done) {
            // Another control holds the scene of this url: shared at once.
            Some(read) => self.loaded(read),
            None => {
                self.load_state = LoadState3D::Loading;
                self.events.push(Event::LoadState(LoadState3D::Loading));
            }
        }
    }

    /// Glides back to the home view.
    fn start_glide(&mut self) {
        self.glide = Some((self.view(), 0.0));
    }

    /// Turns toward a point of the control (pixels from its corner), from home.
    fn tilt_toward(&mut self, x: f32, y: f32, size: (f32, f32)) {
        let (nx, ny) = ((x / size.0.max(1.0)) * 2.0 - 1.0, (y / size.1.max(1.0)) * 2.0 - 1.0);
        let (nx, ny) = (nx.clamp(-1.0, 1.0), ny.clamp(-1.0, 1.0));
        // The face turns to the pointer: the camera goes the other way.
        self.yaw = self.home.0 - nx * self.tilt_to_pointer;
        self.pitch = self.home.1 + ny * self.tilt_to_pointer;
        self.glide = None;
    }

    fn set_zoom_clamped(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(self.zoom_range.0, self.zoom_range.1);
    }

    fn update_ticking(&mut self, cx: &mut Cx) {
        if self.wants_frames() {
            if !std::mem::replace(&mut self.ticking, true) {
                self.last_ms = None;
                cx.start_frame_loop(self.id, tick);
            }
        } else if std::mem::take(&mut self.ticking) {
            cx.stop_frame_loop(self.id);
        }
    }

    /// One frame of time: the animations advance, an animation played once that ended is reported.
    fn advance(&mut self, delta: f32) {
        let Some(scene) = self.scene.clone() else { return };
        self.yaw = (self.yaw + self.auto_rotate * self.auto_direction * delta) % 360.0;
        if self.fling_speed != (0.0, 0.0) {
            let (yaw, pitch) = self.fling_speed;
            self.yaw = (self.yaw + yaw * delta) % 360.0;
            if pitch != 0.0 {
                self.pitch = (self.pitch + pitch * delta).clamp(-60.0, 80.0);
            }
            // The scroll's deceleration: the speed keeps this share of itself each millisecond.
            let keep = FLING_DECELERATION.powf(delta * 1000.0);
            self.fling_speed = (yaw * keep, pitch * keep);
            // Under a degree and a half per second it no longer reads as moving.
            if yaw.abs() * keep < 1.5 && pitch.abs() * keep < 1.5 {
                self.fling_speed = (0.0, 0.0);
            }
        }
        if let Some((from, t)) = self.glide {
            let t = (t + delta / GLIDE_S).min(1.0);
            let v = 1.0 - (1.0 - t).powi(3);
            // The short way round to the home yaw.
            let turn = (self.home.0 - from.0 + 540.0).rem_euclid(360.0) - 180.0;
            self.yaw = from.0 + turn * v;
            self.pitch = from.1 + (self.home.1 - from.1) * v;
            self.zoom = from.2 + (self.home.2 - from.2) * v;
            self.glide = (t < 1.0).then_some((from, t));
        }
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

    /// The view fits the model at rest and through `frame_animations`, `frame_margin` around it,
    /// with a little air. A view nobody turns (no drag, no lean, no auto turn) fits its box exactly
    /// as seen; a turned one fits the model's outline turned around the vertical (a ball keeps its
    /// size at every tilt), a lean toward the pointer over its whole range so hovering never moves
    /// the camera.
    fn camera(&self, scene: &Rc<Scene>, size: (f32, f32)) -> Mat4 {
        let aspect = size.0 / size.1.max(1.0);
        let margin = self.frame_margin;
        let mut framing = self.framing.borrow_mut();
        if framing.as_ref().is_none_or(|f| !Rc::ptr_eq(&f.scene, scene)) {
            *framing = Some(Framing::new(scene, self.frame_animations.as_deref()));
        }
        let f = framing.as_mut().expect("just made");
        if !self.orbit && self.tilt_to_pointer == 0.0 && self.auto_rotate == 0.0 {
            return Scene::camera_as_seen(f.min, f.max, self.yaw, self.pitch, self.zoom, aspect, margin);
        }
        let axis = scene.extent(None).profile.axis;
        let center = Vec3::new(axis.x, (f.min.y + f.max.y) / 2.0, axis.y);
        let tilt = self.tilt_to_pointer;
        let (from, to) = if tilt > 0.0 { (self.home.1 - tilt, self.home.1 + tilt) } else { (self.pitch, self.pitch) };
        let key = (aspect, from, to, margin);
        if f.fit.is_none_or(|(k, _)| k != key) {
            f.fit = Some((key, Scene::outline_fit(&f.outline, center.y, from, to, aspect, margin)));
        }
        let fit = f.fit.map_or(1.0, |(_, fit)| fit);
        Scene::camera_at(center, (f.max - f.min).length() * 0.5 + margin, fit * self.zoom, self.yaw, self.pitch, aspect)
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

/// What a viewer fits for one scene: the box at rest and through the framed animations, their
/// outline turned around the vertical, and the last fit (aspect, pitch range, margin: distance).
struct Framing {
    scene: Rc<Scene>,
    min: Vec3,
    max: Vec3,
    outline: Vec<glam::Vec2>,
    fit: Option<((f32, f32, f32, f32), f32)>,
}

impl Framing {
    fn new(scene: &Rc<Scene>, animations: Option<&[String]>) -> Framing {
        let rest = scene.extent(None);
        let (mut min, mut max, mut outline) = (rest.min, rest.max, Vec::new());
        rest.profile.points(&mut outline);
        for (a, animation) in scene.animations.iter().enumerate() {
            if animations.is_some_and(|names| !names.contains(&animation.name)) {
                continue;
            }
            let extent = scene.extent(Some(a));
            (min, max) = (min.min(extent.min), max.max(extent.max));
            extent.profile.points(&mut outline);
        }
        Framing { scene: scene.clone(), min, max, outline, fit: None }
    }
}

/// One frame: time moves on, then the app hears what happened (an animation ended, a tap).
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let off_screen = cx.visible_rect(id).is_empty();
    cx.with_control::<SkiaModel3D, _>(id, state, |me, state, cx| {
        let model = me.control_mut();
        let delta = model.last_ms.map_or(0.0, |last| ((time_ms - last) / 1000.0) as f32);
        model.last_ms = Some(time_ms);
        model.off_screen = off_screen;
        if !off_screen {
            model.advance(delta);
        }
        let events = std::mem::take(&mut model.events);
        let mut touched = false;
        for event in events {
            match event {
                Event::Ended(name) => {
                    if let Some(mut handler) = me.control_mut().on_ended.take() {
                        handler(me, state, cx, &name);
                        me.control_mut().on_ended.get_or_insert(handler);
                        touched = true;
                    }
                }
                Event::Tapped(hit) => {
                    if let Some(mut handler) = me.control_mut().on_tapped.take() {
                        handler(me, state, cx, &hit);
                        me.control_mut().on_tapped.get_or_insert(handler);
                        touched = true;
                    }
                }
                Event::LoadState(load) => {
                    if let Some(mut handler) = me.control_mut().on_load_state.take() {
                        handler(me, state, cx, load);
                        me.control_mut().on_load_state.get_or_insert(handler);
                        touched = true;
                    }
                }
            }
        }
        let view = me.control_mut().view();
        if view != std::mem::replace(&mut me.control_mut().reported_view, view)
            && let Some(mut handler) = me.control_mut().on_view_changed.take()
        {
            handler(me, state, cx);
            me.control_mut().on_view_changed.get_or_insert(handler);
            touched = true;
        }
        let model = me.control_mut();
        let keep = model.wants_frames();
        model.ticking = keep;
        me.mark(Dirty::DRAW);
        FrameTick { keep, state_touched: touched }
    })
    .unwrap_or_default()
}

/// Rendering ahead (`Control::idle_gpu`): within one canvas height above or below the screen, as
/// DrawnUi.Net's Prefetch3D. The first frame on screen then finds the model, its images, the
/// program and the target uploaded.
fn prefetch_near(cx: &IdleGpuCx) -> bool {
    let h = cx.canvas_height;
    cx.rect.height() > 0.0 && cx.rect.bottom > -h && cx.rect.top < 2.0 * h
}

/// What rendering ahead is expected to cost, milliseconds: the first render of a target on WebGL
/// (Chrome, Windows) measured about 1 ms plus 1 ms per 30 000 pixels (85 x 85: 0.2-1.7 ms,
/// 333 x 238: 0.2-3.6 ms, 775 x 525: 12.8-14.4 ms).
fn prefetch_cost_ms(size: ISize) -> f32 {
    1.0 + (size.width * size.height) as f32 / 30_000.0
}

impl SkiaModel3D {
    /// The scene at `size` pixels, into its own target: on screen and ahead (`idle_gpu`).
    fn render(&self, gpu: &mut Gpu, size: ISize, scale: f32) -> Option<drawnui::skia::Image> {
        if size.width <= 0 || size.height <= 0 {
            return None;
        }
        let scene = self.scene.as_ref()?;
        let pose = scene.world(&self.locals(scene));
        let view_proj = self.camera(scene, (size.width as f32, size.height as f32));
        let hidden = self.hidden(scene);
        let style = render::Style { look: self.look, outline: self.outline.0 * scale, outline_color: outline_color(self.outline.1), antialias: self.antialias };
        let items = [render::Item { scene, pose: &pose, hidden: &hidden, materials: &self.materials }];
        let image = render::draw(&mut self.target.borrow_mut(), gpu, &items, &style, view_proj, size)?;
        self.gpu_ready.set(true);
        Some(image)
    }
}

impl Control for SkiaModel3D {
    fn on_props_changed(&mut self, cx: &mut Cx) {
        render::start_program();
        if self.scene.is_some() {
            self.start_named_animation();
        }
        self.load_source(cx);
        self.update_ticking(cx);
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let rect = cx.base().rect;
        let laid_out = (rect.width(), rect.height(), cx.scale.max(0.01));
        if std::mem::replace(&mut self.laid_out, laid_out) != laid_out && self.on_view_changed.is_some() {
            // Points on screen moved with the size: the app hears it as a view change.
            self.reported_view = (f32::NAN, f32::NAN, f32::NAN);
            self.update_ticking(&mut cx.cx());
        }
        // A scroll moving it back into view starts its time again.
        cx.track_viewport();
        if self.off_screen && !cx.visible_rect().is_empty() {
            self.off_screen = false;
            self.update_ticking(&mut cx.cx());
        }
        // Laid out and not drawn yet: it renders ahead once it is near the screen.
        if !self.gpu_ready.get() && !self.prefetch_queued {
            self.prefetch_queued = true;
            cx.request_idle_gpu();
        }
    }

    fn idle_gpu(&mut self, cx: &mut IdleGpuCx) -> IdleGpu {
        if self.gpu_ready.get() {
            return IdleGpu::Done;
        }
        if self.scene.is_none() || !prefetch_near(cx) {
            return IdleGpu::Wait;
        }
        let size = ISize::new(cx.rect.width().round() as i32, cx.rect.height().round() as i32);
        if !cx.fits(prefetch_cost_ms(size)) {
            return IdleGpu::Later;
        }
        self.prefetch_queued = false;
        if self.render(cx.gpu, size, cx.scale).is_some() { IdleGpu::Worked } else { IdleGpu::Done }
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let handled = match gesture.kind {
            GestureKind::Down => {
                (self.drag, self.pinch) = (None, None);
                // A finger put down catches a flung model.
                (self.fling_speed, self.dragged) = ((0.0, 0.0), false);
                self.velocity.clear();
                Handled::Yes
            }
            GestureKind::Up if self.fling && std::mem::take(&mut self.dragged) && self.pinch.is_none() => {
                // The drag's speed when let go, points per second, turned the way the drag turns it.
                let scale = cx.base().scale.max(0.1);
                let v = self.velocity.final_velocity(gesture.time_ms, 4000.0 * scale);
                self.start_fling(-v.x / scale * 0.5, v.y / scale * 0.3);
                Handled::Yes
            }
            GestureKind::Panning if self.pinch_zoom && gesture.manipulation.is_some() => {
                let step = (1.0 + gesture.manipulation.as_ref().map_or(0.0, |m| m.scale)).max(0.1);
                let (start, product) = self.pinch.get_or_insert((self.zoom, 1.0));
                *product *= step;
                let zoom = *start / *product;
                self.set_zoom_clamped(zoom);
                self.glide = None;
                Handled::Yes
            }
            // A gesture that pinched never turns the model: the fingers' own moves, and the finger
            // left after the other lifted, would jump the view.
            GestureKind::Panning if self.pinch.is_some() => Handled::Yes,
            // A drag on a card that leans toward the finger: mostly up or down is the scroll's around.
            GestureKind::Panning
                if self.tilt_to_pointer > 0.0 && !self.orbit && !*self.drag.get_or_insert(gesture.total.x.abs() >= gesture.total.y.abs() || !cx.scroll_around(true)) =>
            {
                return Handled::No;
            }
            GestureKind::Pointer | GestureKind::Panning if self.tilt_to_pointer > 0.0 && !self.orbit => {
                let rect = cx.base().rect;
                self.tilt_toward(cx.point.x - rect.left, cx.point.y - rect.top, (rect.width(), rect.height()));
                Handled::Yes
            }
            GestureKind::PointerExit if self.tilt_to_pointer > 0.0 => {
                self.start_glide();
                Handled::Yes
            }
            GestureKind::Up if self.tilt_to_pointer > 0.0 && gesture.touch => {
                self.start_glide();
                Handled::No
            }
            GestureKind::Panning if self.orbit => {
                // The direction is decided on the first move (past the slop) and kept for the whole
                // drag: mostly up or down is the scroll's around it (a model in a page never stops it,
                // as a web model viewer's touch-action pan-y), mostly sideways turns the model, its
                // vertical part tilting it too unless lock_pitch. With no scroll around to take it,
                // every drag is the model's.
                let turns = *self.drag.get_or_insert(gesture.total.x.abs() >= gesture.total.y.abs() || !cx.scroll_around(true));
                if !turns {
                    return Handled::No;
                }
                let scale = cx.base().scale.max(0.1);
                // The surface follows the finger: a drag to the right turns the front to the right.
                self.yaw -= gesture.delta.x / scale * 0.5;
                if self.fling {
                    self.dragged = true;
                    self.velocity.capture(gesture.velocity, gesture.time_ms);
                }
                if !self.lock_pitch {
                    self.pitch = (self.pitch + gesture.delta.y / scale * 0.3).clamp(-60.0, 80.0);
                }
                self.glide = None;
                Handled::Yes
            }
            // Only a wheel that asks to zoom (Ctrl / Cmd, a touchpad pinch): a plain wheel scrolls
            // the page around the model.
            GestureKind::Wheel if self.pinch_zoom && gesture.wheel_zoom && !gesture.wheel_horizontal => {
                self.set_zoom_clamped(self.zoom * 0.9f32.powf(gesture.wheel));
                self.glide = None;
                Handled::Yes
            }
            GestureKind::Tapped => {
                let double = self.reset_on_double_tap && self.last_tap_ms.is_some_and(|t| gesture.time_ms - t < DOUBLE_TAP_MS);
                self.last_tap_ms = if double { None } else { Some(gesture.time_ms) };
                if double {
                    self.start_glide();
                }
                let rect = cx.base().rect;
                let hit = if self.on_tapped.is_some() { self.hit(cx.point.x - rect.left, cx.point.y - rect.top, (rect.width(), rect.height())) } else { None };
                match hit {
                    Some(hit) => {
                        self.events.push(Event::Tapped(hit));
                        Handled::Tapped
                    }
                    None if double => Handled::Tapped,
                    None => Handled::No,
                }
            }
            _ => Handled::No,
        };
        if handled != Handled::No || self.glide.is_some() {
            cx.invalidate(Dirty::DRAW);
            self.update_ticking(&mut cx.cx());
        }
        handled
    }

    fn receives_hover(&self) -> bool {
        self.tilt_to_pointer > 0.0
    }

    fn paint(&self, cx: &mut PaintCx) {
        let rect = cx.rect;
        let size = ISize::new(rect.width().round() as i32, rect.height().round() as i32);
        let (canvas, scale) = (cx.canvas, cx.scale);
        let Some(gpu) = cx.gpu() else { return };
        let Some(image) = self.render(gpu, size, scale) else { return };
        canvas.draw_image_rect_with_sampling_options(&image, None, Rect::from_xywh(rect.left, rect.top, size.width as f32, size.height as f32), SamplingOptions::default(), &Paint::default());
    }
}

/// Building a `SkiaModel3D` (`use drawnui_add3d::prelude::*`).
pub trait Model3DBuild: Sized {
    /// The animation it loops from the start, by name (glTF animation name); the first one when
    /// not set.
    fn animation(self, name: impl Into<String>) -> Self;

    /// Plays the animations (on by default).
    fn playing(self, on: bool) -> Self;

    /// Animation speed, 1 = as authored.
    fn speed(self, speed: f32) -> Self;

    /// How long one animation blends into the next, milliseconds (250 by default, 0 = cut).
    fn crossfade_ms(self, ms: f32) -> Self;

    /// Turns the scene by itself, degrees per second.
    fn auto_rotate(self, degrees_per_second: f32) -> Self;

    /// Start angles: around the vertical axis and tilt, degrees.
    fn view_angles(self, yaw: f32, pitch: f32) -> Self;

    /// Distance as a factor of the one that fits the scene (below 1: closer).
    fn zoom(self, zoom: f32) -> Self;

    /// Drags turn the scene (on by default).
    fn orbit(self, on: bool) -> Self;

    /// How it is drawn: `Look::Lit` (default), `Look::Toon` or `Look::Flat`.
    fn look(self, look: Look) -> Self;

    /// An outline around every shape, `width` points, like an illustration's stroke.
    fn outline(self, width: f32, color: Color) -> Self;

    /// Smooth edges (4x multisampling, or the most the device has). Off by default: on costs about
    /// 28 more bytes of GPU memory per pixel of the control; the CPU time stays the same.
    fn antialias(self, on: bool) -> Self;

    /// Two fingers pinch to zoom, and the wheel with Ctrl (Cmd) or a touchpad pinch zooms; a plain
    /// wheel always goes to the scroll around it. Off by default.
    fn pinch_zoom(self, on: bool) -> Self;

    /// The closest zoom a pinch or the wheel reaches (0.5 by default; 1 = the model fits).
    fn min_zoom(self, zoom: f32) -> Self;

    /// The farthest zoom a pinch or the wheel reaches (2.5 by default).
    fn max_zoom(self, zoom: f32) -> Self;

    /// Two taps glide back to the home view (`Model3DSet::reset_view`). Off by default.
    fn reset_on_double_tap(self, on: bool) -> Self;

    /// Drags turn the model around its vertical axis only, never tilting it (3D in a list). Off by
    /// default. A drag that starts up or down is left to a scroll around the model, when there is one.
    fn lock_pitch(self, on: bool) -> Self;

    /// A drag that is let go keeps the model turning, slowing down as a scroll does after a swipe;
    /// a finger put down stops it. Off by default. With `auto_rotate` the turn goes on afterwards
    /// the way of the last swipe.
    fn fling(self, on: bool) -> Self;

    /// Room kept around the model's resting box, in the model's own units, for parts that move
    /// past it (dice that tumble and hop, swinging arms): the view backs off so they stay in sight.
    /// 0 by default.
    fn frame_margin(self, margin: f32) -> Self;

    /// The animations the view keeps in frame besides the model at rest. By default every
    /// animation of the file, so playing any of them never cuts the model; name only the ones the
    /// app plays when the file has bigger ones it never uses (a jump, a fall): the model shows larger.
    fn frame_animations(self, names: &[&str]) -> Self;

    /// The model turns toward the mouse over it, or the finger on it, up to `degrees` at the
    /// control's edge, and glides back home when the pointer leaves or the finger lifts (a card
    /// that follows the hand). Drags no longer turn it. 0 = off (the default).
    fn tilt_to_pointer(self, degrees: f32) -> Self;

    /// Runs once per frame in which the view moved (a drag, a pinch, auto-rotate, a reset, code)
    /// or the control got a new size, the first layout included: move labels anchored with
    /// `screen_position` here.
    fn on_view_changed<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>) + 'static) -> Self;

    /// Runs when loading from a url (`SkiaModel3D::from_url`, `set_source`) starts, succeeds or
    /// fails.
    fn on_load_state<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, LoadState3D) + 'static) -> Self;

    /// Starts with an image on a material (see `Model3DSet::set_material_image`).
    fn material_image(self, name: &str, image: Texture3D) -> Self;

    /// Runs when an animation played with `play_once` or `hold` ended, with its name.
    fn on_animation_ended<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self;

    /// Runs when a tap lands on the scene's surface (not on empty space around it), with what it
    /// hit: the node, the material and the point.
    fn on_tapped_3d<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &ModelHit) + 'static) -> Self;
}

impl Model3DBuild for Build<SkiaModel3D> {
    fn animation(mut self, name: impl Into<String>) -> Self {
        self.control_mut().start_animation = Some(name.into());
        self
    }

    fn playing(mut self, on: bool) -> Self {
        self.control_mut().player.playing = on;
        self
    }

    fn speed(mut self, speed: f32) -> Self {
        self.control_mut().player.speed = speed;
        self
    }

    fn crossfade_ms(mut self, ms: f32) -> Self {
        self.control_mut().player.fade = ms.max(0.0) / 1000.0;
        self
    }

    fn auto_rotate(mut self, degrees_per_second: f32) -> Self {
        self.control_mut().auto_rotate = degrees_per_second;
        self
    }

    fn view_angles(mut self, yaw: f32, pitch: f32) -> Self {
        let model = self.control_mut();
        (model.yaw, model.pitch) = (yaw, pitch);
        (model.home.0, model.home.1) = (yaw, pitch);
        model.reported_view = model.view();
        self
    }

    fn material_image(mut self, name: &str, image: Texture3D) -> Self {
        let model = self.control_mut();
        if let Some(i) = model.material_named(name) {
            model.materials[i].image = Some(image);
        }
        self
    }

    fn zoom(mut self, zoom: f32) -> Self {
        let model = self.control_mut();
        model.zoom = zoom.max(0.05);
        model.home.2 = model.zoom;
        model.reported_view = model.view();
        self
    }

    fn orbit(mut self, on: bool) -> Self {
        self.control_mut().orbit = on;
        self
    }

    fn look(mut self, look: Look) -> Self {
        self.control_mut().look = look;
        self
    }

    fn outline(mut self, width: f32, color: Color) -> Self {
        self.control_mut().outline = (width.max(0.0), color);
        self
    }

    fn antialias(mut self, on: bool) -> Self {
        self.control_mut().antialias = on;
        self
    }

    fn pinch_zoom(mut self, on: bool) -> Self {
        self.control_mut().pinch_zoom = on;
        self
    }

    fn min_zoom(mut self, zoom: f32) -> Self {
        self.control_mut().zoom_range.0 = zoom.max(0.05);
        self
    }

    fn max_zoom(mut self, zoom: f32) -> Self {
        self.control_mut().zoom_range.1 = zoom.max(0.05);
        self
    }

    fn reset_on_double_tap(mut self, on: bool) -> Self {
        self.control_mut().reset_on_double_tap = on;
        self
    }

    fn lock_pitch(mut self, on: bool) -> Self {
        self.control_mut().lock_pitch = on;
        self
    }

    fn fling(mut self, on: bool) -> Self {
        self.control_mut().fling = on;
        self
    }

    fn frame_margin(mut self, margin: f32) -> Self {
        self.control_mut().frame_margin = margin.max(0.0);
        self
    }

    fn frame_animations(mut self, names: &[&str]) -> Self {
        let model = self.control_mut();
        model.frame_animations = Some(names.iter().map(|n| (*n).to_owned()).collect());
        *model.framing.get_mut() = None;
        self
    }

    fn tilt_to_pointer(mut self, degrees: f32) -> Self {
        let model = self.control_mut();
        model.tilt_to_pointer = degrees.max(0.0);
        model.orbit &= degrees <= 0.0;
        self
    }

    fn on_view_changed<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.control_mut().on_view_changed = Some(Box::new(move |me, state, cx| f(me, downcast_state::<S>(state), cx)));
        self
    }

    fn on_load_state<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, LoadState3D) + 'static) -> Self {
        self.control_mut().on_load_state = Some(Box::new(move |me, state, cx, load| f(me, downcast_state::<S>(state), cx, load)));
        self
    }

    fn on_animation_ended<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &str) + 'static) -> Self {
        self.control_mut().on_ended = Some(Box::new(move |me, state, cx, name| f(me, downcast_state::<S>(state), cx, name)));
        self
    }

    fn on_tapped_3d<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaModel3D>, &mut S, &mut Cx<'_>, &ModelHit) + 'static) -> Self {
        self.control_mut().on_tapped = Some(Box::new(move |me, state, cx, hit| f(me, downcast_state::<S>(state), cx, hit)));
        self
    }
}

/// Driving a `SkiaModel3D` from handlers (`cx.get_mut(handle)`), `use drawnui_add3d::prelude::*`.
pub trait Model3DSet {
    /// How it is drawn from now on (see `Model3DBuild::look`).
    fn set_look(&mut self, look: Look);

    /// An outline around every shape, `width` points (0 = none), from now on.
    fn set_outline(&mut self, width: f32, color: Color);

    /// Loops the animation with this name, blending into it; false when the scene has none such.
    fn play(&mut self, name: &str) -> bool;

    /// Plays the animation with this name once, then blends back into the looping one that
    /// played before it (an attack, then idle again); `on_animation_ended` hears its end. With
    /// nothing looping before, it holds its last frame (a death). False when there is none such.
    fn play_once(&mut self, name: &str) -> bool;

    /// Plays the animation once and stays on its last frame; nothing goes back.
    fn hold(&mut self, name: &str) -> bool;

    /// Loops another animation together with the main one (another object of the scene: a coin
    /// spinning while the boss attacks). False when the scene has none such.
    fn play_layer(&mut self, name: &str) -> bool;

    /// Stops an animation started with `play_layer`.
    fn stop_layer(&mut self, name: &str);

    /// Stops or resumes all animations where they are.
    fn set_playing(&mut self, on: bool);

    /// Animation speed, 1 = as authored.
    fn set_speed(&mut self, speed: f32);

    /// The view's angles: around the vertical axis and tilt, degrees (a spin driven by the app).
    fn set_view_angles(&mut self, yaw: f32, pitch: f32);

    /// Turns the scene by itself, degrees per second (0 = still).
    fn set_auto_rotate(&mut self, degrees_per_second: f32);

    /// Shows or hides a node by name, with what hangs under it. False when there is none such.
    fn set_node_visible(&mut self, name: &str, visible: bool) -> bool;

    /// Moves a node by name: its own position, in its parent's units. It replaces what the file
    /// and the animations give it.
    fn set_node_translation(&mut self, name: &str, xyz: [f32; 3]) -> bool;

    /// Turns a node by name: rotations around x, y, z in degrees (applied in that order).
    fn set_node_rotation(&mut self, name: &str, degrees: [f32; 3]) -> bool;

    /// Scales a node by name.
    fn set_node_scale(&mut self, name: &str, xyz: [f32; 3]) -> bool;

    /// Gives a node back what the file and the animations say: position, turn, scale, shown.
    fn reset_node(&mut self, name: &str) -> bool;

    /// Recolors a material by name (a part of the model: skin, armor); `None` = the file's color.
    /// A texture is tinted by it. False when there is none such.
    fn set_material_color(&mut self, name: &str, color: Option<Color>) -> bool;

    /// Shows or hides every surface with this material. False when there is none such.
    fn set_material_visible(&mut self, name: &str, visible: bool) -> bool;

    /// How matte a material is, glTF roughness: 1 = no shine, lower = a brighter, tighter highlight
    /// (0.4 for leather); `None` = the file's. False when there is no such material.
    fn set_material_roughness(&mut self, name: &str, roughness: Option<f32>) -> bool;

    /// Puts an image on a material (a skin, a label, a photo, a pattern), in place of the file's
    /// own; `None` = the file's again. The image is multiplied by the material's color: a white
    /// material (or `set_material_color(name, Some(Color::WHITE))`) shows it as it is. False when
    /// there is no such material.
    fn set_material_image(&mut self, name: &str, image: Option<Texture3D>) -> bool;

    /// Smooth edges on or off (see `Model3DBuild::antialias`).
    fn set_antialias(&mut self, on: bool);

    /// Distance to the model: 1 = it fits, less = closer. Becomes the home zoom.
    fn set_zoom(&mut self, zoom: f32);

    /// Glides (300 ms) back to the home view: the angles and zoom the app set last.
    fn reset_view(&mut self);

    fn set_pinch_zoom(&mut self, on: bool);
    fn set_min_zoom(&mut self, zoom: f32);
    fn set_max_zoom(&mut self, zoom: f32);
    fn set_reset_on_double_tap(&mut self, on: bool);
    fn set_lock_pitch(&mut self, on: bool);
    /// See `Model3DBuild::fling`; off stops a fling under way.
    fn set_fling(&mut self, on: bool);
    /// See `Model3DBuild::frame_margin`.
    fn set_frame_margin(&mut self, margin: f32);
    /// See `Model3DBuild::frame_animations`; `None` keeps every animation in frame.
    fn set_frame_animations(&mut self, names: Option<&[&str]>);

    /// Loads the model from a url instead (see `SkiaModel3D::from_url`).
    fn set_source(&mut self, url: &str);
}

impl Model3DSet for Mut<'_, SkiaModel3D> {
    fn set_look(&mut self, look: Look) {
        self.control_mut().look = look;
        self.mark(Dirty::DRAW);
    }

    fn set_outline(&mut self, width: f32, color: Color) {
        self.control_mut().outline = (width.max(0.0), color);
        self.mark(Dirty::DRAW);
    }

    fn play(&mut self, name: &str) -> bool {
        with_animation(self, name, Player::play)
    }

    fn play_once(&mut self, name: &str) -> bool {
        with_animation(self, name, Player::play_once)
    }

    fn hold(&mut self, name: &str) -> bool {
        with_animation(self, name, Player::hold)
    }

    fn play_layer(&mut self, name: &str) -> bool {
        with_animation(self, name, Player::add_layer)
    }

    fn stop_layer(&mut self, name: &str) {
        with_animation(self, name, Player::remove_layer);
    }

    fn set_playing(&mut self, on: bool) {
        self.control_mut().player.playing = on;
        self.mark(Dirty::DRAW_APPLY);
    }

    fn set_speed(&mut self, speed: f32) {
        self.control_mut().player.speed = speed;
    }

    fn set_view_angles(&mut self, yaw: f32, pitch: f32) {
        let model = self.control_mut();
        (model.yaw, model.pitch) = (yaw, pitch);
        (model.home.0, model.home.1) = (yaw, pitch);
        model.glide = None;
        // APPLY: a view-changed handler hears it.
        self.mark(Dirty::DRAW_APPLY);
    }

    fn set_auto_rotate(&mut self, degrees_per_second: f32) {
        self.control_mut().auto_rotate = degrees_per_second;
        self.mark(Dirty::APPLY);
    }

    fn set_node_visible(&mut self, name: &str, visible: bool) -> bool {
        with_node(self, name, |n| n.hidden = !visible)
    }

    fn set_node_translation(&mut self, name: &str, xyz: [f32; 3]) -> bool {
        with_node(self, name, |n| n.translation = Some(Vec3::from(xyz)))
    }

    fn set_node_rotation(&mut self, name: &str, degrees: [f32; 3]) -> bool {
        let [x, y, z] = degrees.map(f32::to_radians);
        with_node(self, name, |n| n.rotation = Some(Quat::from_euler(glam::EulerRot::XYZ, x, y, z)))
    }

    fn set_node_scale(&mut self, name: &str, xyz: [f32; 3]) -> bool {
        with_node(self, name, |n| n.scale = Some(Vec3::from(xyz)))
    }

    fn reset_node(&mut self, name: &str) -> bool {
        with_node(self, name, |n| *n = NodeOverride::default())
    }

    fn set_material_color(&mut self, name: &str, color: Option<Color>) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].color = color.map(linear);
        self.mark(Dirty::DRAW);
        true
    }

    fn set_material_visible(&mut self, name: &str, visible: bool) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].hidden = !visible;
        self.mark(Dirty::DRAW);
        true
    }

    fn set_material_roughness(&mut self, name: &str, roughness: Option<f32>) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].roughness = roughness.map(|r| r.clamp(0.0, 1.0));
        self.mark(Dirty::DRAW);
        true
    }

    fn set_material_image(&mut self, name: &str, image: Option<Texture3D>) -> bool {
        let model = self.control_mut();
        let Some(i) = model.material_named(name) else { return false };
        model.materials[i].image = image;
        self.mark(Dirty::DRAW);
        true
    }

    fn set_antialias(&mut self, on: bool) {
        self.control_mut().antialias = on;
        self.mark(Dirty::DRAW);
    }

    fn set_zoom(&mut self, zoom: f32) {
        let model = self.control_mut();
        model.zoom = zoom.max(0.05);
        model.home.2 = model.zoom;
        model.glide = None;
        self.mark(Dirty::DRAW_APPLY);
    }

    fn reset_view(&mut self) {
        self.control_mut().start_glide();
        self.mark(Dirty::DRAW_APPLY);
    }

    fn set_pinch_zoom(&mut self, on: bool) {
        self.control_mut().pinch_zoom = on;
    }

    fn set_min_zoom(&mut self, zoom: f32) {
        self.control_mut().zoom_range.0 = zoom.max(0.05);
    }

    fn set_max_zoom(&mut self, zoom: f32) {
        self.control_mut().zoom_range.1 = zoom.max(0.05);
    }

    fn set_reset_on_double_tap(&mut self, on: bool) {
        self.control_mut().reset_on_double_tap = on;
    }

    fn set_lock_pitch(&mut self, on: bool) {
        self.control_mut().lock_pitch = on;
    }

    fn set_fling(&mut self, on: bool) {
        let model = self.control_mut();
        model.fling = on;
        if !on {
            model.fling_speed = (0.0, 0.0);
        }
    }

    fn set_frame_margin(&mut self, margin: f32) {
        self.control_mut().frame_margin = margin.max(0.0);
    }

    fn set_frame_animations(&mut self, names: Option<&[&str]>) {
        let model = self.control_mut();
        model.frame_animations = names.map(|n| n.iter().map(|n| (*n).to_owned()).collect());
        *model.framing.get_mut() = None;
    }

    fn set_source(&mut self, url: &str) {
        let model = self.control_mut();
        model.source = Some(url.to_owned());
        model.load_state = LoadState3D::Loading;
        self.mark(Dirty::DRAW_APPLY);
    }
}

fn with_animation(me: &mut Mut<'_, SkiaModel3D>, name: &str, f: impl FnOnce(&mut Player, usize)) -> bool {
    let model = me.control_mut();
    let Some(animation) = model.animation_named(name) else { return false };
    f(&mut model.player, animation);
    me.mark(Dirty::DRAW_APPLY);
    true
}

fn with_node(me: &mut Mut<'_, SkiaModel3D>, name: &str, f: impl FnOnce(&mut NodeOverride)) -> bool {
    let model = me.control_mut();
    let Some(i) = model.node_named(name) else { return false };
    f(&mut model.nodes[i]);
    me.mark(Dirty::DRAW);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAGON: &[u8] = include_bytes!("../../examples/model3d/assets/dragon.glb");

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
    fn smooth_edges_take_4_samples_or_the_most_the_device_has() {
        assert_eq!(render::msaa_samples(16), 4);
        assert_eq!(render::msaa_samples(4), 4);
        assert_eq!(render::msaa_samples(2), 2);
        assert_eq!(render::msaa_samples(1), 0, "one sample is no smoothing: the plain path");
        assert_eq!(render::msaa_samples(0), 0);
    }

    #[test]
    fn the_same_bytes_are_read_once() {
        let (a, b) = (scene::shared(DRAGON).unwrap(), scene::shared(DRAGON).unwrap());
        assert!(Rc::ptr_eq(&a, &b));
    }
}
