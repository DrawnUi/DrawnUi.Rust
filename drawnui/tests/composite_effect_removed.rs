//! A composite-cached layer keeps drawing its children after a shader effect that read its cache
//! is removed (DrawnUi.Net 8b82c288: two cache records with nothing redrawn between them shared one
//! snapshot, and dropping the old record freed the image the new one showed).

use drawnui::prelude::*;
use drawnui::testing::Headless;

const PASS_THROUGH: &str = "
uniform float4 iMouse;
uniform float  iTime;
uniform float2 iResolution;
uniform float2 iImageResolution;
uniform shader iImage1;
uniform float2 iOffset;
half4 main(float2 fragCoord) {
    return iImage1.eval((fragCoord - iOffset) * iImageResolution.xy / iResolution.xy);
}";

fn color(i: usize) -> Color {
    Color::from_rgb(40 + (i as u8) * 15, 200 - (i as u8) * 10, 120)
}

#[test]
fn a_composite_layer_draws_its_children_after_its_effect_is_removed() {
    let mut layer = Handle::<SkiaLayout>::default();
    let ui = Ui::new((), |_| {
        SkiaLayout::column()
            .use_cache(CacheType::ImageComposite)
            .spacing(0)
            .width_request(100)
            .assign(&mut layer)
            .visual_effect(SkiaShaderEffect::new().shader_code(PASS_THROUGH))
            .children((0..12).map(|i| SkiaShape::new().height_request(20).fill_x().background_color(color(i))).collect::<Vec<_>>())
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 120, 260, 1.0);
    host.settle();
    let check = |host: &mut Headless<()>, when: &str| {
        for i in 0..12 {
            let at = host.pixel(50, i * 20 + 10);
            assert_eq!(at, color(i as usize), "{when}: child {i}");
        }
    };
    check(&mut host, "with the effect");
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    host.ui.tree.get_mut(layer).unwrap().clear_visual_effects();
    for frame in 0..4 {
        host.frame_after(16.0);
        check(&mut host, &format!("frame {frame} after the effect went"));
    }
    host.settle();
    check(&mut host, "settled");
}
