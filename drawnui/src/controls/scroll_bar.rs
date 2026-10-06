//! SkiaScrollBar: the indicator a SkiaScroll draws over its viewport, a rounded thumb on an
//! optional track. It reads what to show from the scroll it sits in while it is painted, so
//! scrolling writes nothing into it and never measures anything; the scroll only steps its
//! visibility (shown while scrolling, faded out after) and handles a drag on it.

use skia_safe::{Color, Paint, Rect};

use crate::control::{Control, Has, PaintCx, part};
use crate::controls::scroll::{BarView, SkiaScroll};
use crate::props;
use crate::tree::{Build, Mut};
use crate::types::{CacheType, Dirty};

/// Milliseconds a bar takes to show when it is told to stay visible.
const FADE_IN_MS: f64 = 150.0;

/// The edge a scroll bar sits at, across the scroll axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollBarDock {
    /// Right for a vertical bar, bottom for a horizontal one.
    #[default]
    End,
    /// Left for a vertical bar, top for a horizontal one.
    Start,
}

props!(ScrollBarProps, ScrollBarBuild, ScrollBarSet {
    dock / set_dock: ScrollBarDock = ScrollBarDock::End, DRAW;
    thumb_color / set_thumb_color: Color = Color::from_argb(0x66, 0x88, 0x88, 0x88), DRAW;
    /// The track behind the thumb; transparent = none.
    track_color / set_track_color: Color = Color::TRANSPARENT, DRAW;
    /// Points.
    thickness / set_thickness: f32 = 4.0, DRAW;
    /// Points from the docked edge.
    edge_margin / set_edge_margin: f32 = 2.0, DRAW;
    /// The thumb is never shorter, points; half of it while the content is pulled past an edge.
    min_thumb_size / set_min_thumb_size: f32 = 32.0, DRAW;
    /// The bar fades out after scrolling stops.
    auto_hide / set_auto_hide: bool = true, NONE;
    /// Seconds after scrolling stopped before the bar fades out.
    hide_delay_secs / set_hide_delay_secs: f32 = 1.0, NONE;
    hide_duration_secs / set_hide_duration_secs: f32 = 0.25, NONE;
    /// The thumb can be dragged, and a press on the track jumps there. The scroll then takes the
    /// whole gesture that starts on the bar. Off: the bar only indicates.
    is_draggable / set_is_draggable: bool = false, NONE;
    /// Points on both sides of the bar, across the scroll axis, that still grab it.
    grab_padding / set_grab_padding: f32 = 8.0, NONE;
});

#[derive(Default)]
pub struct SkiaScrollBar {
    pub p: ScrollBarProps,
    /// 0 hidden, 1 shown; hidden until the first scroll.
    shown: f32,
    /// Frame time since which nothing holds the bar: the fade-out starts `hide_delay_secs` later.
    idle_since: Option<f64>,
    /// Frame time and visibility a fade-in started with.
    fade_in: Option<(f64, f32)>,
    keep: bool,
}

impl SkiaScrollBar {
    /// Give it to a scroll with `scroll_bar` / `scroll_bar_horizontal`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaScrollBar> {
        // It only shows: gestures pass through to the content. A draggable bar is handled by its scroll.
        // Operations: DrawnUI's default.
        Build::new(SkiaScrollBar::default()).fill().input_transparent(true).use_cache(CacheType::Operations)
    }

    /// How visible the bar is: 0 hidden, 1 shown, between while it fades. The colors are drawn
    /// with it; `opacity` stays the app's.
    pub fn shown(&self) -> f32 {
        self.shown
    }

    /// The track inside the bar's rect, pixels.
    pub(crate) fn track(&self, rect: Rect, scale: f32, horizontal: bool) -> Rect {
        let (thickness, margin) = (self.p.thickness * scale, self.p.edge_margin * scale);
        match (horizontal, self.p.dock) {
            (false, ScrollBarDock::End) => Rect::new(rect.right - margin - thickness, rect.top, rect.right - margin, rect.bottom),
            (false, ScrollBarDock::Start) => Rect::new(rect.left + margin, rect.top, rect.left + margin + thickness, rect.bottom),
            (true, ScrollBarDock::End) => Rect::new(rect.left, rect.bottom - margin - thickness, rect.right, rect.bottom - margin),
            (true, ScrollBarDock::Start) => Rect::new(rect.left, rect.top + margin, rect.right, rect.top + margin + thickness),
        }
    }

    /// Where the thumb starts on a track `track` pixels long, and its length (DrawnUI
    /// ApplyScrollProgress). `None` when the content fits: nothing to indicate.
    pub(crate) fn thumb(&self, track: f32, scale: f32, view: &BarView) -> Option<(f32, f32)> {
        if view.ratio >= 1.0 || track <= 0.0 {
            return None;
        }
        let minimum = self.p.min_thumb_size * scale;
        let mut length = minimum.max(track * view.ratio);
        // Past an edge the thumb is squashed, not moved off the track.
        if view.overscroll != 0.0 {
            length = (minimum / 2.0).max(length - view.overscroll.abs() * scale);
        }
        Some((view.progress.clamp(0.0, 1.0) * (track - length), length))
    }
}

