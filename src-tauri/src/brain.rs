// The runtime: polls the boards, announces what's new, hears you, answers, and
// serves the relay. Everything the island shows comes from `view()`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use chrono::Utc;
use dos_core::feed::{Feed, FeedItem, FeedKind};
use dos_core::intent::{self, Intent};
use dos_core::relay::{self, Request, Response};
use dos_core::speech::{self, tts_clean};
use dos_core::trello::Board;
use dos_core::{build_snapshot, Ask, BoardHealth, Decision, Snapshot, Tracker, WorldState};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::platform::{self, Heard, Voice, VoiceEvent};
use crate::settings::{self, Settings};
use crate::trello::Trello;
use crate::{hermes, log};

pub const ISLAND: &str = "island";

struct LocalAsk {
    ask: Ask,
    reply: mpsc::Sender<Decision>,
}

/// What "that" means in "read that", "open that", "do it".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Focus {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub text: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceState {
    pub mode: String,
    pub listening: bool,
    pub dictating: bool,
    pub speaking: bool,
    pub muted: bool,
    pub on_call: bool,
    pub mic_users: Vec<String>,
    pub problem: Option<String>,
    pub last_heard: Option<String>,
    pub note_mode: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub snapshot: Option<Snapshot>,
    pub feed: Vec<FeedItem>,
    pub asks: Vec<Ask>,
    pub voice: VoiceState,
    pub problems: Vec<String>,
    pub trello_ready: bool,
    pub hermes_enabled: bool,
    pub focus: Option<Focus>,
    pub paused: bool,
    pub polling: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub body: String,
    pub world: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speech {
    pub id: u64,
    pub text: String,
    pub wav: String,
    pub millis: u64,
}

pub struct Brain {
    app: AppHandle,
    pub settings: Mutex<Settings>,
    snapshot: Mutex<Option<Snapshot>>,
    boards_cache: Mutex<HashMap<String, Board>>,
    feed: Mutex<Feed>,
    tracker: Mutex<Tracker>,
    local_asks: Mutex<Vec<LocalAsk>>,
    focus: Mutex<Option<Focus>>,
    voice: Mutex<Option<Voice>>,
    voice_state: Mutex<VoiceState>,
    speaking_until: Mutex<Instant>,
    speak_seq: AtomicU64,
    speech_texts: Mutex<HashMap<u64, String>>,
    last_spoken: Mutex<String>,
    note_mode: AtomicBool,
    history: Mutex<Vec<(String, String)>>,
    problems: Mutex<Vec<String>>,
    trello: Trello,
    poke: tokio::sync::Notify,
    paused: AtomicBool,
    polling: AtomicBool,
    ask_seq: AtomicU64,
    /// Cached "are Trello keys saved": reading the Credential Manager on every
    /// state update was needless work on every emit.
    trello_ready: AtomicBool,
}

fn now_eat() -> String {
    let eat = chrono::FixedOffset::east_opt(3 * 3600).expect("offset");
    Utc::now().with_timezone(&eat).format("%Y-%m-%d %H:%M EAT").to_string()
}

fn item(kind: FeedKind, title: impl Into<String>, body: impl Into<String>, source: &str) -> FeedItem {
    FeedItem {
        id: String::new(),
        kind,
        title: title.into(),
        body: body.into(),
        world: None,
        url: None,
        at: Utc::now(),
        alert: false,
        source: source.into(),
    }
}

impl Brain {
    pub fn new(app: AppHandle, settings: Settings) -> Arc<Brain> {
        let tracker = std::fs::read(settings::state_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Tracker>(&b).ok())
            .unwrap_or_default();
        let mode = settings.voice.mode.clone();
        Arc::new(Brain {
            app,
            settings: Mutex::new(settings),
            snapshot: Mutex::new(None),
            boards_cache: Mutex::new(HashMap::new()),
            feed: Mutex::new(Feed::default()),
            tracker: Mutex::new(tracker),
            local_asks: Mutex::new(Vec::new()),
            focus: Mutex::new(None),
            voice: Mutex::new(None),
            voice_state: Mutex::new(VoiceState { mode, ..Default::default() }),
            speaking_until: Mutex::new(Instant::now()),
            speak_seq: AtomicU64::new(1),
            speech_texts: Mutex::new(HashMap::new()),
            last_spoken: Mutex::new(String::new()),
            note_mode: AtomicBool::new(false),
            history: Mutex::new(Vec::new()),
            problems: Mutex::new(Vec::new()),
            trello: Trello::new(),
            poke: tokio::sync::Notify::new(),
            paused: AtomicBool::new(false),
            polling: AtomicBool::new(false),
            ask_seq: AtomicU64::new(1),
            trello_ready: AtomicBool::new(crate::trello::configured()),
        })
    }

