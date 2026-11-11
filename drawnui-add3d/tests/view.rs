//! What the gallery's product viewer, globe, list and loading screens need from SkiaModel3D: points of
//! the scene on screen (anchored labels, pins hidden on the far side), pinch and wheel zoom, going
//! back to the home view, drags left to a scroll, no time and no frames off screen, loading from a
//! url, and hearing when the view moved.

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui_add3d::prelude::*;

const DRAGON: &[u8] = include_bytes!("../../examples/model3d/assets/dragon.glb");

fn ball() -> Model3D {
    Model3D::new().part("ball", Mesh3D::sphere(1.0), Color::WHITE)
}

#[derive(Default)]
struct App {
    model: Handle<SkiaModel3D>,
    scroll: Handle<SkiaScroll>,
    views: u32,
    loads: Vec<LoadState3D>,
}

/// One 200 x 200 model at the top left of a 400 x 400 window, seen straight from the front.
fn single(model: Build<SkiaModel3D>) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaLayout::new().fill().children((model
            .width_request(200)
            .height_request(200)
            .assign(&mut app.model)
            .on_view_changed(|_me, app: &mut App, _cx| app.views += 1)
            .on_load_state(|_me, app: &mut App, _cx, state| app.loads.push(state)),))
    });
    let mut host = Headless::new(ui, 400, 400, 1.0);
    host.settle();
    host
}

fn model(host: &Headless<App>) -> &SkiaModel3D {
    host.ui.tree.find::<SkiaModel3D>(host.ui.state.model).unwrap()
}

#[test]
fn a_point_of_the_scene_lands_where_it_is_drawn_and_the_far_side_is_occluded() {
    let host = single(SkiaModel3D::from_model(&ball()).view_angles(0.0, 0.0));
    let model = model(&host);
    let front = model.screen_position([0.0, 0.0, 1.0]).expect("a laid out model");
    assert!((front.x - 100.0).abs() < 0.5 && (front.y - 100.0).abs() < 0.5, "the middle: {front:?}");
    assert!(!front.occluded, "the near side shows");
    let back = model.screen_position([0.0, 0.0, -1.0]).unwrap();
    assert!(back.occluded, "the far side is behind the ball");
    // Up in the scene is up on screen; right is right.
    let top = model.screen_position([0.0, 1.0, 0.0]).unwrap();
    let right = model.screen_position([1.0, 0.0, 0.0]).unwrap();
    assert!(top.y < 100.0 && right.x > 100.0, "{top:?} {right:?}");
}

#[test]
fn pinch_and_wheel_zoom_only_when_asked_and_stay_in_range() {
    let mut host = single(SkiaModel3D::from_model(&ball()).view_angles(0.0, 0.0));
    assert!(!host.wheel(100.0, 100.0, 3.0), "the wheel is the scroll's while pinch zoom is off");
    assert_eq!(model(&host).view().2, 1.0);

    let mut host = single(SkiaModel3D::from_model(&ball()).view_angles(0.0, 0.0).pinch_zoom(true).min_zoom(0.6).max_zoom(2.0));
    assert!(host.wheel(100.0, 100.0, 1.0));
    assert!(model(&host).view().2 < 1.0, "the wheel away from the user brings it closer");
    for _ in 0..20 {
        host.wheel(100.0, 100.0, 1.0);
    }
    assert_eq!(model(&host).view().2, 0.6, "no closer than min_zoom");
    host.pinch((100.0, 100.0), 40.0, 10.0, 160.0, 8);
    assert!(model(&host).view().2 > 0.6, "fingers together move it away");
    host.pinch((100.0, 100.0), 10.0, 160.0, 160.0, 8);
    host.pinch((100.0, 100.0), 160.0, 10.0, 160.0, 8);
    host.pinch((100.0, 100.0), 160.0, 10.0, 160.0, 8);
    assert_eq!(model(&host).view().2, 2.0, "no farther than max_zoom");
}

#[test]
fn reset_view_glides_back_to_the_view_the_app_set_and_two_taps_do_it_too() {
    let mut host = single(SkiaModel3D::from_model(&ball()).view_angles(30.0, 10.0).pinch_zoom(true).reset_on_double_tap(true));
    let views = host.ui.state.views;
    host.pan((60.0, 100.0), (140.0, 140.0), 160.0, 8);
    host.wheel(100.0, 100.0, 2.0);
    host.settle();
    let moved = model(&host).view();
    assert!(moved != (30.0, 10.0, 1.0), "{moved:?}");
    assert!(host.ui.state.views > views, "the app heard the view move");

    host.ui.tree.get_mut(host.ui.state.model).unwrap().reset_view();
    host.settle();
    let (yaw, pitch, zoom) = model(&host).view();
    assert!((yaw.rem_euclid(360.0) - 30.0).abs() < 0.01 && (pitch - 10.0).abs() < 0.01 && (zoom - 1.0).abs() < 1e-4, "home: {yaw} {pitch} {zoom}");

    host.pan((60.0, 100.0), (140.0, 100.0), 160.0, 8);
    host.settle();
    host.tap(100.0, 100.0);
    host.tap(100.0, 100.0);
    host.settle();
    let (yaw, _, _) = model(&host).view();
    assert!((yaw.rem_euclid(360.0) - 30.0).abs() < 0.01, "two taps went home: {yaw}");
}

