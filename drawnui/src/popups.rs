//! Popup windows: content shown over everything at a point or beside a rect, the way native menus
//! open (a context menu, a dropdown). A desktop host shows each one in a window of its own, which
//! may extend past the app's window; the browser and mobile hosts, and Wayland, draw it on the
//! canvas, kept inside it. Either way it is a root of the same tree: same state, fonts, GPU.

use skia_safe::{Point, Rect, Size};

use crate::tree::{ControlId, Cx, Detached};

/// A popup window opened with [`Cx::open_popup_window`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PopupId(pub(crate) u32);

/// Where a popup window opens from, in canvas points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupAnchor {
    /// A point (where a right click was): the popup's top-left corner is there. Where the screen
    /// ends it opens to the left of the point, or above it.
    Point(Point),
    /// A rect (the button a dropdown belongs to): the popup opens below it, left edges together.
    /// Where the screen ends it opens above it, or right edges together.
    Rect(Rect),
    /// A control: as `Rect`, with the rect the control was last drawn at (moved, scaled, scrolled).
    Control(ControlId),
}

impl From<Point> for PopupAnchor {
    fn from(point: Point) -> Self {
        PopupAnchor::Point(point)
    }
}

impl From<Rect> for PopupAnchor {
    fn from(rect: Rect) -> Self {
        PopupAnchor::Rect(rect)
    }
}

impl<T: crate::Control> From<crate::tree::Handle<T>> for PopupAnchor {
    fn from(handle: crate::tree::Handle<T>) -> Self {
        PopupAnchor::Control(handle.into())
    }
}

impl From<ControlId> for PopupAnchor {
    fn from(id: ControlId) -> Self {
        PopupAnchor::Control(id)
    }
}

impl PopupAnchor {
    /// The anchor in the canvas's pixels (`scale`), and whether it is a point.
    pub(crate) fn pixels(self, tree: &crate::tree::Tree, scale: f32) -> (Rect, bool) {
        match self {
            PopupAnchor::Point(p) => (Rect::from_xywh(p.x * scale, p.y * scale, 0.0, 0.0), true),
            PopupAnchor::Rect(r) => (Rect::new(r.left * scale, r.top * scale, r.right * scale, r.bottom * scale), false),
            PopupAnchor::Control(id) => (crate::ui::drawn_rect(tree, id).unwrap_or_default(), false),
        }
    }
}

/// How a popup window behaves. `PopupWindowOptions::default()`: sized to its content, closed by
/// a press outside it, Escape, the app losing the keyboard, or the app.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopupWindowOptions {
    /// Its size in points; `None`: the size its content measures to. Never larger than the
    /// screen (the canvas, where it is drawn on the canvas).
    pub size: Option<Size>,
    /// It also closes once the mouse has left both it and the app's window (for 300 ms).
    pub close_on_pointer_leave: bool,
}

/// How long the mouse may be outside every window before `close_on_pointer_leave` closes.
pub(crate) const LEAVE_GRACE_MS: f64 = 300.0;

/// A popup the app shows in a window of its own, as a desktop host reads it from
/// [`crate::Host::popup_windows`] after every frame of the app's window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupWindowRequest {
    pub id: u32,
    /// Where it opens from, in the pixels of the app window's canvas: a point (`point`, zero
    /// size) or a rect it opens beside. The host places it with [`place_popup`] on the screen.
    pub anchor: Rect,
    pub point: bool,
    /// The size it wants, in points.
    pub size: Size,
}

pub(crate) struct Popup {
    pub(crate) id: u32,
    pub(crate) root: ControlId,
    pub(crate) anchor: PopupAnchor,
    pub(crate) options: PopupWindowOptions,
    /// In a window of its own, as the host placed it: its size in pixels, its scale, and its
    /// corner relative to the app window's canvas, in the canvas's pixels. `None`: on the canvas,
    /// or the window is not made yet.
    pub(crate) window: Option<(Size, f32, Point)>,
    /// Where it is laid out, in the pixels of what it is drawn on (on the canvas: where it shows;
    /// in its window: from 0, 0), and at what scale.
    pub(crate) rect: Rect,
    pub(crate) scale: f32,
    /// The size its content measured to, in points.
    pub(crate) wanted: Size,
    /// What its window showed when last painted (`Ui::popup_needs_paint`).
    pub(crate) painted: Option<PaintKey>,
}

/// What a popup window's picture depends on: its content (the root's content epoch), the GPU
/// context, its place and scale, the control the focus ring is drawn around.
pub(crate) type PaintKey = (u32, u32, Rect, f32, Option<ControlId>);

