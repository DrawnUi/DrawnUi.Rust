//! A scroll's content, header, footer and refresh indicator set at run time, as DrawnUI's settable
//! SkiaScroll.Content / Header / Footer / RefreshIndicator (asked for by a player that builds
//! controls from data).

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

fn square(color: Color, height: f32) -> Build<SkiaLayout> {
    SkiaLayout::new().fill_x().height_request(height).background_color(color)
}

fn scene() -> (Headless<()>, ControlId) {
    let scroll = SkiaScroll::new().fill().refresh_enabled(true).content(square(Color::BLUE, 800.0));
    let id = scroll.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(scroll)).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    (host, id)
}

#[test]
fn a_header_is_added_replaced_and_removed_at_run_time() {
    let (mut host, scroll) = scene();
    assert_eq!(host.pixel(10, 10), Color::BLUE);

    host.ui.tree.cx().set_scroll_header(scroll, Some(square(Color::RED, 40.0).into()));
    host.settle();
    assert_eq!((host.pixel(10, 10), host.pixel(10, 60)), (Color::RED, Color::BLUE), "header over the content");

    host.ui.tree.cx().set_scroll_header(scroll, Some(square(Color::GREEN, 40.0).into()));
    host.settle();
    assert_eq!(host.pixel(10, 10), Color::GREEN, "replaced");

    host.ui.tree.cx().set_scroll_footer(scroll, Some(square(Color::BLACK, 30.0).into()));
    host.ui.tree.cx().set_scroll_header(scroll, None);
    host.settle();
    assert_eq!(host.pixel(10, 10), Color::BLUE, "removed: the content is first again");
    assert_eq!(host.ui.tree.children(scroll).len(), 2, "content and footer");
}

#[test]
fn a_refresh_indicator_set_at_run_time_shows_on_a_pull() {
    let (mut host, scroll) = scene();
    host.ui.tree.cx().set_refresh_indicator(scroll, Some(square(Color::RED, 40.0).into()));
    host.settle();
    // Pull down from the top, held: the indicator comes in over the content's start.
    host.ui.pointer(PointerKind::Down, 100.0, 50.0, host.time_ms());
    host.frame_after(16.0);
    for i in 1..=10 {
        host.ui.pointer(PointerKind::Move, 100.0, 50.0 + 15.0 * i as f32, host.time_ms());
        host.frame_after(16.0);
    }
    let red = (0..120).any(|y| host.pixel(10, y) == Color::RED || host.pixel(10, y).r() > 200 && host.pixel(10, y).g() < 120);
    host.ui.pointer(PointerKind::Up, 100.0, 200.0, host.time_ms());
    host.settle();
    assert!(red, "the indicator shows while the content is pulled");
}
