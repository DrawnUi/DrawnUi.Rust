//! The gesture router's hooks for controls that ask their children first (React SkiaScroll:
//! `super.ProcessGestures` before its own decision): `GestureCx::route_children`, `Handled::By`,
//! and the gestures replayed to the owner of the press (React / C# IsSavedGesture: Panning, Wheel,
//! Up).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use drawnui::PointerKind;
use drawnui::prelude::*;
use drawnui::testing::Headless;

/// Counts the allocations of the calling thread (each test runs on its own).
struct Counting;
thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|a| a.set(a.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

type Log = Rc<RefCell<Vec<(&'static str, GestureKind)>>>;

/// Logs every gesture it sees and consumes the kinds in `takes`. `children_first` asks the
/// children before deciding, as React's SkiaScroll does.
struct Probe {
    layout: SkiaLayout,
    name: &'static str,
    children_first: bool,
    takes: &'static [GestureKind],
    log: Option<Log>,
}
impl Container for Probe {}
impl Control for Probe {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if let Some(log) = &self.log {
            log.borrow_mut().push((self.name, gesture.kind));
        }
        if self.children_first
            && let Some(child) = cx.route_children(gesture)
        {
            return Handled::By(child);
        }
        if self.takes.contains(&gesture.kind) { Handled::Yes } else { Handled::No }
    }
}

fn probe(name: &'static str, children_first: bool, takes: &'static [GestureKind], log: &Log) -> Build<Probe> {
    let layout = SkiaLayout::default();
    Build::new(Probe { layout, name, children_first, takes, log: Some(log.clone()) })
}

/// A square at (x, y), points.
fn at(build: Build<Probe>, x: f32, y: f32, size: f32) -> Build<Probe> {
    build.margin((x, y, 0.0, 0.0)).width_request(size).height_request(size)
}

#[derive(Default)]
struct App {
    tapped: u32,
    parent: Handle<Probe>,
}

fn host(build: impl FnOnce(&mut App) -> Build<Probe>) -> Headless<App> {
    let ui = Ui::new(App::default(), |app| SkiaLayout::new().fill().children(build(app))).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 300, 1.0);
    host.settle();
    host
}

fn seen(log: &Log, name: &str) -> Vec<GestureKind> {
    log.borrow().iter().filter(|(n, _)| *n == name).map(|(_, k)| *k).collect()
}

use GestureKind::{Down, Panning, Tapped, Up, Wheel};

#[test]
fn a_child_that_consumes_becomes_the_owner_and_the_parent_is_skipped() {
    let log = Log::default();
    let mut host = host(|app| {
        at(probe("parent", true, &[Panning, Up], &log), 0.0, 0.0, 200.0)
            .assign(&mut app.parent)
            .on_tapped(|_me, app: &mut App, _cx| app.tapped += 1)
            .children(at(probe("child", false, &[Down, Panning, Tapped, Up], &log), 0.0, 0.0, 100.0))
    });

    // Down reaches the child through the parent's route_children; the child owns the press, so
    // the pans and the release go to it alone.
    host.pan((50.0, 50.0), (50.0, 90.0), 64.0, 4);
    assert_eq!(seen(&log, "parent"), [Down]);
    assert_eq!(seen(&log, "child"), [Down, Panning, Panning, Panning, Panning, Up]);

    // A tap the child takes (Handled::By) is not the parent's: its tapped handler stays quiet.
    log.borrow_mut().clear();
    host.tap(50.0, 50.0);
    assert_eq!(seen(&log, "parent"), [Down, Tapped]);
    assert_eq!(seen(&log, "child"), [Down, Tapped, Up]);
    assert_eq!(host.ui.state.tapped, 0);

    // Outside the child the parent takes the pan itself.
    log.borrow_mut().clear();
    host.pan((150.0, 150.0), (150.0, 190.0), 64.0, 4);
    assert_eq!(seen(&log, "parent"), [Down, Panning, Panning, Panning, Panning, Up]);
    assert!(seen(&log, "child").is_empty());
}

