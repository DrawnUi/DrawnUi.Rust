//! SkiaCheckbox: a `FrameOff` outline, a `FrameOn` frame and a `ViewCheckOn` mark inside it, in the
//! Default, Cupertino, Material, Material3 and Windows looks of DrawnUI.React (SkiaCheckbox.ts):
//! the platform looks fill the frame and draw an SVG check, the default one strokes the frame and
//! fills an inner square.

use skia_safe::Color;

use crate::control::{Control, Has};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::layout::LayoutProps;
use crate::controls::shape::{ShapeBuild, ShapeSet, ShapeType, SkiaShape};
use crate::controls::svg::{SkiaSvg, SvgBuild, SvgSet};
use crate::controls::toggle::{SkiaToggle, ToggleColor, ToggleProps};
use crate::props;
use crate::tree::{Build, ControlId, Cx, Detached, Handle};
use crate::types::CacheType;
use crate::ui::Aria;

const SVG_CUPERTINO_CHECK: &str = r##"<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none"><path d="M4 12.6111L8.92308 17.5L20 6.5" stroke="#000000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const SVG_MATERIAL_CHECK: &str = r##"<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none"><path d="M5 13L9 17L19 7" stroke="#000000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const SVG_WINDOWS_CHECK: &str = r##"<svg width="800px" height="800px" viewBox="0 0 24 24" fill="none"><path d="M4 11.6L10 17.6L20 7.6" stroke="#000000" stroke-width="2.5" stroke-linecap="square" stroke-linejoin="round"/></svg>"##;

/// The flat DrawnUI look: crimson accent, gray outline.
const DEFAULT_ACCENT: Color = Color::from_rgb(0xDC, 0x14, 0x3C);
const DEFAULT_OUTLINE: Color = Color::from_rgb(0x8E, 0x95, 0x9D);

props!(CheckboxProps, CheckboxBuild, CheckboxSet {
    /// The check mark (the inner square of the default look). Unset = crimson for the default
    /// look, white for the others.
    color_check_on / set_color_check_on: Option<Color> = None, APPLY;
});

pub struct SkiaCheckbox {
    toggle: SkiaToggle,
    pub p: CheckboxProps,
    frame_off: Handle<SkiaShape>,
    frame_on: Handle<SkiaShape>,
    /// A `SkiaSvg` in the platform looks, a `SkiaShape` in the default one.
    check: ControlId,
}

impl SkiaCheckbox {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaCheckbox> {
        let checkbox = SkiaCheckbox {
            toggle: SkiaToggle::new(),
            p: CheckboxProps::default(),
            frame_off: Handle::default(),
            frame_on: Handle::default(),
            check: Handle::<SkiaShape>::default().id(),
        };
        let mut build = Build::new(checkbox);
        let id = build.id();
        build.control_mut().toggle.id = id;
        build
    }

    /// The style's frame colors where the app set none (DrawnUI.React StyleDefault).
    fn style_default(style: PrebuiltControlStyle, which: ToggleColor) -> Option<Color> {
        use PrebuiltControlStyle::*;
        use ToggleColor::*;
        let rgb = |r, g, b| Some(Color::from_rgb(r, g, b));
        match (style, which) {
            (Cupertino, FrameOff) => rgb(0xBF, 0xBF, 0xBF),
            (Cupertino, FrameOn) => rgb(0x00, 0x7A, 0xFF),
            (Material, FrameOff) => rgb(0x75, 0x75, 0x75),
            (Material, FrameOn) => rgb(0x21, 0x96, 0xF3),
            (Material3, FrameOff) => rgb(0x49, 0x45, 0x4F),
            (Material3, FrameOn) => rgb(0x67, 0x50, 0xA4),
            (Windows, FrameOff) => rgb(0x99, 0x99, 0x99),
            (Windows, FrameOn) => rgb(0x00, 0x78, 0xD7),
            (_, FrameOff) => Some(DEFAULT_OUTLINE),
            (_, FrameOn) => Some(DEFAULT_ACCENT),
            _ => None,
        }
    }

    fn color(&self, which: ToggleColor) -> Color {
        self.toggle.color(which, Self::style_default(self.toggle.using_control_style(), which))
    }

    /// The check mark's color as drawn.
    pub fn color_check_on(&self) -> Color {
        let default = if self.toggle.using_control_style() == PrebuiltControlStyle::Unset { DEFAULT_ACCENT } else { Color::WHITE };
        self.p.color_check_on.unwrap_or(default)
    }

