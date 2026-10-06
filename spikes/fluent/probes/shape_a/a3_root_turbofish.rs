// A3: S pinned on the root constructor only.
#[path = "model.rs"]
mod model;
use model::*;

pub fn build() -> Build<SkiaLayout, App> {
    SkiaLayout::column::<App>().spacing(16.0).children((
        SkiaButton::new("Tap me").on_tapped(|_me, cx| cx.state.count += 1),
    ))
}
