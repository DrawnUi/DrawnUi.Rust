//! The plate every button is made of: a SkiaShape whose line is quiet at rest and takes the main
//! color, with a glow, under the pointer or a press. A custom control, because entering and leaving
//! come to a control's own `on_gesture`, not to an app handler.

use drawnui::controls::layout::LayoutProps;
use drawnui::controls::shape::ShapeProps;
use drawnui::prelude::*;

pub struct Plate {
    shape: SkiaShape,
    rest: Color,
    lit: Color,
}

impl Plate {
    /// A plate stroked with `rest`, and with `lit` while hovered or pressed.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rest: Color, lit: Color) -> Build<Plate> {
        Build::new(Plate { shape: SkiaShape::default(), rest, lit }).use_cache(CacheType::Operations).stroke_color(rest)
    }

    fn light(&mut self, cx: &mut GestureCx, on: bool) {
        let color = if on { self.lit } else { self.rest };
        if self.shape.p.stroke_color == color {
            return;
        }
        self.shape.p.stroke_color = color;
        self.shape.p.shadows = if on { vec![SkiaShadow::new(self.lit).x(0).y(0).blur(12).opacity(0.85)] } else { Vec::new() };
        cx.invalidate(Dirty::DRAW_APPLY);
    }
}

impl Has<ShapeProps> for Plate {
    fn part(&self) -> &ShapeProps {
        &self.shape.p
    }
    fn part_mut(&mut self) -> &mut ShapeProps {
        &mut self.shape.p
    }
}

impl Has<LayoutProps> for Plate {
    fn part(&self) -> &LayoutProps {
        Has::<LayoutProps>::part(&self.shape)
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        Has::<LayoutProps>::part_mut(&mut self.shape)
    }
}

impl Container for Plate {}

impl Control for Plate {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.shape)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.shape)
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match gesture.kind {
            GestureKind::PointerEnter | GestureKind::Down => self.light(cx, true),
            GestureKind::PointerExit => self.light(cx, false),
            // A finger leaves; a mouse is still over the button.
            GestureKind::Up if gesture.touch => self.light(cx, false),
            _ => {}
        }
        self.shape.on_gesture(cx, gesture)
    }
}