    /// DrawnUI.React CreateDefaultContent: the frames and the mark of the style, and its size.
    fn build_content(&mut self, cx: &mut Cx) {
        let mut size = 22.0;
        let (mut off_handle, mut on_handle, mut check_id) = (Handle::default(), Handle::default(), self.check);
        self.toggle.rebuild(cx, |style| {
            use PrebuiltControlStyle::*;
            let mut off = SkiaShape::new().tag("FrameOff").shape_type(ShapeType::Rectangle).fill();
            let mut on = SkiaShape::new().tag("FrameOn").shape_type(ShapeType::Rectangle).fill();
            let mut svg = |markup: &str, margin: f32| -> Detached {
                let view = SkiaSvg::from_string(markup).tag("ViewCheckOn").tint_color(Color::WHITE).margin(margin).fill();
                check_id = view.id();
                view.into()
            };
            let check = match style {
                Cupertino => {
                    size = 22.0;
                    off = off.stroke_width(1.5).corner_radius(4.0);
                    on = on.corner_radius(4.0);
                    svg(SVG_CUPERTINO_CHECK, 2.0)
                }
                Material => {
                    size = 24.0;
                    off = off.stroke_width(2.0).corner_radius(2.0);
                    on = on.corner_radius(2.0);
                    svg(SVG_MATERIAL_CHECK, 2.0)
                }
                Material3 => {
                    size = 18.0;
                    off = off.stroke_width(2.0).corner_radius(2.0);
                    on = on.corner_radius(2.0);
                    svg(SVG_MATERIAL_CHECK, 2.0)
                }
                Windows => {
                    size = 20.0;
                    off = off.stroke_width(1.0);
                    svg(SVG_WINDOWS_CHECK, 1.0)
                }
                _ => {
                    off = off.stroke_width(1.0);
                    on = on.stroke_width(1.0);
                    let square = SkiaShape::new()
                        .tag("ViewCheckOn")
                        .shape_type(ShapeType::Rectangle)
                        .margin(3.0)
                        .lock_ratio(1.0)
                        .fill()
                        .use_cache(CacheType::Operations);
                    check_id = square.id();
                    square.into()
                }
            };
            let on = on.assign(&mut on_handle).children(check);
            vec![off.assign(&mut off_handle).into(), on.into()]
        });
        (self.frame_off, self.frame_on, self.check) = (off_handle, on_handle, check_id);
        self.toggle.pin_size(cx, size, size);
    }

    /// DrawnUI.React ApplyProperties: colors, then which frame shows.
    fn apply(&mut self, cx: &mut Cx) {
        let on = self.toggle.p.is_toggled;
        let (frame_on, frame_off, check) = (self.color(ToggleColor::FrameOn), self.color(ToggleColor::FrameOff), self.color_check_on());
        let flat = self.toggle.using_control_style() == PrebuiltControlStyle::Unset;
        if let Some(mut off) = cx.get_mut(self.frame_off) {
            off.set_stroke_color(frame_off);
            off.set_is_visible(!on);
        }
        if let Some(mut frame) = cx.get_mut(self.frame_on) {
            if flat {
                frame.set_stroke_color(frame_on);
            } else {
                frame.set_background_color(frame_on);
            }
            frame.set_is_visible(on);
        }
        if flat {
            if let Some(mut square) = cx.tree.find_mut::<SkiaShape>(self.check) {
                square.set_background_color(check);
            }
        } else if let Some(mut svg) = cx.tree.find_mut::<SkiaSvg>(self.check) {
            svg.set_tint_color(check);
        }
        if let Some(mut mark) = cx.any_mut(self.check) {
            mark.set_is_visible(on);
        }
    }
}

impl Has<CheckboxProps> for SkiaCheckbox {
    fn part(&self) -> &CheckboxProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut CheckboxProps {
        &mut self.p
    }
}

impl Has<ToggleProps> for SkiaCheckbox {
    fn part(&self) -> &ToggleProps {
        &self.toggle.p
    }
    fn part_mut(&mut self) -> &mut ToggleProps {
        &mut self.toggle.p
    }
}

impl Has<LayoutProps> for SkiaCheckbox {
    fn part(&self) -> &LayoutProps {
        Has::<LayoutProps>::part(&self.toggle)
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        Has::<LayoutProps>::part_mut(&mut self.toggle)
    }
}

impl Control for SkiaCheckbox {
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
        Some(Aria::CHECKBOX)
    }
}
