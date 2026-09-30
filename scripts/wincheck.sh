#!/usr/bin/env bash
# Type-checks the Windows-only code — the dos-win crate, the app's
# platform_win.rs bridge and dosctl's pipe call — from Linux or macOS.
#
# There is no Windows standard library here, so we can't `cargo check --target
# x86_64-pc-windows-msvc`. Instead we build a throwaway harness that turns on
# cfg(windows) for the host and patches the two `windows-rs` helper crates that
# genuinely need Windows (raw-dylib imports, OsStr↔UTF-16). The result is a
# real type-check of every Win32/WinRT call against the real bindings — it just
# never links. CI on windows-latest does the real build.
#
# serde cannot build with cfg(windows) forced on a non-Windows std, which is why
# platform_win.rs deliberately uses nothing but std and dos-win.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
H="$ROOT/target/wincheck"
mkdir -p "$H/src"

cat > "$H/Cargo.toml" <<EOF
[package]
name = "wincheck"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
dos-win = { path = "$ROOT/crates/dos-win" }
EOF

cat > "$H/src/lib.rs" <<EOF
#![allow(dead_code)]
#[path = "$ROOT/src-tauri/src/platform_win.rs"]
mod platform_win;

fn dosctl_probe() -> std::io::Result<String> {
    dos_win::pipe::request("{}", std::time::Duration::from_secs(3))
}
EOF

# Resolve once without patches to learn the exact versions in use.
rm -f "$H/Cargo.lock"
( cd "$H" && cargo generate-lockfile -q )
ver() { awk -v n="$1" '$1=="name" && $3=="\""n"\"" {getline; gsub(/"/,"",$3); print $3; exit}' "$H/Cargo.lock"; }
LINK_V="$(ver windows-link)"
STR_V="$(ver windows-strings)"
( cd "$H" && cargo fetch -q )
REG="$(dirname "$(ls -d "$HOME"/.cargo/registry/src/*/windows-link-"$LINK_V" | head -1)")"

rm -rf "$H/vendor" && mkdir -p "$H/vendor"
cp -r "$REG/windows-link-$LINK_V" "$H/vendor/windows-link"
cp -r "$REG/windows-strings-$STR_V" "$H/vendor/windows-strings"

python3 - "$H" <<'PY'
import sys, pathlib
h = pathlib.Path(sys.argv[1])
link = h / "vendor/windows-link/src/lib.rs"
s = link.read_text()
s = s.replace('#[cfg(all(windows, target_arch = "x86"))]', '#[cfg(any())]')
s = s.replace('#[cfg(all(windows, not(target_arch = "x86")))]', '#[cfg(any())]')
s = s.replace('#[cfg(not(windows))]', '#[cfg(all())]')
link.write_text(s)
hs = h / "vendor/windows-strings/src/hstring.rs"
s = hs.read_text()
s = s.replace("std::os::windows::ffi::OsStringExt::from_wide(self)", "unimplemented!()")
s = s.replace("std::os::windows::ffi::OsStrExt::encode_wide(value)",
              "value.to_string_lossy().encode_utf16().collect::<Vec<u16>>().into_iter()")
s = s.replace(".eq(std::os::windows::ffi::OsStrExt::encode_wide(other))",
              ".eq(other.to_string_lossy().encode_utf16())")
hs.write_text(s)
PY

cat >> "$H/Cargo.toml" <<EOF

[patch.crates-io]
windows-link = { path = "vendor/windows-link" }
windows-strings = { path = "vendor/windows-strings" }
EOF

cd "$H"
RUSTFLAGS="--cfg windows -A explicit_builtin_cfgs_in_flags" cargo check --quiet "$@"
echo "wincheck: Windows code type-checks against windows-rs"
