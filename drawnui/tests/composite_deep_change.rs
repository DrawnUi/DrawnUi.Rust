//! ImageComposite draws a change at any depth again by its area (drawnui-cross 6m; DrawnUI
//! CompositeDeepChangeTests): a card in an uncached stack, the stack a child of the composite. Only
//! the card's area is erased and drawn again; the pixels equal a full render of the same state. A
//! transform on the way draws the child whole; too many changes at once draw everything.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const COLORS: [Color; 5] = [
    Color::from_rgb(70, 130, 180),
    Color::from_rgb(255, 165, 0),
    Color::from_rgb(46, 139, 87),
    Color::from_rgb(128, 0, 128),
    Color::from_rgb(255, 215, 0),
];

struct Built {
    host: Headless<()>,
    list: ControlId,
    inner: ControlId,
    caption: ControlId,
    cards: Vec<ControlId>,
}

/// A caption, then the cards in an uncached inner stack (the Background panel's shape), 300 x 400.
fn build(count: usize, cache: CacheType, change: impl Fn(usize, Build<SkiaShape>) -> Build<SkiaShape>) -> Built {
    let height = if count > 5 { 10 } else { 40 };
    let cards: Vec<_> = (0..count)
        .map(|i| {
            let card = SkiaShape::new()
                .corner_radius(10)
                .height_request(height)
                .fill_x()
                .background_color(COLORS[i % COLORS.len()])
                .stroke_color(Color::WHITE)
                .stroke_width(1)
                .use_cache(CacheType::Image);
            change(i, card)
        })
        .collect();
    let ids: Vec<ControlId> = cards.iter().map(|c| c.id()).collect();
    let inner = SkiaLayout::column().spacing(8).children(cards);
    let caption = SkiaLabel::new("Presets").text_color(Color::WHITE).font_size(14);
    let (inner_id, caption_id) = (inner.id(), caption.id());
    let list = SkiaLayout::column().padding(12).spacing(8).use_cache(cache).children((caption, inner));
    let list_id = list.id();
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(list)).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    Built { host, list: list_id, inner: inner_id, caption: caption_id, cards: ids }
}

fn record(b: &Built) -> CompositeRecord {
    b.host.ui.tree.last_composite_record(b.list).expect("recorded")
}

/// The same final state drawn from scratch, without the composite: the partial record must give
/// these pixels.
fn same_as_full_render(b: &mut Built, count: usize, change: impl Fn(usize, Build<SkiaShape>) -> Build<SkiaShape>) {
    let mut reference = build(count, CacheType::None, change);
    let (mut lit, mut different) = (0, Vec::new());
    for y in 0..400 {
        for x in 0..300 {
            let (p, q) = (b.host.pixel(x, y), reference.host.pixel(x, y));
            lit += usize::from(q != Color::BLACK);
            let far = |a: u8, b: u8| a.abs_diff(b) > 2;
            if far(p.r(), q.r()) || far(p.g(), q.g()) || far(p.b(), q.b()) {
                different.push((x, y));
            }
        }
    }
    assert!(lit > 300 * 400 / 5, "a real picture, not two blank ones: {lit}");
    assert!(different.is_empty(), "{} pixels differ, first {:?}", different.len(), different.first());
}

#[test]
fn card_in_uncached_stack_only_its_area_is_redrawn() {
    let mut b = build(5, CacheType::ImageComposite, |_, c| c);
    assert!(!record(&b).partial);

    let card = b.cards[2];
    b.host.ui.tree.any_mut(card).unwrap().set_background_color(Color::RED);
    b.host.settle();

    let r = record(&b);
    assert!(r.partial);
    assert_eq!(r.changed, [card]);
    assert_eq!(r.areas, [b.host.rect(card)]);
    assert!(r.children.contains(&b.inner));
    same_as_full_render(&mut b, 5, |i, c| if i == 2 { c.background_color(Color::RED) } else { c });
}

