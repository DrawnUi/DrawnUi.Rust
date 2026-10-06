//! Visual effects (DrawnUI VisualEffects): objects attached to a control through
//! `Base::visual_effects`. A post renderer draws over what the control painted (SkiaShaderEffect:
//! an SkSL shader over the control's output), an effect may report a margin it paints outside
//! the rect, and it may take the control's gestures before the control does (ISkiaGestureProcessor).
//!
//! The control's own cache holds its content without the effect: a post renderer samples that
//! cache and draws where the cache would be blitted, so a cache above the control records the
//! effect's output. An effect keeps its Skia objects (compiled effect, uniform bytes, texture
//! shaders, the shader of the last frame) in a `RefCell`, so paint, which sees the tree read-only,
//! reuses them: a frame whose uniforms and textures did not change allocates nothing; one whose
//! uniforms changed (an `iTime` shader) builds one shader, as upstream.
//!
//! Shader files (`shader_source`) come through the asset channel of the tree, once per url, and
//! file textures (`primary_source`, `secondary_source`) through the image manager: paint finds
//! what is missing, `after_paint` asks for it, and the control repaints when it arrived.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

use skia_safe::{
    BlendMode, Color, Data, FilterMode, IRect, Image, Matrix, MipmapMode, Paint, Point, Rect, RuntimeEffect, SamplingOptions,
    Shader, TileMode, runtime_effect::ChildPtr, shaders,
};

use crate::animators::{AnimationId, easing};
use crate::control::{Control, GestureCx, Handled, PaintCx};
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{Base, Build, ControlId, Cx, Mut, Raw, Tree};
use crate::types::{Dirty, Thickness};

/// A control's rasterized output and the canvas rect it covers (DrawnUI CachedTexture): texel
/// (0,0) is the top-left of `bounds`.
#[derive(Clone, Debug)]
pub struct CachedTexture {
    pub image: Image,
    pub bounds: Rect,
}

/// Something attached to a control's `visual_effects` (DrawnUI SkiaEffect).
pub trait SkiaEffect: Any {
    /// The GPU context was made again: textures and shaders made on the old one go.
    fn gpu_lost(&mut self) {}

    /// Pixels the effect paints outside the control's rect, per side (DrawnUI GetEffectMargin).
    /// Caches and the bounds clip grow by it.
    fn effect_margin(&self, _scale: f32) -> Thickness {
        Thickness::ZERO
    }

    /// True for a post renderer (DrawnUI IPostRendererEffect): `render` draws over the control's
    /// output.
    fn is_post_renderer(&self) -> bool {
        false
    }

    /// Draws over the control's output (DrawnUI IPostRendererEffect.Render). `cached` is the
    /// control's Image cache, which is then not blitted: the effect draws in its place. Without
    /// one the control was painted (or its picture replayed) already. `cx.rect` is the control's
    /// drawing rect. Returns true when it drew; when no post renderer drew, the cache is blitted.
    fn render(&self, _cx: &mut PaintCx<'_>, _cached: Option<&CachedTexture>) -> bool {
        false
    }

    /// Sees the control's gestures before the control and its children do (DrawnUI
    /// ISkiaGestureProcessor). `cx.point` is in the control's space, pixels, like its rect.
    fn on_gesture(&mut self, _cx: &mut GestureCx<'_>, _gesture: &Gesture) -> Handled {
        Handled::No
    }

    /// The shader effect inside, for an effect built on one (`MultiRippleWithTouchEffect`): the
    /// engine hands it the frame time and loaded textures through it.
    fn shader_mut(&mut self) -> Option<&mut SkiaShaderEffect> {
        None
    }
}

impl fmt::Debug for dyn SkiaEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SkiaEffect")
    }
}

// ---------------------------------------------------------------- attach / detach

impl<T: Control> Build<T> {
    /// Attaches a visual effect (DrawnUI VisualEffects). Post renderers draw in the order attached.
    pub fn visual_effect(mut self, effect: impl SkiaEffect) -> Self {
        self.base.visual_effects.push(Box::new(effect));
        self
    }
}

impl<T: ?Sized> Mut<'_, T> {
    /// Attaches a visual effect; the control draws again.
    pub fn add_visual_effect(&mut self, effect: impl SkiaEffect) {
        self.base_mut().visual_effects.push(Box::new(effect));
        self.mark(Dirty::DRAW);
    }

    /// Replaces every visual effect; the control draws again.
    pub fn set_visual_effects(&mut self, effects: Vec<Box<dyn SkiaEffect>>) {
        self.base_mut().visual_effects = effects;
        self.mark(Dirty::DRAW);
    }

    /// Detaches every visual effect; the control draws again.
    pub fn clear_visual_effects(&mut self) {
        if !self.base().visual_effects.is_empty() {
            self.base_mut().visual_effects.clear();
            self.mark(Dirty::DRAW);
        }
    }

    /// The first attached effect of type `E`.
    pub fn effect<E: SkiaEffect>(&self) -> Option<&E> {
        self.base().visual_effects.iter().find_map(|e| (e.as_ref() as &dyn Any).downcast_ref())
    }

    /// Write access to the first attached effect of type `E`; the control repaints (DrawnUI
    /// SkiaEffect.Update: its own cache stays, what shows it draws again). An effect whose
    /// `effect_margin` changed needs `mark(Dirty::DRAW)` as well.
    pub fn effect_mut<E: SkiaEffect>(&mut self) -> Option<&mut E> {
        self.mark(Dirty::REPAINT);
        self.base_mut().visual_effects.iter_mut().find_map(|e| (e.as_mut() as &mut dyn Any).downcast_mut())
    }

    /// `iTime` of every shader effect of the control, seconds; the control repaints.
    pub fn set_shader_time(&mut self, seconds: f32) {
        for effect in &mut self.base_mut().visual_effects {
            if let Some(shader) = effect.shader_mut() {
                shader.time_seconds = seconds;
            }
        }
        self.mark(Dirty::REPAINT);
    }
}

