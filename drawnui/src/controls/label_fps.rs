//! SkiaLabelFps (DrawnUI SkiaLabelFps, React SkiaLabelFps): a label showing the canvas FPS, above
//! its siblings and input-transparent. The engine writes the value only on frames that something
//! else asked for (an animation, a scroll), so the label never keeps the canvas drawing; once the
//! screen is still it shows 00.0 (one frame wakes for that). `force_refresh` keeps the canvas
//! drawing every frame, as C# ForceRefresh does. Not ported: Format, MonoForDigits.

use std::any::Any;

use skia_safe::Color;

use crate::animators::FrameTick;
use crate::control::{Control, Has};
use crate::controls::label::{LabelBuild, LabelProps, SkiaLabel};
use crate::tree::{Build, ControlId, Cx, Mut};
use crate::types::Dirty;
use crate::ui::Aria;

/// DrawnUI's default text color, LimeGreen.
const LIME_GREEN: Color = Color::from_argb(255, 0x32, 0xCD, 0x32);

pub struct SkiaLabelFps {
    label: SkiaLabel,
    id: ControlId,
    /// Keeps the canvas drawing every frame (C# ForceRefresh); off, the label asks for no frames.
    force_refresh: bool,
    ticking: bool,
}

impl SkiaLabelFps {
    /// "FPS 00.0" on black in LimeGreen, one line, above everything, taking no input.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaLabelFps> {
        let mut build = Build::new(SkiaLabelFps { label: SkiaLabel::default(), id: ControlId { index: 0, generation: 0 }, force_refresh: false, ticking: false });
        let id = build.id();
        build.control_mut().id = id;
        build
            .text(Self::text(0.0))
            .max_lines(1)
            .text_color(LIME_GREEN)
            .background_color(Color::BLACK)
            .input_transparent(true)
            .z_index(i32::MAX)
            .accessibility_role(Aria::PRESENTATION)
    }

    /// The text for an FPS value (C# `FPS {0:00.0}`, React `FPS ${fps.toFixed(1).padStart(4, "0")}`).
    pub(crate) fn text(fps: f32) -> String {
        format!("FPS {:04.1}", fps.max(0.0))
    }

    /// Whether the label keeps the canvas drawing every frame.
    pub fn force_refresh(&self) -> bool {
        self.force_refresh
    }
}

impl Build<SkiaLabelFps> {
    /// Keeps the canvas drawing every frame, so the label measures the rate the screen can reach
    /// (C# ForceRefresh). Off (the default), a still screen draws nothing and shows 00.0.
    pub fn force_refresh(mut self, on: bool) -> Self {
        self.control_mut().force_refresh = on;
        self
    }
}

impl Mut<'_, SkiaLabelFps> {
    /// `force_refresh` at run time.
    pub fn set_force_refresh(&mut self, on: bool) {
        if self.control_mut().force_refresh != on {
            self.control_mut().force_refresh = on;
            self.mark(Dirty::APPLY);
        }
    }
}

/// One frame of `force_refresh`: it asks for the next one while the setting is on.
fn tick(id: ControlId, _time_ms: f64, _state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let keep = cx.tree.find::<SkiaLabelFps>(id).is_some_and(|l| l.force_refresh);
    if !keep && let Some(mut label) = cx.tree.find_mut::<SkiaLabelFps>(id) {
        label.control_mut().ticking = false;
    }
    FrameTick { keep, state_touched: false }
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

    fn on_props_changed(&mut self, cx: &mut Cx) {
        if self.force_refresh && !std::mem::replace(&mut self.ticking, true) {
            cx.start_frame_loop(self.id, tick);
        }
        self.label.on_props_changed(cx);
    }
}
