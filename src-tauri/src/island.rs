// The island window: a transparent, always-on-top, never-focused strip, docked
// to the top edge of a screen like a notch or dragged anywhere, on any screen. It only takes the mouse over the shape the front end
// says is visible (the bar or the open panel); everywhere else clicks fall
// through to whatever is underneath.
//
// Placement, click-through and the cursor poll follow Coucou's Windows build
// (MIT, © Louis Raillé), trimmed to one window size.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, WebviewWindow};

use crate::brain::ISLAND;
use crate::{platform, settings};

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

// ── where the bar sits ──────────────────────────────────────────────────────
//
// A spot is the window's top-left as logical pixels from the top-left of a
// monitor's work area (the screen minus the taskbar), plus that monitor's
// name, so it lands in the same place after a restart or a DPI change.

/// Within this many logical pixels of the top edge, a dropped bar docks there.
const SNAP_TOP: f64 = 44.0;
/// Within this many of the middle (and docked), it centres itself.
const SNAP_CENTRE: f64 = 70.0;

static DRAGGING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Spot {
    pub monitor: String,
    pub x: f64,
    pub y: f64,
    pub docked: bool,
}

fn load_spot() -> Option<Spot> {
    serde_json::from_slice(&std::fs::read(settings::position_path()).ok()?).ok()
}

fn save_spot(spot: &Spot) {
    let _ = std::fs::create_dir_all(settings::config_dir());
    if let Ok(json) = serde_json::to_vec_pretty(spot) {
        let _ = std::fs::write(settings::position_path(), json);
    }
}

/// Forget the dragged position; the "Screen" setting decides again.
pub fn forget_spot() {
    let _ = std::fs::remove_file(settings::position_path());
}

/// Docked to a top edge (a notch) or floating free — the front end styles each.
pub fn docked() -> bool {
    load_spot().map(|s| s.docked).unwrap_or(true)
}

fn name_of(m: &Monitor) -> String {
    m.name().cloned().unwrap_or_default()
}

/// (x, y, width, height) of the work area in physical pixels.
fn area(m: &Monitor) -> (i32, i32, f64, f64) {
    let wa = m.work_area();
    (wa.position.x, wa.position.y, wa.size.width as f64, wa.size.height as f64)
}

/// Puts the window at a spot on `m`, kept fully on that screen so the panel
/// always has room to open. Returns the spot as actually used.
fn put(win: &WebviewWindow, m: &Monitor, x: f64, y: f64, docked: bool) -> Spot {
    let scale = m.scale_factor();
    let (ax, ay, aw, ah) = area(m);
    let max_x = (aw / scale - WIN_W).max(0.0);
    let max_y = (ah / scale - WIN_H).max(0.0);
    let x = x.clamp(0.0, max_x);
    let y = if docked { 0.0 } else { y.clamp(0.0, max_y) };
    let _ = win.set_size(LogicalSize::new(WIN_W, WIN_H));
    let _ = win.set_position(PhysicalPosition::new(ax + (x * scale).round() as i32, ay + (y * scale).round() as i32));
    let _ = win.set_always_on_top(true);
    Spot { monitor: name_of(m), x, y, docked }
}

fn top_centre(win: &WebviewWindow, m: &Monitor) -> Spot {
    let (_, _, aw, _) = area(m);
    put(win, m, (aw / m.scale_factor() - WIN_W) / 2.0, 0.0, true)
}

fn tell_dock(app: &AppHandle, docked: bool) {
    let _ = app.emit_to(ISLAND, "ui", if docked { "dock:top" } else { "dock:free" });
}

/// Startup and "Screen" changes: the last dragged spot if its screen is still
/// plugged in, otherwise top centre of the screen the setting names.
pub fn place(app: &AppHandle, pref: &str) {
    let Some(win) = window(app) else { return };
    if let Some(spot) = load_spot() {
        let monitors = app.available_monitors().unwrap_or_default();
        if let Some(m) = monitors.iter().find(|m| name_of(m) == spot.monitor) {
            let used = put(&win, m, spot.x, spot.y, spot.docked);
            tell_dock(app, used.docked);
            return;
        }
    }
    if let Some(m) = target_monitor(app, pref) {
        top_centre(&win, &m);
        tell_dock(app, true);
    }
}

