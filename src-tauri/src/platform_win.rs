// The app's one door into Windows. Uses std and dos-win only — no serde, no
// tauri — so scripts/wincheck.sh can type-check it from Linux.
//
// platform.rs mirrors every item here for non-Windows builds, so the rest of
// the app compiles (and is checked) everywhere.

use std::time::Duration;

#[allow(unused_imports)] // VoiceInfo mirrors platform.rs; Heard is used by brain.rs
pub use dos_win::voice::{Heard, Voice, VoiceEvent, VoiceInfo};

pub fn make_non_activating(hwnd: isize) {
    dos_win::window::make_non_activating(hwnd)
}

pub fn set_activating(hwnd: isize, activating: bool) {
    dos_win::window::set_activating(hwnd, activating)
}

/// True when Windows accepted the request.
pub fn exclude_from_capture(hwnd: isize, hidden: bool) -> bool {
    dos_win::window::exclude_from_capture(hwnd, hidden)
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    dos_win::window::cursor_pos()
}

pub fn window_origin(hwnd: isize) -> Option<(i32, i32)> {
    dos_win::window::window_origin(hwnd)
}

pub fn dpi_scale(hwnd: isize) -> f64 {
    dos_win::window::dpi_scale(hwnd)
}

/// Apps other than Dos Live holding the microphone right now.
pub fn mic_users() -> Vec<String> {
    dos_win::callguard::mic_users()
}

/// Starts the relay pipe server; `handle` maps a request line to a reply line.
pub fn serve_relay<F>(handle: F) -> std::io::Result<()>
where
    F: Fn(String) -> String + Send + Sync + 'static,
{
    dos_win::pipe::serve(handle)
}

/// For the settings window's "test" button: talk to ourselves over the pipe.
pub fn relay_self_test() -> std::io::Result<String> {
    dos_win::pipe::request(r#"{"op":"ping"}"#, Duration::from_secs(2))
}

pub fn start_voice<F>(on_event: F) -> Voice
where
    F: Fn(VoiceEvent) + Send + Sync + 'static,
{
    Voice::start(on_event)
}
