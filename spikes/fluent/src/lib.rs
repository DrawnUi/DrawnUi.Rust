//! DrawnUi.Rust fluent builder API: compile spike. Types only, std only, no rendering.

use std::any::{Any, type_name};
use std::cell::RefCell;
use std::marker::PhantomData;

pub mod prelude {
    pub use crate::*;
}

// ---------------------------------------------------------------- ids

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ControlId {
    pub index: u32,
    pub generation: u32,
}

thread_local! {
    // (generation per index, free indices). Builders have no tree to ask, so ids come from here.
    // ponytail: one allocator per thread shared by every Tree (Build/Tree are !Send anyway), so a
    // second tree gets sparse slots, and a Build dropped unmounted leaks its index. Move the
    // allocator into a shared tree context when either matters.
    static IDS: RefCell<(Vec<u32>, Vec<u32>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

fn reserve_id() -> ControlId {
    IDS.with_borrow_mut(|(generations, free)| {
        let index = free.pop().unwrap_or_else(|| {
            generations.push(0);
            generations.len() as u32 - 1
        });
        ControlId { index, generation: generations[index as usize] }
    })
}

fn release_id(id: ControlId) {
    IDS.with_borrow_mut(|(generations, free)| {
        generations[id.index as usize] += 1;
        free.push(id.index);
    })
}

/// Typed, copyable reference to a control. `Default` is dangling.
pub struct Handle<T>(ControlId, PhantomData<fn() -> T>);

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> Default for Handle<T> {
    fn default() -> Self {
        Handle(ControlId { index: u32::MAX, generation: 0 }, PhantomData)
    }
}
impl<T> From<Handle<T>> for ControlId {
    fn from(h: Handle<T>) -> Self {
        h.0
    }
}

// ---------------------------------------------------------------- small value types

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Dirty(u8);

impl Dirty {
    pub const MEASURE: Dirty = Dirty(1);
    pub const DRAW: Dirty = Dirty(2);
    pub const REPAINT: Dirty = Dirty(4);

    pub fn contains(self, other: Dirty) -> bool {
        self.0 & other.0 == other.0
    }
}
impl std::ops::BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Dirty) {
        self.0 |= rhs.0
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color(pub u32);

impl Color {
    pub const WHITE: Color = Color(0xFFFF_FFFF);
    pub const BLACK: Color = Color(0xFF00_0000);
}

/// Stand-in for the Skia canvas: records what was painted.
#[derive(Default)]
pub struct Canvas {
    pub ops: Vec<String>,
}

// ---------------------------------------------------------------- control model

pub trait Control: Any {
    /// The embedded base part (the C# base class), if any.
    fn inner(&self) -> Option<&dyn Control> {
        None
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        None
    }
    fn paint(&self, _canvas: &mut Canvas) {}
}

/// `Self` carries the props part `P`, directly or through its inner chain.
pub trait Has<P>: Control {
    fn part(&self) -> &P;
    fn part_mut(&mut self) -> &mut P;
}

fn part<T: Control>(c: &dyn Control) -> Option<&T> {
    match (c as &dyn Any).downcast_ref() {
        Some(t) => Some(t),
        None => part(c.inner()?),
    }
}

fn part_mut<T: Control>(c: &mut dyn Control) -> Option<&mut T> {
    if (&*c as &dyn Any).is::<T>() {
        return (c as &mut dyn Any).downcast_mut();
    }
    part_mut(c.inner_mut()?)
}

/// Write access to a mounted control. Setters compare and mark the node dirty.
pub struct Mut<'a, T> {
    control: &'a mut T,
    dirty: &'a mut Dirty,
}

/// Declares one props part: the struct with defaults, the builder trait (for `Build<T>`)
/// and the setter trait (for `Mut<'_, T>`), both for every `T: Has<Part>`.
macro_rules! props {
    ($props:ident, $build:ident, $set:ident {
        $($name:ident / $setter:ident : $ty:ty = $default:expr, $dirty:ident;)*
    }) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $props { $(pub $name: $ty,)* }

        impl Default for $props {
            fn default() -> Self { Self { $($name: $default,)* } }
        }

        pub trait $build: Sized { $(fn $name(self, v: impl Into<$ty>) -> Self;)* }

        impl<T: Has<$props>> $build for Build<T> {
            $(fn $name(mut self, v: impl Into<$ty>) -> Self {
                Has::<$props>::part_mut(&mut self.control).$name = v.into();
                self
            })*
        }

        pub trait $set { $(fn $setter(&mut self, v: impl Into<$ty>);)* }

        impl<T: Has<$props>> $set for Mut<'_, T> {
            $(fn $setter(&mut self, v: impl Into<$ty>) {
                let (v, p) = (v.into(), Has::<$props>::part_mut(self.control));
                if p.$name != v {
                    p.$name = v;
                    *self.dirty |= Dirty::$dirty;
                }
            })*
        }
    };
}

