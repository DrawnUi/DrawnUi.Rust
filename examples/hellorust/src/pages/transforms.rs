//! Render transforms, Opacity, hit-testing through transforms and the *ToAsync animations.
//! Ported from the React demo's TransformsPage.tsx.

use drawnui::prelude::*;

use super::{column, scrolling};
use crate::{App, hex};

const MUTED: Color = hex(0xADB5BD);
const BLUE: Color = hex(0x0D6EFD);

/// What the page keeps between taps.
#[derive(Default)]
pub struct State {
    pub(super) taps: u32,
    /// The control the animation buttons move.
    pub(super) logo: Handle<SkiaSvg>,
    /// The endless rotation, while it runs.
    pub(super) spin: Option<AnimationId>,
}

/// Builds the page.
pub fn build(app: &mut App) -> Build<SkiaLayout> {
    // The content is not cached as a whole: two of its cards change while the page is open.
    scrolling(column().children((
        SkiaLabel::new("Transforms").font_size(24).text_color(Color::WHITE).horizontal_options(LayoutOptions::Center),
        SkiaLabel::new("Applied at render around the arranged box, so layout is untouched and caches stay valid. Same names as MAUI: TranslationX/Y, Rotation, ScaleX/Y, SkewX/Y, AnchorX/Y, Opacity.")
            .font_size(13)
            .text_color(hex(0xD3D3D3))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center),
        card(
            "One property each",
            SkiaWrap::new().spacing(4).horizontal_options(LayoutOptions::Center).children((
                tile("none", |s| s),
                tile("Rotation={15}", |s| s.rotation(15)),
                tile("Scale={1.3}", |s| s.scale(1.3)),
                tile("ScaleX={-1}", |s| s.scale_x(-1)),
                tile("SkewX={20}", |s| s.skew_x(20)),
                tile("TranslationY={10}", |s| s.translation_y(10)),
                tile("Opacity={0.35}", |s| s.opacity(0.35)),
                tile("Rotation={15} AnchorX/Y={0}", |s| s.rotation(15).anchor_x(0).anchor_y(0)),
            )),
        )
        // Static: one bitmap, blitted while the page scrolls.
        .use_cache(CacheType::Image),
        card(
            "Gestures map through transforms — tap the rotated, scaled button",
            (
                SkiaStack::new().spacing(8).height_request(120).children(
                    SkiaButton::new("Tapped 0×")
                        .background_color(hex(0xD63384))
                        .center()
                        .rotation(-20)
                        .scale(1.4)
                        .observe(|me, app: &App| me.set_text(format!("Tapped {}×", app.transforms.taps)))
                        .on_tapped(|_me, app: &mut App, _cx| app.transforms.taps += 1),
                ),
                SkiaLabel::new("The hit rect is the drawn one (inverse RenderTransformMatrix), not the layout box; the ripple lands under the finger.")
                    .font_size(12)
                    .text_color(MUTED)
                    .fill_x(),
            ),
        )
        // The ripple repaints the button every frame: a cached card would be recorded again each time.
        .use_cache(CacheType::None),
        card(
            "Animations — FadeToAsync, ScaleToAsync, TranslateToAsync, RotateToAsync (Promises)",
            (
                SkiaStack::new().spacing(0).height_request(140).children(
                    SkiaSvg::new("assets/images/drawnui.svg").width_request(100).lock_ratio(1).center().assign(&mut app.transforms.logo),
                ),
                SkiaWrap::new().spacing(6).children((
                    SkiaButton::new("Fade").background_color(BLUE).on_tapped(|_me, app: &mut App, cx| {
                        let logo = app.transforms.logo;
                        let out = cx.fade_to(logo, 0.15, 300, easing::linear);
                        cx.on_finished(out, move |_app: &mut App, cx| {
                            cx.fade_to(logo, 1, 300, easing::linear);
                        });
                    }),
                    SkiaButton::new("Scale").background_color(BLUE).on_tapped(|_me, app: &mut App, cx| {
                        let logo = app.transforms.logo;
                        let up = cx.scale_to(logo, 1.6, 1.6, 250, easing::cubic_out);
                        cx.on_finished(up, move |_app: &mut App, cx| {
                            cx.scale_to(logo, 1, 1, 250, easing::cubic_in);
                        });
                    }),
                    SkiaButton::new("Translate").background_color(BLUE).on_tapped(|_me, app: &mut App, cx| {
                        let logo = app.transforms.logo;
                        let away = cx.translate_to(logo, 140, 0, 300, easing::cubic_in_out);
                        cx.on_finished(away, move |_app: &mut App, cx| {
                            cx.translate_to(logo, 0, 0, 300, easing::cubic_in_out);
                        });
                    }),
                    SkiaButton::new("Rotate").background_color(BLUE).on_tapped(|_me, app: &mut App, cx| {
                        let logo = app.transforms.logo;
                        if let Some(mut logo) = cx.get_mut(logo) {
                            logo.set_rotation(0);
                        }
                        cx.rotate_to(logo, 360, 600, easing::cubic_in_out);
                    }),
                    SkiaButton::new("Spin")
                        .observe(|me, app: &App| {
                            let spinning = app.transforms.spin.is_some();
                            me.set_text(if spinning { "Stop spin" } else { "Spin" });
                            me.set_background_color(hex(if spinning { 0xDC3545 } else { 0x20C997 }));
                        })
                        .on_tapped(|_me, app: &mut App, cx| {
                            let logo = app.transforms.logo;
                            match app.transforms.spin.take() {
                                Some(spin) => cx.stop_animation(spin),
                                None => {
                                    let turn = ValueAnimator::new(0.0, 360.0, 1200.0, easing::linear).repeat(-1);
                                    app.transforms.spin = Some(cx.start_animator(logo, turn, move |degrees, cx| {
                                        if let Some(mut logo) = cx.get_mut(logo) {
                                            logo.set_rotation(degrees);
                                        }
                                    }));
                                }
                            }
                        }),
                )),
                SkiaLabel::new("Each *ToAsync cancels its previous run of the same kind (like the C# per-property CancellationTokenSource); pass an AbortSignal to cancel from outside.")
                    .font_size(12)
                    .text_color(MUTED)
                    .fill_x(),
            ),
        )
        // Something inside moves every frame while an animation runs: painted live, not recorded again per frame.
        .use_cache(CacheType::None),
    )))
}

