// %LOCALAPPDATA%\Dos Live\dos-live.log — what happened, never what was said.
// Voice transcripts, ask texts and card contents are not logged.

use std::io::Write;

pub fn line(message: impl AsRef<str>) {
    // `npm run dev:app` has a terminal: show the log there too.
    #[cfg(debug_assertions)]
    eprintln!("[dos-live] {}", message.as_ref());
    let dir = crate::settings::local_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("dos-live.log");
    if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("dos-live.old.log"));
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{stamp} {}", message.as_ref());
    }
}
