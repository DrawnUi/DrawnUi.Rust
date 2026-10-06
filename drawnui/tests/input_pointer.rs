//! Pointer input besides the press: hover (DrawnUI Pointer, IsPointerOver) and the cursor it asks
//! for, the other mouse buttons, LongPressing (C# desktop heads: 1500 ms held still), the context
//! menu (React ContextMenuEventArgs, routed like a tap, the Canvas fallback last).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use drawnui::prelude::*;
use drawnui::testing::Headless;
use drawnui::{LONG_PRESS_MS, PointerKind};

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

type Log = Rc<RefCell<Vec<String>>>;

/// Counts the hover family it sees and logs it unless `quiet`; `cursor` answers from x = 100 px.
struct Probe {
    layout: SkiaLayout,
    name: &'static str,
    cursor: Option<Cursor>,
    log: Log,
    seen: Rc<Cell<u32>>,
    quiet: Rc<Cell<bool>>,
}
impl Container for Probe {}
impl Control for Probe {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
    fn on_gesture(&mut self, _cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if matches!(gesture.kind, GestureKind::PointerEnter | GestureKind::PointerExit) {
            self.seen.set(self.seen.get() + 1);
            if !self.quiet.get() {
                self.log.borrow_mut().push(format!("{} {:?}", self.name, gesture.kind));
            }
        }
        Handled::No
    }
    fn cursor(&self, local: Point) -> Option<Cursor> {
        self.cursor.filter(|_| local.x >= 100.0)
    }
}

#[derive(Default)]
struct App {
    log: Log,
    seen: Rc<Cell<u32>>,
    quiet: Rc<Cell<bool>>,
    outer: Handle<Probe>,
    inner: Handle<Probe>,
    text: Handle<Probe>,
}

fn probe(name: &'static str, cursor: Option<Cursor>, app: &App) -> Build<Probe> {
    let (log, seen, quiet) = (app.log.clone(), app.seen.clone(), app.quiet.clone());
    Build::new(Probe { layout: SkiaLayout::default(), name, cursor, log, seen, quiet })
}

/// outer (0..200 x 0..100) with a tapped handler holds inner (0..100); text (0..200 x 200..300)
/// answers the I-beam over its right half.
fn host() -> Headless<App> {
    let ui = Ui::new(App::default(), |app| {
        let (outer, inner, text) = (probe("outer", None, app), probe("inner", None, app), probe("text", Some(Cursor::Text), app));
        SkiaLayout::new().fill().children((
            outer
                .width_request(200)
                .height_request(100)
                .assign(&mut app.outer)
                .on_tapped(|_me, app: &mut App, cx| {
                    let button = cx.gesture().map(|g| g.button);
                    app.log.borrow_mut().push(format!("outer tapped {button:?}"));
                })
                .children(inner.width_request(100).height_request(100).assign(&mut app.inner)),
            text
                .margin((0.0, 200.0, 0.0, 0.0))
                .width_request(200)
                .height_request(100)
                .assign(&mut app.text),
        ))
    });
    let mut host = Headless::new(ui, 300, 400, 1.0);
    host.settle();
    host
}

fn take(host: &Headless<App>) -> Vec<String> {
    std::mem::take(&mut *host.ui.state.log.borrow_mut())
}

#[test]
fn hover_enters_and_exits_the_controls_under_the_mouse_and_sets_the_cursor() {
    let mut host = host();
    let (outer, inner) = (host.ui.state.outer.id(), host.ui.state.inner.id());
    host.hover(50.0, 50.0);
    assert_eq!(take(&host), ["outer PointerEnter", "inner PointerEnter"]);
    assert!(host.ui.pointer_over().ends_with(&[outer, inner]));
    // Over something tappable: the hand.
    assert_eq!(host.ui.cursor(), Cursor::Pointer);

    // Still inside: no new enter.
    host.hover(60.0, 50.0);
    assert!(take(&host).is_empty());
    host.hover(150.0, 50.0);
    assert_eq!(take(&host), ["inner PointerExit"]);
    assert_eq!(host.ui.cursor(), Cursor::Pointer);

    // A control answers for itself: the I-beam over the right half only.
    host.hover(50.0, 250.0);
    assert_eq!(take(&host), ["text PointerEnter", "outer PointerExit"]);
    assert_eq!(host.ui.cursor(), Cursor::Default);
    host.hover(150.0, 250.0);
    assert_eq!(host.ui.cursor(), Cursor::Text);

    host.leave();
    assert_eq!(take(&host), ["text PointerExit"]);
    assert_eq!(host.ui.cursor(), Cursor::Default);
    assert!(host.ui.pointer_over().is_empty());
}

