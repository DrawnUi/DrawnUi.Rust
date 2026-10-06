// A5: as A4 but the handler parameter is a fn pointer naming S (expectation can flow in).
#[path = "model.rs"]
mod model;
use model::*;

pub fn build() -> Build<SkiaButton, App> {
    SkiaButton::new("Tap me").on_tapped_fn(|_me, cx| cx.state.count += 1)
}
