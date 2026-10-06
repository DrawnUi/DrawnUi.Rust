//! SkiaShaderCarousel (React SkiaShaderCarousel, DrawnUI SkiaShaderCarousel): a SkiaCarousel
//! whose slides are shown through a gl-transitions shader. The carousel moves as usual; a
//! `CarouselTransition` effect draws in place of its Image cache, blending the Image caches of the
//! slide the position comes from (`iImage1`) and the one it goes to (`iImage2`) by the fraction
//! between them (`progress`, 0..1). `transition_shader` is a file (or registered name) holding a
//! `vec4 transition(vec2 uv)`, wrapped by `effects::TRANSITION_TEMPLATE`. Slides must be cached as
//! Image: the effect samples their caches. While the shader is not ready the carousel shows its
//! slides as a plain carousel.

use std::any::Any;
use std::cell::Cell;

use skia_safe::{Image, Point, Rect};

use crate::control::{Control, Has, LayoutCx, PaintCx, part};
use crate::controls::carousel::{CarouselProps, CarouselSet as _, SkiaCarousel};
use crate::controls::layout::LayoutProps;
use crate::controls::snapping_layout::SnappingProps;
use crate::effects::{CachedTexture, SkiaEffect, SkiaShaderEffect};
use crate::props;
use crate::tree::{Build, Container, ControlId, Cx, Handle, Mut};
use crate::types::{CacheType, Dirty, LayoutOptions};

props!(ShaderCarouselProps, ShaderCarouselBuild, ShaderCarouselSet {
    /// Url or registered name of the transition file (React TransitionShader); it can change any time.
    transition_shader / set_transition_shader: String = String::new(), APPLY;
    /// Inline SkSL instead of a file (React TransitionShaderCode).
    transition_shader_code / set_transition_shader_code: String = String::new(), APPLY;
    /// Url or registered name of a template replacing the gl-transitions adapter (React TransitionTemplate).
    transition_template / set_transition_template: String = String::new(), APPLY;
});

/// The carousel: a SkiaCarousel, cached as Image, with the transition effect attached.
pub struct SkiaShaderCarousel {
    carousel: SkiaCarousel,
    pub p: ShaderCarouselProps,
    id: ControlId,
    /// The sources the effect was given.
    applied: (String, String, String),
    /// Past an end of a looped carousel the transition wraps to the other end (React wasWrapped).
    wrapped: Cell<bool>,
    /// The last transition: kept while a carousel that is not looped bounces past an end.
    last: Cell<Option<(usize, usize, f32)>>,
}

impl SkiaShaderCarousel {
    /// A horizontal carousel filling the width, clipped to its bounds, cached as Image.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaShaderCarousel> {
        let carousel = SkiaShaderCarousel {
            carousel: SkiaCarousel::default(),
            p: ShaderCarouselProps::default(),
            // Set right below, once the builder reserved one.
            id: Handle::<SkiaShaderCarousel>::default().id(),
            applied: Default::default(),
            wrapped: Cell::new(false),
            last: Cell::new(None),
        };
        let mut build = Build::new(carousel)
            .horizontal_options(LayoutOptions::Fill)
            .is_clipped_to_bounds(true)
            .use_cache(CacheType::Image)
            .visual_effect(CarouselTransition::default());
        build.control_mut().id = build.id();
        build
    }

    /// The carousel inside: position, snap points, selected slide.
    pub fn carousel(&self) -> &SkiaCarousel {
        &self.carousel
    }

    /// The slide the transition comes from (React TransitionFromIndex); `None` below two slides.
    pub fn transition_from_index(&self) -> Option<usize> {
        self.transition().map(|t| t.0)
    }

    /// The slide the transition goes to, 0 after the last one when looped (React TransitionToIndex).
    pub fn transition_to_index(&self) -> Option<usize> {
        self.transition().map(|t| t.1)
    }

    /// How far the transition is, 0..1.
    pub fn transition_progress(&self) -> Option<f32> {
        self.transition().map(|t| t.2)
    }

    /// From, to and progress for the position of the carousel (React OnScrollProgressChanged).
    fn transition(&self) -> Option<(usize, usize, f32)> {
        let c = &self.carousel;
        let snaps = c.snap_points();
        let count = c.children_count();
        if count < 2 || snaps.len() != count {
            return None;
        }
        let max = count - 1;
        let along = |p: Point| if c.p.is_vertical { p.y } else { p.x };
        let last = along(snaps[max]);
        // React ScrollProgress: 0 at the first slide, 1 at the last.
        let progress = if last == 0.0 { 0.0 } else { along(c.current_position()) / last };
        let mut scaled = progress * max as f32;
        if c.p.is_looped {
            if scaled < 0.0 || scaled > max as f32 {
                // Beyond the strip: a wrap when panning, when going to the slide at the other end
                // (a virtual anchor) or when a wrap is shown already; else the rubber band at an end.
                let selected = c.selected_index();
                let wrap = c.is_user_panning()
                    || self.wrapped.get()
                    || (scaled < 0.0 && selected == max)
                    || (scaled > max as f32 && selected == 0);
                self.wrapped.set(wrap);
                let slides = count as f32;
                scaled = if wrap { scaled.rem_euclid(slides) } else { scaled.clamp(0.0, max as f32) };
            } else {
                self.wrapped.set(false);
            }
            let from = (scaled.floor() as usize).min(max);
            let t = (from, (from + 1) % count, scaled - from as f32);
            self.last.set(Some(t));
            return Some(t);
        }
        // Not looped: a bounce past an end keeps the last transition.
        if !(0.0..=1.0).contains(&progress)
            && let Some(t) = self.last.get()
        {
            return Some(t);
        }
        let from = if progress > 0.0 { (scaled.floor() as usize).min(max) } else { 0 };
        let t = if from < max { (from, from + 1, scaled - from as f32) } else { (max - 1, max, 1.0) };
        self.last.set(Some(t));
        Some(t)
    }
}

