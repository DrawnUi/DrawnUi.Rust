//! SkiaDecoratedGrid: a Grid that paints a line in the spacing between its columns and between
//! its rows, under the children (DrawnUI SkiaDecoratedGrid, as DrawnUi.React draws it).

use std::cell::RefCell;

use skia_safe::{Color, Paint, Rect};

use super::{LayoutProps, LayoutType, SkiaLayout};
use crate::control::{Control, Has, PaintCx};
use crate::layout::content_rect;
use crate::paint::{GradientShader, gradient_shader};
use crate::props;
use crate::tree::{Build, Container};
use crate::types::{GradientType, LayoutOptions, SkiaGradient};

/// The default line: a soft light band that fades out at both ends, along `x` or along `y`.
fn line(along_x: bool) -> SkiaGradient {
    let band = Color::from_argb(0x78, 0xE8, 0xE3, 0xD7);
    let (end_x, end_y) = if along_x { (1.0, 0.0) } else { (0.0, 1.0) };
    SkiaGradient::default()
        .gradient_type(GradientType::Linear)
        .colors(vec![band.with_a(0), band, band, band.with_a(0)])
        .color_positions(vec![0.0, 0.1, 0.9, 1.0])
        .end_x_ratio(end_x)
        .end_y_ratio(end_y)
}

props!(DecoratedGridProps, DecoratedGridBuild, DecoratedGridSet {
    /// Painted between rows over the whole `row_spacing`, over black; `None` paints nothing there.
    horizontal_line / set_horizontal_line: Option<Box<SkiaGradient>> = Some(Box::new(SkiaDecoratedGrid::horizontal_gradient())), DRAW;
    /// Painted between columns over the whole `column_spacing`; `None` paints nothing there.
    vertical_line / set_vertical_line: Option<Box<SkiaGradient>> = Some(Box::new(SkiaDecoratedGrid::vertical_gradient())), DRAW;
});

/// A Grid filling the width, with lines in its spacing.
#[derive(Default)]
pub struct SkiaDecoratedGrid {
    layout: SkiaLayout,
    pub p: DecoratedGridProps,
    /// The shader of every line, per column gap and per row gap, kept between frames and built
    /// again only when a line's rect or gradient changed.
    shaders: RefCell<(Vec<Option<GradientShader>>, Vec<Option<GradientShader>>)>,
}

impl SkiaDecoratedGrid {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaDecoratedGrid> {
        let mut grid = SkiaDecoratedGrid::default();
        grid.layout.p.layout_type = LayoutType::Grid;
        Build::new(grid).horizontal_options(LayoutOptions::Fill)
    }

    /// The default `horizontal_line` (C# and React `HorizontalGradient`).
    pub fn horizontal_gradient() -> SkiaGradient {
        line(true)
    }

    /// The default `vertical_line` (C# and React `VerticalGradient`).
    pub fn vertical_gradient() -> SkiaGradient {
        line(false)
    }
}

impl Has<DecoratedGridProps> for SkiaDecoratedGrid {
    fn part(&self) -> &DecoratedGridProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut DecoratedGridProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaDecoratedGrid {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for SkiaDecoratedGrid {}

impl Control for SkiaDecoratedGrid {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The lines over the background, then the children over them.
    fn paint(&self, cx: &mut PaintCx) {
        let (grid, lp, scale) = (self.layout.grid_tracks(), &self.layout.p, cx.scale);
        let inner = content_rect(cx.base(), scale);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let shaders = &mut *self.shaders.borrow_mut();
        let mut fill = |cx: &mut PaintCx, cache: &mut Option<GradientShader>, rect: Rect, gradient: &SkiaGradient| {
            if let Some(shader) = gradient_shader(cache, gradient, rect, (0.0, 0.0)) {
                paint.set_shader(shader);
                paint.set_blend_mode(gradient.blend_mode);
                cx.canvas.draw_rect(rect, &paint);
            }
        };
        if let Some(gradient) = self.p.vertical_line.as_deref()
            && lp.column_spacing > 0.0
        {
            let gaps = grid.columns().saturating_sub(1);
            shaders.0.resize_with(gaps, || None);
            for (index, cache) in shaders.0.iter_mut().enumerate() {
                let (start, end) = grid.column_gap(index + 1, lp.column_spacing, inner.left, scale);
                fill(cx, cache, Rect::new(start, inner.top, end, inner.bottom), gradient);
            }
        }
        if let Some(gradient) = self.p.horizontal_line.as_deref()
            && lp.row_spacing > 0.0
        {
            let gaps = grid.rows().saturating_sub(1);
            shaders.1.resize_with(gaps, || None);
            // Over black, as the C# control puts a black shape under its gradient.
            let mut black = Paint::default();
            black.set_anti_alias(true);
            black.set_color(Color::BLACK);
            for (index, cache) in shaders.1.iter_mut().enumerate() {
                let (start, end) = grid.row_gap(index + 1, lp.row_spacing, inner.top, scale);
                let rect = Rect::new(inner.left, start, inner.right, end);
                cx.canvas.draw_rect(rect, &black);
                fill(cx, cache, rect, gradient);
            }
        }
        self.layout.paint(cx);
    }
}
