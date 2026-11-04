// Dos's natural voice: Kokoro v1.0 (an open, Apache-2.0 speech model) run on
// this PC by the sherpa-onnx engine. Text goes in, audio comes out, nothing
// leaves the machine — which matters, because briefings name tickets and money.
//
// Neither the engine nor the model is part of the app. Both are downloaded
// once from the sherpa-onnx GitHub releases into %LOCALAPPDATA%\Dos Live\voices,
// checked against pinned SHA-256 hashes, and the engine is loaded at runtime
// (libloading). So the app builds with nothing extra, and if the voice is
// missing or broken Dos simply keeps speaking with the Windows voice.
//
// One engine thread owns the model. A long reply is spoken a sentence at a
// time: the first sentence plays while the next is being made, so Dos starts
// talking in about a second instead of after the whole paragraph.

use std::ffi::{c_char, c_void, CString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

// ── what gets downloaded ────────────────────────────────────────────────────

struct Pack {
    name: &'static str,
    url: &'static str,
    bytes: u64,
    sha256: &'static str,
}

const ENGINE_VERSION: &str = "v1.12.9";

#[cfg(windows)]
const ENGINE: Pack = Pack {
    name: "engine",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.12.9/sherpa-onnx-v1.12.9-win-x64-shared.tar.bz2",
    bytes: 24_139_676,
    sha256: "57a0dff63c468ed067c6ffd1a9c9b6f66f4cf73304d44b5f17518010ce2e0977",
};
#[cfg(not(windows))]
const ENGINE: Pack = Pack {
    name: "engine",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.12.9/sherpa-onnx-v1.12.9-linux-x64-shared.tar.bz2",
    bytes: 24_606_687,
    sha256: "cd22ee337e205674536643d2bc86e984fbbbf9c5c52743c4df6fc8d8dd17371b",
};

const MODEL: Pack = Pack {
    name: "voice",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_0.tar.bz2",
    bytes: 132_353_410,
    sha256: "72c347230107ef8a31098ec4d468a605244beef5f1ed00db97efa13518800653",
};

pub const MODEL_DIR: &str = "kokoro-int8-multi-lang-v1_0";

/// The engine's shared libraries, in load order (dependencies first).
#[cfg(windows)]
const ENGINE_LIBS: &[&str] = &["onnxruntime.dll", "onnxruntime_providers_shared.dll", "sherpa-onnx-c-api.dll"];
#[cfg(not(windows))]
const ENGINE_LIBS: &[&str] = &["libonnxruntime.so", "libsherpa-onnx-c-api.so"];

/// The model files the engine opens. Kokoro v1.0 is multilingual and the
/// engine insists on its Chinese lexicon and dictionary even for English — and
/// ends the whole process if they're missing — so every one is checked first.
const MODEL_FILES: &[&str] = &["model.int8.onnx", "voices.bin", "tokens.txt", "lexicon-us-en.txt", "lexicon-gb-en.txt", "lexicon-zh.txt"];
const MODEL_DIRS: &[&str] = &["espeak-ng-data", "dict"];

/// A short breath after each sentence so back-to-back sentences don't run together.
const SENTENCE_GAP_MS: u32 = 170;

// ── voices ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct VoiceDef {
    pub id: &'static str,
    pub label: &'static str,
    #[serde(skip)]
    pub sid: i32,
}

