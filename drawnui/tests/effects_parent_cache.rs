//! DrawnUi.Net d0b1f2b6 ShaderKeepParentCacheTests (drawnui-cross 6ae): a shader effect paints over
//! its control's cache. In Rust a uniform change or a play_shader tick only repaints the control
//! (`Mut::effect_mut`: Dirty::REPAINT), so the cache stays and the new value is drawn with it: the
//! C# default (`ShouldInvalidateParentCache = false`, 0c6d9ef1; first `KeepParentCache = true`) is
//! Rust's only behavior. Checked for Image and Operations caches.

use drawnui::prelude::*;
use drawnui::testing::Headless;

// Every uniform the shader declares is written by the engine or by the test.
const HEADER: &str = "
uniform float4 iMouse;
uniform float  iTime;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform shader iImage1;
uniform float2 iOffset;
";
const BODY: &str = "
half4 main(float2 fragCoord) {
    float2 inputCoord = (fragCoord - iOffset) * iImageResolution.xy / iResolution.xy;
    return mix(iImage1.eval(inputCoord), half4(0, 0, 1, 1), AMOUNT);
}
";

fn scene(cache: CacheType, effect: SkiaShaderEffect) -> (Headless<()>, Handle<SkiaLayout>) {
    let mut card: Handle<SkiaLayout> = Handle::default();
    let content = SkiaLayout::new()
        .width_request(200)
        .height_request(200)
        .use_cache(cache)
        .visual_effect(effect)
        .assign(&mut card)
        .children(SkiaShape::new().fill().background_color(Color::RED));
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(content)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 200, 1.0);
    host.settle();
    assert!(host.cache_records(card) >= 1, "{cache:?}: the control has a cache");
    (host, card)
}

fn red(c: Color) -> bool {
    c.r() > 200 && c.b() < 50
}

fn blue(c: Color) -> bool {
    c.b() > 200 && c.r() < 50
}

#[test]
fn a_uniform_change_draws_the_new_value_over_the_kept_cache() {
    for cache in [CacheType::Image, CacheType::Operations] {
        let code = format!("{HEADER}uniform float uAmount;{}", BODY.replace("AMOUNT", "uAmount"));
        let (mut host, card) = scene(cache, SkiaShaderEffect::new().shader_code(code).uniform("uAmount", &[0.0]));
        let records = host.cache_records(card);
        assert!(red(host.pixel(100, 100)), "{cache:?}: red before, got {:?}", host.pixel(100, 100));

        if let Some(mut control) = host.ui.tree.any_mut(card)
            && let Some(effect) = control.effect_mut::<SkiaShaderEffect>()
        {
            effect.set_uniform("uAmount", &[1.0]);
        }
        assert!(host.ui.needs_frame(), "{cache:?}: a uniform change asks for a frame");
        host.settle();
        assert!(blue(host.pixel(100, 100)), "{cache:?}: the new uniform was not drawn: {:?}", host.pixel(100, 100));
        assert_eq!(host.cache_records(card), records, "{cache:?}: the control's cache was recorded again");
    }
}

#[test]
fn animated_ticks_play_over_the_same_cache() {
    for cache in [CacheType::Image, CacheType::Operations] {
        let code = format!("{HEADER}uniform float progress; uniform float2 iCenter;{}", BODY.replace("AMOUNT", "progress"));
        let (mut host, card) = scene(cache, SkiaShaderEffect::new().shader_code(code).duration_ms(200.0));
        let records = host.cache_records(card);
        host.ui.tree.cx().play_shader(card);
        for _ in 0..20 {
            host.frame_after(16.0);
        }
        host.settle();
        assert!(blue(host.pixel(100, 100)), "{cache:?}: the last tick was not drawn: {:?}", host.pixel(100, 100));
        assert_eq!(host.cache_records(card), records, "{cache:?}: the control's cache was recorded again");
    }
}
