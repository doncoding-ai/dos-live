//! dos-win — every call into Windows that Dos Live makes, behind small safe
//! functions. Nothing here knows about Trello or Tauri.
//!
//! Only compiled for Windows. `scripts/wincheck.sh` type-checks it on Linux.

#![cfg(windows)]

pub mod callguard;
pub mod pipe;
pub mod sid;
pub mod voice;
pub mod window;

/// UTF-16, NUL-terminated, for PCWSTR arguments.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
