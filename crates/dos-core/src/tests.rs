//! Tests against boards shaped like the real Dos / Don / Boom boards.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;

use crate::intent::{command_phrases, is_work_sensitive, parse, strip_wake, Intent};
use crate::relay::{check, redacted_status, Request};
use crate::snapshot::{answered_title, build_snapshot, BoardHealth, BoardLayout, Decision, Tracker};
use crate::speech::{brief, first_sentence, ping_line, tts_clean};
use crate::trello::{normalize, Board, Stage};
use crate::worlds::{contains_word, default_worlds, short_title, Rules, WorldState};

fn now() -> DateTime<Utc> {
    // 30 Sep 2026, 20:00 UTC (23:00 in Nairobi).
    Utc.with_ymd_and_hms(2026, 9, 30, 20, 0, 0).unwrap()
}

fn dos_board() -> Board {
    let v = json!({
        "id": "65955b31a89ea0a29b1be161", "name": "Dos", "url": "https://trello.com/b/vKKgCDGD/dos",
        "lists": [
            {"id": "L-todo", "name": "To do", "pos": 1},
            {"id": "L-doing", "name": "In progress", "pos": 2},
            {"id": "L-done", "name": "Done", "pos": 3},
            {"id": "L-asks", "name": "Dos asks", "pos": 4},
            {"id": "L-answered", "name": "Dos answered", "pos": 5}
        ],
        "labels": [],
        "cards": [
            {"id": "6abca9f422a53f089f26b9ce", "name": "🔔 Morning Briefing — September 30, 2026",
             "desc": "Open day — just the noon BI standup, nothing needs you this morning. 4 open Jira tickets (1 in progress, 3 not started), BDI-4064 due in two days.\nFull briefing (rendered page): https://claude.ai/artifact/1jbnzyng5rSqemsjfGnAeB",
             "idList": "L-doing", "idLabels": [], "dateLastActivity": "2026-09-30T06:19:33.006Z", "due": null, "dueComplete": false,
             "shortUrl": "https://trello.com/c/sPYEFsKU"},
            {"id": "6abb66b353d0104105d249bf", "name": "🔔 Trello Check-In — September 29, 2026",
             "desc": "Don: To Do untouched.", "idList": "L-doing", "idLabels": [],
             "dateLastActivity": "2026-09-29T07:20:19.116Z", "shortUrl": "https://trello.com/c/k4M7oZdN"},
            {"id": "6ab9f9156f1662d27f4ced7f", "name": "Weekly Priority Report — Monday, September 28, 2026",
             "desc": "WEEKLY CHECK-IN — Sep 28, 2026\n\nJIRA SYNC …", "idList": "L-doing", "idLabels": [],
             "dateLastActivity": "2026-09-28T05:20:21.321Z", "shortUrl": "https://trello.com/c/NnwuLQH6"},
            {"id": "6a917e563504e44c4e15b816", "name": "Office chair — buy", "desc": "Target: October 2026.",
             "idList": "L-todo", "idLabels": [], "dateLastActivity": "2026-08-28T13:06:56.674Z",
             "due": "2026-10-31T21:00:00.000Z", "shortUrl": "https://trello.com/c/cBe7cWdj"},
            {"id": "6abcb00022a53f089f26b9ff", "name": "Archive the 12 Done ping cards?",
             "desc": "Weekly clean-up of the Done list.", "idList": "L-asks", "idLabels": [],
             "dateLastActivity": "2026-09-30T19:00:00.000Z", "shortUrl": "https://trello.com/c/ask1"}
        ]
    });
    serde_json::from_value(v).unwrap()
}

