// The island: a terminal bar hugging the top edge of the screen that drops
// open into Dos's panel (feed, worlds, questions) or a single alert card.

import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/800.css";
import "./island.css";

import { Bridge, inTauri } from "./bridge";
import { DosFace, type Mood } from "./dos";
import { configureSound, cue } from "./sound";
import { enqueue, setVoiceVolume, stopAll } from "./speech";
import type { Alert, Ask, FeedItem, Settings, ViewState, WorldStatus } from "./types";

type Mode = "bar" | "panel" | "alert";
type Tab = "feed" | "worlds" | "asks";

const $ = <T extends HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector(sel) as T;

let settings: Settings;
let state: ViewState;
let mode: Mode = "bar";
let tab: Tab = "feed";
let alert: Alert | null = null;
let alertTimer = 0;
let leaveTimer = 0;
let hovering = false;
let speakingText = "";
let playing = false;
let thinkingUntil = 0;
let happyUntil = 0;
let openWorld: string | null = null;
let tickerTarget = "";
let tickerShown = "";
let tickerTimer = 0;

const root = $("#root");
root.innerHTML = `
<div id="shell" class="shell mode-bar" role="region" aria-label="Dos Live">
  <header class="bar" title="Open Dos">
    <canvas class="mini-face" aria-hidden="true"></canvas>
    <span class="prompt"><b>dos</b><span class="dim">:~$</span></span>
    <span class="ticker" aria-live="polite"><span class="ticker-text"></span><span class="caret"></span></span>
    <span class="dots" role="list"></span>
    <button class="mic" type="button" aria-label="Talk to Dos"></button>
  </header>
  <section class="body">
    <div class="face-col">
      <canvas class="face" aria-hidden="true"></canvas>
      <p class="face-caption"></p>
      <div class="face-acts">
        <button type="button" class="act" data-act="brief">brief me</button>
        <button type="button" class="act" data-act="talk">talk</button>
      </div>
    </div>
    <div class="main-col">
      <nav class="tabs" role="tablist">
        <button type="button" role="tab" data-tab="feed">feed</button>
        <button type="button" role="tab" data-tab="worlds">worlds</button>
        <button type="button" role="tab" data-tab="asks">asks<span class="count"></span></button>
        <span class="spacer"></span>
        <button type="button" class="icon" data-act="refresh" aria-label="Refresh boards" title="Refresh boards">↻</button>
        <button type="button" class="icon" data-act="settings" aria-label="Settings" title="Settings">⚙</button>
        <button type="button" class="icon" data-act="close" aria-label="Close" title="Close (Esc)">✕</button>
      </nav>
      <div class="view" role="tabpanel"></div>
      <form class="cmd">
        <label for="cmd-input" class="dim">$</label>
        <input id="cmd-input" autocomplete="off" spellcheck="false" placeholder="try: status, note buy cement, how is the farm" />
      </form>
    </div>
    <div class="alert-col"></div>
  </section>
  <footer class="status"></footer>
</div>`;

const shell = $("#shell");
const bigFace = new DosFace($<HTMLCanvasElement>(".face"));
const miniFace = new DosFace($<HTMLCanvasElement>(".mini-face"), true);

// ── helpers ─────────────────────────────────────────────────────────────────

function esc(s: string) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

function clock(iso: string | null) {
  if (!iso) return "--:--";
  const d = new Date(iso);
  const today = new Date();
  const hm = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  if (d.toDateString() === today.toDateString()) return hm;
  return d.toLocaleDateString([], { weekday: "short" }).toLowerCase();
}

