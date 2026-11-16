//! A drawn context menu in its own window (desktop): choosing an item closes it and leaves the
//! page where it was (Nick, 3D gallery: "Copy Link" scrolled the page down).

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::skia::Point;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
    menu: Option<PopupId>,
    copied: Vec<String>,
}

fn item(text: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .width_request(150)
        .height_request(36)
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_can_interact(true)
        .accessibility_label(text)
        .on_tapped(move |_me, app: &mut App, cx| {
            app.copied.push(text.to_owned());
            if let Some(menu) = app.menu.take() {
                cx.close_popup_window(menu);
            }
        })
}

fn host(reader: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().fill_x().padding(20).spacing(20).children((
            SkiaLabel::new("Source on GitHub")
                .font_size(18)
                .accessibility_text_selectable(true)
                .accessibility_role(Aria::LINK)
                .accessibility_label("Source on GitHub")
                .on_tapped(|_me, _app: &mut App, _cx| {})
                .on_context_menu(|_me, app: &mut App, cx, menu| {
                    let panel = SkiaShape::new().padding(4).background_color(Color::GRAY).children((SkiaLayout::column()
                        .accessibility_role(Aria::MENU)
                        .accessibility_label("Text")
                        .children((item("Copy Link"), item("Cancel"))),));
                    app.menu = Some(cx.open_popup_window(panel, menu.location, PopupWindowOptions { cpu: true, ..Default::default() }));
                    true
                }),
            (0..30).map(|i| SkiaButton::new(format!("Button {i}")).width_request(160).height_request(44)).collect::<Vec<_>>(),
        )))
    })
    .font_bytes("Default", FONT)
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 500, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.ui.set_screen_reader_running(reader);
    host.popup_windows(true);
    host.settle();
    host
}

fn offset(host: &Headless<App>) -> f32 {
    host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

#[test]
fn choosing_an_item_of_a_menu_window_leaves_the_page_where_it_was() {
    for reader in [false, true] {
        let mut host = host(reader);
        host.right_click(60.0, 30.0);
        let request = host.popup_window_requests()[0];
        host.place_popup_window(request.id, 158.0, 80.0, 1.0, Point::new(60.0, 30.0));
        host.popup_tap(request.id, 40.0, 20.0);
        for _ in 0..80 {
            host.frame_after(16.0);
        }
        host.frame_after(1100.0);
        for _ in 0..40 {
            host.frame_after(16.0);
        }
        assert_eq!(host.ui.state.copied, ["Copy Link"], "reader {reader}");
        assert!(host.popup_window_requests().is_empty());
        assert_eq!(offset(&host), 0.0, "reader {reader}: the page stayed where it was");
    }
}

