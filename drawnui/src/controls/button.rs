//! SkiaButton: a rounded frame with a centered caption. Consumes Down / Up / Tapped inside its
//! rect and lets a press go when it turns into a pan. `control_style` picks the look (fill,
//! corners, font, minimum size, shadow) for what the app left unset, with the values of
//! DrawnUI.React (SkiaButton.ts).
// ponytail: the frame is painted by the button itself (DrawnUI composes a "BtnShape" child).

use skia_safe::{Color, ImageFilter, Paint, PaintStyle, Path, Point, RRect, Rect, Size};

use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::label::{LabelSet, SkiaLabel};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::gestures::{Gesture, GestureKind};
use crate::{paint, props};
use crate::tree::{Build, Cx, Handle};
use crate::types::{Dirty, LayoutOptions, SkiaShadow, Thickness};
use crate::ui::Aria;

/// A press that moves further than this (points) is a pan, not a press.
const PAN_THRESHOLD: f32 = 5.0;
/// The values that mean "not customized": the look's own apply (DrawnUI.React).
const UNSET_FONT_SIZE: f32 = 15.0;
const UNSET_CORNER_RADIUS: f32 = 8.0;

/// What a style gives a button where the app left the value unset (the C# style builders, with
/// the numbers of DrawnUI.React).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ButtonLook {
    pub background: Color,
    /// Points.
    pub corner_radius: f32,
    /// Points.
    pub font_size: f32,
    pub font_weight: i32,
    /// Points; every style floors the width at 100.
    pub minimum_width: f32,
    pub minimum_height: f32,
    pub shadow: Option<SkiaShadow>,
}

impl ButtonLook {
    /// The look of a style; `Platform` resolves first.
    pub fn of(style: PrebuiltControlStyle) -> ButtonLook {
        use PrebuiltControlStyle::*;
        let shadow = |y, blur, opacity| Some(SkiaShadow::new(Color::BLACK).x(0.0).y(y).blur(blur).opacity(opacity));
        let look = |rgb: u32, corner_radius, font_size, font_weight, minimum_height, shadow| ButtonLook {
            background: Color::new(0xFF00_0000 | rgb),
            corner_radius,
            font_size,
            font_weight,
            minimum_width: 100.0,
            minimum_height,
            shadow,
        };
        match style.resolve() {
            Cupertino => look(0x007AFF, 8.0, 17.0, 600, 36.0, shadow(1.0, 2.0, 0.2)),
            Material => look(0x2196F3, 4.0, 14.0, 0, 40.0, shadow(2.0, 4.0, 0.3)),
            Material3 => look(0x6750A4, 20.0, 14.0, 0, 40.0, None),
            Windows => look(0x0078D7, 4.0, 15.0, 500, 32.0, shadow(1.0, 1.0, 0.2)),
            _ => look(0xDC143C, 8.0, 15.0, 0, 41.0, None),
        }
    }
}

props!(ButtonProps, ButtonBuild, ButtonSet {
    text / set_text: String = String::new(), APPLY;
    text_color / set_text_color: Color = Color::WHITE, APPLY;
    /// Points. 15 = the style's.
    font_size / set_font_size: f32 = 15.0, APPLY;
    font_family / set_font_family: String = String::new(), APPLY;
    /// Font aliases, comma separated, for glyphs `font_family` lacks (the caption's
    /// `font_family_fallback`).
    font_family_fallback / set_font_family_fallback: String = String::new(), APPLY;
    /// Points. 8 = the style's.
    corner_radius / set_corner_radius: f32 = 8.0, DRAW;
    stroke_color / set_stroke_color: Color = Color::TRANSPARENT, DRAW;
    stroke_width / set_stroke_width: f32 = 0.0, DRAW;
    is_disabled / set_is_disabled: bool = false, NONE;
    /// Touch feedback played on Down, in the control's `touch_effect_color`.
    apply_effect / set_apply_effect: SkiaTouchAnimation = SkiaTouchAnimation::Ripple, NONE;
    /// The look for what the app left unset: fill, corners, font, minimum size, shadow.
    control_style / set_control_style: PrebuiltControlStyle = PrebuiltControlStyle::Unset, MEASURE_APPLY;
});

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SkiaTouchAnimation {
    None,
    #[default]
    Ripple,
}

pub struct SkiaButton {
    layout: SkiaLayout,
    pub p: ButtonProps,
    label: Handle<SkiaLabel>,
    pub is_pressed: bool,
    had_down: bool,
    last_down: Point,
    /// Minimum width and height requests the look set: a style change moves only these.
    pinned: [bool; 2],
    /// The frame's shadow for (style, scale).
    shadow: Option<(PrebuiltControlStyle, f32, Option<ImageFilter>)>,
}

impl SkiaButton {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: impl Into<String>) -> Build<SkiaButton> {
        // The button is the accessible node, not its caption (C# MainLabel.AccessibilityRole = null).
        let label = SkiaLabel::new("").tag("BtnText").center().accessibility_role(Aria::PRESENTATION);
        let button = SkiaButton {
            layout: SkiaLayout::default(),
            p: ButtonProps::default(),
            label: label.handle(),
            is_pressed: false,
            had_down: false,
            last_down: Point::default(),
            pinned: [false; 2],
            shadow: None,
        };
        let mut build = Build::new(button).text(text.into()).padding((16, 10));
        build.push_child(label);
        build
    }

    /// The look of `control_style`.
    pub fn look(&self) -> ButtonLook {
        ButtonLook::of(self.p.control_style)
    }

    fn frame(&self, rect: Rect, scale: f32) -> RRect {
        let radius = if self.p.corner_radius == UNSET_CORNER_RADIUS { self.look().corner_radius } else { self.p.corner_radius };
        RRect::new_rect_xy(rect, radius * scale, radius * scale)
    }

    fn release(&mut self, cx: &mut GestureCx) {
        self.is_pressed = false;
        self.had_down = false;
        cx.invalidate(Dirty::REPAINT);
    }
}

