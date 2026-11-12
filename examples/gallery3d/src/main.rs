//! The 3D gallery on the desktop and in the browser (`gallery3d::run`).

// Release builds are a plain Windows app with no console window; debug builds keep the console
// for logs and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gallery3d::run();
}
