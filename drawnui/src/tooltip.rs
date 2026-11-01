//! Tooltips (DrawnUI `SkiaControl.Tooltip`, drawnui-cross 6ar): the text of the innermost control
//! under a resting mouse, shown in a passive popup (no input, no accessibility node, past the
//! window's edge where the host has popup windows).

use std::rc::Rc;

use skia_safe::{Color, Point};

use super::Ui;
use crate::controls::label::SkiaLabel;
use crate::controls::shape::SkiaShape;
use crate::prelude::*;
use crate::tree::{ControlId, Cx, Detached};
use crate::types::Thickness;

/// How tooltips look and when they show (`Ui::tooltip_style`). Sizes in points, times in ms.
#[derive(Clone)]
pub struct TooltipStyle {
    pub background_color: Color,
    pub text_color: Color,
    /// Empty: the default font.
    pub font_family: String,
    pub font_size: f32,
    pub corner_radius: f32,
    pub padding: Thickness,
    /// The text wraps past this width.
    pub max_width: f32,
    /// How long the mouse rests on a control before its tooltip shows.
    pub show_delay_ms: f64,
    /// Moving to the next control within this time after a tooltip closed shows its tooltip at once.
    pub follow_window_ms: f64,
    /// Builds the tooltip's content from its text instead of the default bubble.
    pub content: Option<Rc<dyn Fn(&str) -> Detached>>,
}

impl Default for TooltipStyle {
    fn default() -> Self {
        Self {
            background_color: Color::from_argb(0xF2, 0x20, 0x21, 0x24),
            text_color: Color::WHITE,
            font_family: String::new(),
            font_size: 13.0,
            corner_radius: 6.0,
            padding: Thickness::new(8.0, 5.0, 8.0, 5.0),
            max_width: 320.0,
            show_delay_ms: 600.0,
            follow_window_ms: 500.0,
            content: None,
        }
    }
}

/// Where the tooltip logic stands (`Ui::tip`).
#[derive(Default)]
pub(crate) struct Tip {
    /// The mouse over the canvas, pixels; `None` after it left or a finger touched.
    pub(crate) pointer: Option<Point>,
    /// The mouse moved since the last frame looked.
    pub(crate) moved: bool,
    /// When the mouse last moved.
    pub(crate) rest_since: f64,
    /// The control with a tooltip under the mouse.
    pub(crate) target: Option<ControlId>,
    /// The tooltip shown: its control and popup.
    pub(crate) shown: Option<(ControlId, PopupId)>,
    /// When the last shown tooltip closed (the follow window).
    pub(crate) closed_at: Option<f64>,
    /// A press, a wheel or a key closed it: no tooltip for this control until the mouse is on another one.
    pub(crate) suppressed: Option<ControlId>,
}

impl<S: 'static> Ui<S> {
    /// The tooltip of a control: its own text, else (with `tooltips_from_accessibility`) its
    /// accessibility hint, else its accessibility label when it shows no text of its own.
    fn tooltip_text(&self, id: ControlId) -> Option<&str> {
        let p = &self.tree.base(id)?.p;
        if !p.tooltip.is_empty() {
            return Some(&p.tooltip);
        }
        if !self.tooltips_from_accessibility {
            return None;
        }
        if !p.accessibility_hint.is_empty() {
            return Some(&p.accessibility_hint);
        }
        (!p.accessibility_label.is_empty() && !self.shows_text(id)).then_some(p.accessibility_label.as_str())
    }

    /// A label with text in the control or below it.
    fn shows_text(&self, id: ControlId) -> bool {
        self.tree.find::<SkiaLabel>(id).is_some_and(|l| !l.p.text.is_empty() || !l.spans.is_empty())
            || self.tree.children(id).iter().any(|c| self.shows_text(*c))
    }

    /// The innermost control with a tooltip at a canvas point.
    fn tooltip_target(&self, point: Point) -> Option<ControlId> {
        let mut at = self.hit_test(point);
        while let Some(id) = at {
            if self.tooltip_text(id).is_some() {
                return Some(id);
            }
            at = self.tree.parent(id);
        }
        None
    }

    /// Closes the tooltip shown. `suppress`: the control under the mouse shows none until the
    /// mouse is on another one (after a press, a wheel or a key).
    pub(crate) fn close_tooltip(&mut self, suppress: bool) {
        if suppress {
            self.tip.suppressed = self.tip.target;
        }
        if let Some((_, popup)) = self.tip.shown.take() {
            Cx { tree: &mut self.tree }.close_popup_window(popup);
            self.tip.closed_at = Some(self.tree.time_ms);
        }
    }

    /// When the tooltip waiting for the mouse to rest is due (`Ui::wake_at`).
    pub(crate) fn tooltip_due(&self) -> Option<f64> {
        let waiting = self.tip.shown.is_none() && self.tip.target.is_some() && self.tip.suppressed != self.tip.target;
        waiting.then(|| self.tip.rest_since + self.tooltip_style.show_delay_ms)
    }

    /// Once per frame, after the layout: follows the mouse and the content under it.
    pub(crate) fn update_tooltip(&mut self, time_ms: f64) {
        let Some(point) = self.tip.pointer else {
            self.tip.target = None;
            self.close_tooltip(false);
            return;
        };
        let moved = std::mem::take(&mut self.tip.moved);
        if moved {
            self.tip.rest_since = time_ms;
        } else if self.tip.target.is_none() {
            // A resting mouse over no tooltip costs nothing per frame.
            return;
        }
        // Content that moves under a resting mouse changes the target too: the tooltip closes.
        let target = self.tooltip_target(point);
        if target != self.tip.target {
            let was_shown = self.tip.shown.is_some();
            self.close_tooltip(false);
            self.tip.target = target;
            // Content that moved under a resting mouse waits for a new rest.
            self.tip.rest_since = time_ms;
            if self.tip.suppressed != target {
                self.tip.suppressed = None;
            }
            let following = moved && (was_shown || self.tip.closed_at.is_some_and(|at| time_ms - at <= self.tooltip_style.follow_window_ms));
            if following && target.is_some() && self.tip.suppressed.is_none() {
                self.show_tooltip(point);
            }
            return;
        }
        if self.tip.shown.is_none() && target.is_some() && self.tip.suppressed != target && time_ms - self.tip.rest_since >= self.tooltip_style.show_delay_ms {
            self.show_tooltip(point);
        }
    }

    fn show_tooltip(&mut self, point: Point) {
        let Some(target) = self.tip.target else { return };
        let Some(text) = self.tooltip_text(target).map(str::to_owned) else { return };
        let style = &self.tooltip_style;
        let content: Detached = match &style.content {
            Some(make) => make(&text),
            None => {
                let mut label = SkiaLabel::new(text).font_size(style.font_size).text_color(style.text_color).maximum_width_request(style.max_width);
                if !style.font_family.is_empty() {
                    label = label.font_family(style.font_family.clone());
                }
                SkiaShape::new().corner_radius(style.corner_radius).background_color(style.background_color).padding(style.padding).children((label,)).into()
            }
        };
        let scale = self.scale.max(0.1);
        // At the pointer: a passive popup is placed as a tooltip (`place_tooltip`).
        let anchor = Point::new(point.x / scale, point.y / scale);
        let options = PopupWindowOptions { passive: true, cpu: true, ..Default::default() };
        let popup = Cx { tree: &mut self.tree }.open_popup_window(content, anchor, options);
        self.tip.shown = Some((target, popup));
    }
}
