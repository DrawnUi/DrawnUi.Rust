//! SkiaScene3D: a 3D layer over a 2D game field. Many objects, each a model with its own place,
//! size, facing and animations, are drawn in one pass into one texture; each model is read and
//! uploaded once however many objects use it. Objects are placed in the layer's own points, like
//! sprites: (x, y) is where an object stands (the bottom center of its model), `size` its height.
//! An object lower on the field is drawn in front. The view is straight on, tilted forward by
//! `tilt` degrees on each object, so a game moves them as it moves 2D sprites.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glam::{Mat4, Quat, Vec3};
use drawnui::skia::{Color, ISize, Paint, Rect, SamplingOptions};

use super::player::Player;
use super::render;
use super::scene::{self, MaterialOverride, Scene};
use drawnui::{Build, Control, ControlId, Cx, Dirty, FrameTick, Gesture, GestureCx, GestureKind, Gpu, Handled, IdleGpu, IdleGpuCx, LayoutCx, Mut, PaintCx, downcast_state};

/// What a tap on the layer hit (`SkiaScene3D::on_tapped_3d`): the object by its name, and the node
/// and material of its model under the finger.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneHit {
    pub object: String,
    pub node: String,
    pub material: String,
}

type EndedHandler = Box<dyn FnMut(&mut Mut<'_, SkiaScene3D>, &mut dyn Any, &mut Cx<'_>, &str, &str)>;
type TappedHandler = Box<dyn FnMut(&mut Mut<'_, SkiaScene3D>, &mut dyn Any, &mut Cx<'_>, &SceneHit)>;

/// A model the layer can place: its scene, the point it stands on and its height at rest.
struct Model {
    name: String,
    scene: Rc<Scene>,
    anchor: Vec3,
    height: f32,
}

struct Object {
    name: String,
    model: usize,
    x: f32,
    y: f32,
    size: f32,
    facing: f32,
    visible: bool,
    player: Player,
    materials: Vec<MaterialOverride>,
}

enum Event {
    Ended(String, String),
    Tapped(SceneHit),
}

/// A 3D layer over a 2D game field (see the module docs).
pub struct SkiaScene3D {
    id: ControlId,
    models: Vec<Model>,
    objects: Vec<Object>,
    target: RefCell<Option<render::Target>>,
    /// It rendered once (on screen or ahead): its models and images are on the GPU.
    gpu_ready: Cell<bool>,
    /// Waiting to render ahead (`LayoutCx::request_idle_gpu`).
    prefetch_queued: bool,
    /// Forward tilt of every object, degrees: how much of its top the player sees.
    tilt: f32,
    playing: bool,
    last_ms: Option<f64>,
    ticking: bool,
    /// Nothing of it is on screen: animations stand still and no frames are asked for, until a
    /// scroll brings it back.
    off_screen: bool,
    events: Vec<Event>,
    on_ended: Option<EndedHandler>,
    on_tapped: Option<TappedHandler>,
    /// Files that could not be read: the model name and why.
    errors: Vec<(String, String)>,
    look: super::Look,
    /// Outline width in points (0 = none) and color.
    outline: (f32, Color),
    /// Smooth edges (multisampling).
    antialias: bool,
}

