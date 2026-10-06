// A6: the fn-pointer variant inside a children tuple, and followed by another builder call.
#[path = "model.rs"]
mod model;
use model::*;

pub fn in_tuple() -> Build<SkiaLayout, App> {
    SkiaLayout::column().children((
        SkiaButton::new("Tap me").on_tapped_fn(|_me, cx| cx.state.count += 1),
    ))
}
pub fn chained() -> Build<SkiaButton, App> {
    SkiaButton::new("Tap me").on_tapped_fn(|_me, cx| cx.state.count += 1).spacing(4.0)
}
