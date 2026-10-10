//! Value types with the DrawnUI names. Units are points unless a name says pixels.

use skia_safe::{BlendMode, Color, Point, TileMode};

/// Where a control sits inside the box its parent gives it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LayoutOptions {
    #[default]
    Start,
    Center,
    End,
    Fill,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Thickness {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Thickness {
    pub const ZERO: Thickness = Thickness { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
    pub const fn uniform(all: f32) -> Self {
        Self::new(all, all, all, all)
    }
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
    /// The larger of the two on every side.
    pub fn max(self, other: Thickness) -> Thickness {
        let (a, b) = (self, other);
        Thickness::new(a.left.max(b.left), a.top.max(b.top), a.right.max(b.right), a.bottom.max(b.bottom))
    }
}

/// Where the canvas draws (DrawnUI `Canvas.RenderingMode`; Accelerated by default as in
/// DrawnUi.React, C# defaults to Default).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RenderingModeType {
    /// The CPU: each frame is drawn into memory, then shown. Caches are CPU bitmaps. A browser
    /// that refuses WebGL2 draws this way whatever was asked.
    Default,
    /// The GPU: WebGL2, OpenGL or Metal.
    #[default]
    Accelerated,
}

/// What the canvas does with gestures the page could take too (DrawnUI `Canvas.Gestures`). The
/// browser host acts on it; the native hosts own their window and ignore it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GesturesMode {
    /// The canvas takes its gestures; the page keeps what the app does not use (the wheel) and its
    /// own CSS decides touch panning.
    #[default]
    Enabled,
    /// The canvas owns every touch: no page scroll, bounce, pull-down or text selection starts on
    /// it (DrawnUi.Web `applyGestureStyle` with lock). For full-screen apps and games.
    Lock,
}

/// The GPU API the canvas draws with (DrawnUi.Rust; DrawnUI picks per platform itself).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GpuBackend {
    /// Vulkan on Android, OpenGL ES where Vulkan cannot be made; OpenGL on Windows and Linux,
    /// Metal on Apple platforms, WebGL2 in the browser.
    #[default]
    Auto,
    /// OpenGL (ES on Android) where the platform has a choice.
    OpenGl,
    /// Vulkan where the platform has it (Android), else as `Auto`.
    Vulkan,
}

/// What a control keeps between frames instead of painting again (DrawnUI SkiaCacheType).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[allow(clippy::upper_case_acronyms)]
pub enum CacheType {
    #[default]
    None,
    /// Recorded draw commands, replayed each frame.
    Operations,
    /// Recorded draw commands over the whole area the canvas shows (its clip), not only the
    /// control's rect: for a control that paints outside its rect. Recorded again when the size
    /// of that area changes; a cache that must move inside a scroll belongs on the scrolled parent.
    OperationsFull,
    /// Offscreen image on the window's GPU context, blitted each frame (DrawnUI's GPU cache; its
    /// Image cache is a CPU bitmap).
    Image,
    /// DrawnUI GPU: the same as `Image`, which is on the GPU already.
    GPU,
    /// A CPU bitmap made off the frame thread: the last one is drawn while the next one is made
    /// (the desktop; the browser makes it in the frame, as `Image`).
    ImageDoubleBuffered,
    /// An Image cache whose surface is kept: when only some children changed, they and the
    /// siblings they overlap are drawn again into it, the rest stays.
    ImageComposite,
    /// DrawnUI ImageCompositeGPU: the same as `ImageComposite`, which is on the GPU already.
    ImageCompositeGPU,
}

impl CacheType {
    /// The cache a control gets: the GPU names are the caches that are on the GPU already.
    pub(crate) fn resolved(self) -> Self {
        match self {
            CacheType::GPU => CacheType::Image,
            CacheType::ImageCompositeGPU => CacheType::ImageComposite,
            other => other,
        }
    }
}

/// Which gestures a control lets through to its children.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LockTouch {
    #[default]
    Disabled,
    Enabled,
    PassNone,
    PassTap,
    PassTapAndLongPress,
}

/// What a property change invalidates.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Dirty(pub(crate) u8);

impl Dirty {
    pub const NONE: Dirty = Dirty(0);
    /// Size may change: the control and its ancestors measure again, caches are dropped.
    pub const MEASURE: Dirty = Dirty(1);
    /// Own pixels change: own cache and ancestor caches are recorded again.
    pub const DRAW: Dirty = Dirty(2);
    /// Position, transform or opacity: own cache stays, ancestors composite again.
    pub const REPAINT: Dirty = Dirty(4);
    /// The control's `on_props_changed` runs before the next layout.
    pub const APPLY: Dirty = Dirty(8);
    pub const MEASURE_APPLY: Dirty = Dirty(1 | 8);
    pub const DRAW_APPLY: Dirty = Dirty(2 | 8);

