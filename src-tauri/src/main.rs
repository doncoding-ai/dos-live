// No console window: the island and the tray icon are the whole app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dos_live_lib::run()
}