/// The English voices in Kokoro v1.0, by speaker id (from the model's metadata).
pub const VOICES: &[VoiceDef] = &[
    VoiceDef { id: "am_puck", label: "Puck — American, light and upbeat", sid: 18 },
    VoiceDef { id: "af_heart", label: "Heart — American, warm", sid: 3 },
    VoiceDef { id: "am_michael", label: "Michael — American, calm", sid: 16 },
    VoiceDef { id: "am_fenrir", label: "Fenrir — American, deep", sid: 14 },
    VoiceDef { id: "am_adam", label: "Adam — American", sid: 11 },
    VoiceDef { id: "am_echo", label: "Echo — American", sid: 12 },
    VoiceDef { id: "am_eric", label: "Eric — American", sid: 13 },
    VoiceDef { id: "am_liam", label: "Liam — American", sid: 15 },
    VoiceDef { id: "am_onyx", label: "Onyx — American, low", sid: 17 },
    VoiceDef { id: "af_bella", label: "Bella — American", sid: 2 },
    VoiceDef { id: "af_nicole", label: "Nicole — American, soft", sid: 6 },
    VoiceDef { id: "af_sarah", label: "Sarah — American", sid: 9 },
    VoiceDef { id: "af_nova", label: "Nova — American", sid: 7 },
    VoiceDef { id: "bm_george", label: "George — British, steady", sid: 26 },
    VoiceDef { id: "bm_fable", label: "Fable — British, crisp", sid: 25 },
    VoiceDef { id: "bm_lewis", label: "Lewis — British", sid: 27 },
    VoiceDef { id: "bm_daniel", label: "Daniel — British", sid: 24 },
    VoiceDef { id: "bf_emma", label: "Emma — British", sid: 21 },
    VoiceDef { id: "bf_isabella", label: "Isabella — British", sid: 22 },
    VoiceDef { id: "bf_alice", label: "Alice — British", sid: 20 },
];

pub const DEFAULT_VOICE: &str = "am_puck";

fn voice_def(id: &str) -> &'static VoiceDef {
    VOICES.iter().find(|v| v.id == id).unwrap_or(&VOICES[0])
}

/// British voices read with British pronunciation.
fn lang_of(id: &str) -> &'static str {
    if voice_def(id).id.starts_with('b') {
        "en-gb"
    } else {
        "en-us"
    }
}

pub fn voices_dir() -> PathBuf {
    crate::settings::local_dir().join("voices")
}

pub fn model_dir() -> PathBuf {
    voices_dir().join(MODEL_DIR)
}

pub fn engine_dir() -> PathBuf {
    voices_dir().join(format!("engine-{ENGINE_VERSION}"))
}

fn model_ok(d: &Path) -> bool {
    MODEL_FILES.iter().all(|f| std::fs::metadata(d.join(f)).map(|m| m.len() > 0).unwrap_or(false))
        && MODEL_DIRS.iter().all(|f| d.join(f).is_dir())
}

fn engine_ok(d: &Path) -> bool {
    ENGINE_LIBS.iter().all(|f| std::fs::metadata(d.join(f)).map(|m| m.len() > 0).unwrap_or(false))
}

/// Every file the engine opens is there. Unpacking lands in a scratch folder
/// renamed into place only when complete, so a half-finished download never
/// looks installed.
pub fn installed() -> bool {
    model_ok(&model_dir()) && engine_ok(&engine_dir())
}

// ── the engine, loaded at runtime ───────────────────────────────────────────
//
// These mirror sherpa-onnx v1.12.9's c-api.h exactly (field order and types).
// Only pointers, int32 and float: the same layout on Windows and Linux x64.

