//! The island window's Win32 flags: never steal focus, stay out of Alt-Tab,
//! and — the stealth part — never appear in a screen share or recording.

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE, WDA_EXCLUDEFROMCAPTURE,
    WDA_NONE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

/// WS_EX_NOACTIVATE: clicks never steal focus. WS_EX_TOOLWINDOW: not in Alt-Tab.
pub fn make_non_activating(raw: isize) {
    unsafe {
        let h = hwnd(raw);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize);
    }
}

/// Lets a text field take the keyboard while it is being typed in.
pub fn set_activating(raw: isize, activating: bool) {
    unsafe {
        let h = hwnd(raw);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let want = if activating { ex & !(WS_EX_NOACTIVATE.0 as isize) } else { ex | WS_EX_NOACTIVATE.0 as isize };
        SetWindowLongPtrW(h, GWL_EXSTYLE, want);
    }
}

/// With `hidden`, screen capture (Teams, Meet, Zoom, OBS, the Snipping Tool)
/// sees nothing where the island is, while you still see it. Windows 10 2004+.
/// Returns false when Windows refused (older builds): the caller should say so.
pub fn exclude_from_capture(raw: isize, hidden: bool) -> bool {
    unsafe { SetWindowDisplayAffinity(hwnd(raw), if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE }).is_ok() }
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x, p.y))
}

pub fn left_button_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}
