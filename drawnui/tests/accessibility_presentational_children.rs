//! The texts inside a button, a radio, a switch, a tab or a slider are its look, not separate
//! nodes (ARIA "children presentational"): a screen reader says the control's name once, not the
//! name and then each text again. Selectable text and controls nested in it stay; a link and a
//! group keep their texts (DrawnUi.Net AccessibilityPresentationalChildrenTests, b55f0f72).

use drawnui::prelude::*;
use drawnui::testing::Headless;

fn labels(role: &'static str, selectable: bool) -> Vec<String> {
    let ui = Ui::new((), move |_: &mut ()| {
        SkiaLayout::column().fill().children((SkiaShape::new()
            .width_request(120)
            .height_request(40)
            .accessibility_role(role)
            .accessibility_label("Linen, $1,190")
            .children((SkiaLabel::new("Linen").accessibility_role(Aria::TEXT).accessibility_label("Linen").accessibility_text_selectable(selectable),)),))
    });
    let mut host = Headless::new(ui, 400, 300, 1.0);
    host.ui.set_accessibility_enabled(true);
    host.settle();
    host.frame_after(1100.0);
    host.ui.accessibility_nodes().iter().map(|n| n.label.clone()).filter(|l| !l.is_empty()).collect()
}

#[test]
fn texts_inside_a_control_with_presentational_children_are_not_nodes() {
    for role in [Aria::RADIO, Aria::BUTTON, Aria::SWITCH, Aria::TAB] {
        let labels = labels(role, false);
        assert_eq!(labels.iter().filter(|l| *l == "Linen, $1,190").count(), 1, "{role}: the control once: {labels:?}");
        assert!(!labels.iter().any(|l| l == "Linen"), "{role}: no text node inside: {labels:?}");
    }
}

#[test]
fn selectable_text_inside_a_button_stays() {
    let labels = labels(Aria::BUTTON, true);
    assert!(labels.iter().any(|l| l == "Linen"), "{labels:?}");
}

#[test]
fn texts_inside_a_link_or_a_group_stay_nodes() {
    for role in [Aria::LINK, Aria::GROUP] {
        let labels = labels(role, false);
        assert!(labels.iter().any(|l| l == "Linen"), "{role}: {labels:?}");
    }
}
