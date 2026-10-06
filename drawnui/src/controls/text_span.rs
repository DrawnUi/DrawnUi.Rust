//! TextSpan: a styled fragment inside a SkiaLabel (DrawnUI TextSpan). Not a control: it has no
//! box of its own, the label lays it out and hit-tests it.

use std::any::{Any, type_name};

use skia_safe::Color;

use crate::controls::label::SkiaLabel;
use crate::tree::{Cx, Mut, Raw};
use crate::types::IntoProp;

pub(crate) type SpanTapped = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;

/// One fragment of a label's text with its own style. Unset color, size and family inherit from
/// the label. `underline_width`: points, negative = that many pixels (default -1 = 1 px).
pub struct TextSpan {
    /// What it shows; `\n` breaks the line.
    pub text: String,
    /// `None` = the label's.
    pub text_color: Option<Color>,
    /// Points.
    pub font_size: Option<f32>,
    /// A registered alias; `None` = the label's.
    pub font_family: Option<String>,
    /// 0 = the label's weight.
    pub font_weight: i32,
    /// Weight 700: the nearest registered face, emboldened when it is lighter than 600.
    pub is_bold: bool,
    /// Slanted: a skew of the face, as DrawnUi.React (no italic faces).
    pub is_italic: bool,
    /// A line under the text in its color.
    pub underline: bool,
    /// Points; negative = that many pixels.
    pub underline_width: f32,
    /// A line through the text in `strikeout_color`.
    pub strikeout: bool,
    /// Points.
    pub strikeout_width: f32,
    /// Red by default.
    pub strikeout_color: Color,
    /// Fills the span's part of each line box.
    pub background_color: Option<Color>,
    /// Free payload (SkiaRichLabel keeps the link url here).
    pub tag: String,
    /// Takes taps without a handler of its own (a rich label's links).
    pub force_capture_input: bool,
    /// A hidden span takes no room.
    pub is_visible: bool,
    pub(crate) tapped: Option<SpanTapped>,
}

impl Default for TextSpan {
    fn default() -> Self {
        Self {
            text: String::new(),
            text_color: None,
            font_size: None,
            font_family: None,
            font_weight: 0,
            is_bold: false,
            is_italic: false,
            underline: false,
            underline_width: -1.0,
            strikeout: false,
            strikeout_width: 1.0,
            strikeout_color: Color::RED,
            background_color: None,
            tag: String::new(),
            force_capture_input: false,
            is_visible: true,
            tapped: None,
        }
    }
}

/// Fluent setters named after the fields.
macro_rules! setters {
    ($($name:ident: $ty:ty),* $(,)?) => {
        impl TextSpan {
            $(#[doc = concat!("Sets `", stringify!($name), "`.")]
            pub fn $name(mut self, v: impl IntoProp<$ty>) -> Self {
                self.$name = v.into_prop();
                self
            })*
        }
    };
}

setters!(
    text: String,
    text_color: Option<Color>,
    font_weight: i32,
    is_bold: bool,
    is_italic: bool,
    underline: bool,
    underline_width: f32,
    strikeout: bool,
    strikeout_width: f32,
    strikeout_color: Color,
    background_color: Option<Color>,
    tag: String,
    force_capture_input: bool,
    is_visible: bool,
);

impl TextSpan {
    /// A span with a text and the label's style.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Self::default() }
    }

    /// Points.
    pub fn font_size(mut self, v: impl IntoProp<f32>) -> Self {
        self.font_size = Some(v.into_prop());
        self
    }

    /// A registered alias instead of the label's family.
    pub fn font_family(mut self, v: impl Into<String>) -> Self {
        self.font_family = Some(v.into());
        self
    }

    /// Runs when the span is tapped, with its label as `me`.
    pub fn on_tapped<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, SkiaLabel>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.tapped = Some(Box::new(move |raw, state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| panic!("handler expects app state `{}`", type_name::<S>()));
            f(&mut raw.typed(), state, cx)
        }));
        self
    }

    /// Takes taps: it has a handler or `force_capture_input`.
    pub fn has_tap_handler(&self) -> bool {
        self.tapped.is_some() || self.force_capture_input
    }
}

/// What `SkiaLabel::spans` accepts: one span, a `Vec`, or a tuple of them.
pub trait IntoSpans {
    /// Appends the spans to `out`.
    fn push_into(self, out: &mut Vec<TextSpan>);
}

impl IntoSpans for TextSpan {
    fn push_into(self, out: &mut Vec<TextSpan>) {
        out.push(self)
    }
}
impl IntoSpans for Vec<TextSpan> {
    fn push_into(self, out: &mut Vec<TextSpan>) {
        out.extend(self)
    }
}

macro_rules! tuple_spans {
    () => {};
    ($head:ident $($tail:ident)*) => {
        impl<$head: IntoSpans, $($tail: IntoSpans),*> IntoSpans for ($head, $($tail,)*) {
            #[allow(non_snake_case)]
            fn push_into(self, out: &mut Vec<TextSpan>) {
                let ($head, $($tail,)*) = self;
                $head.push_into(out);
                $($tail.push_into(out);)*
            }
        }
        tuple_spans!($($tail)*);
    };
}
tuple_spans!(A B C D E F G H I J K L M N O P Q R S T);
