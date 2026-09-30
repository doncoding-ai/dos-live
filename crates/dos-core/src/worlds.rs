//! Don's specialists and Boom, as pills. A world is defined by which boards it
//! reads and which labels/keywords claim a card — all editable in settings.json,
//! so the Trello board can evolve without a rebuild.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::trello::{normalize, Board, Card, Stage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorldDef {
    pub id: String,
    pub name: String,
    /// One or two characters for the bar's dot.
    pub short: String,
    pub color: String,
    /// Board keys this world scans ("dos", "don", "boom").
    #[serde(default = "default_boards")]
    pub boards: Vec<String>,
    /// A card belongs here when one of its labels contains one of these (normalized).
    #[serde(default)]
    pub labels: Vec<String>,
    /// …or when its title contains one of these (normalized).
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Every open card on these boards belongs here (Boom is a whole board).
    #[serde(default)]
    pub whole_boards: Vec<String>,
    /// Extra spoken names for voice ("phoenix", "housing" …).
    #[serde(default)]
    pub aliases: Vec<String>,
}

fn default_boards() -> Vec<String> {
    vec!["don".into()]
}

fn w(id: &str, name: &str, short: &str, color: &str, labels: &[&str], keywords: &[&str], aliases: &[&str]) -> WorldDef {
    WorldDef {
        id: id.into(),
        name: name.into(),
        short: short.into(),
        color: color.into(),
        boards: default_boards(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        whole_boards: vec![],
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
    }
}

/// Matches the charter: Don's six specialists, then Boom.
pub fn default_worlds() -> Vec<WorldDef> {
    let mut boom = w("boom", "Boom", "B", "#4aa8ff", &[], &[], &["build", "persona", "lab"]);
    boom.boards = vec!["boom".into()];
    boom.whole_boards = vec!["boom".into()];
    vec![
        w("work", "Work", "W", "#ffd24a", &["work"], &["sportserve", "bdi", "sgtm", "jira"], &["sportserve", "the job", "tickets"]),
        w("finance", "Finance", "F", "#3ddc84", &["finance"], &["loan", "rent", "fees", "budget", "fos"], &["money", "the money"]),
        w("land", "Land & Housing", "L", "#2dd4bf", &["land", "housing"], &["modular", "housing", "land", "plot", "container"], &["land", "housing", "building point"]),
        w("farm", "Farm & Phoenix", "P", "#ff8a3d", &["agriculture"], &["goat", "phoenix", "farm"], &["farm", "phoenix", "goats"]),
        w("creative", "Creative", "C", "#b388ff", &["creative"], &["hollow men", "poetry", "poem", "writing", "film"], &["writing", "creative"]),
        w("donstech", "Dons Tech", "T", "#ff6ec7", &[], &["dons tech", "don s tech"], &["dons tech", "the shop"]),
        boom,
    ]
}

impl WorldDef {
    pub fn claims(&self, board_key: &str, board: &Board, card: &Card) -> bool {
        if self.whole_boards.iter().any(|b| b == board_key) {
            return true;
        }
        if !self.boards.iter().any(|b| b == board_key) {
            return false;
        }
        let labels: Vec<String> = board.label_names(card).iter().map(|l| normalize(l)).collect();
        if self
            .labels
            .iter()
            .map(|t| normalize(t))
            .filter(|t| !t.is_empty())
            .any(|t| labels.iter().any(|l| contains_word(l, &t)))
        {
            return true;
        }
        let title = normalize(&card.name);
        self.keywords
            .iter()
            .map(|k| normalize(k))
            .filter(|k| !k.is_empty())
            .any(|k| contains_word(&title, &k))
    }

    /// Every way this world may be spoken: its id, name and aliases.
    pub fn spoken_names(&self) -> Vec<String> {
        let mut names = vec![normalize(&self.name), normalize(&self.id)];
        names.extend(self.aliases.iter().map(|a| normalize(a)));
        // "Land & Housing" normalizes to "land housing" — also accept "land and housing".
        if self.name.contains('&') {
            names.push(normalize(&self.name.replace('&', "and")));
        }
        names.retain(|n| !n.is_empty());
        names.sort();
        names.dedup();
        names
    }
}

/// Whole-word containment on normalized text ("rent" must not match "current").
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let hay = format!(" {haystack} ");
    hay.contains(&format!(" {needle} ")) || {
        // Ticket-style keywords ("bdi") should also catch "bdi 4027" glued forms.
        needle.chars().all(|c| c.is_ascii_alphabetic())
            && haystack.split(' ').any(|word| {
                word.starts_with(needle) && word[needle.len()..].chars().all(|c| c.is_ascii_digit()) && word.len() > needle.len()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorldState {
    /// Something is overdue.
    Alert,
    /// Due soon, or flagged ⚠️/blocked.
    Attention,
    /// Work in progress, nothing wrong.
    Active,
    /// Only parked or planned cards.
    Quiet,
    /// Nothing open at all.
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemBrief {
    pub card_id: String,
    pub name: String,
    pub stage: Stage,
    pub flags: Vec<String>,
    pub url: String,
    pub due: Option<DateTime<Utc>>,
    pub board: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorldStatus {
    pub id: String,
    pub name: String,
    pub short: String,
    pub color: String,
    pub state: WorldState,
    pub open: usize,
    pub doing: usize,
    pub hold: usize,
    pub todo: usize,
    pub overdue: usize,
    pub due_soon: usize,
    pub stale: usize,
    pub flagged: usize,
    pub headline: String,
    /// Open items, most urgent first.
    pub items: Vec<ItemBrief>,
}

#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub stale_days: i64,
    pub due_soon_days: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { stale_days: 21, due_soon_days: 3 }
    }
}

/// Flags for one card, in urgency order.
pub fn card_flags(card: &Card, stage: Stage, now: DateTime<Utc>, rules: Rules) -> Vec<String> {
    let mut flags = Vec::new();
    if !stage.is_open() {
        return flags;
    }
    if let Some(due) = card.due {
        if !card.due_complete {
            if due < now {
                flags.push("overdue".into());
            } else if due - now <= Duration::days(rules.due_soon_days) {
                flags.push("due-soon".into());
            }
        }
    }
    let text = format!("{}\n{}", card.name, card.desc);
    if text.contains('⚠') || card.desc.trim_start().starts_with("BLOCKED") {
        flags.push("flagged".into());
    }
    if let Some(t) = card.touched_at() {
        if now - t > Duration::days(rules.stale_days) {
            flags.push("stale".into());
        }
    }
    if stage == Stage::Hold {
        flags.push("on-hold".into());
    }
    flags
}

fn urgency(item: &ItemBrief) -> (u8, i64) {
    let rank = if item.flags.iter().any(|f| f == "overdue") {
        0
    } else if item.flags.iter().any(|f| f == "due-soon") {
        1
    } else if item.flags.iter().any(|f| f == "flagged") {
        2
    } else if item.stage == Stage::Doing {
        3
    } else if item.flags.iter().any(|f| f == "stale") {
        4
    } else if item.stage == Stage::Hold {
        5
    } else {
        6
    };
    let due = item.due.map(|d| d.timestamp()).unwrap_or(i64::MAX);
    (rank, due)
}

/// Builds one pill from every board it reads.
pub fn world_status(def: &WorldDef, boards: &[(String, Board)], now: DateTime<Utc>, rules: Rules) -> WorldStatus {
    let mut items: Vec<ItemBrief> = Vec::new();
    for (key, board) in boards {
        for card in &board.cards {
            if !def.claims(key, board, card) {
                continue;
            }
            let stage = Stage::from_list_name(board.list_name(&card.id_list));
            if !stage.is_open() {
                continue;
            }
            items.push(ItemBrief {
                card_id: card.id.clone(),
                name: card.name.clone(),
                stage,
                flags: card_flags(card, stage, now, rules),
                url: card.short_url.clone(),
                due: card.due,
                board: key.clone(),
            });
        }
    }
    items.sort_by_key(urgency);

    let has = |it: &ItemBrief, f: &str| it.flags.iter().any(|x| x == f);
    let count = |pred: &dyn Fn(&ItemBrief) -> bool| items.iter().filter(|i| pred(i)).count();
    let overdue = count(&|i| has(i, "overdue"));
    let due_soon = count(&|i| has(i, "due-soon"));
    let stale = count(&|i| has(i, "stale"));
    let flagged = count(&|i| has(i, "flagged"));
    let doing = count(&|i| i.stage == Stage::Doing);
    let hold = count(&|i| i.stage == Stage::Hold);
    let todo = count(&|i| i.stage == Stage::Todo || i.stage == Stage::Other);

    let state = if overdue > 0 {
        WorldState::Alert
    } else if due_soon > 0 || flagged > 0 {
        // Stale alone does not raise a pill: some cards are open-ended by design.
        WorldState::Attention
    } else if doing > 0 {
        WorldState::Active
    } else if !items.is_empty() {
        WorldState::Quiet
    } else {
        WorldState::Empty
    };

    let headline = match items.first() {
        None => "nothing open".to_string(),
        Some(top) if has(top, "overdue") => format!("{} — overdue", short_title(&top.name)),
        Some(top) if has(top, "due-soon") => format!("{} — due {}", short_title(&top.name), top.due.map(|d| day_word(d, now)).unwrap_or_default()),
        Some(top) if has(top, "flagged") => format!("{} — needs a look", short_title(&top.name)),
        Some(_) if doing > 0 => format!("{doing} in progress"),
        Some(top) if has(top, "stale") => format!("{} — gone quiet", short_title(&top.name)),
        Some(_) if hold > 0 && todo == 0 => format!("{hold} on hold"),
        Some(_) => format!("{} planned", items.len()),
    };

    WorldStatus {
        id: def.id.clone(),
        name: def.name.clone(),
        short: def.short.clone(),
        color: def.color.clone(),
        state,
        open: items.len(),
        doing,
        hold,
        todo,
        overdue,
        due_soon,
        stale,
        flagged,
        headline,
        items,
    }
}

/// "sGTM Marketing Campaign - Next Sprint (BDI-4025/…)" → "sGTM Marketing Campaign".
/// Trims at the first separator so a headline fits in a pill.
pub fn short_title(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !is_decoration(*c))
        .collect::<String>()
        .trim()
        .to_string();
    let cut = [" — ", " - ", " (", ": ", " – "]
        .iter()
        .filter_map(|sep| cleaned.find(sep))
        .min()
        .unwrap_or(cleaned.len());
    let head = cleaned[..cut].trim();
    if head.chars().count() > 42 {
        let mut s: String = head.chars().take(40).collect();
        s.push('…');
        s
    } else {
        head.to_string()
    }
}

/// Emoji and symbols that decorate card titles but should not be read out.
pub fn is_decoration(c: char) -> bool {
    let u = c as u32;
    (0x1F000..=0x1FAFF).contains(&u)
        || (0x2600..=0x27BF).contains(&u)
        || (0x2B00..=0x2BFF).contains(&u)
        || (0xFE00..=0xFE0F).contains(&u)
        || u == 0x200D
}

/// "today", "tomorrow", a weekday within the week, else "on 30 Sep".
pub fn day_word(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    // East Africa Time: the only zone Dos lives in.
    let eat = chrono::FixedOffset::east_opt(3 * 3600).expect("valid offset");
    let d = when.with_timezone(&eat).date_naive();
    let today = now.with_timezone(&eat).date_naive();
    let days = (d - today).num_days();
    match days {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        2..=6 => d.format("%A").to_string(),
        _ => d.format("%-d %b").to_string(),
    }
}
