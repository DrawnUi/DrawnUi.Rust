//! Feature `designer`: the properties of a control by name, for tools (DrawnUI Designer). Every
//! properties group made by `props!` (and the common `ControlProps`) lists its properties with their
//! values as text and takes new values as text:
//!
//! - numbers as Rust writes them (`1.5`, `-2`), `true` / `false`, strings as they are;
//! - a choice by its case name (`Center`, `AspectFill`), any letter case;
//! - a color as `#RRGGBB` or `#AARRGGBB`;
//! - a `Thickness` as `l,t,r,b`, `h,v` or one number; a `CornerRadius` as four numbers or one;
//! - an `Option` as `none` or its value.
//!
//! Other types (gradients, shadows, lists) are listed with their debug text and cannot be set.

use std::borrow::Cow;

use crate::controls::button::{ButtonProps, SkiaButton, SkiaTouchAnimation};
use crate::controls::carousel::{CarouselProps, SkiaCarousel};
use crate::controls::checkbox::{CheckboxProps, SkiaCheckbox};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::animated_frames::FramesProps;
use crate::controls::game::DrawnGame;
use crate::controls::drawer::{DrawerDirection, DrawerProps, SkiaDrawer};
use crate::controls::editor::{EditorProps, KeyboardType, ReturnType, SkiaEditor};
use crate::controls::gif::{GifProps, SkiaGif};
use crate::controls::image::{DrawImageAlignment, FilterQuality, ImageProps, SkiaImage, SkiaImageEffect, SkiaImageTiles, TilesProps, TransformAspect};
use crate::controls::label::{FontAttributes, LabelProps, LineBreakMode, SkiaLabel, TextAlignment, TextTransform};
use crate::controls::label_fps::SkiaLabelFps;
use crate::controls::layout::decorated_grid::{DecoratedGridProps, SkiaDecoratedGrid};
use crate::controls::layout::{LayoutProps, LayoutType, MeasuringStrategy, RecyclingTemplate, SkiaLayout};
use crate::controls::lottie::{LottieProps, SkiaLottie};
use crate::controls::progress::{ProgressProps, SkiaProgress};
use crate::controls::radio_button::{RadioProps, SkiaRadioButton};
use crate::controls::rescaling_layout::{RescalingLayout, RescalingProps};
use crate::controls::rich_label::{RichLabelProps, SkiaRichLabel};
use crate::controls::scroll::{RelativePositionType, ScrollOrientation, ScrollProps, SkiaScroll, SnapToChildrenType};
use crate::controls::scroll_bar::{ScrollBarDock, ScrollBarProps, SkiaScrollBar};
use crate::controls::shader_carousel::{ShaderCarouselProps, SkiaShaderCarousel};
use crate::controls::shape::{BevelType, ShapeProps, ShapeType, SkiaShape};
use crate::controls::shell::{ShellProps, SkiaShell};
use crate::controls::slider::{SkiaSlider, SliderProps};
use crate::controls::snapping_layout::SnappingProps;
use crate::controls::sprite::{SkiaSprite, SkiaSpriteSet, SpriteProps, SpriteSetProps};
use crate::controls::svg::{SkiaSvg, SvgProps};
use crate::controls::switch::SkiaSwitch;
use crate::controls::toggle::{SkiaToggle, ToggleProps};
use crate::types::{CacheType, CornerRadius, LayoutOptions, LockTouch, Thickness};
use crate::skia::Color;
use crate::{Control, Dirty, Has};

/// One property of a control (`Tree::props`).
#[derive(Clone, Debug, PartialEq)]
pub struct PropInfo {
    pub name: &'static str,
    /// The Rust type as written in the properties table (`f32`, `Option<Color>`).
    pub type_name: &'static str,
    /// The value as text (see the module docs).
    pub value: String,
    /// Whether it can be set from text.
    pub settable: bool,
    /// The value differs from the default.
    pub is_set: bool,
    /// For a choice (an enum): its case names, what `set_prop` takes (an `Option` also takes
    /// `none`). `None` for other types.
    pub choices: Option<&'static [&'static str]>,
}