function slug(s: string) {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function firstSentence(s: string) {
  const m = s.match(/^(.{12,}?[.!?])(\s|$)/);
  return (m ? m[1] : s).trim();
}

function worldById(id: string | null): WorldStatus | undefined {
  return id ? state.snapshot?.worlds.find((w) => w.id === id) : undefined;
}

// ── geometry: tell Rust which part of the window takes the mouse ────────────

function pushRect() {
  const r = shell.getBoundingClientRect();
  void Bridge.setRect(r.left, r.top, r.width, r.height);
}
new ResizeObserver(pushRect).observe(shell);

function setMode(next: Mode) {
  if (next === mode) return;
  mode = next;
  shell.classList.remove("mode-bar", "mode-panel", "mode-alert");
  shell.classList.add(`mode-${next}`);
  if (next !== "alert") {
    alert = null;
    clearTimeout(alertTimer);
  }
  if (next === "bar") {
    ($<HTMLInputElement>("#cmd-input")).blur();
  }
  requestAnimationFrame(() => {
    bigFace.resize();
    pushRect();
  });
  render();
}

// ── rendering ───────────────────────────────────────────────────────────────

function mood(): Mood {
  const v = state.voice;
  const now = Date.now();
  if (v.onCall) return "quiet";
  if (playing) return "speaking";
  if (v.dictating) return "listening";
  if (now < thinkingUntil) return "thinking";
  if (now < happyUntil) return "happy";
  if (mode === "alert" && alert?.kind === "ask") return "ask";
  if (mode === "alert") return "alert";
  if (!state.trelloReady) return "offline";
  if (state.snapshot?.boards.some((b) => !b.ok)) return "error";
  if (state.asks.length) return "ask";
  return "idle";
}

function caption(): string {
  const v = state.voice;
  if (v.onCall) return `on a call${v.micUsers.length ? ` (${v.micUsers.join(", ")})` : ""} — quiet`;
  if (playing) return "speaking";
  if (v.dictating) return v.noteMode ? "listening for your note…" : "listening…";
  if (Date.now() < thinkingUntil) return "thinking…";
  if (!state.trelloReady) return "not connected";
  if (state.asks.length) return state.asks.length === 1 ? "one question for you" : `${state.asks.length} questions for you`;
  if (v.muted) return "muted";
  if (v.mode === "wake" && v.listening) return `say “${settings.voice.wakeWord}”`;
  if (v.mode === "hotkey") return `press ${settings.voice.hotkey}`;
  return "standing by";
}

function tickerLine(): string {
  const v = state.voice;
  if (v.dictating) return v.noteMode ? "listening for your note…" : "listening…";
  if (playing && speakingText) return speakingText;
  if (v.onCall) return "on a call — Dos is quiet";
  if (state.problems.length) return state.problems[0];
  if (state.asks.length) return `question waiting: ${state.asks[0].question}`;
  const latest = state.feed.find((f) => f.kind === "ping" || f.kind === "say");
  if (latest) return `${latest.title.toLowerCase()} — ${firstSentence(latest.body)}`;
  return state.trelloReady ? "all quiet" : "open settings to connect Trello";
}

function typeTicker(text: string) {
  if (text === tickerTarget) return;
  tickerTarget = text;
  clearInterval(tickerTimer);
  const el = $(".ticker-text");
  el.title = text;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    tickerShown = text;
    el.textContent = text;
    return;
  }
  tickerShown = "";
  let i = 0;
  tickerTimer = window.setInterval(() => {
    i += 2;
    tickerShown = text.slice(0, i);
    el.textContent = tickerShown;
    if (i >= text.length) clearInterval(tickerTimer);
  }, 16);
}

function renderDots() {
  const worlds = state.snapshot?.worlds ?? settings.worlds.map((w) => ({ ...w, state: "empty", headline: "", open: 0 }) as unknown as WorldStatus);
  $(".dots").innerHTML = worlds
    .map(
      (w) => `<button type="button" role="listitem" class="dot st-${w.state}" data-world="${esc(w.id)}"
        style="--c:${esc(w.color)}" title="${esc(w.name)}: ${esc(w.headline)}" aria-label="${esc(w.name)}: ${esc(w.headline)}">${esc(w.short)}</button>`,
    )
    .join("");
}

function renderMic() {
  const v = state.voice;
  const mic = $(".mic");
  mic.className = "mic";
  if (v.onCall) mic.classList.add("call");
  else if (v.dictating) mic.classList.add("hot");
  else if (v.listening) mic.classList.add("on");
  if (v.muted) mic.classList.add("muted");
  mic.title = v.onCall ? "On a call — Dos is quiet" : `Talk to Dos (${settings.voice.hotkey})`;
  mic.innerHTML = `<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><rect x="5.5" y="1.5" width="5" height="8.5" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M3 7.5a5 5 0 0 0 10 0M8 12.5v2.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>${v.onCall || v.muted ? '<path d="M2 2l12 12" stroke="currentColor" stroke-width="1.4"/>' : ""}</svg>`;
}

