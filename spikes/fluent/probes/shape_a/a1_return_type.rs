// A1: free constructors, no annotations, S expected from the fn return type.
#[path = "model.rs"]
mod model;
use model::*;

pub fn build() -> Build<SkiaLayout, App> {
    SkiaLayout::column().spacing(16.0).children((
        SkiaButton::new("Tap me").on_tapped(|_me, cx| cx.state.count += 1),
    ))
}
