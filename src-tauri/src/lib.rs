// Dos Live — app wiring and the commands the two windows call.

mod brain;
mod hermes;
mod island;
mod log;
mod platform;
mod secrets;
mod settings;
mod trello;
mod tray;

use std::sync::Arc;

use dos_core::intent::Intent;
use dos_core::Decision;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use brain::{Brain, ViewState, ISLAND};
use island::Gate;
use settings::Settings;

pub struct Shared {
    pub brain: Arc<Brain>,
    pub gate: Arc<Gate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Keys {
    trello_key: bool,
    trello_token: bool,
    hermes_key: bool,
}

fn keys() -> Keys {
    Keys {
        trello_key: secrets::present("trello-key"),
        trello_token: secrets::present("trello-token"),
        hermes_key: secrets::present("hermes-key"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootInfo {
    settings: Settings,
    state: ViewState,
    keys: Keys,
    version: String,
    windows: bool,
}

// Every command is async: Tauri runs sync commands on the main (UI) thread,
// and anything there that waits — a lock, the Credential Manager, the pipe —
// freezes the window ("Not responding"). Async commands run on the worker pool.
// Commands borrowing State must return Result (a Tauri rule), hence `R<()>`.

type R<T> = Result<T, String>;

#[tauri::command]
async fn boot(shared: State<'_, Shared>) -> R<BootInfo> {
    let settings = shared.brain.settings.lock().unwrap().clone();
    Ok(BootInfo {
        settings,
        state: shared.brain.view(),
        keys: keys(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        windows: cfg!(windows),
    })
}

#[tauri::command]
async fn save_settings(app: AppHandle, shared: State<'_, Shared>, settings: Settings) -> R<()> {
    let old = shared.brain.settings.lock().unwrap().clone();
    *shared.brain.settings.lock().unwrap() = settings.clone();
    settings::save(&settings).map_err(|e| e.to_string())?;

    if old.voice != settings.voice || old.worlds != settings.worlds {
        shared.brain.apply_voice_mode();
    }
    if old.voice.hotkey != settings.voice.hotkey {
        register_hotkey(&app, &settings.voice.hotkey);
    }
    if old.ui.autostart != settings.ui.autostart {
        let m = app.autolaunch();
        let _ = if settings.ui.autostart { m.enable() } else { m.disable() };
    }
    if old.ui.screen != settings.ui.screen {
        island::place(&app, &settings.ui.screen);
    }
    if old.ui.stealth != settings.ui.stealth && !island::set_stealth(&app, settings.ui.stealth) && settings.ui.stealth {
        log::line("stealth: Windows refused to hide the window from capture");
    }
    if old.boards != settings.boards || old.layout != settings.layout || old.worlds != settings.worlds || old.poll_secs != settings.poll_secs {
        shared.brain.refresh();
    }
    let _ = app.emit("settings-changed", settings);
    shared.brain.emit_state();
    Ok(())
}

#[tauri::command]
async fn state(shared: State<'_, Shared>) -> R<ViewState> {
    Ok(shared.brain.view())
}

#[tauri::command]
async fn set_rect(shared: State<'_, Shared>, x: f64, y: f64, width: f64, height: f64) -> R<()> {
    *shared.gate.rect.lock().unwrap() = island::Rect { x, y, w: width, h: height };
    Ok(())
}

#[tauri::command]
async fn set_keyboard(app: AppHandle, typing: bool) {
    island::set_keyboard(&app, typing);
}

#[tauri::command]
async fn decide(shared: State<'_, Shared>, ask_id: String, decision: String) -> R<()> {
    let d = Decision::parse(&decision).ok_or("decision must be do_it, hold or skip")?;
    shared.brain.decide(&ask_id, d).await
}

#[tauri::command]
async fn refresh(shared: State<'_, Shared>) -> R<()> {
    shared.brain.refresh();
    Ok(())
}

#[tauri::command]
async fn open_link(url: String) {
    open_url(url);
}

#[tauri::command]
async fn push_to_talk(shared: State<'_, Shared>) -> R<()> {
    shared.brain.push_to_talk();
    Ok(())
}

#[tauri::command]
async fn say(shared: State<'_, Shared>, text: String) -> R<()> {
    shared.brain.speak(&text);
    Ok(())
}

#[tauri::command]
async fn run_intent(shared: State<'_, Shared>, intent: Intent) -> R<()> {
    let brain = shared.brain.clone();
    tauri::async_runtime::spawn(async move { brain.handle(intent).await });
    Ok(())
}

#[tauri::command]
async fn run_text(shared: State<'_, Shared>, text: String) -> R<()> {
    shared.brain.typed(text.chars().take(500).collect());
    Ok(())
}

#[tauri::command]
async fn speech_done(shared: State<'_, Shared>, id: u64) -> R<()> {
    shared.brain.speech_finished(id);
    Ok(())
}

#[tauri::command]
async fn stop_speaking(shared: State<'_, Shared>) -> R<()> {
    shared.brain.stop_speaking();
    Ok(())
}

#[tauri::command]
async fn set_muted(shared: State<'_, Shared>, muted: bool) -> R<()> {
    shared.brain.set_muted(muted);
    Ok(())
}

#[tauri::command]
async fn set_paused(shared: State<'_, Shared>, paused: bool) -> R<()> {
    shared.brain.set_paused(paused);
    Ok(())
}

#[tauri::command]
async fn create_note(shared: State<'_, Shared>, text: String) -> R<()> {
    shared.brain.create_note(&text).await
}

#[tauri::command]
async fn secret_present(key: String) -> bool {
    secrets::present(&key)
}

#[tauri::command]
async fn secret_set(shared: State<'_, Shared>, key: String, value: String) -> R<()> {
    secrets::set(&key, &value)?;
    shared.brain.keys_changed();
    Ok(())
}

#[tauri::command]
async fn secret_clear(shared: State<'_, Shared>, key: String) -> R<()> {
    secrets::clear(&key)?;
    shared.brain.keys_changed();
    Ok(())
}

#[tauri::command]
async fn key_status() -> Keys {
    keys()
}

#[tauri::command]
async fn test_trello(shared: State<'_, Shared>) -> R<String> {
    shared.brain.test_trello().await
}

#[tauri::command]
async fn setup_lists(shared: State<'_, Shared>) -> R<String> {
    shared.brain.setup_lists().await
}

#[tauri::command]
async fn test_hermes(shared: State<'_, Shared>) -> R<String> {
    let s = shared.brain.settings.lock().unwrap().hermes.clone();
    hermes::ask(&s.url, &s.model, &[], "Reply with exactly: Dos Live connected.").await
}

#[tauri::command]
async fn test_relay() -> R<String> {
    platform::relay_self_test().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_voices(shared: State<'_, Shared>) -> R<()> {
    shared.brain.list_voices();
    Ok(())
}

#[tauri::command]
async fn open_settings(app: AppHandle) {
    show_settings_window(&app);
}

#[tauri::command]
async fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
async fn log_line(message: String) {
    log::line(format!("ui  {}", message.chars().take(300).collect::<String>()));
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Opens http(s) links in the default browser; anything else is ignored.
pub fn open_url(url: String) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
}

fn register_hotkey(app: &AppHandle, hotkey: &str) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if hotkey.trim().is_empty() {
        return;
    }
    if let Err(e) = gs.register(hotkey.trim()) {
        log::line(format!("hotkey {hotkey}: {e}"));
    }
}

fn settings_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/settings.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("settings.html".into())
}

/// Created hidden at launch and only shown/hidden afterwards: a WebView2 window
/// created late can come up blank (learned the hard way in Coucou).
fn create_settings_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, "settings", settings_url(app))
        .title("Dos Live — Settings")
        .inner_size(600.0, 760.0)
        .min_inner_size(480.0, 520.0)
        .resizable(true)
        .visible(false)
        .center();
    // Must match the island's browser arguments or WebView2 refuses the window.
    #[cfg(windows)]
    let builder = builder.additional_browser_args(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required",
    );
    match builder.build() {
        Ok(win) => {
            let hidden = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hidden.hide();
                }
            });
        }
        Err(e) => log::line(format!("settings window failed: {e}")),
    }
}

