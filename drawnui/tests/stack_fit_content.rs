//! A stack that takes the size of its content across (drawnui-cross 6an, CSS fit-content) is as
//! large there as its largest child, a Fill child counting with its content up to the room there
//! is; Fill children are then laid out at that size and the rows placed for their heights.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

#[derive(Default)]
struct App {
    stack: Handle<SkiaLayout>,
    title: Handle<SkiaLabel>,
    divider: Handle<SkiaShape>,
    cards: [Handle<SkiaShape>; 2],
    row: Handle<SkiaLayout>,
}

fn host(build: impl FnOnce(&mut App) -> Build<SkiaLayout> + 'static, width: i32, scale: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), build).font_bytes("Default", FONT);
    let mut host = Headless::new(ui, width, 800, scale);
    host.settle();
    host
}

fn rect(host: &Headless<App>, id: impl Into<ControlId>) -> Rect {
    host.ui.tree.base(id).unwrap().rect
}

#[test]
fn a_short_title_and_a_divider_are_as_wide_as_the_title() {
    let host = host(
        |app| {
            SkiaLayout::new().fill().children((SkiaLayout::column().horizontal_options(LayoutOptions::Center).spacing(4).assign(&mut app.stack).children((
                SkiaLabel::new("Short title").font_size(20).assign(&mut app.title),
                SkiaShape::new().fill_x().height_request(1).background_color(Color::WHITE).assign(&mut app.divider),
            )),))
        },
        600,
        1.0,
    );
    let (stack, title, divider) = (rect(&host, host.ui.state.stack), rect(&host, host.ui.state.title), rect(&host, host.ui.state.divider));
    assert!(title.width() > 50.0 && title.width() < 200.0, "title {title:?}");
    assert_eq!(stack.width(), title.width(), "the stack fits its title: {stack:?}");
    assert_eq!(divider.width(), title.width(), "the divider takes the stack's width: {divider:?}");
}

#[test]
fn fill_cards_with_long_text_take_the_maximum_width() {
    const LONG: &str = "A card with a long text that wraps over more than one line when the column is narrow, and is much longer than the buttons below it.";
    let card = |text: &'static str, slot: &mut Handle<SkiaShape>| {
        SkiaShape::new().fill_x().padding(16).background_color(Color::from_rgb(40, 44, 60)).assign(slot).children((SkiaLabel::new(text).font_size(16),))
    };
    let host = host(
        move |app| {
            let [a, b] = &mut app.cards;
            SkiaLayout::new().fill().children((SkiaLayout::column().horizontal_options(LayoutOptions::Center).maximum_width_request(640).spacing(16).assign(&mut app.stack).children((
                card(LONG, a),
                card(LONG, b),
                SkiaLayout::row().spacing(8).assign(&mut app.row).children((SkiaButton::new("One"), SkiaButton::new("Two"), SkiaButton::new("Three"))),
            )),))
        },
        1000,
        1.25,
    );
    let state = &host.ui.state;
    let (stack, a, b, row) = (rect(&host, state.stack), rect(&host, state.cards[0]), rect(&host, state.cards[1]), rect(&host, state.row));
    assert_eq!(stack.width(), 800.0, "640 points at 1.25: {stack:?}");
    assert_eq!((a.width(), b.width()), (800.0, 800.0), "the cards take the stack's width");
    assert_eq!(b.top, a.bottom + 20.0, "16 points between the cards, no overlap: {a:?} {b:?}");
    assert_eq!(row.top, b.bottom + 20.0, "the row under the second card: {b:?} {row:?}");
}

#[test]
fn fill_cards_wrap_when_the_room_is_smaller_than_their_text() {
    const LONG: &str = "A card with a long text that wraps over more than one line when the column is narrow.";
    let host = host(
        move |app| {
            let [a, _] = &mut app.cards;
            SkiaLayout::new().fill().children((SkiaLayout::column().horizontal_options(LayoutOptions::Center).spacing(16).assign(&mut app.stack).children((
                SkiaShape::new().fill_x().padding(16).assign(a).children((SkiaLabel::new(LONG).font_size(16),)),
                SkiaLabel::new("Below").assign(&mut app.title),
            )),))
        },
        300,
        1.0,
    );
    let state = &host.ui.state;
    let (stack, card, below) = (rect(&host, state.stack), rect(&host, state.cards[0]), rect(&host, state.title));
    assert_eq!(stack.width(), 300.0, "the room caps the content: {stack:?}");
    assert_eq!(card.width(), 300.0);
    assert!(card.height() > 3.0 * 16.0, "the text wraps: {card:?}");
    assert_eq!(below.top, card.bottom + 16.0, "the next child goes under the wrapped card: {card:?} {below:?}");
}

#[test]
fn a_row_fits_its_fill_children_in_height() {
    let host = host(
        |app| {
            SkiaLayout::new().fill().children((SkiaLayout::row().vertical_options(LayoutOptions::Center).spacing(4).assign(&mut app.stack).children((
                SkiaShape::new().width_request(40).height_request(30).assign(&mut app.cards[0]),
                SkiaShape::new().width_request(1).fill_y().background_color(Color::WHITE).assign(&mut app.divider),
            )),))
        },
        400,
        1.0,
    );
    let state = &host.ui.state;
    assert_eq!(rect(&host, state.stack).height(), 30.0);
    assert_eq!(rect(&host, state.divider).height(), 30.0);
}
