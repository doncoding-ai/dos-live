// Trello over HTTPS. Credentials ride as the key/token query pair (Trello's
// documented way; the OAuth-style header is refused for some tokens). A URL with
// credentials in it must never be logged: every reqwest error is mapped to a fixed
// string below, and request URLs are never formatted into messages.

use std::time::Duration;

use dos_core::trello::{Board, BOARD_QUERY};
use serde::Deserialize;

const API: &str = "https://api.trello.com/1";

#[derive(Clone)]
pub struct Trello {
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub username: String,
    #[serde(default)]
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // kept for callers that need the new id
pub struct Created {
    pub id: String,
}

struct Creds {
    key: String,
    token: String,
}

fn creds() -> Result<Creds, String> {
    let key = crate::secrets::get("trello-key").ok_or("Trello API key not set — open Settings")?;
    let token = crate::secrets::get("trello-token").ok_or("Trello token not set — open Settings")?;
    // A pasted value often carries a stray space or newline.
    let (key, token) = (key.trim().to_string(), token.trim().to_string());
    if key.is_empty() || token.is_empty() {
        return Err("Trello key/token is blank — open Settings".into());
    }
    Ok(Creds { key, token })
}

pub fn configured() -> bool {
    creds().is_ok()
}

/// Card and list ids from Trello are 24 hex characters; anything else is refused
/// before it can become part of a URL path.
fn check_id(id: &str) -> Result<&str, String> {
    if id.len() == 24 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(id)
    } else {
        Err("invalid Trello id".into())
    }
}

fn check_short(short: &str) -> Result<&str, String> {
    if !short.is_empty() && short.len() <= 32 && short.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(short)
    } else {
        Err(format!("\"{short}\" is not a board short link"))
    }
}

impl Trello {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("DosLive/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        Trello { http }
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, String> {
        let c = creds()?;
        Ok(rb.query(&[("key", c.key.as_str()), ("token", c.token.as_str())]))
    }

    async fn send(&self, rb: reqwest::RequestBuilder) -> Result<String, String> {
        let resp = self.auth(rb)?.send().await.map_err(|e| {
            if e.is_timeout() {
                "Trello timed out".to_string()
            } else if e.is_connect() {
                "can't reach Trello (offline?)".to_string()
            } else {
                "Trello request failed".to_string()
            }
        })?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        match status.as_u16() {
            200..=299 => Ok(body),
            401 => Err("Trello rejected the key/token (401) — check Settings".into()),
            403 => Err("Trello says this token can't do that (403) — it needs read,write scope".into()),
            404 => Err("Trello couldn't find that board or card (404)".into()),
            429 => Err("Trello rate limit — backing off".into()),
            code => Err(format!("Trello error {code}")),
        }
    }

    pub async fn me(&self) -> Result<Me, String> {
        let body = self.send(self.http.get(format!("{API}/members/me?fields=username,fullName"))).await?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    pub async fn board(&self, short: &str) -> Result<Board, String> {
        let short = check_short(short)?;
        let body = self.send(self.http.get(format!("{API}/boards/{short}?{BOARD_QUERY}"))).await?;
        Board::parse(&body)
    }

    /// Answers an ask: new title, a comment for the record, then off to `to_list`.
    pub async fn answer_card(&self, card_id: &str, new_title: &str, comment: &str, to_list: Option<&str>) -> Result<(), String> {
        let id = check_id(card_id)?;
        let mut form: Vec<(&str, &str)> = vec![("name", new_title)];
        if let Some(list) = to_list {
            form.push(("idList", check_id(list)?));
            form.push(("pos", "top"));
        }
        self.send(self.http.put(format!("{API}/cards/{id}")).form(&form)).await?;
        // The comment is a nice-to-have; the title already carries the answer.
        let _ = self
            .send(self.http.post(format!("{API}/cards/{id}/actions/comments")).form(&[("text", comment)]))
            .await;
        Ok(())
    }

    pub async fn create_card(&self, list_id: &str, name: &str, desc: &str) -> Result<Created, String> {
        let list = check_id(list_id)?;
        let body = self
            .send(self.http.post(format!("{API}/cards")).form(&[("idList", list), ("name", name), ("desc", desc), ("pos", "top")]))
            .await?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    pub async fn create_list(&self, board_id: &str, name: &str) -> Result<Created, String> {
        let board = check_id(board_id)?;
        let body = self
            .send(self.http.post(format!("{API}/lists")).form(&[("idBoard", board), ("name", name), ("pos", "bottom")]))
            .await?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }
}