impl Cx<'_> {
    /// Keeps the `iTime` shaders of a control running: every frame their time is the frame time
    /// (seconds) and the control repaints, until the returned animation is stopped (React
    /// ShadersPage: a looping animator on the host). A shader nobody animates draws no frames.
    pub fn animate_shaders(&mut self, id: impl Into<ControlId>) -> AnimationId {
        let id = id.into();
        self.action_on_tick(id, move |time_ms, cx| {
            if let Some(mut control) = cx.any_mut(id) {
                control.set_shader_time((time_ms / 1000.0) as f32);
            }
        })
    }

    /// DrawnUI AnimatedShaderEffect.Play: the `progress` of the control's first shader effect goes
    /// 0 to 1 over its `duration_ms`, linear, and a Once background is captured again. Stop it
    /// with `stop_animation`; `on_finished` on the returned id is the Completed event.
    pub fn play_shader(&mut self, id: impl Into<ControlId>) -> AnimationId {
        let id = id.into();
        let mut duration = 2500.0;
        if let Some(mut control) = self.any_mut(id)
            && let Some(effect) = control.effect_mut::<SkiaShaderEffect>()
        {
            effect.progress = 0.0;
            effect.release_frozen_snapshot();
            duration = effect.duration_ms;
        }
        self.animate(id, duration, easing::linear, move |v, cx| {
            if let Some(mut control) = cx.any_mut(id)
                && let Some(effect) = control.effect_mut::<SkiaShaderEffect>()
            {
                effect.progress = v;
            }
        })
    }
}

/// The effects of a control see a gesture before the control does (`Router::route`). The effects
/// leave the node while they run, so they may reach the tree. True when one consumed it; an Up is
/// never consumed (DrawnUI).
pub(crate) fn route(tree: &mut Tree, id: ControlId, gesture: &Gesture, point: Point) -> bool {
    let Some(node) = tree.node_mut(id) else { return false };
    if node.base.visual_effects.is_empty() {
        return false;
    }
    let mut effects = std::mem::take(&mut node.base.visual_effects);
    let mut consumed = false;
    for effect in &mut effects {
        if effect.on_gesture(&mut GestureCx::new(tree, id, point), gesture) != Handled::No {
            consumed = gesture.kind != GestureKind::Up;
            break;
        }
    }
    if let Some(node) = tree.node_mut(id) {
        // Effects a handler attached meanwhile go after the ones that were there.
        effects.append(&mut node.base.visual_effects);
        node.base.visual_effects = effects;
    }
    consumed
}

/// The margin of every effect of a control, per-side maximum, pixels.
pub(crate) fn margin(base: &Base, scale: f32) -> Thickness {
    base.visual_effects.iter().fold(Thickness::ZERO, |m, e| m.max(e.effect_margin(scale)))
}

pub(crate) fn has_post_renderer(base: &Base) -> bool {
    !base.visual_effects.is_empty() && base.visual_effects.iter().any(|e| e.is_post_renderer())
}

/// Runs the post renderers of control `id` over `rect`, its drawing rect on the canvas of `cx`
/// (DrawnUI DrawDirectInternal / DrawRenderObject). True when one drew.
pub(crate) fn post_render(cx: &mut PaintCx<'_>, id: ControlId, base: &Base, rect: Rect, cached: Option<&CachedTexture>) -> bool {
    // A worker's picture takes no texture: the content shows plain.
    if cx.offthread {
        return false;
    }
    let mut own = PaintCx {
        canvas: cx.canvas,
        rect,
        scale: cx.scale,
        id,
        fonts: cx.fonts,
        nodes: cx.nodes,
        animators: cx.animators,
        render: &mut *cx.render,
        drops: &mut *cx.drops,
        gpu: &mut *cx.gpu,
        bakes: &mut *cx.bakes,
        offthread: cx.offthread,
        target: cx.target,
    };
    let mut drew = false;
    for effect in base.visual_effects.iter().filter(|e| e.is_post_renderer()) {
        drew |= effect.render(&mut own, cached);
    }
    drew
}

// ---------------------------------------------------------------- shader sources

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

enum Source {
    /// Asked for; the controls to repaint when it arrives.
    Loading(Vec<ControlId>),
    Ready(String),
    Failed(String),
}

#[derive(Default)]
struct Registry {
    /// SkSL text by name: registered by the app, or fetched by url.
    sources: HashMap<String, Source>,
    /// Compiled once per final code text (DrawnUI SkSl cache).
    compiled: HashMap<String, RuntimeEffect>,
    /// Urls paint found missing, for `after_paint` to fetch.
    fetch: Vec<String>,
    /// File textures paint is waiting for: (source, control).
    textures: Vec<(String, ControlId)>,
    /// Texture sources given to the image manager once.
    asked: Vec<String>,
    /// Controls whose shader effect has an error for its `on_compilation_error`.
    errors: Vec<ControlId>,
}

/// Registers the SkSL text a `shader_source` or `shader_template` name stands for (DrawnUI
/// SkSl.LoadFromResources): an app that embeds its shaders (`include_str!`) needs no fetch. An
/// unregistered name is fetched as a url.
pub fn register_shader_source(name: impl Into<String>, code: impl Into<String>) {
    REGISTRY.with_borrow_mut(|r| {
        r.sources.insert(name.into(), Source::Ready(code.into()));
    });
}

enum Text {
    Ready(String),
    Waiting,
    Failed(String),
}

/// The text of a source name; a missing one is asked for and `control` repaints when it arrives.
fn source_text(name: &str, control: ControlId) -> Text {
    REGISTRY.with_borrow_mut(|r| match r.sources.get_mut(name) {
        Some(Source::Ready(code)) => Text::Ready(code.clone()),
        Some(Source::Failed(error)) => Text::Failed(error.clone()),
        Some(Source::Loading(waiting)) => {
            if !waiting.contains(&control) {
                waiting.push(control);
            }
            Text::Waiting
        }
        None => {
            r.sources.insert(name.to_owned(), Source::Loading(vec![control]));
            r.fetch.push(name.to_owned());
            Text::Waiting
        }
    })
}

/// A file texture paint found missing: `after_paint` loads it.
fn want_texture(source: &str, control: ControlId) {
    REGISTRY.with_borrow_mut(|r| {
        if !r.textures.iter().any(|(s, c)| s == source && *c == control) {
            r.textures.push((source.to_owned(), control));
        }
    });
}

