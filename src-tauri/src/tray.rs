// Notification-area icon: open, talk, mute, pause, settings, quit.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::brain::ISLAND;
use crate::Shared;

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Dos", true, None::<&str>)?;
    let talk = MenuItem::with_id(app, "talk", "Talk to Dos", true, None::<&str>)?;
    let brief = MenuItem::with_id(app, "brief", "Brief me", true, None::<&str>)?;
    let mute = CheckMenuItem::with_id(app, "mute", "Mute voice", true, false, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pause", true, false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Dos Live", true, None::<&str>)?;
    let s1 = PredefinedMenuItem::separator(app)?;
    let s2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &talk, &brief, &s1, &mute, &pause, &settings, &s2, &quit])?;

    let mute_item = mute.clone();
    let pause_item = pause.clone();
    let mut builder = TrayIconBuilder::with_id("dos-live")
        .tooltip("Dos Live")
        .menu(&menu)
        .on_menu_event(move |app: &AppHandle, event| {
            let shared = app.state::<Shared>();
            let brain = shared.brain.clone();
            match event.id.as_ref() {
                "quit" => app.exit(0),
                "settings" => crate::show_settings_window(app),
                "open" => {
                    let _ = app.emit_to(ISLAND, "ui", "expand");
                }
                "talk" => brain.push_to_talk(),
                "brief" => {
                    tauri::async_runtime::spawn(async move { brain.handle(dos_core::intent::Intent::Status).await });
                }
                "mute" => brain.set_muted(mute_item.is_checked().unwrap_or(false)),
                "pause" => brain.set_paused(pause_item.is_checked().unwrap_or(false)),
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
