//! SkiaRadioButton: a circle indicator and a caption, exclusive inside its group: buttons that
//! share `group_name`, else the siblings under the same parent (DrawnUI RadioButtons). A tap on an
//! untoggled one toggles it and every other one of the group goes off; a tap on a toggled one does
//! nothing. Looks of DrawnUI.React (SkiaRadioButton.ts).

use skia_safe::Color;

use crate::control::{Control, GestureCx, Handled, Has, part};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::label::{LabelBuild, LabelSet, SkiaLabel};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::controls::shape::{ShapeBuild, ShapeSet, ShapeType, SkiaShape};
use crate::controls::toggle::{SkiaToggle, ToggleColor, ToggleProps};
use crate::gestures::{Gesture, GestureKind};
use crate::props;
use crate::tree::{Build, ControlId, Cx, Handle};
use crate::types::{CacheType, Dirty, LayoutOptions, Thickness};
use crate::ui::Aria;

/// The flat DrawnUI look: crimson accent, gray outline.
const DEFAULT_ACCENT: Color = Color::from_rgb(0xDC, 0x14, 0x3C);
const DEFAULT_OUTLINE: Color = Color::from_rgb(0x8E, 0x95, 0x9D);

props!(RadioProps, RadioBuild, RadioSet {
    /// The caption right of the circle.
    text / set_text: String = String::new(), APPLY;
    /// Buttons with the same name are exclusive anywhere in the tree; empty = the siblings under
    /// the same parent are.
    group_name / set_group_name: String = String::new(), NONE;
});

pub struct SkiaRadioButton {
    toggle: SkiaToggle,
    pub p: RadioProps,
    /// The outline shown in both states.
    off: Handle<SkiaShape>,
    /// The toggled indicator (DrawnUI ViewOn), tag "On".
    on: Handle<SkiaShape>,
    /// The inner dot; none in the Windows look.
    dot: Handle<SkiaShape>,
    /// The caption (DrawnUI ViewText), tag "Text".
    label: Handle<SkiaLabel>,
}

impl SkiaRadioButton {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(text: impl Into<String>) -> Build<SkiaRadioButton> {
        let radio = SkiaRadioButton {
            toggle: SkiaToggle::new(),
            p: RadioProps { text: text.into(), ..RadioProps::default() },
            off: Handle::default(),
            on: Handle::default(),
            dot: Handle::default(),
            label: Handle::default(),
        };
        let mut build = Build::new(radio).use_cache(CacheType::Image);
        let id = build.id();
        build.control_mut().toggle.id = id;
        build
    }

    /// The style's indicator colors where the app set none (DrawnUI.React StyleDefault).
    fn style_default(style: PrebuiltControlStyle, which: ToggleColor) -> Option<Color> {
        use PrebuiltControlStyle::*;
        use ToggleColor::*;
        let rgb = |r, g, b| Some(Color::from_rgb(r, g, b));
        match (style, which) {
            (Cupertino, ThumbOff) => rgb(0xBF, 0xBF, 0xBF),
            (Cupertino, ThumbOn) => rgb(0x00, 0x7A, 0xFF),
            (Material, ThumbOff) => rgb(0x75, 0x75, 0x75),
            (Material, ThumbOn) => rgb(0x21, 0x96, 0xF3),
            (Material3, ThumbOff) => rgb(0x49, 0x45, 0x4F),
            (Material3, ThumbOn) => rgb(0x67, 0x50, 0xA4),
            (Windows, ThumbOff) => rgb(0x76, 0x76, 0x76),
            (Windows, ThumbOn) => rgb(0x00, 0x78, 0xD7),
            (_, ThumbOff) => Some(DEFAULT_OUTLINE),
            (_, ThumbOn) => Some(DEFAULT_ACCENT),
            _ => None,
        }
    }

    fn color(&self, which: ToggleColor) -> Color {
        self.toggle.color(which, Self::style_default(self.toggle.using_control_style(), which))
    }

    /// DrawnUI.React CreateDefaultContent: a square box with the outline, the indicator and its
    /// dot, and the caption after it.
    fn build_content(&mut self, cx: &mut Cx) {
        let (mut off_handle, mut on_handle, mut dot_handle, mut label_handle) = Default::default();
        self.toggle.rebuild(cx, |style| {
            use PrebuiltControlStyle::*;
            let flat = style == Unset;
            let circle = || SkiaShape::new().shape_type(ShapeType::Circle).fill();
            let mut off = circle();
            let mut on = circle().tag("On");
            // The dot's margin; the Windows look has none, its ring is a thick stroke.
            let mut dot = None;
            match style {
                Cupertino => {
                    off = off.stroke_width(1.5);
                    dot = Some(6.5);
                }
                Windows => {
                    off = off.stroke_width(1.0);
                    on = on.stroke_width(5.0);
                }
                Material | Material3 => {
                    off = off.stroke_width(2.0);
                    on = on.stroke_width(2.0);
                    dot = Some(5.0);
                }
                _ => {
                    off = off.stroke_width(2.0);
                    on = on.stroke_width(2.0);
                    dot = Some(4.0);
                }
            }
            let dot = dot.map(|margin| {
                let dot = circle().margin(margin).assign(&mut dot_handle);
                if style == Cupertino { dot.background_color(Color::WHITE) } else { dot }
            });
            let indicator = SkiaLayout::new()
                .height_request(if flat { 18.0 } else { 20.0 })
                .lock_ratio(1.0)
                .vertical_options(LayoutOptions::Center)
                .children((off.assign(&mut off_handle), on.assign(&mut on_handle).children(dot)));
            let label = SkiaLabel::new("")
                .tag("Text")
                .margin(Thickness::new(if flat { 26.0 } else { 28.0 }, 0.0, 0.0, 0.0))
                .font_size(14.0)
                .max_lines(2)
                .text_color(Color::BLACK)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .assign(&mut label_handle);
            vec![indicator.into(), label.into()]
        });
        (self.off, self.on, self.dot, self.label) = (off_handle, on_handle, dot_handle, label_handle);
        self.toggle.pin_minimum_height(cx, 24.0);
    }

