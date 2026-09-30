//! The running feed on the panel: pings, relay messages, what you said and
//! what Dos answered. Newest first, capped.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Ping,
    Say,
    Heard,
    Reply,
    Ask,
    Note,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    pub id: String,
    pub kind: FeedKind,
    pub title: String,
    pub body: String,
    pub world: Option<String>,
    pub url: Option<String>,
    pub at: DateTime<Utc>,
    pub alert: bool,
    /// Who sent it: "trello", "hermes", "voice", "dos" …
    pub source: String,
}

#[derive(Debug, Default)]
pub struct Feed {
    items: VecDeque<FeedItem>,
    counter: u64,
}

pub const FEED_CAP: usize = 60;

impl Feed {
    pub fn push(&mut self, mut item: FeedItem) -> FeedItem {
        self.counter += 1;
        if item.id.is_empty() {
            item.id = format!("f{}-{}", item.at.timestamp_millis(), self.counter);
        }
        // A ping re-announced (e.g. after a restart) replaces its old line.
        self.items.retain(|i| i.id != item.id);
        self.items.push_front(item.clone());
        while self.items.len() > FEED_CAP {
            self.items.pop_back();
        }
        item
    }

    pub fn items(&self) -> Vec<FeedItem> {
        self.items.iter().cloned().collect()
    }

    pub fn latest(&self) -> Option<&FeedItem> {
        self.items.front()
    }

    pub fn latest_of(&self, kinds: &[FeedKind]) -> Option<&FeedItem> {
        self.items.iter().find(|i| kinds.contains(&i.kind))
    }
}
