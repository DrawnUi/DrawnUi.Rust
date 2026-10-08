//! SkiaSlider: one thumb (`end`) or two (`enable_range`: `start` and `end`) on a track between
//! `min` and `max`, moved in `step`s by a drag or a press on the trail, in the Default, Cupertino,
//! Material, Material3 and Windows looks. Painted directly, as DrawnUI.React does (SkiaSlider.ts);
//! the C# thumb-position math is kept: the thumb box is `slider_height` wide and travels over the
//! width minus that box.

use std::any::Any;

use skia_safe::{Color, ImageFilter, Paint, PaintStyle, RRect, Rect, Size};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx, part_mut};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::gestures::{Gesture, GestureKind};
use crate::keyboard::{InputKey, KeyEvent, KeyKind};
use crate::tree::{Build, ControlId, Cx, Mut, Raw, Tree, wrong_state};
use crate::types::{CacheType, LayoutOptions, SkiaShadow, Thickness};
use crate::ui::Aria;
use crate::{paint, props};

props!(SliderProps, SliderBuild, SliderSet {
    /// The look.
    control_style / set_control_style: PrebuiltControlStyle = PrebuiltControlStyle::Unset, MEASURE_APPLY;
    min / set_min: f32 = 0.0, DRAW_APPLY;
    max / set_max: f32 = 100.0, DRAW_APPLY;
    /// Values snap to `min` plus whole steps; 0 = no snapping.
    step / set_step: f32 = 1.0, NONE;
    /// Two thumbs: `start` and `end`.
    enable_range / set_enable_range: bool = false, MEASURE_APPLY;
    /// Smallest distance between the two thumbs, in values.
    range_min / set_range_min: f32 = 0.0, NONE;
    /// The value of the start thumb (range mode).
    start / set_start: f32 = 0.0, DRAW_APPLY;
    /// The value of the thumb (the end thumb in range mode).
    end / set_end: f32 = 0.0, DRAW_APPLY;
    /// A press on the trail moves the nearest thumb there.
    click_on_trail_enabled / set_click_on_trail_enabled: bool = true, NONE;
    /// Off, the slider takes no gesture.
    responds_to_gestures / set_responds_to_gestures: bool = true, NONE;
    /// Points around a thumb that still grab it (C# moreHotspotSize).
    more_hotspot_size / set_more_hotspot_size: f32 = 10.0, NONE;
    /// Points: the thumb box and the control's height. -1 = the style's (35 default, 28
    /// Cupertino, 20 Material and Windows; 8 more in range mode except the default look).
    slider_height / set_slider_height: f32 = -1.0, MEASURE_APPLY;
    /// Unset = the style's.
    thumb_color / set_thumb_color: Option<Color> = None, DRAW;
    /// Unset = the style's.
    track_color / set_track_color: Option<Color> = None, DRAW;
    /// The trail between the thumbs (from the start in single mode). Unset = the style's.
    track_selected_color / set_track_selected_color: Option<Color> = None, DRAW;
});

/// What a style draws (the C# style builders): thumb diameter and track height in points, the
/// palette, the thumb's shadow.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SliderLook {
    pub thumb: f32,
    pub track_height: f32,
    pub track: Color,
    pub selected: Color,
    pub thumb_color: Color,
    pub shadow: SkiaShadow,
}

impl SliderLook {
    pub fn of(style: PrebuiltControlStyle) -> SliderLook {
        use PrebuiltControlStyle::*;
        let rgb = |v: u32| Color::new(0xFF00_0000 | v);
        let shadow = |y, blur, opacity, color| SkiaShadow::new(color).x(0.0).y(y).blur(blur).opacity(opacity);
        let look = |thumb, track_height, track, selected, thumb_color, shadow| SliderLook {
            thumb,
            track_height,
            track: rgb(track),
            selected: rgb(selected),
            thumb_color,
            shadow,
        };
        match style.resolve() {
            Cupertino => look(28.0, 2.0, 0xCCCCCC, 0x007AFF, Color::WHITE, shadow(1.0, 3.0, 0.2, rgb(0x808080))),
            Material => look(20.0, 4.0, 0xE8EAED, 0x2196F3, rgb(0x2196F3), shadow(1.0, 2.0, 0.3, Color::BLACK)),
            Material3 => look(20.0, 4.0, 0xE6E0E9, 0x6750A4, rgb(0x6750A4), shadow(1.0, 2.0, 0.3, Color::BLACK)),
            Windows => look(20.0, 4.0, 0xC6C6C6, 0x0078D4, rgb(0x0078D4), shadow(1.0, 2.0, 0.25, Color::BLACK)),
            _ => look(35.0, 6.0, 0xD7DBE0, 0xDC143C, rgb(0xDC143C), shadow(1.0, 3.0, 0.25, Color::BLACK)),
        }
    }
}

