//! 3D transforms as DrawnUi.Net 1e4610f6: RotationX / RotationY / RotationZ seen with perspective,
//! TranslationZ as depth, image caches blitted smooth once something distorts them (their own
//! transform, a live ancestor's, or the picture they are recorded into), and taps through the
//! inverse of the 3D matrix. On the CPU canvas.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const W: i32 = 400;
const H: i32 = 300;

fn host<S: 'static, T: Control>(state: S, content: impl Fn(&mut S) -> Build<T> + 'static) -> Headless<S> {
    let ui = Ui::new(state, move |s| SkiaLayout::new().fill().children(content(s))).background(Color::WHITE);
    let mut host = Headless::new(ui, W, H, 1.0);
    host.settle();
    host
}

fn pixels<S>(host: &mut Headless<S>) -> Vec<Color> {
    (0..H).flat_map(|y| (0..W).map(move |x| (x, y))).map(|(x, y)| host.pixel(x, y)).collect()
}

/// Pixels whose channels differ by more than `tolerance`.
fn differing(a: &[Color], b: &[Color], tolerance: u8) -> usize {
    let far = |x: u8, y: u8| x.abs_diff(y) > tolerance;
    a.iter().zip(b).filter(|(a, b)| far(a.r(), b.r()) || far(a.g(), b.g()) || far(a.b(), b.b()) || far(a.a(), b.a())).count()
}

/// Width of the ink on row `y`, edge pixels counted by their coverage.
fn ink_on_row<S>(host: &mut Headless<S>, y: i32) -> f32 {
    (0..W).map(|x| 1.0 - host.pixel(x, y).g() as f32 / 255.0).sum()
}

#[test]
fn translation_z_is_depth_on_a_flat_control() {
    // 576 / (576 - z): -300 = 0.66x, +150 = 1.35x, +300 = 2.09x.
    for (z, width) in [(-300.0, 65.75), (0.0, 100.0), (150.0, 135.21), (300.0, 208.70)] {
        let mut host = host((), move |_| SkiaLayout::new().width_request(100).height_request(100).center().background_color(Color::RED).translation_z(z));
        let drawn = ink_on_row(&mut host, H / 2);
        assert!((drawn - width).abs() <= 0.25, "TranslationZ {z}: {drawn} px wide, expected {width}");
    }
}

#[test]
fn rotation_y_turns_with_perspective_and_rotation_z_turns_as_rotation() {
    // RotationY 60 around the center: the right half goes away from the viewer, so it draws
    // narrower than the left one (cos 60 = 0.5 without perspective: 50 px each side).
    let mut card = host((), |_| SkiaLayout::new().width_request(200).height_request(100).center().background_color(Color::RED).rotation_y(60));
    let row: Vec<i32> = (0..W).filter(|&x| card.pixel(x, H / 2) == Color::RED).collect();
    let (left, right) = (W / 2 - row[0], row[row.len() - 1] + 1 - W / 2);
    assert!(left > 52 && right < 48, "left half {left} px, right half {right} px");

    let bar = |rotation: f32, rotation_z: f32| {
        let mut bar = host((), move |_| {
            SkiaLayout::new().width_request(160).height_request(20).center().background_color(Color::RED).rotation(rotation).rotation_z(rotation_z)
        });
        pixels(&mut bar)
    };
    let (turned, turned_z) = (bar(30.0, 0.0), bar(0.0, 30.0));
    assert_eq!(differing(&turned, &turned_z, 8), 0, "RotationZ 30 draws as Rotation 30");
}

/// A card turned like the hello app's tilted strip, holding a tile with sharp edges inside.
fn tilted(card_cache: CacheType, tile_cache: CacheType, turned: bool) -> Build<SkiaLayout> {
    let tile = SkiaLayout::new()
        .width_request(120)
        .height_request(60)
        .center()
        .background_color(Color::BLUE)
        .use_cache(tile_cache)
        .children(SkiaLayout::new().width_request(40).height_request(20).center().background_color(Color::YELLOW));
    let card = SkiaLayout::new().width_request(200).height_request(120).center().background_color(Color::from_rgb(17, 24, 39)).use_cache(card_cache);
    let card = if turned { card.rotation_x(16).rotation_y(20).rotation(-6) } else { card };
    card.children(tile)
}

