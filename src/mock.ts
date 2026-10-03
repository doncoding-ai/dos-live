// Browser-only stand-in for the Rust side, seeded with boards shaped like the
// real Dos / Don / Boom boards. `?view=panel|alert|ask|listening|speaking|call|setup`
// jumps straight to a state for design work and screenshots.

import type { BootInfo, Settings, ViewState, WorldStatus } from "./types";

type Handler = (p: unknown) => void;
const listeners = new Map<string, Set<Handler>>();

export function mockListen(name: string, h: Handler) {
  if (!listeners.has(name)) listeners.set(name, new Set());
  listeners.get(name)!.add(h);
  return () => listeners.get(name)?.delete(h);
}

export function mockEmit(name: string, payload: unknown) {
  listeners.get(name)?.forEach((h) => h(payload));
}

const view = new URLSearchParams(location.search).get("view") ?? "";

const worlds: WorldStatus[] = [
  w("work", "Work", "W", "#ffd24a", "attention", 1, 1, 0, 0, 0, 1, "4 open · 1 due soon", [
    ["Work item 1", "doing", ["due-soon"]],
  ]),
  w("finance", "Finance", "F", "#3ddc84", "active", 2, 1, 0, 1, 0, 0, "1 in progress", [
    ["Phoenix Collection — target December 2026", "doing", []],
    ["Dons Tech — revival plan + equipment list", "todo", ["flagged"]],
  ]),
  w("land", "Land & Housing", "L", "#2dd4bf", "attention", 1, 0, 1, 0, 0, 0, "Modular housing venture — needs a look", [
    ["Modular housing venture — pick a route to cost out", "hold", ["flagged", "stale", "on-hold"]],
  ]),
  w("farm", "Farm & Phoenix", "P", "#ff8a3d", "attention", 2, 1, 1, 0, 0, 0, "Goat farming — needs a look", [
    ["Goat farming — target December 2026", "hold", ["flagged", "on-hold"]],
    ["Phoenix Collection — target December 2026", "doing", []],
  ]),
  w("creative", "Creative", "C", "#b388ff", "quiet", 2, 0, 0, 2, 0, 0, "Keep the next piece in view — gone quiet", [
    ["Keep the next piece in view", "todo", ["stale"]],
    ["Creative writing & film — Hollow Men outline or next piece", "todo", ["stale"]],
  ]),
  w("donstech", "Dons Tech", "T", "#ff6ec7", "attention", 1, 0, 0, 1, 0, 0, "Dons Tech — needs a look", [
    ["Dons Tech — revival plan + equipment list", "todo", ["flagged", "stale"]],
  ]),
  w("boom", "Boom", "B", "#4aa8ff", "active", 6, 3, 1, 2, 0, 0, "3 in progress", [
    ["Dos Phase A — unify the mind", "doing", []],
    ["Set up anonymity protocol", "doing", []],
    ["Dos Phase D — Dos app", "todo", []],
  ]),
];

function w(
  id: string, name: string, short: string, color: string, state: WorldStatus["state"],
  open: number, doing: number, hold: number, todo: number, overdue: number, dueSoon: number,
  headline: string, items: [string, string, string[]][],
): WorldStatus {
  return {
    id, name, short, color, state, open, doing, hold, todo, overdue, dueSoon,
    stale: items.filter((i) => i[2].includes("stale")).length,
    flagged: items.filter((i) => i[2].includes("flagged")).length,
    headline,
    items: items.map(([n, stage, flags], i) => ({
      cardId: `${id}-${i}`, name: n, stage: stage as never, flags, url: "https://trello.com", due: null, board: id === "boom" ? "boom" : "don",
    })),
  };
}

const ask = {
  id: "6abcb00022a53f089f26b9ff",
  question: "Archive the 12 Done ping cards on the Dos board?",
  detail: "Weekly clean-up. They're all older than 7 days and already summarised in Monday's report.",
  world: null,
  url: "https://trello.com/c/ask1",
  at: new Date().toISOString(),
  source: "trello",
};

