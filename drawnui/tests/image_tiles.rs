//! SkiaImageTiles: the picture as a tile repeated over the box, shifted by the offsets
//! (React `SkiaImageTiles.ts`).

mod image_common;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use image_common::*;

#[derive(Default)]
struct App {
    tiles: Handle<SkiaImageTiles>,
}

fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        SkiaLayout::new().fill().children((SkiaImageTiles::new("q.png")
            .tile_width(64)
            .tile_height(64)
            .width_request(200)
            .height_request(100)
            .background_color(Color::BLACK)
            .assign(&mut app.tiles),))
    });
    let mut host = Headless::new(ui.background(Color::from_rgb(9, 9, 9)), 240, 140, 1.0);
    host.settle();
    host
}

#[test]
fn the_picture_is_one_tile_repeated_and_the_offsets_shift_the_grid() {
    let mut host = host();
    // Nothing loaded: the box is the background only.
    assert_eq!(host.pixel(10, 10), Color::BLACK);
    let asked = host.deliver_images(|_| Some(quadrants(40, 20)));
    assert_eq!(asked.len(), 1);
    host.settle();
    // 40 x 20 covers a 64 x 64 tile at 3.2: 128 x 64 centered, the quadrants meet at the tile's middle.
    for (x, y, color) in [(10, 10, Color::RED), (50, 10, Color::GREEN), (10, 50, Color::BLUE), (50, 50, Color::YELLOW)] {
        assert_eq!(host.pixel(x, y), color, "({x}, {y})");
        assert_eq!(host.pixel(x + 64, y), color, "next tile ({x}, {y})");
        assert_eq!(host.pixel(x + 128, y), color, "third tile ({x}, {y})");
    }
    // Clipped to the box.
    assert_eq!(host.pixel(210, 10), Color::from_rgb(9, 9, 9));
    assert_eq!(host.pixel(10, 110), Color::from_rgb(9, 9, 9));

    // Shifted by 16 points: the grid starts one tile before the box, the edges are at 16 and 80.
    host.ui.tree.get_mut(host.ui.state.tiles).unwrap().set_tile_offset_x(16);
    host.settle();
    assert_eq!(host.pixel(26, 10), Color::RED);
    assert_eq!(host.pixel(66, 10), Color::GREEN);
    assert_eq!(host.pixel(10, 10), Color::GREEN);
    // Offsets wrap: 80 is 16 again.
    host.ui.tree.get_mut(host.ui.state.tiles).unwrap().set_tile_offset_x(80);
    host.settle();
    assert_eq!(host.pixel(26, 10), Color::RED);
    assert_eq!(host.pixel(66, 10), Color::GREEN);
}