/// Where a popup of `size` goes for `anchor` (both pixels) inside `bounds` (the screen's work
/// area, or the canvas): at the anchor's corner, flipped to the other side of the anchor where it
/// does not fit, then moved and cut to stay inside. The rect is whole pixels.
pub fn place_popup(anchor: Rect, point: bool, size: Size, bounds: Rect) -> Rect {
    let (w, h) = (size.width.min(bounds.width()).max(0.0), size.height.min(bounds.height()).max(0.0));
    let mut left = anchor.left;
    if left + w > bounds.right {
        left = if point { anchor.left - w } else { anchor.right - w };
    }
    let mut top = anchor.bottom;
    if top + h > bounds.bottom {
        top = anchor.top - h;
    }
    let left = left.clamp(bounds.left, bounds.right - w).round();
    let top = top.clamp(bounds.top, bounds.bottom - h).round();
    Rect::from_xywh(left, top, w.round(), h.round())
}

impl Cx<'_> {
    /// Shows `content` over everything, opening from `anchor` (canvas points) the way native menus
    /// do. A desktop host gives it a window of its own (frameless, on top, no taskbar entry, it
    /// does not take the activation from the app's window), which may extend past the app's
    /// window; elsewhere it is drawn on the canvas. It closes with a press outside it, Escape,
    /// the app losing the keyboard, [`Cx::close_popup_window`], and with the options.
    pub fn open_popup_window(&mut self, content: impl Into<Detached>, anchor: impl Into<PopupAnchor>, options: PopupWindowOptions) -> PopupId {
        let tree = &mut *self.tree;
        let main = tree.root;
        let root = tree.mount(None, content);
        tree.root = main;
        tree.popup_next_id += 1;
        let id = tree.popup_next_id;
        let popup = Popup {
            id,
            root,
            anchor: anchor.into(),
            options,
            window: None,
            rect: Rect::default(),
            scale: 1.0,
            wanted: Size::default(),
            painted: None,
        };
        tree.popups.push(popup);
        tree.needs_frame = true;
        PopupId(id)
    }

    /// Closes the popup window (nothing when it is closed already).
    pub fn close_popup_window(&mut self, id: PopupId) {
        self.tree.close_popup(id.0);
    }

    /// Closes every popup window.
    pub fn close_popup_windows(&mut self) {
        while let Some(id) = self.tree.popups.last().map(|p| p.id) {
            self.tree.close_popup(id);
        }
    }

    /// The popup window is still open.
    pub fn is_popup_window_open(&self, id: PopupId) -> bool {
        self.tree.popups.iter().any(|p| p.id == id.0)
    }
}

impl crate::tree::Tree {
    pub(crate) fn close_popup(&mut self, id: u32) {
        let Some(index) = self.popups.iter().position(|p| p.id == id) else { return };
        let popup = self.popups.remove(index);
        self.removals.push(popup.root);
        self.needs_frame = true;
    }

    /// The popup whose content `id` is part of.
    pub(crate) fn popup_of(&self, mut id: ControlId) -> Option<&Popup> {
        while let Some(parent) = self.parent(id) {
            id = parent;
        }
        self.popups.iter().find(|p| p.root == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0.0, top: 0.0, right: 1000.0, bottom: 800.0 };

    #[test]
    fn a_popup_opens_at_the_point_and_flips_where_the_screen_ends() {
        let at = |x, y| Rect::from_xywh(x, y, 0.0, 0.0);
        let size = Size::new(200.0, 300.0);
        assert_eq!(place_popup(at(100.0, 100.0), true, size, SCREEN), Rect::from_xywh(100.0, 100.0, 200.0, 300.0));
        // Right edge: to the left of the point; bottom: above it.
        assert_eq!(place_popup(at(900.0, 700.0), true, size, SCREEN), Rect::from_xywh(700.0, 400.0, 200.0, 300.0));
        // No room on either side: kept inside, cut to the screen.
        assert_eq!(place_popup(at(100.0, 200.0), true, Size::new(200.0, 900.0), SCREEN), Rect::from_xywh(100.0, 0.0, 200.0, 800.0));
    }

    #[test]
    fn a_dropdown_opens_below_its_rect_else_above_or_right_aligned() {
        let button = Rect::from_xywh(850.0, 600.0, 120.0, 30.0);
        let size = Size::new(200.0, 300.0);
        assert_eq!(place_popup(Rect::from_xywh(100.0, 100.0, 120.0, 30.0), false, size, SCREEN), Rect::from_xywh(100.0, 130.0, 200.0, 300.0));
        assert_eq!(place_popup(button, false, size, SCREEN), Rect::from_xywh(770.0, 300.0, 200.0, 300.0));
    }
}