impl SkiaScene3D {
    /// An empty layer: give it models with `model`, then place objects with `add`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaScene3D> {
        let layer = SkiaScene3D {
            id: ControlId::from_bits(0),
            models: Vec::new(),
            objects: Vec::new(),
            target: RefCell::new(None),
            gpu_ready: Cell::new(false),
            prefetch_queued: false,
            tilt: 15.0,
            playing: true,
            last_ms: None,
            ticking: false,
            off_screen: false,
            events: Vec::new(),
            on_ended: None,
            on_tapped: None,
            errors: Vec::new(),
            look: super::Look::Lit,
            outline: (0.0, Color::BLACK),
            antialias: false,
        };
        let mut build = Build::new(layer);
        let id = build.id();
        build.control_mut().id = id;
        build
    }

    /// The objects' names, in the order they were added.
    pub fn object_names(&self) -> Vec<String> {
        self.objects.iter().map(|o| o.name.clone()).collect()
    }

    /// Models whose files could not be read: the model name and why.
    pub fn errors(&self) -> &[(String, String)] {
        &self.errors
    }

    /// The main animation of an object, by name.
    pub fn current_animation(&self, object: &str) -> Option<&str> {
        let o = self.objects.iter().find(|o| o.name == object)?;
        Some(self.models[o.model].scene.animations.get(o.player.main?.animation)?.name.as_str())
    }

    fn add_model(&mut self, name: String, read: Result<Rc<Scene>, String>) {
        match read {
            Ok(scene) => {
                let (lo, hi) = scene.bounds;
                let model = Model { name: name.clone(), anchor: Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5), height: (hi.y - lo.y).max(1e-3), scene };
                match self.models.iter_mut().find(|m| m.name == name) {
                    Some(old) => *old = model,
                    None => self.models.push(model),
                }
            }
            Err(why) => self.errors.push((name, why)),
        }
    }

    fn wants_frames(&self) -> bool {
        !self.events.is_empty() || (self.playing && !self.off_screen && self.objects.iter().any(|o| o.visible && o.player.busy()))
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

    fn advance(&mut self, delta: f32) {
        if !self.playing {
            return;
        }
        for o in &mut self.objects {
            let scene = &self.models[o.model].scene;
            if let Some(ended) = o.player.advance(scene, delta) {
                self.events.push(Event::Ended(o.name.clone(), scene.animations[ended].name.clone()));
            }
        }
    }

    /// The object's place in the layer: points, y down on the field, nearer the lower it stands.
    fn placement(&self, o: &Object) -> Mat4 {
        let model = &self.models[o.model];
        let scale = o.size / model.height;
        Mat4::from_translation(Vec3::new(o.x, -o.y, o.y))
            * Mat4::from_quat(Quat::from_rotation_x(self.tilt.to_radians()) * Quat::from_rotation_y(o.facing.to_radians()))
            * Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_translation(-model.anchor)
    }

    /// Every node's world matrix of an object, its place included.
    fn pose(&self, o: &Object) -> Vec<Mat4> {
        let scene = &self.models[o.model].scene;
        let place = self.placement(o);
        scene.world(&o.player.locals(scene)).into_iter().map(|m| place * m).collect()
    }

    /// A straight-on view of the layer in points (`size`), deep enough for any object.
    fn camera(size: (f32, f32)) -> Mat4 {
        let depth = (size.0 + size.1) * 4.0 + 10_000.0;
        Mat4::orthographic_rh_gl(0.0, size.0, -size.1, 0.0, -depth, depth)
    }

    /// The nearest object surface under a point of the layer (points from its corner).
    fn hit(&self, x: f32, y: f32, size: (f32, f32)) -> Option<SceneHit> {
        let (from, dir) = scene::ray(Self::camera(size), x / size.0, y / size.1);
        let mut best: Option<(f32, SceneHit)> = None;
        for o in self.objects.iter().filter(|o| o.visible) {
            let scene = &self.models[o.model].scene;
            let hidden = vec![false; scene.nodes.len()];
            let Some((node, primitive, t)) = scene.ray_hit(&self.pose(o), &hidden, &o.materials, from, dir) else { continue };
            if best.as_ref().is_none_or(|(d, _)| t < *d) {
                best = Some((t, SceneHit { object: o.name.clone(), node: scene.nodes[node].name.clone(), material: scene.material_of(node, primitive) }));
            }
        }
        best.map(|(_, hit)| hit)
    }
}