    pub fn contains(self, other: Dirty) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Dirty) {
        self.0 |= rhs.0
    }
}

/// Conversion accepted by property builders and setters, so `16`, `16.0` and `"text"` all work.
pub trait IntoProp<T> {
    fn into_prop(self) -> T;
}

impl<T> IntoProp<T> for T {
    fn into_prop(self) -> T {
        self
    }
}
impl IntoProp<f32> for i32 {
    fn into_prop(self) -> f32 {
        self as f32
    }
}
impl IntoProp<f32> for f64 {
    fn into_prop(self) -> f32 {
        self as f32
    }
}
impl IntoProp<String> for &str {
    fn into_prop(self) -> String {
        self.to_owned()
    }
}
/// A text that is a constant most of the time (an `Aria` role) and comes from data the rest:
/// a constant costs nothing, a `String` is kept as it is.
impl IntoProp<std::borrow::Cow<'static, str>> for &'static str {
    fn into_prop(self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(self)
    }
}
impl IntoProp<std::borrow::Cow<'static, str>> for String {
    fn into_prop(self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Owned(self)
    }
}
impl IntoProp<Option<Color>> for Color {
    fn into_prop(self) -> Option<Color> {
        Some(self)
    }
}
impl IntoProp<Option<char>> for char {
    fn into_prop(self) -> Option<char> {
        Some(self)
    }
}
impl IntoProp<Option<bool>> for bool {
    fn into_prop(self) -> Option<bool> {
        Some(self)
    }
}
impl IntoProp<Thickness> for f32 {
    fn into_prop(self) -> Thickness {
        Thickness::uniform(self)
    }
}
impl IntoProp<Thickness> for i32 {
    fn into_prop(self) -> Thickness {
        Thickness::uniform(self as f32)
    }
}
impl IntoProp<Thickness> for (f32, f32) {
    /// (horizontal, vertical)
    fn into_prop(self) -> Thickness {
        Thickness::new(self.0, self.1, self.0, self.1)
    }
}
impl IntoProp<Thickness> for (i32, i32) {
    fn into_prop(self) -> Thickness {
        (self.0 as f32, self.1 as f32).into_prop()
    }
}
impl IntoProp<Thickness> for (f32, f32, f32, f32) {
    /// (left, top, right, bottom)
    fn into_prop(self) -> Thickness {
        Thickness::new(self.0, self.1, self.2, self.3)
    }
}
impl IntoProp<Thickness> for (i32, i32, i32, i32) {
    fn into_prop(self) -> Thickness {
        Thickness::new(self.0 as f32, self.1 as f32, self.2 as f32, self.3 as f32)
    }
}

// ---------------------------------------------------------------- shapes, gradients, shadows

/// Corner radii in points, in the order of the MAUI constructor.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

