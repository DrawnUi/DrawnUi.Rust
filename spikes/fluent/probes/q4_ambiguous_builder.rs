// Question 4: `padding` is declared on ButtonProps and LayoutProps; SkiaButton has both parts.
use drawnui_spike::prelude::*;

pub fn build() -> Build<SkiaButton> {
    SkiaButton::new("Tap me").padding(4.0)
}