#[repr(C)]
struct VitsCfg {
    model: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    noise_scale_w: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct MatchaCfg {
    acoustic_model: *const c_char,
    vocoder: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct KokoroCfg {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
    dict_dir: *const c_char,
    lexicon: *const c_char,
    lang: *const c_char,
}

#[repr(C)]
struct KittenCfg {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
}

#[repr(C)]
struct ModelCfg {
    vits: VitsCfg,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    matcha: MatchaCfg,
    kokoro: KokoroCfg,
    kitten: KittenCfg,
}

#[repr(C)]
struct TtsCfg {
    model: ModelCfg,
    rule_fsts: *const c_char,
    max_num_sentences: i32,
    rule_fars: *const c_char,
    silence_scale: f32,
}

#[repr(C)]
struct Audio {
    samples: *const f32,
    n: i32,
    sample_rate: i32,
}

type CreateFn = unsafe extern "C" fn(*const TtsCfg) -> *const c_void;
type DestroyFn = unsafe extern "C" fn(*const c_void);
type GenerateFn = unsafe extern "C" fn(*const c_void, *const c_char, i32, f32) -> *const Audio;
type FreeAudioFn = unsafe extern "C" fn(*const Audio);

struct Engine {
    create: CreateFn,
    destroy: DestroyFn,
    generate: GenerateFn,
    free_audio: FreeAudioFn,
    /// Kept loaded for as long as the function pointers above are used.
    _libs: Vec<libloading::Library>,
}

fn open_lib(path: &Path) -> Result<libloading::Library, String> {
    // SAFETY: loading a library runs its initialisers; these are the pinned,
    // hash-checked sherpa-onnx / onnxruntime builds.
    unsafe {
        #[cfg(windows)]
        {
            use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
            // Altered search path: the DLL's own folder is searched for its dependencies.
            Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH).map(Into::into)
        }
        #[cfg(not(windows))]
        {
            use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
            Library::open(Some(path), RTLD_NOW | RTLD_GLOBAL).map(Into::into)
        }
    }
    .map_err(|e| format!("couldn't load {} ({e})", path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()))
}

impl Engine {
    fn load(dir: &Path) -> Result<Engine, String> {
        if !engine_ok(dir) {
            return Err("the voice engine isn't downloaded".into());
        }
        let mut libs = Vec::new();
        for name in ENGINE_LIBS {
            libs.push(open_lib(&dir.join(name))?);
        }
        let api = libs.last().expect("at least one library");
        // SAFETY: the symbol types match c-api.h of the pinned engine version.
        unsafe {
            let sym = |name: &[u8]| -> Result<*const c_void, String> {
                api.get::<*const c_void>(name)
                    .map(|s| *s)
                    .map_err(|e| format!("the voice engine is missing {} ({e})", String::from_utf8_lossy(&name[..name.len() - 1])))
            };
            let create = std::mem::transmute::<*const c_void, CreateFn>(sym(b"SherpaOnnxCreateOfflineTts\0")?);
            let destroy = std::mem::transmute::<*const c_void, DestroyFn>(sym(b"SherpaOnnxDestroyOfflineTts\0")?);
            let generate = std::mem::transmute::<*const c_void, GenerateFn>(sym(b"SherpaOnnxOfflineTtsGenerate\0")?);
            let free_audio = std::mem::transmute::<*const c_void, FreeAudioFn>(sym(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio\0")?);
            Ok(Engine { create, destroy, generate, free_audio, _libs: libs })
        }
    }
}

/// One open Kokoro model.
pub struct Tts {
    engine: Arc<Engine>,
    raw: *const c_void,
}

// The engine thread is the only user; sherpa-onnx's TTS object isn't tied to
// the thread that made it.
unsafe impl Send for Tts {}

impl Drop for Tts {
    fn drop(&mut self) {
        // SAFETY: raw came from create and is destroyed exactly once.
        unsafe { (self.engine.destroy)(self.raw) }
    }
}

pub struct Clip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Tts {
    /// Opens the model with the engine in `engine_dir`. A model that won't open
    /// is refused here, before a null engine could reach `generate`.
    pub fn open(engine_dir: &Path, model_dir: &Path, lang: &str, threads: i32) -> Result<Tts, String> {
        if !model_ok(model_dir) {
            return Err("the voice files aren't all there".into());
        }
        let engine = Arc::new(Engine::load(engine_dir)?);
        let c = |s: &str| CString::new(s).map_err(|_| "a voice file path has a NUL in it".to_string());
        let p = |f: &str| c(&model_dir.join(f).to_string_lossy());
        let lexicon = if lang == "en-gb" { "lexicon-gb-en.txt" } else { "lexicon-us-en.txt" };
        let lexicons = c(&format!(
            "{},{}",
            model_dir.join(lexicon).to_string_lossy(),
            model_dir.join("lexicon-zh.txt").to_string_lossy()
        ))?;
        let (model, voices, tokens, data, dict, lang_c, provider) =
            (p("model.int8.onnx")?, p("voices.bin")?, p("tokens.txt")?, p("espeak-ng-data")?, p("dict")?, c(lang)?, c("cpu")?);
        // SAFETY: every pointer refers to a CString alive for this call; the
        // rest of the config is zeroed, which the engine reads as "default".
        let raw = unsafe {
            let mut cfg: TtsCfg = std::mem::zeroed();
            cfg.model.num_threads = threads;
            cfg.model.provider = provider.as_ptr();
            cfg.model.kokoro = KokoroCfg {
                model: model.as_ptr(),
                voices: voices.as_ptr(),
                tokens: tokens.as_ptr(),
                data_dir: data.as_ptr(),
                length_scale: 1.0,
                dict_dir: dict.as_ptr(),
                lexicon: lexicons.as_ptr(),
                lang: lang_c.as_ptr(),
            };
            cfg.max_num_sentences = 1;
            (engine.create)(&cfg)
        };
        if raw.is_null() {
            return Err("the voice engine couldn't open the voice files".into());
        }
        Ok(Tts { engine, raw })
    }

    pub fn speak(&mut self, text: &str, sid: i32, speed: f32) -> Result<Clip, String> {
        let t = CString::new(text.replace('\0', " ")).map_err(|e| e.to_string())?;
        // SAFETY: raw is a live engine; the returned audio is copied, then freed once.
        unsafe {
            let a = (self.engine.generate)(self.raw, t.as_ptr(), sid, speed);
            if a.is_null() {
                return Err("the voice engine made no audio".into());
            }
            let clip = if (*a).samples.is_null() || (*a).n <= 0 {
                Clip { samples: Vec::new(), sample_rate: (*a).sample_rate.max(1) as u32 }
            } else {
                Clip {
                    samples: std::slice::from_raw_parts((*a).samples, (*a).n as usize).to_vec(),
                    sample_rate: (*a).sample_rate.max(1) as u32,
                }
            };
            (self.engine.free_audio)(a);
            Ok(clip)
        }
    }
}

// ── the engine thread ───────────────────────────────────────────────────────

pub enum Out {
    /// One sentence, ready to play.
    Chunk { text: String, wav: Vec<u8> },
    /// The natural voice couldn't speak this; say it another way.
    Failed { text: String, error: String },
}

enum Job {
    Say { gen: u64, text: String, voice: String, speed: f32 },
    Warm { voice: String },
}

pub struct Natural {
    tx: Mutex<mpsc::Sender<Job>>,
    gen: Arc<AtomicU64>,
    loaded: Arc<AtomicBool>,
}

impl Natural {
    pub fn start<F>(out: F) -> Natural
    where
        F: Fn(Out) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Job>();
        let gen = Arc::new(AtomicU64::new(0));
        let loaded = Arc::new(AtomicBool::new(false));
        let (g, l) = (gen.clone(), loaded.clone());
        let _ = std::thread::Builder::new().name("dos-natural".into()).spawn(move || {
            let mut engine: Option<(&'static str, Tts)> = None;
            for job in rx {
                match job {
                    Job::Warm { voice } => {
                        if let Err(e) = ensure(&mut engine, lang_of(&voice), &l) {
                            crate::log::line(format!("natural voice: {e}"));
                        }
                    }
                    Job::Say { gen, text, voice, speed } => {
                        if gen != g.load(Ordering::SeqCst) {
                            continue;
                        }
                        if let Err(error) = ensure(&mut engine, lang_of(&voice), &l) {
                            out(Out::Failed { text, error });
                            continue;
                        }
                        let Some((_, tts)) = engine.as_mut() else { continue };
                        let sid = voice_def(&voice).sid;
                        let parts = sentences(&text);
                        for (i, part) in parts.iter().enumerate() {
                            if gen != g.load(Ordering::SeqCst) {
                                break;
                            }
                            let started = Instant::now();
                            match tts.speak(part, sid, speed.clamp(0.6, 1.6)) {
                                Ok(clip) if !clip.samples.is_empty() => {
                                    if i == 0 {
                                        crate::log::line(format!("natural voice: first sentence in {} ms", started.elapsed().as_millis()));
                                    }
                                    out(Out::Chunk { text: part.clone(), wav: wav16(&clip.samples, clip.sample_rate) });
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    out(Out::Failed { text: parts[i..].join(" "), error: e });
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
        Natural { tx: Mutex::new(tx), gen, loaded }
    }

    pub fn say(&self, text: String, voice: &str, speed: f32) {
        let gen = self.gen.load(Ordering::SeqCst);
        let _ = self.tx.lock().unwrap().send(Job::Say { gen, text, voice: voice.to_string(), speed });
    }

    /// Load the model ahead of the first sentence (it takes a few seconds).
    pub fn warm(&self, voice: &str) {
        if installed() {
            let _ = self.tx.lock().unwrap().send(Job::Warm { voice: voice.to_string() });
        }
    }

    /// Drop everything queued or half-spoken.
    pub fn stop(&self) {
        self.gen.fetch_add(1, Ordering::SeqCst);
    }

    pub fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }
}

fn ensure(engine: &mut Option<(&'static str, Tts)>, lang: &'static str, loaded: &AtomicBool) -> Result<(), String> {
    if matches!(engine, Some((l, _)) if *l == lang) {
        return Ok(());
    }
    *engine = None;
    loaded.store(false, Ordering::Relaxed);
    if !installed() {
        return Err("the natural voice isn't downloaded yet".into());
    }
    let threads = std::thread::available_parallelism().map(|n| n.get().clamp(1, 4) as i32).unwrap_or(2);
    let started = Instant::now();
    let tts = Tts::open(&engine_dir(), &model_dir(), lang, threads)?;
    crate::log::line(format!("natural voice: loaded ({lang}, {threads} threads) in {} ms", started.elapsed().as_millis()));
    *engine = Some((lang, tts));
    loaded.store(true, Ordering::Relaxed);
    Ok(())
}

/// Sentences, with tiny fragments folded into their neighbour so the voice
/// never stops after two words.
pub fn sentences(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let end = matches!(c, '.' | '!' | '?' | ';' | '\n') && chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        if end && cur.trim().chars().count() >= 24 {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    let rest = cur.trim();
    if !rest.is_empty() {
        match out.last_mut() {
            Some(last) if rest.chars().count() < 24 => {
                last.push(' ');
                last.push_str(rest);
            }
            _ => out.push(rest.to_string()),
        }
    }
    out
}

/// 16-bit mono WAV, with a short silence after the sentence.
pub fn wav16(samples: &[f32], rate: u32) -> Vec<u8> {
    let gap = (rate * SENTENCE_GAP_MS / 1000) as usize;
    let n = samples.len() + gap;
    let data_len = (n * 2) as u32;
    let mut out = Vec::with_capacity(44 + n * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out.resize(44 + n * 2, 0);
    out
}

// ── the one-time download ───────────────────────────────────────────────────

static DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "downloading", "unpacking", "ready" or "failed".
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub downloading: bool,
    pub loaded: bool,
    pub size_mb: u64,
}

const TOTAL_BYTES: u64 = ENGINE.bytes + MODEL.bytes;

pub fn status(n: Option<&Natural>) -> Status {
    Status {
        installed: installed(),
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        loaded: n.map(|n| n.loaded()).unwrap_or(false),
        size_mb: TOTAL_BYTES / 1_000_000,
    }
}

fn tell(app: &AppHandle, p: Progress) {
    let _ = app.emit("voice-download", p);
}

/// Downloads, checks and unpacks the engine and the voice, reporting progress
/// as `voice-download` events. A second call while one runs does nothing.
pub async fn download(app: AppHandle) -> Result<(), String> {
    if installed() {
        tell(&app, Progress { stage: "ready", done: TOTAL_BYTES, total: TOTAL_BYTES, error: None });
        return Ok(());
    }
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = fetch_all(&app).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    match &result {
        Ok(()) => tell(&app, Progress { stage: "ready", done: TOTAL_BYTES, total: TOTAL_BYTES, error: None }),
        Err(e) => {
            crate::log::line(format!("natural voice download: {e}"));
            tell(&app, Progress { stage: "failed", done: 0, total: TOTAL_BYTES, error: Some(e.clone()) });
        }
    }
    result
}

async fn fetch_all(app: &AppHandle) -> Result<(), String> {
    let dir = voices_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent(concat!("DosLive/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    if !engine_ok(&engine_dir()) {
        let file = fetch(app, &http, &ENGINE, &dir, 0).await?;
        tell(app, Progress { stage: "unpacking", done: ENGINE.bytes, total: TOTAL_BYTES, error: None });
        let (f, d) = (file.clone(), dir.clone());
        tauri::async_runtime::spawn_blocking(move || unpack_engine(&f, &d, &engine_dir())).await.map_err(|e| e.to_string())??;
        let _ = std::fs::remove_file(&file);
    }
    if !model_ok(&model_dir()) {
        let file = fetch(app, &http, &MODEL, &dir, ENGINE.bytes).await?;
        tell(app, Progress { stage: "unpacking", done: TOTAL_BYTES, total: TOTAL_BYTES, error: None });
        let (f, d) = (file.clone(), dir.clone());
        tauri::async_runtime::spawn_blocking(move || unpack_model(&f, &d)).await.map_err(|e| e.to_string())??;
        let _ = std::fs::remove_file(&file);
    }
    if installed() {
        Ok(())
    } else {
        Err("the download unpacked, but some voice files are missing".into())
    }
}

/// Streams one archive to disk, hashing as it goes; refuses it unless the size
/// and SHA-256 match exactly what was pinned.
async fn fetch(app: &AppHandle, http: &reqwest::Client, pack: &Pack, dir: &Path, base: u64) -> Result<PathBuf, String> {
    let part = dir.join(format!("{}.tar.bz2.part", pack.name));
    let mut resp = http.get(pack.url).send().await.map_err(|e| format!("can't reach GitHub ({e})"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {} for the {}", resp.status(), pack.name));
    }
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut done = 0u64;
    let mut last = Instant::now() - Duration::from_secs(1);
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => return Err(format!("download interrupted ({e})")),
        };
        if done + chunk.len() as u64 > pack.bytes {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err(format!("the {} download is bigger than expected — refusing it", pack.name));
        }
        hash.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(250) {
            last = Instant::now();
            tell(app, Progress { stage: "downloading", done: base + done, total: TOTAL_BYTES, error: None });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let got = hex(&hash.finalize());
    if done != pack.bytes || got != pack.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(if done != pack.bytes {
            format!("the {} download stopped early ({done} of {} bytes) — try again", pack.name, pack.bytes)
        } else {
            format!("the {} download didn't match its checksum — refusing it", pack.name)
        });
    }
    Ok(part)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

type Archive = tar::Archive<bzip2::read::BzDecoder<std::io::BufReader<std::fs::File>>>;

fn open_archive(path: &Path) -> Result<Archive, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    Ok(tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(file))))
}

/// The voice: the whole folder, into a scratch folder renamed into place once
/// complete. tar's `unpack` refuses entries that would land outside it.
fn unpack_model(archive: &Path, dir: &Path) -> Result<(), String> {
    let scratch = dir.join("unpacking");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    open_archive(archive)?.unpack(&scratch).map_err(|e| format!("couldn't unpack the voice ({e})"))?;
    let unpacked = scratch.join(MODEL_DIR);
    if !model_ok(&unpacked) {
        return Err("the voice archive didn't contain the expected files".into());
    }
    let target = dir.join(MODEL_DIR);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&unpacked, &target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(())
}

/// The engine: only the shared libraries it needs, picked by exact file name
/// from its lib/ folder, then moved into `target` once all are there.
fn unpack_engine(archive: &Path, dir: &Path, target: &Path) -> Result<(), String> {
    let scratch = dir.join("engine-unpacking");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let mut ar = open_archive(archive)?;
    for entry in ar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let in_lib = path.parent().and_then(|p| p.file_name()).map(|n| n == "lib").unwrap_or(false);
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
        if in_lib && ENGINE_LIBS.contains(&name.as_str()) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            std::fs::write(scratch.join(&name), bytes).map_err(|e| e.to_string())?;
        }
    }
    if !engine_ok(&scratch) {
        return Err("the engine archive didn't contain the expected libraries".into());
    }
    let _ = std::fs::remove_dir_all(target);
    std::fs::rename(&scratch, target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_fold_fragments() {
        let s = sentences("Good afternoon, Elijah. Three things need you today. Land and Housing first, the venture needs a look. OK.");
        assert_eq!(
            s,
            vec!["Good afternoon, Elijah. Three things need you today.", "Land and Housing first, the venture needs a look. OK."]
        );
        assert_eq!(sentences("Hi."), vec!["Hi."]);
        assert!(sentences("  ").is_empty());
    }

    #[test]
    fn wav_has_a_tail_and_valid_header() {
        let w = wav16(&[0.0; 2400], 24000);
        assert_eq!(&w[0..4], b"RIFF");
        let d = dos_core::speech::wav_duration(&w);
        assert!(d.as_millis() >= 100 + SENTENCE_GAP_MS as u128 - 5);
    }

    #[test]
    fn voices_resolve() {
        assert_eq!(voice_def("am_puck").sid, 18);
        assert_eq!(voice_def("af_heart").sid, 3);
        assert_eq!(voice_def("nonsense").id, DEFAULT_VOICE);
        assert_eq!(lang_of("bm_george"), "en-gb");
    }

    #[test]
    fn missing_files_are_refused_gently() {
        let e = Tts::open(Path::new("/nonexistent-engine"), Path::new("/nonexistent-model"), "en-us", 1).err().unwrap();
        assert!(e.contains("aren't all there"), "{e}");
    }

    /// The whole path for real: download → size and hash check → unpack →
    /// load the engine at runtime → speak. Needs network.
    /// DOS_VOICE_E2E=<scratch dir> cargo test -p dos-live downloads_unpacks -- --ignored --nocapture
    #[test]
    #[ignore]
    fn downloads_unpacks_and_speaks() {
        let Ok(scratch) = std::env::var("DOS_VOICE_E2E") else { return };
        let dir = PathBuf::from(scratch);
        std::fs::create_dir_all(&dir).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let http = reqwest::Client::new();
        for pack in [&ENGINE, &MODEL] {
            let part = dir.join(format!("{}.tar.bz2.part", pack.name));
            if !part.exists() {
                let bytes = rt.block_on(async { http.get(pack.url).send().await.unwrap().bytes().await.unwrap() });
                std::fs::write(&part, &bytes).unwrap();
            }
            let bytes = std::fs::read(&part).unwrap();
            assert_eq!(bytes.len() as u64, pack.bytes, "{} size", pack.name);
            assert_eq!(hex(&Sha256::digest(&bytes)), pack.sha256, "{} hash", pack.name);
        }
        unpack_engine(&dir.join("engine.tar.bz2.part"), &dir, &dir.join("engine")).unwrap();
        assert!(engine_ok(&dir.join("engine")));
        if !model_ok(&dir.join(MODEL_DIR)) {
            unpack_model(&dir.join("voice.tar.bz2.part"), &dir).unwrap();
        }
        let started = Instant::now();
        let mut tts = Tts::open(&dir.join("engine"), &dir.join(MODEL_DIR), "en-us", 2).expect("opens");
        eprintln!("loaded in {:?}", started.elapsed());
        for voice in ["am_puck", "af_heart"] {
            let t = Instant::now();
            let clip = tts.speak("Good afternoon, Elijah. Three things need you today.", voice_def(voice).sid, 1.0).expect("speaks");
            eprintln!("{voice}: {:.1}s of audio in {:?}", clip.samples.len() as f32 / clip.sample_rate as f32, t.elapsed());
            assert_eq!(clip.sample_rate, 24000);
            assert!(clip.samples.len() > 24000);
            std::fs::write(dir.join(format!("{voice}.wav")), wav16(&clip.samples, clip.sample_rate)).unwrap();
        }
    }
}