// ---------------------------------------------------------------- builder

type Observer = Box<dyn Fn(&mut dyn Control, &mut Dirty, &dyn Any)>;
type Tapped = Box<dyn Fn(&mut dyn Control, &mut Dirty, &mut dyn Any, &mut Cx<'_>)>;

/// A control under construction. Its id is already reserved, so `assign` hands out a real handle.
pub struct Build<T: Control> {
    id: ControlId,
    control: T,
    children: Vec<Detached>,
    observers: Vec<Observer>,
    tapped: Option<Tapped>,
}

/// A type-erased subtree, ready for `Tree::mount`.
pub struct Detached {
    id: ControlId,
    control: Box<dyn Control>,
    children: Vec<Detached>,
    observers: Vec<Observer>,
    tapped: Option<Tapped>,
}

impl<T: Control> From<Build<T>> for Detached {
    fn from(b: Build<T>) -> Self {
        Detached {
            id: b.id,
            control: Box::new(b.control),
            children: b.children,
            observers: b.observers,
            tapped: b.tapped,
        }
    }
}

// The engine is not generic over the app state: handlers get it as `dyn Any` and downcast here.
fn wrong_state<S>() -> ! {
    panic!("handler expects app state `{}`", type_name::<S>())
}

impl<T: Control> Build<T> {
    pub fn new(control: T) -> Self {
        Build { id: reserve_id(), control, children: Vec::new(), observers: Vec::new(), tapped: None }
    }

    pub fn assign(self, slot: &mut Handle<T>) -> Self {
        *slot = Handle(self.id, PhantomData);
        self
    }

    /// Runs on `Tree::run_observers`; setters only mark dirty when the value changed.
    pub fn observe<S: Any>(mut self, f: impl Fn(&mut Mut<'_, T>, &S) + 'static) -> Self {
        self.observers.push(Box::new(move |control, dirty, state| {
            let control = part_mut::<T>(control).expect("observer bound to its own control type");
            let state = state.downcast_ref::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut Mut { control, dirty }, state)
        }));
        self
    }

    pub fn on_tapped<S: Any>(
        mut self,
        f: impl Fn(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static,
    ) -> Self {
        self.tapped = Some(Box::new(move |control, dirty, state, cx| {
            let control = part_mut::<T>(control).expect("handler bound to its own control type");
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut Mut { control, dirty }, state, cx)
        }));
        self
    }
}

impl<T: Has<LayoutProps>> Build<T> {
    pub fn children(mut self, children: impl IntoChildren) -> Self {
        children.push_into(&mut self.children);
        self
    }
}

pub trait IntoChildren {
    fn push_into(self, out: &mut Vec<Detached>);
}

impl IntoChildren for Detached {
    fn push_into(self, out: &mut Vec<Detached>) {
        out.push(self)
    }
}
impl<T: Control> IntoChildren for Build<T> {
    fn push_into(self, out: &mut Vec<Detached>) {
        out.push(self.into())
    }
}
impl<C: IntoChildren> IntoChildren for Option<C> {
    fn push_into(self, out: &mut Vec<Detached>) {
        if let Some(c) = self {
            c.push_into(out)
        }
    }
}
impl<C: IntoChildren> IntoChildren for Vec<C> {
    fn push_into(self, out: &mut Vec<Detached>) {
        for c in self {
            c.push_into(out)
        }
    }
}

macro_rules! tuple_children {
    () => {};
    ($head:ident $($tail:ident)*) => {
        impl<$head: IntoChildren, $($tail: IntoChildren),*> IntoChildren for ($head, $($tail,)*) {
            #[allow(non_snake_case)]
            fn push_into(self, out: &mut Vec<Detached>) {
                let ($head, $($tail,)*) = self;
                $head.push_into(out);
                $($tail.push_into(out);)*
            }
        }
        tuple_children!($($tail)*);
    };
}
tuple_children!(A B C D E F G H I J K L);

// ---------------------------------------------------------------- tree

struct Node {
    id: ControlId,
    parent: Option<ControlId>,
    control: Box<dyn Control>,
    dirty: Dirty,
    children: Vec<ControlId>,
    observers: Vec<Observer>,
    tapped: Option<Tapped>,
}

#[derive(Default)]
pub struct Tree {
    slots: Vec<Option<Node>>,
}

impl Tree {
    fn node(&self, id: ControlId) -> Option<&Node> {
        self.slots.get(id.index as usize)?.as_ref().filter(|n| n.id == id)
    }

    fn node_mut(&mut self, id: ControlId) -> Option<&mut Node> {
        self.slots.get_mut(id.index as usize)?.as_mut().filter(|n| n.id == id)
    }

    fn take(&mut self, id: ControlId) -> Option<Node> {
        self.slots.get_mut(id.index as usize)?.take_if(|n| n.id == id)
    }

    /// Moves a detached subtree into the arena under `parent` (`None` for a root).
    pub fn mount(&mut self, parent: Option<ControlId>, detached: impl Into<Detached>) -> ControlId {
        let d = detached.into();
        let i = d.id.index as usize;
        if self.slots.len() <= i {
            self.slots.resize_with(i + 1, || None);
        }
        self.slots[i] = Some(Node {
            id: d.id,
            parent,
            control: d.control,
            dirty: Dirty::default(),
            children: Vec::new(),
            observers: d.observers,
            tapped: d.tapped,
        });
        if let Some(p) = parent.and_then(|p| self.node_mut(p)) {
            p.children.push(d.id);
        }
        for child in d.children {
            self.mount(Some(d.id), child);
        }
        d.id
    }

    /// Removes the node and its subtree; every handle into it goes stale.
    pub fn remove(&mut self, id: impl Into<ControlId>) {
        let id = id.into();
        let Some(node) = self.take(id) else { return };
        release_id(id);
        if let Some(p) = node.parent.and_then(|p| self.node_mut(p)) {
            p.children.retain(|c| *c != id);
        }
        for child in node.children {
            self.remove(child);
        }
    }

    pub fn children(&self, id: impl Into<ControlId>) -> &[ControlId] {
        self.node(id.into()).map_or(&[], |n| &n.children)
    }

    /// Typed lookup; walks the inner chain, so `find::<SkiaLayout>(button)` works.
    pub fn find<T: Control>(&self, id: impl Into<ControlId>) -> Option<&T> {
        part(&*self.node(id.into())?.control)
    }

    pub fn find_mut<T: Control>(&mut self, id: impl Into<ControlId>) -> Option<Mut<'_, T>> {
        let node = self.node_mut(id.into())?;
        Some(Mut { control: part_mut(&mut *node.control)?, dirty: &mut node.dirty })
    }

    pub fn get_mut<T: Control>(&mut self, handle: Handle<T>) -> Option<Mut<'_, T>> {
        self.find_mut(handle)
    }

    /// Returns the node's dirty flags and clears them (what a layout/draw pass would do).
    pub fn take_dirty(&mut self, id: impl Into<ControlId>) -> Dirty {
        self.node_mut(id.into()).map_or(Dirty::default(), |n| std::mem::take(&mut n.dirty))
    }

    pub fn run_observers(&mut self, state: &dyn Any) {
        for node in self.slots.iter_mut().flatten() {
            for observer in &node.observers {
                observer(&mut *node.control, &mut node.dirty, state);
            }
        }
    }

    /// Invokes the node's tapped handler. Returns false when there is no such node or handler.
    pub fn tap(&mut self, id: impl Into<ControlId>, state: &mut dyn Any) -> bool {
        let id = id.into();
        // The node leaves its slot while its handler runs, so `cx` can borrow the rest of the
        // tree; `cx.get_mut(own handle)` is None inside the handler (the handler has `me`).
        let Some(mut node) = self.take(id) else { return false };
        let handled = node.tapped.is_some();
        if let Some(tapped) = &node.tapped {
            tapped(&mut *node.control, &mut node.dirty, state, &mut Cx { tree: self });
        }
        self.slots[id.index as usize] = Some(node);
        handled
    }

    // ponytail: slot order, not tree order; enough to show `paint` dispatch through `dyn Control`.
    pub fn paint(&self, canvas: &mut Canvas) {
        for node in self.slots.iter().flatten() {
            node.control.paint(canvas);
        }
    }
}

