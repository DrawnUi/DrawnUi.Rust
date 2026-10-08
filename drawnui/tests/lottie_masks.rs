//! Lottie layer masks, as Skottie's geometric merge (`AttachMask`): solid layers cut by masks of
//! every mode. A spinner made of masked solid layers (bodymovin's usual export) drew as a filled
//! square before masks were applied.

use drawnui::prelude::*;
use drawnui::testing::Headless;

/// A closed rectangle path for a mask.
macro_rules! rect {
    ($x0:expr, $y0:expr, $x1:expr, $y1:expr) => {
        concat!(r#"{"a":0,"k":{"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]],"v":[["#,
            $x0, ",", $y0, "],[", $x1, ",", $y0, "],[", $x1, ",", $y1, "],[", $x0, ",", $y1, r#"]],"c":true}}"#)
    };
}
const KS: &str = r#""ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[0,0,0]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]}}"#;

/// 100 x 100, top layer first. Magenta: inverted full mask (nothing left). Red: xor (the spinner's
/// mode). Green: add then subtract (two bars). Blue: intersect twice. Yellow, at the bottom: a
/// first subtract (everything but the bottom-right square).
fn json() -> String {
    let solid = |color: &str, masks: &[(&str, bool, &str)]| {
        let masks: Vec<String> = masks
            .iter()
            .map(|(mode, inv, pt)| format!(r#"{{"mode":"{mode}","inv":{inv},"o":{{"a":0,"k":100}},"x":{{"a":0,"k":0}},"pt":{pt}}}"#))
            .collect();
        format!(r##"{{"ty":1,"sc":"#{color}","sw":100,"sh":100,"ip":0,"op":60,"st":0,{KS},"hasMask":true,"masksProperties":[{}]}}"##, masks.join(","))
    };
    let layers = [
        solid("ff00ff", &[("a", true, rect!(0, 0, 100, 100))]),
        solid("ff0000", &[("f", false, rect!(5, 5, 25, 25))]),
        solid("00ff00", &[("a", false, rect!(55, 5, 95, 25)), ("s", false, rect!(70, 5, 80, 25))]),
        solid("0000ff", &[("i", false, rect!(5, 55, 45, 95)), ("i", false, rect!(25, 55, 45, 95))]),
        solid("ffff00", &[("s", false, rect!(55, 55, 95, 95))]),
    ];
    format!(r#"{{"v":"5.5.8","fr":30,"ip":0,"op":60,"w":100,"h":100,"assets":[],"layers":[{}]}}"#, layers.join(","))
}

#[test]
fn masks_cut_solid_layers_in_every_mode() {
    let json: &'static str = Box::leak(json().into_boxed_str());
    let ui = Ui::new((), move |_| SkiaLottie::new("").json(json).auto_play(false).width_request(100).height_request(100))
        .background(Color::WHITE);
    let mut host = Headless::new(ui, 100, 100, 1.0);
    host.settle();
    let expect = [
        ((15, 15), Color::RED, "xor mask"),
        ((60, 15), Color::GREEN, "add"),
        ((75, 15), Color::YELLOW, "subtracted from the add"),
        ((90, 15), Color::GREEN, "add, past the subtracted bar"),
        ((35, 75), Color::BLUE, "intersect"),
        ((10, 75), Color::YELLOW, "outside the intersection"),
        ((75, 75), Color::WHITE, "a first subtract leaves the rest of the layer"),
        ((50, 40), Color::YELLOW, "an inverted full mask leaves nothing"),
    ];
    for ((x, y), color, what) in expect {
        assert_eq!(host.pixel(x, y), color, "({x}, {y}) {what}");
    }
}
