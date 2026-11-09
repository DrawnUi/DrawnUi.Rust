//! SkewX / SkewY of either sign (DrawnUi.Net d7759667: a negative skew was ignored there). A negative
//! skew draws and hits as the mirror of the positive one around the control's center.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    taps: u32,
}

/// A 100 x 100 red square in the middle of a 300 x 300 canvas, skewed.
fn host(skew_x: f32, skew_y: f32) -> Headless<App> {
    let ui = Ui::new(App::default(), move |_| {
        SkiaLayout::new().fill().children((SkiaShape::new()
            .width_request(100)
            .height_request(100)
            .center()
            .background_color(Color::RED)
            .skew_x(skew_x)
            .skew_y(skew_y)
            .on_tapped(|_me, app: &mut App, _cx| app.taps += 1),))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    host
}

fn red(host: &mut Headless<App>) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..300 {
        for x in 0..300 {
            if host.pixel(x, y).r() > 128 {
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
fn a_negative_skew_draws_as_the_mirror_of_the_positive_one() {
    for (kx, ky) in [(30.0, 0.0), (0.0, 30.0), (20.0, 15.0)] {
        let (mut pos, mut neg) = (host(kx, ky), host(-kx, -ky));
        let (p, n) = (red(&mut pos), red(&mut neg));
        let square = red(&mut host(0.0, 0.0));
        assert_ne!(p, square, "({kx}, {ky}) skews");
        assert_ne!(n, square, "({}, {}) skews", -kx, -ky);
        // Mirrored left to right, pixel for pixel (a pixel at x covers x..x+1: its mirror is 299 - x).
        let mut mirrored: Vec<(i32, i32)> = p.iter().map(|&(x, y)| (299 - x, y)).collect();
        mirrored.sort();
        let mut n_sorted = n.clone();
        n_sorted.sort();
        let diff = mirrored.iter().filter(|p| n_sorted.binary_search(p).is_err()).count();
        assert!(diff <= p.len() / 100, "({kx}, {ky}): {diff} of {} pixels differ from the mirror", p.len());
    }
}

#[test]
fn a_negative_skew_hits_where_it_draws() {
    // tan 30 = 0.577: 45 points below the center the sheared edge moves 26 points sideways.
    for (k, x) in [(30.0, 220.0), (-30.0, 80.0)] {
        let mut host = host(k, 0.0);
        host.tap(x, 195.0);
        assert_eq!(host.ui.state.taps, 1, "skew {k}: a tap on the sheared corner at x {x}");
        host.tap(300.0 - x, 195.0);
        assert_eq!(host.ui.state.taps, 1, "skew {k}: the corner it left is empty");
    }
}
