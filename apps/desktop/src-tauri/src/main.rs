// Prevents additional console window on Windows in release.
// macOS-only target, but kept for future cross-compile sanity.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quill_desktop_lib::run();
}
