#!/usr/bin/env bash
# Type-checks and lints the Windows-only code — the dos-win crate, the app's
# platform_win.rs bridge and dosctl's pipe call — from Linux or macOS.
#
# There is no Windows standard library here, so we can't `cargo check --target
# x86_64-pc-windows-msvc`. Instead we build a throwaway harness that turns on
# cfg(windows) for the host and patches the two `windows-rs` helper crates that
# genuinely need Windows (raw-dylib imports, OsStr↔UTF-16). The result is a
# real type-check of every Win32/WinRT call against the real bindings — it just
# never links. It also runs clippy with -D warnings, the same bar CI applies on
# windows-latest.
#
# serde cannot build with cfg(windows) forced on a non-Windows std, which is why
# platform_win.rs deliberately uses nothing but std and dos-win.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
H="$ROOT/target/wincheck"
rm -rf "$H/win" "$H/app" "$H/src" "$H/vendor" "$H/Cargo.lock"
mkdir -p "$H/win" "$H/app/src"

# Two workspace members so clippy lints both (it skips path dependencies):
#   win — the real crates/dos-win sources under their own package name,
#   app — the app's platform_win.rs plus dosctl's one pipe call.
python3 - "$ROOT" "$H" <<'PY'
import sys, pathlib, tomllib
root, h = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
ws = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["dependencies"]
win = tomllib.loads((root / "crates/dos-win/Cargo.toml").read_text())["dependencies"]

def dep(name, spec):
    if isinstance(spec, dict) and spec.get("workspace"):
        spec = ws[name]
    if isinstance(spec, str):
        return f'{name} = "{spec}"'
    parts = [f'version = "{spec["version"]}"']
    if "features" in spec:
        parts.append("features = [" + ", ".join(f'"{f}"' for f in spec["features"]) + "]")
    return f"{name} = {{ {', '.join(parts)} }}"

deps = "\n".join(dep(n, s) for n, s in win.items())
(h / "Cargo.toml").write_text('[workspace]\nmembers = ["win", "app"]\nresolver = "2"\n')
(h / "win/Cargo.toml").write_text(f'''[package]
name = "dos-win"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "{root}/crates/dos-win/src/lib.rs"

[dependencies]
{deps}
''')
(h / "app/Cargo.toml").write_text('''[package]
name = "wincheck-app"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
dos-win = { path = "../win" }
''')
(h / "app/src/lib.rs").write_text(f'''#![allow(dead_code)]
#[path = "{root}/src-tauri/src/platform_win.rs"]
mod platform_win;

fn dosctl_probe() -> std::io::Result<String> {{
    dos_win::pipe::request("{{}}", std::time::Duration::from_secs(3))
}}
''')
PY

# Resolve once without patches to learn the exact versions in use.
( cd "$H" && cargo generate-lockfile -q )
ver() { awk -v n="$1" '$1=="name" && $3=="\""n"\"" {getline; gsub(/"/,"",$3); print $3; exit}' "$H/Cargo.lock"; }
LINK_V="$(ver windows-link)"
STR_V="$(ver windows-strings)"
( cd "$H" && cargo fetch -q )
REG="$(dirname "$(ls -d "$HOME"/.cargo/registry/src/*/windows-link-"$LINK_V" | head -1)")"

mkdir -p "$H/vendor"
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
RUSTFLAGS="--cfg windows -A explicit_builtin_cfgs_in_flags" cargo clippy --quiet --workspace --all-targets "$@" -- -D warnings
echo "wincheck: Windows code type-checks and passes clippy against windows-rs"
