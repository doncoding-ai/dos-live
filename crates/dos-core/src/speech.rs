//! What Dos says out loud. Short, direct, one breath per thought — and cleaned
//! so a speech engine never reads out a URL, an emoji or "B-D-I minus 4027".

use chrono::{DateTime, Utc};

use crate::snapshot::{Ask, Ping, Snapshot};
use crate::worlds::{day_word, is_decoration, short_title, WorldState, WorldStatus};

fn count_word(n: usize) -> String {
    const WORDS: [&str; 11] = ["No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"];
    WORDS.get(n).map(|s| s.to_string()).unwrap_or_else(|| n.to_string())
}

/// One sentence about one world.
pub fn world_line(w: &WorldStatus, now: DateTime<Utc>) -> String {
    let top = w.items.first();
    let has = |f: &str| top.map(|t| t.flags.iter().any(|x| x == f)).unwrap_or(false);
    match (w.state, top) {
        (WorldState::Empty, _) | (_, None) => format!("{}: nothing open.", w.name),
        (WorldState::Alert, Some(t)) => {
            let more = if w.overdue > 1 { format!(", and {} more overdue", w.overdue - 1) } else { String::new() };
            format!("{}: {} is overdue{}.", w.name, short_title(&t.name), more)
        }
        (_, Some(t)) if has("due-soon") => format!(
            "{}: {} is due {}.",
            w.name,
            short_title(&t.name),
            t.due.map(|d| day_word(d, now)).unwrap_or_else(|| "soon".into())
        ),
        (_, Some(t)) if has("flagged") => format!("{}: {} needs a look.", w.name, short_title(&t.name)),
        (WorldState::Active, _) => format!("{}: {} in progress, all steady.", w.name, count_word(w.doing).to_lowercase()),
        (_, Some(t)) if has("stale") => format!("{}: {} has gone quiet.", w.name, short_title(&t.name)),
        _ => format!("{}: {} parked, nothing urgent.", w.name, count_word(w.open).to_lowercase()),
    }
}

/// The spoken status: what needs you first, then a one-line all-clear.
pub fn brief(snap: &Snapshot, now: DateTime<Utc>) -> String {
    let needs = snap.needs_you();
    let mut parts = Vec::new();
    if !snap.asks.is_empty() {
        parts.push(format!(
            "{} waiting on your call.",
            if snap.asks.len() == 1 { "One question".to_string() } else { format!("{} questions", count_word(snap.asks.len())) }
        ));
    }
    if needs.is_empty() {
        let moving: usize = snap.worlds.iter().map(|w| w.doing).sum();
        parts.push(format!("All quiet. {} things in motion, nothing urgent.", count_word(moving)));
    } else {
        let n = needs.len();
        parts.push(format!("{} {} need{} you.", count_word(n), if n == 1 { "thing" } else { "things" }, if n == 1 { "s" } else { "" }));
        for w in needs.iter().take(3) {
            parts.push(world_line(w, now));
        }
        if n > 3 {
            parts.push(format!("And {} more on the panel.", count_word(n - 3).to_lowercase()));
        }
    }
    if let Some(bad) = snap.boards.iter().find(|b| !b.ok) {
        parts.push(format!("I can't see the {} board right now.", bad.key));
    }
    parts.join(" ")
}

pub fn ping_line(p: &Ping) -> String {
    let first = first_sentence(&p.summary);
    if first.is_empty() {
        format!("{} is in.", p.kind)
    } else {
        format!("{}. {}", p.kind, first)
    }
}

pub fn ask_line(a: &Ask) -> String {
    format!("I need your call. {}. Say do it, hold, or skip.", a.question.trim_end_matches(['?', '.']))
}

pub fn first_sentence(text: &str) -> String {
    let t = text.trim();
    let mut end = t.len();
    for (i, ch) in t.char_indices() {
        if (ch == '.' || ch == '!' || ch == '?' || ch == '—') && i > 12 {
            let next = t[i + ch.len_utf8()..].chars().next();
            if next.map(|c| c.is_whitespace()).unwrap_or(true) {
                end = i + if ch == '—' { 0 } else { ch.len_utf8() };
                break;
            }
        }
    }
    t[..end].trim().to_string()
}

/// Makes text safe and natural for a speech engine.
pub fn tts_clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") || word.starts_with("www.") {
            continue;
        }
        let w: String = word
            .chars()
            .filter(|c| !is_decoration(*c) && !matches!(c, '*' | '#' | '`' | '_' | '[' | ']' | '|'))
            .collect();
        if w.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&spell_ticket(&w));
    }
    let out = out.replace(" — ", ", ").replace('—', ", ").replace(" – ", ", ").replace("→", " to ").replace('&', " and ");
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    clip_sentences(&out, 420)
}

/// "BDI-4027" → "B D I 40 27": letters spelled, number read in pairs.
fn spell_ticket(word: &str) -> String {
    let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
    let Some((letters, digits)) = trimmed.split_once('-') else { return word.to_string() };
    let ok = (2..=6).contains(&letters.len())
        && letters.chars().all(|c| c.is_ascii_uppercase())
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return word.to_string();
    }
    let spelled: Vec<String> = letters.chars().map(|c| c.to_string()).collect();
    let number = if digits.len() == 4 { format!("{} {}", &digits[..2], &digits[2..]) } else { digits.to_string() };
    let tail = &word[word.find(trimmed).map(|i| i + trimmed.len()).unwrap_or(word.len())..];
    format!("{} {}{}", spelled.join(" "), number, tail)
}

/// Playing time of a PCM WAV, read from its header. Used to stop listening
/// while Dos talks, so it never hears itself. Unknown → 3 s.
pub fn wav_duration(wav: &[u8]) -> std::time::Duration {
    let fallback = std::time::Duration::from_secs(3);
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return fallback;
    }
    let u32_at = |i: usize| u32::from_le_bytes([wav[i], wav[i + 1], wav[i + 2], wav[i + 3]]);
    let mut byte_rate = 0u32;
    let mut i = 12;
    while i + 8 <= wav.len() {
        let id = &wav[i..i + 4];
        let size = u32_at(i + 4) as usize;
        if id == b"fmt " && i + 16 <= wav.len() {
            byte_rate = u32_at(i + 16);
        } else if id == b"data" {
            if byte_rate == 0 {
                return fallback;
            }
            let data = size.min(wav.len().saturating_sub(i + 8));
            return std::time::Duration::from_secs_f64(data as f64 / byte_rate as f64);
        }
        i += 8 + size + (size & 1);
    }
    fallback
}

fn clip_sentences(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut = 0;
    for (i, ch) in text.char_indices() {
        if i > max {
            break;
        }
        if ch == '.' || ch == '!' || ch == '?' {
            cut = i + 1;
        }
    }
    if cut == 0 {
        let s: String = text.chars().take(max).collect();
        format!("{s}…")
    } else {
        text[..cut].to_string()
    }
}
