// Mirrors the Rust types that cross the bridge (serde camelCase).

export type WorldStateName = "alert" | "attention" | "active" | "quiet" | "empty";
export type Stage = "todo" | "doing" | "hold" | "done" | "other";

export interface ItemBrief {
  cardId: string;
  name: string;
  stage: Stage;
  flags: string[];
  url: string;
  due: string | null;
  board: string;
}

export interface WorldStatus {
  id: string;
  name: string;
  short: string;
  color: string;
  state: WorldStateName;
  open: number;
  doing: number;
  hold: number;
  todo: number;
  overdue: number;
  dueSoon: number;
  stale: number;
  flagged: number;
  headline: string;
  items: ItemBrief[];
}

export interface Ping {
  cardId: string;
  kind: string;
  title: string;
  summary: string;
  link: string;
  cardUrl: string;
  at: string | null;
  live: boolean;
}

export interface Ask {
  id: string;
  question: string;
  detail: string;
  world: string | null;
  url: string;
  at: string | null;
  source: string;
}

export interface BoardHealth {
  key: string;
  ok: boolean;
  error: string | null;
}

export interface Snapshot {
  takenAt: string;
  worlds: WorldStatus[];
  pings: Ping[];
  asks: Ask[];
  boards: BoardHealth[];
  asksListId: string | null;
  answeredListId: string | null;
  notesListId: string | null;
}

export type FeedKind = "ping" | "say" | "heard" | "reply" | "ask" | "note" | "system";

export interface FeedItem {
  id: string;
  kind: FeedKind;
  title: string;
  body: string;
  world: string | null;
  url: string | null;
  at: string;
  alert: boolean;
  source: string;
}

export interface VoiceState {
  mode: string;
  listening: boolean;
  dictating: boolean;
  speaking: boolean;
  muted: boolean;
  onCall: boolean;
  micUsers: string[];
  problem: string | null;
  lastHeard: string | null;
  noteMode: boolean;
}

export interface Focus {
  kind: string;
  id: string;
  title: string;
  text: string;
  url: string | null;
}

export interface ViewState {
  snapshot: Snapshot | null;
  feed: FeedItem[];
  asks: Ask[];
  voice: VoiceState;
  problems: string[];
  trelloReady: boolean;
  hermesEnabled: boolean;
  focus: Focus | null;
  paused: boolean;
  polling: boolean;
}

export interface WorldDef {
  id: string;
  name: string;
  short: string;
  color: string;
  boards: string[];
  labels: string[];
  keywords: string[];
  wholeBoards: string[];
  aliases: string[];
}

export interface Settings {
  boards: { dos: string; don: string; boom: string };
  pollSecs: number;
  layout: { pingPrefixes: string[]; asksList: string; answeredList: string; notesList: string };
  worlds: WorldDef[];
  staleDays: number;
  dueSoonDays: number;
  voice: {
    mode: "wake" | "hotkey" | "off" | string;
    wakeWord: string;
    voiceName: string;
    rate: number;
    speakPings: boolean;
    speakAsks: boolean;
    hotkey: string;
  };
  callGuard: boolean;
  callGuardIgnore: string[];
  hermes: { enabled: boolean; url: string; model: string };
  ui: {
    screen: string;
    sound: boolean;
    volume: number;
    stealth: boolean;
    discreetWork: boolean;
    autostart: boolean;
    expandOnAlert: boolean;
    collapseAfterSecs: number;
  };
}

export interface Keys {
  trelloKey: boolean;
  trelloToken: boolean;
  hermesKey: boolean;
}

export interface BootInfo {
  settings: Settings;
  state: ViewState;
  keys: Keys;
  version: string;
  windows: boolean;
}

export interface Alert {
  kind: "ping" | "ask" | "say";
  id: string;
  title: string;
  body: string;
  world: string | null;
  url: string | null;
}

export interface Speech {
  id: number;
  text: string;
  wav: string;
  millis: number;
}

export type Intent =
  | { kind: "wake" | "status" | "latest" | "read_that" | "open_that" | "hide" | "show" | "mute" | "unmute" | "cancel" | "repeat" | "note_prompt" }
  | { kind: "decide"; value: "do_it" | "hold" | "skip" }
  | { kind: "world"; value: string }
  | { kind: "note"; value: string }
  | { kind: "ask"; value: string };
