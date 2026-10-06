//! SkiaShape: every Type, fill + stroke, gradients, bevel, shadows, corner radii, and children
//! clipped to the shape. Ported from the React demo's ShapesPage.tsx.

use drawnui::prelude::*;

use super::scrolling;
use crate::{App, hex};

const HEART: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";

/// A five-point star, as ratios of the shape's box.
const STAR: [(f32, f32); 10] = [
    (0.5, 0.0),
    (0.62, 0.38),
    (1.0, 0.38),
    (0.69, 0.61),
    (0.81, 1.0),
    (0.5, 0.76),
    (0.19, 1.0),
    (0.31, 0.61),
    (0.0, 0.38),
    (0.38, 0.38),
];


/// Builds the page.
pub fn build(_app: &mut App) -> Build<SkiaLayout> {
    scrolling(
        // Nothing here changes: the scroll content is one bitmap, blitted at the scroll offset.
        // It also keeps the blurs of the shadows, which would run again on every paint.
        SkiaStack::new().spacing(20).padding(16).use_cache(CacheType::Image).children((
            title("SkiaShape", 24),
            caption("Stroke is drawn inside the bounds; children are clipped to the shape."),
            // A tuple of children holds twelve at most: the ContextMenu section is one.
            (
                title("ContextMenu", 20),
                SkiaLabel::new("Right-click / long-press the shape: its ContextMenu handler takes the request and the browser menu stays away. Elsewhere on the canvas the browser menu shows as usual.")
                    .font_size(13)
                    .text_color(hex(0xD3D3D3))
                    .horizontal_options(LayoutOptions::Center)
                    .maximum_width_request(680)
                    .horizontal_text_alignment(TextAlignment::Center),
                row(demo(
                    "ContextMenu -> toast",
                    SkiaShape::new()
                        .corner_radius(12)
                        .width_request(120)
                        .height_request(70)
                        .center()
                        .background_color(hex(0x0D6EFD))
                        .on_context_menu(|_me, app: &mut App, cx, menu| {
                            let text = format!("ContextMenu at {}, {} px ({:?})", menu.local.x.round(), menu.local.y.round(), menu.source);
                            cx.show_toast(app.shell, &text, 4000);
                            true
                        })
                        .children(SkiaLabel::new("right-click me").font_size(13).text_color(Color::WHITE).center()),
                )),
            ),
            title("FillGradient / StrokeGradient", 20),
            row((
                demo(
                    "Linear · Angle={45}",
                    SkiaShape::new()
                        .corner_radius(12)
                        .width_request(110)
                        .height_request(70)
                        .fill_gradient(gradient(GradientType::Linear, &[0x0D6EFD, 0xD63384]).angle(45.0))
                        .center(),
                ),
                demo(
                    "Circular at (0.5, 0.5)",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .width_request(80)
                        .lock_ratio(1)
                        .fill_gradient(
                            gradient(GradientType::Circular, &[0xFFFFFF, 0x0D6EFD, 0x0A2A6B])
                                .start_x_ratio(0.5)
                                .start_y_ratio(0.5)
                                .color_positions(vec![0.0, 0.6, 1.0]),
                        )
                        .center(),
                ),
                demo(
                    "Oval · Light={1.4}",
                    SkiaShape::new()
                        .shape_type(ShapeType::Ellipse)
                        .width_request(120)
                        .height_request(60)
                        .fill_gradient(
                            gradient(GradientType::Oval, &[0x20C997, 0x0F3460])
                                .start_x_ratio(0.5)
                                .start_y_ratio(0.5)
                                .light(1.4),
                        )
                        .center(),
                ),
                demo(
                    "Sweep · TileMode Repeat",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .width_request(80)
                        .lock_ratio(1)
                        // A sweep turns about its center, so it is given one.
                        .fill_gradient(
                            gradient(GradientType::Sweep, &[0xE94560, 0xFFC107, 0x20C997, 0x0D6EFD, 0xE94560])
                                .start_x_ratio(0.5)
                                .start_y_ratio(0.5),
                        )
                        .center(),
                ),
                demo(
                    "StrokeGradient · StrokeWidth 8",
                    SkiaShape::new()
                        .corner_radius(16)
                        .width_request(110)
                        .height_request(70)
                        .stroke_width(8)
                        .clip_background_color(true)
                        .stroke_gradient(
                            gradient(GradientType::Linear, &[0xFFC107, 0xD63384]).end_x_ratio(1).end_y_ratio(0),
                        )
                        .center(),
                ),
                demo(
                    "SkiaLabel FillGradient → glyphs (GradientByLines)",
                    SkiaLabel::new("Gradient text, line by line")
                        .font_size(18)
                        .font_family("FontTextBold")
                        .text_color(Color::WHITE)
                        .center()
                        .horizontal_text_alignment(TextAlignment::Center)
                        .width_request(130)
                        .fill_gradient(gradient(GradientType::Linear, &[0xFFC107, 0xD63384]).angle(90.0)),
                ),
                demo(
                    "Label: BackgroundColor + FillGradient = both",
                    SkiaLabel::new("bg + text")
                        .font_size(16)
                        .text_color(Color::WHITE)
                        .background_color(hex(0x212529))
                        .padding((12, 8))
                        .center()
                        .gradient_by_lines(false)
                        .fill_gradient(gradient(GradientType::Linear, &[0x6610F2, 0x0DCAF0]).angle(0.0)),
                ),
            )),
            title("Bevel / Emboss", 20),
            row((
                demo(
                    "BevelType=Bevel · Depth 4",
                    SkiaShape::new()
                        .corner_radius(12)
                        .background_color(hex(0x495057))
                        .width_request(110)
                        .height_request(70)
                        .center()
                        .bevel_type(BevelType::Bevel)
                        .bevel(SkiaBevel::new(4)),
                ),
                demo(
                    "BevelType=Emboss · Depth 4",
                    SkiaShape::new()
                        .corner_radius(12)
                        .background_color(hex(0x495057))
                        .width_request(110)
                        .height_request(70)
                        .center()
                        .bevel_type(BevelType::Emboss)
                        .bevel(SkiaBevel::new(4)),
                ),
                demo(
                    "Circle · Bevel, colored edges",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .background_color(hex(0x0D6EFD))
                        .width_request(80)
                        .lock_ratio(1)
                        .center()
                        .bevel_type(BevelType::Bevel)
                        .bevel(SkiaBevel::new(6).light_color(hex(0x9EC5FE)).shadow_color(hex(0x052C65)).opacity(0.8)),
                ),
                demo(
                    "Polygon (star) · Emboss",
                    SkiaShape::new()
                        .shape_type(ShapeType::Polygon)
                        .points(STAR.to_vec())
                        .background_color(hex(0xFFC107))
                        .width_request(90)
                        .height_request(90)
                        .center()
                        .bevel_type(BevelType::Emboss)
                        .bevel(SkiaBevel::new(3).opacity(0.7)),
                ),
                demo(
                    "Path (heart) · Bevel",
                    SkiaShape::new()
                        .shape_type(ShapeType::Path)
                        .path_data(HEART)
                        .background_color(hex(0xD63384))
                        .width_request(90)
                        .height_request(90)
                        .center()
                        .bevel_type(BevelType::Bevel)
                        .bevel(SkiaBevel::new(3).opacity(0.7)),
                ),
                demo(
                    "Sharp rectangle · Bevel, Opacity 1",
                    SkiaShape::new()
                        .background_color(hex(0x6C757D))
                        .width_request(110)
                        .height_request(70)
                        .center()
                        .bevel_type(BevelType::Bevel)
                        .bevel(SkiaBevel::new(5).opacity(1)),
                ),
            )),
            title("Shadows", 20),
            row((
                demo(
                    "Shadows=[{Y:4, Blur:6, Opacity:.5}]",
                    SkiaShape::new()
                        .corner_radius(12)
                        .background_color(Color::WHITE)
                        .width_request(100)
                        .height_request(60)
                        .shadows(SkiaShadow::new(Color::BLACK).x(0).y(4).blur(6).opacity(0.5))
                        .center(),
                ),
                demo(
                    "Colored, offset X",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .background_color(hex(0xFFC107))
                        .width_request(64)
                        .lock_ratio(1)
                        .shadows(SkiaShadow::new(hex(0x6610F2)).x(6).y(6).blur(4).opacity(0.8))
                        .center(),
                ),
                demo(
                    "Two shadows (glow + drop)",
                    SkiaShape::new()
                        .corner_radius(30)
                        .background_color(hex(0x20C997))
                        .width_request(110)
                        .height_request(60)
                        .shadows(vec![
                            SkiaShadow::new(hex(0x20C997)).x(0).y(0).blur(10).opacity(0.9),
                            SkiaShadow::new(Color::BLACK).x(0).y(6).blur(4).opacity(0.6),
                        ])
                        .center(),
                ),
                demo(
                    "ShadowOnly + hollow ClipBackgroundColor",
                    SkiaShape::new()
                        .corner_radius(12)
                        .clip_background_color(true)
                        .stroke_color(Color::WHITE)
                        .stroke_width(2)
                        .width_request(100)
                        .height_request(60)
                        .shadows(SkiaShadow::new(Color::BLACK).x(0).y(5).blur(5).opacity(0.7))
                        .center(),
                ),
            )),
            title("Types", 20),
            row((
                demo(
                    "Rectangle CornerRadius={16}",
                    SkiaShape::new()
                        .corner_radius(16)
                        .background_color(hex(0x0D6EFD))
                        .stroke_color(Color::WHITE)
                        .stroke_width(3)
                        .width_request(110)
                        .height_request(70)
                        .center(),
                ),
                demo(
                    "CornerRadius(24, 0, 0, 24)",
                    SkiaShape::new()
                        .corner_radius((24, 0, 0, 24))
                        .background_color(hex(0x20C997))
                        .width_request(110)
                        .height_request(70)
                        .center(),
                ),
                demo(
                    "Circle + stroke",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .background_color(hex(0x6610F2))
                        .stroke_color(hex(0xFFD93D))
                        .stroke_width(4)
                        .width_request(80)
                        .lock_ratio(1)
                        .center(),
                ),
                demo(
                    "Ellipse",
                    SkiaShape::new()
                        .shape_type(ShapeType::Ellipse)
                        .background_color(hex(0xDC3545))
                        .width_request(120)
                        .height_request(70)
                        .center(),
                ),
                demo(
                    "Arc Value1=-90 Value2=270",
                    SkiaShape::new()
                        .shape_type(ShapeType::Arc)
                        .value1(-90)
                        .value2(270)
                        .stroke_color(hex(0x0DCAF0))
                        .stroke_width(8)
                        .width_request(80)
                        .lock_ratio(1)
                        .center(),
                ),
                demo(
                    "Polygon (star, Points)",
                    SkiaShape::new()
                        .shape_type(ShapeType::Polygon)
                        .points(STAR.to_vec())
                        .background_color(hex(0xFFC107))
                        .width_request(90)
                        .lock_ratio(1)
                        .center(),
                ),
                demo(
                    "Line (Points)",
                    SkiaShape::new()
                        .shape_type(ShapeType::Line)
                        .points(vec![(0.0, 1.0), (0.33, 0.2), (0.66, 0.8), (1.0, 0.0)])
                        .stroke_color(hex(0xFD7E14))
                        .stroke_width(4)
                        .width_request(120)
                        .height_request(70)
                        .center(),
                ),
                demo(
                    "Path (SVG PathData)",
                    SkiaShape::new()
                        .shape_type(ShapeType::Path)
                        .path_data(HEART)
                        .background_color(hex(0xE83E8C))
                        .width_request(80)
                        .lock_ratio(1)
                        .center(),
                ),
                demo(
                    "Hollow: ClipBackgroundColor",
                    SkiaShape::new()
                        .corner_radius(12)
                        .clip_background_color(true)
                        .background_color(hex(0x0D6EFD))
                        .stroke_color(hex(0x0D6EFD))
                        .stroke_width(3)
                        .width_request(110)
                        .height_request(70)
                        .center(),
                ),
                demo(
                    "Children clipped",
                    SkiaShape::new()
                        .shape_type(ShapeType::Circle)
                        .background_color(hex(0x1F2937))
                        .stroke_color(hex(0x67E8F9))
                        .stroke_width(3)
                        .width_request(80)
                        .lock_ratio(1)
                        .center()
                        .children((
                            SkiaShape::new()
                                .background_color(hex(0x67E8F9))
                                .width_request(100)
                                .height_request(24)
                                .horizontal_options(LayoutOptions::Center)
                                .vertical_options(LayoutOptions::End),
                            SkiaLabel::new("AB")
                                .font_size(22)
                                .font_family("FontTextBold")
                                .text_color(Color::WHITE)
                                .center()
                                .margin((0, 0, 0, 10)),
                        )),
                ),
                demo(
                    "StrokeCap Butt, thin",
                    SkiaShape::new()
                        .shape_type(ShapeType::Line)
                        .stroke_cap(PaintCap::Butt)
                        .points(vec![(0.0, 0.5), (1.0, 0.5)])
                        .stroke_color(Color::WHITE)
                        .stroke_width(1)
                        .width_request(120)
                        .height_request(40)
                        .center(),
                ),
                demo(
                    "Gradient fill",
                    SkiaShape::new()
                        .corner_radius(35)
                        .width_request(120)
                        .height_request(70)
                        .fill_gradient(
                            gradient(GradientType::Linear, &[0xFF6B6B, 0xFFD93D, 0x4ECDC4]).end_x_ratio(1).end_y_ratio(0),
                        )
                        .center(),
                ),
            )),
        )),
    )
}

