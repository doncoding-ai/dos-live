//! Hearing and speaking, with Windows' own speech engine (free, no account).
//!
//! Two listeners share the microphone:
//! * the **command listener** runs continuously on a fixed list of phrases
//!   ("dos", "dos status", "dos do it" …). A list grammar is matched on the
//!   device — nothing leaves the machine;
//! * the **dictation listener** runs once, after the wake word or the
//!   push-to-talk hotkey, to catch a free sentence (a note, a question). Windows
//!   dictation uses Microsoft's online speech service, so it only works with
//!   *Settings → Privacy → Speech → Online speech recognition* switched on.
//!
//! Speaking returns WAV bytes instead of playing them: the island plays the
//! audio itself so Dos's visor can move with the voice.
//!
//! All WinRT objects live on one dedicated MTA thread, driven by commands.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use windows::core::{Ref, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionCompletedEventArgs, SpeechContinuousRecognitionResultGeneratedEventArgs,
    SpeechContinuousRecognitionSession, SpeechRecognitionConfidence, SpeechRecognitionListConstraint,
    SpeechRecognitionResultStatus, SpeechRecognitionScenario, SpeechRecognitionTopicConstraint, SpeechRecognizer,
};
use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceGender, VoiceInformation};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows_collections::IIterable;

/// HRESULT when online speech recognition is off in Windows privacy settings.
const SPEECH_PRIVACY_DECLINED: i32 = 0x8004_5509_u32 as i32;

#[derive(Debug, Clone, PartialEq)]
pub enum Heard {
    /// An exact phrase from the command list.
    Command,
    /// A free sentence from dictation.
    Dictation,
}

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    Heard { text: String, kind: Heard, confident: bool },
    /// The command listener is on (true) or off (false).
    Listening(bool),
    /// Dictation started (true) / ended (false) — the island shows "listening…".
    Dictating(bool),
    /// Dictation heard nothing usable.
    NothingHeard,
    Spoken { id: u64, wav: Vec<u8> },
    Voices(Vec<VoiceInfo>),
    Problem(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceInfo {
    pub name: String,
    pub language: String,
    pub male: bool,
}

enum Cmd {
    Listen(Vec<String>),
    StopListening,
    Pause,
    Resume,
    Dictate,
    Speak { id: u64, text: String, voice: String, rate: f64 },
    ListVoices,
    /// Internal: the continuous session ended on its own; start it again.
    Restart,
}

/// Cheap to clone; every method just posts to the voice thread.
#[derive(Clone)]
pub struct Voice {
    tx: Sender<Cmd>,
}

impl Voice {
    pub fn start<F>(on_event: F) -> Voice
    where
        F: Fn(VoiceEvent) + Send + Sync + 'static,
    {
        let (tx, rx) = channel();
        let events: Arc<dyn Fn(VoiceEvent) + Send + Sync> = Arc::new(on_event);
        let self_tx = tx.clone();
        let _ = std::thread::Builder::new().name("dos-voice".into()).spawn(move || run(rx, self_tx, events));
        Voice { tx }
    }

    /// (Re)start the command listener on these phrases.
    pub fn listen(&self, phrases: Vec<String>) {
        let _ = self.tx.send(Cmd::Listen(phrases));
    }
    pub fn stop_listening(&self) {
        let _ = self.tx.send(Cmd::StopListening);
    }
    /// Stop hearing while Dos talks or a call is on, without tearing down.
    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }
    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Resume);
    }
    /// Listen once for a free sentence.
    pub fn dictate(&self) {
        let _ = self.tx.send(Cmd::Dictate);
    }
    pub fn speak(&self, id: u64, text: String, voice: String, rate: f64) {
        let _ = self.tx.send(Cmd::Speak { id, text, voice, rate });
    }
    pub fn list_voices(&self) {
        let _ = self.tx.send(Cmd::ListVoices);
    }
}

struct Listener {
    recognizer: SpeechRecognizer,
    session: SpeechContinuousRecognitionSession,
    phrases: Vec<String>,
    paused: bool,
}

fn span(d: Duration) -> TimeSpan {
    TimeSpan { Duration: (d.as_nanos() / 100) as i64 }
}

fn friendly(e: &windows::core::Error) -> String {
    if e.code().0 == SPEECH_PRIVACY_DECLINED {
        "Dictation needs Online speech recognition: Settings → Privacy & security → Speech → turn it on. Short commands still work without it.".into()
    } else {
        format!("speech: {} ({:#010x})", e.message(), e.code().0)
    }
}

