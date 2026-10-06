//! SkiaProgress: a horizontal track with the progress trail over it, in the Default, Cupertino,
//! Material, Material3 (gap and stop dot) and Windows looks. Painted directly, as DrawnUI.React
//! does (SkiaProgress.ts); C# composes it of child shapes with the same geometry.

use skia_safe::{Color, Paint, RRect, Rect, Size};

use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::props;
use crate::tree::Build;
use crate::types::{CacheType, LayoutOptions};
use crate::ui::Aria;

props!(ProgressProps, ProgressBuild, ProgressSet {
    /// The look.
    control_style / set_control_style: PrebuiltControlStyle = PrebuiltControlStyle::Unset, MEASURE;
    value / set_value: f32 = 0.0, DRAW;
    min / set_min: f32 = 0.0, DRAW;
    max / set_max: f32 = 100.0, DRAW;
    /// Unset = the style's.
    track_color / set_track_color: Option<Color> = None, DRAW;
    /// Unset = the style's.
    progress_color / set_progress_color: Option<Color> = None, DRAW;
});

/// What a style draws: colors, track height and corner radius in points.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProgressLook {
    pub track: Color,
    pub progress: Color,
    pub height: f32,
    pub radius: f32,
}

impl ProgressLook {
    /// The palette and track of a style (C# ResolvedTrackColor / ResolvedProgressColor).
    pub fn of(style: PrebuiltControlStyle) -> ProgressLook {
        use PrebuiltControlStyle::*;
        let look = |track: u32, progress: u32, height, radius| ProgressLook {
            track: Color::new(0xFF00_0000 | track),
            progress: Color::new(0xFF00_0000 | progress),
            height,
            radius,
        };
        match style.resolve() {
            Cupertino => look(0xE5E5EA, 0x007AFF, 4.0, 2.0),
            Material => look(0xE8EAED, 0x2196F3, 4.0, 2.0),
            Material3 => look(0xE6E0E9, 0x6750A4, 4.0, 2.0),
            Windows => look(0xF3F2F1, 0x0078D4, 6.0, 3.0),
            _ => look(0xD7DBE0, 0xDC143C, 8.0, 4.0),
        }
    }
}

#[derive(Default)]
pub struct SkiaProgress {
    pub p: ProgressProps,
}

impl SkiaProgress {
    /// Fills the width, at least 100 points, ImageDoubleBuffered (DrawnUI's default).
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaProgress> {
        Build::new(SkiaProgress::default())
            .horizontal_options(LayoutOptions::Fill)
            .minimum_width_request(100.0)
            .use_cache(CacheType::ImageDoubleBuffered)
    }

    /// Where `value` is between `min` and `max`, 0..1.
    pub fn ratio(&self) -> f32 {
        let p = &self.p;
        if p.max > p.min { ((p.value - p.min) / (p.max - p.min)).clamp(0.0, 1.0) } else { 0.0 }
    }

    fn look(&self) -> ProgressLook {
        let mut look = ProgressLook::of(self.p.control_style);
        look.track = self.p.track_color.unwrap_or(look.track);
        look.progress = self.p.progress_color.unwrap_or(look.progress);
        look
    }
}

impl Has<ProgressProps> for SkiaProgress {
    fn part(&self) -> &ProgressProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ProgressProps {
        &mut self.p
    }
}

impl Control for SkiaProgress {
    /// `value` between `min` and `max`, read as the rounded percentage (C#
    /// DefaultAccessibilityLabel, which made it the name). Asked only when the snapshot is built.
    fn accessibility_value(&self) -> Option<crate::AccessibilityValue> {
        let p = &self.p;
        Some(crate::AccessibilityValue { now: p.value as f64, min: p.min as f64, max: p.max as f64, step: 0.0, text: format!("{}%", (self.ratio() * 100.0).round()) })
    }

    /// React DefaultAccessibilityRole.
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::PROGRESS_BAR)
    }

    /// The width it is given (200 points when unbounded), the style's track height.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, _height: f32) -> Size {
        let scale = cx.scale;
        Size::new(if width.is_finite() { width } else { 200.0 * scale }, self.look().height * scale)
    }

    fn paint(&self, cx: &mut PaintCx) {
        let (look, scale, d) = (self.look(), cx.scale, cx.rect);
        let (h, r) = (look.height * scale, look.radius * scale);
        let top = d.top + (d.height() - h) / 2.0;
        let progress = d.width() * self.ratio();
        let material3 = self.p.control_style.resolve() == PrebuiltControlStyle::Material3;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        // The track; Material 3 starts it 4 points after the trail.
        paint.set_color(look.track);
        let left = if material3 && progress > 0.0 { d.right.min(d.left + progress + 4.0 * scale) } else { d.left };
        if d.right - left > 0.0 {
            cx.canvas.draw_rrect(RRect::new_rect_xy(Rect::new(left, top, d.right, top + h), r, r), &paint);
        }
        paint.set_color(look.progress);
        if progress > 0.0 {
            cx.canvas.draw_rrect(RRect::new_rect_xy(Rect::new(d.left, top, d.left + progress, top + h), r, r), &paint);
        }
        if material3 {
            let dot = 4.0 * scale;
            cx.canvas.draw_circle((d.right - dot / 2.0, top + h / 2.0), dot / 2.0, &paint);
        }
    }
}
