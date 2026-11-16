//! A list scrolled inside a page that is itself scrolled past the list: the accessibility snapshot
//! puts the list's rows where they are, above the screen, never back on it (a screen reader would
//! otherwise find invisible rows over the controls that are there). Seen on Android with the 3D
//! gallery's shop list after scrolling the page to its end.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    page: Handle<SkiaScroll>,
    list: Handle<SkiaScroll>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let button = |text: String| SkiaButton::new(text).width_request(120).height_request(44);
        SkiaScroll::new().fill().assign(&mut app.page).content(SkiaLayout::column().fill_x().spacing(16).children((
            SkiaShape::new().fill_x().height_request(300).is_clipped_to_bounds(true).children((SkiaScroll::new()
                .fill()
                .assign(&mut app.list)
                .content(SkiaLayout::column().fill_x().spacing(8).children((0..30).map(|i| button(format!("Row {i}"))).collect::<Vec<_>>())),)),
            SkiaLayout::column().fill_x().spacing(16).children((0..30).map(|i| button(format!("Below {i}"))).collect::<Vec<_>>()),
        )))
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 500, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn rebuilt(host: &mut Headless<App>) {
    // The snapshot is rebuilt at most once a second.
    host.frame_after(16.0);
    host.frame_after(1100.0);
    host.frame_after(16.0);
}

#[test]
fn rows_of_a_list_scrolled_off_with_the_page_stay_off_the_screen() {
    let mut host = host();
    let (page, list) = (host.ui.state.page, host.ui.state.list);
    // The list to its end, then the page well past the list.
    host.ui.tree.cx().set_viewport_offset_y(list, -10_000.0);
    rebuilt(&mut host);
    host.ui.tree.cx().set_viewport_offset_y(page, -1200.0);
    rebuilt(&mut host);
    let on_screen: Vec<String> = host
        .ui
        .accessibility_nodes()
        .iter()
        .filter(|n| n.label.starts_with("Row ") && n.rect.bottom > 0.0 && n.rect.top < 500.0)
        .map(|n| format!("{} {:?}", n.label, n.rect))
        .collect();
    assert!(on_screen.is_empty(), "rows of a list above the screen reported on it: {on_screen:?}");
}
