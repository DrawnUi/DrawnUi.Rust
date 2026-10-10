//! The accessibility snapshot for screen readers on the desktop and on iOS, through AccessKit:
//! UI Automation on Windows (Narrator), NSAccessibility on macOS and iOS (VoiceOver), AT-SPI on
//! Linux (Orca). The same snapshot the browser shows as an ARIA overlay; a node's default action
//! (Invoke, press) is the overlay's activation, a tap at its center.

use accesskit::{Action, Affine, Live, Node, NodeId, Orientation, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

use crate::ui::{AccessibilityNode, Aria};

/// The window: the root of the tree. Snapshot nodes are `id + 1`, so none of them is 0.
pub(crate) const ROOT: NodeId = NodeId(0);

/// The node id of snapshot node `id`, and back.
pub(crate) fn node_id(id: u32) -> NodeId {
    NodeId(id as u64 + 1)
}
pub(crate) fn snapshot_id(node: NodeId) -> Option<u32> {
    node.0.checked_sub(1).and_then(|id| u32::try_from(id).ok())
}

/// The node a screen reader moves to when the one it was on is gone (its page closed): the first
/// node in reading order that is on screen (`height` points tall) and says something, else the
/// window. Without it VoiceOver kept its cursor on the empty spot: accesskit_ios posts only a
/// layout change, with no element, for a removed focus.
pub(crate) fn refocus(nodes: &[AccessibilityNode], focus: NodeId, height: f32) -> NodeId {
    let Some(id) = snapshot_id(focus) else { return focus };
    if nodes.iter().any(|n| n.id == id) {
        return focus;
    }
    let says = |n: &&AccessibilityNode| n.can_interact || !n.label.is_empty();
    nodes.iter().filter(says).find(|n| n.rect.bottom > 0.0 && n.rect.top < height).map_or(ROOT, |n| node_id(n.id))
}

/// The whole tree: the window and every node of the snapshot, each under its nearest node above
/// (a card holds its items), in reading order. Bounds are in points; the window's transform scales
/// them to its pixels.
pub(crate) fn tree(title: &str, nodes: &[AccessibilityNode], scale: f64, focus: NodeId) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label(title);
    window.set_transform(Affine::scale(scale));
    let children = |parent: Option<u32>| nodes.iter().filter(|n| n.parent == parent).map(|n| node_id(n.id)).collect::<Vec<_>>();
    window.set_children(children(None));
    let mut out = Vec::with_capacity(nodes.len() + 1);
    out.push((ROOT, window));
    for n in nodes {
        let role = role(&n.role);
        let mut node = Node::new(role);
        // A text's words are its value: AccessKit names a Label from it (UI Automation Name), and
        // a label set on it would be read as nothing.
        // A card named by its own title text is read once: the text says it.
        let said_inside = crate::ui::said_by_child(nodes, n);
        match (role, n.label.is_empty() || said_inside) {
            (_, true) => {}
            (Role::Label, false) => node.set_value(n.label.as_str()),
            (_, false) => node.set_label(n.label.as_str()),
        }
        let inside = children(Some(n.id));
        if !inside.is_empty() {
            node.set_children(inside);
        }
        if !n.hint.is_empty() {
            node.set_description(n.hint.as_str());
        }
        let r = n.rect;
        node.set_bounds(Rect::new(r.left as f64, r.top as f64, r.right as f64, r.bottom as f64));
        // Any node can be scrolled into view (a screen reader moving to a node off screen).
        node.add_action(Action::ScrollIntoView);
        if n.can_interact {
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
        } else if crate::ui::is_control_role(&n.role) {
            node.set_disabled();
        }
        // A scroll pages along the axes its content moves (Android picks the axis by orientation).
        let (h, v) = n.scrolls;
        if h {
            node.add_action(Action::ScrollLeft);
            node.add_action(Action::ScrollRight);
        }
        if v {
            node.add_action(Action::ScrollUp);
            node.add_action(Action::ScrollDown);
        }
        if h != v {
            node.set_orientation(if v { Orientation::Vertical } else { Orientation::Horizontal });
        }
        if let Some(v) = &n.value {
            // Sliders and progress bars run left to right. Without an orientation VoiceOver on
            // macOS reads a "circular slider" and its adjust jumped to the ends.
            node.set_orientation(Orientation::Horizontal);
            node.set_numeric_value(v.now);
            node.set_min_numeric_value(v.min);
            node.set_max_numeric_value(v.max);
            if !v.text.is_empty() {
                node.set_value(v.text.as_str());
            }
            if v.step > 0.0 {
                node.set_numeric_value_step(v.step);
                if n.can_interact {
                    node.add_action(Action::Increment);
                    node.add_action(Action::Decrement);
                    node.add_action(Action::SetValue);
                }
            }
        }
        if let Some(pressed) = n.is_pressed {
            node.set_toggled(if pressed { Toggled::True } else { Toggled::False });
        }
        match &*n.live {
            Aria::LIVE_POLITE => node.set_live(Live::Polite),
            Aria::LIVE_ASSERTIVE => node.set_live(Live::Assertive),
            _ => {}
        }
        out.push((node_id(n.id), node));
    }
    let focus = if out.iter().any(|(id, _)| *id == focus) { focus } else { ROOT };
    TreeUpdate { nodes: out, tree: Some(TreeInfo::new(ROOT)), tree_id: TreeId::ROOT, focus }
}