/// After paint: fetches the shader files paint asked for, hands file textures that arrived to the
/// effects waiting for them. Allocation-free when nothing waits.
pub(crate) fn after_paint(tree: &mut Tree) {
    let Some((fetch, textures)) = REGISTRY.with_borrow_mut(|r| {
        (!r.fetch.is_empty() || !r.textures.is_empty()).then(|| (std::mem::take(&mut r.fetch), std::mem::take(&mut r.textures)))
    }) else {
        return;
    };
    for url in fetch {
        let name = url.clone();
        tree.assets.fetch(&url, move |tree, bytes| {
            let source = match bytes.is_empty() {
                true => Source::Failed(format!("shader source {name} did not load")),
                false => Source::Ready(String::from_utf8_lossy(&bytes).into_owned()),
            };
            let waiting = REGISTRY.with_borrow_mut(|r| match r.sources.insert(name, source) {
                Some(Source::Loading(waiting)) => waiting,
                _ => Vec::new(),
            });
            for id in waiting {
                tree.invalidate(id, Dirty::REPAINT);
            }
        });
    }
    let mut still = Vec::new();
    for (source, id) in textures {
        let Some(image) = tree.images.get(&source).cloned() else {
            // Asked once: a file that failed is not asked for again on every frame.
            let first = REGISTRY.with_borrow_mut(|r| {
                let first = !r.asked.contains(&source);
                if first {
                    r.asked.push(source.clone());
                }
                first
            });
            if first {
                tree.images.preload([&source]);
            }
            still.push((source, id));
            continue;
        };
        if let Some(node) = tree.node_mut(id) {
            for effect in &mut node.base.visual_effects {
                if let Some(shader) = effect.shader_mut() {
                    shader.texture_loaded(&source, &image);
                }
            }
            tree.invalidate(id, Dirty::REPAINT);
        }
        // Loaded: a later want (the bitmap was dropped meanwhile) asks again.
        REGISTRY.with_borrow_mut(|r| r.asked.retain(|asked| *asked != source));
    }
    if !still.is_empty() {
        REGISTRY.with_borrow_mut(|r| r.textures.extend(still));
    }
}

/// Runs the `on_compilation_error` handlers of the errors paint met (React OnCompilationError),
/// once per error. True when one ran: it may have changed the app state.
pub(crate) fn dispatch_errors(tree: &mut Tree, state: &mut dyn Any) -> bool {
    let Some(ids) = REGISTRY.with_borrow_mut(|r| (!r.errors.is_empty()).then(|| std::mem::take(&mut r.errors))) else {
        return false;
    };
    let mut ran = false;
    for id in ids {
        // The control leaves the tree while its handlers run, as for a tap; its effects leave it.
        let Some(mut node) = tree.take(id) else { continue };
        let mut effects = std::mem::take(&mut node.base.visual_effects);
        let mut queue = Vec::new();
        if let Some(control) = node.kind.as_deref_mut() {
            let mut me = Raw { id, control, base: &mut node.base, queue: &mut queue }.any();
            for effect in &mut effects {
                let Some(shader) = effect.shader_mut() else { continue };
                let rt = shader.runtime.get_mut();
                let (Some(error), false) = (rt.error.clone(), rt.reported) else { continue };
                rt.reported = true;
                if let Some(handler) = shader.on_compilation_error.as_mut() {
                    handler(&mut me, &mut *state, &mut Cx { tree }, &error);
                    ran = true;
                }
            }
        }
        // Effects a handler attached meanwhile go after the ones that were there.
        effects.append(&mut node.base.visual_effects);
        node.base.visual_effects = effects;
        tree.put_back(node);
        tree.queue.append(&mut queue);
    }
    ran
}

/// A compile or load error for the effect of `control`: its handler runs after paint.
fn report_error(control: ControlId) {
    REGISTRY.with_borrow_mut(|r| {
        if !r.errors.contains(&control) {
            r.errors.push(control);
        }
    });
}

/// DrawnUI SkiaShader.NormalizeLineEndings plus the BOM a resource file may start with.
fn normalize(code: &str) -> String {
    code.trim_start_matches('\u{FEFF}').replace("\r\n", "\n").replace('\r', "\n")
}

fn compile(code: &str) -> Result<RuntimeEffect, String> {
    if let Some(effect) = REGISTRY.with_borrow(|r| r.compiled.get(code).cloned()) {
        return Ok(effect);
    }
    let effect = RuntimeEffect::make_for_shader(code, None)?;
    REGISTRY.with_borrow_mut(|r| r.compiled.insert(code.to_owned(), effect.clone()));
    Ok(effect)
}

// ---------------------------------------------------------------- SkiaShaderEffect

/// How a post renderer sources its input texture (DrawnUI PostRendererEffectUseBackgroud).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum UseBackground {
    /// The control's output every frame: its Image cache, else a snapshot of what was painted.
    #[default]
    Always,
    /// The first texture is kept until `release_frozen_snapshot` (`Cx::play_shader` releases it).
    Once,
    /// No input texture: an output-only shader.
    Never,
}

/// One uniform of the compiled shader: byte offset and float count.
struct Slot {
    name: String,
    offset: usize,
    floats: usize,
}

/// A child shader the compiled code declares.
enum Child {
    Image1,
    Image2,
    Other,
}

/// What a texture shader was made from: image, filter, mipmap, tile.
type TextureKey = (u32, FilterMode, MipmapMode, TileMode);

/// What the effect keeps between frames: everything bound to the compiled shader.
#[derive(Default)]
struct Runtime {
    compiled: Option<RuntimeEffect>,
    /// The `revision` the code was compiled (or failed) for; `None` = not yet.
    compiled_for: Option<u32>,
    error: Option<String>,
    /// `error` went to `on_compilation_error`.
    reported: bool,
    /// The code as compiled (DrawnUI LoadedCode).
    loaded_code: String,
    slots: Vec<Slot>,
    children_declared: Vec<Child>,
    /// Uniform bytes in the layout of `compiled`, written every frame.
    uniforms: Vec<u8>,
    children: Vec<ChildPtr>,
    primary: Option<(TextureKey, Shader)>,
    secondary: Option<(u32, Shader)>,
    /// A file texture drawn into the control's box (DrawnUI Resize*LoadedBitmap): (source image, w, h, image).
    resized: [Option<(u32, i32, i32, Image)>; 2],
    /// The texture of `UseBackground::Once`.
    frozen: Option<CachedTexture>,
    transparent: Option<Shader>,
    /// The shader of the last frame, the uniform bytes and textures it was made from.
    last: Option<Shader>,
    last_uniforms: Vec<u8>,
    last_textures: (Option<TextureKey>, u32),
    paint: Paint,
}

