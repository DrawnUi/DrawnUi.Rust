//! Port of DrawnUi.Net.Tests ChildrenCollectionTests (C# 66a1eb02, GitHub #156): every change of a
//! control's children changes what is drawn, in the children's order. Insert, replace, move and
//! clear, read from the children list and from where the column drew them; a control's own parts
//! stay through a clear.

use drawnui::prelude::*;
use drawnui::testing::Headless;

#[derive(Default)]
struct App {
    column: Handle<SkiaLayout>,
    /// Every item made, with its name.
    names: Vec<(ControlId, &'static str)>,
}

fn item() -> Build<SkiaShape> {
    SkiaShape::new().height_request(20).background_color(Color::RED)
}

/// A column holding "a" and "b".
fn column() -> Headless<App> {
    let ui = Ui::new(App::default(), |app: &mut App| {
        let (mut a, mut b) = (Handle::default(), Handle::default());
        let column = SkiaLayout::column().spacing(0).assign(&mut app.column).children((item().assign(&mut a), item().assign(&mut b)));
        app.names = vec![(a.id(), "a"), (b.id(), "b")];
        column
    })
    .background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    host
}

fn name(host: &Headless<App>, id: ControlId) -> &'static str {
    host.ui.state.names.iter().find(|(c, _)| *c == id).map_or("?", |(_, n)| n)
}

/// The column's children in order.
fn children(host: &Headless<App>) -> String {
    let column = host.ui.state.column.id();
    host.ui.tree.children(column).iter().map(|c| name(host, *c)).collect::<Vec<_>>().join(",")
}

/// The order the column drew its children in, read from where they landed.
fn drawn(host: &Headless<App>) -> String {
    let column = host.ui.state.column.id();
    let mut placed: Vec<ControlId> = host.ui.tree.children(column).to_vec();
    placed.sort_by(|a, b| host.rect(*a).top.total_cmp(&host.rect(*b).top));
    placed.iter().map(|c| name(host, *c)).collect::<Vec<_>>().join(",")
}

fn named(host: &mut Headless<App>, id: ControlId, name: &'static str) -> ControlId {
    host.ui.state.names.push((id, name));
    id
}

#[test]
fn insert_replace_move_follow_the_children() {
    let mut host = column();
    let column = host.ui.state.column.id();

    let i = host.ui.tree.cx().insert_child(column, 0, item());
    named(&mut host, i, "i");
    host.settle();
    assert_eq!(children(&host), "i,a,b");
    assert_eq!(drawn(&host), "i,a,b");

    let replaced = host.ui.tree.children(column)[1];
    let r = host.ui.tree.cx().replace_child(replaced, item()).expect("a child");
    named(&mut host, r, "r");
    host.settle();
    assert_eq!(children(&host), "i,r,b");
    assert_eq!(drawn(&host), "i,r,b");
    assert!(host.ui.tree.base(replaced).is_none(), "the replaced child is gone");

    host.ui.tree.cx().move_child(column, 0, 2);
    host.settle();
    assert_eq!(children(&host), "r,b,i");
    assert_eq!(drawn(&host), "r,b,i");
}

#[test]
fn clear_removes_the_children_built_and_added() {
    let mut host = column();
    let column = host.ui.state.column.id();
    let c = host.ui.tree.cx().add_child(column, item());
    named(&mut host, c, "c");
    host.settle();
    assert_eq!(children(&host), "a,b,c");
    host.ui.tree.cx().clear_children(column);
    host.settle();
    assert!(host.ui.tree.children(column).is_empty());
}

/// A clear drops only what the app put there: the parts a control made itself stay (C#
/// Reset_KeepsSubviewsAddedByTheControl: a subview that is not a Children item).
#[test]
fn clear_keeps_the_parts_a_control_made() {
    let mut check = Handle::<SkiaCheckbox>::default();
    let ui = Ui::new((), |_: &mut ()| SkiaLayout::column().children(SkiaCheckbox::new().assign(&mut check))).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    let parts = host.ui.tree.children(check).to_vec();
    assert!(!parts.is_empty(), "the checkbox built its look");
    host.ui.tree.cx().add_child(check, item());
    host.settle();
    assert_eq!(host.ui.tree.cx().app_children(check).len(), 1);
    host.ui.tree.cx().clear_children(check);
    host.settle();
    assert_eq!(host.ui.tree.children(check), parts.as_slice(), "the look stays, in its place");
}

/// Insert and move count the app's children only: a part keeps its place among them.
#[test]
fn a_part_keeps_its_place_when_the_app_children_move() {
    let mut check = Handle::<SkiaCheckbox>::default();
    let ui = Ui::new((), |_: &mut ()| SkiaLayout::column().children(SkiaCheckbox::new().assign(&mut check))).background(Color::WHITE);
    let mut host = Headless::new(ui, 200, 300, 1.0);
    host.settle();
    let parts = host.ui.tree.children(check).len();
    let x = host.ui.tree.cx().add_child(check, item());
    let y = host.ui.tree.cx().insert_child(check, 0, item());
    host.settle();
    let all = host.ui.tree.children(check).to_vec();
    assert_eq!(&all[parts..], &[y, x], "the app's children after the parts, in their order");
    host.ui.tree.cx().move_child(check, 1, 0);
    host.settle();
    assert_eq!(&host.ui.tree.children(check)[parts..], &[x, y]);
    assert_eq!(host.ui.tree.cx().app_children(check), vec![x, y]);
}