#[test]
fn two_cards_changed_two_areas() {
    let mut b = build(5, CacheType::ImageComposite, |_, c| c);
    let (first, last) = (b.cards[0], b.cards[4]);
    b.host.ui.tree.any_mut(first).unwrap().set_background_color(Color::RED);
    b.host.ui.tree.any_mut(last).unwrap().set_background_color(Color::WHITE);
    b.host.settle();

    let r = record(&b);
    assert!(r.partial);
    assert_eq!(r.areas.len(), 2);
    assert_eq!(r.changed.len(), 2);
    same_as_full_render(&mut b, 5, |i, c| match i {
        0 => c.background_color(Color::RED),
        4 => c.background_color(Color::WHITE),
        _ => c,
    });
}

#[test]
fn a_moved_card_is_redrawn_where_it_was_and_where_it_is() {
    // C# draws the inner stack whole here (any transform on the way); DrawnUi.Rust maps the area
    // through the transform, as drawn before and now (drawnui-cross 6m rule b; PARITY.md).
    let mut b = build(5, CacheType::ImageComposite, |_, c| c);
    let card = b.cards[1];
    let before = b.host.rect(card);
    b.host.ui.tree.any_mut(card).unwrap().set_translation_x(20);
    b.host.settle();

    let r = record(&b);
    assert!(r.partial);
    assert_eq!(r.changed, [card]);
    let moved = before.with_offset((20.0, 0.0));
    assert_eq!(r.areas, [Rect::new(before.left, before.top, moved.right, moved.bottom)]);
    assert!(r.children.contains(&b.inner));
    same_as_full_render(&mut b, 5, |i, c| if i == 1 { c.translation_x(20) } else { c });

    // The moved card changes color, then goes back.
    b.host.ui.tree.any_mut(card).unwrap().set_background_color(Color::RED);
    b.host.settle();
    assert_eq!(record(&b).changed, [card]);
    same_as_full_render(&mut b, 5, |i, c| if i == 1 { c.translation_x(20).background_color(Color::RED) } else { c });
    b.host.ui.tree.any_mut(card).unwrap().set_translation_x(0);
    b.host.settle();
    same_as_full_render(&mut b, 5, |i, c| if i == 1 { c.background_color(Color::RED) } else { c });
}

#[test]
fn an_effect_on_the_way_redraws_the_direct_child_whole() {
    let mut b = build(5, CacheType::ImageComposite, |_, c| c);
    // An effect that changes nothing: what matters is that one is there.
    struct Marker;
    impl drawnui::effects::SkiaEffect for Marker {}
    b.host.ui.tree.any_mut(b.inner).unwrap().add_visual_effect(Marker);
    b.host.settle();
    b.host.ui.tree.any_mut(b.cards[2]).unwrap().set_background_color(Color::RED);
    b.host.settle();

    let r = record(&b);
    assert!(r.partial);
    assert!(r.changed.is_empty(), "an effect over the card's stack depends on all of it");
    assert_eq!(r.children, [b.inner]);
}

#[test]
fn direct_child_change_as_before() {
    let mut b = build(5, CacheType::ImageComposite, |_, c| c);
    let caption = b.caption;
    b.host.ui.tree.find_mut::<SkiaLabel>(caption).unwrap().set_text_color(Color::YELLOW);
    b.host.settle();

    let r = record(&b);
    assert!(r.partial);
    assert!(r.changed.is_empty());
    assert_eq!(r.children, [caption]);
}

#[test]
fn many_changes_one_full_record() {
    let count = MAX_COMPOSITE_AREAS + 4;
    let mut b = build(count, CacheType::ImageComposite, |_, c| c);
    for &card in &b.cards.clone() {
        b.host.ui.tree.any_mut(card).unwrap().set_background_color(Color::RED);
    }
    b.host.settle();

    assert!(!record(&b).partial);
    same_as_full_render(&mut b, count, |_, c| c.background_color(Color::RED));
}
