//! DrawnUI `Scale`: sets ScaleX and ScaleY together, reads the smaller of them.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[test]
fn scale_sets_both_axes_and_reads_the_smaller() {
    let square = SkiaLayout::new().width_request(20).height_request(20).scale(2.0);
    let id = square.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(square));
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    let p = &host.ui.tree.base(id).unwrap().p;
    assert_eq!((p.scale_x, p.scale_y, p.scale()), (2.0, 2.0, 2.0));

    host.ui.tree.any_mut(id).unwrap().set_scale(0.5);
    host.ui.tree.any_mut(id).unwrap().set_scale_y(0.25);
    let p = &host.ui.tree.base(id).unwrap().p;
    assert_eq!((p.scale_x, p.scale_y, p.scale()), (0.5, 0.25, 0.25));
}