/// What a handler may reach besides its own control and the app state.
pub struct Cx<'a> {
    tree: &'a mut Tree,
}

impl Cx<'_> {
    pub fn get_mut<T: Control>(&mut self, handle: Handle<T>) -> Option<Mut<'_, T>> {
        self.tree.get_mut(handle)
    }
}

// ---------------------------------------------------------------- controls

props!(LayoutProps, LayoutBuild, LayoutSet {
    spacing / set_spacing: f32 = 0.0, MEASURE;
    padding / set_padding: f32 = 0.0, MEASURE;
});

#[derive(Default)]
pub struct SkiaLayout {
    pub props: LayoutProps,
}

impl SkiaLayout {
    // Spike: a column is the only layout type modelled.
    pub fn column() -> Build<SkiaLayout> {
        Build::new(SkiaLayout::default())
    }
}
impl Control for SkiaLayout {}
impl Has<LayoutProps> for SkiaLayout {
    fn part(&self) -> &LayoutProps {
        &self.props
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.props
    }
}

props!(LabelProps, LabelBuild, LabelSet {
    text / set_text: String = String::new(), MEASURE;
    font_size / set_font_size: f32 = 12.0, MEASURE;
    text_color / set_text_color: Color = Color::BLACK, DRAW;
});

#[derive(Default)]
pub struct SkiaLabel {
    pub props: LabelProps,
}

