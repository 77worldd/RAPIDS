// Hides the console window for release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    rapids_lib::run();
}
