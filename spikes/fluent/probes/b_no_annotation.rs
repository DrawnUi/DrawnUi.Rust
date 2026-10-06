// Shape B misuse: the state annotation is left out.
use drawnui_spike::prelude::*;

pub struct App {
    pub count: i32,
}

pub fn build() -> Build<SkiaButton> {
    SkiaButton::new("Tap me").on_tapped(|_me, app, _cx| app.count += 1)
}