#[test]
fn hover_moves_are_one_route_per_frame_and_allocate_nothing() {
    let mut host = host();
    host.hover(50.0, 50.0);
    host.hover(150.0, 250.0);
    host.ui.state.quiet.set(true);
    let seen = host.ui.state.seen.get();
    let before = ALLOCATIONS.with(|a| a.get());
    for i in 0..30 {
        // Many moves between two frames: only the last one is routed.
        for x in 0..10 {
            host.ui.pointer(PointerKind::Hover, (i % 2 * 100 + x) as f32, 50.0 + (i % 2 * 200) as f32, host.time_ms());
        }
        host.frame_after(16.0);
    }
    assert_eq!(ALLOCATIONS.with(|a| a.get()) - before, 0);
    // Every frame moved between outer + inner and text: three enters and exits each.
    assert_eq!(host.ui.state.seen.get() - seen, 30 * 3);
}

#[test]
fn every_button_taps_and_the_handler_can_tell_which() {
    let mut host = host();
    host.tap(50.0, 50.0);
    assert_eq!(take(&host), ["outer tapped Some(Left)"]);
    // A right click is a tap of the right button, and a context menu nobody took.
    assert!(!host.right_click(50.0, 50.0));
    assert_eq!(take(&host), ["outer tapped Some(Right)"]);
    // Middle click.
    host.ui.pointer_button(PointerKind::Down, MouseButton::Middle, 50.0, 50.0, host.time_ms());
    host.frame();
    host.ui.pointer_button(PointerKind::Up, MouseButton::Middle, 50.0, 50.0, host.time_ms());
    host.settle();
    assert_eq!(take(&host), ["outer tapped Some(Middle)"]);
    // Another button while one is held is ignored: one press at a time.
    host.ui.pointer_button(PointerKind::Down, MouseButton::Left, 50.0, 50.0, host.time_ms());
    host.ui.pointer_button(PointerKind::Down, MouseButton::Right, 50.0, 50.0, host.time_ms());
    host.ui.pointer_button(PointerKind::Up, MouseButton::Right, 50.0, 50.0, host.time_ms());
    host.ui.pointer_button(PointerKind::Up, MouseButton::Left, 50.0, 50.0, host.time_ms());
    host.settle();
    assert_eq!(take(&host), ["outer tapped Some(Left)"]);
}

#[derive(Default)]
struct Menus {
    log: Vec<String>,
    card: Handle<SkiaShape>,
    fallback: bool,
}

fn menus() -> Headless<Menus> {
    let ui = Ui::new(Menus { fallback: true, ..Menus::default() }, |app| {
        SkiaLayout::new().fill().children((
            // The card takes menus, its inner area declines them, its badge takes them itself.
            SkiaShape::new()
                .margin((20.0, 20.0, 0.0, 0.0))
                .width_request(200)
                .height_request(100)
                .assign(&mut app.card)
                .on_context_menu(|_me, app: &mut Menus, _cx, menu| {
                    app.log.push(format!("card {:?} {:?} {:?}", menu.local, menu.location, menu.source));
                    true
                })
                .children((
                    SkiaShape::new().width_request(100).height_request(100).on_context_menu(|_me, app: &mut Menus, _cx, _menu| {
                        app.log.push("inner declines".into());
                        false
                    }),
                    SkiaShape::new()
                        .margin((150.0, 0.0, 0.0, 0.0))
                        .width_request(50)
                        .height_request(50)
                        .on_context_menu(|_me, app: &mut Menus, _cx, menu| {
                            app.log.push(format!("badge {:?}", menu.local));
                            true
                        }),
                )),
            SkiaShape::new()
                .margin((20.0, 200.0, 0.0, 0.0))
                .width_request(100)
                .height_request(50)
                .on_long_pressing(|_me, app: &mut Menus, cx| {
                    let at = cx.gesture().map(|g| g.location);
                    app.log.push(format!("long press {at:?}"));
                })
                .on_tapped(|_me, app: &mut Menus, _cx| app.log.push("tapped".into())),
        ))
    })
    .on_context_menu(|app: &mut Menus, menu, _cx| {
        app.log.push(format!("canvas {:?} {:?}", menu.location, menu.control));
        app.fallback
    });
    let mut host = Headless::new(ui, 600, 600, 2.0);
    host.settle();
    host
}

