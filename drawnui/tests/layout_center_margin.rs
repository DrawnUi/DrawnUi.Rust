//! Arrange with margins, all four layout options on both axes (C# `SkiaControl.CalculateLayout`).
//! `centered_with_right_margin_in_narrow_cell_keeps_measured_width` is the port of
//! DrawnUi.Net.Tests `CenterAlignmentMarginTests`; the other numbers were read from the C# engine
//! with the same control in the same box.

use drawnui::prelude::*;
use drawnui::testing::Headless;

fn item(width: i32, height: i32) -> Build<SkiaLayout> {
    SkiaLayout::new().width_request(width).height_request(height)
}

/// The drawing rect of `item` arranged alone in a cell of that size at the origin.
fn place(cell: (i32, i32), item: Build<SkiaLayout>) -> Rect {
    let id = item.id();
    let ui = Ui::new((), |_| SkiaLayout::new().width_request(cell.0).height_request(cell.1).children(item));
    let mut host = Headless::new(ui, 400, 100, 1.0);
    host.settle();
    host.rect(id)
}

fn xywh(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::from_xywh(x as f32, y as f32, w as f32, h as f32)
}

/// ArtOfFoto timer settings: a 13 pt icon with a right margin of 12 in a 32 pt cell. The centered
/// margin box does not fit to the right of the center; it moves back inside instead of being cut.
/// The upstream test file still expects a width of 13 and fails there: it is older than
/// `RoundCenterAlignment`, which gives the icon the odd free pixel. 14 is what the C# engine draws.
#[test]
fn centered_with_right_margin_in_narrow_cell_keeps_measured_width() {
    let icon = place((32, 38), item(13, 10).center().margin((0, 0, 12, 0)));
    assert_eq!(icon.width(), 14.0);
    assert_eq!(icon, xywh(-6, 14, 14, 10));
}

#[test]
fn center_puts_the_content_in_the_middle_and_moves_it_by_the_margin_difference() {
    assert_eq!(place((100, 38), item(20, 10).center().margin((0, 0, 12, 0))), xywh(28, 14, 20, 10));
    assert_eq!(place((100, 38), item(20, 10).center().margin((12, 0, 0, 0))), xywh(52, 14, 20, 10));
    assert_eq!(place((100, 38), item(20, 10).center().margin((12, 0, 4, 0))), xywh(48, 14, 20, 10));
    assert_eq!(place((38, 100), item(10, 20).center().margin((0, 0, 0, 12))), xywh(14, 28, 10, 20));
    assert_eq!(place((38, 100), item(10, 20).center().margin((0, 12, 0, 0))), xywh(14, 52, 10, 20));
}

#[test]
fn center_keeps_the_margin_box_inside_before_the_shift() {
    // Margin box 50 in 60: pushed back from the right edge, then moved left by the margin.
    assert_eq!(place((60, 38), item(20, 10).center().margin((0, 0, 30, 0))), xywh(-20, 14, 20, 10));
    // Pushed back from the left edge, then moved right by the margin.
    assert_eq!(place((60, 38), item(20, 10).center().margin((30, 0, 0, 0))), xywh(60, 14, 20, 10));
}

#[test]
fn center_without_room_starts_at_the_edge() {
    // The margin box is exactly the cell: nothing to center, nothing shifts.
    assert_eq!(place((32, 38), item(20, 10).center().margin((0, 0, 12, 0))), xywh(0, 14, 20, 10));
    // Larger than the cell: cut at the far margin.
    assert_eq!(place((30, 38), item(20, 10).center().margin((0, 0, 12, 0))), xywh(0, 14, 18, 10));
    assert_eq!(place((30, 38), item(20, 10).center().horizontal_options(LayoutOptions::End).margin((0, 0, 12, 0))), xywh(0, 14, 18, 10));
}

#[test]
fn start_end_and_fill_keep_their_margins() {
    assert_eq!(place((100, 38), item(20, 10).horizontal_options(LayoutOptions::End).margin((0, 0, 12, 0))), xywh(68, 0, 20, 10));
    assert_eq!(place((100, 38), item(20, 10).margin((12, 0, 0, 0))), xywh(12, 0, 20, 10));
    assert_eq!(place((100, 38), SkiaLayout::new().height_request(10).fill_x().margin((12, 0, 4, 0))), xywh(12, 0, 84, 10));
    assert_eq!(place((38, 100), item(10, 20).vertical_options(LayoutOptions::End).margin((0, 0, 0, 12))), xywh(0, 68, 10, 20));
    assert_eq!(place((38, 100), SkiaLayout::new().width_request(10).fill_y().margin((0, 12, 0, 4))), xywh(0, 12, 10, 84));
}

#[test]
fn fill_with_a_size_request_is_that_size_at_the_start() {
    assert_eq!(place((100, 38), item(20, 10).fill_x()), xywh(0, 0, 20, 10));
}

/// An odd free pixel goes to the control, so that both gaps are equal (C# `RoundCenterAlignment`).
#[test]
fn center_with_an_odd_free_pixel_takes_it() {
    assert_eq!(place((32, 38), item(13, 10).center()), xywh(9, 14, 14, 10));
}