    // ── view ────────────────────────────────────────────────────────────────

    pub fn view(&self) -> ViewState {
        let settings = self.settings.lock().unwrap().clone();
        let mut snapshot = self.snapshot.lock().unwrap().clone();
        if settings.ui.discreet_work {
            if let Some(s) = snapshot.as_mut() {
                for w in s.worlds.iter_mut().filter(|w| w.id == "work") {
                    let mut bits = vec![format!("{} open", w.open)];
                    if w.overdue > 0 {
                        bits.push(format!("{} overdue", w.overdue));
                    } else if w.due_soon > 0 {
                        bits.push(format!("{} due soon", w.due_soon));
                    }
                    w.headline = bits.join(" · ");
                    for (i, it) in w.items.iter_mut().enumerate() {
                        it.name = format!("Work item {}", i + 1);
                    }
                }
            }
        }
        let mut asks: Vec<Ask> = snapshot.as_ref().map(|s| s.asks.clone()).unwrap_or_default();
        asks.extend(self.local_asks.lock().unwrap().iter().map(|l| l.ask.clone()));
        let mut voice = self.voice_state.lock().unwrap().clone();
        voice.speaking = Instant::now() < *self.speaking_until.lock().unwrap();
        voice.note_mode = self.note_mode.load(Ordering::Relaxed);
        ViewState {
            snapshot,
            feed: self.feed.lock().unwrap().items(),
            asks,
            voice,
            problems: self.problems.lock().unwrap().clone(),
            trello_ready: self.trello_ready.load(Ordering::Relaxed),
            hermes_enabled: settings.hermes.enabled,
            focus: self.focus.lock().unwrap().clone(),
            paused: self.paused.load(Ordering::Relaxed),
            polling: self.polling.load(Ordering::Relaxed),
        }
    }

    pub fn emit_state(&self) {
        let _ = self.app.emit("state", self.view());
    }

    fn cue(&self, name: &str) {
        let _ = self.app.emit_to(ISLAND, "cue", name.to_string());
    }

    fn alert(&self, a: Alert) {
        let _ = self.app.emit_to(ISLAND, "alert", a);
    }

    fn push_feed(&self, it: FeedItem) {
        self.feed.lock().unwrap().push(it);
        self.emit_state();
    }

    /// Clone out of the lock: a guard held across a `match` would deadlock on
    /// the next emit_state().
    fn current_focus(&self) -> Option<Focus> {
        let f = self.focus.lock().unwrap().clone();
        f
    }

    fn set_focus(&self, f: Focus) {
        *self.focus.lock().unwrap() = Some(f);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if !paused {
            self.poke.notify_one();
        }
        self.emit_state();
    }

    pub fn refresh(&self) {
        self.poke.notify_one();
    }

    /// A key was saved or removed in Settings.
    pub fn keys_changed(&self) {
        self.trello_ready.store(crate::trello::configured(), Ordering::Relaxed);
        self.refresh();
        self.emit_state();
    }

    // ── polling ─────────────────────────────────────────────────────────────

