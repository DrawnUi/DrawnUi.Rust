// Question 5: this one import is everything the app side needs.
use drawnui_spike::prelude::*;

#[derive(Default)]
struct App {
    count: i32,
    button: Handle<SkiaButton>,
    title: Handle<SkiaLabel>,
}

// Questions 1 and 3: the target sample, as written in the brief.
fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayout::column()
        .spacing(16.0)
        .padding(24.0)
        .children((
            SkiaLabel::new("")
                .font_size(24.0)
                .text_color(Color::WHITE)
                .assign(&mut app.title)
                .observe(|me, app: &App| me.set_text(format!("Count {}", app.count))),
            SkiaButton::new("Tap me")
                .assign(&mut app.button)
                .on_tapped(|_me, app: &mut App, _cx| app.count += 1),
        ))
}

fn title_text(tree: &Tree, app: &App) -> String {
    tree.find::<SkiaLabel>(app.title).unwrap().props.text.clone()
}

// Question 6.
#[test]
fn counter_runs_through_the_tree() {
    let mut app = App::default();
    let mut tree = Tree::default();
    let root = tree.mount(None, build(&mut app));
    assert_eq!(tree.children(root), [app.title.into(), app.button.into()]);
    assert_eq!(tree.find::<SkiaLayout>(root).unwrap().props, LayoutProps { spacing: 16.0, padding: 24.0 });

    tree.run_observers(&app);
    assert_eq!(title_text(&tree, &app), "Count 0");
    assert!(tree.take_dirty(app.title).contains(Dirty::MEASURE));

    assert!(tree.tap(app.button, &mut app));
    assert_eq!(app.count, 1);
    tree.run_observers(&app);
    assert_eq!(title_text(&tree, &app), "Count 1");
    assert_eq!(tree.take_dirty(app.title), Dirty::MEASURE);

    // Same value again: the setter compares, nothing is marked.
    tree.run_observers(&app);
    assert_eq!(tree.take_dirty(app.title), Dirty::default());

    // Inner chain: the button node answers for its SkiaLayout base, not for an unrelated type.
    assert!(tree.find::<SkiaButton>(app.button).is_some());
    assert!(tree.find::<SkiaLayout>(app.button).is_some());
    assert!(tree.find::<SkiaLabel>(app.button).is_none());
    tree.find_mut::<SkiaLayout>(app.button).unwrap().set_spacing(8.0);
    assert_eq!(tree.find::<SkiaButton>(app.button).unwrap().layout.props.spacing, 8.0);
    assert_eq!(tree.take_dirty(app.button), Dirty::MEASURE);

    // Stale handle: gone after removal, and still gone once the slot is reused.
    tree.remove(app.title);
    assert!(tree.get_mut(app.title).is_none());
    assert_eq!(tree.children(root), [app.button.into()]);
    let mut fresh = Handle::default();
    tree.mount(Some(root), SkiaLabel::new("fresh").assign(&mut fresh));
    let (old, new) = (ControlId::from(app.title), ControlId::from(fresh));
    assert_eq!((old.index, old.generation + 1), (new.index, new.generation));
    assert!(tree.get_mut(app.title).is_none());
    assert!(tree.get_mut(fresh).is_some());
}

// `cx` reaches other nodes by handle from inside a handler.
#[test]
fn handler_reaches_other_controls_through_cx() {
    let mut app = App::default();
    let mut tree = Tree::default();
    tree.mount(
        None,
        SkiaLayout::column().children((
            SkiaLabel::new("").assign(&mut app.title),
            SkiaButton::new("Tap me").assign(&mut app.button).on_tapped(|me, app: &mut App, cx| {
                me.set_text("Tapped");
                cx.get_mut::<SkiaLabel>(app.title).unwrap().set_text_color(Color::WHITE);
                assert!(cx.get_mut(app.button).is_none(), "own node is `me`, not reachable via cx");
            }),
        )),
    );

    assert!(tree.tap(app.button, &mut app));
    assert!(!tree.tap(app.title, &mut app), "label has no tapped handler");
    assert_eq!(tree.find::<SkiaButton>(app.button).unwrap().props.text, "Tapped");
    assert_eq!(tree.find::<SkiaLabel>(app.title).unwrap().props.text_color, Color::WHITE);
    assert_eq!(tree.take_dirty(app.title), Dirty::DRAW);
}

// Shape B cost: the state type is checked when the handler runs, not when it compiles.
#[test]
#[should_panic(expected = "handler expects app state")]
fn wrong_state_type_panics_at_runtime() {
    struct Other;
    let mut app = App::default();
    let mut tree = Tree::default();
    tree.mount(None, build(&mut app));
    tree.tap(app.button, &mut Other);
}

// Question 7: a user control with only a `paint` override.
struct Gauge {
    value: f32,
}

impl Control for Gauge {
    fn paint(&self, canvas: &mut Canvas) {
        canvas.ops.push(format!("gauge {}", self.value));
    }
}

#[test]
fn custom_control_and_children_shapes() {
    let mut gauge = Handle::default();
    let rows: Vec<Detached> = (0..2).map(|i| SkiaLabel::new(format!("row {i}")).into()).collect();
    let more: Vec<_> = (0..2).map(|i| SkiaLabel::new(format!("more {i}"))).collect();
    let show_footer = false;

    let mut tree = Tree::default();
    let root = tree.mount(
        None,
        SkiaLayout::column().children((
            SkiaLabel::new("Title"),
            Build::new(Gauge { value: 0.5 }).assign(&mut gauge),
            // Question 4: `padding` is on ButtonProps and LayoutProps, so it must be qualified.
            LayoutBuild::padding(SkiaButton::new("Tap me").spacing(4.0), 2.0),
            rows,
            more,
            Some(SkiaLabel::new("shown")),
            show_footer.then(|| SkiaLabel::new("hidden")),
        )),
    );

    assert_eq!(tree.children(root).len(), 8);
    assert_eq!(tree.find::<Gauge>(gauge).unwrap().value, 0.5);
    let mut canvas = Canvas::default();
    tree.paint(&mut canvas);
    assert_eq!(canvas.ops, ["gauge 0.5"]);
}
