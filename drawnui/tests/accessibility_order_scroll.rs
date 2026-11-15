//! Reading and Tab order with a scroll (drawnui-cross, React 129e8e9): two nodes compare by their
//! place inside the nearest scroll they share, a scroll's nodes take the scroll's place among the
//! nodes around it, so the order is the same at any scroll position: the header, the items, the
//! footer, also when the items above the view are scrolled under the header.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let button = |text: String| SkiaButton::new(text).width_request(120).height_request(44);
        SkiaGrid::new().fill().row_definitions("Auto,*,Auto").children((
            SkiaLayout::row().spacing(10).padding(8).row(0).children((button("Back".into()), button("Home".into()))),
            SkiaScroll::new()
                .fill()
                .row(1)
                .assign(&mut app.scroll)
                .content(SkiaLayout::column().fill_x().spacing(16).padding(8).children((0..12).map(|i| button(format!("Item {i}"))).collect::<Vec<_>>())),
            SkiaLayout::row().padding(8).row(2).children((button("Footer".into()),)),
        ))
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 500, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn order(host: &Headless<App>) -> Vec<String> {
    host.ui.accessibility_nodes().iter().map(|n| n.label.clone()).filter(|l| !l.is_empty()).collect()
}

fn expected() -> Vec<String> {
    let mut all = vec!["Back".to_owned(), "Home".to_owned()];
    all.extend((0..12).map(|i| format!("Item {i}")));
    all.push("Footer".to_owned());
    all
}

#[test]
fn the_order_is_header_items_footer_at_any_scroll_position() {
    let mut host = host();
    assert_eq!(order(&host), expected(), "at the top");

    let scroll = host.ui.state.scroll;
    host.ui.tree.cx().set_viewport_offset_y(scroll, -160.0);
    // The snapshot is rebuilt at most once a second.
    host.frame_after(16.0);
    host.frame_after(1100.0);
    host.frame_after(16.0);
    let offset = host.ui.tree.find::<SkiaScroll>(scroll).unwrap().viewport_offset_y();
    assert!((offset + 160.0).abs() < 0.5, "scrolled by 160: {offset}");
    assert_eq!(order(&host), expected(), "scrolled by 160 pt");
}