/// A properties group that lists and sets its properties by name.
pub trait PropTable {
    fn props(&self) -> Vec<PropInfo>;
    /// `None`: no property of that name here. `Some(Err)`: the text is not a value of its type.
    /// `Some(Ok(dirty))`: set; what the change invalidates.
    fn set_prop(&mut self, name: &str, text: &str) -> Option<Result<Dirty, String>>;
}

/// A property type as text.
pub trait PropText: Sized + std::fmt::Debug {
    fn to_text(&self) -> String {
        format!("{self:?}")
    }
    /// `None` when the text is not a value, or the type cannot be set from text.
    fn from_text(_text: &str) -> Option<Self> {
        None
    }
    fn settable() -> bool {
        false
    }
    /// The case names of a choice.
    fn choices() -> Option<&'static [&'static str]> {
        None
    }
}

macro_rules! number_text {
    ($($t:ty),*) => {$(
        impl PropText for $t {
            fn to_text(&self) -> String { self.to_string() }
            fn from_text(text: &str) -> Option<Self> { text.trim().parse().ok() }
            fn settable() -> bool { true }
        }
    )*};
}
number_text!(f32, f64, i32, u32, usize, bool, char);

impl PropText for String {
    fn to_text(&self) -> String {
        self.clone()
    }
    fn from_text(text: &str) -> Option<Self> {
        Some(text.to_owned())
    }
    fn settable() -> bool {
        true
    }
}

impl PropText for Cow<'static, str> {
    fn to_text(&self) -> String {
        self.to_string()
    }
    fn from_text(text: &str) -> Option<Self> {
        Some(Cow::Owned(text.to_owned()))
    }
    fn settable() -> bool {
        true
    }
}

impl PropText for Color {
    fn to_text(&self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.a(), self.r(), self.g(), self.b())
    }
    fn from_text(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        let v = u32::from_str_radix(hex, 16).ok()?;
        match hex.len() {
            6 => Some(Color::new(0xFF00_0000 | v)),
            8 => Some(Color::new(v)),
            _ => None,
        }
    }
    fn settable() -> bool {
        true
    }
}

fn numbers(text: &str) -> Option<Vec<f32>> {
    text.split(',').map(|n| n.trim().parse().ok()).collect()
}

impl PropText for Thickness {
    fn to_text(&self) -> String {
        format!("{},{},{},{}", self.left, self.top, self.right, self.bottom)
    }
    fn from_text(text: &str) -> Option<Self> {
        match numbers(text)?[..] {
            [a] => Some(Thickness::new(a, a, a, a)),
            [h, v] => Some(Thickness::new(h, v, h, v)),
            [l, t, r, b] => Some(Thickness::new(l, t, r, b)),
            _ => None,
        }
    }
    fn settable() -> bool {
        true
    }
}

impl PropText for CornerRadius {
    fn to_text(&self) -> String {
        format!("{},{},{},{}", self.top_left, self.top_right, self.bottom_left, self.bottom_right)
    }
    fn from_text(text: &str) -> Option<Self> {
        let (tl, tr, bl, br) = match numbers(text)?[..] {
            [a] => (a, a, a, a),
            [tl, tr, bl, br] => (tl, tr, bl, br),
            _ => return None,
        };
        Some(CornerRadius { top_left: tl, top_right: tr, bottom_left: bl, bottom_right: br })
    }
    fn settable() -> bool {
        true
    }
}

impl<T: PropText> PropText for Option<T> {
    fn to_text(&self) -> String {
        self.as_ref().map_or_else(|| "none".to_owned(), PropText::to_text)
    }
    fn from_text(text: &str) -> Option<Self> {
        if text.trim().eq_ignore_ascii_case("none") { Some(None) } else { T::from_text(text).map(Some) }
    }
    fn settable() -> bool {
        T::settable()
    }
    fn choices() -> Option<&'static [&'static str]> {
        T::choices()
    }
}

impl<T: PropText> PropText for Box<T> {
    fn to_text(&self) -> String {
        (**self).to_text()
    }
    fn from_text(text: &str) -> Option<Self> {
        T::from_text(text).map(Box::new)
    }
    fn settable() -> bool {
        T::settable()
    }
    fn choices() -> Option<&'static [&'static str]> {
        T::choices()
    }
}