/// Which thumb a press grabbed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RangeZone {
    #[default]
    Unknown,
    Start,
    End,
}

type Changed = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, f32)>;

pub struct SkiaSlider {
    pub p: SliderProps,
    id: ControlId,
    /// Thumb positions in points from the left edge (C# StartThumbX / EndThumbX). A drag moves
    /// them freely; the values follow in steps.
    pub start_thumb_x: f32,
    pub end_thumb_x: f32,
    /// A press is on the slider.
    pub is_pressed: bool,
    /// The press moves a thumb.
    pub is_user_panning: bool,
    touch: RangeZone,
    last_touch_x: f32,
    /// Width at the last arrange, points.
    width: f32,
    /// The values the handlers last saw; `None` before the first apply.
    reported: Option<(f32, f32)>,
    pending: (Option<f32>, Option<f32>),
    /// A frame animator will run the handlers.
    firing: bool,
    on_start_changed: Option<Changed>,
    on_end_changed: Option<Changed>,
    /// The thumb shadow for (style, scale).
    shadow: Option<(PrebuiltControlStyle, f32, Option<ImageFilter>)>,
}

impl SkiaSlider {
    /// Fills the width, at least 64 points, ImageDoubleBuffered (DrawnUI's default).
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaSlider> {
        let slider = SkiaSlider {
            p: SliderProps::default(),
            id: crate::tree::Handle::<SkiaSlider>::default().id(),
            start_thumb_x: 0.0,
            end_thumb_x: 0.0,
            is_pressed: false,
            is_user_panning: false,
            touch: RangeZone::Unknown,
            last_touch_x: 0.0,
            width: 0.0,
            reported: None,
            pending: (None, None),
            firing: false,
            on_start_changed: None,
            on_end_changed: None,
            shadow: None,
        };
        let mut build = Build::new(slider)
            .horizontal_options(LayoutOptions::Fill)
            .minimum_width_request(64.0)
            .use_cache(CacheType::ImageDoubleBuffered);
        let id = build.id();
        build.control_mut().id = id;
        build
    }

    /// The look with the app's colors over the style's.
    pub fn look(&self) -> SliderLook {
        let mut look = SliderLook::of(self.p.control_style);
        look.thumb_color = self.p.thumb_color.unwrap_or(look.thumb_color);
        look.track = self.p.track_color.unwrap_or(look.track);
        look.selected = self.p.track_selected_color.unwrap_or(look.selected);
        look
    }

    /// The thumb box and the control's height, points (C# SliderHeight).
    pub fn slider_height(&self) -> f32 {
        if self.p.slider_height >= 0.0 {
            return self.p.slider_height;
        }
        let range = self.p.enable_range && self.p.control_style.resolve() != PrebuiltControlStyle::Unset;
        SliderLook::of(self.p.control_style).thumb + if range { 8.0 } else { 0.0 }
    }

    fn clamp(&self, v: f32) -> f32 {
        v.min(self.p.max).max(self.p.min)
    }

    /// How far a thumb travels, points.
    fn total_length(&self) -> f32 {
        (self.width - self.slider_height()).max(0.0)
    }

    fn position_from_value(&self, v: f32) -> f32 {
        let p = &self.p;
        if p.max > p.min { (v - p.min) / (p.max - p.min) * self.total_length() } else { 0.0 }
    }

    fn value_from_position(&self, x: f32) -> f32 {
        let total = self.total_length();
        if total <= 0.0 { self.p.min } else { self.p.min + x / total * (self.p.max - self.p.min) }
    }

    fn adjust_to_step(&self, v: f32) -> f32 {
        let (min, step) = (self.p.min, self.p.step);
        // JavaScript Math.round: halves go up.
        if step > 0.0 { min + ((v - min) / step + 0.5).floor() * step } else { v }
    }

    /// What `range_min` keeps between the two thumbs, as C# and React compute it: the value
    /// divided by the step, used as points.
    fn range_gap(&self) -> f32 {
        self.p.range_min / if self.p.step != 0.0 { self.p.step } else { 1.0 }
    }

    fn set_start_offset_clamped(&mut self, x: f32) {
        let max = if self.p.enable_range { self.end_thumb_x - self.range_gap() } else { self.total_length() };
        self.start_thumb_x = x.min(max).max(0.0);
    }