#[test]
fn an_image_cache_inside_a_turned_picture_is_smooth() {
    let mut live = host((), |_| tilted(CacheType::Operations, CacheType::None, true));
    let live = pixels(&mut live);
    // The card records a picture (SkiaShape's default cache) that replays the tile's bitmap
    // under the card's 3D matrix; the same under a live (uncached) card.
    for card in [CacheType::Operations, CacheType::None] {
        let mut cached = host((), move |_| tilted(card, CacheType::Image, true));
        let cached = pixels(&mut cached);
        let off = differing(&live, &cached, 48);
        // Nearest sampling and an aliased border: 252 pixels off by more than 48 (382 by more than 16).
        assert_eq!(off, 0, "card {card:?}: {off} pixels differ from the uncached tile");
        let close = differing(&live, &cached, 16);
        assert!(close <= 20, "card {card:?}: {close} pixels differ by more than 16");
    }
}

#[test]
fn a_picture_with_image_blits_records_again_when_its_playback_turns() {
    struct Ids {
        card: Handle<SkiaLayout>,
    }
    let mut host = host(Ids { card: Handle::default() }, |s| tilted(CacheType::Operations, CacheType::Image, false).assign(&mut s.card));
    let card = host.ui.state.card;
    let records = host.cache_records(card);

    // The card starts to turn: its picture was recorded for a straight playback.
    host.ui.tree.get_mut(card).unwrap().set_rotation_x(16);
    host.ui.tree.get_mut(card).unwrap().set_rotation_y(20);
    host.ui.tree.get_mut(card).unwrap().set_rotation(-6);
    host.settle();
    assert_eq!(host.cache_records(card), records + 1, "recorded again for a distorted playback");
    let turned = pixels(&mut host);
    let mut live = Headless::new(Ui::new((), |_| SkiaLayout::new().fill().children(tilted(CacheType::Operations, CacheType::None, true))).background(Color::WHITE), W, H, 1.0);
    live.settle();
    let off = differing(&pixels(&mut live), &turned, 48);
    assert_eq!(off, 0, "{off} pixels differ from the uncached tile");

    // Turning on records nothing more; back to flat records it again, sharp.
    host.ui.tree.get_mut(card).unwrap().set_rotation_y(30);
    host.settle();
    assert_eq!(host.cache_records(card), records + 1);
    host.ui.tree.get_mut(card).unwrap().set_rotation_x(0);
    host.ui.tree.get_mut(card).unwrap().set_rotation_y(0);
    host.ui.tree.get_mut(card).unwrap().set_rotation(0);
    host.settle();
    assert_eq!(host.cache_records(card), records + 2, "recorded again for a straight playback");
}

#[test]
fn a_picture_without_image_blits_never_records_again_for_a_turn() {
    struct Ids {
        card: Handle<SkiaLayout>,
    }
    let mut host = host(Ids { card: Handle::default() }, |s| tilted(CacheType::Operations, CacheType::None, false).assign(&mut s.card));
    let card = host.ui.state.card;
    let records = host.cache_records(card);
    host.ui.tree.get_mut(card).unwrap().set_rotation_y(40);
    host.settle();
    host.ui.tree.get_mut(card).unwrap().set_rotation_y(0);
    host.settle();
    assert_eq!(host.cache_records(card), records);
}

#[test]
fn taps_go_through_the_inverse_of_the_3d_matrix() {
    #[derive(Default)]
    struct Taps {
        n: u32,
    }
    let tap_at = |x: f32, build: fn() -> Build<SkiaLayout>| {
        let mut host = host(Taps::default(), move |_| build().on_tapped(|_me, taps: &mut Taps, _cx| taps.n += 1));
        host.tap(x, H as f32 / 2.0);
        host.settle();
        host.ui.state.n
    };
    // 200 wide at the center, RotationY 60: the right edge draws about 43 px right of the center.
    let turned = || SkiaLayout::new().width_request(200).height_request(100).center().background_color(Color::RED).rotation_y(60);
    assert_eq!(tap_at(200.0 + 38.0, turned), 1, "inside the drawn card");
    assert_eq!(tap_at(200.0 + 70.0, turned), 0, "inside the layout box, outside the drawn card");
    // 100 wide, TranslationZ 150: drawn 135 wide, so 60 px right of the center is on it.
    let near = || SkiaLayout::new().width_request(100).height_request(100).center().background_color(Color::RED).translation_z(150);
    assert_eq!(tap_at(200.0 + 60.0, near), 1, "on the bigger drawn card");
}
