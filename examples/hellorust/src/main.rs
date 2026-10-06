//! HelloRust on the desktop and in the browser (`hellorust::run`).

// Release builds are a plain Windows app with no console window; debug builds keep the console
// for logs and panic messages.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hellorust::run();
}
