//! Hover (drawnui-cross 6m; DrawnUI HoverTests): every control under the mouse that takes hover is
//! hovered, a card and the button inside it alike; only opted-in controls hover; nothing changes
//! while content animates under the pointer, one check follows when it stops; leaving the canvas
//! ends hover at once.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    card_events: Vec<bool>,
    changes: usize,
}

fn hovered(host: &Headless<App>, id: ControlId) -> bool {
    host.ui.tree.base(id).unwrap().is_hovered()
}

fn host(root: Build<SkiaLayout>) -> Headless<App> {
    let ui = Ui::new(App::default(), |_| root);
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    host
}

#[test]
fn card_and_button_inside_both_hovered() {
    let button = SkiaButton::new("×").width_request(60).height_request(40).margin(Thickness::new(220.0, 10.0, 0.0, 0.0));
    let button_id = button.id();
    let card = SkiaShape::new()
        .width_request(300)
        .height_request(200)
        .background_color(Color::GRAY)
        .children(button)
        .on_hovered(|_me, app: &mut App, _cx, on| app.card_events.push(on));
    let card_id = card.id();
    let mut host = host(SkiaLayout::new().fill().children(card));

    host.hover(50.0, 150.0); // card only
    assert!(hovered(&host, card_id));
    assert!(!hovered(&host, button_id));

    host.hover(250.0, 30.0); // the button inside the card
    assert!(hovered(&host, card_id));
    assert!(hovered(&host, button_id));
    assert_eq!(host.ui.state.card_events, [true], "the card did not flicker");

    host.hover(380.0, 380.0); // outside
    assert!(!hovered(&host, card_id));
    assert!(!hovered(&host, button_id));
    assert_eq!(host.ui.state.card_events, [true, false]);
}

#[test]
fn only_opted_in_controls_hover() {
    let tappable = SkiaShape::new().width_request(150).height_request(150).background_color(Color::GRAY).on_tapped(|_me, _app: &mut App, _cx| {});
    let off = SkiaButton::new("Off").receives_hover(Some(false)).width_request(150).height_request(60);
    let on = SkiaButton::new("On").width_request(150).height_request(60).margin(Thickness::new(0.0, 100.0, 0.0, 0.0));
    let (tappable_id, off_id, on_id) = (tappable.id(), off.id(), on.id());
    let buttons = SkiaLayout::new().margin(Thickness::new(200.0, 0.0, 0.0, 0.0)).children((off, on));
    let mut host = host(SkiaLayout::new().fill().children((tappable, buttons)));

    host.hover(50.0, 50.0);
    assert!(!hovered(&host, tappable_id), "a tap handler does not make a control hover");
    host.hover(250.0, 20.0);
    assert!(!hovered(&host, off_id), "hover turned off on a control that has it by default");
    host.hover(250.0, 120.0);
    assert!(hovered(&host, on_id), "a button takes hover by default");
}

#[test]
fn touch_never_hovers() {
    let button = SkiaButton::new("Tap").width_request(150).height_request(60);
    let id = button.id();
    let mut host = host(SkiaLayout::new().fill().children(button));
    host.use_touch(true);
    host.tap(50.0, 20.0);
    host.settle();
    assert!(!hovered(&host, id));
    assert!(host.ui.hovered().is_empty());
}

/// A scroll over 30 cards 50 points tall that take hover; their ids.
fn cards_in_a_scroll() -> (Headless<App>, ControlId, Vec<ControlId>) {
    let cards: Vec<_> = (0..30)
        .map(|i| {
            let color = if i % 2 == 0 { Color::GRAY } else { Color::DARK_GRAY };
            SkiaShape::new().height_request(50).fill_x().background_color(color).on_hovered(|_me, app: &mut App, _cx, _on| app.changes += 1)
        })
        .collect();
    let ids = cards.iter().map(|c| c.id()).collect();
    let scroll = SkiaScroll::new().fill().content(SkiaLayout::column().spacing(0).children(cards));
    let scroll_id = scroll.id();
    let host = host(SkiaLayout::new().fill().children(scroll));
    (host, scroll_id, ids)
}

fn animating(host: &Headless<App>, scroll: ControlId) -> bool {
    host.ui.tree.find::<SkiaScroll>(scroll).unwrap().is_animating()
}

#[test]
fn scroll_animation_hover_waits_for_the_end() {
    let (mut host, scroll, cards) = cards_in_a_scroll();
    host.hover(100.0, 75.0); // card 1
    assert!(hovered(&host, cards[1]));
    host.ui.state.changes = 0;

    host.ui.tree.cx().scroll_to(scroll, 0.0, -500.0, 500.0); // animated, 10 cards down
    host.frame_after(16.0);
    assert!(animating(&host, scroll));

    // The mouse moves while the content glides: hover is not tracked.
    for i in 0..5 {
        host.hover(100.0, 75.0 + i as f32);
    }
    assert!(hovered(&host, cards[1]));
    assert_eq!(host.ui.state.changes, 0);

    for _ in 0..120 {
        if !animating(&host, scroll) {
            break;
        }
        host.frame_after(16.0);
    }
    assert!(!animating(&host, scroll));
    host.settle(); // the check at the last pointer position

    assert_eq!(host.ui.hovered(), [cards[11]], "y 79 + 500 scrolled = card 11");
    assert!(!hovered(&host, cards[1]));
}

#[test]
fn leave_clears_hover_even_while_content_animates() {
    let (mut host, scroll, _) = cards_in_a_scroll();
    host.hover(100.0, 25.0);
    let card = host.ui.hovered()[0];

    host.ui.tree.cx().scroll_to(scroll, 0.0, -500.0, 500.0);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    assert!(animating(&host, scroll));

    host.leave();
    assert!(!hovered(&host, card));
    assert!(host.ui.hovered().is_empty());

    host.settle();
    assert!(host.ui.hovered().is_empty(), "no check after the end: the mouse is gone");
}

#[test]
fn a_jump_or_a_hidden_card_checks_hover_again() {
    let (mut host, scroll, cards) = cards_in_a_scroll();
    host.hover(100.0, 75.0);
    assert_eq!(host.ui.hovered(), [cards[1]]);

    // A jump without animation moves the content under a still mouse.
    host.ui.tree.cx().scroll_to(scroll, 0.0, -100.0, 0.0);
    host.settle();
    assert_eq!(host.ui.hovered(), [cards[3]]);

    // The hovered card is hidden: the next one moves up under the mouse.
    host.ui.tree.any_mut(cards[3]).unwrap().set_is_visible(false);
    host.settle();
    assert!(!hovered(&host, cards[3]));
    assert_eq!(host.ui.hovered(), [cards[4]]);
}