function feedLine(f: FeedItem) {
  const who = f.kind === "heard" ? "you" : f.kind === "reply" ? (f.source === "hermes" ? "hermes" : "dos") : f.title.toLowerCase();
  const tag = f.kind === "ping" ? "ping" : f.kind === "say" ? f.source : f.kind;
  const w = worldById(f.world);
  const body = f.kind === "heard" || f.kind === "reply" ? f.title : f.body;
  const acts = [
    f.body || f.kind === "reply" ? `<button type="button" class="link" data-say="${esc(f.id)}">read</button>` : "",
    f.url ? `<button type="button" class="link" data-open="${esc(f.url)}">open</button>` : "",
  ].join("");
  return `<article class="line k-${f.kind}${f.alert ? " is-alert" : ""}">
    <time>${clock(f.at)}</time>
    <div class="line-main">
      <div class="line-head"><span class="who">${esc(who)}</span>${w ? `<span class="wtag" style="--c:${esc(w.color)}">${esc(w.short)}</span>` : ""}<span class="tag">${esc(tag)}</span></div>
      <p>${esc(body)}</p>
    </div>
    <div class="line-acts">${acts}</div>
  </article>`;
}

function stateWord(w: WorldStatus) {
  return { alert: "overdue", attention: "needs a look", active: "moving", quiet: "parked", empty: "clear" }[w.state];
}

function worldRow(w: WorldStatus) {
  const open = openWorld === w.id;
  const items = open
    ? `<ul class="items">${w.items
        .slice(0, 6)
        .map((i) => `<li><span class="stage s-${i.stage}">${i.stage}</span><button type="button" class="link" data-open="${esc(i.url)}">${esc(i.name)}</button>${i.flags.filter((f) => f !== "on-hold").map((f) => `<span class="flag f-${slug(f)}">${esc(f)}</span>`).join("")}</li>`)
        .join("") || "<li class=\"dim\">nothing open</li>"}</ul>`
    : "";
  return `<div class="world st-${w.state}${open ? " open" : ""}" style="--c:${esc(w.color)}">
    <button type="button" class="world-head" data-world="${esc(w.id)}" aria-expanded="${open}">
      <span class="sq">${esc(w.short)}</span>
      <span class="wname">${esc(w.name)}</span>
      <span class="wstate">${stateWord(w)}</span>
      <span class="wcount">${w.open}</span>
    </button>
    <p class="headline">${esc(w.headline)}</p>
    ${items}
  </div>`;
}

function askCard(a: Ask, big = false) {
  const w = worldById(a.world);
  return `<div class="ask${big ? " big" : ""}">
    <p class="q">${esc(a.question)}</p>
    ${a.detail ? `<p class="d">${esc(a.detail)}</p>` : ""}
    <div class="ask-meta">${w ? `<span class="wtag" style="--c:${esc(w.color)}">${esc(w.short)}</span>` : ""}<span class="dim">from ${esc(a.source === "trello" ? "the Dos board" : a.source)}</span>${a.url ? `<button type="button" class="link" data-open="${esc(a.url)}">card</button>` : ""}</div>
    <div class="ask-acts">
      <button type="button" class="btn go" data-decide="do_it" data-ask="${esc(a.id)}">Do it</button>
      <button type="button" class="btn" data-decide="hold" data-ask="${esc(a.id)}">Hold</button>
      <button type="button" class="btn" data-decide="skip" data-ask="${esc(a.id)}">Skip</button>
    </div>
  </div>`;
}