    fn set_end_offset_clamped(&mut self, x: f32) {
        let min = if self.p.enable_range { self.start_thumb_x + self.range_gap() } else { 0.0 };
        self.end_thumb_x = x.min(self.total_length()).max(min);
    }

    fn sync_thumbs(&mut self) {
        self.start_thumb_x = self.position_from_value(self.p.start);
        self.end_thumb_x = self.position_from_value(self.p.end);
    }

    /// C# RecalculateValues: thumb positions to values in steps.
    fn recalculate(&mut self, tree: &mut Tree) {
        if self.p.enable_range {
            let start = self.adjust_to_step(self.value_from_position(self.start_thumb_x));
            if start != self.p.start {
                self.p.start = self.clamp(start);
            }
        }
        let end = self.adjust_to_step(self.value_from_position(self.end_thumb_x));
        if end != self.p.end {
            self.p.end = self.clamp(end);
        }
        self.report(tree);
    }

    /// Values that changed since the handlers last saw them are handed to them at the start of
    /// the next frame, before the observers. The first apply reports nothing.
    fn report(&mut self, tree: &mut Tree) {
        let now = (self.p.start, self.p.end);
        let Some(before) = self.reported.replace(now) else { return };
        if now.0 != before.0 && self.on_start_changed.is_some() {
            self.pending.0 = Some(now.0);
        }
        if now.1 != before.1 && self.on_end_changed.is_some() {
            self.pending.1 = Some(now.1);
        }
        if (self.pending.0.is_some() || self.pending.1.is_some()) && !std::mem::replace(&mut self.firing, true) {
            animators::start_frame(tree, self.id, fire);
        }
    }

    /// C# SkiaSlider.OnAccessibilityKey: Right / Up and Left / Down move `end` by a step (a
    /// hundredth of the range without one), PageUp / PageDown by a tenth of the range, Home / End
    /// to the lowest value (`start` in range mode) and `max`. True for these keys, also when the
    /// value cannot move further.
    fn key(&mut self, key: InputKey, tree: &mut Tree) -> bool {
        let p = &self.p;
        let range = p.max - p.min;
        let small = if p.step > 0.0 { p.step } else { range / 100.0 };
        let large = small.max(range / 10.0);
        let low = if p.enable_range { p.start } else { p.min };
        let value = match key {
            "ArrowRight" | "ArrowUp" => p.end + small,
            "ArrowLeft" | "ArrowDown" => p.end - small,
            "PageUp" => p.end + large,
            "PageDown" => p.end - large,
            "Home" => low,
            "End" => p.max,
            _ => return false,
        };
        self.p.end = value.min(self.p.max).max(low);
        self.end_thumb_x = self.position_from_value(self.p.end);
        self.report(tree);
        true
    }

    /// Draws one thumb centered at `x` (C# SliderThumb looks).
    fn paint_thumb(&self, cx: &mut PaintCx, look: &SliderLook, paint: &mut Paint, x: f32, y: f32) {
        let (canvas, scale) = (cx.canvas, cx.scale);
        let thumb = look.thumb * scale;
        let filter = self.shadow.as_ref().and_then(|s| s.2.clone());
        // The thumb's body carries the style's shadow.
        let mut body = |radius: f32, color: Color| {
            paint.set_image_filter(filter.clone());
            paint.set_color(color);
            canvas.draw_circle((x, y), radius, paint);
            paint.set_image_filter(None);
        };
        match self.p.control_style.resolve() {
            PrebuiltControlStyle::Windows => {
                body(thumb / 2.0, Color::WHITE);
                paint.set_style(PaintStyle::Stroke).set_stroke_width(scale).set_color(Color::from_rgb(0xE5, 0xE5, 0xE5));
                canvas.draw_circle((x, y), thumb / 2.0 - scale / 2.0, paint);
                paint.set_style(PaintStyle::Fill).set_color(look.thumb_color);
                canvas.draw_circle((x, y), 5.0 * scale, paint);
            }
            PrebuiltControlStyle::Cupertino => {
                body(thumb / 2.0, look.thumb_color);
                paint.set_style(PaintStyle::Stroke).set_stroke_width(0.5 * scale).set_color(Color::from_rgb(0xCC, 0xCC, 0xCC));
                canvas.draw_circle((x, y), thumb / 2.0, paint);
                paint.set_style(PaintStyle::Fill);
            }
            PrebuiltControlStyle::Unset => {
                // The accent circle sits 5 points inside the thumb box, a white dot in its middle.
                body((thumb - 10.0 * scale) / 2.0, look.thumb_color);
                paint.set_color(Color::WHITE);
                canvas.draw_circle((x, y), 3.0 * scale, paint);
            }
            _ => body(thumb / 2.0, look.thumb_color),
        }
    }
}

