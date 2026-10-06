//! SkiaLabelFps (DrawnUI SkiaLabelFps, React SkiaLabelFps): a label showing the canvas FPS, above
//! its siblings and input-transparent. As React: the engine writes the value only on frames that
//! something else asked for (an animation, a scroll), so the label never keeps an idle canvas
//! drawing; idle, it keeps the last value. C# sets the text on every draw, which keeps redrawing.
//! Not ported: ForceRefresh, Format, MonoForDigits.

use skia_safe::Color;

use crate::control::{Control, Has};
use crate::controls::label::{LabelBuild, LabelProps, SkiaLabel};
use crate::tree::Build;
use crate::ui::Aria;

/// DrawnUI's default text color, LimeGreen.
const LIME_GREEN: Color = Color::from_argb(255, 0x32, 0xCD, 0x32);

pub struct SkiaLabelFps {
    label: SkiaLabel,
}

impl SkiaLabelFps {
    /// "FPS --" on black in LimeGreen, one line, above everything, taking no input.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLabelFps> {
        Build::new(SkiaLabelFps { label: SkiaLabel::default() })
            .text("FPS --")
            .max_lines(1)
            .text_color(LIME_GREEN)
            .background_color(Color::BLACK)
            .input_transparent(true)
            .z_index(i32::MAX)
            .accessibility_role(Aria::PRESENTATION)
    }

    /// The text for an FPS value (React: `FPS ${fps.toFixed(1).padStart(4, "0")}`).
    pub(crate) fn text(fps: f32) -> String {
        if fps > 0.0 { format!("FPS {fps:04.1}") } else { "FPS --".to_owned() }
    }
}

impl Has<LabelProps> for SkiaLabelFps {
    fn part(&self) -> &LabelProps {
        &self.label.p
    }
    fn part_mut(&mut self) -> &mut LabelProps {
        &mut self.label.p
    }
}

impl Control for SkiaLabelFps {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.label)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.label)
    }
}