#[test]
fn the_context_menu_goes_to_the_deepest_control_that_takes_it_then_the_canvas() {
    let mut host = menus();
    // Pixels at scale 2: the card starts at (40, 40).
    assert!(host.right_click(100.0, 100.0));
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["inner declines", "card Point { x: 60.0, y: 60.0 } Point { x: 50.0, y: 50.0 } Mouse"]);
    assert!(host.right_click(360.0, 60.0));
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["badge Point { x: 20.0, y: 20.0 }"]);
    // Outside every control that takes one: the canvas fallback; it may leave the browser's menu.
    assert!(host.right_click(500.0, 500.0));
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["canvas Point { x: 250.0, y: 250.0 } None"]);
    host.ui.state.fallback = false;
    assert!(!host.ui.context_menu(500.0, 500.0, ContextMenuSource::Keyboard, host.time_ms()));
    // The keyboard source reaches the control too.
    assert!(host.ui.context_menu(100.0, 100.0, ContextMenuSource::Keyboard, host.time_ms()));
    assert!(host.ui.state.log.last().unwrap().ends_with("Keyboard"));
}

#[test]
fn a_press_held_still_is_a_long_press_and_the_host_sleeps_until_it_is_due() {
    let mut host = menus();
    // (40..240 x 400..500) pixels.
    host.ui.pointer(PointerKind::Down, 100.0, 450.0, host.time_ms());
    host.frame();
    let down = host.time_ms();
    // No frames are asked for while it waits: the host wakes at the time.
    assert!(!host.ui.needs_frame());
    assert_eq!(host.ui.wake_at(), Some(down + LONG_PRESS_MS));
    host.frame_after(LONG_PRESS_MS - 16.0);
    assert!(host.ui.state.log.is_empty());
    host.frame_after(16.0);
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["long press Some(Point { x: 100.0, y: 450.0 })"]);
    // Fires once; the release that follows is no tap (AppoMobi.Gestures `!IsLongPressing`, the
    // MAUI and Blazor heads; the C# OpenTK / WPF canvas still taps).
    host.frame_after(LONG_PRESS_MS);
    host.ui.pointer(PointerKind::Up, 100.0, 450.0, host.time_ms());
    host.settle();
    assert!(host.ui.state.log.is_empty(), "{:?}", host.ui.state.log);

    // A press that moved past the tap threshold (16 points) is a pan: no long press.
    host.ui.pointer(PointerKind::Down, 100.0, 450.0, host.time_ms());
    host.frame();
    host.ui.pointer(PointerKind::Move, 140.0, 450.0, host.time_ms());
    host.frame_after(16.0);
    assert_eq!(host.ui.wake_at(), None);
    host.frame_after(LONG_PRESS_MS);
    host.ui.pointer(PointerKind::Up, 140.0, 450.0, host.time_ms());
    host.settle();
    assert!(host.ui.state.log.is_empty());

    // The robot does the same.
    host.long_press(100.0, 450.0);
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["long press Some(Point { x: 100.0, y: 450.0 })"]);
    // The next press taps again.
    host.tap(100.0, 450.0);
    assert_eq!(std::mem::take(&mut host.ui.state.log), ["tapped"]);
}
