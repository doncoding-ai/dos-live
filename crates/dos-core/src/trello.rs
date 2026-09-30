//! The slice of Trello that Dos Live reads: one board with its lists, cards and
//! labels, fetched in a single call. Only models and parsing live here — the
//! HTTP happens in the app, so everything below is testable without a network.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// Fields asked for on every board fetch. Kept here so the app and the tests
/// agree on the exact shape the parser expects.
pub const BOARD_QUERY: &str = "fields=name,url\
&lists=open&list_fields=name,pos\
&cards=open&card_fields=name,desc,idList,idLabels,dateLastActivity,due,dueComplete,shortUrl\
&labels=all&label_fields=name,color";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Label {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct List {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pos: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub id_list: String,
    #[serde(default)]
    pub id_labels: Vec<String>,
    #[serde(default)]
    pub date_last_activity: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due: Option<DateTime<Utc>>,
    #[serde(default)]
    pub due_complete: bool,
    #[serde(default)]
    pub short_url: String,
}

impl Card {
    /// A Trello object id starts with its creation time: 8 hex digits of Unix seconds.
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        let head = self.id.get(0..8)?;
        let secs = u32::from_str_radix(head, 16).ok()?;
        Utc.timestamp_opt(secs as i64, 0).single()
    }

    /// Most recent sign of life: last activity, falling back to creation.
    pub fn touched_at(&self) -> Option<DateTime<Utc>> {
        self.date_last_activity.or_else(|| self.created_at())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub lists: Vec<List>,
    #[serde(default)]
    pub cards: Vec<Card>,
    #[serde(default)]
    pub labels: Vec<Label>,
}

impl Board {
    pub fn parse(json: &str) -> Result<Board, String> {
        serde_json::from_str(json).map_err(|e| format!("unexpected Trello response: {e}"))
    }

    pub fn list_name(&self, list_id: &str) -> &str {
        self.lists
            .iter()
            .find(|l| l.id == list_id)
            .map(|l| l.name.as_str())
            .unwrap_or("")
    }

    pub fn list_by_name(&self, name: &str) -> Option<&List> {
        let want = normalize(name);
        self.lists.iter().find(|l| normalize(&l.name) == want)
    }

    pub fn label_names(&self, card: &Card) -> Vec<String> {
        card.id_labels
            .iter()
            .filter_map(|id| self.labels.iter().find(|l| &l.id == id))
            .map(|l| l.name.clone())
            .collect()
    }
}

/// Lower-case, letters/digits/spaces only, single spaces. "🟢Finance" → "finance",
/// "🔴 Family Legacy" → "family legacy", "In progress" → "in progress".
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            for low in ch.to_lowercase() {
                out.push(low);
            }
            space = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_' || ch == '&' || ch == '/') && !out.is_empty() && !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

/// Where a card sits in its life, read from the list name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Todo,
    Doing,
    Hold,
    Done,
    Other,
}

impl Stage {
    pub fn from_list_name(name: &str) -> Stage {
        let n = normalize(name);
        match n.as_str() {
            "to do" | "todo" | "backlog" | "next" | "ideas" => Stage::Todo,
            "in progress" | "doing" | "active" | "now" => Stage::Doing,
            "on hold" | "hold" | "blocked" | "waiting" | "paused" => Stage::Hold,
            "done" | "complete" | "completed" | "shipped" | "archive" => Stage::Done,
            _ => Stage::Other,
        }
    }

    pub fn is_open(self) -> bool {
        !matches!(self, Stage::Done)
    }
}
