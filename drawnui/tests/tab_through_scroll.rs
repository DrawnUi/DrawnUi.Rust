//! Tab and Shift+Tab reach every control of a scroll, also the ones outside its viewport, and
//! bring each into view (DrawnUi.Net ac70a1d9): plain children, recycled items made only near the
//! viewport, and tall cards with a button at their top and at their bottom.

use drawnui::prelude::*;
use drawnui::Detached;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    scroll: Handle<SkiaScroll>,
}

const VIEW: f32 = 280.0;

fn button(text: String) -> Build<SkiaButton> {
    SkiaButton::new(text).width_request(160).height_request(44)
}

/// Back / Home over a scroll VIEW tall over a Footer; `content` fills the scroll.
fn host(content: impl FnOnce(&mut App) -> Detached + 'static) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaGrid::new().fill().row_definitions(format!("Auto,{VIEW},Auto").as_str()).children((
            SkiaLayout::row().spacing(10).padding(8).row(0).children((button("Back".into()), button("Home".into()))),
            SkiaScroll::new().fill().row(1).assign(&mut app.scroll).content(content(app)),
            SkiaLayout::row().padding(8).row(2).children((button("Footer".into()),)),
        ))
    })
    .default_accessibility_role::<SkiaButton>(Aria::BUTTON);
    let mut host = Headless::new(ui, 400, 500, 1.0);
    host.ui.set_keyboard_navigation(true);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host
}

fn press(host: &mut Headless<App>, key: &str, shift: bool) {
    host.key_down(key, Modifiers { shift, ..Modifiers::default() });
    host.key_up(key, Modifiers { shift, ..Modifiers::default() });
    for _ in 0..60 {
        host.frame_after(16.0);
    }
    host.frame_after(1100.0);
}

/// The name of the control the keyboard is on, checked to be inside the scroll's viewport when
/// it is one of the scroll's.
fn keyboard_on(host: &Headless<App>) -> String {
    let focused = host.ui.accessibility_focused().expect("something has the keyboard");
    let node = host.ui.accessibility_nodes().iter().find(|n| n.control == focused).expect("its node");
    let scroll = host.rect(host.ui.state.scroll);
    let inside = std::iter::successors(Some(focused), |c| host.ui.tree.parent(*c)).any(|c| c == host.ui.state.scroll.into());
    if inside {
        let r = node.rect;
        assert!(r.top >= scroll.top - 0.5 && r.bottom <= scroll.bottom + 0.5, "{} at {r:?} outside the viewport {scroll:?}", node.label);
    }
    node.label.clone()
}

fn walk(host: &mut Headless<App>, expected: &[String]) {
    for (i, name) in expected.iter().enumerate() {
        press(host, "Tab", false);
        assert_eq!(&keyboard_on(host), name, "Tab #{i}");
    }
    for (i, name) in expected.iter().rev().skip(1).enumerate() {
        press(host, "Tab", true);
        assert_eq!(&keyboard_on(host), name, "Shift+Tab #{i}");
    }
}

fn names(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut all = vec!["Back".to_owned(), "Home".to_owned()];
    all.extend(items);
    all.push("Footer".to_owned());
    all
}

#[test]
fn tab_walks_every_child_of_a_scroll() {
    let mut host = host(|_| SkiaLayout::column().fill_x().spacing(8).padding(8).children((0..30).map(|i| button(format!("Item {i}"))).collect::<Vec<_>>()).into());
    walk(&mut host, &names((0..30).map(|i| format!("Item {i}"))));
}

#[test]
fn tab_walks_recycled_items_past_the_made_cells() {
    let mut host = host(|_| {
        SkiaStack::new().fill_x().spacing(8).recycling_template(RecyclingTemplate::Enabled).measure_items_strategy(MeasuringStrategy::MeasureFirst).items(
            |_: &App| 200,
            || {
                let mut b = Handle::default();
                (button(String::new()).assign(&mut b), b)
            },
            |b: &Handle<SkiaButton>, _: &App, index, cx| {
                if let Some(mut b) = cx.get_mut(*b) {
                    b.set_text(format!("Item {index}"));
                }
            },
        )
        .into()
    });
    walk(&mut host, &names((0..200).map(|i| format!("Item {i}"))));
}

#[test]
fn tab_reaches_both_buttons_of_tall_cards() {
    for cached in [false, true] {
        let mut host = host(move |_| {
            SkiaLayout::column().fill_x().spacing(8).children(
                (0..3)
                    .map(|i| {
                        SkiaLayout::new()
                            .fill_x()
                            .height_request(600)
                            .use_cache(if cached { CacheType::Image } else { CacheType::None })
                            .children((button(format!("Top {i}")), button(format!("Bottom {i}")).vertical_options(LayoutOptions::End)))
                    })
                    .collect::<Vec<_>>(),
            )
            .into()
        });
        walk(&mut host, &names((0..3).flat_map(|i| [format!("Top {i}"), format!("Bottom {i}")])));
    }
}

/// 200 recycled cells, each a row of two buttons "A i" and "B i".
fn two_button_cells() -> Headless<App> {
    host(|_| {
        SkiaStack::new().fill_x().spacing(8).recycling_template(RecyclingTemplate::Enabled).measure_items_strategy(MeasuringStrategy::MeasureFirst).items(
            |_: &App| 200,
            || {
                let (mut a, mut b) = (Handle::default(), Handle::default());
                let row = SkiaLayout::row().spacing(8).children((button(String::new()).width_request(100).assign(&mut a), button(String::new()).width_request(100).assign(&mut b)));
                (row, (a, b))
            },
            |(a, b): &(Handle<SkiaButton>, Handle<SkiaButton>), _: &App, index, cx| {
                if let Some(mut a) = cx.get_mut(*a) {
                    a.set_text(format!("A {index}"));
                }
                if let Some(mut b) = cx.get_mut(*b) {
                    b.set_text(format!("B {index}"));
                }
            },
        )
        .into()
    })
}

#[test]
fn shift_tab_enters_each_recycled_cell_at_its_last_stop() {
    let mut host = two_button_cells();
    // Shift+Tab with nothing focused starts at the end.
    press(&mut host, "Tab", true);
    assert_eq!(keyboard_on(&host), "Footer");
    for i in (0..200).rev() {
        press(&mut host, "Tab", true);
        assert_eq!(keyboard_on(&host), format!("B {i}"));
        press(&mut host, "Tab", true);
        assert_eq!(keyboard_on(&host), format!("A {i}"));
    }
    press(&mut host, "Tab", true);
    assert_eq!(keyboard_on(&host), "Home");
}