const state: ViewState = {
  snapshot: {
    takenAt: new Date().toISOString(),
    worlds,
    pings: [],
    asks: view === "ask" ? [ask] : [],
    boards: [
      { key: "dos", ok: true, error: null },
      { key: "don", ok: true, error: null },
      { key: "boom", ok: true, error: null },
    ],
    asksListId: "x", answeredListId: "y", notesListId: "z",
  },
  feed: [
    {
      id: "ping:1", kind: "ping", title: "Morning Briefing",
      body: "Open day — just the noon BI standup, nothing needs you this morning. 4 open Jira tickets (1 in progress, 3 not started), BDI-4064 due in two days.",
      world: null, url: "https://claude.ai", at: iso(-16), alert: false, source: "trello",
    },
    {
      id: "say:1", kind: "say", title: "Hermes",
      body: "Goat prices at Machakos market are up about 8% this month — worth it before December.",
      world: "farm", url: null, at: iso(-3), alert: false, source: "hermes",
    },
    {
      id: "ping:2", kind: "ping", title: "Trello Check-In",
      body: "Don: To Do untouched. Phoenix Collection underway — 10k sent to Mutheu. Goat farming card still says blocked on the loan decision.",
      world: null, url: "https://trello.com", at: iso(-38), alert: false, source: "trello",
    },
    {
      id: "ping:3", kind: "ping", title: "Weekly Priority Report",
      body: "Jira connector down again (403). Dos ping filing healthy. Persona #1 still blocked on the anonymity protocol.",
      world: null, url: "https://trello.com", at: iso(-62), alert: false, source: "trello",
    },
  ],
  asks: view === "ask" ? [ask] : [],
  voice: {
    mode: "wake",
    listening: true,
    dictating: view === "listening",
    speaking: view === "speaking",
    muted: false,
    onCall: view === "call",
    micUsers: view === "call" ? ["MSTeams"] : [],
    problem: null,
    lastHeard: view === "speaking" ? "dos status" : null,
    noteMode: false,
  },
  problems: view === "setup" ? ["Add your Trello key and token in Settings to wake Dos up."] : [],
  trelloReady: view !== "setup",
  hermesEnabled: true,
  focus: null,
  paused: false,
  polling: false,
};

function iso(hoursAgo: number) {
  return new Date(Date.now() + hoursAgo * 3600_000).toISOString();
}

const settings: Settings = {
  boards: { dos: "vKKgCDGD", don: "5jR6zssk", boom: "vbw8udrd" },
  pollSecs: 60,
  layout: { pingPrefixes: ["🔔"], asksList: "Dos asks", answeredList: "Dos answered", notesList: "To do" },
  worlds: worlds.map((x) => ({ id: x.id, name: x.name, short: x.short, color: x.color, boards: ["don"], labels: [], keywords: [], wholeBoards: [], aliases: [] })),
  staleDays: 21,
  dueSoonDays: 3,
  voice: { mode: "wake", wakeWord: "dos", voiceName: "", rate: 1, speakPings: true, speakAsks: true, hotkey: "Ctrl+Alt+D" },
  callGuard: true,
  callGuardIgnore: ["SpeechRuntime"],
  hermes: { enabled: true, url: "http://127.0.0.1:8642/v1", model: "hermes-agent" },
  ui: { screen: "primary", sound: true, volume: 0.5, stealth: true, discreetWork: true, autostart: true, expandOnAlert: true, collapseAfterSecs: 14, autoHide: true, hideAfterSecs: 8, hideHotkey: "Ctrl+Alt+H" },
};

export function mockInvoke(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "boot": {
      const b: BootInfo = { settings, state, keys: { trelloKey: view !== "setup", trelloToken: view !== "setup", hermesKey: true }, version: "0.1.0", windows: true, docked: view !== "floating" };
      return b;
    }
    case "state":
      return state;
    case "key_status":
      return { trelloKey: true, trelloToken: true, hermesKey: false };
    case "test_trello":
      return "Signed in as Elijah Ndeto — Dos (24 cards), Don (16 cards), Boom (9 cards)";
    case "decide":
      state.asks = [];
      mockEmit("state", state);
      return null;
    default:
      if (args) console.debug("[mock]", cmd, args);
      return null;
  }
}

export const mockView = view;