/// The frame animator that runs the value handlers with the app state: the node leaves its slot
/// meanwhile, like for a tapped handler, so the handlers can reach the rest of the tree.
fn fire(id: ControlId, _time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return tick };
    let mut queue = Vec::new();
    if let Some(control) = node.kind.as_deref_mut() {
        let due = part_mut::<SkiaSlider>(control).map(|s| {
            s.firing = false;
            let pending = std::mem::take(&mut s.pending);
            (pending, s.on_start_changed.take(), s.on_end_changed.take())
        });
        if let Some(((start, end), mut on_start, mut on_end)) = due {
            for (value, handler) in [(start, &mut on_start), (end, &mut on_end)] {
                if let (Some(value), Some(handler)) = (value, handler.as_mut()) {
                    handler(Raw { id, control: &mut *control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, value);
                    tick.state_touched = true;
                }
            }
            if let Some(slider) = part_mut::<SkiaSlider>(control) {
                slider.on_start_changed = on_start;
                slider.on_end_changed = on_end;
            }
        }
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    tick
}

impl Has<SliderProps> for SkiaSlider {
    fn part(&self) -> &SliderProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut SliderProps {
        &mut self.p
    }
}

impl Control for SkiaSlider {
    fn receives_hover(&self) -> bool {
        true
    }
    /// Values set from code: clamped, the thumbs follow, the handlers hear of it.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        (self.p.start, self.p.end) = (self.clamp(self.p.start), self.clamp(self.p.end));
        self.sync_thumbs();
        self.report(cx.tree);
    }

    /// The width it is given (200 points when unbounded), `slider_height` tall.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, _height: f32) -> Size {
        let scale = cx.scale;
        Size::new(if width.is_finite() { width } else { 200.0 * scale }, self.slider_height() * scale)
    }

    /// The thumbs go where the values are (C# OnLayoutChanged).
    fn arrange(&mut self, cx: &mut LayoutCx) {
        let scale = cx.scale.max(f32::EPSILON);
        self.width = cx.base().rect.width() / scale;
        self.sync_thumbs();
        let style = self.p.control_style.resolve();
        if self.shadow.as_ref().is_none_or(|s| s.0 != style || s.1 != scale) {
            self.shadow = Some((style, scale, paint::create_shadow(&SliderLook::of(style).shadow, scale)));
        }
    }

    /// The thumb shadow paints outside the box.
    fn effects_margin(&self, scale: f32) -> Thickness {
        let s = SliderLook::of(self.p.control_style).shadow;
        let (spread, x, y) = (3.0 * s.blur * scale, s.x * scale, s.y * scale);
        Thickness::new((spread - x).max(0.0), (spread - y).max(0.0), spread + x, spread + y)
    }

    fn paint(&self, cx: &mut PaintCx) {
        let (look, scale, d) = (self.look(), cx.scale, cx.rect);
        let h = self.slider_height() * scale;
        let cy = d.top + d.height() / 2.0;
        let r = look.track_height * scale / 2.0;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(look.track);
        cx.canvas.draw_rrect(RRect::new_rect_xy(Rect::new(d.left, cy - r, d.right, cy + r), r, r), &paint);
        // The trail between the thumb centers, from the left edge in single mode.
        let end = d.left + self.end_thumb_x * scale + h / 2.0;
        let start = if self.p.enable_range { d.left + self.start_thumb_x * scale + h / 2.0 } else { d.left };
        paint.set_color(look.selected);
        if end > start {
            cx.canvas.draw_rrect(RRect::new_rect_xy(Rect::new(start, cy - r, end, cy + r), r, r), &paint);
        }
        if self.p.enable_range {
            self.paint_thumb(cx, &look, &mut paint, start, cy);
        }
        self.paint_thumb(cx, &look, &mut paint, end, cy);
    }

    /// C# SkiaSlider.ProcessGestures, one pointer: Down grabs the thumb under it (or moves the
    /// nearest one there when the trail is pressed), Panning drags it, Up lets go.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if !self.p.responds_to_gestures {
            return Handled::No;
        }
        let base = cx.base();
        let scale = base.scale.max(f32::EPSILON);
        let local_x = (cx.point.x - base.rect.left) / scale;
        let (h, more) = (self.slider_height(), self.p.more_hotspot_size);
        let over = |thumb: f32| local_x >= thumb - more && local_x <= thumb + h + more;
        match gesture.kind {
            GestureKind::Down => {
                self.is_user_panning = false;
                self.touch = if self.p.enable_range && over(self.start_thumb_x) {
                    RangeZone::Start
                } else if over(self.end_thumb_x) {
                    RangeZone::End
                } else {
                    RangeZone::Unknown
                };
                self.is_pressed = true;
                if self.touch == RangeZone::Unknown && self.p.click_on_trail_enabled {
                    if self.p.enable_range && local_x <= self.width / 2.0 {
                        self.touch = RangeZone::Start;
                        self.set_start_offset_clamped(local_x - h / 2.0);
                    } else {
                        self.touch = RangeZone::End;
                        self.set_end_offset_clamped(local_x - h / 2.0);
                    }
                    self.recalculate(cx.tree);
                    cx.invalidate(crate::Dirty::DRAW);
                }
                self.last_touch_x = if self.touch == RangeZone::Start { self.start_thumb_x } else { self.end_thumb_x };
                Handled::Yes
            }
            GestureKind::Panning => {
                if self.touch == RangeZone::Unknown {
                    return Handled::No;
                }
                self.is_user_panning = true;
                let x = self.last_touch_x + gesture.total.x / scale;
                if self.touch == RangeZone::Start {
                    self.set_start_offset_clamped(x);
                } else {
                    self.set_end_offset_clamped(x);
                }
                self.recalculate(cx.tree);
                cx.invalidate(crate::Dirty::DRAW);
                Handled::Yes
            }
            GestureKind::Up => {
                self.is_user_panning = false;
                self.is_pressed = false;
                Handled::No
            }
            GestureKind::Tapped if self.touch != RangeZone::Unknown => Handled::Yes,
            _ => Handled::No,
        }
    }

    /// Keys while focused move the value (C# OnAccessibilityKey); an activation (a tap at the
    /// center with no press before it) leaves it, as C# OnAccessibilityActivated.
    fn on_key(&mut self, cx: &mut GestureCx, event: &KeyEvent) -> bool {
        if event.kind != KeyKind::Down || !self.p.responds_to_gestures || !self.key(event.key, cx.tree) {
            return false;
        }
        cx.invalidate(crate::Dirty::DRAW);
        true
    }

    /// `end` moves to the value, on a step, as `key` moves it.
    fn accessibility_set_value(&mut self, cx: &mut GestureCx, value: f64) -> bool {
        if !self.p.responds_to_gestures {
            return false;
        }
        let low = if self.p.enable_range { self.p.start } else { self.p.min };
        self.p.end = self.adjust_to_step(value as f32).min(self.p.max).max(low);
        self.end_thumb_x = self.position_from_value(self.p.end);
        self.report(cx.tree);
        cx.invalidate(crate::Dirty::DRAW);
        true
    }

    /// `end` between `min` and `max`, read as "25", or "20 – 80" in range mode (C#
    /// DefaultAccessibilityLabel, which made the value the name). The step is the arrow key's
    /// (`key`). Asked only when the snapshot is built.
    fn accessibility_value(&self) -> Option<crate::AccessibilityValue> {
        let p = &self.p;
        Some(crate::AccessibilityValue {
            now: p.end as f64,
            min: p.min as f64,
            max: p.max as f64,
            step: if p.step > 0.0 { p.step } else { (p.max - p.min) / 100.0 } as f64,
            text: match p.enable_range {
                true => format!("{} \u{2013} {}", p.start, p.end),
                false => String::new(),
            },
        })
    }

    /// React DefaultAccessibilityRole.
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::SLIDER)
    }

    /// A tab stop while it takes gestures (React DefaultAccessibilityCanInteract).
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(self.p.responds_to_gestures)
    }
}

impl Build<SkiaSlider> {
    /// Runs after `start` changed (range mode): by a drag, a press on the trail, or set from
    /// code. `value` is the new one. It runs at the start of the next frame, before the observers
    /// (DrawnUI StartChanged); several changes in one frame are reported once, with the last.
    pub fn on_start_changed<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, SkiaSlider>, &mut S, &mut Cx<'_>, f32) + 'static) -> Self {
        self.control_mut().on_start_changed = Some(changed(f));
        self
    }

    /// Runs after `end` changed, as `on_start_changed` (DrawnUI EndChanged).
    pub fn on_end_changed<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, SkiaSlider>, &mut S, &mut Cx<'_>, f32) + 'static) -> Self {
        self.control_mut().on_end_changed = Some(changed(f));
        self
    }
}

fn changed<S: Any>(mut f: impl FnMut(&mut Mut<'_, SkiaSlider>, &mut S, &mut Cx<'_>, f32) + 'static) -> Changed {
    Box::new(move |me, state, cx, value| {
        let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
        f(&mut me.typed(), state, cx, value)
    })
}