fn don_board() -> Board {
    let v = json!({
        "id": "6a8f5142d1b90c56edcd33e7", "name": "Don", "url": "https://trello.com/b/5jR6zssk/don",
        "lists": [
            {"id": "D-todo", "name": "To Do"}, {"id": "D-doing", "name": "In progress"},
            {"id": "D-hold", "name": "On hold"}, {"id": "D-done", "name": "Done"}
        ],
        "labels": [
            {"id": "lab-fin", "name": "🟢Finance", "color": "green"},
            {"id": "lab-fam", "name": "🔴 Family Legacy", "color": "red"},
            {"id": "lab-ent", "name": "Entrepreneurship", "color": "green_dark"},
            {"id": "lab-work", "name": "🟡Work", "color": "yellow"},
            {"id": "lab-agri", "name": "🟠Agriculture", "color": "orange"},
            {"id": "lab-cre", "name": "🟣Creative", "color": "purple"}
        ],
        "cards": [
            {"id": "6aa3ed09e2b80a379f37510f", "name": "sGTM Marketing Campaign - Next Sprint (BDI-4025/4026/4027/4043)",
             "desc": "Tracking the four sGTM tickets.", "idList": "D-doing", "idLabels": ["lab-work"],
             "dateLastActivity": "2026-09-23T22:03:23.046Z", "due": "2026-10-02T09:00:00.000Z", "shortUrl": "https://trello.com/c/5e4nXOkd"},
            {"id": "6ab55744240678fc95787b9e", "name": "Phoenix Collection — target December 2026",
             "desc": "Underway.", "idList": "D-doing", "idLabels": ["lab-fin", "lab-fam", "lab-ent"],
             "dateLastActivity": "2026-09-26T05:53:38.976Z", "due": "2026-12-31T21:00:00.000Z", "shortUrl": "https://trello.com/c/p3WutaOB"},
            {"id": "6a87079df04269454ff6290f", "name": "Goat farming — target December 2026",
             "desc": "BLOCKED — depends on which loan option gets picked", "idList": "D-hold", "idLabels": ["lab-fam", "lab-ent", "lab-agri"],
             "dateLastActivity": "2026-09-24T17:02:38.762Z", "due": "2026-12-31T21:00:00.000Z", "shortUrl": "https://trello.com/c/G6KeJXwz"},
            {"id": "6a87079a2d0265131232b14e", "name": "Modular housing venture — pick a route to cost out",
             "desc": "⚠️ Checkbox stuck complete via API", "idList": "D-hold", "idLabels": ["lab-ent"],
             "dateLastActivity": "2026-08-28T13:07:25.283Z", "shortUrl": "https://trello.com/c/egOEGRs7"},
            {"id": "6a8f5387725beeceb65de96b", "name": "Keep the next piece in view",
             "desc": "The Hollow Men, the poetry.", "idList": "D-todo", "idLabels": ["lab-cre"],
             "dateLastActivity": "2026-08-26T21:05:56.349Z", "shortUrl": "https://trello.com/c/jhz2WU2H"},
            {"id": "6a8a1fd456ef688a6d90a7dd", "name": "Dons Tech — revival plan + equipment list",
             "desc": "BLOCKED until December 2026.", "idList": "D-todo", "idLabels": ["lab-fin", "lab-ent"],
             "dateLastActivity": "2026-08-28T12:45:20.919Z", "due": "2026-12-31T21:00:00.000Z", "shortUrl": "https://trello.com/c/8JAjq3fM"},
            {"id": "6a917e4eef1c9a000f916c3e", "name": "Rent — monthly payment tracker", "desc": "Paid.",
             "idList": "D-done", "idLabels": ["lab-fin", "lab-fam"], "dateLastActivity": "2026-09-14T06:02:43.098Z",
             "due": "2026-09-10T21:00:00.000Z", "dueComplete": true, "shortUrl": "https://trello.com/c/lZTj99iS"}
        ]
    });
    serde_json::from_value(v).unwrap()
}

fn boom_board() -> Board {
    let v = json!({
        "id": "6a8f50e4bdd2cd2da5871505", "name": "Boom",
        "lists": [{"id": "B-todo", "name": "To Do"}, {"id": "B-doing", "name": "In progress"}, {"id": "B-hold", "name": "On hold"}],
        "labels": [],
        "cards": [
            {"id": "6ac0000022a53f089f26aaaa", "name": "Dos Phase D (Dos app)", "idList": "B-todo", "dateLastActivity": "2026-09-16T08:00:00.000Z"},
            {"id": "6ac0000122a53f089f26aaab", "name": "Set up anonymity protocol", "idList": "B-doing", "dateLastActivity": "2026-09-16T08:00:00.000Z"}
        ]
    });
    serde_json::from_value(v).unwrap()
}

fn boards() -> Vec<(String, Board)> {
    vec![("dos".into(), dos_board()), ("don".into(), don_board()), ("boom".into(), boom_board())]
}

fn health() -> Vec<BoardHealth> {
    ["dos", "don", "boom"].iter().map(|k| BoardHealth { key: k.to_string(), ok: true, error: None }).collect()
}

fn snap() -> crate::Snapshot {
    build_snapshot(&boards(), health(), &default_worlds(), &BoardLayout::default(), Rules::default(), now())
}

#[test]
fn normalizes_labels_and_lists() {
    assert_eq!(normalize("🟢Finance"), "finance");
    assert_eq!(normalize("🔴 Family Legacy"), "family legacy");
    assert_eq!(normalize("In progress"), "in progress");
    assert_eq!(normalize("Don's Tech"), "dons tech");
    assert_eq!(Stage::from_list_name("To Do"), Stage::Todo);
    assert_eq!(Stage::from_list_name("On hold"), Stage::Hold);
}