    /// DrawnUI.React ApplyProperties: colors, the indicator, the caption.
    fn apply(&mut self, cx: &mut Cx) {
        let (on, off_color, on_color) = (self.toggle.p.is_toggled, self.color(ToggleColor::ThumbOff), self.color(ToggleColor::ThumbOn));
        let cupertino = self.toggle.using_control_style() == PrebuiltControlStyle::Cupertino;
        if let Some(mut off) = cx.get_mut(self.off) {
            off.set_stroke_color(off_color);
        }
        if let Some(mut indicator) = cx.get_mut(self.on) {
            if cupertino {
                indicator.set_background_color(on_color);
            } else {
                indicator.set_stroke_color(on_color);
            }
            indicator.set_is_visible(on);
        }
        if !cupertino && let Some(mut dot) = cx.get_mut(self.dot) {
            dot.set_background_color(on_color);
        }
        if let Some(mut label) = cx.get_mut(self.label) {
            label.set_text(self.p.text.clone());
        }
    }

    /// The other buttons of the group that are on (DrawnUI RadioButtons.All).
    // ponytail: scans the arena on every toggle-on; a registry by group when groups get large.
    fn others_on(&self, cx: &Cx) -> Vec<ControlId> {
        let me = self.toggle.id;
        let is_on = |id: ControlId, same: &dyn Fn(&SkiaRadioButton) -> bool| {
            let node = cx.tree.node(id)?;
            let radio = part::<SkiaRadioButton>(node.kind.as_deref()?)?;
            (id != me && radio.toggle.p.is_toggled && same(radio)).then_some(id)
        };
        if self.p.group_name.is_empty() {
            let Some(parent) = cx.tree.parent(me) else { return Vec::new() };
            cx.tree.children(parent).iter().filter_map(|c| is_on(*c, &|r| r.p.group_name.is_empty())).collect()
        } else {
            let name = self.p.group_name.as_str();
            cx.tree.nodes.iter().flatten().filter_map(|n| is_on(n.id, &|r| r.p.group_name == name)).collect()
        }
    }
}

impl Has<RadioProps> for SkiaRadioButton {
    fn part(&self) -> &RadioProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut RadioProps {
        &mut self.p
    }
}

impl Has<ToggleProps> for SkiaRadioButton {
    fn part(&self) -> &ToggleProps {
        &self.toggle.p
    }
    fn part_mut(&mut self) -> &mut ToggleProps {
        &mut self.toggle.p
    }
}

impl Has<LayoutProps> for SkiaRadioButton {
    fn part(&self) -> &LayoutProps {
        Has::<LayoutProps>::part(&self.toggle)
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        Has::<LayoutProps>::part_mut(&mut self.toggle)
    }
}

impl Control for SkiaRadioButton {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.toggle)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.toggle)
    }

    /// Builds the look for the style, pushes the state and the colors into it; a button that
    /// came on turns the others of its group off.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if self.toggle.needs_content() {
            self.build_content(cx);
        }
        let came_on = self.toggle.p.is_toggled && self.toggle.reported() != Some(true);
        self.toggle.take_change(cx);
        if came_on {
            for other in self.others_on(cx) {
                if let Some(mut radio) = cx.tree.find_mut::<SkiaRadioButton>(other) {
                    radio.control_mut().toggle.p.is_toggled = false;
                    radio.mark(Dirty::APPLY);
                }
            }
        }
        self.apply(cx);
    }

    /// Only an untoggled button takes a tap: it comes on.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match gesture.kind {
            GestureKind::Tapped if !self.toggle.p.is_toggled && self.toggle.p.responds_to_gestures => {
                self.toggle.p.is_toggled = true;
                cx.invalidate(Dirty::APPLY);
                Handled::Tapped
            }
            _ => Handled::No,
        }
    }

    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        (!self.p.text.is_empty()).then(|| self.p.text.as_str().into())
    }

    /// React DefaultAccessibilityRole.
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(Aria::RADIO)
    }
}