pub fn show_settings_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn run() {
    // No console in release builds and panics abort, so a crash would vanish
    // without a trace. Write it to the log first.
    std::panic::set_hook(Box::new(|info| {
        log::line(format!("PANIC: {info}"));
    }));
    log::line(format!("--- Dos Live {} starting ---", env!("CARGO_PKG_VERSION")));

    // `Dos Live.exe --safe` (or DOSLIVE_SAFE=1): no voice, no call guard, no
    // relay — just the island and Trello. For telling which part misbehaves.
    let safe = std::env::args().any(|a| a == "--safe") || std::env::var_os("DOSLIVE_SAFE").is_some();
    let loaded = settings::load();
    let gate = Gate::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = app.emit_to(ISLAND, "ui", "expand");
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // Runs on the UI thread: hand off, and never assume setup is done.
                    if let Some(shared) = app.try_state::<Shared>() {
                        let brain = shared.brain.clone();
                        tauri::async_runtime::spawn(async move { brain.push_to_talk() });
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            boot,
            save_settings,
            state,
            set_rect,
            set_keyboard,
            decide,
            refresh,
            open_link,
            push_to_talk,
            say,
            run_intent,
            run_text,
            speech_done,
            stop_speaking,
            set_muted,
            set_paused,
            create_note,
            secret_present,
            secret_set,
            secret_clear,
            key_status,
            test_trello,
            setup_lists,
            test_hermes,
            test_relay,
            list_voices,
            open_settings,
            quit_app,
            log_line,
        ])
        .setup(move |app| {
            let step = |s: &str| log::line(format!("setup: {s}"));
            let handle = app.handle().clone();
            let brain = Brain::new(handle.clone(), loaded.clone());
            app.manage(Shared { brain: brain.clone(), gate: gate.clone() });
            step("state ready");

            tray::build(&handle)?;
            step("tray ready");
            create_settings_window(&handle);
            step("settings window ready");

            let mut hwnd = None;
            if let Some(win) = island::window(&handle) {
                island::place(&handle, &loaded.ui.screen);
                if !island::prepare(&win, loaded.ui.stealth) && loaded.ui.stealth {
                    log::line("stealth unavailable on this Windows build");
                }
                hwnd = island::raw_hwnd(&win);
                let _ = win.set_ignore_cursor_events(true);
                let _ = win.show();
            }
            island::spawn_cursor_poll(handle.clone(), gate.clone(), hwnd);
            step("island ready");

            if loaded.ui.autostart {
                let _ = handle.autolaunch().enable();
            }
            register_hotkey(&handle, &loaded.voice.hotkey);
            step("hotkey ready");

            tauri::async_runtime::spawn(brain.clone().run_poller());

            if safe {
                log::line("safe mode: voice, call guard and relay are off");
            } else {
                // Let the window come up and settle before the microphone and
                // the speech engine start; neither belongs on the startup path.
                let late = brain.clone();
                std::thread::Builder::new()
                    .name("dos-late-start".into())
                    .spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        late.start_voice();
                        log::line("setup: voice started");
                        late.clone().run_call_guard();
                        let relay_brain = late.clone();
                        match platform::serve_relay(move |line| relay_brain.relay(line)) {
                            Ok(()) => log::line("setup: relay ready"),
                            Err(e) => log::line(format!("relay pipe unavailable: {e}")),
                        }
                    })
                    .expect("late start thread");
            }
            log::line(format!("--- Dos Live {} started ---", env!("CARGO_PKG_VERSION")));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Dos Live");
}
