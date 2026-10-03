// The island window: a transparent, always-on-top, never-focused strip at the
// top centre of the screen. It only takes the mouse over the shape the front end
// says is visible (the bar or the open panel); everywhere else clicks fall
// through to whatever is underneath.
//
// Placement, click-through and the cursor poll follow Coucou's Windows build
// (MIT, © Louis Raillé), trimmed to one window size.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::brain::ISLAND;
use crate::platform;

/// Logical size of the island window — the open panel plus room for shadows.
pub const WIN_W: f64 = 760.0;
pub const WIN_H: f64 = 380.0;
/// Extra margin that still counts as "on the island" so a click is never lost.
const HIT_MARGIN: f64 = 10.0;

#[derive(Clone, Copy, Default, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Serialize, Clone)]
pub struct Cursor {
    pub x: f64,
    pub y: f64,
    pub inside: bool,
}

pub struct Gate {
    pub rect: Mutex<Rect>,
    ignoring: AtomicBool,
    pub stop: AtomicBool,
}

impl Gate {
    pub fn new() -> Arc<Gate> {
        Arc::new(Gate { rect: Mutex::new(Rect::default()), ignoring: AtomicBool::new(false), stop: AtomicBool::new(false) })
    }
}

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(ISLAND)
}

fn monitor_contains(m: &Monitor, x: f64, y: f64) -> bool {
    let p = m.position();
    let s = m.size();
    x >= p.x as f64 && x < (p.x + s.width as i32) as f64 && y >= p.y as f64 && y < (p.y + s.height as i32) as f64
}

fn target_monitor(app: &AppHandle, pref: &str) -> Option<Monitor> {
    let monitors = app.available_monitors().ok()?;
    if pref == "cursor" {
        if let Some((cx, cy)) = platform::cursor_pos() {
            if let Some(m) = monitors.iter().find(|m| monitor_contains(m, cx as f64, cy as f64)) {
                return Some(m.clone());
            }
        }
    }
    app.primary_monitor().ok().flatten().or_else(|| monitors.into_iter().next())
}

pub fn place(app: &AppHandle, pref: &str) {
    let Some(win) = window(app) else { return };
    let Some(m) = target_monitor(app, pref) else { return };
    let scale = m.scale_factor();
    let mp = *m.position();
    let ms = *m.size();
    let pw = (WIN_W * scale).round() as u32;
    let ph = (WIN_H * scale).round() as u32;
    let x = mp.x + (ms.width as i32 - pw as i32) / 2;
    let _ = win.set_size(PhysicalSize::new(pw, ph));
    let _ = win.set_position(PhysicalPosition::new(x, mp.y));
    let _ = win.set_size(PhysicalSize::new(pw, ph));
    let _ = win.set_always_on_top(true);
}

#[cfg(windows)]
pub fn raw_hwnd(win: &WebviewWindow) -> Option<isize> {
    win.hwnd().ok().map(|h| h.0 as isize).filter(|h| *h != 0)
}

#[cfg(not(windows))]
pub fn raw_hwnd(_win: &WebviewWindow) -> Option<isize> {
    None
}

pub fn prepare(win: &WebviewWindow, stealth: bool) -> bool {
    let Some(h) = raw_hwnd(win) else { return false };
    platform::make_non_activating(h);
    platform::exclude_from_capture(h, stealth)
}

/// Returns false when Windows would not hide the window from capture.
pub fn set_stealth(app: &AppHandle, on: bool) -> bool {
    window(app).and_then(|w| raw_hwnd(&w)).map(|h| platform::exclude_from_capture(h, on)).unwrap_or(false)
}

pub fn set_keyboard(app: &AppHandle, typing: bool) {
    let Some(win) = window(app) else { return };
    if let Some(h) = raw_hwnd(&win) {
        platform::set_activating(h, typing);
    }
    if typing {
        let _ = win.set_focus();
    }
}

/// ~30 Hz while the cursor is near the top of the screen, ~4 Hz otherwise.
/// Decides click-through and feeds the front end the cursor for Dos's eyes.
///
/// With `hwnd` (Windows), position and DPI are read straight from Win32, so the
/// loop never waits on the UI thread; the only UI-thread call left is the
/// click-through toggle, made only when the cursor crosses the island's edge.
pub fn spawn_cursor_poll(app: AppHandle, gate: Arc<Gate>, hwnd: Option<isize>) {
    std::thread::Builder::new()
        .name("dos-cursor".into())
        .spawn(move || {
            let mut last = (f64::MIN, f64::MIN);
            let Some(win) = window(&app) else { return };
            loop {
                if gate.stop.load(Ordering::Relaxed) {
                    return;
                }
                let (origin, scale) = match hwnd {
                    Some(h) => (platform::window_origin(h), platform::dpi_scale(h)),
                    None => (
                        win.outer_position().ok().map(|p| (p.x, p.y)),
                        win.scale_factor().unwrap_or(1.0),
                    ),
                };
                let (Some((ox, oy)), Some((cx, cy))) = (origin, platform::cursor_pos()) else {
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                };
                let x = (cx - ox) as f64 / scale;
                let y = (cy - oy) as f64 / scale;
                let near = y < WIN_H + 120.0 && x > -200.0 && x < WIN_W + 200.0;
                std::thread::sleep(Duration::from_millis(if near { 33 } else { 250 }));
                if (x - last.0).abs() < 1.0 && (y - last.1).abs() < 1.0 {
                    continue;
                }
                last = (x, y);
                let r = *gate.rect.lock().unwrap();
                let inside = r.w > 0.0
                    && x >= r.x - HIT_MARGIN
                    && x <= r.x + r.w + HIT_MARGIN
                    && y >= r.y - HIT_MARGIN
                    && y <= r.y + r.h + HIT_MARGIN;
                if gate.ignoring.load(Ordering::Relaxed) == inside {
                    gate.ignoring.store(!inside, Ordering::Relaxed);
                    let _ = win.set_ignore_cursor_events(!inside);
                }
                if near {
                    let _ = app.emit_to(ISLAND, "cursor", Cursor { x, y, inside });
                }
            }
        })
        .expect("cursor thread");
}

