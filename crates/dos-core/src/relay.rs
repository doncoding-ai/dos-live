//! The wire format between `dosctl` (used by Hermes or any script) and the app:
//! one JSON object per line each way over a local named pipe.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::snapshot::Snapshot;
use crate::worlds::WorldDef;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Info,
    Alert,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Show a line in the feed; optionally say it out loud.
    Say {
        text: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        world: Option<String>,
        #[serde(default)]
        level: Level,
        #[serde(default)]
        speak: bool,
        #[serde(default)]
        source: Option<String>,
    },
    /// Put a Do it / Hold / Skip card on screen and wait for the answer.
    Ask {
        question: String,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        world: Option<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        source: Option<String>,
    },
    /// The pills, redacted: counts and states, never Work card titles.
    Status,
    /// Is the app there?
    Ping,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Value>,
}

impl Response {
    pub fn ok() -> Self {
        Response { ok: true, ..Default::default() }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Response { ok: false, error: Some(msg.into()), ..Default::default() }
    }
}

pub const MAX_TEXT: usize = 2000;
pub const MAX_ASK_SECS: u64 = 600;
pub const DEFAULT_ASK_SECS: u64 = 120;

/// Relay rules. Hermes never carries Sportserve data, so nothing on this pipe
/// may be filed under Work — and nothing absurdly large gets through.
pub fn check(req: &Request) -> Result<(), String> {
    let (text, world) = match req {
        Request::Say { text, world, .. } => (text.as_str(), world.as_deref()),
        Request::Ask { question, world, .. } => (question.as_str(), world.as_deref()),
        Request::Status | Request::Ping => return Ok(()),
    };
    if text.trim().is_empty() {
        return Err("empty text".into());
    }
    if text.chars().count() > MAX_TEXT {
        return Err(format!("text longer than {MAX_TEXT} characters"));
    }
    if world == Some("work") {
        return Err("work stays out of the relay".into());
    }
    Ok(())
}

/// What `status` returns: enough for another agent to reason about priorities,
/// with the Work pill reduced to its counts.
pub fn redacted_status(snap: &Snapshot) -> Value {
    let worlds: Vec<Value> = snap
        .worlds
        .iter()
        .map(|w| {
            let redact = w.id == "work";
            json!({
                "id": w.id,
                "name": w.name,
                "state": w.state,
                "open": w.open,
                "overdue": w.overdue,
                "dueSoon": w.due_soon,
                "headline": if redact { format!("{} open", w.open) } else { w.headline.clone() },
            })
        })
        .collect();
    json!({
        "takenAt": snap.taken_at,
        "worlds": worlds,
        "asksWaiting": snap.asks.len(),
        "latestPing": snap.pings.first().map(|p| &p.kind),
    })
}

/// Validates a world id against the configured worlds (unknown → none).
pub fn known_world(world: Option<&str>, worlds: &[WorldDef]) -> Option<String> {
    let w = world?;
    worlds.iter().find(|d| d.id == w).map(|d| d.id.clone())
}
