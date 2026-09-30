// Keys live in the Windows Credential Manager, never on disk and never in the
// front end — the windows can only ask whether a key is there.

use keyring::Entry;

const SERVICE: &str = "dev.doncoding.doslive";

pub const KNOWN_KEYS: &[&str] = &["trello-key", "trello-token", "hermes-key"];

fn entry(key: &str) -> Option<Entry> {
    if !KNOWN_KEYS.contains(&key) {
        return None;
    }
    Entry::new(SERVICE, key).ok()
}

pub fn get(key: &str) -> Option<String> {
    entry(key)?.get_password().ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    let e = entry(key).ok_or_else(|| format!("unknown key {key}"))?;
    let value = value.trim();
    if value.is_empty() {
        return clear(key);
    }
    e.set_password(value).map_err(|e| e.to_string())
}

pub fn clear(key: &str) -> Result<(), String> {
    let e = entry(key).ok_or_else(|| format!("unknown key {key}"))?;
    match e.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn present(key: &str) -> bool {
    get(key).is_some()
}