/// DrawnUI SkiaShaderEffect: an SkSL fragment shader drawn over the control's output. It gets
/// `iResolution`, `iImageResolution`, `iTime`, `iOffset`, `iMouse` (each written when the shader
/// declares it), the control's output as `iImage1`, a second texture as `iImage2`, and the custom
/// `uniform`s. One struct stands for the C# family:
/// - SkiaShaderEffect: `new()`.
/// - ShaderDoubleTexturesEffect: `control_from` / `primary_source` give `iImage1`, `control_to` /
///   `secondary_source` give `iImage2`.
/// - ShaderTransitionEffect: `transition()`: `progress` (0..1) and `ratio` uniforms, and a
///   gl-transitions `transition(vec2 uv)` wrapped by `TRANSITION_TEMPLATE`.
/// - AnimatedShaderEffect: `progress` and `iCenter` uniforms, driven by `Cx::play_shader`.
pub struct SkiaShaderEffect {
    shader_source: String,
    shader_code: String,
    shader_template: String,
    pub use_background: UseBackground,
    /// Make `iImage1` from the control's output when it has no Image cache; false for
    /// output-only shaders.
    pub auto_create_input_texture: bool,
    pub blend_mode: BlendMode,
    pub filter_mode: FilterMode,
    pub mipmap_mode: MipmapMode,
    /// Tile mode of the input texture.
    pub tile_mode: TileMode,
    /// `iTime`, seconds; `Cx::animate_shaders` writes the frame clock into it.
    pub time_seconds: f32,
    /// `iMouse.xy`, pixels relative to the control.
    pub mouse_current: Point,
    /// `iMouse.zw`: where a drag started, zero = not dragging.
    pub mouse_initial: Point,
    /// `iImage1` from the Image cache of another control (ShaderDoubleTexturesEffect ControlFrom).
    pub control_from: Option<ControlId>,
    /// `iImage2` from the Image cache of another control (ShaderDoubleTexturesEffect ControlTo).
    pub control_to: Option<ControlId>,
    /// `progress` uniform (ShaderTransitionEffect, AnimatedShaderEffect).
    pub progress: f32,
    /// `iCenter` uniform, normalized (AnimatedShaderEffect).
    pub center: Point,
    /// What `Cx::play_shader` runs for (AnimatedShaderEffect DurationMs).
    pub duration_ms: f32,
    primary_source: String,
    secondary_source: String,
    /// The bitmaps of `primary_source` and `secondary_source` once loaded.
    loaded: [Option<Image>; 2],
    uniforms: Vec<(String, Vec<f32>)>,
    /// Wraps the code in `TRANSITION_TEMPLATE` when no template is given.
    transition: bool,
    /// Grows with every code change; the runtime compiles again when it differs.
    revision: u32,
    runtime: RefCell<Runtime>,
    on_compilation_error: Option<ErrorHandler>,
}

type ErrorHandler = Box<dyn FnMut(&mut Mut<'_, dyn Control>, &mut dyn Any, &mut Cx<'_>, &str)>;

impl Default for SkiaShaderEffect {
    fn default() -> Self {
        Self {
            shader_source: String::new(),
            shader_code: String::new(),
            shader_template: String::new(),
            use_background: UseBackground::Always,
            auto_create_input_texture: true,
            blend_mode: BlendMode::SrcOver,
            filter_mode: FilterMode::Linear,
            mipmap_mode: MipmapMode::None,
            tile_mode: TileMode::Clamp,
            time_seconds: 0.0,
            mouse_current: Point::default(),
            mouse_initial: Point::default(),
            control_from: None,
            control_to: None,
            progress: 0.0,
            center: Point::new(0.5, 0.5),
            duration_ms: 2500.0,
            primary_source: String::new(),
            secondary_source: String::new(),
            loaded: [None, None],
            uniforms: Vec::new(),
            transition: false,
            revision: 0,
            runtime: RefCell::new(Runtime::default()),
            on_compilation_error: None,
        }
    }
}

/// DrawnUI ShaderTransitionEffect.DefaultTemplate: the gl-transitions adapter. Declares the
/// uniforms and `getFromColor` / `getToColor` with the GLSL bottom-left to SkSL top-left flip.
pub const TRANSITION_TEMPLATE: &str = "
uniform float ratio; // width / height
uniform float progress; // 0.0 - 1.0
uniform shader iImage1; // Texture
uniform shader iImage2; // Texture for backside
uniform float2 iOffset; // Top-left corner of DrawingRect
uniform float2 iResolution; // Viewport resolution (pixels)
uniform float2 iImageResolution; // iImage1 resolution (pixels)
uniform float  iTime; // Shader playback time (s)
uniform float4 iMouse; // Mouse drag pos=.xy Click pos=.zw (pixels)

//In GLSL, the texture coordinate origin is at the bottom-left corner,
//whereas in SKSL the origin is at the top-left corner.

vec4 getFromColor(vec2 uv) {
    vec2 adjustedUV = float2(uv.x, 1.0 - uv.y) * iImageResolution;
    return iImage1.eval(adjustedUV);
}

vec4 getToColor(vec2 uv) {
    vec2 adjustedUV = float2(uv.x, 1.0 - uv.y) * iImageResolution;
    return iImage2.eval(adjustedUV);
}

//script-goes-here
";

const TEMPLATE_PLACEHOLDER: &str = "//script-goes-here";

impl SkiaShaderEffect {
    /// SkiaShaderEffect.
    pub fn new() -> Self {
        Self::default()
    }

    /// ShaderTransitionEffect: `iImage1` = `control_from` / `primary_source`, `iImage2` =
    /// `control_to` / `secondary_source`, `progress` 0..1 and `ratio`; a gl-transitions style
    /// `transition(vec2 uv)` shader is wrapped by `TRANSITION_TEMPLATE` unless a template is set.
    pub fn transition() -> Self {
        Self { transition: true, ..Self::default() }
    }

    /// AnimatedShaderEffect: `progress` driven by `Cx::play_shader`, plus `iCenter`.
    pub fn animated() -> Self {
        Self::default()
    }

