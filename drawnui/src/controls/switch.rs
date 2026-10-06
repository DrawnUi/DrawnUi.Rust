//! SkiaSwitch: a `Frame` shape and a `Thumb` circle that slides to the on end, in the Default,
//! Cupertino, Material, Material3 and Windows looks of the C# style builders. The colors change
//! at once and the thumb moves over 200 ms (DrawnUI.React; C# changes the colors when the thumb
//! arrives).

use skia_safe::Color;

use crate::animators::easing;
use crate::control::{Control, Has, LayoutCx};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::layout::LayoutProps;
use crate::controls::shape::{ShapeBuild, ShapeSet, ShapeType, SkiaShape};
use crate::controls::toggle::{SkiaToggle, ToggleColor, ToggleProps};
use crate::tree::{Build, Cx, Handle};
use crate::types::{CacheType, LayoutOptions, SkiaShadow, Thickness};
use crate::ui::Aria;

/// Milliseconds the thumb takes to the other end (DrawnUI SkiaSwitch.AnimationSpeed).
pub const ANIMATION_SPEED_MS: f32 = 200.0;
/// Material 3 outline, the unselected track border and thumb.
const MATERIAL_OUTLINE: Color = Color::from_rgb(0x79, 0x74, 0x7E);

pub struct SkiaSwitch {
    toggle: SkiaToggle,
    track: Handle<SkiaShape>,
    thumb: Handle<SkiaShape>,
    /// Width of the track at the last arrange, pixels: a change snaps the thumb to its place.
    track_width: f32,
}

impl SkiaSwitch {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaSwitch> {
        let switch = SkiaSwitch { toggle: SkiaToggle::new(), track: Handle::default(), thumb: Handle::default(), track_width: 0.0 };
        let mut build = Build::new(switch);
        let id = build.id();
        build.control_mut().toggle.id = id;
        build
    }

    /// The style's colors where the app set none (C# SetStyleDefault per style).
    fn style_default(style: PrebuiltControlStyle, which: ToggleColor) -> Option<Color> {
        use PrebuiltControlStyle::*;
        use ToggleColor::*;
        let rgb = |r, g, b| Some(Color::from_rgb(r, g, b));
        match (style, which) {
            (Cupertino, FrameOff) => rgb(0xE5, 0xE5, 0xE5),
            (Cupertino, FrameOn) => rgb(0x30, 0xD1, 0x58),
            (Cupertino, ThumbOff | ThumbOn) => Some(Color::WHITE),
            (Material, FrameOff) => rgb(0x9E, 0x9E, 0x9E),
            (Material, FrameOn) => rgb(0x21, 0x96, 0xF3),
            (Material, ThumbOff | ThumbOn) => Some(Color::WHITE),
            (Material3, FrameOff) => rgb(0xE6, 0xE0, 0xE9),
            (Material3, FrameOn) => rgb(0x67, 0x50, 0xA4),
            (Material3, ThumbOff) => Some(MATERIAL_OUTLINE),
            (Material3, ThumbOn) => Some(Color::WHITE),
            (Windows, FrameOff | ThumbOff) => rgb(0x76, 0x76, 0x76),
            (Windows, FrameOn) => rgb(0x00, 0x78, 0xD7),
            (Windows, ThumbOn) => Some(Color::WHITE),
            // The flat DrawnUI look: crimson on, neutral off; thumbs keep the SkiaToggle white.
            (_, FrameOn) => rgb(0xDC, 0x14, 0x3C),
            (_, FrameOff) => rgb(0xD7, 0xDB, 0xE0),
            _ => None,
        }
    }

    fn color(&self, which: ToggleColor) -> Color {
        self.toggle.color(which, Self::style_default(self.toggle.using_control_style(), which))
    }

    /// C# Create*StyleContent: the frame and the thumb of the style, and the size it pins.
    fn build_content(&mut self, cx: &mut Cx) {
        let mut size = (46.0, 28.0);
        let (mut track, mut thumb_handle) = (Handle::default(), Handle::default());
        self.toggle.rebuild(cx, |style| {
            use PrebuiltControlStyle::*;
            let mut frame = SkiaShape::new().tag("Frame").shape_type(ShapeType::Rectangle).fill();
            let mut thumb = SkiaShape::new()
                .tag("Thumb")
                .shape_type(ShapeType::Circle)
                .horizontal_options(LayoutOptions::Start)
                .vertical_options(LayoutOptions::Fill)
                .lock_ratio(-1.0)
                .use_cache(CacheType::Operations);
            let shadow = |x, y, blur| SkiaShadow::new(Color::BLACK).x(x).y(y).blur(blur).opacity(0.1);
            match style {
                Cupertino => {
                    size = (51.0, 31.0);
                    frame = frame.corner_radius(100.0);
                    thumb = thumb.margin(2.0).shadows(shadow(0.0, 3.0, 3.0));
                }
                Material => {
                    frame = frame.corner_radius(7.0).height_request(15.0).vertical_options(LayoutOptions::Center);
                    thumb = thumb.margin(Thickness::ZERO).shadows(shadow(1.0, 1.0, 3.0));
                }
                Material3 => {
                    size = (52.0, 32.0);
                    frame = frame.corner_radius(16.0).stroke_width(2.0).stroke_color(MATERIAL_OUTLINE);
                    thumb = thumb
                        .margin(4.0)
                        .width_request(24.0)
                        .lock_ratio(1.0)
                        .vertical_options(LayoutOptions::Center)
                        .shadows(shadow(1.0, 1.0, 3.0));
                }
                Windows => {
                    size = (48.0, 22.0);
                    frame = frame.corner_radius(12.0).stroke_width(2.5).stroke_color(Color::from_rgb(0x76, 0x76, 0x76));
                    thumb = thumb.margin(5.5);
                }
                _ => {
                    frame = frame.corner_radius(20.0);
                    thumb = thumb.margin(2.0);
                }
            }
            vec![frame.assign(&mut track).into(), thumb.assign(&mut thumb_handle).into()]
        });
        (self.track, self.thumb) = (track, thumb_handle);
        self.toggle.pin_size(cx, size.0, size.1);
        self.track_width = 0.0;
    }

