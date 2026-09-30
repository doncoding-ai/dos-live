//! What you said, turned into what Dos should do. Works on both kinds of input:
//! exact phrases from the offline command grammar, and free dictation after
//! the wake word.

use serde::{Deserialize, Serialize};

use crate::snapshot::Decision;
use crate::trello::normalize;
use crate::worlds::{contains_word, WorldDef};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Intent {
    /// Just the name — start listening for a sentence.
    Wake,
    Status,
    Latest,
    ReadThat,
    Decide(Decision),
    OpenThat,
    Hide,
    Show,
    Mute,
    Unmute,
    Cancel,
    Repeat,
    /// "note" on its own — ask for the note's text next.
    NotePrompt,
    Note(String),
    World(String),
    /// Anything else: a question for Hermes (unless it touches work).
    Ask(String),
}

/// Ways the recognizer tends to hear "Dos" at the start of a sentence.
const WAKE_FORMS: &[&str] = &["hey dos", "ok dos", "okay dos", "yo dos", "dos", "dose", "doss", "das", "dosh"];

/// Strips the wake word. Returns None when the text did not start with it.
pub fn strip_wake(text: &str, wake: &str) -> Option<String> {
    let t = normalize(text);
    let wake = normalize(wake);
    let mut forms: Vec<String> = vec![format!("hey {wake}"), format!("ok {wake}"), format!("okay {wake}"), wake.clone()];
    if wake == "dos" {
        forms.extend(WAKE_FORMS.iter().map(|s| s.to_string()));
    }
    forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
    for f in forms {
        if t == f {
            return Some(String::new());
        }
        if let Some(rest) = t.strip_prefix(&format!("{f} ")) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn one_of(t: &str, options: &[&str]) -> bool {
    options.contains(&t)
}

/// Parses what follows the wake word.
pub fn parse(rest: &str, worlds: &[WorldDef]) -> Intent {
    let t = normalize(rest);
    let t = t
        .trim_start_matches("please ")
        .trim_end_matches(" please")
        .trim()
        .to_string();
    let t = t.as_str();
    if t.is_empty() || one_of(t, &["listen", "listen up", "are you there", "you there", "wake up"]) {
        return Intent::Wake;
    }
    if one_of(t, &["status", "brief me", "briefing", "whats up", "what s up", "how are we", "how are we doing", "report", "sitrep", "what needs me", "what do you need"]) {
        return Intent::Status;
    }
    if one_of(t, &["whats new", "what s new", "latest", "anything new", "news", "last ping", "what came in"]) {
        return Intent::Latest;
    }
    if one_of(t, &["read that", "read it", "read more", "details", "tell me more", "more"]) {
        return Intent::ReadThat;
    }
    if one_of(t, &["do it", "approve", "go ahead", "yes do it", "yes", "approved"]) {
        return Intent::Decide(Decision::DoIt);
    }
    if one_of(t, &["hold", "hold it", "hold on that", "wait", "later", "not now"]) {
        return Intent::Decide(Decision::Hold);
    }
    if one_of(t, &["skip", "skip it", "no", "drop it", "don t", "dont"]) {
        return Intent::Decide(Decision::Skip);
    }
    if one_of(t, &["open that", "open it", "show me that", "open"]) {
        return Intent::OpenThat;
    }
    if one_of(t, &["hide", "go away", "minimize", "dismiss", "close"]) {
        return Intent::Hide;
    }
    if one_of(t, &["show", "come back", "show yourself", "open up"]) {
        return Intent::Show;
    }
    if one_of(t, &["quiet", "mute", "be quiet", "silence", "shut up", "stop talking", "stop"]) {
        return Intent::Mute;
    }
    if one_of(t, &["speak up", "unmute", "talk to me", "you can talk"]) {
        return Intent::Unmute;
    }
    if one_of(t, &["cancel", "never mind", "nevermind", "forget it"]) {
        return Intent::Cancel;
    }
    if one_of(t, &["repeat", "say again", "say that again", "come again", "what"]) {
        return Intent::Repeat;
    }
    if one_of(t, &["note", "take a note", "make a note", "new note", "remember this"]) {
        return Intent::NotePrompt;
    }
    for prefix in ["take a note ", "make a note ", "note that ", "note ", "remind me to ", "remember to ", "add a card "] {
        if let Some(body) = t.strip_prefix(prefix) {
            let body = body.trim();
            if !body.is_empty() {
                // Keep the words as spoken, not the normalized form, when we can.
                return Intent::Note(original_tail(rest, body));
            }
        }
    }
    let world_query = t
        .strip_prefix("how is ")
        .or_else(|| t.strip_prefix("hows "))
        .or_else(|| t.strip_prefix("how s "))
        .or_else(|| t.strip_prefix("whats up with "))
        .or_else(|| t.strip_prefix("what about "))
        .or_else(|| t.strip_suffix(" status"))
        .unwrap_or(t)
        .trim_start_matches("the ")
        .trim();
    for w in worlds {
        if w.spoken_names().iter().any(|n| n == world_query) {
            return Intent::World(w.id.clone());
        }
    }
    Intent::Ask(rest.trim().to_string())
}

/// Recovers the original-cased tail of `raw` matching the normalized `body`.
fn original_tail(raw: &str, body: &str) -> String {
    let raw_words: Vec<&str> = raw.split_whitespace().collect();
    let n = body.split_whitespace().count();
    if n <= raw_words.len() {
        let tail = raw_words[raw_words.len() - n..].join(" ");
        if normalize(&tail) == body {
            return tail;
        }
    }
    body.to_string()
}

/// The offline grammar: every exact phrase the command listener accepts.
/// Dictation handles everything else once the wake word is heard.
pub fn command_phrases(wake: &str, worlds: &[WorldDef]) -> Vec<String> {
    let wake = normalize(wake);
    let mut tails: Vec<String> = [
        "", "status", "brief me", "what's new", "latest", "read that", "read it", "do it", "hold", "skip",
        "open that", "hide", "show", "quiet", "mute", "speak up", "cancel", "repeat", "note", "take a note",
        "listen",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for w in worlds {
        tails.extend(w.spoken_names());
    }
    let mut phrases = Vec::new();
    for tail in tails {
        let tail = tail.trim();
        if tail.is_empty() {
            phrases.push(wake.clone());
            phrases.push(format!("hey {wake}"));
        } else {
            phrases.push(format!("{wake} {tail}"));
        }
    }
    phrases.sort();
    phrases.dedup();
    phrases
}

/// Words that mean the sentence is about the day job. Such sentences are
/// answered from the Work pill only and never forwarded to Hermes.
pub fn is_work_sensitive(text: &str, worlds: &[WorldDef]) -> bool {
    let t = normalize(text);
    let base = ["work", "office", "colleague", "colleagues", "standup", "stand up", "ticket", "tickets", "sprint", "jira", "sportserve", "boss", "manager"];
    if base.iter().any(|b| contains_word(&t, b)) {
        return true;
    }
    worlds
        .iter()
        .filter(|w| w.id == "work")
        .flat_map(|w| w.keywords.iter().chain(w.labels.iter()).chain(w.aliases.iter()))
        .map(|k| normalize(k))
        .any(|k| contains_word(&t, &k))
}