    /// Url or registered name of the SkSL file (DrawnUI ShaderSource); used before `shader_code`.
    pub fn shader_source(mut self, name: impl Into<String>) -> Self {
        self.set_shader_source(name);
        self
    }
    /// Inline SkSL (DrawnUI ShaderCode).
    pub fn shader_code(mut self, code: impl Into<String>) -> Self {
        self.set_shader_code(code);
        self
    }
    /// Url or registered name of a template whose `//script-goes-here` the shader replaces.
    pub fn shader_template(mut self, name: impl Into<String>) -> Self {
        self.set_shader_template(name);
        self
    }
    pub fn use_background(mut self, mode: UseBackground) -> Self {
        self.use_background = mode;
        self
    }
    pub fn auto_create_input_texture(mut self, on: bool) -> Self {
        self.auto_create_input_texture = on;
        self
    }
    pub fn blend_mode(mut self, mode: BlendMode) -> Self {
        self.blend_mode = mode;
        self
    }
    pub fn filter_mode(mut self, mode: FilterMode) -> Self {
        self.filter_mode = mode;
        self
    }
    pub fn mipmap_mode(mut self, mode: MipmapMode) -> Self {
        self.mipmap_mode = mode;
        self
    }
    pub fn tile_mode(mut self, mode: TileMode) -> Self {
        self.tile_mode = mode;
        self
    }
    pub fn control_from(mut self, id: impl Into<ControlId>) -> Self {
        self.control_from = Some(id.into());
        self
    }
    pub fn control_to(mut self, id: impl Into<ControlId>) -> Self {
        self.control_to = Some(id.into());
        self
    }
    /// A picture file as `iImage1`, drawn into the control's box (ShaderDoubleTexturesEffect PrimarySource).
    pub fn primary_source(mut self, source: impl Into<String>) -> Self {
        self.set_primary_source(source);
        self
    }
    /// A picture file as `iImage2`, drawn into the control's box (ShaderDoubleTexturesEffect SecondarySource).
    pub fn secondary_source(mut self, source: impl Into<String>) -> Self {
        self.set_secondary_source(source);
        self
    }
    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress;
        self
    }
    pub fn center(mut self, center: Point) -> Self {
        self.center = center;
        self
    }
    pub fn duration_ms(mut self, ms: f32) -> Self {
        self.duration_ms = ms;
        self
    }
    /// A custom uniform (DrawnUI SetUniform).
    pub fn uniform(mut self, name: impl Into<String>, values: &[f32]) -> Self {
        self.set_uniform(name, values);
        self
    }

    /// Runs once for every compile or load error of the shader, after the frame that met it
    /// (React OnCompilationError): `me` is the control the effect is attached to, the app state
    /// is `S`. Without it the error is only logged.
    pub fn on_compilation_error<S: Any>(
        mut self,
        mut f: impl FnMut(&mut Mut<'_, dyn Control>, &mut S, &mut Cx<'_>, &str) + 'static,
    ) -> Self {
        self.on_compilation_error = Some(Box::new(move |me, state, cx, error| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(me, state, cx, error)
        }));
        self
    }

    /// Another shader file, compiled at the next paint (DrawnUI ApplyShaderSource).
    pub fn set_shader_source(&mut self, name: impl Into<String>) {
        self.shader_source = name.into();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn set_shader_code(&mut self, code: impl Into<String>) {
        self.shader_code = code.into();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn set_shader_template(&mut self, name: impl Into<String>) {
        self.shader_template = name.into();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn set_primary_source(&mut self, source: impl Into<String>) {
        self.primary_source = source.into();
        self.loaded[0] = None;
    }
    pub fn set_secondary_source(&mut self, source: impl Into<String>) {
        self.secondary_source = source.into();
        self.loaded[1] = None;
    }

    /// Sets a custom uniform by name (DrawnUI SetUniform): 1 float for `float`, 2 for `float2`,
    /// n x 2 for `float2[n]`. A name the shader does not declare is skipped.
    pub fn set_uniform(&mut self, name: impl Into<String>, values: &[f32]) {
        let name = name.into();
        match self.uniforms.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => {
                v.clear();
                v.extend_from_slice(values);
            }
            None => self.uniforms.push((name, values.to_vec())),
        }
    }

    /// The compilation or load error of the current code, until the code changes (React
    /// OnCompilationError).
    pub fn error(&self) -> Option<String> {
        self.runtime.borrow().error.clone()
    }

    /// A compiled shader exists (DrawnUI IsCompiled).
    pub fn is_compiled(&self) -> bool {
        self.runtime.borrow().compiled.is_some()
    }

    /// The code as it was last compiled, before the template wrapped it (DrawnUI LoadedCode).
    pub fn loaded_code(&self) -> String {
        self.runtime.borrow().loaded_code.clone()
    }

    /// The uniforms the compiled shader declares: (name, float count).
    pub fn declared_uniforms(&self) -> Vec<(String, usize)> {
        self.runtime.borrow().slots.iter().map(|s| (s.name.clone(), s.floats)).collect()
    }

    /// Once mode: the texture was captured (DrawnUI AquiredBackground).
    pub fn aquired_background(&self) -> bool {
        self.runtime.borrow().frozen.is_some()
    }

    /// Once mode: the kept texture goes, the next paint captures again.
    pub fn release_frozen_snapshot(&mut self) {
        self.runtime.get_mut().frozen = None;
    }

    /// Textures (and the shaders made of them) of a GPU context that is gone.
    fn forget_textures(&mut self) {
        let rt = self.runtime.get_mut();
        (rt.primary, rt.secondary, rt.frozen, rt.last) = (None, None, None, None);
        rt.resized = [None, None];
        rt.last_textures = Default::default();
    }

    /// A file texture arrived from the image manager.
    fn texture_loaded(&mut self, source: &str, image: &Image) {
        if self.primary_source == source {
            self.loaded[0] = Some(image.clone());
        }
        if self.secondary_source == source {
            self.loaded[1] = Some(image.clone());
        }
    }

    /// Compiles when the code changed or a source it waited for arrived. True when a compiled
    /// shader exists (DrawnUI EnsureCompiled / NeedApply).
    fn ensure_compiled(&self, rt: &mut Runtime, control: ControlId) -> bool {
        if rt.compiled_for == Some(self.revision) {
            return rt.compiled.is_some();
        }
        let source = match self.shader_source.is_empty() {
            true => Text::Ready(self.shader_code.clone()),
            false => source_text(&self.shader_source, control),
        };
        let template = match (self.shader_template.is_empty(), self.transition) {
            (false, _) => source_text(&self.shader_template, control),
            (true, true) => Text::Ready(TRANSITION_TEMPLATE.to_owned()),
            (true, false) => Text::Ready(String::new()),
        };
        rt.compiled = None;
        rt.slots.clear();
        rt.children_declared.clear();
        (rt.primary, rt.secondary, rt.last) = (None, None, None);
        let (source, template) = match (source, template) {
            (Text::Ready(source), Text::Ready(template)) => (source, template),
            (Text::Failed(error), _) | (_, Text::Failed(error)) => {
                eprintln!("drawnui: {error}");
                (rt.compiled_for, rt.error, rt.reported) = (Some(self.revision), Some(error), false);
                if self.on_compilation_error.is_some() {
                    report_error(control);
                }
                return false;
            }
            // Not here yet: the control repaints when it arrives.
            _ => return false,
        };
        rt.compiled_for = Some(self.revision);
        rt.error = None;
        rt.loaded_code = normalize(&source);
        let code = match template.is_empty() {
            true => rt.loaded_code.clone(),
            false => normalize(&template).replace(TEMPLATE_PLACEHOLDER, &rt.loaded_code),
        };
        if code.trim().is_empty() {
            return false;
        }
        match compile(&code) {
            Ok(effect) => {
                rt.slots = effect
                    .uniforms()
                    .iter()
                    .map(|u| Slot { name: u.name().to_owned(), offset: u.offset(), floats: u.size_in_bytes() / 4 })
                    .collect();
                rt.children_declared = effect
                    .children()
                    .iter()
                    .map(|c| match c.name() {
                        "iImage1" => Child::Image1,
                        "iImage2" => Child::Image2,
                        _ => Child::Other,
                    })
                    .collect();
                rt.uniforms.clear();
                rt.uniforms.resize(effect.uniform_size(), 0);
                rt.compiled = Some(effect);
                true
            }
            Err(error) => {
                eprintln!("drawnui: shader compilation failed: {error}");
                (rt.error, rt.reported) = (Some(error), false);
                if self.on_compilation_error.is_some() {
                    report_error(control);
                }
                false
            }
        }
    }

    /// DrawnUI CreateTextureShader: the image as a shader with the effect's sampling, kept while
    /// the image and the sampling are the same. Texel (0,0) is at (0,0).
    fn texture_shader(&self, rt: &mut Runtime, image: &Image) -> Option<(TextureKey, Shader)> {
        let key = (image.unique_id(), self.filter_mode, self.mipmap_mode, self.tile_mode);
        if let Some((k, shader)) = &rt.primary
            && *k == key
        {
            return Some((key, shader.clone()));
        }
        let sampling = SamplingOptions::new(self.filter_mode, self.mipmap_mode);
        let shader = image.to_shader((self.tile_mode, self.tile_mode), sampling, None)?;
        rt.primary = Some((key, shader.clone()));
        Some((key, shader))
    }

    /// DrawnUI Resize*LoadedBitmap: file texture `index` drawn into a `destination`-sized image,
    /// kept per size. A missing one is asked for.
    fn file_texture(&self, cx: &mut PaintCx<'_>, rt: &mut Runtime, index: usize, destination: Rect) -> Option<Image> {
        let Some(source) = &self.loaded[index] else {
            want_texture([&self.primary_source, &self.secondary_source][index], cx.id);
            return None;
        };
        let (w, h) = (destination.width().round() as i32, destination.height().round() as i32);
        if w <= 0 || h <= 0 {
            return None;
        }
        if let Some((id, rw, rh, image)) = &rt.resized[index]
            && (*id, *rw, *rh) == (source.unique_id(), w, h)
        {
            return Some(image.clone());
        }
        let mut offscreen = cx.gpu.offscreen(w, h)?;
        let canvas = offscreen.canvas();
        canvas.clear(Color::TRANSPARENT);
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
        canvas.draw_image_rect_with_sampling_options(source, None, Rect::from_iwh(w, h), sampling, &Paint::default());
        let image = cx.gpu.snapshot(&mut offscreen, None)?;
        rt.resized[index] = Some((source.unique_id(), w, h, image.clone()));
        Some(image)
    }

    /// DrawnUI GetPrimaryTexture: `iImage1` and the canvas rect it covers.
    fn primary_texture(
        &self,
        cx: &mut PaintCx<'_>,
        rt: &mut Runtime,
        cached: Option<&CachedTexture>,
        destination: Rect,
    ) -> Option<CachedTexture> {
        if let Some(from) = self.control_from {
            return control_texture(cx, from);
        }
        if !self.primary_source.is_empty() {
            return self.file_texture(cx, rt, 0, destination).map(|image| CachedTexture { image, bounds: destination });
        }
        match self.use_background {
            UseBackground::Never => None,
            UseBackground::Once => {
                if rt.frozen.is_none() {
                    rt.frozen = match cached {
                        Some(texture) => Some(texture.clone()),
                        None if self.auto_create_input_texture => snapshot(cx, destination),
                        None => None,
                    };
                }
                rt.frozen.clone()
            }
            UseBackground::Always => cached.cloned(),
        }
    }

    /// DrawnUI GetSecondaryTexture: `iImage2`.
    fn secondary_texture(&self, cx: &mut PaintCx<'_>, rt: &mut Runtime, destination: Rect) -> Option<Image> {
        match self.control_to {
            Some(to) => Some(control_texture(cx, to)?.image),
            None if !self.secondary_source.is_empty() => self.file_texture(cx, rt, 1, destination),
            None => None,
        }
    }

    /// `iImage2` as a clamped, linear shader, kept while the image is the same.
    fn secondary_shader(rt: &mut Runtime, image: &Image) -> Option<(u32, Shader)> {
        if let Some((id, shader)) = &rt.secondary
            && *id == image.unique_id()
        {
            return Some((*id, shader.clone()));
        }
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
        let shader = image.to_shader((TileMode::Clamp, TileMode::Clamp), sampling, None)?;
        rt.secondary = Some((image.unique_id(), shader.clone()));
        Some((image.unique_id(), shader))
    }

    /// Draws the shader over `cx.rect` (DrawnUI Render). `extra` writes uniforms of its own after
    /// the standard and custom ones: what a subclass does in CreateUniforms upstream. True when it drew.
    pub fn render_with(
        &self,
        cx: &mut PaintCx<'_>,
        cached: Option<&CachedTexture>,
        extra: &mut dyn FnMut(&mut Uniforms<'_>),
    ) -> bool {
        let rt = &mut *self.runtime.borrow_mut();
        if !self.ensure_compiled(rt, cx.id) {
            return false;
        }
        let destination = cx.rect;
        if destination.width() <= 0.0 || destination.height() <= 0.0 {
            return false;
        }
        let mut texture = self.primary_texture(cx, rt, cached, destination);
        if self.use_background != UseBackground::Never && texture.is_none() {
            if self.auto_create_input_texture {
                texture = snapshot(cx, destination);
            }
            if texture.is_none() {
                return false;
            }
        }
        let secondary = self.secondary_texture(cx, rt, destination);
        self.draw(cx, rt, texture, secondary, extra)
    }

    /// Draws the shader over `cx.rect` with the textures given (a control that picks them itself,
    /// SkiaShaderCarousel): `primary` is `iImage1` and where it lies on the canvas, `secondary` is
    /// `iImage2`. True when it drew.
    pub fn render_textures(
        &self,
        cx: &mut PaintCx<'_>,
        primary: Option<CachedTexture>,
        secondary: Option<Image>,
        extra: &mut dyn FnMut(&mut Uniforms<'_>),
    ) -> bool {
        let rt = &mut *self.runtime.borrow_mut();
        if !self.ensure_compiled(rt, cx.id) || cx.rect.width() <= 0.0 || cx.rect.height() <= 0.0 {
            return false;
        }
        self.draw(cx, rt, primary, secondary, extra)
    }

    /// Writes the uniforms and draws the compiled shader over `cx.rect`.
    fn draw(
        &self,
        cx: &mut PaintCx<'_>,
        rt: &mut Runtime,
        texture: Option<CachedTexture>,
        secondary: Option<Image>,
        extra: &mut dyn FnMut(&mut Uniforms<'_>),
    ) -> bool {
        let destination = cx.rect;
        let primary = texture.as_ref().and_then(|t| self.texture_shader(rt, &t.image));
        let secondary = secondary.as_ref().and_then(|image| Self::secondary_shader(rt, image));
        let bounds = texture.as_ref().map_or(destination, |t| t.bounds);

        let mut u = Uniforms { slots: &rt.slots, bytes: &mut rt.uniforms };
        u.set("iResolution", &[destination.width(), destination.height()]);
        u.set("iImageResolution", &[destination.width(), destination.height()]);
        u.set("iTime", &[self.time_seconds]);
        // Where the texture starts on the canvas: shaders sample (fragCoord - iOffset).
        u.set("iOffset", &[bounds.left, bounds.top]);
        u.set("iMouse", &[self.mouse_current.x, self.mouse_current.y, self.mouse_initial.x, self.mouse_initial.y]);
        u.set("progress", &[self.progress]);
        u.set("ratio", &[destination.width() / destination.height()]);
        u.set("iCenter", &[self.center.x, self.center.y]);
        for (name, values) in &self.uniforms {
            u.set(name, values);
        }
        extra(&mut u);

        let Some(effect) = &rt.compiled else { return false };
        let textures = (primary.as_ref().map(|p| p.0), secondary.as_ref().map_or(0, |s| s.0));
        // Nothing changed since the last frame: the same shader draws again.
        let reuse = rt.last.is_some() && rt.last_textures == textures && rt.last_uniforms == rt.uniforms;
        if !reuse {
            let transparent = rt.transparent.get_or_insert_with(|| shaders::color(Color::TRANSPARENT)).clone();
            rt.children.clear();
            for child in &rt.children_declared {
                let shader = match child {
                    Child::Image1 => primary.as_ref().map(|p| p.1.clone()),
                    Child::Image2 => secondary.as_ref().map(|s| s.1.clone()),
                    Child::Other => None,
                };
                // An undeclared texture samples transparent.
                rt.children.push(ChildPtr::Shader(shader.unwrap_or_else(|| transparent.clone())));
            }
            rt.last = effect.make_shader(Data::new_copy(&rt.uniforms), &rt.children, None);
            rt.last_uniforms.clear();
            rt.last_uniforms.extend_from_slice(&rt.uniforms);
            rt.last_textures = textures;
        }
        let Some(shader) = rt.last.clone() else { return false };
        rt.paint.set_blend_mode(self.blend_mode);
        rt.paint.set_shader(shader);
        cx.canvas.draw_rect(destination, &rt.paint);
        rt.paint.set_shader(None);
        true
    }
}

impl SkiaEffect for SkiaShaderEffect {
    fn gpu_lost(&mut self) {
        self.forget_textures();
    }

    fn is_post_renderer(&self) -> bool {
        true
    }

    fn render(&self, cx: &mut PaintCx<'_>, cached: Option<&CachedTexture>) -> bool {
        self.render_with(cx, cached, &mut |_| {})
    }

    fn shader_mut(&mut self) -> Option<&mut SkiaShaderEffect> {
        Some(self)
    }
}

/// The uniform buffer of a compiled shader during a render: `set` writes a uniform by name into
/// the bytes Skia reads; a name the shader does not declare is skipped, values beyond the
/// uniform's size are cut.
pub struct Uniforms<'a> {
    slots: &'a [Slot],
    bytes: &'a mut [u8],
}

impl Uniforms<'_> {
    /// Writes a float uniform (`float`, `float2`, `float4`, arrays of them).
    pub fn set(&mut self, name: &str, values: &[f32]) {
        let Some(slot) = self.slots.iter().find(|s| s.name == name) else { return };
        for (i, v) in values.iter().take(slot.floats).enumerate() {
            let at = slot.offset + i * 4;
            self.bytes[at..at + 4].copy_from_slice(&v.to_ne_bytes());
        }
    }
}

/// The Image cache of a control and the canvas rect it covers, when it has one.
fn control_texture(cx: &PaintCx<'_>, id: ControlId) -> Option<CachedTexture> {
    cx.node(id)?;
    let (image, bounds) = cx.render[id.index as usize].cache.as_ref()?.image()?;
    Some(CachedTexture { image: image.clone(), bounds })
}

/// A copy of what the canvas of `cx` shows inside `rect` (the canvas' own coordinates), from the
/// surface it draws into: the window, or the offscreen surface of an Image cache being recorded
/// (DrawnUI CreateSnapshot with UseContext). `None` while a picture is recorded. Exact when the
/// canvas is only translated; under a scale or a rotation the texture covers the rect's bounds.
pub fn snapshot(cx: &mut PaintCx<'_>, rect: Rect) -> Option<CachedTexture> {
    let (canvas, matrix) = surface_under(cx)?;
    let (image, taken) = cx.gpu.snapshot_canvas(canvas, round_out(matrix.map_rect(rect).0))?;
    let bounds = matrix.invert()?.map_rect(Rect::from(taken)).0;
    Some(CachedTexture { image, bounds })
}

/// The device pixels `rect` (the canvas' own coordinates) covers on the surface `cx` draws onto.
pub(crate) fn device_bounds(cx: &mut PaintCx<'_>, rect: Rect) -> IRect {
    let matrix = surface_under(cx).map_or_else(|| cx.canvas.local_to_device_as_3x3(), |(_, m)| m);
    round_out(matrix.map_rect(rect).0)
}

fn round_out(r: Rect) -> IRect {
    IRect::new(r.left.floor() as i32, r.top.floor() as i32, r.right.ceil() as i32, r.bottom.ceil() as i32)
}

/// The canvas whose surface holds what is under the control `cx` paints, and the matrix from the
/// canvas of `cx` to that surface's pixels: the canvas' own surface, or while an Operations cache
/// records, the surface its picture lands on (C# and React: `Context.Surface`). That picture then
/// keeps a copy of what is under it: it is marked to record again when that changes.
fn surface_under<'a>(cx: &mut PaintCx<'a>) -> Option<(&'a skia_safe::Canvas, Matrix)> {
    let local = cx.canvas.local_to_device_as_3x3();
    // SAFETY: only asked whether the canvas draws into a surface; nothing goes through it.
    if unsafe { cx.canvas.surface() }.is_some() {
        return Some((cx.canvas, local));
    }
    let target = cx.target?;
    cx.render[target.recording.index as usize].reads_below = true;
    Some((target.canvas, Matrix::concat(&target.matrix, &local)))
}

// ---------------------------------------------------------------- touch ripples

/// One ripple of `MultiRippleWithTouchEffect`: where it started (pixels from the control's
/// top-left) and how far it is, 0..1.
#[derive(Clone, Copy, Debug)]
pub struct Ripple {
    pub origin: Point,
    pub progress: f32,
    stamp: u64,
}

/// Port of the Sandbox MultiRippleWithTouchEffect, as the React ShadersPage: a WWDC-style ripple
/// starts where the control is touched (every Down), up to ten at once, each animated 0 to 1 over
/// `duration_ms` and passed as the `origins[10]` / `progresses[10]` uniforms of `ripples.sksl`;
/// `iImage1` is the control's cache, `iImage2` the reflection texture (`secondary_source`). Also
/// the example of a custom effect built on `SkiaShaderEffect`: gestures and extra uniforms.
pub struct MultiRippleWithTouchEffect {
    pub shader: SkiaShaderEffect,
    /// Oldest first.
    pub ripples: Vec<Ripple>,
    pub duration_ms: f32,
    stamps: u64,
}

impl Default for MultiRippleWithTouchEffect {
    fn default() -> Self {
        let shader = SkiaShaderEffect::new().shader_source("shaders/ripples.sksl");
        Self { shader, ripples: Vec::new(), duration_ms: 4500.0, stamps: 0 }
    }
}

impl MultiRippleWithTouchEffect {
    /// The effect with `shader_source` "shaders/ripples.sksl".
    pub fn new() -> Self {
        Self::default()
    }

    /// Where `ripples.sksl` is.
    pub fn shader_source(mut self, name: impl Into<String>) -> Self {
        self.shader.set_shader_source(name);
        self
    }

    /// The reflection texture (`iImage2`), a picture file.
    pub fn secondary_source(mut self, source: impl Into<String>) -> Self {
        self.shader.set_secondary_source(source);
        self
    }

    /// See `SkiaShaderEffect::on_compilation_error`.
    pub fn on_compilation_error<S: Any>(
        mut self,
        f: impl FnMut(&mut Mut<'_, dyn Control>, &mut S, &mut Cx<'_>, &str) + 'static,
    ) -> Self {
        self.shader = self.shader.on_compilation_error(f);
        self
    }
}

impl SkiaEffect for MultiRippleWithTouchEffect {
    fn gpu_lost(&mut self) {
        self.shader.gpu_lost();
    }

    fn is_post_renderer(&self) -> bool {
        true
    }

    fn render(&self, cx: &mut PaintCx<'_>, cached: Option<&CachedTexture>) -> bool {
        let mut origins = [0.0f32; 20];
        let mut progresses = [-1.0f32; 10]; // -1 = inactive
        for (i, ripple) in self.ripples.iter().rev().take(10).enumerate() {
            (origins[i * 2], origins[i * 2 + 1]) = (ripple.origin.x, ripple.origin.y);
            progresses[i] = ripple.progress;
        }
        self.shader.render_with(cx, cached, &mut |u| {
            u.set("origins", &origins);
            u.set("progresses", &progresses);
        })
    }

    fn on_gesture(&mut self, cx: &mut GestureCx<'_>, gesture: &Gesture) -> Handled {
        if gesture.kind != GestureKind::Down {
            return Handled::No;
        }
        let (id, rect) = (cx.id, cx.base().rect);
        self.stamps += 1;
        let stamp = self.stamps;
        let origin = Point::new(cx.point.x - rect.left, cx.point.y - rect.top);
        self.ripples.push(Ripple { origin, progress: 0.0, stamp });
        cx.cx().animate(id, self.duration_ms, easing::linear, move |v, cx| {
            let Some(mut control) = cx.any_mut(id) else { return };
            let Some(effect) = control.effect_mut::<MultiRippleWithTouchEffect>() else { return };
            match v >= 1.0 {
                true => effect.ripples.retain(|r| r.stamp != stamp),
                false => {
                    if let Some(ripple) = effect.ripples.iter_mut().find(|r| r.stamp == stamp) {
                        ripple.progress = v;
                    }
                }
            }
        });
        Handled::No
    }

    fn shader_mut(&mut self) -> Option<&mut SkiaShaderEffect> {
        Some(&mut self.shader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App as _;
    use crate::controls::layout::SkiaLayout;
    use crate::testing::Headless;
    use crate::ui::Ui;

    const GREEN: &str = "half4 main(float2 p) { return half4(0, 1, 0, 1); }";

    /// Two red blocks whose shader, `url`, comes through the asset channel.
    fn host(url: &str) -> Headless<()> {
        let block = |left: f32| {
            let effect = SkiaShaderEffect::new().shader_source(url).use_background(UseBackground::Never);
            let block = SkiaLayout::new().width_request(20).height_request(20).background_color(Color::RED);
            block.margin(Thickness::new(left, 0.0, 0.0, 0.0)).visual_effect(effect)
        };
        let ui = Ui::new((), |_| SkiaLayout::new().fill().children((block(0.0), block(30.0))));
        let mut host = Headless::new(ui, 60, 20, 1.0);
        host.settle();
        host
    }

    #[test]
    fn a_shader_file_is_fetched_once_and_its_controls_repaint_when_it_arrives() {
        let mut host = host("green.sksl");
        assert_eq!(host.pixel(10, 10), Color::RED, "plain while it loads");
        let requests = host.ui.tree.assets.take_requests();
        assert_eq!(requests.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(), ["green.sksl"]);
        host.ui.asset(requests[0].0, GREEN.as_bytes().to_vec());
        host.settle();
        assert_eq!(host.pixel(10, 10), Color::GREEN);
        assert_eq!(host.pixel(40, 10), Color::GREEN);
        assert!(host.ui.tree.assets.take_requests().is_empty());
    }

    #[test]
    fn a_shader_file_that_did_not_load_leaves_the_controls_plain() {
        let mut host = host("missing.sksl");
        let requests = host.ui.tree.assets.take_requests();
        host.ui.asset(requests[0].0, Vec::new());
        host.settle();
        assert_eq!(host.pixel(10, 10), Color::RED);
        let root = host.ui.tree.root().unwrap();
        let first = host.ui.tree.children(root)[0];
        let error = host.ui.tree.base(first).unwrap().visual_effects[0].as_ref() as &dyn Any;
        let error = error.downcast_ref::<SkiaShaderEffect>().unwrap().error();
        assert!(error.is_some_and(|e| e.contains("missing.sksl")));
        // Not asked again.
        host.frame_after(16.0);
        assert!(host.ui.tree.assets.take_requests().is_empty());
    }

    #[test]
    fn a_registered_source_needs_no_fetch() {
        register_shader_source("inline.sksl", GREEN);
        let mut host = host("inline.sksl");
        assert_eq!(host.pixel(10, 10), Color::GREEN);
        assert!(host.ui.tree.assets.take_requests().is_empty());
    }
}
