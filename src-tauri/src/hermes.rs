// Questions that aren't about the boards go to Hermes, running locally with its
// OpenAI-compatible API server (API_SERVER_ENABLED=true, default port 8642).
// Only loopback addresses are accepted: Dos Live never sends speech off-machine
// on its own. Anything touching work is stopped before it gets here.

use std::time::Duration;

use serde_json::{json, Value};

const SYSTEM: &str = "You are Dos, Elijah's personal operating system, answering out loud through Dos Live. \
Reply in one to three short spoken sentences. No markdown, no lists, no links, no emoji. \
Be direct and confident. If you don't know, say so in one sentence. \
Never discuss Sportserve or any work matter — say that stays with the Work pill.";

pub fn is_loopback(url: &str) -> bool {
    let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")).unwrap_or("");
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or("").to_string()
    };
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]")
}

pub async fn ask(url: &str, model: &str, history: &[(String, String)], question: &str) -> Result<String, String> {
    if !is_loopback(url) {
        return Err("Hermes URL must be on this machine (127.0.0.1)".into());
    }
    let key = crate::secrets::get("hermes-key").ok_or("Hermes API key not set — open Settings")?;
    let mut messages = vec![json!({"role": "system", "content": SYSTEM})];
    for (q, a) in history.iter().rev().take(4).rev() {
        messages.push(json!({"role": "user", "content": q}));
        messages.push(json!({"role": "assistant", "content": a}));
    }
    messages.push(json!({"role": "user", "content": question}));

    let client = reqwest::Client::builder().timeout(Duration::from_secs(90)).build().map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/chat/completions", url.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&json!({"model": model, "messages": messages, "stream": false}))
        .send()
        .await
        .map_err(|e| if e.is_connect() { "Hermes isn't running (no API server on that port)".to_string() } else { "Hermes didn't answer".to_string() })?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|_| "Hermes sent something unexpected".to_string())?;
    if !status.is_success() {
        return Err(if status.as_u16() == 401 { "Hermes rejected the API key".into() } else { format!("Hermes error {status}") });
    }
    body["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Hermes gave an empty answer".into())
}

#[cfg(test)]
mod tests {
    use super::is_loopback;

    #[test]
    fn only_loopback() {
        assert!(is_loopback("http://127.0.0.1:8642/v1"));
        assert!(is_loopback("http://localhost:8642/v1"));
        assert!(is_loopback("http://[::1]:8642/v1"));
        assert!(!is_loopback("http://192.168.1.4:8642/v1"));
        assert!(!is_loopback("https://evil.example/v1"));
        assert!(!is_loopback("http://127.0.0.1.evil.example/v1"));
    }
}
