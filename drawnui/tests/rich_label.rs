//! SkiaRichLabel markdown: the spans it builds equal those of DrawnUi.React's parser (a node
//! probe over its `dist` parsed the same texts with `SkiaRichLabel.Text`, FontSize 15). Tricky
//! input on purpose: escapes, nesting, unclosed markers, links with titles, multibyte text,
//! headings, lists and their numbering, fences, line endings.

use drawnui::prelude::*;
use drawnui::testing::Headless;

const FONT: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hellorust/assets/OpenSans-Regular.ttf"));

/// Markdown and its spans: text, flags (b bold, i italic, s strikeout, u underline, c takes
/// taps), font size (0 = the label's), text color, background color, tag.
type Case = (&'static str, &'static [(&'static str, &'static str, i32, &'static str, &'static str, &'static str)]);
const REACT: &[Case] = &[
    ("plain *it* and **bold** and __b2__ and _i2_", &[("plain ", "", 0, "", "", ""), ("it", "i", 0, "", "", ""), (" and ", "", 0, "", "", ""), ("bold", "b", 0, "", "", ""), (" and ", "", 0, "", "", ""), ("b2", "b", 0, "", "", ""), (" and ", "", 0, "", "", ""), ("i2", "i", 0, "", "", "")]),
    ("esc \\*not\\* and \\\\ and a\\`b", &[("esc *not* and \\ and a`b", "", 0, "", "", "")]),
    ("**bold *nested italic* bold** ~~strike **sb**~~", &[("bold ", "b", 0, "", "", ""), ("nested italic", "bi", 0, "", "", ""), (" bold", "b", 0, "", "", ""), (" ", "", 0, "", "", ""), ("strike ", "s", 0, "", "", ""), ("sb", "bs", 0, "", "", "")]),
    ("unclosed **bold and *it and ~~st", &[("unclosed **bold and *it and ~~st", "", 0, "", "", "")]),
    ("***both*** and ****", &[("*both", "b", 0, "", "", ""), ("* and ****", "", 0, "", "", "")]),
    ("\u{E9}**\u{F1}**\u{FC} *\u{E7}* \u{2192} ~~\u{DF}~~", &[("\u{E9}", "", 0, "", "", ""), ("\u{F1}", "b", 0, "", "", ""), ("\u{FC} ", "", 0, "", "", ""), ("\u{E7}", "i", 0, "", "", ""), (" \u{2192} ", "", 0, "", "", ""), ("\u{DF}", "s", 0, "", "", "")]),
    ("[link](http://a.b \"title\") [](http://empty) [bad] (x) [x](y z) [t](u)", &[("link", "uc", 0, "#6495ED", "", "http://a.b"), (" ", "", 0, "", "", ""), ("http://empty", "uc", 0, "#6495ED", "", "http://empty"), (" [bad] (x) [x](y z) ", "", 0, "", "", ""), ("t", "uc", 0, "#6495ED", "", "u")]),
    ("# C#\n## Trailing ##\n####### seven\n#nospace", &[("C\n", "b", 24, "#FFFFFF", "", ""), ("Trailing\n", "b", 19, "#FFFFFF", "", ""), ("####### seven\n#nospace", "", 0, "", "", "")]),
    ("- a\n  continued\n- b\n\n1) one\n5. two\n* star\n+ plus", &[("\u{2022} ", "", 0, "", "", ""), ("a\n", "", 0, "", "", ""), ("continued\n", "", 0, "", "", ""), ("\u{2022} ", "", 0, "", "", ""), ("b\n", "", 0, "", "", ""), ("1. ", "", 0, "", "", ""), ("one\n", "", 0, "", "", ""), ("2. ", "", 0, "", "", ""), ("two\n", "", 0, "", "", ""), ("\u{2022} ", "", 0, "", "", ""), ("star\n", "", 0, "", "", ""), ("\u{2022} ", "", 0, "", "", ""), ("plus", "", 0, "", "", "")]),
    ("```\n```\npara", &[("\n", "", 0, "", "", ""), ("para", "", 0, "", "", "")]),
    ("``\n`code` x ` y", &[("\n", "", 0, "", "", ""), ("code", "", 0, "#FFFFFF", "#696969", ""), (" x ` y", "", 0, "", "", "")]),
    ("line1\r\nline2\rline3", &[("line1\nline2\nline3", "", 0, "", "", "")]),
    ("* not a list*", &[("\u{2022} ", "", 0, "", "", ""), ("not a list*", "", 0, "", "", "")]),
    ("a * b * c _ d _", &[("a * b * c _ d _", "", 0, "", "", "")]),
    ("  indented para\n\n\n\nnext", &[("  indented para\n", "", 0, "", "", ""), ("next", "", 0, "", "", "")]),
    ("1. x\n- y\n2. z", &[("1. ", "", 0, "", "", ""), ("x\n", "", 0, "", "", ""), ("\u{2022} ", "", 0, "", "", ""), ("y\n", "", 0, "", "", ""), ("2. ", "", 0, "", "", ""), ("z", "", 0, "", "", "")]),
    ("**a\\**b**", &[("a**b", "b", 0, "", "", "")]),
];

/// The spans of the React TextPage markdown.
const PAGE: Case = ("# Heading 1\n## Heading 2\n### Heading 3\nA paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code` and a [tappable link](https://drawnui.net).\nSoft line breaks stay inside the paragraph.\n\n- Bullet item with **bold**\n- Second bullet\n1. Numbered item\n2. Another one, *emphasised*\n\n```\nconst label = new SkiaRichLabel();\nlabel.Text = \"# Hello\";\n```", &[
    ("Heading 1\n", "b", 24, "#FFFFFF", "", ""),
    ("Heading 2\n", "b", 19, "#FFFFFF", "", ""),
    ("Heading 3\n", "b", 17, "#FFFFFF", "", ""),
    ("A paragraph with ", "", 0, "", "", ""),
    ("bold", "b", 0, "", "", ""),
    (", ", "", 0, "", "", ""),
    ("italic", "i", 0, "", "", ""),
    (", ", "", 0, "", "", ""),
    ("strikethrough", "s", 0, "", "", ""),
    (", ", "", 0, "", "", ""),
    ("inline code", "", 0, "#FFFFFF", "#696969", ""),
    (" and a ", "", 0, "", "", ""),
    ("tappable link", "uc", 0, "#6495ED", "", "https://drawnui.net"),
    (".\nSoft line breaks stay inside the paragraph.\n", "", 0, "", "", ""),
    ("\u{2022} ", "", 0, "", "", ""),
    ("Bullet item with ", "", 0, "", "", ""),
    ("bold\n", "b", 0, "", "", ""),
    ("\u{2022} ", "", 0, "", "", ""),
    ("Second bullet\n", "", 0, "", "", ""),
    ("1. ", "", 0, "", "", ""),
    ("Numbered item\n", "", 0, "", "", ""),
    ("2. ", "", 0, "", "", ""),
    ("Another one, ", "", 0, "", "", ""),
    ("emphasised\n", "i", 0, "", "", ""),
    ("const label = new SkiaRichLabel();\n", "", 0, "#FFFFFF", "#222222", ""),
    ("label.Text = \"# Hello\";", "", 0, "#FFFFFF", "#222222", ""),
]);

fn hex(color: Option<Color>) -> String {
    color.map_or(String::new(), |c| format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()))
}

/// The spans a rich label built from `markdown`, in the shape of the table.
fn spans(markdown: &str) -> Vec<(String, String, f32, String, String, String)> {
    let build = |rich: &mut Handle<SkiaRichLabel>| SkiaLayout::new().fill().children(SkiaRichLabel::new(markdown).font_size(15).assign(rich));
    let mut host = Headless::new(Ui::new(Handle::default(), build).font_bytes("FontText", FONT), 400, 400, 1.0);
    host.settle();
    let rich: &SkiaRichLabel = host.ui.tree.find(host.ui.state).unwrap();
    rich.label()
        .spans
        .iter()
        .map(|s| {
            let flags = [(s.is_bold, 'b'), (s.is_italic, 'i'), (s.strikeout, 's'), (s.underline, 'u'), (s.force_capture_input, 'c')];
            let flags = flags.iter().filter(|f| f.0).map(|f| f.1).collect();
            (s.text.clone(), flags, s.font_size.unwrap_or(0.0), hex(s.text_color), hex(s.background_color), s.tag.clone())
        })
        .collect()
}

#[test]
fn markdown_parses_as_the_react_engine() {
    for (markdown, expected) in REACT.iter().chain([&PAGE]) {
        let expected: Vec<_> =
            expected.iter().map(|e| (e.0.to_owned(), e.1.to_owned(), e.2 as f32, e.3.to_owned(), e.4.to_owned(), e.5.to_owned())).collect();
        assert_eq!(spans(markdown), expected, "{markdown:?}");
    }
}

#[test]
fn without_markdown_the_text_is_one_span() {
    let build = |rich: &mut Handle<SkiaRichLabel>| {
        SkiaLayout::new().fill().children(SkiaRichLabel::new("**not bold** `x`").markdown_enabled(false).assign(rich))
    };
    let mut host = Headless::new(Ui::new(Handle::default(), build).font_bytes("FontText", FONT), 400, 100, 1.0);
    host.settle();
    let rich: &SkiaRichLabel = host.ui.tree.find(host.ui.state).unwrap();
    let texts: Vec<&str> = rich.label().spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, ["**not bold** `x`"]);
}

#[test]
fn a_new_text_or_font_size_builds_the_spans_again() {
    let build = |rich: &mut Handle<SkiaRichLabel>| SkiaLayout::new().fill().children(SkiaRichLabel::new("# Title").font_size(15).assign(rich));
    let mut host = Headless::new(Ui::new(Handle::default(), build).font_bytes("FontText", FONT), 400, 100, 1.0);
    host.settle();
    let id = host.ui.state;
    host.ui.tree.find_mut::<SkiaRichLabel>(id).unwrap().set_font_size(20);
    host.settle();
    assert_eq!(host.ui.tree.find::<SkiaRichLabel>(id).unwrap().label().spans[0].font_size, Some(29.0));
    host.ui.tree.find_mut::<SkiaRichLabel>(id).unwrap().set_text("*plain*");
    host.settle();
    let rich = host.ui.tree.find::<SkiaRichLabel>(id).unwrap();
    assert_eq!((rich.label().spans[0].text.as_str(), rich.label().spans[0].is_italic), ("plain", true));
}