#[test]
fn children_are_asked_once_and_the_own_tapped_handler_still_runs() {
    let log = Log::default();
    let mut host = host(|app| {
        at(probe("parent", true, &[], &log), 0.0, 0.0, 200.0)
            .assign(&mut app.parent)
            .on_tapped(|_me, app: &mut App, _cx| app.tapped += 1)
            .children(at(probe("child", false, &[], &log), 0.0, 0.0, 100.0))
    });
    host.tap(50.0, 50.0);
    // route_children ran from the hook: the router does not route into the children again.
    assert_eq!(seen(&log, "child"), [Down, Tapped, Up]);
    assert_eq!(seen(&log, "parent"), [Down, Tapped, Up]);
    // Nobody took the tap: the parent's own handler runs after its children, as React's super.
    assert_eq!(host.ui.state.tapped, 1);
}

#[test]
fn locked_children_are_not_routed() {
    let log = Log::default();
    let mut host = host(|app| {
        at(probe("parent", true, &[], &log), 0.0, 0.0, 200.0)
            .assign(&mut app.parent)
            .lock_children_gestures(LockTouch::Enabled)
            .on_tapped(|_me, app: &mut App, _cx| app.tapped += 1)
            .children(at(probe("child", false, &[Down, Tapped], &log), 0.0, 0.0, 100.0))
    });
    host.tap(50.0, 50.0);
    assert!(seen(&log, "child").is_empty());
    assert_eq!(host.ui.state.tapped, 1);

    // PassTap lets the tap through, nothing else.
    let parent = host.ui.state.parent;
    host.ui.tree.get_mut(parent).unwrap().set_lock_children_gestures(LockTouch::PassTap);
    host.tap(50.0, 50.0);
    assert_eq!(seen(&log, "child"), [Tapped]);
    assert_eq!(host.ui.state.tapped, 1);
}

#[test]
fn route_children_follows_the_content_offset_and_the_z_order_at_every_level() {
    let log = Log::default();
    let mut host = host(|app| {
        at(probe("parent", true, &[], &log), 0.0, 0.0, 200.0).assign(&mut app.parent).children((
            at(probe("a", false, &[Down], &log), 0.0, 0.0, 100.0),
            at(probe("b", true, &[Down], &log), 0.0, 100.0, 100.0).children((
                at(probe("b1", false, &[Down, Up], &log), 0.0, 0.0, 100.0).z_index(2),
                at(probe("b2", false, &[Down], &log), 0.0, 0.0, 100.0).z_index(1),
            )),
            // Later in the list but below b by z.
            at(probe("c", false, &[Down], &log), 0.0, 100.0, 100.0).z_index(-1),
        ))
    });
    let parent = host.ui.state.parent;
    // The content is drawn 100 points up: what is at y = 50 on screen is b.
    host.ui.tree.set_content_offset(parent, Point::new(0.0, -100.0));
    host.settle();
    host.ui.pointer(PointerKind::Down, 50.0, 50.0, host.time_ms());
    host.frame();
    let downs: Vec<&str> = log.borrow().iter().filter(|(_, k)| *k == Down).map(|(n, _)| *n).collect();
    assert_eq!(downs, ["parent", "b", "b1"]);

    // b1 owns the press: the release goes to it alone.
    log.borrow_mut().clear();
    host.ui.pointer(PointerKind::Up, 50.0, 130.0, host.time_ms());
    host.frame();
    assert_eq!(*log.borrow(), [("b1", Up)]);
}