impl CornerRadius {
    pub const fn new(top_left: f32, top_right: f32, bottom_left: f32, bottom_right: f32) -> Self {
        Self { top_left, top_right, bottom_left, bottom_right }
    }
    pub const fn uniform(all: f32) -> Self {
        Self::new(all, all, all, all)
    }
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

impl IntoProp<CornerRadius> for f32 {
    fn into_prop(self) -> CornerRadius {
        CornerRadius::uniform(self)
    }
}
impl IntoProp<CornerRadius> for i32 {
    fn into_prop(self) -> CornerRadius {
        CornerRadius::uniform(self as f32)
    }
}
impl IntoProp<CornerRadius> for (f32, f32, f32, f32) {
    /// (top left, top right, bottom left, bottom right)
    fn into_prop(self) -> CornerRadius {
        CornerRadius::new(self.0, self.1, self.2, self.3)
    }
}
impl IntoProp<CornerRadius> for (i32, i32, i32, i32) {
    fn into_prop(self) -> CornerRadius {
        CornerRadius::new(self.0 as f32, self.1 as f32, self.2 as f32, self.3 as f32)
    }
}

/// Polygon and Line points, as ratios of the shape's box.
impl IntoProp<Vec<Point>> for Vec<(f32, f32)> {
    fn into_prop(self) -> Vec<Point> {
        self.into_iter().map(Point::from).collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GradientType {
    None,
    #[default]
    Linear,
    /// Radial, radius = half of the smaller side.
    Circular,
    /// Radial, stretched to both sides.
    Oval,
    /// Around the center, between the control's `value1` and `value1 + value2` degrees.
    Sweep,
    /// Drawn as Linear, as upstream.
    Conical,
}

/// DrawnUI SkiaGradient. Start and end are ratios of the rect the gradient fills.
#[derive(Clone, PartialEq, Debug)]
pub struct SkiaGradient {
    pub gradient_type: GradientType,
    pub colors: Vec<Color>,
    /// 0..1 per color; used only when there is one per color, else the colors are spread evenly.
    pub color_positions: Vec<f32>,
    /// Linear: the start point. Circular and Oval: the center.
    pub start_x_ratio: f32,
    pub start_y_ratio: f32,
    pub end_x_ratio: f32,
    pub end_y_ratio: f32,
    pub tile_mode: TileMode,
    /// Below 1 darkens the colors, above 1 lightens them.
    pub light: f32,
    /// Multiplies the alpha of every color.
    pub opacity: f32,
    pub blend_mode: BlendMode,
}

impl Default for SkiaGradient {
    fn default() -> Self {
        Self {
            gradient_type: GradientType::Linear,
            colors: Vec::new(),
            color_positions: Vec::new(),
            start_x_ratio: 0.0,
            start_y_ratio: 0.0,
            end_x_ratio: 0.0,
            end_y_ratio: 1.0,
            tile_mode: TileMode::Clamp,
            light: 1.0,
            opacity: 1.0,
            blend_mode: BlendMode::SrcOver,
        }
    }
}

impl SkiaGradient {
    /// Top to bottom for Linear; set the ratios or `angle` for another direction.
    pub fn new(gradient_type: GradientType, colors: impl Into<Vec<Color>>) -> Self {
        Self { gradient_type, colors: colors.into(), ..Self::default() }
    }

    /// Direction of a Linear gradient in degrees: 0 = top to bottom, 90 = left to right, 180 =
    /// bottom to top, 270 = right to left. Sets the start and end ratios (DrawnUI
    /// LinearGradientAngleToPoints).
    pub fn angle(mut self, degrees: f32) -> Self {
        let mut direction = degrees - 90.0;
        if direction < 0.0 {
            direction += 360.0;
        }
        let angle = direction.min(360.0) % 360.0;
        // A direction that points backwards on an axis starts at 0 on it.
        let ratio = |v: f32| if v <= f32::EPSILON { 0.0 } else { v };
        let (start, end) = ((180.0 - angle).to_radians(), (360.0 - angle).to_radians());
        (self.start_x_ratio, self.start_y_ratio) = (ratio(start.cos()), ratio(start.sin()));
        (self.end_x_ratio, self.end_y_ratio) = (ratio(end.cos()), ratio(end.sin()));
        self
    }
}

/// Fluent setters named after the fields: `SkiaShadow::new(color).y(4).blur(6)`.
macro_rules! fluent {
    ($ty:ident { $($name:ident: $field:ty),* $(,)? }) => {
        impl $ty {
            $(pub fn $name(mut self, v: impl IntoProp<$field>) -> Self {
                self.$name = v.into_prop();
                self
            })*
        }
    };
}

fluent!(SkiaGradient {
    gradient_type: GradientType,
    colors: Vec<Color>,
    color_positions: Vec<f32>,
    start_x_ratio: f32,
    start_y_ratio: f32,
    end_x_ratio: f32,
    end_y_ratio: f32,
    tile_mode: TileMode,
    light: f32,
    opacity: f32,
    blend_mode: BlendMode,
});

impl IntoProp<Option<Box<SkiaGradient>>> for SkiaGradient {
    fn into_prop(self) -> Option<Box<SkiaGradient>> {
        Some(Box::new(self))
    }
}

/// DrawnUI SkiaShadow: a blurred copy of the shape behind it. Offsets and blur are points.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SkiaShadow {
    pub x: f32,
    pub y: f32,
    /// The blur sigma.
    pub blur: f32,
    /// Alpha of the shadow when `color` is fully opaque; a color with its own alpha keeps it.
    pub opacity: f32,
    pub color: Color,
    /// Draws the shadow without the shape.
    pub shadow_only: bool,
}

impl Default for SkiaShadow {
    fn default() -> Self {
        Self { x: 2.0, y: 2.0, blur: 5.0, opacity: 0.5, color: Color::TRANSPARENT, shadow_only: false }
    }
}

impl SkiaShadow {
    /// A shadow of this color with the upstream defaults: 2 points right and down, blur 5,
    /// opacity 0.5.
    pub fn new(color: Color) -> Self {
        Self { color, ..Self::default() }
    }
}

fluent!(SkiaShadow { x: f32, y: f32, blur: f32, opacity: f32, color: Color, shadow_only: bool });

impl IntoProp<Vec<SkiaShadow>> for SkiaShadow {
    fn into_prop(self) -> Vec<SkiaShadow> {
        vec![self]
    }
}