function renderView() {
  const view = $(".view");
  document.querySelectorAll<HTMLButtonElement>(".tabs [data-tab]").forEach((b) => {
    const on = b.dataset.tab === tab;
    b.classList.toggle("on", on);
    b.setAttribute("aria-selected", String(on));
  });
  $(".tabs .count").textContent = state.asks.length ? ` ${state.asks.length}` : "";

  if (!state.trelloReady) {
    view.innerHTML = `<div class="empty"><p>Dos can't see your boards yet.</p><p class="dim">Add your Trello API key and token, and the pings, worlds and questions show up here.</p><button type="button" class="btn go" data-act="settings">Open settings</button></div>`;
    return;
  }
  if (tab === "feed") {
    view.innerHTML = state.feed.length
      ? state.feed.slice(0, 40).map(feedLine).join("")
      : `<div class="empty"><p>Nothing yet.</p><p class="dim">Pings from your scheduled tasks land here as they arrive.</p></div>`;
  } else if (tab === "worlds") {
    const worlds = state.snapshot?.worlds ?? [];
    view.innerHTML = worlds.length ? `<div class="worlds">${worlds.map(worldRow).join("")}</div>` : `<div class="empty"><p>Reading the boards…</p></div>`;
  } else {
    view.innerHTML = state.asks.length
      ? state.asks.map((a) => askCard(a)).join("")
      : `<div class="empty"><p>Nothing needs your call.</p><p class="dim">When Dos wants a go-ahead, the question shows up here and on the bar. Answer by clicking, or say “${esc(settings.voice.wakeWord)}, do it”.</p></div>`;
  }
}

function renderAlert() {
  const col = $(".alert-col");
  if (!alert) {
    col.innerHTML = "";
    return;
  }
  if (alert.kind === "ask") {
    const a = state.asks.find((x) => x.id === alert!.id) ?? {
      id: alert.id, question: alert.title, detail: alert.body, world: alert.world, url: alert.url ?? "", at: null, source: "dos",
    };
    col.innerHTML = `<p class="alert-kind">question</p>${askCard(a, true)}`;
    return;
  }
  const w = worldById(alert.world);
  col.innerHTML = `<p class="alert-kind">${esc(alert.kind === "ping" ? "new ping" : "message")}${w ? ` <span class="wtag" style="--c:${esc(w.color)}">${esc(w.short)}</span>` : ""}</p>
    <h2>${esc(alert.title)}</h2>
    <p class="alert-body">${esc(alert.body)}</p>
    <div class="ask-acts">
      ${alert.url ? `<button type="button" class="btn go" data-open="${esc(alert.url)}">Open</button>` : ""}
      <button type="button" class="btn" data-act="read-focus">Read it to me</button>
      <button type="button" class="btn" data-act="dismiss">Dismiss</button>
    </div>`;
}

function renderStatus() {
  const v = state.voice;
  const parts: string[] = [];
  if (v.onCall) parts.push(`<span class="warn">on a call — voice paused</span>`);
  else if (v.mode === "off") parts.push(`<span class="dim">voice off</span>`);
  else if (v.mode === "hotkey") parts.push(`<span>push-to-talk ${esc(settings.voice.hotkey)}</span>`);
  else if (v.listening) parts.push(`<span class="ok">● listening for “${esc(settings.voice.wakeWord)}”</span>`);
  else parts.push(`<span class="warn">○ not listening</span>`);
  if (v.muted) parts.push(`<span class="dim">muted</span>`);
  if (v.problem) parts.push(`<span class="warn" title="${esc(v.problem)}">${esc(v.problem.length > 70 ? v.problem.slice(0, 68) + "…" : v.problem)}</span>`);
  for (const p of state.problems.slice(0, 1)) parts.push(`<span class="warn" title="${esc(p)}">${esc(p)}</span>`);
  const right = state.polling ? "reading boards…" : state.snapshot ? `boards ${clock(state.snapshot.takenAt)}` : "";
  $(".status").innerHTML = `<div class="left">${parts.join("")}</div><div class="right dim">${state.paused ? "paused" : esc(right)}${settings.ui.stealth ? ` · <span title="Hidden from screen sharing and recordings">stealth</span>` : ""}</div>`;
}

function render() {
  if (!state) return;
  renderDots();
  renderMic();
  typeTicker(tickerLine());
  shell.classList.toggle("has-ask", state.asks.length > 0);
  if (mode === "panel") renderView();
  if (mode === "alert") renderAlert();
  if (mode !== "bar") {
    $(".face-caption").textContent = caption();
    renderStatus();
  }
  const m = mood();
  bigFace.setMood(m);
  miniFace.setMood(m);
}

// ── alerts ──────────────────────────────────────────────────────────────────