#[test]
fn the_wheel_goes_to_the_owner_of_the_press_first() {
    let log = Log::default();
    let mut host = host(|app| {
        at(probe("parent", false, &[], &log), 0.0, 0.0, 300.0).assign(&mut app.parent).children((
            at(probe("a", false, &[Down, Up, Wheel], &log), 0.0, 0.0, 100.0),
            at(probe("b", false, &[Wheel], &log), 100.0, 0.0, 100.0),
            at(probe("c", false, &[Down, Up], &log), 200.0, 0.0, 100.0),
        ))
    });

    // Held down on a: a wheel over b is a's.
    host.ui.pointer(PointerKind::Down, 50.0, 50.0, host.time_ms());
    host.frame();
    assert!(host.wheel(150.0, 50.0, 1.0));
    assert_eq!(seen(&log, "a"), [Down, Wheel]);
    assert!(seen(&log, "b").is_empty());
    host.ui.pointer(PointerKind::Up, 50.0, 50.0, host.time_ms());
    host.frame();

    // Between presses the wheel goes to what is under the pointer.
    assert!(host.wheel(150.0, 50.0, 1.0));
    assert_eq!(seen(&log, "b"), [Wheel]);

    // An owner that does not use the wheel keeps its press: the wheel goes by position, the
    // release still to the owner.
    log.borrow_mut().clear();
    host.ui.pointer(PointerKind::Down, 250.0, 50.0, host.time_ms());
    host.frame();
    assert!(host.wheel(150.0, 50.0, 1.0));
    host.ui.pointer(PointerKind::Up, 250.0, 50.0, host.time_ms());
    host.frame();
    assert_eq!(seen(&log, "b"), [Wheel]);
    assert_eq!(seen(&log, "c"), [Down, Wheel, Tapped, Up]);
}