fn run(rx: Receiver<Cmd>, self_tx: Sender<Cmd>, events: Arc<dyn Fn(VoiceEvent) + Send + Sync>) {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let mut listener: Option<Listener> = None;
    let mut synth: Option<SpeechSynthesizer> = None;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Listen(phrases) => {
                stop(&mut listener);
                match start_listener(&phrases, &events, &self_tx) {
                    Ok(l) => {
                        listener = Some(l);
                        events(VoiceEvent::Listening(true));
                    }
                    Err(e) => {
                        events(VoiceEvent::Listening(false));
                        events(VoiceEvent::Problem(friendly(&e)));
                    }
                }
            }
            Cmd::Restart => {
                // The session ended by itself (device change, long silence, error).
                if let Some(l) = listener.take() {
                    if l.paused {
                        listener = Some(l);
                        continue;
                    }
                    let phrases = l.phrases.clone();
                    drop(l);
                    std::thread::sleep(Duration::from_millis(600));
                    match start_listener(&phrases, &events, &self_tx) {
                        Ok(l) => listener = Some(l),
                        Err(e) => {
                            events(VoiceEvent::Listening(false));
                            events(VoiceEvent::Problem(friendly(&e)));
                        }
                    }
                }
            }
            Cmd::StopListening => {
                stop(&mut listener);
                events(VoiceEvent::Listening(false));
            }
            Cmd::Pause => {
                if let Some(l) = listener.as_mut() {
                    if !l.paused {
                        if let Ok(op) = l.session.PauseAsync() {
                            let _ = op.get();
                        }
                        l.paused = true;
                    }
                }
            }
            Cmd::Resume => {
                if let Some(l) = listener.as_mut() {
                    if l.paused {
                        let _ = l.session.Resume();
                        l.paused = false;
                    }
                }
            }
            Cmd::Dictate => {
                let was_running = listener.as_ref().map(|l| !l.paused).unwrap_or(false);
                if was_running {
                    if let Some(l) = listener.as_mut() {
                        if let Ok(op) = l.session.PauseAsync() {
                            let _ = op.get();
                        }
                        l.paused = true;
                    }
                }
                events(VoiceEvent::Dictating(true));
                match dictate_once() {
                    Ok(Some((text, confident))) => events(VoiceEvent::Heard { text, kind: Heard::Dictation, confident }),
                    Ok(None) => events(VoiceEvent::NothingHeard),
                    Err(e) => events(VoiceEvent::Problem(friendly(&e))),
                }
                events(VoiceEvent::Dictating(false));
                if was_running {
                    if let Some(l) = listener.as_mut() {
                        let _ = l.session.Resume();
                        l.paused = false;
                    }
                }
            }
            Cmd::Speak { id, text, voice, rate } => {
                if synth.is_none() {
                    synth = SpeechSynthesizer::new().ok();
                }
                let Some(s) = synth.as_ref() else {
                    events(VoiceEvent::Problem("speech output is not available on this PC".into()));
                    continue;
                };
                match synthesize(s, &text, &voice, rate) {
                    Ok(wav) => events(VoiceEvent::Spoken { id, wav }),
                    Err(e) => events(VoiceEvent::Problem(friendly(&e))),
                }
            }
            Cmd::ListVoices => match voices() {
                Ok(v) => events(VoiceEvent::Voices(v)),
                Err(e) => events(VoiceEvent::Problem(friendly(&e))),
            },
        }
    }
}

fn stop(listener: &mut Option<Listener>) {
    if let Some(l) = listener.take() {
        if let Ok(op) = l.session.StopAsync() {
            let _ = op.get();
        }
        let _ = l.recognizer.Close();
    }
}

fn start_listener(
    phrases: &[String],
    events: &Arc<dyn Fn(VoiceEvent) + Send + Sync>,
    self_tx: &Sender<Cmd>,
) -> windows::core::Result<Listener> {
    let recognizer = SpeechRecognizer::new()?;
    let items: Vec<HSTRING> = phrases.iter().map(HSTRING::from).collect();
    let iterable: IIterable<HSTRING> = items.into();
    let list = SpeechRecognitionListConstraint::CreateWithTag(&iterable, &HSTRING::from("dos-commands"))?;
    recognizer.Constraints()?.Append(&list)?;
    let compiled = recognizer.CompileConstraintsAsync()?.get()?;
    if compiled.Status()? != SpeechRecognitionResultStatus::Success {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x8000_4005_u32 as i32),
            format!("command grammar did not compile ({:?})", compiled.Status()?),
        ));
    }
    let session = recognizer.ContinuousRecognitionSession()?;
    // Never stop on silence: the listener is meant to sit there all day.
    session.SetAutoStopSilenceTimeout(span(Duration::from_secs(60 * 60 * 24)))?;

    let ev = events.clone();
    session.ResultGenerated(&TypedEventHandler::new(
        move |_: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
            if let Some(args) = args.as_ref() {
                let result = args.Result()?;
                let confidence = result.Confidence()?;
                if confidence == SpeechRecognitionConfidence::Rejected {
                    return Ok(());
                }
                let text = result.Text()?.to_string_lossy();
                if !text.trim().is_empty() {
                    ev(VoiceEvent::Heard {
                        text,
                        kind: Heard::Command,
                        confident: confidence == SpeechRecognitionConfidence::High
                            || confidence == SpeechRecognitionConfidence::Medium,
                    });
                }
            }
            Ok(())
        },
    ))?;

    let restart = self_tx.clone();
    session.Completed(&TypedEventHandler::new(
        move |_: Ref<SpeechContinuousRecognitionSession>, _: Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
            let _ = restart.send(Cmd::Restart);
            Ok(())
        },
    ))?;

    session.StartAsync()?.get()?;
    Ok(Listener { recognizer, session, phrases: phrases.to_vec(), paused: false })
}