/// A titled card.
fn card(title: &str, content: impl IntoChildren) -> Build<SkiaShape> {
    SkiaShape::new().corner_radius(8).background_color(hex(0x2B3035)).fill_x().children(
        SkiaStack::new().spacing(10).padding((16, 12)).children((
            SkiaLabel::new(title)
                .font_size(12)
                .text_color(hex(0x6EA8FE))
                .font_attributes(FontAttributes::Bold)
                .text_transform(TextTransform::Uppercase)
                .font_family_fallback("FontSymbols,FontSymbols2"),
            content,
        )),
    )
}

/// The 96 x 56 "DrawnUI" box every transform is shown on.
fn drawnui_tile() -> Build<SkiaShape> {
    SkiaShape::new().corner_radius(10).background_color(BLUE).width_request(96).height_request(56).children(
        SkiaLabel::new("DrawnUI").font_size(14).font_family("FontTextBold").text_color(Color::WHITE).center(),
    )
}

/// The box with one transform applied and a caption under it.
fn tile(text: &str, transform: impl FnOnce(Build<SkiaShape>) -> Build<SkiaShape>) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(6).width_request(120).padding((0, 12)).children((
        transform(drawnui_tile().horizontal_options(LayoutOptions::Center)),
        SkiaLabel::new(text)
            .font_size(12)
            .text_color(MUTED)
            .horizontal_options(LayoutOptions::Center)
            .horizontal_text_alignment(TextAlignment::Center),
    ))
}
