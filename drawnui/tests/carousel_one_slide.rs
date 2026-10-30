//! One finger gesture moves a SkiaCarousel at most one slide, as iOS paging and the Android
//! ViewPager do: from slide N a release lands on N-1, N or N+1, the velocity only decides between
//! staying and the neighbour. Ports CarouselOneSlidePerSwipeTests (DrawnUi.Net.Tests, C# 1070e0db):
//! a drag past the middle followed by a flick the same way used to skip a slide (0 to 2).

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const WIDTH: f32 = 400.0;
const Y: f32 = 125.0;

#[derive(Default)]
struct App {
    carousel: Handle<SkiaCarousel>,
}

type Host = Headless<App>;

fn create(looped: bool) -> Host {
    let ui = Ui::new(App::default(), move |app: &mut App| {
        let slides: Vec<_> = (0..3).map(|_| SkiaShape::new().fill().background_color(Color::from_rgb(72, 61, 139))).collect();
        SkiaLayer::new().fill().children((SkiaCarousel::new()
            .height_request(250)
            .fill_x()
            .bounces(false)
            .is_looped(looped)
            .linear_speed_ms(420)
            .assign(&mut app.carousel)
            .children(slides),))
    });
    let mut host = Headless::new(ui.background(Color::BLACK), WIDTH as i32, 300, 1.0);
    host.settle();
    host
}

fn carousel(host: &Host) -> &SkiaCarousel {
    host.ui.tree.find(host.ui.state.carousel).unwrap()
}

fn frames(host: &mut Host, count: u32) {
    for _ in 0..count {
        host.frame_after(16.0);
    }
}

/// One gesture: a slow drag of `ratio` of a slide (20 moves, 20 ms apart), then without lifting a
/// flick of 4 fast moves the same way (30 points, 8 ms apart). `dir` -1 = finger to the left (next
/// slide), +1 = to the right (previous slide).
fn drag_then_flick(host: &mut Host, ratio: f32, dir: f32, flick: bool) {
    let mut x = if dir < 0.0 { WIDTH - 20.0 } else { 20.0 };
    let drag = ratio * WIDTH;
    host.ui.pointer(PointerKind::Down, x, Y, host.time_ms());
    for i in 1..=20 {
        host.frame_after(20.0);
        host.ui.pointer(PointerKind::Move, x + dir * drag * i as f32 / 20.0, Y, host.time_ms());
    }
    x += dir * drag;
    if flick {
        for i in 1..=4 {
            host.frame_after(8.0);
            host.ui.pointer(PointerKind::Move, x + dir * 30.0 * i as f32, Y, host.time_ms());
        }
        x += dir * 120.0;
    }
    host.frame_after(8.0);
    host.ui.pointer(PointerKind::Up, x, Y, host.time_ms());
    frames(host, 90);
}

fn assert_on(host: &Host, index: usize, what: &str) {
    let c = carousel(host);
    assert_eq!(c.selected_index(), index, "{what}: at {:?}", c.current_position());
    assert_eq!(c.current_position(), c.snap_points()[index], "{what}");
}

#[test]
fn half_drag_then_flick_from_the_first_slide_lands_on_the_second() {
    for ratio in [0.45, 0.5, 0.55, 0.7] {
        let mut host = create(false);
        assert_eq!(carousel(&host).selected_index(), 0);
        drag_then_flick(&mut host, ratio, -1.0, true);
        assert_on(&host, 1, &format!("drag {ratio}"));
    }
}

#[test]
fn half_drag_then_flick_back_from_the_last_slide_lands_on_the_middle() {
    let mut host = create(false);
    host.ui.tree.get_mut(host.ui.state.carousel).unwrap().scroll_to(2, false);
    frames(&mut host, 4);
    assert_on(&host, 2, "start");
    drag_then_flick(&mut host, 0.6, 1.0, true);
    assert_on(&host, 1, "back");
}

#[test]
fn looped_half_drag_then_flick_moves_one_slide() {
    let mut host = create(true);
    drag_then_flick(&mut host, 0.6, -1.0, true);
    assert_eq!(carousel(&host).selected_index(), 1);
}

#[test]
fn slow_drag_past_the_middle_without_flick_goes_to_the_neighbour() {
    let mut host = create(false);
    drag_then_flick(&mut host, 0.7, -1.0, false);
    assert_eq!(carousel(&host).selected_index(), 1);
}

#[test]
fn code_moves_are_not_limited() {
    let mut host = create(false);
    host.ui.tree.get_mut(host.ui.state.carousel).unwrap().scroll_to(2, true);
    frames(&mut host, 90);
    assert_on(&host, 2, "scroll_to(2)");
}
