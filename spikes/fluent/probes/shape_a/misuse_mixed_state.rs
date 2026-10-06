// Misuse: a child built for another state type.
#[path = "model.rs"]
mod model;
use model::*;

pub fn build() -> Build<SkiaLayout, App> {
    SkiaLayout::column().children((
        SkiaButton::new::<App>("a").on_tapped(|_me, cx| cx.state.count += 1),
        SkiaButton::new::<Other>("b").on_tapped(|_me, cx| cx.state.count += 1),
    ))
}
