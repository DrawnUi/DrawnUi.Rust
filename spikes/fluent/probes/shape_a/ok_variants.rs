// OK variants: what the user has to write for shape A to compile.
#[path = "model.rs"]
mod model;
use model::*;

// 1. Turbofish on every constructor that carries a handler.
pub fn turbofish() -> Build<SkiaLayout, App> {
    SkiaLayout::column().spacing(16.0).children((
        SkiaButton::new::<App>("Tap me").on_tapped(|_me, cx| cx.state.count += 1),
    ))
}
// 2. Annotate the closure parameter (no shorter than shape B).
pub fn annotated() -> Build<SkiaLayout, App> {
    SkiaLayout::column().spacing(16.0).children((
        SkiaButton::new("Tap me").on_tapped(|_me, cx: &mut Cx<App>| cx.state.count += 1),
    ))
}
// 3. State-typed factory instead of free constructors: no annotations in the closure.
pub fn factory(ui: &Ui<App>) -> Build<SkiaLayout, App> {
    ui.column().spacing(16.0).children((
        ui.button("Tap me").on_tapped(|_me, cx| cx.state.count += 1),
    ))
}
// 4. Handler-free controls need nothing: S is inferred from the return type.
pub fn no_handlers() -> Build<SkiaLayout, App> {
    SkiaLayout::column().children((SkiaButton::new("a"), SkiaButton::new("b")))
}
