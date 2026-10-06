//! Port of DrawnUi.Net.Tests LabelDescenderClipTests. Glyph ink can reach below FontMetrics.Descent (Inter:
//! FontMetrics.Bottom is ~1.5 px past Descent at 18 px), so it lands below the label's drawing rect. Upstream
//! lost those pixels of the last line's g/p/y in the default Operations cache and reports the overshoot as
//! an effects margin. The test asserts that margin, as upstream does, and the result: the cached label
//! shows the same pixels as the uncached one, the overshoot band included.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));
/// As upstream: the font of the app the bug was seen in. Without it the test passes and proves nothing.
const INTER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Inter-Regular.ttf");
const TEXT: &str = "We have added Looks! Save your favorite combinations of background, face, filter and adjustments, and switch between them with one tap, right from the main bar.";
const BACKGROUND: Color = Color::from_argb(255, 0x1a, 0x1b, 0x1e);

fn host(scale: f32, inter: &[u8], cache: Option<CacheType>) -> Headless<Handle<SkiaLabel>> {
    let build = |label: &mut Handle<SkiaLabel>| {
        let mut text = SkiaLabel::new(TEXT)
            .font_family("FontText")
            .font_size(14)
            .text_color(Color::WHITE)
            .fill_x()
            .margin((16, 0, 0, 0))
            .line_break_mode(drawnui::controls::label::LineBreakMode::WordWrap)
            .assign(label);
        if let Some(cache) = cache {
            text = text.use_cache(cache);
        }
        SkiaLayout::column().spacing(12).padding((24, 22)).fill_x().children(
            SkiaLayer::new().children((SkiaLabel::new("\u{2022}").font_family("FontText").font_size(15), text)),
        )
    };
    let ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).font_bytes("FontText", inter).background(BACKGROUND);
    let mut host = Headless::new(ui, (480.0 * scale) as i32, (320.0 * scale) as i32, scale);
    host.settle();
    host
}

#[test]
fn last_line_descenders_survive_operations_cache() {
    let Ok(inter) = std::fs::read(INTER) else {
        eprintln!("Inter is not on this machine: nothing proved");
        return;
    };
    for scale in [1.25f32, 1.5] {
        let mut cached = host(scale, &inter, None);
        let mut live = host(scale, &inter, Some(CacheType::None));
        let label = cached.ui.state;
        assert_eq!(cached.ui.tree.base(label).unwrap().p.use_cache, CacheType::Operations);
        assert!(cached.ui.tree.find::<SkiaLabel>(label).unwrap().lines_count() > 1, "text did not wrap");

        // What upstream reports as the bottom effects margin: FontMetrics.Bottom past Descent, whole pixels.
        let mut fonts = drawnui::Fonts::default();
        fonts.add("FontText", &inter);
        let (_, metrics) = fonts.font("FontText", (14.0 * scale).round()).unwrap().metrics();
        let overshoot = (metrics.bottom - metrics.descent).ceil() as i32;
        assert!(overshoot >= 1, "Inter should overshoot its descent at this size: {} / {}", metrics.descent, metrics.bottom);
        let margin = Control::effects_margin(cached.ui.tree.find::<SkiaLabel>(label).unwrap(), scale);
        assert_eq!(margin.bottom, overshoot as f32, "the label's effects margin: {margin:?}");

        let rect = cached.rect(label);
        assert_eq!(rect, live.rect(live.ui.state));
        let (left, right) = (rect.left.floor() as i32, rect.right.ceil() as i32);
        let (top, bottom) = (rect.top.floor() as i32 - overshoot, rect.bottom.ceil() as i32 + overshoot);
        let mut descender_ink = 0;
        for y in top..bottom {
            for x in left..right {
                let pixel = cached.pixel(x, y);
                assert_eq!(pixel, live.pixel(x, y), "the cache differs at {x}, {y} (rect {rect:?})");
                // The rows a g, p or y reaches: from the last baseline down.
                if y as f32 >= rect.bottom - metrics.descent && pixel != BACKGROUND {
                    descender_ink += 1;
                }
            }
        }
        assert!(descender_ink > 0, "the last line has no descenders to lose");
    }
}