#[test]
fn a_set_view_becomes_home_and_the_app_hears_it() {
    let mut host = single(SkiaModel3D::from_model(&ball()));
    let views = host.ui.state.views;
    host.ui.tree.get_mut(host.ui.state.model).unwrap().set_view_angles(90.0, 0.0);
    host.settle();
    assert_eq!(host.ui.state.views, views + 1, "one change, heard once");
    host.pan((60.0, 100.0), (140.0, 100.0), 160.0, 8);
    host.ui.tree.get_mut(host.ui.state.model).unwrap().reset_view();
    host.settle();
    assert!((model(&host).view().0.rem_euclid(360.0) - 90.0).abs() < 0.01);
}

/// A column of 3D rows in a 400 x 400 scroll, each row 150 tall; the model is the first row's
/// and turns by itself.
fn list(lock: bool) -> Headless<App> {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        SkiaScroll::new().fill().assign(&mut app.scroll).content(SkiaLayout::column().fill_x().children(
            (0..8)
                .map(|i| {
                    let row = SkiaModel3D::from_model(&ball()).fill_x().height_request(150).lock_pitch(lock);
                    if i == 0 { row.auto_rotate(90.0).assign(&mut app.model) } else { row }
                })
                .collect::<Vec<_>>(),
        ))
    });
    Headless::new(ui, 400, 400, 1.0)
}

/// How far the list scrolled, points (positive = down the list).
fn offset(host: &Headless<App>) -> f32 {
    -host.ui.tree.find::<SkiaScroll>(host.ui.state.scroll).unwrap().viewport_offset_y()
}

#[test]
fn with_lock_pitch_a_vertical_drag_scrolls_the_list_and_a_sideways_one_turns_the_model() {
    let mut host = list(true);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    let pitch = model(&host).view().1;
    host.pan((200.0, 120.0), (204.0, 40.0), 160.0, 8);
    for _ in 0..40 {
        host.frame_after(16.0);
    }
    assert!(offset(&host) > 30.0, "the list scrolled: {}", offset(&host));
    assert_eq!(model(&host).view().1, pitch, "no tilt");

    let mut host = list(true);
    host.frame_after(16.0);
    let before = model(&host).view().0;
    host.pan((200.0, 75.0), (300.0, 80.0), 160.0, 8);
    assert!(offset(&host).abs() < 1.0, "the list stayed: {}", offset(&host));
    assert!((model(&host).view().0 - before).abs() > 20.0, "the model turned: {before} -> {:?}", model(&host).view());

    // Without the lock, a vertical drag on the model is the model's.
    let mut host = list(false);
    host.frame_after(16.0);
    host.pan((200.0, 120.0), (204.0, 40.0), 160.0, 8);
    assert!(offset(&host).abs() < 1.0, "{}", offset(&host));
}

#[test]
fn a_model_scrolled_out_of_view_stands_still_and_asks_for_no_frames() {
    let mut host = list(true);
    for _ in 0..3 {
        host.frame_after(16.0);
    }
    let turning = model(&host).view().0;
    host.frame_after(100.0);
    assert!(model(&host).view().0 != turning, "it turns while on screen");

    // Scroll it out of view.
    host.pan((200.0, 380.0), (200.0, 20.0), 120.0, 6);
    for _ in 0..80 {
        host.frame_after(16.0);
    }
    assert!(offset(&host) > 160.0, "scrolled past the first row: {}", offset(&host));
    assert!(!host.ui.needs_frame(), "nothing asks for frames");
    let stopped = model(&host).view().0;
    host.frame_after(1000.0);
    assert_eq!(model(&host).view().0, stopped, "time stands still off screen");

    // Back into view: it turns again.
    host.pan((200.0, 20.0), (200.0, 380.0), 120.0, 6);
    for _ in 0..80 {
        host.frame_after(16.0);
    }
    assert!(offset(&host) < 140.0, "{}", offset(&host));
    assert!(host.ui.needs_frame(), "it asks for frames again");
    let back = model(&host).view().0;
    host.frame_after(100.0);
    assert!(model(&host).view().0 != back, "it turns again");
}

#[test]
fn a_model_from_a_url_tells_loading_then_ready_or_failed() {
    let mut host = single(SkiaModel3D::from_url("models/dragon.glb"));
    assert_eq!(model(&host).load_state(), LoadState3D::Loading);
    assert!(model(&host).screen_position([0.0, 0.0, 0.0]).is_none(), "nothing to place before it is here");
    let asked = host.deliver_assets(|url| (url == "models/dragon.glb").then(|| DRAGON.to_vec()));
    assert_eq!(asked, ["models/dragon.glb"]);
    host.frame_after(16.0);
    host.frame_after(16.0);
    assert_eq!(model(&host).load_state(), LoadState3D::Ready);
    assert!(model(&host).animations().len() == 8, "the file's moves are there");
    assert_eq!(host.ui.state.loads, [LoadState3D::Loading, LoadState3D::Ready]);

    let mut host = single(SkiaModel3D::from_url("models/missing.glb"));
    host.deliver_assets(|_| None);
    host.settle();
    assert_eq!(model(&host).load_state(), LoadState3D::Failed);
    assert_eq!(host.ui.state.loads, [LoadState3D::Loading, LoadState3D::Failed]);

    // Another url later, from code.
    host.ui.tree.get_mut(host.ui.state.model).unwrap().set_source("models/dragon.glb");
    host.frame_after(16.0);
    host.deliver_assets(|_| Some(DRAGON.to_vec()));
    host.frame_after(16.0);
    host.frame_after(16.0);
    assert_eq!(model(&host).load_state(), LoadState3D::Ready);
    assert_eq!(host.ui.state.loads.last(), Some(&LoadState3D::Ready));
}
