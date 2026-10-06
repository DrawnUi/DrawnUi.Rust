// Question 4, runtime setter side of the same clash.
use drawnui_spike::prelude::*;

pub struct App;

pub fn build() -> Build<SkiaButton> {
    SkiaButton::new("Tap me").on_tapped(|me, _app: &mut App, _cx| me.set_padding(4.0))
}
