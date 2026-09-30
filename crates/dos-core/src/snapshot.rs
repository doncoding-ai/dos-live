//! One picture of the whole empire, rebuilt on every poll: the pills, the ping
//! feed from the Dos board, and the questions Dos is waiting on.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::trello::{normalize, Board, Card, Stage};
use crate::worlds::{is_decoration, world_status, Rules, WorldDef, WorldState, WorldStatus};

/// Where Dos Live expects things to live on the Dos board. Defaults match the
/// board as it is today; settings.json can rename them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardLayout {
    /// Card titles starting with any of these are pings from a scheduled task.
    pub ping_prefixes: Vec<String>,
    /// Pending questions from Dos.
    pub asks_list: String,
    /// Where answered questions go, for the next Dos run to act on.
    pub answered_list: String,
    /// Voice notes land here, at the top.
    pub notes_list: String,
}

impl Default for BoardLayout {
    fn default() -> Self {
        BoardLayout {
            ping_prefixes: vec![
                "🔔".into(),
                "Weekly Priority Report".into(),
                "Trello Check-In".into(),
                "Morning Briefing".into(),
                "Working Dashboard Refresh".into(),
                "Watchdog".into(),
            ],
            asks_list: "Dos asks".into(),
            answered_list: "Dos answered".into(),
            notes_list: "To do".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ping {
    pub card_id: String,
    /// "Morning Briefing", "Trello Check-In" …
    pub kind: String,
    /// Full title without decoration.
    pub title: String,
    /// First paragraph of the description, links removed.
    pub summary: String,
    /// The rendered page linked from the card if there is one, else the card.
    pub link: String,
    pub card_url: String,
    pub at: Option<DateTime<Utc>>,
    /// Still the live one (In progress) rather than filed.
    pub live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    DoIt,
    Hold,
    Skip,
}

impl Decision {
    pub fn parse(s: &str) -> Option<Decision> {
        match normalize(s).as_str() {
            "do it" | "doit" | "do" | "yes" | "approve" | "allow" => Some(Decision::DoIt),
            "hold" | "wait" | "later" => Some(Decision::Hold),
            "skip" | "no" | "deny" | "drop" => Some(Decision::Skip),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Decision::DoIt => "do_it",
            Decision::Hold => "hold",
            Decision::Skip => "skip",
        }
    }

    /// Prefix written onto the Trello card so the next Dos run sees the answer.
    pub fn marker(self) -> &'static str {
        match self {
            Decision::DoIt => "✅ DO IT",
            Decision::Hold => "⏸ HOLD",
            Decision::Skip => "✖ SKIP",
        }
    }

    pub fn spoken(self) -> &'static str {
        match self {
            Decision::DoIt => "Done. I'll tell Dos to go ahead.",
            Decision::Hold => "Holding it.",
            Decision::Skip => "Skipped.",
        }
    }
}

/// "✅ DO IT — Archive old pings?" from "Archive old pings?" — idempotent.
pub fn answered_title(original: &str, decision: Decision) -> String {
    let mut base = original.trim();
    for d in [Decision::DoIt, Decision::Hold, Decision::Skip] {
        if let Some(rest) = base.strip_prefix(d.marker()) {
            base = rest.trim_start_matches([' ', '—', '-', ':']).trim();
        }
    }
    format!("{} — {}", decision.marker(), base)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub id: String,
    pub question: String,
    pub detail: String,
    pub world: Option<String>,
    pub url: String,
    pub at: Option<DateTime<Utc>>,
    /// "trello" for questions on the board, "local" for ones from the relay.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardHealth {
    pub key: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub taken_at: DateTime<Utc>,
    pub worlds: Vec<WorldStatus>,
    pub pings: Vec<Ping>,
    pub asks: Vec<Ask>,
    pub boards: Vec<BoardHealth>,
    /// Ids of the Dos board lists the app writes to, when they exist.
    pub asks_list_id: Option<String>,
    pub answered_list_id: Option<String>,
    pub notes_list_id: Option<String>,
}

impl Snapshot {
    pub fn world(&self, id: &str) -> Option<&WorldStatus> {
        self.worlds.iter().find(|w| w.id == id)
    }

    pub fn needs_you(&self) -> Vec<&WorldStatus> {
        let mut v: Vec<&WorldStatus> = self
            .worlds
            .iter()
            .filter(|w| matches!(w.state, WorldState::Alert | WorldState::Attention))
            .collect();
        v.sort_by_key(|w| w.state);
        v
    }
}

pub fn is_ping(title: &str, layout: &BoardLayout) -> bool {
    let t = title.trim();
    layout.ping_prefixes.iter().any(|p| {
        let p = p.trim();
        !p.is_empty() && (t.starts_with(p) || normalize(t).starts_with(&normalize(p)) && !normalize(p).is_empty())
    })
}

fn strip_decoration(s: &str) -> String {
    s.chars().filter(|c| !is_decoration(*c)).collect::<String>().trim().to_string()
}

fn first_link(text: &str, prefer: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| c == '(' || c == ')' || c == '<' || c == '>' || c == ',' || c == '.'))
        .find(|w| w.starts_with(prefer))
        .map(|w| w.to_string())
}