/// One frame of a bar's visibility, on the frame clock (DrawnUI SetScrollProgress, SetKeepVisible
/// and ScheduleHide). `changed`: the scroll moved, its content or its scrolling state changed,
/// which shows the bar at once. Answers the frame time the bar needs its next step at: now or
/// earlier while it fades, later while it waits to hide, `None` when it has nothing to do.
pub(crate) fn step(
    bar: &mut Mut<'_, SkiaScrollBar>,
    time_ms: f64,
    changed: bool,
    scrolling: bool,
    keep: bool,
    travels: bool,
) -> Option<f64> {
    let before = bar.shown;
    let (delay, duration, auto_hide) = (bar.p.hide_delay_secs as f64 * 1000.0, bar.p.hide_duration_secs as f64 * 1000.0, bar.p.auto_hide);
    let mut wake = None;
    let me = bar.control_mut();
    let kept = std::mem::replace(&mut me.keep, keep);
    if !travels {
        (me.shown, me.idle_since, me.fade_in) = (0.0, None, None);
    } else {
        // Shown at once, and the wait before it hides starts over.
        if changed {
            (me.shown, me.idle_since, me.fade_in) = (1.0, None, None);
        }
        if keep && !kept && me.shown < 1.0 {
            me.fade_in = Some((time_ms, me.shown));
        }
        if let Some((start, from)) = me.fade_in {
            me.shown = (from + (1.0 - from) * ((time_ms - start) / FADE_IN_MS) as f32).min(1.0);
            me.fade_in = (me.shown < 1.0).then_some((start, from));
        }
        if scrolling || keep || !auto_hide || me.shown == 0.0 {
            me.idle_since = None;
        } else {
            let since = *me.idle_since.get_or_insert(time_ms);
            let over = time_ms - since - delay;
            if over >= 0.0 {
                let left = if duration > 0.0 { (1.0 - over / duration).max(0.0) as f32 } else { 0.0 };
                me.shown = me.shown.min(left);
                me.idle_since = (me.shown > 0.0).then_some(since);
            }
            // Nothing to do before the delay is over.
            wake = me.idle_since.map(|since| time_ms.max(since + delay));
        }
    }
    if me.fade_in.is_some() {
        wake = Some(time_ms);
    }
    // The thumb moved or resized with its scroll, or the bar faded: its cache is recorded again.
    if bar.shown != before || changed {
        bar.mark(Dirty::DRAW);
    }
    wake
}

impl Has<ScrollBarProps> for SkiaScrollBar {
    fn part(&self) -> &ScrollBarProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ScrollBarProps {
        &mut self.p
    }
}

impl Control for SkiaScrollBar {
    /// Track and thumb for where its scroll stands now.
    fn paint(&self, cx: &mut PaintCx) {
        if self.shown <= 0.0 {
            return;
        }
        let scroll = cx.node(cx.id).and_then(|node| cx.node(node.parent?)).and_then(|parent| part::<SkiaScroll>(parent.kind.as_deref()?));
        let Some(view) = scroll.and_then(|scroll| scroll.bar_view(cx.id)) else { return };
        let track = self.track(cx.rect, cx.scale, view.horizontal);
        let along = if view.horizontal { track.width() } else { track.height() };
        let Some((start, length)) = self.thumb(along, cx.scale, &view) else { return };
        let thumb = match view.horizontal {
            true => Rect::from_xywh(track.left + start, track.top, length, track.height()),
            false => Rect::from_xywh(track.left, track.top + start, track.width(), length),
        };
        let radius = self.p.thickness * cx.scale / 2.0;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        for (rect, color) in [(track, self.p.track_color), (thumb, self.p.thumb_color)] {
            if color.a() > 0 {
                paint.set_color(color.with_a((color.a() as f32 * self.shown).round() as u8));
                cx.canvas.draw_round_rect(rect, radius, radius, &paint);
            }
        }
    }
}
