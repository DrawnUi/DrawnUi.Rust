//! Port of DrawnUi.Net.Tests LabelTruncationPhantomLineTests (upstream issue #338): MaxLines = 1 with
//! TailTruncation on unbreakable text reported two lines (a phantom empty line after the truncation exit),
//! doubling the content size and defeating VerticalTextAlignment = Center.

use drawnui::controls::label::LineBreakMode;
use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/bench/assets/OpenSans-Regular.ttf"));

fn host(text: &str) -> Headless<Handle<SkiaLabel>> {
    let build = |label: &mut Handle<SkiaLabel>| {
        SkiaLayout::new().fill().children(
            SkiaLabel::new(text)
                .font_size(17)
                .max_lines(1)
                .line_break_mode(LineBreakMode::TailTruncation)
                .vertical_text_alignment(TextAlignment::Center)
                .horizontal_text_alignment(TextAlignment::Center)
                .height_request(50)
                .fill_x()
                .assign(label),
        )
    };
    let ui = Ui::new(Handle::default(), build).font_bytes("Default", FONT).background(Color::BLACK);
    let mut host = Headless::new(ui, 300, 100, 1.0);
    host.settle();
    host
}

const TEXTS: [&str; 3] = [
    "Demo.App.Very.Long.Namespace.Without.Any.Break.Opportunity.At.All.Truncate.Me",
    "Demo App with spaces that is also far too long to fit into the label width given",
    "Demo.App",
];

#[test]
fn max_lines_1_tail_truncation_reports_one_line() {
    for text in TEXTS {
        let host = host(text);
        let label = host.ui.tree.find::<SkiaLabel>(host.ui.state).unwrap();

        assert_eq!(label.lines_count(), 1, "{text}");
        assert!(label.measured_line_height() > 0.0, "line height not measured");
        assert!(
            label.content_size().height <= label.measured_line_height() * 1.5,
            "content {} spans more than one line of {}",
            label.content_size().height,
            label.measured_line_height()
        );
    }
}

/// Not upstream: what the single line holds.
#[test]
fn the_cut_line_ends_with_the_trail_and_fits() {
    for text in &TEXTS[..2] {
        let host = host(text);
        let label = host.ui.tree.find::<SkiaLabel>(host.ui.state).unwrap();
        let (line, width) = label.lines().next().unwrap();

        // DrawnUi.React: the ellipsis character, cut by advance (C#: "..", cut by ink).
        assert!(line.ends_with('\u{2026}'), "'{line}'");
        assert!(text.starts_with(line.trim_end_matches('\u{2026}')), "'{line}'");
        assert!(width <= 300.0 && width > 260.0, "{width} '{line}'");
    }
    let host = host(TEXTS[2]);
    assert_eq!(host.ui.tree.find::<SkiaLabel>(host.ui.state).unwrap().lines().next().unwrap().0, TEXTS[2]);
}