#[test]
fn whole_word_matching() {
    assert!(contains_word("rent monthly payment tracker", "rent"));
    assert!(!contains_word("current plan", "rent"));
    assert!(contains_word("next sprint bdi 4025 4026", "bdi"));
    assert!(contains_word("fix bdi4027 now", "bdi"));
}

#[test]
fn card_created_time_comes_from_id() {
    let b = dos_board();
    let card = b.cards.iter().find(|c| c.name.contains("Morning Briefing")).unwrap();
    let created = card.created_at().unwrap();
    assert_eq!(created.date_naive().to_string(), "2026-09-30");
}

#[test]
fn worlds_classify_like_the_charter() {
    let s = snap();
    let work = s.world("work").unwrap();
    assert_eq!(work.open, 1);
    // Due 2 Oct, now is 30 Sep: due soon.
    assert_eq!(work.state, WorldState::Attention);
    assert!(work.headline.contains("sGTM Marketing Campaign"), "{}", work.headline);

    let farm = s.world("farm").unwrap();
    // Goat farming (Agriculture label, on hold, BLOCKED) + Phoenix (keyword).
    assert_eq!(farm.open, 2);
    assert_eq!(farm.state, WorldState::Attention);

    let land = s.world("land").unwrap();
    assert_eq!(land.open, 1, "modular housing claimed by keyword");
    assert!(land.items[0].flags.contains(&"flagged".to_string()));

    let finance = s.world("finance").unwrap();
    // Rent is Done → not open. Phoenix + Dons Tech carry the Finance label.
    assert_eq!(finance.open, 2);

    let creative = s.world("creative").unwrap();
    // Open-ended cards that went quiet: dimmed, not flagged.
    assert_eq!(creative.state, WorldState::Quiet);
    assert!(creative.headline.contains("gone quiet"), "{}", creative.headline);

    let tech = s.world("donstech").unwrap();
    assert_eq!(tech.open, 1);

    let boom = s.world("boom").unwrap();
    assert_eq!(boom.open, 2);
    assert_eq!(boom.doing, 1);
}

#[test]
fn pings_and_asks_are_split_out_of_the_dos_board() {
    let s = snap();
    assert_eq!(s.pings.len(), 3, "office chair is not a ping");
    assert_eq!(s.pings[0].kind, "Morning Briefing");
    assert!(s.pings[0].live);
    assert_eq!(s.pings[0].link, "https://claude.ai/artifact/1jbnzyng5rSqemsjfGnAeB");
    assert!(!s.pings[0].summary.contains("http"));
    assert!(!s.pings[0].summary.to_lowercase().contains("full briefing"));
    assert_eq!(s.pings[2].kind, "Weekly Priority Report");
    assert_eq!(s.asks.len(), 1);
    assert_eq!(s.asks[0].question, "Archive the 12 Done ping cards?");
    assert_eq!(s.asks_list_id.as_deref(), Some("L-asks"));
    assert_eq!(s.answered_list_id.as_deref(), Some("L-answered"));
    assert_eq!(s.notes_list_id.as_deref(), Some("L-todo"));
}

#[test]
fn tracker_learns_first_then_reports_only_new() {
    let mut t = Tracker::default();
    let s = snap();
    let first = t.observe(&s, now());
    assert!(first.pings.is_empty(), "first launch must not flood");
    assert_eq!(first.asks.len(), 1, "asks always surface");
    let again = t.observe(&s, now());
    assert!(again.pings.is_empty() && again.asks.is_empty());

    let mut b = boards();
    let dos = &mut b[0].1;
    let mut fresh = dos.cards[0].clone();
    // 6abcf000 ≈ 30 Sep 2026 11:08 UTC — well within 24 h of now().
    fresh.id = "6abcf00022a53f089f26bbbb".into();
    fresh.name = "🔔 Watchdog — all tasks delivered".into();
    dos.cards.push(fresh);
    let s2 = build_snapshot(&b, health(), &default_worlds(), &BoardLayout::default(), Rules::default(), now());
    let news = t.observe(&s2, now());
    assert_eq!(news.pings.len(), 1);
    assert_eq!(news.pings[0].kind, "Watchdog");
}

#[test]
fn answered_titles_are_idempotent() {
    let once = answered_title("Archive the Done pings?", Decision::DoIt);
    assert_eq!(once, "✅ DO IT — Archive the Done pings?");
    let twice = answered_title(&once, Decision::Hold);
    assert_eq!(twice, "⏸ HOLD — Archive the Done pings?");
}