function showAlert(a: Alert) {
  if (!settings.ui.expandOnAlert || mode === "panel") {
    render();
    return;
  }
  alert = a;
  setMode("alert");
  renderAlert();
  armAlertTimer();
}

function armAlertTimer() {
  clearTimeout(alertTimer);
  const secs = alert?.kind === "ask" ? Math.max(45, settings.ui.collapseAfterSecs) : settings.ui.collapseAfterSecs;
  alertTimer = window.setTimeout(() => {
    if (!hovering && mode === "alert") setMode("bar");
  }, secs * 1000);
}

// ── events ──────────────────────────────────────────────────────────────────

$(".bar").addEventListener("click", (e) => {
  const t = e.target as HTMLElement;
  if (t.closest(".mic")) {
    cue("wake");
    void Bridge.pushToTalk();
    return;
  }
  const dot = t.closest<HTMLElement>(".dot");
  if (dot) {
    tab = "worlds";
    openWorld = dot.dataset.world ?? null;
    setMode("panel");
    renderView();
    return;
  }
  miniFace.poke();
  setMode(mode === "panel" ? "bar" : "panel");
});

shell.addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const el = t.closest<HTMLElement>("[data-tab],[data-act],[data-open],[data-decide],[data-say],[data-world]");
  if (!el || el.closest(".bar")) return;
  if (el.dataset.tab) {
    tab = el.dataset.tab as Tab;
    renderView();
  } else if (el.dataset.open) {
    void Bridge.openLink(el.dataset.open);
  } else if (el.dataset.decide) {
    const id = el.dataset.ask!;
    el.closest(".ask")?.classList.add("answered");
    try {
      await Bridge.decide(id, el.dataset.decide as "do_it" | "hold" | "skip");
      happyUntil = Date.now() + 1400;
      if (mode === "alert") setMode("bar");
    } catch (err) {
      el.closest(".ask")?.classList.remove("answered");
      cue("error");
      void Bridge.log(`decide failed: ${String(err)}`);
    }
  } else if (el.dataset.say) {
    const item = state.feed.find((f) => f.id === el.dataset.say);
    if (item) void Bridge.say(item.kind === "reply" || item.kind === "heard" ? item.title : `${item.title}. ${item.body}`);
  } else if (el.dataset.world) {
    openWorld = openWorld === el.dataset.world ? null : el.dataset.world;
    if (openWorld) void Bridge.runIntent({ kind: "world", value: openWorld });
    renderView();
  } else {
    switch (el.dataset.act) {
      case "brief":
        thinkingUntil = Date.now() + 900;
        void Bridge.runIntent({ kind: "status" });
        break;
      case "talk":
        cue("wake");
        void Bridge.pushToTalk();
        break;
      case "refresh":
        void Bridge.refresh();
        break;
      case "settings":
        void Bridge.openSettings();
        break;
      case "close":
      case "dismiss":
        setMode("bar");
        break;
      case "read-focus":
        void Bridge.runIntent({ kind: "read_that" });
        break;
    }
  }
});

shell.addEventListener("mouseenter", () => {
  hovering = true;
  clearTimeout(leaveTimer);
});
shell.addEventListener("mouseleave", () => {
  hovering = false;
  clearTimeout(leaveTimer);
  const input = $<HTMLInputElement>("#cmd-input");
  if (mode === "panel" && document.activeElement !== input) {
    leaveTimer = window.setTimeout(() => setMode("bar"), 5000);
  } else if (mode === "alert") {
    armAlertTimer();
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    if (playing) {
      stopAll();
      void Bridge.stopSpeaking();
    } else setMode("bar");
  }
});

const input = $<HTMLInputElement>("#cmd-input");
input.addEventListener("focus", () => void Bridge.setKeyboard(true));
input.addEventListener("blur", () => void Bridge.setKeyboard(false));
$("form.cmd").addEventListener("submit", (e) => {
  e.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  input.value = "";
  thinkingUntil = Date.now() + 700;
  void Bridge.runText(text);
});

// ── boot ────────────────────────────────────────────────────────────────────

function applySettings(s: Settings) {
  settings = s;
  configureSound(s.ui.sound, s.ui.volume);
  setVoiceVolume(Math.min(1, 0.4 + s.ui.volume));
}

