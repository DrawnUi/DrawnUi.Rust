//! Dungeon Run on the desktop and in the browser (`dungeon::run`).

// Release builds are a plain Windows app with no console window; debug builds keep the console
// for logs and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dungeon::run();
}
