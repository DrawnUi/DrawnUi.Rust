//! SkiaShaderEffect: SkSL on a control's output, generative shaders, touch ripples, and the
//! SkiaShaderCarousel gl-transitions. Ported from the React demo's ShadersPage.tsx.

use drawnui::prelude::*;

use super::{card, card_title, page_title, scrolling};
use crate::{App, hex};

const PLASMA: &str = "
uniform float2 iResolution;
uniform float2 iOffset;
uniform float  iTime;
uniform float4 iMouse;

half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution.xy;
    float t = iTime * 0.6;
    float v = sin(uv.x * 6.0 + t) + sin((uv.y * 6.0 + t) * 0.8) + sin((uv.x + uv.y) * 4.0 - t) + sin(length(uv - 0.5) * 12.0 - t * 1.5);
    v *= 0.25;
    float3 col = 0.5 + 0.5 * cos(6.2831 * (v + float3(0.0, 0.33, 0.67)) + t);
    return half4(col, 1.0);
}";

const WAVE: &str = "
uniform shader iImage1;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform float2 iOffset;
uniform float  iTime;
uniform float4 iMouse;
uniform float  strength;

half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution.xy;
    float2 d = float2(sin(uv.y * 20.0 + iTime * 3.0), cos(uv.x * 20.0 + iTime * 2.0)) * strength;
    float2 p = (uv + d) * iImageResolution;
    return iImage1.eval(p);
}";

const STRENGTHS: [f32; 4] = [0.0, 0.005, 0.01, 0.03];

const TRANSITIONS: [&str; 14] = [
    "cube", "fade", "swirl", "doorway", "bounce", "waterdrop", "pixelize", "windowslice", "crosszoom", "pagecurl", "morph", "heart",
    "kaleidoscope", "wind",
];

const PHOTOS: [&str; 4] = ["assets/images/hugrobot2.jpg", "assets/images/8.jpg", "assets/images/dungeon.jpg", "assets/images/nebula.jpg"];

