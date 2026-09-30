use std::path::PathBuf;

fn main() {
    // The Windows bundle ships dosctl.exe from ../target/release (see
    // tauri.windows.conf.json), and tauri-build refuses to run when a resource
    // is missing — even for `cargo check` or `cargo clippy`. `npm run pack` and
    // `npm run dev:app` build dosctl first. For plain debug checks, drop an
    // empty stand-in so the check can run. Release builds are left alone: a
    // real bundle must never be made without the real dosctl.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let debug = std::env::var("PROFILE").as_deref() == Ok("debug");
    if windows && debug {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
        let exe = manifest.join("../target/release/dosctl.exe");
        if !exe.exists() {
            if let Some(dir) = exe.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&exe, b"");
            println!("cargo:warning=dosctl.exe not built yet; wrote an empty stand-in for this debug check. Run `cargo build --release -p dosctl` before bundling.");
        }
    }
    tauri_build::build()
}
