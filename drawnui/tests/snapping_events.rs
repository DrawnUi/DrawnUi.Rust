//! SnappingLayout events of DrawnUI on the carousel and the drawer: Scrolled (the position, once a
//! frame while it moves), Stopped (where it came to rest), the drawer's TransitionChanged.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scrolled: u32,
    stopped: Vec<Point>,
    transitions: Vec<bool>,
}

fn slides() -> Vec<Build<SkiaLayout>> {
    [Color::RED, Color::GREEN, Color::BLUE].into_iter().map(|c| SkiaLayout::new().fill().background_color(c)).collect()
}

#[test]
fn a_carousel_reports_scrolled_and_stopped() {
    let ui = Ui::new(App::default(), |_| {
        let carousel = SkiaCarousel::new()
            .fill()
            .children(slides())
            .on_scrolled(|_me, app: &mut App, _cx, _at: Point| app.scrolled += 1)
            .on_stopped(|_me, app: &mut App, _cx, at: Point| app.stopped.push(at));
        SkiaLayout::new().fill().children(carousel)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 200, 1.0);
    host.settle();
    host.ui.state.scrolled = 0;
    host.fling((250.0, 100.0), (100.0, 100.0), 100.0, 6);
    let app = &host.ui.state;
    assert!(app.scrolled >= 3, "scrolled {} times", app.scrolled);
    assert_eq!(app.stopped.len(), 1, "{:?}", app.stopped);
    assert!((app.stopped[0].x + 300.0).abs() < 1.0, "stopped on the second slide: {:?}", app.stopped);
}

#[test]
fn a_drawer_reports_transitions_scrolled_and_stopped() {
    let ui = Ui::new(App::default(), |_| {
        let drawer = SkiaDrawer::new()
            .header_size(40)
            .height_request(200)
            .vertical_options(LayoutOptions::End)
            .children(SkiaLayout::new().fill().background_color(Color::BLUE))
            .on_transition_changed(|_me, app: &mut App, _cx, on| app.transitions.push(on))
            .on_scrolled(|_me, app: &mut App, _cx, _at: Point| app.scrolled += 1)
            .on_stopped(|_me, app: &mut App, _cx, at: Point| app.stopped.push(at));
        SkiaLayout::new().fill().children(drawer)
    });
    let mut host = Headless::new(ui.background(Color::BLACK), 300, 400, 1.0);
    host.settle();
    let app = &mut host.ui.state;
    (app.scrolled, app.transitions, app.stopped) = (0, Vec::new(), Vec::new());
    // Flick the header up: it opens.
    host.pan((150.0, 380.0), (150.0, 280.0), 60.0, 6);
    host.settle();
    let app = &host.ui.state;
    assert!(app.scrolled >= 3, "scrolled {} times", app.scrolled);
    assert_eq!(app.transitions, [true, false]);
    assert_eq!(app.stopped, [Point::new(0.0, 0.0)], "came to rest open");
}
