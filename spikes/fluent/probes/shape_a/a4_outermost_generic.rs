// A4: no tree at all; on_tapped is the outermost call and the return type names S.
#[path = "model.rs"]
mod model;
use model::*;

pub fn build() -> Build<SkiaButton, App> {
    SkiaButton::new("Tap me").on_tapped(|_me, cx| cx.state.count += 1)
}
