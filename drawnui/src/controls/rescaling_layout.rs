//! RescalingLayout: fits a fixed logical viewport (`logical_width` x `logical_height` points) into
//! the space it gets by changing the rendering scale of its children, not by a scale transform
//! (React `RescalingLayout.ts` of the Pong page, C# Pong.Shared RescalingCanvas, Blazor
//! AspectLockedCanvas). The children are measured, arranged and drawn at the fitted scale, so
//! their text, strokes and caches are sharp at it, and hit testing needs nothing special.

use skia_safe::Size;

use crate::control::{Control, Has, LayoutCx, PaintCx};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::props;
use crate::tree::{Build, Container};
use crate::types::LayoutOptions;

props!(RescalingProps, RescalingBuild, RescalingSet {
    /// Width of the logical viewport, points; 0 = no rescaling.
    logical_width / set_logical_width: f32 = 0.0, MEASURE;
    /// Height of the logical viewport, points; 0 = no rescaling.
    logical_height / set_logical_height: f32 = 0.0, MEASURE;
});

/// A layout whose children get the scale that fits its logical viewport into its box.
pub struct RescalingLayout {
    layout: SkiaLayout,
    pub p: RescalingProps,
    /// Pixels per point of the children (React `ContextScale`).
    context_scale: f32,
}

impl RescalingLayout {
    /// Fits `width` x `height` points into the box it fills.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(width: f32, height: f32) -> Build<RescalingLayout> {
        let layout = RescalingLayout { layout: SkiaLayout::default(), p: RescalingProps::default(), context_scale: 1.0 };
        Build::new(layout)
            .logical_width(width)
            .logical_height(height)
            .horizontal_options(LayoutOptions::Fill)
            .vertical_options(LayoutOptions::Fill)
    }

    /// The scale the children are measured, arranged and drawn at, pixels per point.
    pub fn context_scale(&self) -> f32 {
        self.context_scale
    }

    /// A layout context of the children's scale.
    fn scaled<'a>(&self, cx: &'a mut LayoutCx) -> LayoutCx<'a> {
        LayoutCx { tree: &mut *cx.tree, fonts: cx.fonts, state: cx.state, id: cx.id, scale: self.context_scale }
    }
}

impl Has<RescalingProps> for RescalingLayout {
    fn part(&self) -> &RescalingProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut RescalingProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for RescalingLayout {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for RescalingLayout {}

impl Control for RescalingLayout {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// The fitted scale is the smaller of width / logical width and height / logical height (pixels
    /// per logical point); the layout takes its whole box. Without a logical size, or in an
    /// unbounded box, the children keep the scale of the tree.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let (lw, lh) = (self.p.logical_width, self.p.logical_height);
        if lw <= 0.0 || lh <= 0.0 || !width.is_finite() || !height.is_finite() {
            self.context_scale = cx.scale;
            return self.layout.measure(cx, width, height);
        }
        self.context_scale = (width / lw).min(height / lh);
        let mut scaled = self.scaled(cx);
        self.layout.measure(&mut scaled, width, height);
        Size::new(width, height)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        let mut scaled = self.scaled(cx);
        self.layout.arrange(&mut scaled);
    }

    fn paint(&self, cx: &mut PaintCx) {
        let own = std::mem::replace(&mut cx.scale, self.context_scale);
        cx.paint_children();
        cx.scale = own;
    }
}