/// One free sentence. `Ok(None)` when nothing usable was said.
fn dictate_once() -> windows::core::Result<Option<(String, bool)>> {
    let recognizer = SpeechRecognizer::new()?;
    let topic = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &HSTRING::from("dos-dictation"))?;
    recognizer.Constraints()?.Append(&topic)?;
    let compiled = recognizer.CompileConstraintsAsync()?.get()?;
    if compiled.Status()? != SpeechRecognitionResultStatus::Success {
        let _ = recognizer.Close();
        return Err(windows::core::Error::new(
            windows::core::HRESULT(SPEECH_PRIVACY_DECLINED),
            "dictation unavailable",
        ));
    }
    let timeouts = recognizer.Timeouts()?;
    timeouts.SetInitialSilenceTimeout(span(Duration::from_secs(6)))?;
    timeouts.SetEndSilenceTimeout(span(Duration::from_millis(1100)))?;
    timeouts.SetBabbleTimeout(span(Duration::from_secs(20)))?;
    let result = recognizer.RecognizeAsync()?.get()?;
    let _ = recognizer.Close();
    if result.Status()? != SpeechRecognitionResultStatus::Success {
        return Ok(None);
    }
    let confidence = result.Confidence()?;
    let text = result.Text()?.to_string_lossy();
    if text.trim().is_empty() || confidence == SpeechRecognitionConfidence::Rejected {
        return Ok(None);
    }
    Ok(Some((text, confidence != SpeechRecognitionConfidence::Low)))
}

fn voice_list() -> windows::core::Result<Vec<VoiceInformation>> {
    let all = SpeechSynthesizer::AllVoices()?;
    let mut out = Vec::new();
    for i in 0..all.Size()? {
        out.push(all.GetAt(i)?);
    }
    Ok(out)
}

fn voices() -> windows::core::Result<Vec<VoiceInfo>> {
    voice_list()?
        .iter()
        .map(|v| {
            Ok(VoiceInfo {
                name: v.DisplayName()?.to_string_lossy(),
                language: v.Language()?.to_string_lossy(),
                male: v.Gender()? == VoiceGender::Male,
            })
        })
        .collect()
}

/// Dos's voice: the one named in settings, else a male Kenyan-English voice,
/// else any male English voice, else the system default.
fn pick_voice(wanted: &str) -> windows::core::Result<Option<VoiceInformation>> {
    let list = voice_list()?;
    let wanted = wanted.trim().to_lowercase();
    if !wanted.is_empty() {
        for v in &list {
            if v.DisplayName()?.to_string_lossy().to_lowercase().contains(&wanted) {
                return Ok(Some(v.clone()));
            }
        }
    }
    let mut male_en = None;
    for v in &list {
        let lang = v.Language()?.to_string_lossy().to_lowercase();
        let male = v.Gender()? == VoiceGender::Male;
        if male && lang == "en-ke" {
            return Ok(Some(v.clone()));
        }
        if male && lang.starts_with("en") && male_en.is_none() {
            male_en = Some(v.clone());
        }
    }
    Ok(male_en)
}

fn synthesize(s: &SpeechSynthesizer, text: &str, voice: &str, rate: f64) -> windows::core::Result<Vec<u8>> {
    if let Some(v) = pick_voice(voice)? {
        let _ = s.SetVoice(&v);
    }
    if let Ok(opts) = s.Options() {
        let _ = opts.SetSpeakingRate(rate.clamp(0.5, 2.0));
    }
    let stream = s.SynthesizeTextToStreamAsync(&HSTRING::from(text))?.get()?;
    let size = stream.Size()? as u32;
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    reader.LoadAsync(size)?.get()?;
    let mut wav = vec![0u8; size as usize];
    reader.ReadBytes(&mut wav)?;
    Ok(wav)
}