/// One frame: the objects' animations advance, then the app hears what happened.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let off_screen = cx.visible_rect(id).is_empty();
    cx.with_control::<SkiaScene3D, _>(id, state, |me, state, cx| {
        let layer = me.control_mut();
        let delta = layer.last_ms.map_or(0.0, |last| ((time_ms - last) / 1000.0) as f32);
        layer.last_ms = Some(time_ms);
        layer.off_screen = off_screen;
        if !off_screen {
            layer.advance(delta);
        }
        let events = std::mem::take(&mut layer.events);
        let mut touched = false;
        for event in events {
            match event {
                Event::Ended(object, animation) => {
                    if let Some(mut handler) = me.control_mut().on_ended.take() {
                        handler(me, state, cx, &object, &animation);
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
            }
        }
        let layer = me.control_mut();
        let keep = layer.wants_frames();
        layer.ticking = keep;
        me.mark(Dirty::DRAW);
        FrameTick { keep, state_touched: touched }
    })
    .unwrap_or_default()
}

impl Control for SkiaScene3D {
    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.update_ticking(cx);
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
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
        if self.objects.is_empty() || !super::prefetch_near(cx) {
            return IdleGpu::Wait;
        }
        let size = ISize::new(cx.rect.width().round() as i32, cx.rect.height().round() as i32);
        self.prefetch_queued = false;
        if self.render(cx.gpu, size, cx.scale).is_some() { IdleGpu::Worked } else { IdleGpu::Done }
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if self.on_tapped.is_none() {
            return Handled::No;
        }
        match gesture.kind {
            GestureKind::Tapped => {
                let (rect, scale) = (cx.base().rect, cx.base().scale.max(0.1));
                let size = (rect.width() / scale, rect.height() / scale);
                match self.hit((cx.point.x - rect.left) / scale, (cx.point.y - rect.top) / scale, size) {
                    Some(hit) => {
                        self.events.push(Event::Tapped(hit));
                        self.update_ticking(&mut cx.cx());
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
        let (canvas, scale) = (cx.canvas, cx.scale);
        let Some(gpu) = cx.gpu() else { return };
        let Some(image) = self.render(gpu, size, scale) else { return };
        canvas.draw_image_rect_with_sampling_options(&image, None, Rect::from_xywh(rect.left, rect.top, size.width as f32, size.height as f32), SamplingOptions::default(), &Paint::default());
    }
}

impl SkiaScene3D {
    /// The visible objects at `size` pixels, into its own target: on screen and ahead (`idle_gpu`).
    fn render(&self, gpu: &mut Gpu, size: ISize, scale: f32) -> Option<drawnui::skia::Image> {
        if size.width <= 0 || size.height <= 0 || self.objects.is_empty() {
            return None;
        }
        let scale = scale.max(0.1);
        let view_proj = Self::camera((size.width as f32 / scale, size.height as f32 / scale));
        let visible: Vec<&Object> = self.objects.iter().filter(|o| o.visible).collect();
        let poses: Vec<Vec<Mat4>> = visible.iter().map(|o| self.pose(o)).collect();
        let hidden: Vec<Vec<bool>> = visible.iter().map(|o| vec![false; self.models[o.model].scene.nodes.len()]).collect();
        let items: Vec<render::Item> = visible
            .iter()
            .zip(&poses)
            .zip(&hidden)
            .map(|((o, pose), hidden)| render::Item { scene: &self.models[o.model].scene, pose, hidden, materials: &o.materials })
            .collect();
        let style = render::Style { look: self.look, outline: self.outline.0 * scale, outline_color: super::outline_color(self.outline.1), antialias: self.antialias };
        let image = render::draw(&mut self.target.borrow_mut(), gpu, &items, &style, view_proj, size)?;
        self.gpu_ready.set(true);
        Some(image)
    }
}

/// Building a `SkiaScene3D` (`use drawnui_add3d::prelude::*`).
pub trait Scene3DBuild: Sized {
    /// A model objects can be made of, by name, from the bytes of a `.glb`. The same bytes used by
    /// other layers or `SkiaModel3D` controls are read once.
    fn model(self, name: impl Into<String>, glb: &[u8]) -> Self;

    /// A model made in code (`Model3D`) objects can be made of, by name.
    fn model_from(self, name: impl Into<String>, model: &super::Model3D) -> Self;

    /// How objects are drawn: `Look::Lit` (default), `Look::Toon` or `Look::Flat`.
    fn look(self, look: super::Look) -> Self;

    /// An outline around every shape, `width` points, like an illustration's stroke.
    fn outline(self, width: f32, color: Color) -> Self;

    /// Smooth edges (see `Model3DBuild::antialias`); off by default.
    fn antialias(self, on: bool) -> Self;

    /// Forward tilt of every object, degrees (15 by default): how much of its top the player sees.
    fn tilt(self, degrees: f32) -> Self;

    /// Runs when an object's animation played with `play_once` or `hold` ended: the object's name
    /// and the animation's.
    fn on_animation_ended<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaScene3D>, &mut S, &mut Cx<'_>, &str, &str) + 'static) -> Self;

    /// Runs when a tap lands on an object (the nearest one under the finger), with what it hit. Taps
    /// on empty field fall through to what is behind the layer.
    fn on_tapped_3d<S: Any>(self, f: impl FnMut(&mut Mut<'_, SkiaScene3D>, &mut S, &mut Cx<'_>, &SceneHit) + 'static) -> Self;
}

impl Scene3DBuild for Build<SkiaScene3D> {
    fn model(mut self, name: impl Into<String>, glb: &[u8]) -> Self {
        self.control_mut().add_model(name.into(), scene::shared(glb));
        self
    }

    fn model_from(mut self, name: impl Into<String>, model: &super::Model3D) -> Self {
        self.control_mut().add_model(name.into(), Ok(Rc::new(model.to_scene())));
        self
    }

    fn look(mut self, look: super::Look) -> Self {
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

    fn tilt(mut self, degrees: f32) -> Self {
        self.control_mut().tilt = degrees;
        self
    }

    fn on_animation_ended<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaScene3D>, &mut S, &mut Cx<'_>, &str, &str) + 'static) -> Self {
        self.control_mut().on_ended = Some(Box::new(move |me, state, cx, object, animation| f(me, downcast_state::<S>(state), cx, object, animation)));
        self
    }

    fn on_tapped_3d<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaScene3D>, &mut S, &mut Cx<'_>, &SceneHit) + 'static) -> Self {
        self.control_mut().on_tapped = Some(Box::new(move |me, state, cx, hit| f(me, downcast_state::<S>(state), cx, hit)));
        self
    }
}

/// Driving a `SkiaScene3D` from handlers (`cx.get_mut(handle)`), `use drawnui_add3d::prelude::*`.
pub trait Scene3DSet {
    /// Places an object made of `model` (a name given to `model`), standing at (x, y) of the layer
    /// in points, `size` points tall; it loops its model's first animation. The same name replaces
    /// an object. False when there is no such model.
    fn add(&mut self, name: &str, model: &str, x: f32, y: f32, size: f32) -> bool;

    /// Removes an object; false when there is none such.
    fn remove(&mut self, name: &str) -> bool;

    /// Removes every object.
    fn clear(&mut self);

    /// Moves an object: where it stands, in the layer's points.
    fn set_position(&mut self, name: &str, x: f32, y: f32) -> bool;

    /// Turns an object around its vertical axis, degrees (0 = facing the player, 90 = to the right).
    fn set_facing(&mut self, name: &str, degrees: f32) -> bool;

    /// An object's height in points.
    fn set_size(&mut self, name: &str, size: f32) -> bool;

    /// Shows or hides an object (a hidden one asks for no frames).
    fn set_visible(&mut self, name: &str, visible: bool) -> bool;

    /// Loops an object's animation by name, blending into it.
    fn play(&mut self, name: &str, animation: &str) -> bool;

    /// Plays an object's animation once, then back to the loop it played before.
    fn play_once(&mut self, name: &str, animation: &str) -> bool;

    /// Plays an object's animation once and holds its last frame.
    fn hold(&mut self, name: &str, animation: &str) -> bool;

    /// An object's animation speed, 1 = as authored (vary it so a crowd does not move in step).
    fn set_speed(&mut self, name: &str, speed: f32) -> bool;

    /// Recolors one material of one object; `None` = the file's color.
    fn set_material_color(&mut self, name: &str, material: &str, color: Option<Color>) -> bool;

    /// Puts an image on one material of one object (see `Model3DSet::set_material_image`); `None` =
    /// the file's again.
    fn set_material_image(&mut self, name: &str, material: &str, image: Option<super::Texture3D>) -> bool;

    /// Stops or resumes every animation of the layer.
    fn set_playing(&mut self, on: bool);

    /// Smooth edges on or off.
    fn set_antialias(&mut self, on: bool);
}

impl Scene3DSet for Mut<'_, SkiaScene3D> {
    fn add(&mut self, name: &str, model: &str, x: f32, y: f32, size: f32) -> bool {
        let layer = self.control_mut();
        let Some(m) = layer.models.iter().position(|m| m.name == model) else { return false };
        let scene = &layer.models[m].scene;
        let object = Object {
            name: name.to_owned(),
            model: m,
            x,
            y,
            size,
            facing: 0.0,
            visible: true,
            player: Player::new(Some(scene)),
            materials: vec![MaterialOverride::default(); scene.materials.len()],
        };
        match layer.objects.iter_mut().find(|o| o.name == name) {
            Some(old) => *old = object,
            None => layer.objects.push(object),
        }
        self.mark(Dirty::DRAW_APPLY);
        true
    }

    fn remove(&mut self, name: &str) -> bool {
        let layer = self.control_mut();
        let before = layer.objects.len();
        layer.objects.retain(|o| o.name != name);
        let removed = layer.objects.len() != before;
        self.mark(Dirty::DRAW_APPLY);
        removed
    }

    fn clear(&mut self) {
        self.control_mut().objects.clear();
        self.mark(Dirty::DRAW_APPLY);
    }

    fn set_position(&mut self, name: &str, x: f32, y: f32) -> bool {
        with_object(self, name, Dirty::DRAW, |o, _| {
            (o.x, o.y) = (x, y);
            true
        })
    }

    fn set_facing(&mut self, name: &str, degrees: f32) -> bool {
        with_object(self, name, Dirty::DRAW, |o, _| {
            o.facing = degrees;
            true
        })
    }

    fn set_size(&mut self, name: &str, size: f32) -> bool {
        with_object(self, name, Dirty::DRAW, |o, _| {
            o.size = size;
            true
        })
    }

    fn set_visible(&mut self, name: &str, visible: bool) -> bool {
        with_object(self, name, Dirty::DRAW_APPLY, |o, _| {
            o.visible = visible;
            true
        })
    }

    fn play(&mut self, name: &str, animation: &str) -> bool {
        with_object(self, name, Dirty::DRAW_APPLY, |o, s| s.animation_named(animation).map(|a| o.player.play(a)).is_some())
    }

    fn play_once(&mut self, name: &str, animation: &str) -> bool {
        with_object(self, name, Dirty::DRAW_APPLY, |o, s| s.animation_named(animation).map(|a| o.player.play_once(a)).is_some())
    }

    fn hold(&mut self, name: &str, animation: &str) -> bool {
        with_object(self, name, Dirty::DRAW_APPLY, |o, s| s.animation_named(animation).map(|a| o.player.hold(a)).is_some())
    }

    fn set_speed(&mut self, name: &str, speed: f32) -> bool {
        with_object(self, name, Dirty::NONE, |o, _| {
            o.player.speed = speed;
            true
        })
    }

    fn set_material_color(&mut self, name: &str, material: &str, color: Option<Color>) -> bool {
        with_object(self, name, Dirty::DRAW, |o, s| match s.materials.iter().position(|m| m.name == material) {
            Some(i) => {
                o.materials[i].color = color.map(super::linear);
                true
            }
            None => false,
        })
    }

    fn set_material_image(&mut self, name: &str, material: &str, image: Option<super::Texture3D>) -> bool {
        with_object(self, name, Dirty::DRAW, |o, s| match s.materials.iter().position(|m| m.name == material) {
            Some(i) => {
                o.materials[i].image = image;
                true
            }
            None => false,
        })
    }

    fn set_playing(&mut self, on: bool) {
        self.control_mut().playing = on;
        self.mark(Dirty::DRAW_APPLY);
    }

    fn set_antialias(&mut self, on: bool) {
        self.control_mut().antialias = on;
        self.mark(Dirty::DRAW);
    }
}

fn with_object(me: &mut Mut<'_, SkiaScene3D>, name: &str, dirty: Dirty, f: impl FnOnce(&mut Object, &Scene) -> bool) -> bool {
    let layer = me.control_mut();
    let Some(o) = layer.objects.iter_mut().find(|o| o.name == name) else { return false };
    let done = f(o, &layer.models[o.model].scene);
    me.mark(dirty);
    done
}