/// A choice set by its case name.
macro_rules! choice_text {
    ($($t:ty: [$($v:ident),*];)*) => {$(
        impl PropText for $t {
            fn from_text(text: &str) -> Option<Self> {
                let text = text.trim();
                [$(<$t>::$v),*].into_iter().find(|v| format!("{v:?}").eq_ignore_ascii_case(text))
            }
            fn settable() -> bool { true }
            fn choices() -> Option<&'static [&'static str]> { Some(&[$(stringify!($v)),*]) }
        }
    )*};
}
choice_text! {
    LayoutOptions: [Start, Center, End, Fill];
    TextAlignment: [Start, Center, End];
    DrawImageAlignment: [Start, Center, End];
    TransformAspect: [None, Fill, Fit, AspectFit, AspectFill, AspectFitFill, FitFill, Cover, AspectCover, Tile];
    PrebuiltControlStyle: [Unset, Platform, Cupertino, Material, Material3, Windows];
    SkiaTouchAnimation: [None, Ripple];
    TextTransform: [None, Lowercase, Uppercase, Titlecase];
    SnapToChildrenType: [Disabled, Side, Center];
    SkiaImageEffect: [None, BlackAndWhite, Pastel, Tint, Darken, Lighten, Grayscale, Sepia, InvertColors, Contrast, Saturation, Brightness, Gamma, TSL, HSL, Custom];
    ShapeType: [Rectangle, Circle, Ellipse, Arc, Path, Polygon, Line];
    ScrollOrientation: [Vertical, Horizontal, Both, Neither];
    ScrollBarDock: [End, Start];
    ReturnType: [Default, Done, Go, Next, Search, Send];
    RelativePositionType: [None, Start, Center, End];
    RecyclingTemplate: [Enabled, Disabled];
    MeasuringStrategy: [MeasureAll, MeasureFirst, MeasureVisible];
    LockTouch: [Disabled, Enabled, PassNone, PassTap, PassTapAndLongPress];
    LineBreakMode: [NoWrap, WordWrap, CharacterWrap, HeadTruncation, TailTruncation, MiddleTruncation];
    LayoutType: [Absolute, Column, Row, Wrap, Grid, Masonry];
    KeyboardType: [Default, Numeric, Decimal, Phone, Email];
    FontAttributes: [None, Bold, Italic, BoldItalic];
    FilterQuality: [None, Low, Medium, High, Ultra];
    DrawerDirection: [FromBottom, FromTop, FromLeft, FromRight];
    CacheType: [None, Operations, OperationsFull, Image, GPU, ImageDoubleBuffered, ImageComposite, ImageCompositeGPU];
    BevelType: [None, Bevel, Emboss];
}

/// Listed with their debug text, not settable from text.
macro_rules! listed_only {
    ($($t:ty),*) => {$( impl PropText for $t {} )*};
}
listed_only!(
    crate::skia::BlendMode,
    crate::skia::PaintCap,
    crate::controls::layout::MeasureBudget,
    crate::controls::layout::GridLength,
    crate::SkiaGradient,
    crate::SkiaShadow,
    crate::controls::shape::SkiaBevel,
    crate::skia::Point,
    crate::controls::image::AppFilter<crate::skia::ImageFilter>,
    crate::controls::image::AppFilter<crate::skia::ColorFilter>
);
impl<T: std::fmt::Debug> PropText for Vec<T> {}