impl Has<ButtonProps> for SkiaButton {
    fn part(&self) -> &ButtonProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ButtonProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaButton {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Control for SkiaButton {
    fn receives_hover(&self) -> bool {
        true
    }
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The caption follows the button's own text properties.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        let look = self.look();
        if let Some(mut label) = cx.get_mut(self.label) {
            label.set_text(self.p.text.clone());
            label.set_text_color(self.p.text_color);
            label.set_font_size(if self.p.font_size == UNSET_FONT_SIZE { look.font_size } else { self.p.font_size });
            label.set_font_weight(look.font_weight);
            label.set_font_family(self.p.font_family.clone());
            label.set_font_family_fallback(self.p.font_family_fallback.clone());
        }
    }

    /// Caption plus padding. The style's minimum size applies where the app left the size unset.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let (scale, look) = (cx.scale, self.look());
        let p = &mut cx.base_mut().p;
        if self.pinned[0] || p.width_request < 0.0 && p.minimum_width_request < 0.0 && p.horizontal_options != LayoutOptions::Fill {
            p.minimum_width_request = look.minimum_width;
            self.pinned[0] = true;
        }
        if self.pinned[1] || p.height_request < 0.0 && p.minimum_height_request < 0.0 && p.vertical_options != LayoutOptions::Fill {
            p.minimum_height_request = look.minimum_height;
            self.pinned[1] = true;
        }
        let (px, py) = (p.padding.horizontal() * scale, p.padding.vertical() * scale);
        let caption = cx.measure_child(self.label.id(), width - px, height - py);
        Size::new(caption.width + px, caption.height + py)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let scale = cx.scale;
        let (r, p) = (cx.base().rect, cx.base().p.padding);
        let inner = Rect::new(r.left + p.left * scale, r.top + p.top * scale, r.right - p.right * scale, r.bottom - p.bottom * scale);
        cx.arrange_child(self.label.id(), inner);
        let style = self.p.control_style.resolve();
        if self.shadow.as_ref().is_none_or(|s| s.0 != style || s.1 != scale) {
            let filter = ButtonLook::of(style).shadow.and_then(|s| paint::create_shadow(&s, scale));
            self.shadow = Some((style, scale, filter));
        }
    }

    /// The look's shadow paints outside the frame (C# MergeShadowMargin).
    fn effects_margin(&self, scale: f32) -> Thickness {
        let Some(s) = self.look().shadow else { return Thickness::ZERO };
        let (spread, dx, dy) = (3.0 * s.blur * scale, s.x * scale, s.y * scale);
        Thickness::new(spread - dx, spread - dy, spread + dx, spread + dy)
    }

    /// The caption is the spoken label (React DefaultAccessibilityLabel).
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        (!self.p.text.is_empty()).then(|| self.p.text.as_str().into())
    }

    /// A disabled button is no tab stop (React DefaultAccessibilityCanInteract).
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(!self.p.is_disabled)
    }

    /// The ripple stays inside the rounded frame.
    fn create_clip(&self, rect: Rect, scale: f32) -> Path {
        Path::rrect(self.frame(rect, scale), None)
    }

    fn paint_background(&self, cx: &mut PaintCx) {
        let mut frame = self.frame(cx.rect, cx.scale);
        // The control's gradient or color; the style's fill when it has neither.
        // The style's fill is for a button the app gave no background; a transparent one has none.
        let unset = cx.base().p.background_color.is_none();
        let fill = paint::background_paint(cx, cx.rect, (0.0, 0.0)).or_else(|| {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(self.look().background);
            unset.then_some(paint)
        });
        if let Some(mut fill) = fill {
            // The look's shadow is drawn with the fill, as a shape draws its shadows.
            fill.set_image_filter(self.shadow.as_ref().and_then(|s| s.2.clone()));
            cx.canvas.draw_rrect(frame, &fill);
        }
        if self.p.stroke_width > 0.0 && self.p.stroke_color.a() > 0 {
            let width = self.p.stroke_width * cx.scale;
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(width);
            paint.set_color(self.p.stroke_color);
            frame.inset((width / 2.0, width / 2.0));
            cx.canvas.draw_rrect(frame, &paint);
        }
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if self.p.is_disabled {
            return Handled::No;
        }
        match gesture.kind {
            GestureKind::Down => {
                self.is_pressed = true;
                self.had_down = true;
                self.last_down = gesture.location;
                cx.invalidate(Dirty::REPAINT);
                if self.p.apply_effect == SkiaTouchAnimation::Ripple {
                    // From the press point, in points inside the button.
                    let (id, point, base) = (cx.id, cx.point, cx.base());
                    let (x, y) = ((point.x - base.rect.left) / base.scale, (point.y - base.rect.top) / base.scale);
                    let color = base.p.touch_effect_color;
                    cx.cx().play_ripple(id, color, x, y, 0.0);
                }
                Handled::Yes
            }
            GestureKind::Panning => {
                let threshold = PAN_THRESHOLD * cx.base().scale;
                let moved = gesture.location - self.last_down;
                if moved.x.abs() > threshold || moved.y.abs() > threshold {
                    if self.had_down {
                        self.release(cx);
                    }
                    return Handled::No;
                }
                if self.had_down { Handled::Yes } else { Handled::No }
            }
            GestureKind::Up => {
                self.release(cx);
                Handled::No
            }
            GestureKind::Tapped => Handled::Tapped,
            GestureKind::Wheel => Handled::No,
            _ => Handled::No,
        }
    }
}