/// What the page keeps.
pub struct State {
    carousel: Handle<SkiaShaderCarousel>,
    pub(super) transition: &'static str,
    /// "· from → to" of the last transition.
    pub(super) from_to: String,
    wave: Handle<SkiaImage>,
    plasma: Handle<SkiaLayout>,
    blit_host: Handle<SkiaImage>,
    pub(super) strength: f32,
    /// The animations that keep `iTime` running, while they run.
    pub(super) running: Vec<AnimationId>,
    pub(super) blit: bool,
    /// The last shader that did not compile or load, for the error line.
    pub(super) error: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            carousel: Handle::default(),
            transition: "cube",
            from_to: String::new(),
            wave: Handle::default(),
            plasma: Handle::default(),
            blit_host: Handle::default(),
            strength: 0.01,
            running: Vec::new(),
            blit: false,
            error: String::new(),
        }
    }
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    // The shaders animate: nothing above them is cached; each image caches its own picture,
    // which the effect samples.
    scrolling(SkiaStack::new().spacing(16).padding(16).horizontal_options(LayoutOptions::Center).maximum_width_request(720).children((
        page_title("Shaders"),
        SkiaLabel::new("")
            .font_size(12)
            .text_color(hex(0xFF6B6B))
            .fill_x()
            .is_visible(false)
            .observe(|me, app: &App| {
                let error = &app.shaders.error;
                me.set_is_visible(!error.is_empty());
                if !error.is_empty() {
                    me.set_text(format!("Shader error: {error}"));
                }
            }),
        card(
            card_title("").font_family_fallback("FontSymbols,FontSymbols2").observe(|me, app: &App| {
                let page = &app.shaders;
                me.set_text(format!(
                    "SkiaShaderCarousel — TransitionShader=\"shaders/transitions/{}.sksl\" · IsLooped · LinearSpeedMs=750 {}",
                    page.transition, page.from_to
                ))
            }),
            (
                SkiaShaderCarousel::new()
                    .height_request(280)
                    .is_looped(true)
                    .linear_speed_ms(750)
                    .transition_shader("assets/shaders/transitions/cube.sksl")
                    .assign(&mut app.shaders.carousel)
                    .items(|_app: &App| PHOTOS.len(), slide, bind_slide)
                    .on_transition_changed(|me, app: &mut App, _cx, _running| {
                        let carousel = me.control_mut();
                        if let (Some(from), Some(to)) = (carousel.transition_from_index(), carousel.transition_to_index()) {
                            app.shaders.from_to = format!("· {from} → {to}");
                        }
                    }),
                SkiaWrap::new().spacing(6).children((
                    carousel_button("‹ Prev", |carousel| carousel.go_prev()),
                    carousel_button("Next ›", |carousel| carousel.go_next()),
                    TRANSITIONS.map(transition_button).into_iter().collect::<Vec<_>>(),
                )),
                note("Slides never move: a ShaderTransitionEffect blends the Image caches of the outgoing and incoming slides (iImage1 / iImage2, progress, ratio) through a gl-transitions style transition(uv) wrapped by the adapter template. Swipe, or drag slowly to scrub the transition."),
            ),
        ),
        card(
            card_title("SkiaShaderEffect on a SkiaImage — ShaderSource=\"shaders/ripples.sksl\" (Sandbox MultiRippleWithTouchEffect) · tap to ripple"),
            (
                SkiaImage::new("assets/images/hugrobot2.jpg")
                    .aspect(TransformAspect::AspectCover)
                    .fill_x()
                    .height_request(260)
                    .use_cache(CacheType::Image)
                    .visual_effect(
                        MultiRippleWithTouchEffect::new()
                            .shader_source("assets/shaders/ripples.sksl")
                            .secondary_source("assets/images/nebula.jpg")
                            .on_compilation_error(report_error),
                    ),
                note("The effect takes the gestures of its control: every Down starts a ripple at the touch point, animated 0→1 over 4.5 s and passed as the origins[10] / progresses[10] array uniforms; iImage1 is the image's own cache, iImage2 (SecondarySource) the reflection texture."),
            ),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                let state = if app.shaders.running.is_empty() { "paused" } else { "animating" };
                me.set_text(format!("Inline ShaderCode + SetUniform(\"strength\", {}) + iTime · {state}", app.shaders.strength))
            }),
            (
                SkiaImage::new("assets/images/8.jpg")
                    .aspect(TransformAspect::AspectCover)
                    .fill_x()
                    .height_request(200)
                    .use_cache(CacheType::Image)
                    .visual_effect(SkiaShaderEffect::new().shader_code(WAVE).uniform("strength", &[0.01]).on_compilation_error(report_error))
                    .assign(&mut app.shaders.wave),
                SkiaWrap::new().spacing(6).children((
                    STRENGTHS.map(strength_button).into_iter().collect::<Vec<_>>(),
                    SkiaButton::new("")
                        .background_color(hex(0x0D6EFD))
                        .font_size(12)
                        .observe(|me, app: &App| me.set_text(if app.shaders.running.is_empty() { "Run" } else { "Pause" }))
                        .on_tapped(|_me, app: &mut App, cx| {
                            if app.shaders.running.is_empty() {
                                run(app, cx);
                            } else {
                                for animation in app.shaders.running.drain(..) {
                                    cx.stop_animation(animation);
                                }
                            }
                        }),
                )),
            ),
        ),
        card(
            card_title("Generative shader — UseBackground=\"Never\" on a SkiaLayer (no input texture)"),
            SkiaLayer::new()
                .fill_x()
                .height_request(140)
                .visual_effect(
                    SkiaShaderEffect::new()
                        .shader_code(PLASMA)
                        .use_background(UseBackground::Never)
                        .auto_create_input_texture(false)
                        .on_compilation_error(report_error),
                )
                .assign(&mut app.shaders.plasma),
        ),
        card(
            card_title("").observe(|me, app: &App| {
                me.set_text(format!(
                    "ShaderSource=\"shaders/blit.sksl\" (pass-through) toggled through VisualEffects · {}",
                    if app.shaders.blit { "on" } else { "off" }
                ))
            }),
            (
                SkiaImage::new("assets/images/dungeon.jpg")
                    .aspect(TransformAspect::AspectCover)
                    .fill_x()
                    .height_request(160)
                    .use_cache(CacheType::Image)
                    .assign(&mut app.shaders.blit_host),
                SkiaButton::new("")
                    .background_color(hex(0x0D6EFD))
                    .font_size(12)
                    .observe(|me, app: &App| me.set_text(if app.shaders.blit { "Remove effect" } else { "Add effect" }))
                    .on_tapped(|_me, app: &mut App, cx| {
                        app.shaders.blit = !app.shaders.blit;
                        let Some(mut image) = cx.get_mut(app.shaders.blit_host) else { return };
                        if app.shaders.blit {
                            image.add_visual_effect(SkiaShaderEffect::new().shader_source("assets/shaders/blit.sksl").on_compilation_error(report_error));
                        } else {
                            image.clear_visual_effects();
                        }
                    }),
            ),
        ),
    )))
}

