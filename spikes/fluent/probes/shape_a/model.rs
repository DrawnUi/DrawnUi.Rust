// Shape A model: the builder carries the app state type S, handlers get Cx<'_, S>.
// Types only; just enough surface to mirror the real spike's call shapes.
#![allow(dead_code)]
use std::marker::PhantomData;

pub struct Cx<'a, S> {
    pub state: &'a mut S,
}

pub struct Detached<S>(PhantomData<fn(&mut S)>);

pub struct Build<T, S> {
    pub control: T,
    tapped: Option<Box<dyn Fn(&mut T, &mut Cx<'_, S>)>>,
    children: Vec<Detached<S>>,
}

impl<T, S> Build<T, S> {
    pub fn new(control: T) -> Self {
        Build { control, tapped: None, children: Vec::new() }
    }
    pub fn spacing(self, _v: f32) -> Self {
        self
    }
    pub fn on_tapped(mut self, f: impl Fn(&mut T, &mut Cx<'_, S>) + 'static) -> Self {
        self.tapped = Some(Box::new(f));
        self
    }
    /// Variant whose parameter type names S directly (fn pointer instead of a bounded generic).
    pub fn on_tapped_fn(mut self, f: fn(&mut T, &mut Cx<'_, S>)) -> Self
    where
        T: 'static,
        S: 'static,
    {
        self.tapped = Some(Box::new(f));
        self
    }
    pub fn children(mut self, c: impl IntoChildren<S>) -> Self {
        c.push_into(&mut self.children);
        self
    }
}

pub trait IntoChildren<S> {
    fn push_into(self, out: &mut Vec<Detached<S>>);
}
impl<T, S> IntoChildren<S> for Build<T, S> {
    fn push_into(self, out: &mut Vec<Detached<S>>) {
        out.push(Detached(PhantomData))
    }
}
impl<S, A: IntoChildren<S>> IntoChildren<S> for (A,) {
    fn push_into(self, out: &mut Vec<Detached<S>>) {
        self.0.push_into(out)
    }
}
impl<S, A: IntoChildren<S>, B: IntoChildren<S>> IntoChildren<S> for (A, B) {
    fn push_into(self, out: &mut Vec<Detached<S>>) {
        self.0.push_into(out);
        self.1.push_into(out)
    }
}

pub struct SkiaLayout;
pub struct SkiaButton;

impl SkiaLayout {
    pub fn column<S>() -> Build<SkiaLayout, S> {
        Build::new(SkiaLayout)
    }
}
impl SkiaButton {
    pub fn new<S>(_text: &str) -> Build<SkiaButton, S> {
        Build::new(SkiaButton)
    }
}

pub struct Tree<S>(Vec<Detached<S>>);

impl<S> Tree<S> {
    pub fn new() -> Self {
        Tree(Vec::new())
    }
    pub fn mount<T>(&mut self, root: Build<T, S>) {
        root.push_into(&mut self.0)
    }
}

/// State-typed factory: the only way the closure sees S without an annotation.
pub struct Ui<S>(PhantomData<fn(&mut S)>);

impl<S> Ui<S> {
    pub fn new() -> Self {
        Ui(PhantomData)
    }
    pub fn column(&self) -> Build<SkiaLayout, S> {
        Build::new(SkiaLayout)
    }
    pub fn button(&self, _text: &str) -> Build<SkiaButton, S> {
        Build::new(SkiaButton)
    }
}

pub struct App {
    pub count: i32,
}
pub struct Other {
    pub count: i32,
}