#[test]
fn voice_intents() {
    let worlds = default_worlds();
    assert_eq!(strip_wake("Dos, status.", "dos").as_deref(), Some("status"));
    assert_eq!(strip_wake("hey Dos", "dos").as_deref(), Some(""));
    assert_eq!(strip_wake("Dose what's new", "dos").as_deref(), Some("whats new"));
    assert_eq!(strip_wake("what's new", "dos"), None);

    assert_eq!(parse("", &worlds), Intent::Wake);
    assert_eq!(parse("status", &worlds), Intent::Status);
    assert_eq!(parse("what's new", &worlds), Intent::Latest);
    assert_eq!(parse("do it", &worlds), Intent::Decide(Decision::DoIt));
    assert_eq!(parse("skip", &worlds), Intent::Decide(Decision::Skip));
    assert_eq!(parse("finance", &worlds), Intent::World("finance".into()));
    assert_eq!(parse("how is the farm", &worlds), Intent::World("farm".into()));
    assert_eq!(parse("land and housing", &worlds), Intent::World("land".into()));
    assert_eq!(parse("Dons Tech", &worlds), Intent::World("donstech".into()));
    assert_eq!(parse("note", &worlds), Intent::NotePrompt);
    assert_eq!(parse("note Buy cement for Matuu", &worlds), Intent::Note("Buy cement for Matuu".into()));
    assert_eq!(parse("remind me to call Mutheu", &worlds), Intent::Note("call Mutheu".into()));
    assert_eq!(parse("what is the exchange rate today", &worlds), Intent::Ask("what is the exchange rate today".into()));
}

#[test]
fn grammar_covers_every_world() {
    let phrases = command_phrases("dos", &default_worlds());
    for p in ["dos", "hey dos", "dos status", "dos do it", "dos farm", "dos phoenix", "dos dons tech", "dos boom", "dos land and housing"] {
        assert!(phrases.contains(&p.to_string()), "missing {p}");
    }
    let mut sorted = phrases.clone();
    sorted.dedup();
    assert_eq!(sorted.len(), phrases.len());
}

#[test]
fn work_never_goes_to_hermes() {
    let worlds = default_worlds();
    assert!(is_work_sensitive("what's the status of BDI-4027", &worlds));
    assert!(is_work_sensitive("summarize my sprint", &worlds));
    assert!(is_work_sensitive("anything from Sportserve", &worlds));
    assert!(!is_work_sensitive("how much is cement in Matuu", &worlds));
    assert!(!is_work_sensitive("write me a poem about the Root", &worlds));
}

#[test]
fn relay_rules() {
    let ok = Request::Say { text: "Goat prices up 8% in Machakos".into(), title: None, world: Some("farm".into()), level: Default::default(), speak: true, source: Some("hermes".into()) };
    assert!(check(&ok).is_ok());
    let work = Request::Say { text: "ticket moved".into(), title: None, world: Some("work".into()), level: Default::default(), speak: false, source: None };
    assert!(check(&work).is_err());
    let parsed: Request = serde_json::from_str(r#"{"op":"ask","question":"Send the draft to Mutheu?"}"#).unwrap();
    assert!(matches!(parsed, Request::Ask { .. }));

    let status = redacted_status(&snap());
    let text = status.to_string();
    assert!(!text.contains("sGTM"), "work titles must be redacted: {text}");
    assert!(text.contains("Phoenix") || text.contains("farm"));
}

#[test]
fn wav_duration_reads_the_header() {
    use crate::speech::wav_duration;
    // 16 kHz mono 16-bit: 32000 bytes/s; 48000 data bytes → 1.5 s.
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32 + 48000).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&32000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&48000u32.to_le_bytes());
    wav.resize(wav.len() + 48000, 0);
    assert_eq!(wav_duration(&wav).as_millis(), 1500);
    assert_eq!(wav_duration(b"nope").as_secs(), 3);
}

#[test]
fn spoken_output_is_clean() {
    let s = snap();
    let b = brief(&s, now());
    assert!(b.contains("One question waiting on your call."), "{b}");
    assert!(b.contains("need"), "{b}");
    let line = ping_line(&s.pings[0]);
    assert!(line.starts_with("Morning Briefing."), "{line}");
    let clean = tts_clean("🔔 BDI-4064 due Friday — see https://claude.ai/x **now**");
    assert_eq!(clean, "B D I 40 64 due Friday, see now");
    assert_eq!(first_sentence("Open day — just the noon standup. Then more."), "Open day — just the noon standup.");
    assert_eq!(short_title("sGTM Marketing Campaign - Next Sprint (BDI-4025/4026)"), "sGTM Marketing Campaign");
}
