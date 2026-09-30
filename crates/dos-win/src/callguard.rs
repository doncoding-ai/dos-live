//! Call guard: is another app using the microphone right now (Teams, Meet in a
//! browser, Zoom, WhatsApp desktop)? If so Dos stops talking out loud and stops
//! listening, so a meeting never hears it and it never hears the meeting.
//!
//! Windows records live microphone use per app under
//! HKCU\…\CapabilityAccessManager\ConsentStore\microphone — an app is using it
//! while its LastUsedTimeStart is set and LastUsedTimeStop is 0.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};

use crate::wide;

const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(parent: HKEY, path: &str) -> Option<Key> {
    let w = wide(path);
    let mut out = HKEY::default();
    let rc = unsafe { RegOpenKeyExW(parent, PCWSTR(w.as_ptr()), None, KEY_READ, &mut out) };
    (rc == ERROR_SUCCESS).then_some(Key(out))
}

fn subkeys(key: &Key) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 0u32;
    loop {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let rc = unsafe { RegEnumKeyExW(key.0, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) };
        if rc != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buf[..len as usize]));
        i += 1;
    }
    names
}

fn qword(key: &Key, value: &str) -> Option<u64> {
    let w = wide(value);
    let mut data = [0u8; 8];
    let mut size = data.len() as u32;
    let rc = unsafe { RegQueryValueExW(key.0, PCWSTR(w.as_ptr()), None, None, Some(data.as_mut_ptr()), Some(&mut size)) };
    (rc == ERROR_SUCCESS && size == 8).then(|| u64::from_le_bytes(data))
}

fn in_use(key: &Key) -> bool {
    matches!((qword(key, "LastUsedTimeStart"), qword(key, "LastUsedTimeStop")), (Some(start), Some(0)) if start > 0)
}

/// Friendly name from a registry entry: "C:#Program Files#…#Teams.exe" → "Teams".
fn app_name(entry: &str) -> String {
    let last = entry.rsplit(['#', '\\']).next().unwrap_or(entry);
    let base = last.strip_suffix(".exe").or_else(|| last.strip_suffix(".EXE")).unwrap_or(last);
    // Packaged apps: "MSTeams_8wekyb3d8bbwe" → "MSTeams".
    base.split('_').next().unwrap_or(base).to_string()
}

/// Apps other than us currently holding the microphone.
pub fn mic_users() -> Vec<String> {
    let Some(root) = open(HKEY_CURRENT_USER, ROOT) else { return vec![] };
    let me = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "#").to_lowercase())
        .unwrap_or_default();
    let mut users = Vec::new();
    for name in subkeys(&root) {
        if name == "NonPackaged" {
            if let Some(np) = open(root.0, "NonPackaged") {
                for exe in subkeys(&np) {
                    if !me.is_empty() && exe.to_lowercase() == me {
                        continue;
                    }
                    if let Some(k) = open(np.0, &exe) {
                        if in_use(&k) {
                            users.push(app_name(&exe));
                        }
                    }
                }
            }
        } else if let Some(k) = open(root.0, &name) {
            if in_use(&k) {
                users.push(app_name(&name));
            }
        }
    }
    users.sort();
    users.dedup();
    users
}