/// The monitor under the bar right now (by the bar's middle, not the window's).
fn monitor_under_bar(app: &AppHandle, win: &WebviewWindow, gate: &Gate) -> Option<Monitor> {
    let pos = win.outer_position().ok()?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let r = *gate.rect.lock().unwrap();
    let cx = if r.w > 0.0 { r.x + r.w / 2.0 } else { WIN_W / 2.0 };
    let px = pos.x as f64 + cx * scale;
    let py = pos.y as f64 + (r.y + 15.0) * scale;
    let monitors = app.available_monitors().ok()?;
    monitors
        .iter()
        .find(|m| monitor_contains(m, px, py))
        .cloned()
        .or_else(|| app.primary_monitor().ok().flatten())
}

/// Hands the mouse to Windows' own window-move loop (so the bar follows the
/// cursor across screens, DPI changes included), then settles it on release.
pub fn begin_drag(app: &AppHandle, gate: Arc<Gate>) {
    let Some(win) = window(app) else { return };
    if DRAGGING.swap(true, Ordering::SeqCst) {
        return;
    }
    if win.start_dragging().is_err() {
        DRAGGING.store(false, Ordering::SeqCst);
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new().name("dos-drag".into()).spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        let started = std::time::Instant::now();
        while platform::left_button_down() && started.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(30));
        }
        std::thread::sleep(Duration::from_millis(60));
        settle(&app, &win, &gate);
        DRAGGING.store(false, Ordering::SeqCst);
        let _ = app.emit_to(ISLAND, "ui", "drag-end");
    });
}

/// Where the bar was dropped: dock to the top edge if close, centre if close,
/// keep it on one screen, remember it.
fn settle(app: &AppHandle, win: &WebviewWindow, gate: &Gate) {
    let (Some(m), Ok(pos)) = (monitor_under_bar(app, win, gate), win.outer_position()) else { return };
    let scale = m.scale_factor();
    let (ax, ay, aw, _) = area(&m);
    let mut x = (pos.x - ax) as f64 / scale;
    let y = (pos.y - ay) as f64 / scale;
    let bar_top = y + gate.rect.lock().unwrap().y;
    let docked = bar_top < SNAP_TOP;
    let centre = (aw / scale - WIN_W) / 2.0;
    if docked && (x - centre).abs() < SNAP_CENTRE {
        x = centre;
    }
    let spot = put(win, &m, x, y, docked);
    save_spot(&spot);
    tell_dock(app, spot.docked);
    crate::log::line(format!("island: {} on {}", if spot.docked { "docked" } else { "floating" }, spot.monitor));
}

/// Tray: hop to the top centre of the next screen (left to right, wrapping).
pub fn next_screen(app: &AppHandle, gate: &Gate) {
    let Some(win) = window(app) else { return };
    let mut monitors = app.available_monitors().unwrap_or_default();
    if monitors.is_empty() {
        return;
    }
    monitors.sort_by_key(|m| (m.position().x, m.position().y));
    let here = monitor_under_bar(app, &win, gate).map(|m| name_of(&m)).unwrap_or_default();
    let i = monitors.iter().position(|m| name_of(m) == here).map(|i| (i + 1) % monitors.len()).unwrap_or(0);
    let spot = top_centre(&win, &monitors[i]);
    save_spot(&spot);
    tell_dock(app, true);
}

/// Tray: back into the notch on whichever screen it is on.
pub fn home(app: &AppHandle, gate: &Gate) {
    let Some(win) = window(app) else { return };
    let Some(m) = monitor_under_bar(app, &win, gate) else { return };
    let spot = top_centre(&win, &m);
    save_spot(&spot);
    tell_dock(app, true);
}

/// Every few seconds: if the screen the bar was on has gone (unplugged, or
/// the layout changed), bring it back where it can be seen.
pub fn spawn_screen_watch(app: AppHandle, gate: Arc<Gate>) {
    let _ = std::thread::Builder::new().name("dos-screens".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(4));
        if gate.stop.load(Ordering::Relaxed) {
            return;
        }
        if DRAGGING.load(Ordering::SeqCst) {
            continue;
        }
        let Some(win) = window(&app) else { continue };
        let (Ok(pos), Ok(monitors)) = (win.outer_position(), app.available_monitors()) else { continue };
        let scale = win.scale_factor().unwrap_or(1.0);
        let (px, py) = (pos.x as f64 + WIN_W / 2.0 * scale, pos.y as f64 + 10.0 * scale);
        if !monitors.iter().any(|m| monitor_contains(m, px, py)) {
            let pref = app
                .try_state::<crate::Shared>()
                .map(|s| s.brain.settings.lock().unwrap().ui.screen.clone())
                .unwrap_or_else(|| "primary".into());
            crate::log::line("island: its screen went away — moving it back");
            place(&app, &pref);
        }
    });
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

