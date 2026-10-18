//! `props!` takes doc comments for the three items it makes and for each property, so a crate that
//! denies `missing_docs` can use it: this file stops compiling if one of them is not passed on.
#![deny(missing_docs)]

use drawnui::props;

props!(
    /// What a gauge shows.
    GaugeProps,
    /// Sets them while building.
    GaugeBuild,
    /// Sets them at run time.
    GaugeSet {
        /// The value shown, 0 to 1.
        value / set_value: f32 = 0.5, DRAW;
    }
);

#[test]
fn documented_props_keep_their_defaults() {
    assert_eq!(GaugeProps::default().value, 0.5);
}