/// The AccessKit role of an ARIA role (`Aria`); an unknown one is a plain container.
fn role(aria: &str) -> Role {
    match aria {
        Aria::BUTTON => Role::Button,
        Aria::LINK => Role::Link,
        Aria::CHECKBOX => Role::CheckBox,
        Aria::RADIO => Role::RadioButton,
        Aria::SWITCH => Role::Switch,
        Aria::SLIDER => Role::Slider,
        Aria::SPIN_BUTTON => Role::SpinButton,
        Aria::TEXT_BOX => Role::TextInput,
        Aria::SEARCH_BOX => Role::SearchInput,
        Aria::COMBO_BOX => Role::ComboBox,
        Aria::LIST_BOX => Role::ListBox,
        Aria::OPTION => Role::ListBoxOption,
        Aria::TAB => Role::Tab,
        Aria::TAB_PANEL => Role::TabPanel,
        Aria::TAB_LIST => Role::TabList,
        Aria::MENU => Role::Menu,
        Aria::MENU_ITEM => Role::MenuItem,
        Aria::MENU_ITEM_CHECKBOX => Role::MenuItemCheckBox,
        Aria::MENU_ITEM_RADIO => Role::MenuItemRadio,
        Aria::SCROLL_BAR => Role::ScrollBar,
        Aria::TEXT => Role::Label,
        Aria::HEADING => Role::Heading,
        Aria::IMG => Role::Image,
        Aria::LIST => Role::List,
        Aria::LIST_ITEM => Role::ListItem,
        Aria::SEPARATOR => Role::Splitter,
        Aria::PROGRESS_BAR => Role::ProgressIndicator,
        Aria::TOOLTIP => Role::Tooltip,
        Aria::DIALOG => Role::Dialog,
        Aria::ALERT_DIALOG => Role::AlertDialog,
        Aria::STATUS => Role::Status,
        Aria::ALERT => Role::Alert,
        Aria::GROUP => Role::Group,
        Aria::REGION => Role::Region,
        Aria::NAVIGATION => Role::Navigation,
        Aria::MAIN => Role::Main,
        Aria::GRID => Role::Grid,
        Aria::TOOLBAR => Role::Toolbar,
        Aria::RADIO_GROUP => Role::RadioGroup,
        Aria::MENU_BAR => Role::MenuBar,
        // Pages on iOS and Android (accessibilityScroll, TalkBack scroll forward / back). The
        // desktop adapters page nothing; there a scroll pane would only nest the content (Narrator
        // reads an unnamed region, VoiceOver must interact into it), so it is a container the
        // adapters leave out.
        Aria::SCROLL_VIEW if cfg!(any(target_os = "ios", target_os = "android")) => Role::ScrollView,
        Aria::SCROLL_VIEW => Role::GenericContainer,
        _ => Role::GenericContainer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::ControlId;

    fn node(id: u32, role: &'static str, label: &str, can_interact: bool, parent: Option<u32>) -> AccessibilityNode {
        AccessibilityNode {
            id,
            control: ControlId { index: id, generation: 0 },
            role: role.into(),
            label: label.to_owned(),
            hint: String::new(),
            rect: skia_safe::Rect::from_xywh(0.0, id as f32 * 10.0, 100.0, 10.0),
            can_interact,
            is_pressed: None,
            value: None,
            scrolls: (false, false),
            live: "".into(),
            text_lines: Vec::new(),
            parent,
            group: None,
        }
    }

    fn built(nodes: &[AccessibilityNode], id: u32) -> Node {
        tree("w", nodes, 1.0, ROOT).nodes.into_iter().find(|(n, _)| *n == node_id(id)).expect("node").1
    }

    #[test]
    fn a_card_titled_by_a_heading_or_a_text_is_named_once() {
        let nodes = [
            node(1, Aria::GROUP, "Toggles", false, None),
            node(2, Aria::HEADING, "Toggles", false, Some(1)),
            node(3, Aria::BUTTON, "Images", true, None),
            node(4, Aria::TEXT, "Images", false, Some(3)),
        ];
        assert_eq!(built(&nodes, 1).label(), None, "the heading says it");
        assert_eq!(built(&nodes, 3).label(), None, "the text says it");
        assert_eq!(built(&nodes, 2).label(), Some("Toggles"));
        assert_eq!(built(&nodes, 4).value(), Some("Images"));
    }

    #[test]
    fn a_control_that_takes_no_input_is_disabled() {
        let nodes = [node(1, Aria::BUTTON, "Disabled", false, None), node(2, Aria::BUTTON, "On", true, None), node(3, Aria::GROUP, "Card", false, None)];
        assert!(built(&nodes, 1).is_disabled());
        assert!(!built(&nodes, 2).is_disabled());
        assert!(!built(&nodes, 3).is_disabled(), "a group is not a control");
    }

    #[test]
    fn a_focus_whose_node_is_gone_moves_to_the_first_node_on_screen() {
        // Rects are 10 points tall at id * 10; node 1 is moved above the 45 point tall screen.
        let mut above = node(1, Aria::BUTTON, "Above", true, None);
        above.rect = skia_safe::Rect::from_xywh(0.0, -20.0, 100.0, 10.0);
        let nodes = [above, node(2, Aria::GROUP, "", false, None), node(3, Aria::TEXT, "Title", false, None)];
        // The page with the focused "Back" (id 9) closed.
        assert_eq!(refocus(&nodes, node_id(9), 45.0), node_id(3));
        // A focus that is still there stays; the window stays the window.
        assert_eq!(refocus(&nodes, node_id(2), 45.0), node_id(2));
        assert_eq!(refocus(&nodes, ROOT, 45.0), ROOT);
        assert_eq!(refocus(&[], node_id(9), 45.0), ROOT);
    }
}