async function boot() {
  // The window can load before the app has finished starting; keep asking.
  let info: Awaited<ReturnType<typeof Bridge.boot>> | null = null;
  for (let attempt = 0; !info; attempt++) {
    try {
      info = await Bridge.boot();
    } catch (err) {
      if (attempt === 20) void Bridge.log(`boot still failing: ${String(err)}`).catch(() => {});
      await new Promise((r) => setTimeout(r, Math.min(2000, 150 * (attempt + 1))));
    }
  }
  applySettings(info.settings);
  state = info.state;
  requestAnimationFrame(() => {
    bigFace.resize();
    miniFace.resize();
    bigFace.start();
    miniFace.start();
    pushRect();
  });
  render();

  void Bridge.onState((s) => {
    state = s;
    render();
  });
  void Bridge.onSettings((s) => {
    applySettings(s);
    render();
  });
  void Bridge.onAlert(showAlert);
  void Bridge.onCue((name) => {
    if (name === "think") thinkingUntil = Date.now() + 8000;
    if (name === "done") happyUntil = Date.now() + 1400;
    cue(name);
    render();
  });
  void Bridge.onHeard(() => {
    thinkingUntil = Date.now() + 1500;
    render();
  });
  void Bridge.onUi((cmd) => {
    if (cmd === "expand") setMode("panel");
    else if (cmd === "collapse") setMode("bar");
    else if (cmd.startsWith("world:")) {
      tab = "worlds";
      openWorld = cmd.slice(6);
      setMode("panel");
      renderView();
    }
  });
  void Bridge.onSpeech((s) =>
    enqueue(
      s,
      (lvl) => {
        bigFace.setLevel(lvl);
        miniFace.setLevel(lvl);
      },
      (started) => {
        playing = true;
        thinkingUntil = 0;
        speakingText = started.text;
        render();
      },
      (id) => {
        playing = false;
        speakingText = "";
        void Bridge.speechDone(id);
        render();
      },
    ),
  );
  void Bridge.onSpeechStop(() => {
    stopAll();
    playing = false;
    render();
  });
  void Bridge.onCursor(({ x, y }) => {
    for (const [face, sel] of [[bigFace, ".face"], [miniFace, ".mini-face"]] as const) {
      const r = $(sel).getBoundingClientRect();
      face.lookAt(x - r.left, y - r.top);
    }
  });
  if (!inTauri && import.meta.env.DEV) {
    document.addEventListener("mousemove", (e) => {
      for (const [face, sel] of [[bigFace, ".face"], [miniFace, ".mini-face"]] as const) {
        const r = $(sel).getBoundingClientRect();
        face.lookAt(e.clientX - r.left, e.clientY - r.top);
      }
    });
    await mockScenes();
  }
  // Keep time-based moods (thinking, happy) moving between events.
  setInterval(render, 1000);
}

/** Browser preview only: `?view=` jumps to a state for screenshots. */
async function mockScenes() {
  const { mockView, mockEmit } = await import("./mock");
  const look = new URLSearchParams(location.search).get("look");
  if (look) {
    const [x, y] = look.split(",").map(Number);
    bigFace.lookAt(x, y);
    miniFace.lookAt(x, y);
  }
  switch (mockView) {
    case "panel":
    case "setup":
    case "call":
      setMode("panel");
      break;
    case "worlds":
      tab = "worlds";
      openWorld = "farm";
      setMode("panel");
      break;
    case "ask":
      showAlert({ kind: "ask", id: state.asks[0].id, title: state.asks[0].question, body: state.asks[0].detail, world: null, url: null });
      break;
    case "alert":
      showAlert({
        kind: "ping",
        id: "ping:1",
        title: "Morning Briefing",
        body: "Open day — just the noon BI standup, nothing needs you this morning.",
        world: null,
        url: "https://claude.ai",
      });
      break;
    case "listening":
      setMode("panel");
      break;
    case "speaking":
      playing = true;
      speakingText = "Four things need you. Land and Housing: Modular housing venture needs a look.";
      setMode("panel");
      setInterval(() => {
        const v = 0.35 + Math.random() * 0.6;
        bigFace.setLevel(v);
        miniFace.setLevel(v);
      }, 90);
      break;
  }
  mockEmit("state", state);
}

void boot();
