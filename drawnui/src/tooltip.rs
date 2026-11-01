//! Tooltips (DrawnUI `SkiaControl.Tooltip`, drawnui-cross 6ar): the text of the innermost control
//! under a resting mouse, shown in a passive popup (no input, no accessibility node, past the
//! window's edge where the host has popup windows).

use std::rc::Rc;

use skia_safe::{Color, Point};

use super::Ui;
use crate::controls::label::SkiaLabel;
use crate::controls::layout::SkiaLayout;
use crate::controls::shape::SkiaShape;
use crate::prelude::*;
use crate::tree::{ControlId, Cx, Detached};
use crate::types::Thickness;

/// How tooltips look and when they show (`Ui::tooltip_style`). Sizes in points, times in ms.
#[derive(Clone)]
pub struct TooltipStyle {
    pub background_color: Color,
    /// A 1 point border; transparent: none.
    pub border_color: Color,
    /// A soft shadow under the bubble (blur 6, 2 points down) in a margin of 8 points around it;
    /// transparent: none.
    pub shadow_color: Color,
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
    /// False: centered 6 points above its control (below it where there is no room). True: at the
    /// mouse, 4 points right of and 20 points below where it rested (above it where there is no
    /// room), as the Windows and macOS tooltips do. Either way it stays put while the mouse moves
    /// over the control.
    pub at_cursor: bool,
    /// Moves it right (negative: left) from where it would go, points.
    pub offset_x: f32,
    /// Moves it down (negative: up) from where it would go, points; on the flipped side it moves
    /// the other way, so it keeps its distance from the control or the pointer.
    pub offset_y: f32,
    /// Builds the tooltip's content from its text instead of the default bubble.
    pub content: Option<Rc<dyn Fn(&str) -> Detached>>,
}

impl Default for TooltipStyle {
    fn default() -> Self {
        Self {
            // DrawnUI's default look (Nick, 2026-11-01): reads on light and dark apps.
            background_color: Color::from_rgb(0x2B, 0x2D, 0x31),
            border_color: Color::from_argb(0x24, 0xFF, 0xFF, 0xFF),
            shadow_color: Color::from_argb(0x59, 0, 0, 0),
            text_color: Color::from_rgb(0xF2, 0xF3, 0xF5),
            font_family: String::new(),
            font_size: 13.0,
            corner_radius: 6.0,
            padding: Thickness::new(12.0, 8.0, 12.0, 8.0),
            max_width: 320.0,
            show_delay_ms: 600.0,
            follow_window_ms: 500.0,
            at_cursor: false,
            offset_x: 0.0,
            offset_y: 0.0,
            content: None,
        }
    }
}

impl Cx<'_> {
    /// Changes how tooltips look (a theme switched at run time): from the next tooltip shown; one
    /// already open keeps its look until it closes.
    pub fn set_tooltip_style(&mut self, style: TooltipStyle) {
        self.tree.tooltip_style = style;
    }

    /// Turns the accessibility-text fallback on or off at run time (`Ui::tooltips_from_accessibility`).
    pub fn set_tooltips_from_accessibility(&mut self, on: bool) {
        self.tree.tooltips_from_accessibility = on;
    }

    /// How tooltips look now, to change one field and set it again.
    pub fn tooltip_style(&self) -> &TooltipStyle {
        &self.tree.tooltip_style
    }
}

/// Points of room around the default bubble for its shadow.
const SHADOW_ROOM: f32 = 8.0;

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
        if !self.tree.tooltips_from_accessibility {
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

    /// The innermost control with a tooltip at a canvas point. A layout that draws nothing of its
    /// own there (an empty overlay layer of a shell) is looked through, unless it has a tooltip.
    fn tooltip_target(&self, point: Point) -> Option<ControlId> {
        let tree = &self.tree;
        let seen = |id: ControlId| {
            tree.find::<SkiaLayout>(id).is_none()
                || tree.base(id).is_some_and(|b| b.p.background_color.is_some_and(|c| c.a() > 0) || b.p.fill_gradient.is_some())
                || self.tooltip_text(id).is_some()
        };
        let identity = skia_safe::Matrix::default();
        // Popups drawn on the canvas first, as `Ui::hit_test`.
        let on_canvas = tree.popups.iter().rev().filter(|p| !p.options.passive && !p.in_window(self.popup_windows_on));
        let mut at = on_canvas
            .filter_map(|p| tree.visual_pick_where(p.root, point, &identity, &seen))
            .next()
            .or_else(|| tree.visual_pick_where(tree.root?, point, &identity, &seen));
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
        waiting.then(|| self.tip.rest_since + self.tree.tooltip_style.show_delay_ms)
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
            let following = moved && (was_shown || self.tip.closed_at.is_some_and(|at| time_ms - at <= self.tree.tooltip_style.follow_window_ms));
            if following && target.is_some() && self.tip.suppressed.is_none() {
                self.show_tooltip();
            }
            return;
        }
        if self.tip.shown.is_none() && target.is_some() && self.tip.suppressed != target && time_ms - self.tip.rest_since >= self.tree.tooltip_style.show_delay_ms {
            self.show_tooltip();
        }
    }

    fn show_tooltip(&mut self) {
        let Some(target) = self.tip.target else { return };
        let Some(text) = self.tooltip_text(target).map(str::to_owned) else { return };
        let style = &self.tree.tooltip_style;
        let content: Detached = match &style.content {
            Some(make) => make(&text),
            None => {
                let mut label = SkiaLabel::new(text).font_size(style.font_size).text_color(style.text_color).maximum_width_request(style.max_width);
                if !style.font_family.is_empty() {
                    label = label.font_family(style.font_family.clone());
                }
                let mut bubble = SkiaShape::new().corner_radius(style.corner_radius).background_color(style.background_color).padding(style.padding);
                if style.border_color.a() > 0 {
                    bubble = bubble.stroke_color(style.border_color).stroke_width(1.0);
                }
                if style.shadow_color.a() > 0 {
                    let shadow = crate::types::SkiaShadow { x: 0.0, y: 2.0, blur: 6.0, opacity: 1.0, color: style.shadow_color, shadow_only: false };
                    bubble = bubble.margin(SHADOW_ROOM).shadows(vec![shadow]);
                }
                bubble.children((label,)).into()
            }
        };
        // The default bubble's shadow room is around its visible box: the anchor carries it, so the
        // visible box, not the room, keeps the gap to the control (`place_tooltip`).
        let inset = if style.content.is_none() && style.shadow_color.a() > 0 { SHADOW_ROOM } else { 0.0 };
        let scale = self.scale.max(0.1);
        // At the control where it is drawn now, or at the pointer, fixed while it shows: the pointer
        // moving over the control does not move it.
        let anchor = if style.at_cursor {
            let Some(at) = self.tip.pointer else { return };
            let at = Point::new(at.x / scale, at.y / scale);
            skia_safe::Rect::new(at.x - inset, at.y - inset, at.x + inset, at.y + inset)
        } else {
            let Some(drawn) = super::drawn_rect(&self.tree, target) else { return };
            skia_safe::Rect::new(drawn.left / scale, drawn.top / scale + inset, drawn.right / scale, drawn.bottom / scale - inset)
        };
        let tooltip = crate::TooltipPlacement { at_cursor: style.at_cursor, offset_x: style.offset_x, offset_y: style.offset_y };
        let options = PopupWindowOptions { passive: true, cpu: true, tooltip, ..Default::default() };
        let popup = Cx { tree: &mut self.tree }.open_popup_window(content, anchor, options);
        self.tip.shown = Some((target, popup));
    }
}