/// Every control type and its properties groups (the `Has<...>` it implements).
macro_rules! controls {
    ($m:ident) => {
        $m! {
            SkiaLabel => [LabelProps];
            SkiaLabelFps => [LabelProps];
            SkiaRichLabel => [LabelProps, RichLabelProps];
            SkiaLayout => [LayoutProps];
            DrawnGame => [LayoutProps];
            SkiaShape => [LayoutProps, ShapeProps];
            SkiaButton => [LayoutProps, ButtonProps];
            SkiaImage => [ImageProps];
            SkiaImageTiles => [ImageProps, TilesProps];
            SkiaSvg => [SvgProps];
            SkiaScroll => [ScrollProps];
            SkiaScrollBar => [ScrollBarProps];
            SkiaEditor => [LayoutProps, EditorProps];
            SkiaCarousel => [LayoutProps, SnappingProps, CarouselProps];
            SkiaShaderCarousel => [LayoutProps, SnappingProps, CarouselProps, ShaderCarouselProps];
            SkiaDrawer => [LayoutProps, SnappingProps, DrawerProps];
            SkiaDecoratedGrid => [LayoutProps, DecoratedGridProps];
            RescalingLayout => [LayoutProps, RescalingProps];
            SkiaCheckbox => [LayoutProps, ToggleProps, CheckboxProps];
            SkiaRadioButton => [LayoutProps, ToggleProps, RadioProps];
            SkiaSwitch => [LayoutProps, ToggleProps];
            SkiaToggle => [LayoutProps, ToggleProps];
            SkiaSlider => [SliderProps];
            SkiaProgress => [ProgressProps];
            SkiaLottie => [LottieProps];
            SkiaGif => [FramesProps, GifProps];
            SkiaSprite => [SpriteProps];
            SkiaSpriteSet => [LayoutProps, SpriteSetProps];
            SkiaShell => [ShellProps];
        }
    };
}

/// The properties of a control's own type, group by group (the common ones are not included).
pub(crate) fn own_props(control: &dyn Control) -> Vec<PropInfo> {
    let any = control as &dyn std::any::Any;
    macro_rules! parts {
        ($($control:ty => [$($part:ty),*];)*) => {$(
            if let Some(c) = any.downcast_ref::<$control>() {
                let mut out = Vec::new();
                $(out.extend(Has::<$part>::part(c).props());)*
                return out;
            }
        )*};
    }
    controls!(parts);
    Vec::new()
}

/// Sets a property of a control's own type by name (see `PropTable::set_prop`).
pub(crate) fn set_own_prop(control: &mut dyn Control, name: &str, text: &str) -> Option<Result<Dirty, String>> {
    let any = control as &mut dyn std::any::Any;
    macro_rules! parts {
        ($($control:ty => [$($part:ty),*];)*) => {$(
            if let Some(c) = any.downcast_mut::<$control>() {
                $(if let Some(done) = Has::<$part>::part_mut(c).set_prop(name, text) {
                    return Some(done);
                })*
                return None;
            }
        )*};
    }
    controls!(parts);
    None
}

impl crate::Tree {
    /// Every property of a control by name: the common ones (`ControlProps`), then those of its
    /// type. Empty for a control that is not there.
    pub fn props(&self, id: impl Into<crate::ControlId>) -> Vec<PropInfo> {
        let Some(node) = self.node(id.into()) else { return Vec::new() };
        let mut out = node.base.p.props();
        if let Some(kind) = node.kind.as_deref() {
            out.extend(own_props(kind));
        }
        out
    }

    /// A control the engine made for itself, not the app: a button's caption, an editor's text, a
    /// scroll bar, a toggle's look, a shell's layers, bars and title. `false` for a control that is
    /// gone. A wrapper the engine puts around the app's content is internal; the content is not.
    pub fn is_internal(&self, id: impl Into<crate::ControlId>) -> bool {
        let id = id.into();
        let Some(node) = self.node(id) else { return false };
        // A toggle's look is a part, with all it holds.
        node.internal || std::iter::successors(Some(id), |c| self.parent(*c)).any(|c| self.node(c).is_some_and(|n| n.part))
    }

    /// The popup windows open, oldest first: each one's id and the root of its content (a part of
    /// this tree, not under `root`).
    pub fn popup_roots(&self) -> Vec<(crate::PopupId, crate::ControlId)> {
        self.popups.iter().map(|p| (crate::popups::PopupId(p.id), p.root)).collect()
    }

    /// Sets a property of a control by name from text (see the module docs) and invalidates what
    /// it changes, as its setter does.
    pub fn set_prop(&mut self, id: impl Into<crate::ControlId>, name: &str, text: &str) -> Result<(), String> {
        let id = id.into();
        let node = self.node_mut(id).ok_or_else(|| "no such control".to_owned())?;
        let done = match node.base.p.set_prop(name, text) {
            Some(done) => done,
            None => node.kind.as_deref_mut().and_then(|kind| set_own_prop(kind, name, text)).ok_or_else(|| format!("no property {name}"))?,
        };
        self.invalidate(id, done?);
        Ok(())
    }
}
