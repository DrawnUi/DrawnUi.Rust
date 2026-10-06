// `impl Into<f32>` with an integer literal (C# habit: `Spacing = 16`).
use drawnui_spike::prelude::*;

pub fn build() -> Build<SkiaLayout> {
    SkiaLayout::column().spacing(16)
}