impl SkiaLabel {
    pub fn new(text: impl Into<String>) -> Build<SkiaLabel> {
        Build::new(SkiaLabel::default()).text(text)
    }
}
impl Control for SkiaLabel {}
impl Has<LabelProps> for SkiaLabel {
    fn part(&self) -> &LabelProps {
        &self.props
    }
    fn part_mut(&mut self) -> &mut LabelProps {
        &mut self.props
    }
}

// `text` repeats LabelProps (different control: no clash). `padding` repeats LayoutProps, which
// SkiaButton also has: that is the deliberate clash for question 4.
props!(ButtonProps, ButtonBuild, ButtonSet {
    text / set_text: String = String::new(), MEASURE;
    padding / set_padding: f32 = 0.0, MEASURE;
});

/// Models C# `SkiaButton : SkiaLayout`.
#[derive(Default)]
pub struct SkiaButton {
    pub layout: SkiaLayout,
    pub props: ButtonProps,
}

impl SkiaButton {
    pub fn new(text: impl Into<String>) -> Build<SkiaButton> {
        Build::new(SkiaButton::default()).text(text)
    }
}
impl Control for SkiaButton {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }
}
impl Has<ButtonProps> for SkiaButton {
    fn part(&self) -> &ButtonProps {
        &self.props
    }
    fn part_mut(&mut self) -> &mut ButtonProps {
        &mut self.props
    }
}
impl Has<LayoutProps> for SkiaButton {
    fn part(&self) -> &LayoutProps {
        &self.layout.props
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.props
    }
}
