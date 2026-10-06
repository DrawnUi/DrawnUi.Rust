//! Frame cost of shader effects and backdrops on the desktop GPU window: N controls of 300 x 300
//! points stacked on each other, all drawn again every frame. The host prints the CPU time of a
//! frame and the frame interval every 300 frames; the FPS counter is on screen.
//!
//! `cargo run --release -p drawnui --example shader_cost -- <count> <mode>`, modes:
//! - `plain`: the controls without an effect (the baseline),
//! - `plasma`: an output-only `iTime` shader (UseBackground Never),
//! - `wave`: an `iTime` shader sampling the control's Image cache,
//! - `snapshot`: the same shader over a control without a cache (a copy of the window per frame),
//! - `backdrop`: a SkiaBackdrop, blur 5, over the picture (a copy of the window and a blur per frame).
//!
//! - `carousel`: one SkiaShaderCarousel of three pictures and the cube transition, going on every
//!   second (the count is ignored).
//!
//! Every mode but `plasma` shows a picture (run from the repository root).

use drawnui::prelude::*;

const PLASMA: &str = "
uniform float2 iResolution;
uniform float2 iOffset;
uniform float  iTime;
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
uniform float  strength;
half4 main(float2 fragCoord) {
    float2 uv = (fragCoord - iOffset) / iResolution.xy;
    float2 d = float2(sin(uv.y * 20.0 + iTime * 3.0), cos(uv.x * 20.0 + iTime * 2.0)) * strength;
    return iImage1.eval((uv + d) * iImageResolution);
}";

fn gradient() -> SkiaGradient {
    SkiaGradient {
        gradient_type: GradientType::Linear,
        colors: vec![Color::from_rgb(233, 69, 96), Color::from_rgb(15, 52, 96), Color::from_rgb(83, 52, 131)],
        end_x_ratio: 1.0,
        end_y_ratio: 1.0,
        ..SkiaGradient::default()
    }
}

/// Run from the repository root: the picture is read relative to the working directory.
const PICTURE: &str = "examples/hellorust/assets/images/baboon.jpg";

fn tile(mode: &str) -> Build<SkiaLayout> {
    let picture = SkiaImage::new(PICTURE).aspect(TransformAspect::AspectCover).fill();
    let tile = SkiaLayout::new().width_request(300).height_request(300).margin(20).fill_gradient(gradient());
    let tile = if mode == "plasma" { tile } else { tile.children(picture) };
    match mode {
        "plasma" => tile.visual_effect(
            SkiaShaderEffect::new().shader_code(PLASMA).use_background(UseBackground::Never).auto_create_input_texture(false),
        ),
        "wave" => tile.use_cache(CacheType::Image).visual_effect(SkiaShaderEffect::new().shader_code(WAVE).uniform("strength", &[0.01])),
        "snapshot" => tile.visual_effect(SkiaShaderEffect::new().shader_code(WAVE).uniform("strength", &[0.01])),
        "backdrop" => tile.children(SkiaBackdrop::new().blur(5)),
        _ => tile,
    }
}

fn main() {
    drawnui::run("Shader cost", || {
        let mut args = std::env::args().skip(1);
        let count: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
        let mode = args.next().unwrap_or_else(|| "plasma".to_owned());
        println!("{count} x 300 x 300 pt, {mode}");
        if mode == "carousel" {
            return carousel();
        }
        let mut tiles = Vec::new();
        let mut ui = Ui::new((), |_| {
            let children: Vec<_> = (0..count).map(|_| tile(&mode)).collect();
            tiles = children.iter().map(|t| t.id()).collect();
            SkiaLayout::new().fill().children(children)
        });
        let root = ui.tree.root().expect("mounted");
        let mut cx = ui.tree.cx();
        match mode.as_str() {
            "plasma" | "wave" | "snapshot" => {
                for id in tiles {
                    cx.animate_shaders(id);
                }
            }
            // Frames keep coming; nothing is cached, so everything is drawn again.
            _ => {
                cx.action_on_tick(root, |_, _| {});
            }
        }
        let font = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf");
        Box::new(ui.font("Default", font).background(Color::from_rgb(18, 18, 24)).show_fps(true))
    });
}

fn carousel() -> Box<dyn drawnui::App> {
    register_shader_source("cube.sksl", include_str!("../tests/shaders/transitions/cube.sksl"));
    let pictures = ["examples/hellorust/assets/images/baboon.jpg", "examples/hellorust/assets/images/glass2.jpg"];
    let slides: Vec<_> = (0..3)
        .map(|i| SkiaLayout::new().use_cache(CacheType::Image).children(SkiaImage::new(pictures[i % 2]).aspect(TransformAspect::AspectCover).fill()))
        .collect();
    let mut handle = Handle::default();
    let ui = Ui::new((), |_| {
        let carousel = SkiaShaderCarousel::new()
            .horizontal_options(LayoutOptions::Start)
            .width_request(300)
            .height_request(300)
            .margin(20)
            .is_looped(true)
            .linear_speed_ms(750.0)
            .transition_shader("cube.sksl")
            .children(slides)
            .assign(&mut handle);
        SkiaLayout::new().fill().children(carousel)
    });
    let mut ui = ui;
    let mut next_at = 1000.0;
    ui.tree.cx().action_on_tick(handle, move |time, cx| {
        if time >= next_at {
            next_at = time + 1000.0;
            if let Some(mut c) = cx.get_mut(handle) {
                c.go_next();
            }
        }
    });
    let font = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf");
    Box::new(ui.font("Default", font).background(Color::from_rgb(18, 18, 24)).show_fps(true))
}