fn title(text: &str, size: i32) -> Build<SkiaLabel> {
    SkiaLabel::new(text)
        .font_size(size)
        .text_color(Color::WHITE)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 8, 0, 0))
}

fn caption(text: &str) -> Build<SkiaLabel> {
    SkiaLabel::new(text).font_size(13).text_color(hex(0xD3D3D3)).horizontal_options(LayoutOptions::Center)
}

/// The swatches of one section: a centered wrap capped at 680 points, which holds four per line.
fn row(children: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaWrap::new().spacing(16).horizontal_options(LayoutOptions::Center).maximum_width_request(680).children(children)
}

/// One labeled swatch: the demo shape centered on a card.
fn demo(title: &str, content: impl IntoChildren) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(8).width_request(150).children((
        SkiaShape::new()
            .width_request(150)
            .height_request(110)
            .background_color(hex(0x2B3035))
            .corner_radius(8)
            .children(content),
        // The fallback draws the arrow of "FillGradient → glyphs" (the React caption shows tofu).
        SkiaLabel::new(title)
            .font_size(13)
            .text_color(hex(0xADB5BD))
            .horizontal_options(LayoutOptions::Center)
            .font_family_fallback("FontSymbols,FontSymbols2"),
    ))
}

/// A gradient of the given colors; Linear runs top to bottom until `angle` or the ratios say otherwise.
fn gradient(kind: GradientType, colors: &[u32]) -> SkiaGradient {
    SkiaGradient::new(kind, colors.iter().map(|color| hex(*color)).collect::<Vec<_>>())
}
