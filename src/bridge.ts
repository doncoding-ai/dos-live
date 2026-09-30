// The only place the front end talks to Rust. Outside Tauri (plain `npm run dev`
// in a browser) it falls back to the mock in mock.ts, so the island can be
// designed and screenshotted without Windows.

import type { Alert, BootInfo, Intent, Keys, Settings, Speech, ViewState } from "./types";

type Unlisten = () => void;

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let tauriCore: typeof import("@tauri-apps/api/core") | null = null;
let tauriEvent: typeof import("@tauri-apps/api/event") | null = null;

async function core() {
  tauriCore ??= await import("@tauri-apps/api/core");
  return tauriCore;
}
async function events() {
  tauriEvent ??= await import("@tauri-apps/api/event");
  return tauriEvent;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) {
    if (!import.meta.env.DEV) throw new Error("Dos Live must run inside the app");
    const { mockInvoke } = await import("./mock");
    return mockInvoke(cmd, args) as T;
  }
  return (await core()).invoke<T>(cmd, args);
}

async function on<T>(name: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!inTauri) {
    if (!import.meta.env.DEV) return () => {};
    const { mockListen } = await import("./mock");
    return mockListen(name, handler as (p: unknown) => void);
  }
  return (await events()).listen<T>(name, (e) => handler(e.payload));
}

export const Bridge = {
  boot: () => call<BootInfo>("boot"),
  state: () => call<ViewState>("state"),
  saveSettings: (settings: Settings) => call<void>("save_settings", { settings }),
  setRect: (x: number, y: number, width: number, height: number) => call<void>("set_rect", { x, y, width, height }),
  setKeyboard: (typing: boolean) => call<void>("set_keyboard", { typing }),
  decide: (askId: string, decision: "do_it" | "hold" | "skip") => call<void>("decide", { askId, decision }),
  refresh: () => call<void>("refresh"),
  openLink: (url: string) => call<void>("open_link", { url }),
  pushToTalk: () => call<void>("push_to_talk"),
  say: (text: string) => call<void>("say", { text }),
  runIntent: (intent: Intent) => call<void>("run_intent", { intent }),
  runText: (text: string) => call<void>("run_text", { text }),
  speechDone: (id: number) => call<void>("speech_done", { id }),
  stopSpeaking: () => call<void>("stop_speaking"),
  setMuted: (muted: boolean) => call<void>("set_muted", { muted }),
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
  createNote: (text: string) => call<void>("create_note", { text }),
  secretSet: (key: string, value: string) => call<void>("secret_set", { key, value }),
  secretClear: (key: string) => call<void>("secret_clear", { key }),
  keyStatus: () => call<Keys>("key_status"),
  testTrello: () => call<string>("test_trello"),
  setupLists: () => call<string>("setup_lists"),
  testHermes: () => call<string>("test_hermes"),
  testRelay: () => call<string>("test_relay"),
  listVoices: () => call<void>("list_voices"),
  openSettings: () => call<void>("open_settings"),
  quit: () => call<void>("quit_app"),
  log: (message: string) => call<void>("log_line", { message }),

  onState: (h: (s: ViewState) => void) => on<ViewState>("state", h),
  onAlert: (h: (a: Alert) => void) => on<Alert>("alert", h),
  onSpeech: (h: (s: Speech) => void) => on<Speech>("speech", h),
  onSpeechStop: (h: () => void) => on<null>("speech-stop", () => h()),
  onCue: (h: (name: string) => void) => on<string>("cue", h),
  onUi: (h: (cmd: string) => void) => on<string>("ui", h),
  onHeard: (h: (p: { text: string }) => void) => on<{ text: string }>("heard", h),
  onCursor: (h: (p: { x: number; y: number; inside: boolean }) => void) => on("cursor", h),
  onSettings: (h: (s: Settings) => void) => on<Settings>("settings-changed", h),
  onVoices: (h: (v: { name: string; language: string; male: boolean }[]) => void) => on("voices", h),
};
