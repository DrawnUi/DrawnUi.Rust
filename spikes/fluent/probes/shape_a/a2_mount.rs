// A2: free constructors, no annotations, S expected from the Tree<App> it is mounted into.
#[path = "model.rs"]
mod model;
use model::*;

pub fn run() {
    let mut tree: Tree<App> = Tree::new();
    tree.mount(SkiaLayout::column().children((
        SkiaButton::new("Tap me").on_tapped(|_me, cx| cx.state.count += 1),
    )));
}