#[test]
fn routing_through_z_ordered_children_allocates_nothing() {
    let quiet = |name: &'static str, children_first: bool, takes: &'static [GestureKind]| {
        Build::new(Probe { layout: SkiaLayout::default(), name, children_first, takes, log: None })
    };
    let mut host = host(|app| {
        at(quiet("parent", true, &[]), 0.0, 0.0, 300.0).assign(&mut app.parent).children((
            at(quiet("a", true, &[]), 0.0, 0.0, 200.0).z_index(1).children((
                at(quiet("a1", false, &[Wheel]), 0.0, 0.0, 100.0).z_index(2),
                at(quiet("a2", false, &[]), 0.0, 0.0, 100.0),
            )),
            at(quiet("b", false, &[]), 0.0, 0.0, 200.0),
        ))
    });
    assert!(host.wheel(50.0, 50.0, 1.0));
    let before = ALLOCATIONS.with(|a| a.get());
    for _ in 0..20 {
        assert!(host.ui.wheel(50.0, 50.0, 1.0, host.time_ms()));
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
}

#[derive(Default)]
struct Reorder {
    scroll: Handle<SkiaScroll>,
    /// The ghost of the dragged row: its offset from where the drag started, points.
    ghost: f32,
    dragging: bool,
    events: Vec<GestureKind>,
}

/// The React Reorder page: a grip in each row of a scrolled list takes the press
/// (`consume_gestures`, React ConsumeGestures), so its drag moves the ghost and the list stays
/// still; a drag anywhere else on the row scrolls the list.
#[test]
fn a_grip_that_consumes_the_down_owns_the_drag_inside_a_scroll() {
    let rows = |_app: &mut Reorder| -> Vec<Build<SkiaLayout>> {
        (0..30)
            .map(|_| {
                SkiaLayout::new().fill_x().height_request(50).children(
                    SkiaShape::new().margin((250.0, 0.0, 0.0, 0.0)).width_request(50).height_request(50).consume_gestures(
                        |me, app: &mut Reorder, _cx, gesture| {
                            let scale = me.base().scale;
                            match gesture.kind {
                                Down => app.dragging = true,
                                Panning if app.dragging => app.ghost += gesture.delta.y / scale,
                                Up => app.dragging = false,
                                _ => return false,
                            }
                            app.events.push(gesture.kind);
                            true
                        },
                    ),
                )
            })
            .collect()
    };
    let ui = Ui::new(Reorder::default(), |app| {
        let content = SkiaLayout::column().spacing(0).fill_x().children(rows(app));
        SkiaLayout::new().fill().children(SkiaScroll::new().fill().assign(&mut app.scroll).content(content))
    })
    .background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    let offset = |host: &Headless<Reorder>| host.ui.tree.base(host.ui.state.scroll).unwrap().content_offset.y;
    let content = host.ui.tree.children(host.ui.state.scroll)[0];
    let content_offset = |host: &Headless<Reorder>| host.ui.tree.base(content).map_or(0.0, |b| b.content_offset.y) + offset(host);

    // On the grip of the third row: the ghost follows the finger, the list does not move.
    host.pan((275.0, 125.0), (275.0, 25.0), 96.0, 6);
    host.settle();
    assert_eq!(host.ui.state.ghost, -100.0);
    assert_eq!(content_offset(&host), 0.0);
    let events = std::mem::take(&mut host.ui.state.events);
    assert_eq!(events.first(), Some(&Down));
    assert_eq!(events.last(), Some(&Up));
    assert_eq!(events.iter().filter(|k| **k == Up).count(), 1, "the owner hears its Up once");
    assert_eq!(events.iter().filter(|k| **k == Panning).count(), 6);

    // Beside the grip the same drag scrolls the list, and the ghost stays.
    host.pan((100.0, 300.0), (100.0, 100.0), 96.0, 6);
    host.settle();
    assert_eq!(host.ui.state.ghost, -100.0);
    assert!(content_offset(&host) < -100.0, "{}", content_offset(&host));
    assert!(host.ui.state.events.is_empty());
}

#[derive(Default)]
struct Pong {
    /// (where, points inside the game; speed, points per second) of every pan the game saw.
    pans: Vec<(Point, f32)>,
    movement: i32,
    serves: u32,
}

/// React PongGame.ProcessGestures inside the page's RescalingLayout: the game sees every pan
/// (passing it on, as React's `super.ProcessGestures`), its point and speed in the game's own
/// points through the fitted scale; a tap serves, the release stops the paddle.
#[test]
fn a_game_in_a_rescaling_layout_reads_pans_in_its_own_points() {
    let ui = Ui::new(Pong::default(), |_| {
        let game = SkiaShape::new().fill().consume_gestures(|me, app: &mut Pong, cx, gesture| {
            match gesture.kind {
                Panning => {
                    let speed = gesture.velocity.x / me.base().scale;
                    app.pans.push((cx.gesture_point().unwrap(), speed));
                    app.movement = if speed.abs() > 5.0 { speed.signum() as i32 } else { 0 };
                }
                Tapped => app.serves += 1,
                Up => app.movement = 0,
                _ => {}
            }
            false
        });
        SkiaLayout::new().fill().children(RescalingLayout::new(400.0, 200.0).children(game))
    });
    // 800 x 400 pixels at scale 1 for a 400 x 200 point game: the game is drawn at 2.
    let mut host = Headless::new(ui, 800, 400, 1.0);
    host.settle();
    host.ui.pointer(PointerKind::Down, 400.0, 200.0, host.time_ms());
    host.frame_after(25.0);
    host.ui.pointer(PointerKind::Move, 450.0, 200.0, host.time_ms());
    host.frame_after(25.0);
    // 50 pixels in 25 ms: 2000 px/s, 1000 points/s at the fitted scale; at (225, 100) points.
    assert_eq!(host.ui.state.pans, [(Point::new(225.0, 100.0), 1000.0)]);
    assert_eq!(host.ui.state.movement, 1);
    host.ui.pointer(PointerKind::Up, 450.0, 200.0, host.time_ms());
    host.settle();
    assert_eq!(host.ui.state.movement, 0);
    host.tap(400.0, 200.0);
    assert_eq!(host.ui.state.serves, 1);
}
