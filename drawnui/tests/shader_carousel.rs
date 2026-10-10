//! SkiaShaderCarousel on the CPU canvas: the transition blends the caches of the slide it comes
//! from and the one it goes to by the position; slides never show moving; the looped wrap; the
//! plain carousel while no texture exists.

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FADE: &str = include_str!("shaders/transitions/fade.sksl");
const COLORS: [Color; 3] = [Color::RED, Color::GREEN, Color::BLUE];

/// A 100 x 60 carousel at (40, 20) with a red, a green and a blue slide.
fn scene(looped: bool, cached: bool) -> (Headless<()>, Handle<SkiaShaderCarousel>) {
    register_shader_source("fade.sksl", FADE);
    let mut carousel = Handle::default();
    let slides: Vec<_> = COLORS
        .iter()
        .map(|c| SkiaLayout::new().background_color(*c).use_cache(if cached { CacheType::Image } else { CacheType::None }))
        .collect();
    let built = SkiaShaderCarousel::new()
        .horizontal_options(LayoutOptions::Start)
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .is_looped(looped)
        .transition_shader("fade.sksl")
        .children(slides)
        .assign(&mut carousel);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(built)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    (host, carousel)
}

fn near(a: Color, b: Color, tolerance: i32) -> bool {
    let d = |x: u8, y: u8| (x as i32 - y as i32).abs() <= tolerance;
    d(a.r(), b.r()) && d(a.g(), b.g()) && d(a.b(), b.b())
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn transition(host: &mut Headless<()>, carousel: Handle<SkiaShaderCarousel>) -> (usize, usize, f32) {
    let c = host.ui.tree.get_mut(carousel).unwrap();
    (c.transition_from_index().unwrap(), c.transition_to_index().unwrap(), c.transition_progress().unwrap())
}

#[test]
fn a_pan_blends_the_slides_it_goes_between_and_they_never_move() {
    let (mut host, carousel) = scene(false, true);
    assert_eq!(transition(&mut host, carousel), (0, 1, 0.0));
    assert_eq!(host.pixel(45, 50), Color::RED);
    assert_eq!(host.pixel(135, 50), Color::RED);

    // Half a slide to the left, the finger still down.
    let t = host.time_ms();
    host.ui.pointer(PointerKind::Down, 120.0, 50.0, t);
    host.frame_after(16.0);
    for x in [110.0, 95.0, 80.0, 70.0] {
        let t = host.time_ms();
        host.ui.pointer(PointerKind::Move, x, 50.0, t);
        host.frame_after(16.0);
    }
    let (from, to, progress) = transition(&mut host, carousel);
    assert_eq!((from, to), (0, 1));
    let position = host.ui.tree.get_mut(carousel).unwrap().carousel().current_position().x;
    assert!((progress - -position / 100.0).abs() < 1e-4 && progress > 0.3, "{progress} at {position}");
    // Every pixel is the blend: not red on the left and green on the right.
    let blend = mix(Color::RED, Color::GREEN, progress);
    for x in [41, 90, 139] {
        assert!(near(host.pixel(x, 50), blend, 2), "({x}, 50): {:?} vs {blend:?}", host.pixel(x, 50));
    }

    // Let go: it snaps on to the green slide.
    let t = host.time_ms();
    host.ui.pointer(PointerKind::Up, 70.0, 50.0, t);
    host.settle();
    assert_eq!(transition(&mut host, carousel), (1, 2, 0.0));
    assert_eq!(host.pixel(45, 50), Color::GREEN);
    assert_eq!(host.pixel(135, 50), Color::GREEN);
}

#[test]
fn a_looped_carousel_goes_from_the_last_slide_to_the_first() {
    let (mut host, carousel) = scene(true, true);
    host.ui.tree.get_mut(carousel).unwrap().set_selected_index(2);
    host.settle();
    assert_eq!(host.pixel(90, 50), Color::BLUE);
    host.ui.tree.get_mut(carousel).unwrap().go_next();
    // On the way from blue to red, through the virtual slide after the last.
    host.frame_after(16.0);
    host.frame_after(100.0);
    let (from, to, progress) = transition(&mut host, carousel);
    assert_eq!((from, to), (2, 0), "{progress}");
    assert!(progress > 0.0 && progress < 1.0);
    assert!(near(host.pixel(90, 50), mix(Color::BLUE, Color::RED, progress), 2), "{:?}", host.pixel(90, 50));
    host.settle();
    assert_eq!(host.ui.tree.get_mut(carousel).unwrap().carousel().selected_index(), 0);
    assert_eq!(host.pixel(90, 50), Color::RED);
}

#[test]
fn without_slide_caches_it_is_a_plain_carousel() {
    let (mut host, carousel) = scene(false, false);
    host.ui.tree.get_mut(carousel).unwrap().set_selected_index(1);
    host.frame_after(16.0);
    host.frame_after(100.0);
    // The slides move: red leaves on the left while green comes in from the right.
    let (left, right) = (host.pixel(41, 50), host.pixel(139, 50));
    assert!(left == Color::RED && right == Color::GREEN, "{left:?} {right:?}");
}

#[test]
fn templated_slides_show_at_rest_and_after_a_move() {
    // Items with the default RecyclingTemplate: the first slide shows when the carousel opens.
    register_shader_source("fade.sksl", FADE);
    let mut carousel = Handle::default();
    let built = SkiaShaderCarousel::new()
        .horizontal_options(LayoutOptions::Start)
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .transition_shader("fade.sksl")
        .items(
            |_: &()| COLORS.len(),
            || {
                let mut slide = Handle::<SkiaLayout>::default();
                (SkiaLayout::new().use_cache(CacheType::Image).assign(&mut slide), slide)
            },
            |slide, _, index, cx| {
                if let Some(mut slide) = cx.get_mut(*slide) {
                    slide.set_background_color(COLORS[index]);
                }
            },
        )
        .assign(&mut carousel);
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(built)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    assert_eq!(host.pixel(45, 50), Color::RED);
    assert_eq!(host.pixel(135, 50), Color::RED);
    host.ui.tree.get_mut(carousel).unwrap().go_next();
    host.settle();
    assert_eq!(transition(&mut host, carousel), (1, 2, 0.0));
    assert_eq!(host.pixel(90, 50), Color::GREEN);
}

#[test]
fn another_transition_is_compiled_when_set() {
    register_shader_source("cut.sksl", "vec4 transition(vec2 uv) { return progress < 0.5 ? getFromColor(uv) : getToColor(uv); }\nhalf4 main(float2 p) { return transition(float2(0.5)); }");
    let (mut host, carousel) = scene(false, true);
    host.ui.tree.get_mut(carousel).unwrap().set_transition_shader("cut.sksl");
    host.settle();
    let mut c = host.ui.tree.get_mut(carousel).unwrap();
    let effect = c.effect_mut::<CarouselTransition>().unwrap();
    assert!(effect.shader.is_compiled(), "{:?}", effect.shader.error());
    assert!(effect.shader.loaded_code().starts_with("vec4 transition"));
}

#[test]
fn every_slide_shows_at_rest_also_the_last_of_a_looped_carousel() {
    // The cube: at progress 0 it shows the slide it comes from, anything else is its black floor.
    register_shader_source("cube.sksl", include_str!("shaders/transitions/cube.sksl"));
    let (mut host, carousel) = scene(true, true);
    host.ui.tree.get_mut(carousel).unwrap().set_transition_shader("cube.sksl");
    host.settle();
    // Round the loop twice: the end slides move to the other side and back meanwhile.
    for step in 1..=6 {
        host.ui.tree.get_mut(carousel).unwrap().go_next();
        host.settle();
        let index = step % 3;
        assert_eq!(transition(&mut host, carousel).0, index);
        assert_eq!(host.pixel(90, 50), COLORS[index], "slide {index}");
    }
}

/// DrawnUI FromToChanged: runs when the transition is between other slides.
#[test]
fn from_to_changed_runs_when_the_slides_of_the_transition_change() {
    register_shader_source("fade.sksl", FADE);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    let mut carousel = Handle::default();
    let slides: Vec<_> = COLORS.iter().map(|c| SkiaLayout::new().background_color(*c).use_cache(CacheType::Image)).collect();
    let built = SkiaShaderCarousel::new()
        .horizontal_options(LayoutOptions::Start)
        .width_request(100)
        .height_request(60)
        .margin(Thickness::new(40.0, 20.0, 0.0, 0.0))
        .transition_shader("fade.sksl")
        .children(slides)
        .assign(&mut carousel)
        .on_from_to_changed(move |me, _app: &mut (), cx| {
            let c = cx.find::<SkiaShaderCarousel>(me).unwrap();
            log.borrow_mut().push((c.transition_from_index(), c.transition_to_index()));
        });
    let ui = Ui::new((), |_| SkiaLayout::new().fill().children(built)).background(Color::BLACK);
    let mut host = Headless::new(ui, 200, 100, 1.0);
    host.settle();
    seen.borrow_mut().clear();
    // To the second slide and past it: the transition goes 0 -> 1, then 1 -> 2.
    host.ui.tree.get_mut(carousel).unwrap().go_next();
    host.settle();
    host.ui.tree.get_mut(carousel).unwrap().go_next();
    host.settle();
    let seen = seen.borrow();
    assert!(seen.contains(&(Some(1), Some(2))), "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0] != w[1]), "only changes: {seen:?}");
}