    /// Where the thumb rests when on, points (C# GetThumbPosForOn): the track's width minus the
    /// thumb's and its margins. `None` before the layout placed them.
    fn thumb_pos_for_on(&self, cx: &Cx) -> Option<f32> {
        let (track, thumb) = (cx.base(self.track)?, cx.base(self.thumb)?);
        if thumb.rect.width() <= 0.0 {
            return None;
        }
        let scale = thumb.scale.max(f32::EPSILON);
        let track_w = track.rect.width() / scale + track.p.margin.horizontal();
        Some(track_w - thumb.rect.width() / scale - thumb.p.margin.horizontal())
    }

    /// C# ApplyOn / ApplyOff: the colors, then the thumb goes to its end.
    fn apply(&mut self, cx: &mut Cx) {
        let (on, style) = (self.toggle.p.is_toggled, self.toggle.using_control_style());
        let (frame_on, frame_off) = (self.color(ToggleColor::FrameOn), self.color(ToggleColor::FrameOff));
        let thumb_color = if on { self.color(ToggleColor::ThumbOn) } else { self.color(ToggleColor::ThumbOff) };
        if let Some(mut thumb) = cx.get_mut(self.thumb) {
            thumb.set_background_color(thumb_color);
        }
        if let Some(mut track) = cx.get_mut(self.track) {
            use PrebuiltControlStyle::*;
            match (on, style) {
                (true, Windows | Material3) => {
                    track.set_background_color(frame_on);
                    track.set_stroke_color(frame_on);
                }
                (true, _) => track.set_background_color(frame_on),
                (false, Windows) => {
                    track.set_background_color(Color::TRANSPARENT);
                    track.set_stroke_color(frame_off);
                }
                (false, Material3) => {
                    track.set_background_color(frame_off);
                    track.set_stroke_color(MATERIAL_OUTLINE);
                }
                (false, _) => track.set_background_color(frame_off),
            }
        }
        // Not laid out yet: the arrange snaps the thumb.
        let Some(target) = (if on { self.thumb_pos_for_on(cx) } else { Some(0.0) }) else { return };
        let current = cx.base(self.thumb).map_or(0.0, |b| b.p.translation_x);
        if self.toggle.p.is_animated && (current - target).abs() > 0.5 {
            cx.translate_to(self.thumb, target, 0.0, ANIMATION_SPEED_MS, easing::cubic_out);
        } else if let Some(mut thumb) = cx.get_mut(self.thumb) {
            thumb.set_translation_x(target);
        }
    }
}

impl Has<ToggleProps> for SkiaSwitch {
    fn part(&self) -> &ToggleProps {
        &self.toggle.p
    }
    fn part_mut(&mut self) -> &mut ToggleProps {
        &mut self.toggle.p
    }
}

impl Has<LayoutProps> for SkiaSwitch {
    fn part(&self) -> &LayoutProps {
        Has::<LayoutProps>::part(&self.toggle)
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        Has::<LayoutProps>::part_mut(&mut self.toggle)
    }
}

impl Control for SkiaSwitch {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.toggle)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.toggle)
    }

    /// Builds the look for the style, then pushes the state and the colors into it.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if self.toggle.needs_content() {
            self.build_content(cx);
        }
        self.toggle.take_change(cx);
        self.apply(cx);
    }

    /// React DefaultAccessibilityRole.
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::SWITCH)
    }

    /// After the children are placed: a track that changed its width puts the thumb where the
    /// state says, without animation (C# OnLayoutChanged).
    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.toggle.arrange(cx);
        let width = cx.child_base(self.track.id()).rect.width();
        if width == self.track_width {
            return;
        }
        self.track_width = width;
        let on = self.toggle.p.is_toggled;
        let mut cx = Cx { tree: &mut *cx.tree };
        let Some(target) = (if on { self.thumb_pos_for_on(&cx) } else { Some(0.0) }) else { return };
        if let Some(mut thumb) = cx.get_mut(self.thumb)
            && (thumb.base().p.translation_x - target).abs() > 0.5
        {
            thumb.set_translation_x(target);
        }
    }
}
