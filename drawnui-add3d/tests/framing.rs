//! A model viewer fits its model tightly: never cut, whatever the view's shape, turn or tilt, with
//! a little air (DrawnUI .NET FramingTests).

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui_add3d::prelude::*;

#[derive(Default)]
struct App {
    model: Handle<SkiaModel3D>,
}

fn host(model: Build<SkiaModel3D>, width: i32, height: i32) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| SkiaLayout::new().fill().children((model.fill().assign(&mut app.model),)));
    let mut host = Headless::new(ui, width, height, 1.0);
    host.settle();
    host
}

fn corners(half: [f32; 3], margin: f32) -> Vec<[f32; 3]> {
    let h = half.map(|v| v + margin);
    (0..8).map(|i| [if i & 1 == 0 { -h[0] } else { h[0] }, if i & 2 == 0 { -h[1] } else { h[1] }, if i & 4 == 0 { -h[2] } else { h[2] }]).collect()
}

#[test]
fn the_whole_box_stays_in_view_and_fills_it() {
    let cases = [
        (800, 250, 0.0, 0.0, true),
        (800, 250, 37.0, 30.0, true),
        (200, 400, 0.0, 0.0, true),
        (200, 400, 63.0, 60.0, true),
        (300, 300, 125.0, -40.0, true),
        // Nobody turns it: fitted exactly as seen.
        (800, 250, 0.0, 32.0, false),
        (200, 400, 0.0, 32.0, false),
        (300, 300, 125.0, -40.0, false),
    ];
    for (width, height, yaw, pitch, orbit) in cases {
        let box3 = Model3D::new().part("box", Mesh3D::cuboid(2.6, 1.0, 1.0), Color::WHITE);
        let host = host(SkiaModel3D::from_model(&box3).view_angles(yaw, pitch).frame_margin(0.3).orbit(orbit), width, height);
        let model = host.ui.tree.find::<SkiaModel3D>(host.ui.state.model).unwrap();
        let at: Vec<_> = corners([1.3, 0.5, 0.5], 0.3).into_iter().map(|p| model.screen_position(p).unwrap()).collect();
        let (min_x, max_x) = at.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        let (min_y, max_y) = at.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
        let (w, h) = (width as f32, height as f32);
        let case = format!("{width}x{height} yaw {yaw} pitch {pitch} orbit {orbit}: x {min_x:.0}..{max_x:.0}, y {min_y:.0}..{max_y:.0}");
        assert!(min_x >= -0.5 && max_x <= w + 0.5 && min_y >= -0.5 && max_y <= h + 0.5, "cut: {case}");
        // And tight: the box spans most of the view along the side that limits it.
        let fill = if orbit { 0.55 } else { 0.75 };
        assert!((max_x - min_x) / w > fill || (max_y - min_y) / h > fill, "loose: {case}");
    }
}

/// Turning a globe to a city tilts it: the globe keeps its size on screen.
#[test]
fn a_ball_keeps_its_size_when_tilted() {
    let ball = Model3D::new().part("earth", Mesh3D::sphere(1.0), Color::WHITE);
    let mut host = host(SkiaModel3D::from_model(&ball).view_angles(0.0, 0.0), 600, 400);
    let width = |host: &Headless<App>| {
        let model = host.ui.tree.find::<SkiaModel3D>(host.ui.state.model).unwrap();
        let yaw = model.view().0.to_radians();
        // The ball's edge across, seen from the camera: the point at a right angle to the view.
        let right = [yaw.cos(), 0.0, -yaw.sin()];
        (model.screen_position(right).unwrap().x - model.screen_position(right.map(|v| -v)).unwrap().x).abs()
    };
    let level = width(&host);
    for pitch in [20.0, 45.0, 70.0, -40.0] {
        host.ui.tree.get_mut(host.ui.state.model).unwrap().set_view_angles(0.0, pitch);
        host.frame_after(16.7);
        let w = width(&host);
        assert!((w - level).abs() <= level * 0.03, "pitch {pitch}: {w} vs {level}");
    }
}