    pub async fn run_poller(self: Arc<Self>) {
        loop {
            if !self.paused.load(Ordering::Relaxed) {
                self.poll_once().await;
            }
            let secs = self.settings.lock().unwrap().poll_secs.clamp(20, 900);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
                _ = self.poke.notified() => {}
            }
        }
    }

    async fn poll_once(&self) {
        let ready = crate::trello::configured();
        self.trello_ready.store(ready, Ordering::Relaxed);
        if !ready {
            *self.problems.lock().unwrap() = vec!["Add your Trello key and token in Settings to wake Dos up.".into()];
            self.emit_state();
            return;
        }
        self.polling.store(true, Ordering::Relaxed);
        self.emit_state();
        let settings = self.settings.lock().unwrap().clone();
        let mut boards = Vec::new();
        let mut health = Vec::new();
        let mut problems = Vec::new();
        for (key, short) in settings.boards.keyed() {
            match self.trello.board(&short).await {
                Ok(b) => {
                    self.boards_cache.lock().unwrap().insert(key.clone(), b.clone());
                    boards.push((key.clone(), b));
                    health.push(BoardHealth { key, ok: true, error: None });
                }
                Err(e) => {
                    log::line(format!("board {key}: {e}"));
                    problems.push(format!("{key} board: {e}"));
                    // Keep showing the last good copy rather than an empty pill.
                    if let Some(b) = self.boards_cache.lock().unwrap().get(&key).cloned() {
                        boards.push((key.clone(), b));
                    }
                    health.push(BoardHealth { key, ok: false, error: Some(e) });
                }
            }
        }
        self.polling.store(false, Ordering::Relaxed);
        *self.problems.lock().unwrap() = problems;
        if boards.is_empty() {
            self.emit_state();
            return;
        }

        let snap = build_snapshot(&boards, health, &settings.worlds, &settings.layout, settings.rules(), Utc::now());
        let first_run = self.snapshot.lock().unwrap().is_none();
        let fresh = {
            let mut t = self.tracker.lock().unwrap();
            let fresh = t.observe(&snap, Utc::now());
            if let Ok(json) = serde_json::to_vec(&*t) {
                let _ = std::fs::create_dir_all(settings::config_dir());
                let _ = std::fs::write(settings::state_path(), json);
            }
            fresh
        };
        *self.snapshot.lock().unwrap() = Some(snap.clone());

        if first_run {
            // Seed the feed with the latest pings, oldest first so the newest ends on top.
            let mut f = self.feed.lock().unwrap();
            for p in snap.pings.iter().take(6).rev() {
                f.push(self.ping_item(p, false));
            }
        }
        self.emit_state();

        for p in &fresh.pings {
            log::line(format!("new ping: {}", p.kind));
            self.push_feed(self.ping_item(p, true));
            self.set_focus(Focus {
                kind: "ping".into(),
                id: p.card_id.clone(),
                title: p.title.clone(),
                text: p.summary.clone(),
                url: Some(p.link.clone()),
            });
            self.alert(Alert {
                kind: "ping".into(),
                id: format!("ping:{}", p.card_id),
                title: p.kind.clone(),
                body: speech::first_sentence(&p.summary),
                world: None,
                url: Some(p.link.clone()),
            });
            self.cue("ping");
            if settings.voice.speak_pings {
                self.speak(&speech::ping_line(p));
            }
        }
        for a in &fresh.asks {
            self.announce_ask(a, &settings);
        }
    }

    fn ping_item(&self, p: &dos_core::Ping, alert: bool) -> FeedItem {
        FeedItem {
            id: format!("ping:{}", p.card_id),
            kind: FeedKind::Ping,
            title: p.kind.clone(),
            body: p.summary.clone(),
            world: None,
            url: Some(p.link.clone()),
            at: p.at.unwrap_or_else(Utc::now),
            alert,
            source: "trello".into(),
        }
    }

    fn announce_ask(&self, a: &Ask, settings: &Settings) {
        log::line(format!("ask waiting ({})", a.source));
        self.set_focus(Focus {
            kind: "ask".into(),
            id: a.id.clone(),
            title: a.question.clone(),
            text: format!("{} {}", a.question, a.detail).trim().to_string(),
            url: (!a.url.is_empty()).then(|| a.url.clone()),
        });
        let mut it = item(FeedKind::Ask, a.question.clone(), a.detail.clone(), &a.source);
        it.id = format!("ask:{}", a.id);
        it.world = a.world.clone();
        it.alert = true;
        self.push_feed(it);
        self.alert(Alert {
            kind: "ask".into(),
            id: a.id.clone(),
            title: a.question.clone(),
            body: a.detail.clone(),
            world: a.world.clone(),
            url: (!a.url.is_empty()).then(|| a.url.clone()),
        });
        self.cue("ask");
        if settings.voice.speak_asks {
            self.speak(&speech::ask_line(a));
        }
    }

    // ── answering asks ──────────────────────────────────────────────────────

    pub async fn decide(&self, ask_id: &str, decision: Decision) -> Result<(), String> {
        // A question from the relay: hand the answer straight back.
        let local = {
            let mut asks = self.local_asks.lock().unwrap();
            asks.iter().position(|l| l.ask.id == ask_id).map(|i| asks.remove(i))
        };
        if let Some(l) = local {
            let _ = l.reply.send(decision);
            self.after_decision(&l.ask, decision);
            return Ok(());
        }
        // A question on the Dos board: mark it, comment, move it to "Dos answered".
        let (ask, answered) = {
            let snap = self.snapshot.lock().unwrap();
            let s = snap.as_ref().ok_or("no board data yet")?;
            let a = s.asks.iter().find(|a| a.id == ask_id).cloned().ok_or("that question is no longer waiting")?;
            (a, s.answered_list_id.clone())
        };
        let title = dos_core::snapshot::answered_title(&ask.question, decision);
        let comment = format!("Answered from Dos Live: {} · {}", decision.marker(), now_eat());
        self.trello.answer_card(&ask.id, &title, &comment, answered.as_deref()).await?;
        if let Some(s) = self.snapshot.lock().unwrap().as_mut() {
            s.asks.retain(|a| a.id != ask.id);
        }
        self.after_decision(&ask, decision);
        self.refresh();
        Ok(())
    }

    fn after_decision(&self, ask: &Ask, decision: Decision) {
        log::line(format!("answered {} ({})", decision.word(), ask.source));
        let mut it = item(FeedKind::Reply, format!("{} — {}", decision.marker(), ask.question), "", "you");
        it.world = ask.world.clone();
        self.push_feed(it);
        let mut focus = self.focus.lock().unwrap();
        if focus.as_ref().map(|f| f.id == ask.id).unwrap_or(false) {
            *focus = None;
        }
        drop(focus);
        self.cue("done");
        self.emit_state();
    }

    fn pending_ask_id(&self) -> Option<String> {
        if let Some(f) = self.focus.lock().unwrap().as_ref() {
            if f.kind == "ask" {
                return Some(f.id.clone());
            }
        }
        if let Some(l) = self.local_asks.lock().unwrap().first() {
            return Some(l.ask.id.clone());
        }
        self.snapshot.lock().unwrap().as_ref().and_then(|s| s.asks.first().map(|a| a.id.clone()))
    }

    // ── notes and setup ─────────────────────────────────────────────────────

    pub async fn create_note(&self, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("empty note".into());
        }
        let list = self
            .snapshot
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.notes_list_id.clone())
            .ok_or("couldn't find the notes list on the Dos board")?;
        let name: String = format!("📝 {text}").chars().take(200).collect();
        self.trello.create_card(&list, &name, &format!("Voice note from Dos Live · {}", now_eat())).await?;
        log::line("note created");
        let mut it = item(FeedKind::Note, text.to_string(), "saved to the Dos board", "voice");
        it.alert = false;
        self.push_feed(it);
        self.cue("done");
        self.refresh();
        Ok(())
    }

    /// Creates the "Dos asks" and "Dos answered" lists on the Dos board if missing.
    pub async fn setup_lists(&self) -> Result<String, String> {
        let (short, layout) = {
            let s = self.settings.lock().unwrap();
            (s.boards.dos.clone(), s.layout.clone())
        };
        let board = self.trello.board(&short).await?;
        let mut made = Vec::new();
        for name in [&layout.asks_list, &layout.answered_list] {
            if board.list_by_name(name).is_none() {
                self.trello.create_list(&board.id, name).await?;
                made.push(name.clone());
            }
        }
        self.refresh();
        Ok(if made.is_empty() { "Both lists were already there.".into() } else { format!("Created: {}", made.join(", ")) })
    }

    pub async fn test_trello(&self) -> Result<String, String> {
        let me = self.trello.me().await?;
        let boards = self.settings.lock().unwrap().boards.keyed();
        let mut seen = Vec::new();
        for (key, short) in boards {
            let b = self.trello.board(&short).await.map_err(|e| format!("{key}: {e}"))?;
            seen.push(format!("{} ({} cards)", b.name, b.cards.len()));
        }
        self.refresh();
        Ok(format!("Signed in as {} — {}", if me.full_name.is_empty() { &me.username } else { &me.full_name }, seen.join(", ")))
    }

    // ── speaking ────────────────────────────────────────────────────────────

    pub fn speak(&self, text: &str) {
        let clean = tts_clean(text);
        if clean.is_empty() {
            return;
        }
        *self.last_spoken.lock().unwrap() = clean.clone();
        let (voice_name, rate) = {
            let s = self.settings.lock().unwrap();
            (s.voice.voice_name.clone(), s.voice.rate)
        };
        let vs = self.voice_state.lock().unwrap().clone();
        if vs.muted || vs.on_call {
            return;
        }
        let id = self.speak_seq.fetch_add(1, Ordering::Relaxed);
        self.speech_texts.lock().unwrap().insert(id, clean.clone());
        if let Some(v) = self.voice.lock().unwrap().as_ref() {
            v.speak(id, clean, voice_name, rate);
        }
    }

    /// A reply: always written in the feed, spoken when allowed.
    fn reply(&self, text: &str) {
        self.push_feed(item(FeedKind::Reply, text.to_string(), "", "dos"));
        self.speak(text);
    }

    pub fn speech_finished(&self, _id: u64) {
        *self.speaking_until.lock().unwrap() = Instant::now() + Duration::from_millis(250);
        self.emit_state();
    }

    pub fn stop_speaking(&self) {
        *self.speaking_until.lock().unwrap() = Instant::now();
        let _ = self.app.emit_to(ISLAND, "speech-stop", ());
        self.emit_state();
    }

    // ── voice ───────────────────────────────────────────────────────────────

    pub fn start_voice(self: &Arc<Self>) {
        let me = Arc::clone(self);
        let voice = platform::start_voice(move |ev| me.on_voice_event(ev));
        *self.voice.lock().unwrap() = Some(voice);
        self.apply_voice_mode();
    }

    pub fn apply_voice_mode(&self) {
        let s = self.settings.lock().unwrap().clone();
        {
            let mut vs = self.voice_state.lock().unwrap();
            vs.mode = s.voice.mode.clone();
        }
        let on_call = self.voice_state.lock().unwrap().on_call;
        if let Some(v) = self.voice.lock().unwrap().as_ref() {
            if s.voice.mode == "wake" {
                v.listen(intent::command_phrases(&s.voice.wake_word, &s.worlds));
                if on_call {
                    v.pause();
                }
            } else {
                v.stop_listening();
            }
        }
        self.emit_state();
    }

    /// Push-to-talk (hotkey or the mic button): listen for one sentence now.
    pub fn push_to_talk(&self) {
        self.stop_speaking();
        self.cue("wake");
        if let Some(v) = self.voice.lock().unwrap().as_ref() {
            v.dictate();
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.voice_state.lock().unwrap().muted = muted;
        if muted {
            self.stop_speaking();
        }
        self.emit_state();
    }

    pub fn list_voices(&self) {
        if let Some(v) = self.voice.lock().unwrap().as_ref() {
            v.list_voices();
        }
    }

    fn on_voice_event(self: &Arc<Self>, ev: VoiceEvent) {
        match ev {
            VoiceEvent::Listening(on) => {
                let mut vs = self.voice_state.lock().unwrap();
                vs.listening = on;
                if on {
                    vs.problem = None;
                }
                drop(vs);
                self.emit_state();
            }
            VoiceEvent::Dictating(on) => {
                self.voice_state.lock().unwrap().dictating = on;
                self.emit_state();
            }
            VoiceEvent::NothingHeard => {
                self.note_mode.store(false, Ordering::Relaxed);
                self.cue("miss");
                self.emit_state();
            }
            VoiceEvent::Problem(p) => {
                log::line(format!("voice: {p}"));
                self.voice_state.lock().unwrap().problem = Some(p);
                self.emit_state();
            }
            VoiceEvent::Voices(list) => {
                let names: Vec<serde_json::Value> = list
                    .iter()
                    .map(|v| serde_json::json!({"name": v.name, "language": v.language, "male": v.male}))
                    .collect();
                let _ = self.app.emit("voices", names);
            }
            VoiceEvent::Spoken { id, wav } => {
                let dur = speech::wav_duration(&wav);
                *self.speaking_until.lock().unwrap() = Instant::now() + dur + Duration::from_millis(400);
                let text = self.speech_texts.lock().unwrap().remove(&id).unwrap_or_default();
                let _ = self.app.emit_to(
                    ISLAND,
                    "speech",
                    Speech {
                        id,
                        text,
                        wav: base64::engine::general_purpose::STANDARD.encode(&wav),
                        millis: dur.as_millis() as u64,
                    },
                );
                self.emit_state();
            }
            VoiceEvent::Heard { text, kind, confident } => self.on_heard(text, kind, confident),
        }
    }

    fn on_heard(self: &Arc<Self>, text: String, kind: Heard, confident: bool) {
        let settings = self.settings.lock().unwrap().clone();
        let vs = self.voice_state.lock().unwrap().clone();
        // Never act on our own voice, or on a meeting.
        if kind == Heard::Command && (Instant::now() < *self.speaking_until.lock().unwrap() || vs.on_call) {
            return;
        }
        if kind == Heard::Command && !confident {
            return;
        }
        self.voice_state.lock().unwrap().last_heard = Some(text.clone());
        log::line(format!("heard a {} phrase", if kind == Heard::Command { "command" } else { "dictated" }));

        let parsed = if kind == Heard::Dictation && self.note_mode.swap(false, Ordering::Relaxed) {
            Intent::Note(intent::strip_wake(&text, &settings.voice.wake_word).unwrap_or(text.clone()))
        } else {
            let rest = intent::strip_wake(&text, &settings.voice.wake_word).unwrap_or(text.clone());
            intent::parse(&rest, &settings.worlds)
        };
        let _ = self.app.emit_to(ISLAND, "heard", serde_json::json!({"text": text, "intent": parsed}));
        self.push_feed(item(FeedKind::Heard, text.clone(), "", "voice"));
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move { me.handle(parsed).await });
    }

    /// A line typed at the island's prompt: parsed exactly like speech.
    pub fn typed(self: &Arc<Self>, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let settings = self.settings.lock().unwrap().clone();
        let rest = intent::strip_wake(&text, &settings.voice.wake_word).unwrap_or(text.clone());
        let parsed = match intent::parse(&rest, &settings.worlds) {
            // Typing just "dos" should not open the microphone.
            Intent::Wake => Intent::Status,
            other => other,
        };
        self.push_feed(item(FeedKind::Heard, text, "", "typed"));
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move { me.handle(parsed).await });
    }

    pub async fn handle(self: Arc<Self>, intent: Intent) {
        let settings = self.settings.lock().unwrap().clone();
        let now = Utc::now();
        let snap = self.snapshot.lock().unwrap().clone();
        match intent {
            Intent::Wake => {
                self.cue("wake");
                if let Some(v) = self.voice.lock().unwrap().as_ref() {
                    v.dictate();
                }
            }
            Intent::NotePrompt => {
                self.note_mode.store(true, Ordering::Relaxed);
                self.cue("wake");
                self.emit_state();
                if let Some(v) = self.voice.lock().unwrap().as_ref() {
                    v.dictate();
                }
            }
            Intent::Status => match snap {
                Some(s) => {
                    let _ = self.app.emit_to(ISLAND, "ui", "expand");
                    self.reply(&speech::brief(&s, now));
                }
                None => self.reply("I haven't seen the boards yet. Give me a minute."),
            },
            Intent::Latest => match snap.as_ref().and_then(|s| s.pings.first().cloned()) {
                Some(p) => {
                    self.set_focus(Focus {
                        kind: "ping".into(),
                        id: p.card_id.clone(),
                        title: p.title.clone(),
                        text: p.summary.clone(),
                        url: Some(p.link.clone()),
                    });
                    self.reply(&speech::ping_line(&p));
                }
                None => self.reply("Nothing new on the Dos board."),
            },
            Intent::ReadThat => match self.current_focus() {
                Some(f) => self.reply(&format!("{}. {}", f.title, f.text)),
                None => self.reply("There's nothing in focus. Ask me what's new."),
            },
            Intent::OpenThat => match self.current_focus().and_then(|f| f.url) {
                Some(url) => {
                    crate::open_url(url);
                    self.cue("done");
                }
                None => self.reply("Nothing to open."),
            },
            Intent::Decide(d) => match self.pending_ask_id() {
                Some(id) => match self.decide(&id, d).await {
                    Ok(()) => self.reply(d.spoken()),
                    Err(e) => {
                        log::line(format!("decide failed: {e}"));
                        self.reply("I couldn't record that on Trello. Try the button.");
                    }
                },
                None => self.reply("Nothing is waiting on you."),
            },
            Intent::Hide => {
                let _ = self.app.emit_to(ISLAND, "ui", "collapse");
            }
            Intent::Show => {
                let _ = self.app.emit_to(ISLAND, "ui", "expand");
            }
            Intent::Mute => self.set_muted(true),
            Intent::Unmute => {
                self.set_muted(false);
                self.reply("I'm back.");
            }
            Intent::Cancel => {
                self.stop_speaking();
                self.note_mode.store(false, Ordering::Relaxed);
                self.cue("miss");
            }
            Intent::Repeat => {
                let last = self.last_spoken.lock().unwrap().clone();
                if last.is_empty() {
                    self.reply("I haven't said anything yet.");
                } else {
                    self.speak(&last);
                }
            }
            Intent::World(id) => match snap.as_ref().and_then(|s| s.world(&id).cloned()) {
                Some(w) => {
                    let _ = self.app.emit_to(ISLAND, "ui", format!("world:{id}"));
                    self.set_focus(Focus {
                        kind: "world".into(),
                        id: w.id.clone(),
                        title: w.name.clone(),
                        text: w.items.iter().take(4).map(|i| dos_core::worlds::short_title(&i.name)).collect::<Vec<_>>().join(", "),
                        url: w.items.first().map(|i| i.url.clone()),
                    });
                    self.reply(&speech::world_line(&w, now));
                }
                None => self.reply("I don't have that world on the boards yet."),
            },
            Intent::Note(text) => match self.create_note(&text).await {
                Ok(()) => self.reply("Noted."),
                Err(e) => {
                    log::line(format!("note failed: {e}"));
                    self.reply("I couldn't save that note. Is Trello connected?");
                }
            },
            Intent::Ask(q) => self.answer_question(&q, &settings, snap.as_ref()).await,
        }
        self.emit_state();
    }

    async fn answer_question(&self, q: &str, settings: &Settings, snap: Option<&Snapshot>) {
        if intent::is_work_sensitive(q, &settings.worlds) {
            let line = snap
                .and_then(|s| s.world("work"))
                .map(|w| speech::world_line(w, Utc::now()))
                .unwrap_or_else(|| "Work: no data yet.".into());
            self.reply(&format!("That's work, so it stays here. {line}"));
            return;
        }
        if !settings.hermes.enabled {
            self.reply("I can only answer about the boards until Hermes is connected. Try: Dos, status.");
            return;
        }
        self.cue("think");
        let history = self.history.lock().unwrap().clone();
        match hermes::ask(&settings.hermes.url, &settings.hermes.model, &history, q).await {
            Ok(answer) => {
                {
                    let mut h = self.history.lock().unwrap();
                    h.push((q.to_string(), answer.clone()));
                    let excess = h.len().saturating_sub(8);
                    h.drain(..excess);
                }
                self.set_focus(Focus { kind: "reply".into(), id: String::new(), title: "Hermes".into(), text: answer.clone(), url: None });
                let mut it = item(FeedKind::Reply, answer.clone(), "", "hermes");
                it.body = format!("You asked: {q}");
                self.push_feed(it);
                self.speak(&answer);
            }
            Err(e) => {
                log::line(format!("hermes: {e}"));
                self.reply(&format!("Hermes didn't come through. {e}."));
            }
        }
    }

    // ── call guard ──────────────────────────────────────────────────────────

    pub fn run_call_guard(self: Arc<Self>) {
        std::thread::Builder::new()
            .name("dos-callguard".into())
            .spawn(move || {
                let mut last = false;
                let mut streak = 0u8;
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    let (enabled, ignore) = {
                        let s = self.settings.lock().unwrap();
                        (s.call_guard, s.call_guard_ignore.clone())
                    };
                    let users: Vec<String> = if enabled {
                        platform::mic_users()
                            .into_iter()
                            .filter(|u| !ignore.iter().any(|i| !i.is_empty() && u.to_lowercase().contains(&i.to_lowercase())))
                            .collect()
                    } else {
                        vec![]
                    };
                    let on = !users.is_empty();
                    // Two readings in a row before changing state: no flapping.
                    streak = if on != last { streak + 1 } else { 0 };
                    if streak < 2 {
                        continue;
                    }
                    streak = 0;
                    last = on;
                    {
                        let mut vs = self.voice_state.lock().unwrap();
                        vs.on_call = on;
                        vs.mic_users = users.clone();
                    }
                    log::line(if on { format!("call guard on ({})", users.join(", ")) } else { "call guard off".into() });
                    if let Some(v) = self.voice.lock().unwrap().as_ref() {
                        if on {
                            v.pause();
                        } else {
                            v.resume();
                        }
                    }
                    if on {
                        self.stop_speaking();
                    }
                    self.emit_state();
                }
            })
            .expect("call guard thread");
    }

    // ── relay (dosctl / Hermes) ─────────────────────────────────────────────

    /// Runs on a pipe thread. May block for an Ask.
    pub fn relay(self: &Arc<Self>, line: String) -> String {
        let resp = match serde_json::from_str::<Request>(&line) {
            Err(_) => Response::err("not a Dos Live request"),
            Ok(req) => match relay::check(&req) {
                Err(why) => {
                    log::line(format!("relay refused: {why}"));
                    Response::err(why)
                }
                Ok(()) => self.relay_ok(req),
            },
        };
        serde_json::to_string(&resp).unwrap_or_else(|_| r#"{"ok":false}"#.into())
    }

    fn relay_ok(self: &Arc<Self>, req: Request) -> Response {
        let settings = self.settings.lock().unwrap().clone();
        match req {
            Request::Ping => Response::ok(),
            Request::Status => match self.snapshot.lock().unwrap().as_ref() {
                Some(s) => Response { ok: true, status: Some(relay::redacted_status(s)), ..Default::default() },
                None => Response::err("no board data yet"),
            },
            Request::Say { text, title, world, level, speak, source } => {
                let source = source.unwrap_or_else(|| "relay".into());
                let world = relay::known_world(world.as_deref(), &settings.worlds);
                let mut it = item(FeedKind::Say, title.clone().unwrap_or_else(|| source.clone()), text.clone(), &source);
                it.world = world.clone();
                it.alert = level == relay::Level::Alert;
                self.push_feed(it);
                self.set_focus(Focus { kind: "say".into(), id: String::new(), title: title.clone().unwrap_or_default(), text: text.clone(), url: None });
                if level == relay::Level::Alert {
                    self.alert(Alert { kind: "say".into(), id: String::new(), title: title.unwrap_or(source), body: text.clone(), world, url: None });
                    self.cue("ping");
                }
                if speak {
                    self.speak(&text);
                }
                Response::ok()
            }
            Request::Ask { question, detail, world, timeout_secs, source } => {
                let id = format!("local-{}", self.ask_seq.fetch_add(1, Ordering::Relaxed));
                let ask = Ask {
                    id: id.clone(),
                    question,
                    detail: detail.unwrap_or_default(),
                    world: relay::known_world(world.as_deref(), &settings.worlds),
                    url: String::new(),
                    at: Some(Utc::now()),
                    source: source.unwrap_or_else(|| "relay".into()),
                };
                let (tx, rx) = mpsc::channel();
                self.local_asks.lock().unwrap().push(LocalAsk { ask: ask.clone(), reply: tx });
                self.announce_ask(&ask, &settings);
                self.emit_state();
                let wait = Duration::from_secs(timeout_secs.unwrap_or(relay::DEFAULT_ASK_SECS).clamp(5, relay::MAX_ASK_SECS));
                match rx.recv_timeout(wait) {
                    Ok(d) => Response { ok: true, decision: Some(d.word().into()), ..Default::default() },
                    Err(_) => {
                        self.local_asks.lock().unwrap().retain(|l| l.ask.id != id);
                        self.push_feed(item(FeedKind::System, format!("No answer: {}", ask.question), "timed out", "dos"));
                        Response::err("no answer in time")
                    }
                }
            }
        }
    }

    pub fn world_state_summary(&self) -> Option<WorldState> {
        self.snapshot.lock().unwrap().as_ref().and_then(|s| s.worlds.iter().map(|w| w.state).min())
    }
}