/// The first paragraph, without URLs or "Full briefing (rendered page):" lines.
pub fn summarize(desc: &str) -> String {
    let para = desc
        .split("\n\n")
        .map(str::trim)
        .find(|p| !p.is_empty())
        .unwrap_or("");
    let lines: Vec<&str> = para
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.to_lowercase().starts_with("full briefing"))
        .collect();
    let joined = lines.join(" ");
    let words: Vec<&str> = joined.split_whitespace().filter(|w| !w.starts_with("http")).collect();
    let text = words.join(" ");
    if text.chars().count() > 320 {
        let cut: String = text.chars().take(317).collect();
        format!("{}…", cut.trim_end())
    } else {
        text
    }
}

fn ping_from(card: &Card, board: &Board) -> Ping {
    let title = strip_decoration(&card.name);
    let kind = [" — ", " – ", " - ", ": "]
        .iter()
        .filter_map(|s| title.find(s))
        .min()
        .map(|i| title[..i].trim().to_string())
        .unwrap_or_else(|| title.clone());
    let link = first_link(&card.desc, "https://claude.ai/").unwrap_or_else(|| card.short_url.clone());
    Ping {
        card_id: card.id.clone(),
        kind,
        title,
        summary: summarize(&card.desc),
        link,
        card_url: card.short_url.clone(),
        at: card.created_at().or(card.date_last_activity),
        live: Stage::from_list_name(board.list_name(&card.id_list)) == Stage::Doing,
    }
}

/// Which world an ask belongs to, if any world claims its card.
fn world_of(card: &Card, board_key: &str, board: &Board, worlds: &[WorldDef]) -> Option<String> {
    // Asks sit on the Dos board, so match on labels/keywords regardless of board.
    worlds
        .iter()
        .find(|w| {
            let mut probe = (*w).clone();
            probe.boards.push(board_key.to_string());
            probe.whole_boards.clear();
            probe.claims(board_key, board, card)
        })
        .map(|w| w.id.clone())
}

/// Builds the snapshot. `boards` holds whatever fetched fine, keyed "dos"/"don"/"boom";
/// `health` records every board, including the ones that failed.
pub fn build_snapshot(
    boards: &[(String, Board)],
    health: Vec<BoardHealth>,
    worlds: &[WorldDef],
    layout: &BoardLayout,
    rules: Rules,
    now: DateTime<Utc>,
) -> Snapshot {
    let statuses = worlds.iter().map(|w| world_status(w, boards, now, rules)).collect();

    let mut pings = Vec::new();
    let mut asks = Vec::new();
    let mut asks_list_id = None;
    let mut answered_list_id = None;
    let mut notes_list_id = None;

    if let Some((key, dos)) = boards.iter().find(|(k, _)| k == "dos") {
        asks_list_id = dos.list_by_name(&layout.asks_list).map(|l| l.id.clone());
        answered_list_id = dos.list_by_name(&layout.answered_list).map(|l| l.id.clone());
        notes_list_id = dos.list_by_name(&layout.notes_list).map(|l| l.id.clone());

        for card in &dos.cards {
            if Some(&card.id_list) == asks_list_id.as_ref() {
                asks.push(Ask {
                    id: card.id.clone(),
                    question: strip_decoration(&card.name),
                    detail: summarize(&card.desc),
                    world: world_of(card, key, dos, worlds),
                    url: card.short_url.clone(),
                    at: card.created_at(),
                    source: "trello".into(),
                });
            } else if is_ping(&card.name, layout) {
                pings.push(ping_from(card, dos));
            }
        }
    }
    pings.sort_by_key(|p| std::cmp::Reverse(p.at));
    pings.truncate(30);
    asks.sort_by_key(|a| a.at);

    Snapshot {
        taken_at: now,
        worlds: statuses,
        pings,
        asks,
        boards: health,
        asks_list_id,
        answered_list_id,
        notes_list_id,
    }
}

/// Remembers which pings and asks have already been announced, so a restart or
/// a card being moved never re-alerts. Persisted by the app between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tracker {
    pub seen: BTreeSet<String>,
    pub initialized: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fresh {
    pub pings: Vec<Ping>,
    pub asks: Vec<Ask>,
}

impl Tracker {
    /// Returns what is new since the last call. The very first call only learns
    /// the board — nobody wants thirty alerts on first launch.
    pub fn observe(&mut self, snap: &Snapshot, now: DateTime<Utc>) -> Fresh {
        let mut fresh = Fresh::default();
        let first = !self.initialized;
        self.initialized = true;
        for p in &snap.pings {
            if self.seen.insert(p.card_id.clone()) && !first {
                // A card created long ago that just got moved is not news.
                let recent = p.at.map(|t| now - t < Duration::hours(24)).unwrap_or(true);
                if recent {
                    fresh.pings.push(p.clone());
                }
            }
        }
        for a in &snap.asks {
            // Asks always surface, even on first launch: they are waiting on you.
            if self.seen.insert(format!("ask:{}", a.id)) {
                fresh.asks.push(a.clone());
            }
        }
        // Forget what is gone so the set does not grow forever.
        let live: BTreeSet<String> = snap
            .pings
            .iter()
            .map(|p| p.card_id.clone())
            .chain(snap.asks.iter().map(|a| format!("ask:{}", a.id)))
            .collect();
        if self.seen.len() > 400 {
            self.seen.retain(|id| live.contains(id));
        }
        fresh
    }
}