impl Has<ShaderCarouselProps> for SkiaShaderCarousel {
    fn part(&self) -> &ShaderCarouselProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ShaderCarouselProps {
        &mut self.p
    }
}

impl Has<CarouselProps> for SkiaShaderCarousel {
    fn part(&self) -> &CarouselProps {
        &self.carousel.p
    }
    fn part_mut(&mut self) -> &mut CarouselProps {
        &mut self.carousel.p
    }
}

impl Has<SnappingProps> for SkiaShaderCarousel {
    fn part(&self) -> &SnappingProps {
        &self.carousel.sp
    }
    fn part_mut(&mut self) -> &mut SnappingProps {
        &mut self.carousel.sp
    }
}

impl Has<LayoutProps> for SkiaShaderCarousel {
    fn part(&self) -> &LayoutProps {
        Has::<LayoutProps>::part(&self.carousel)
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        Has::<LayoutProps>::part_mut(&mut self.carousel)
    }
}

impl Container for SkiaShaderCarousel {}

impl Control for SkiaShaderCarousel {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.carousel)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.carousel)
    }

    /// The transition sources go into the effect.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.carousel.on_props_changed(cx);
        let wanted = (self.p.transition_shader.clone(), self.p.transition_shader_code.clone(), self.p.transition_template.clone());
        if wanted == self.applied {
            return;
        }
        let Some(node) = cx.tree.node_mut(self.id) else { return };
        let effect = node.base.visual_effects.iter_mut().find_map(|e| (e.as_mut() as &mut dyn Any).downcast_mut::<CarouselTransition>());
        if let Some(effect) = effect {
            effect.shader.set_shader_source(wanted.0.clone());
            effect.shader.set_shader_code(wanted.1.clone());
            effect.shader.set_shader_template(wanted.2.clone());
        }
        self.applied = wanted;
        cx.tree.invalidate(self.id, Dirty::REPAINT);
    }

    /// A new layout starts the transition over (React MeasureAbsolute: initialized = false).
    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.last.set(None);
        self.carousel.arrange(cx);
    }
}

impl Mut<'_, SkiaShaderCarousel> {
    /// The next slide; from the last one to the first when looped (React GoNext).
    pub fn go_next(&mut self) {
        let c = &self.carousel;
        let (selected, max, looped) = (c.selected_index(), c.children_count().saturating_sub(1), c.p.is_looped);
        if selected < max {
            self.set_selected_index(selected + 1);
        } else if looped {
            self.set_selected_index(0);
        }
    }

    /// The slide before; from the first one to the last when looped (React GoPrev).
    pub fn go_prev(&mut self) {
        let c = &self.carousel;
        let (selected, max, looped) = (c.selected_index(), c.children_count().saturating_sub(1), c.p.is_looped);
        if selected > 0 {
            self.set_selected_index(selected - 1);
        } else if looped {
            self.set_selected_index(max);
        }
    }
}

/// The effect of a SkiaShaderCarousel (React ShaderTransitionEffect as its TransitionEffect):
/// `shader` is a `SkiaShaderEffect::transition()`, for extra uniforms through `Mut::effect_mut`.
pub struct CarouselTransition {
    pub shader: SkiaShaderEffect,
}

impl Default for CarouselTransition {
    fn default() -> Self {
        Self { shader: SkiaShaderEffect::transition() }
    }
}

impl SkiaEffect for CarouselTransition {
    fn gpu_lost(&mut self) {
        self.shader.gpu_lost();
    }

    fn is_post_renderer(&self) -> bool {
        true
    }

    fn render(&self, cx: &mut PaintCx<'_>, _cached: Option<&CachedTexture>) -> bool {
        let Some(node) = cx.node(cx.id) else { return false };
        let Some(carousel) = node.kind.as_deref().and_then(part::<SkiaShaderCarousel>) else { return false };
        let Some((from, to, progress)) = carousel.transition() else { return false };
        // Slides lie along the strip, where they were when their caches were recorded; their
        // textures are put where the carousel's content is, as an Image cache is recorded there.
        let shift = (cx.rect.left - node.base.rect.left, cx.rect.top - node.base.rect.top);
        let inner = crate::layout::content_rect(&node.base, cx.scale).with_offset(shift);
        let texture = |cx: &PaintCx<'_>, index: usize| -> Option<(Image, Rect)> {
            let slide = cx.node(*node.children.get(index)?)?;
            let cache = cx.render[slide.id.index as usize].cache.as_ref()?;
            let (image, m) = (cache.image()?.0, cache.margin());
            let (left, top) = ((inner.left - m.left).floor(), (inner.top - m.top).floor());
            Some((image.clone(), Rect::from_xywh(left, top, image.width() as f32, image.height() as f32)))
        };
        // No texture of the slide it comes from yet: the plain carousel shows.
        let Some((image, bounds)) = texture(cx, from) else { return false };
        let to = texture(cx, to).map(|t| t.0);
        self.shader.render_textures(cx, Some(CachedTexture { image, bounds }), to, &mut |u| u.set("progress", &[progress]))
    }

    fn shader_mut(&mut self) -> Option<&mut SkiaShaderEffect> {
        Some(&mut self.shader)
    }
}
