//! What a shader effect gets as iImage1 and what it replaces, per cache type of its control
//! (drawnui-cross 6ae, Nick 2026-10-25; DrawnUi.Net caa1c8c1 ShaderEffectContentByCacheTests). On a
//! CACHED control (Image, Operations, OperationsFull) the shader replaces the control and iImage1 is
//! the control's own content; on an UNCACHED one the control paints and the shader draws over it,
//! iImage1 being the canvas there (the control and what is behind it).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const HEADER: &str = "
uniform float4 iMouse;
uniform float  iTime;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform shader iImage1;
uniform float2 iOffset;
";

/// Returns its input: shows what iImage1 holds.
const PASS_THROUGH: &str = "
half4 main(float2 fragCoord) {
    return iImage1.eval((fragCoord - iOffset) * iImageResolution.xy / iResolution.xy);
}";

/// Returns nothing: shows what is left under the effect.
const TRANSPARENT: &str = "
half4 main(float2 fragCoord) {
    return half4(0) + iImage1.eval(fragCoord) * 0.0 + half4(iMouse.x + iTime + iResolution.x + iImageResolution.x + iOffset.x) * 0.0;
}";

fn name(c: Color) -> String {
    if c.r() > 200 && c.g() < 60 && c.b() < 60 {
        "red(content)".into()
    } else if c.g() > 120 && c.r() < 60 && c.b() < 60 {
        "green(behind)".into()
    } else {
        format!("{c:?}")
    }
}

/// A red 100 x 100 shape at (50, 50) over a green background; the center pixel.
fn render(cache: CacheType, body: &str) -> Color {
    let code = format!("{HEADER}{body}");
    let ui = Ui::new((), move |_| {
        SkiaLayer::new().fill().children((
            SkiaShape::new().fill().background_color(Color::from_rgb(0, 128, 0)),
            SkiaShape::new()
                .use_cache(cache)
                .background_color(Color::RED)
                .width_request(100)
                .height_request(100)
                .margin((50.0, 50.0, 0.0, 0.0))
                .visual_effect(SkiaShaderEffect::new().shader_code(code)),
        ))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    host.pixel(100, 100)
}

#[test]
fn input_and_replacement_per_cache_type() {
    let expected = [
        (CacheType::Image, "red(content)", "green(behind)"),
        (CacheType::Operations, "red(content)", "green(behind)"),
        (CacheType::OperationsFull, "red(content)", "green(behind)"),
        (CacheType::None, "red(content)", "red(content)"),
    ];
    let mut wrong = Vec::new();
    for (cache, pass, clear) in expected {
        let (p, c) = (name(render(cache, PASS_THROUGH)), name(render(cache, TRANSPARENT)));
        if (p.as_str(), c.as_str()) != (pass, clear) {
            wrong.push(format!("{cache:?}: passthrough={p} transparent={c}, expected {pass} / {clear}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The tint as a uniform on a picture-cached shape (SkiaShape's default cache): the picture is kept
/// and the new value is drawn with it.
#[test]
fn a_uniform_change_on_an_operations_shape_keeps_the_picture() {
    let code = format!(
        "{HEADER}uniform float uAmount;
half4 main(float2 fragCoord) {{
    half4 c = iImage1.eval((fragCoord - iOffset) * iImageResolution.xy / iResolution.xy);
    return mix(c, half4(0, 0, 1, 1), uAmount);
}}"
    );
    let mut shape: Handle<SkiaShape> = Handle::default();
    let content = SkiaShape::new()
        .background_color(Color::RED)
        .width_request(100)
        .height_request(100)
        .margin((50.0, 50.0, 0.0, 0.0))
        .visual_effect(SkiaShaderEffect::new().shader_code(code).uniform("uAmount", &[0.0]))
        .assign(&mut shape);
    let ui = Ui::new((), |_| SkiaLayer::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert_eq!(host.ui.tree.base(shape).unwrap().p.use_cache, CacheType::Operations);
    let records = host.cache_records(shape);
    assert!(records >= 1);
    assert_eq!(name(host.pixel(100, 100)), "red(content)");

    if let Some(mut control) = host.ui.tree.any_mut(shape)
        && let Some(effect) = control.effect_mut::<SkiaShaderEffect>()
    {
        effect.set_uniform("uAmount", &[1.0]);
    }
    host.settle();
    let after = host.pixel(100, 100);
    assert!(after.b() > 200 && after.r() < 50, "the new uniform was not drawn: {after:?}");
    assert_eq!(host.cache_records(shape), records, "the picture was recorded again");
}