fn carousel_button(caption: &str, action: fn(&mut Mut<'_, SkiaShaderCarousel>)) -> Build<SkiaButton> {
    SkiaButton::new(caption)
        .font_family_fallback("FontSymbols,FontSymbols2")
        .background_color(hex(0x0F3460))
        .font_size(13)
        .on_tapped(move |_me, app: &mut App, cx| {
            if let Some(mut carousel) = cx.get_mut(app.shaders.carousel) {
                action(&mut carousel);
            }
        })
}

fn transition_button(name: &'static str) -> Build<SkiaButton> {
    SkiaButton::new(name)
        .font_size(12)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.shaders.transition == name { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, cx| {
            app.shaders.transition = name;
            if let Some(mut carousel) = cx.get_mut(app.shaders.carousel) {
                carousel.set_transition_shader(format!("assets/shaders/transitions/{name}.sksl"));
            }
        })
}

/// The controls of one slide that a bind writes.
#[derive(Default)]
struct SlideHandles {
    image: Handle<SkiaImage>,
    label: Handle<SkiaLabel>,
}

/// A slide of the shader carousel: it MUST be cached as an Image, the transition samples the cache.
fn slide() -> (Build<SkiaLayout>, SlideHandles) {
    let mut handles = SlideHandles::default();
    let slide = SkiaLayer::new().fill().use_cache(CacheType::Image).children((
        SkiaImage::new("").aspect(TransformAspect::AspectCover).fill().assign(&mut handles.image),
        SkiaLabel::new("")
            .font_size(28)
            .text_color(Color::WHITE)
            .center()
            .drop_shadow_color(Color::BLACK)
            .drop_shadow_size(4)
            .assign(&mut handles.label),
    ));
    (slide, handles)
}

fn bind_slide(slide: &SlideHandles, _app: &App, index: usize, cx: &mut Cx) {
    if let Some(mut image) = cx.get_mut(slide.image) {
        image.set_source(PHOTOS[index % PHOTOS.len()]);
    }
    super::set_text(cx, slide.label, format!("Slide {}", index + 1));
}

/// A shader did not compile or its file did not load (React OnCompilationError).
fn report_error(_me: &mut Mut<'_, dyn Control>, app: &mut App, _cx: &mut Cx, error: &str) {
    app.shaders.error = error.to_owned();
}

fn note(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(12).text_color(hex(0xADB5BD)).fill_x().font_family_fallback("FontSymbols,FontSymbols2")
}

fn strength_button(strength: f32) -> Build<SkiaButton> {
    SkiaButton::new(format!("strength {strength}"))
        .font_size(12)
        .observe(move |me, app: &App| me.set_background_color(hex(if app.shaders.strength == strength { 0x533483 } else { 0x495057 })))
        .on_tapped(move |_me, app: &mut App, cx| {
            app.shaders.strength = strength;
            if let Some(mut wave) = cx.get_mut(app.shaders.wave)
                && let Some(effect) = wave.effect_mut::<SkiaShaderEffect>()
            {
                effect.set_uniform("strength", &[strength]);
            }
        })
}

/// The `iTime` shaders repaint only while something asks for frames: an animation on each host
/// runs their time (C# and React need the same).
fn run(app: &mut App, cx: &mut Cx) {
    app.shaders.running = vec![cx.animate_shaders(app.shaders.wave), cx.animate_shaders(app.shaders.plasma)];
}

/// The page opened: the shaders start running, as the React page's animators on mount.
pub fn opened(app: &mut App, cx: &mut Cx) {
    run(app, cx);
}
