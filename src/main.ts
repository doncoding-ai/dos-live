// The island. One controller, four looks (see skins.ts): it works out which
// scene Dos is in — idle, a ping, a question, the panel, listening, speaking —
// morphs the shell to that scene's shape, moves Dos's face to its seat, and
// lets the bubble roam. Light or dark is a class on <html>; the look is chosen
// in Settings and swapped by reloading this page.

import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/800.css";
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/geist-mono/500.css";
import "@fontsource/manrope/500.css";
import "@fontsource/manrope/600.css";
import "@fontsource/manrope/700.css";
import "@fontsource/manrope/800.css";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "./island.css";

import { Bridge, inTauri } from "./bridge";
import { Bubble, TAIL } from "./bubble";
import { DosFace, type Mood } from "./dos";
import { type Box, type Scene, type SkinDef, type Slot, skinOf } from "./skins";
import { configureSound, cue } from "./sound";
import { enqueue, setVoiceVolume, stopAll } from "./speech";
import type { Alert, Ask, CursorInfo, FeedItem, Settings, Skin, ViewState, WorldStatus } from "./types";

type Mode = "bar" | "panel" | "alert";
type Tab = "feed" | "worlds" | "asks";
/** shown · tucked (idle: slid up to a line, faded, or peeking) · gone (hidden with the hotkey). */
type Vis = "shown" | "tucked" | "gone";

const $ = <T extends HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector(sel) as T;
const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

let settings: Settings;
let state: ViewState;
let def: SkinDef;
let mode: Mode = "bar";
let scene: Scene = "idle";
let tab: Tab = "feed";
let alert: Alert | null = null;
let answered: string | null = null;
let alertTimer = 0;
let leaveTimer = 0;
let hovering = false;
let playing = false;
let speakingText = "";
let thinkingUntil = 0;
let happyUntil = 0;
let openWorld: string | null = null;
let vis: Vis = "shown";
let hiddenByUser = false;
let lastActive = Date.now();
let dragging = false;
let suppressClick = false;
let press: { x: number; y: number } | null = null;
let docked = true;
let idleMs = 0;
let cursor: { x: number; y: number } | null = null;
let lastBox: Box = { w: 0, h: 0, r: 0, top: 0, hidden: true };
let speech = { words: [] as string[], start: 0, ms: 0, lit: -1 };

let stage: HTMLElement;
let shell: HTMLElement;
let dos: HTMLElement | null = null;
let dosBtn: HTMLElement | null = null;
let eyes: HTMLElement | null = null;
let miniFace: DosFace | null = null;
let bigFace: DosFace | null = null;

let bubble: Bubble | null = null;
let seat: Slot | null = null;
let raf = 0;
let frame = 0;
let throwing = false;
let roamKickUntil = 0;
let wasRoaming = false;

const root = $("#root");

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

function greeting() {
  const h = new Date().getHours();
  return h < 12 ? "Good morning" : h < 17 ? "Good afternoon" : "Good evening";
}

/** Sets text only when it changed, with a soft swap so it never just snaps. */
function swapText(el: HTMLElement | null, text: string) {
  if (!el || el.dataset.t === text) return;
  el.dataset.t = text;
  el.textContent = text;
  el.title = text;
  el.classList.remove("swap-in");
  void el.offsetWidth;
  el.classList.add("swap-in");
}

/** Sets innerHTML only when it changed — re-rendering would replay entrance animations. */
function setHtml(el: HTMLElement | null, html: string) {
  if (!el || el.dataset.h === html) return false;
  el.dataset.h = html;
  el.innerHTML = html;
  return true;
}

const ICON = {
  brief: `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M5 7h14M5 12h10M5 17h7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`,
  mic: `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3" fill="none" stroke="currentColor" stroke-width="1.9"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>`,
  refresh: `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M19 12a7 7 0 1 1-2.05-4.95M19 4v4h-4" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
  gear: `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.9"/><path d="M12 2.8v2.4M12 18.8v2.4M4.2 7.5l2.1 1.2M17.7 15.3l2.1 1.2M4.2 16.5l2.1-1.2M17.7 8.7l2.1-1.2" stroke="currentColor" stroke-width="1.9" stroke-linecap="round"/></svg>`,
  close: `<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`,
  check: `<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
};

// ── markup ──────────────────────────────────────────────────────────────────

function idleMarkup(s: Skin) {
  if (s === "liquid") {
    return `<button type="button" class="idle-hit drag-zone" data-act="open" aria-label="Open Dos">
      <span class="face-gap"></span><span class="idle-text"></span><span class="mini-dots" aria-hidden="true"></span>
    </button>`;
  }
  if (s === "terminal") {
    return `<div class="tbar drag-zone">
      <button type="button" class="helm-btn" data-act="open" aria-label="Open Dos"><canvas class="mini-face" aria-hidden="true"></canvas></button>
      <span class="prompt"><b>dos</b><span class="dim">:~$</span></span>
      <button type="button" class="ticker" data-act="open" aria-live="polite">
        <span class="tick-col"><span class="tick-line t-cur"><span class="tt"></span><i class="caret"></i></span><span class="tick-line t-next"><span class="tt"></span><i class="caret"></i></span></span>
      </button>
      <span class="swap"><span class="dots" role="list"></span><span class="meter" aria-hidden="true">${"<i></i>".repeat(12)}</span></span>
      <button type="button" class="mic" aria-label="Talk to Dos"></button>
    </div>`;
  }
  return "";
}

function listenMarkup(s: Skin) {
  if (s === "terminal") return "";
  const wave = s === "liquid" ? `<span class="wave" aria-hidden="true">${"<i></i>".repeat(14)}</span>` : "";
  const hint = s === "presence" ? `<span class="li-hint">try “how is the farm” or “note: buy cement”</span>` : "";
  return `<div class="li"><span class="face-gap"></span>${wave}<span class="li-text">Listening…</span>${hint}</div>`;
}

function panelMarkup(s: Skin) {
  const placeholder = s === "terminal" ? "try: status, note buy cement, how is the farm" : "Ask Dos — “how is the farm”, “note: buy cement”";
  return `<div class="p-wrap">
    <aside class="p-side">
      <canvas class="face" aria-hidden="true"></canvas>
      <p class="face-caption"></p>
      <div class="face-acts"><button type="button" class="pill" data-act="brief">brief me</button><button type="button" class="pill" data-act="talk">talk</button></div>
    </aside>
    <div class="p-main">
      <header class="p-head drag-zone">
        <span class="face-gap"></span>
        <div class="p-title"><span class="p-greet"></span><span class="p-sum"></span></div>
        <nav class="tabs" role="tablist" aria-label="Dos">
          <span class="tabpill" aria-hidden="true"></span>
          <button type="button" role="tab" data-tab="feed">feed</button>
          <button type="button" role="tab" data-tab="worlds">worlds</button>
          <button type="button" role="tab" data-tab="asks">asks<span class="count"></span></button>
        </nav>
        <span class="spacer"></span>
        <button type="button" class="icon side-dup" data-act="brief" aria-label="Brief me" title="Brief me">${ICON.brief}</button>
        <button type="button" class="icon side-dup" data-act="talk" aria-label="Talk to Dos" title="Talk">${ICON.mic}</button>
        <button type="button" class="icon" data-act="refresh" aria-label="Refresh boards" title="Refresh boards">${ICON.refresh}</button>
        <button type="button" class="icon" data-act="settings" aria-label="Settings" title="Settings">${ICON.gear}</button>
        <button type="button" class="icon" data-act="close" aria-label="Close" title="Close (Esc)">${ICON.close}</button>
      </header>
      <div class="view" role="tabpanel"></div>
      <form class="cmd">
        <label for="cmd-input" class="cmd-label">$<span class="sr"> Ask Dos</span></label>
        <input id="cmd-input" autocomplete="off" spellcheck="false" placeholder="${esc(placeholder)}" />
      </form>
      <footer class="status"></footer>
    </div>
  </div>`;
}

function dosMarkup(s: Skin) {
  const tail = s === "bubble" ? `<div class="tail" aria-hidden="true">${"<i></i>".repeat(TAIL)}</div>` : "";
  const halo = s === "presence" ? `<svg class="halo" viewBox="0 0 100 100" aria-hidden="true"></svg>` : "";
  return `${tail}<div id="dos" class="dos">
    <span class="ripples" aria-hidden="true"><i></i><i></i><i></i></span>
    ${halo}
    <button type="button" class="dos-btn" data-act="open" aria-label="Open Dos"><span class="dos-body"><span class="eyes"><i></i><i></i></span></span></button>
  </div>`;
}

function build() {
  const s = def.id;
  root.innerHTML = `<div id="stage" class="stage skin-${s} scene-idle vis-shown">
    <div id="shell" class="shell" role="region" aria-label="Dos Live">
      <div class="layer l-idle">${idleMarkup(s)}</div>
      <div class="layer l-alert" aria-live="polite"></div>
      <div class="layer l-panel">${panelMarkup(s)}</div>
      <div class="layer l-listen">${listenMarkup(s)}</div>
      <div class="layer l-speak"><div class="sp"><span class="face-gap"></span><span class="sp-kick">dos ›</span><p class="words" aria-live="polite"></p></div></div>
    </div>
    ${def.face === "inline" ? "" : dosMarkup(s)}
  </div>`;
  stage = $("#stage");
  shell = $("#shell");
  dos = document.getElementById("dos");
  dosBtn = dos ? $(".dos-btn", dos) : null;
  eyes = dos ? $(".eyes", dos) : null;
  if (s === "terminal") {
    miniFace = new DosFace($<HTMLCanvasElement>(".mini-face"), true);
    bigFace = new DosFace($<HTMLCanvasElement>(".p-side .face"));
  }
  if (s === "bubble") bubble = new Bubble(window.innerWidth / 2, 26, 30);
}

// ── scenes and geometry ─────────────────────────────────────────────────────

const LAYER: Record<Scene, string> = { idle: "l-idle", ping: "l-alert", ask: "l-alert", panel: "l-panel", listen: "l-listen", speak: "l-speak" };

function sceneNow(): Scene {
  if (mode === "panel") return "panel";
  if (mode === "alert") return alert?.kind === "ask" ? "ask" : "ping";
  if (state.voice.dictating) return "listen";
  if (playing) return "speak";
  return "idle";
}

/** Morph the shell to the current scene and send Dos to its seat. */
function layout() {
  const next = sceneNow();
  if (next !== scene) {
    scene = next;
    stage.classList.remove("scene-idle", "scene-ping", "scene-ask", "scene-panel", "scene-listen", "scene-speak");
    stage.classList.add(`scene-${scene}`);
  }
  const active = $(`.${LAYER[scene]}`, shell);
  shell.querySelectorAll<HTMLElement>(".layer").forEach((el) => el.classList.toggle("on", el === active));
  const offset = def.id === "terminal" && scene !== "idle" && scene !== "listen" ? 32 : 0;
  const probe = def.box(scene, 0);
  active.style.width = `${probe.w}px`;
  if (scene === "panel") active.style.height = `${probe.h - offset}px`;
  const b = def.box(scene, scene === "panel" ? 0 : active.scrollHeight);
  lastBox = b;
  shell.style.width = `${b.w}px`;
  shell.style.height = `${b.h}px`;
  shell.style.top = `${b.top}px`;
  shell.style.borderRadius = def.id === "terminal" && docked ? `0 0 ${b.r}px ${b.r}px` : `${b.r}px`;
  shell.classList.toggle("hidden", b.hidden);
  placeDos();
  pushRectsSoon();
}

function roaming(): boolean {
  if (def.id !== "bubble" || scene !== "idle" || vis === "gone" || reduceMotion) return false;
  if (state.voice.onCall || state.paused || hiddenByUser) return false;
  if (Date.now() < roamKickUntil) return true;
  if (settings.ui.roam === "always") return true;
  if (settings.ui.roam === "never") return false;
  return idleMs > 30_000;
}

function slotNow(): Slot | null {
  const W = window.innerWidth;
  const L = (W - lastBox.w) / 2;
  let s = def.slot(scene, lastBox, L, W);
  if (def.id === "bubble" && scene === "idle") {
    if (roaming()) return null;
    s = { cx: W / 2, cy: vis === "tucked" ? 6 : 26, s: 30 };
  }
  if (s && vis === "tucked" && scene === "idle" && def.id === "liquid") s = { ...s, cy: s.cy - (lastBox.h + lastBox.top - 3) };
  return s;
}

function placeDos() {
  if (!dos) return;
  const s = slotNow();
  if (bubble) {
    const roam = s === null;
    if (roam && !wasRoaming) bubble.kick();
    wasRoaming = roam;
    seat = s;
    wakeBubble();
    return;
  }
  if (!s) return;
  dos.style.width = `${s.s}px`;
  dos.style.height = `${s.s}px`;
  dos.style.transform = `translate(${s.cx - s.s / 2}px, ${s.cy - s.s / 2}px)`;
}

// ── the bubble ──────────────────────────────────────────────────────────────

function wakeBubble() {
  if (!raf && bubble) raf = requestAnimationFrame(bubbleLoop);
}

function drawBubble() {
  if (!bubble || !dos) return;
  const b = bubble;
  dos.style.width = `${b.size}px`;
  dos.style.height = `${b.size}px`;
  dos.style.transform = `translate(${b.x - b.size / 2}px, ${b.y - b.size / 2}px)`;
  const [sx, sy] = b.squash();
  $(".dos-body", dos).style.transform = `scale(${sx.toFixed(3)}, ${sy.toFixed(3)})`;
  if (!seat && eyes) {
    eyes.style.transform = `translate(${Math.max(-3, Math.min(3, b.vx * 0.5)).toFixed(1)}px, ${Math.max(-3, Math.min(3, b.vy * 0.35)).toFixed(1)}px)`;
  }
  const bits = b.tail();
  stage.querySelectorAll<HTMLElement>(".tail i").forEach((el, i) => {
    const t = bits[i];
    el.style.width = `${t.s.toFixed(1)}px`;
    el.style.height = `${t.s.toFixed(1)}px`;
    el.style.opacity = t.o.toFixed(3);
    el.style.transform = `translate(${t.x.toFixed(1)}px, ${t.y.toFixed(1)}px)`;
  });
}

function bubbleLoop() {
  raf = 0;
  if (!bubble) return;
  bubble.step(seat, window.innerWidth, window.innerHeight, cursor);
  drawBubble();
  if (++frame % 2 === 0) pushRects();
  if (seat === null || bubble.moving || bubble.held) raf = requestAnimationFrame(bubbleLoop);
  else pushRects();
}

function startThrow(e: PointerEvent) {
  if (!bubble || !dosBtn) return;
  const btn = dosBtn;
  btn.setPointerCapture(e.pointerId);
  const start = { x: e.clientX, y: e.clientY };
  let moved = false;
  const move = (ev: PointerEvent) => {
    if (!moved && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
    if (!moved) {
      moved = true;
      throwing = true;
      stage.classList.add("dragging");
      pushRects();
    }
    bubble!.hold(ev.clientX, ev.clientY);
    drawBubble();
  };
  const up = () => {
    btn.removeEventListener("pointermove", move);
    btn.removeEventListener("pointerup", up);
    btn.removeEventListener("pointercancel", up);
    if (!moved) return;
    suppressClick = true;
    throwing = false;
    stage.classList.remove("dragging");
    bubble!.release();
    roamKickUntil = Date.now() + 6000;
    wasRoaming = true;
    if (mode !== "bar") setMode("bar");
    else placeDos();
    wakeBubble();
    pushRects();
  };
  btn.addEventListener("pointermove", move);
  btn.addEventListener("pointerup", up);
  btn.addEventListener("pointercancel", up);
}

// ── click-through: tell Rust which shapes take the mouse ────────────────────

let rectKey = "";
let rectTimer = 0;

function pushRects() {
  let out: number[][] = [];
  if (throwing) out = [[0, 0, window.innerWidth, window.innerHeight]];
  else if (vis !== "gone") {
    if (!lastBox.hidden) {
      const r = shell.getBoundingClientRect();
      if (r.width > 1 && r.height > 1) out.push([r.left, r.top, r.width, r.height]);
    }
    if (dosBtn) {
      const r = dosBtn.getBoundingClientRect();
      if (r.width > 1) out.push([r.left, r.top, r.width, r.height]);
    }
  }
  const key = out.map((r) => r.map((n) => Math.round(n)).join(",")).join(";");
  if (key === rectKey) return;
  rectKey = key;
  void Bridge.setRects(out);
}

/** After a morph: now, mid-way and once it has settled. */
function pushRectsSoon() {
  pushRects();
  clearTimeout(rectTimer);
  window.setTimeout(pushRects, 260);
  rectTimer = window.setTimeout(pushRects, 720);
}

// ── tuck & hide ─────────────────────────────────────────────────────────────

function wantVis(): Vis {
  const now = Date.now();
  if (dragging || hovering || throwing) return "shown";
  const busy = mode !== "bar" || playing || state.voice.dictating || now < thinkingUntil;
  if (busy) {
    lastActive = now;
    return "shown";
  }
  if (hiddenByUser) return "gone";
  if (def.id === "bubble" && roaming()) return "shown";
  if (settings.ui.autoHide && now - lastActive > settings.ui.hideAfterSecs * 1000) return "tucked";
  return "shown";
}

function applyVis() {
  const next = wantVis();
  if (next === vis) return;
  vis = next;
  stage.classList.remove("vis-shown", "vis-tucked", "vis-gone");
  stage.classList.add(`vis-${vis}`);
  placeDos();
  pushRectsSoon();
}

function wake() {
  lastActive = Date.now();
  applyVis();
}

function setDocked(d: boolean) {
  docked = d;
  stage.classList.toggle("floating", !d);
  layout();
}

function setMode(next: Mode) {
  if (next === mode) return;
  mode = next;
  if (next !== "alert") {
    alert = null;
    answered = null;
    clearTimeout(alertTimer);
  }
  if (next === "bar") $<HTMLInputElement>("#cmd-input").blur();
  render();
  requestAnimationFrame(() => {
    bigFace?.resize();
    positionTabPill();
  });
}

// ── moods ───────────────────────────────────────────────────────────────────

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

function tone(): string {
  const v = state.voice;
  if (playing || v.dictating) return "blue";
  if (v.onCall || !state.trelloReady || state.paused) return "quiet";
  if (scene === "ask" || state.asks.length) return "amber";
  return "green";
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

function summary(): string {
  if (!state.trelloReady) return "connect Trello in Settings";
  if (state.voice.onCall) return "on a call — quiet";
  if (state.asks.length) return state.asks.length === 1 ? "1 question waiting" : `${state.asks.length} questions waiting`;
  const need = (state.snapshot?.worlds ?? []).filter((w) => w.state === "alert" || w.state === "attention").length;
  if (need) return `${need} thing${need > 1 ? "s" : ""} need${need > 1 ? "" : "s"} you`;
  return "all quiet";
}

function tickerLine(): string {
  const v = state.voice;
  if (v.dictating) return v.noteMode ? "listening for your note…" : "listening…";
  if (playing && speakingText) return speakingText;
  if (v.onCall) return "on a call — Dos is quiet";
  if (state.problems.length) return state.problems[0];
  if (state.asks.length) return `question waiting — ${state.asks[0].question}`;
  const latest = state.feed.find((f) => f.kind === "ping" || f.kind === "say");
  if (latest) return `${latest.title.toLowerCase()} — ${firstSentence(latest.body)}`;
  return state.trelloReady ? "all quiet" : "open settings to connect Trello";
}

// ── the terminal's rolling ticker ───────────────────────────────────────────

let tickTarget = "";
let tickBusy = false;

function rollTicker(text: string) {
  if (def.id !== "terminal" || text === tickTarget) return;
  tickTarget = text;
  if (tickBusy) return;
  const col = $(".tick-col");
  const cur = $(".t-cur .tt");
  const nxt = $(".t-next .tt");
  if (!cur.textContent || reduceMotion) {
    cur.textContent = text;
    $(".ticker").title = text;
    return;
  }
  tickBusy = true;
  nxt.textContent = text;
  col.classList.add("roll");
  window.setTimeout(() => {
    cur.textContent = nxt.textContent;
    col.classList.add("snap");
    col.classList.remove("roll");
    void col.offsetWidth;
    col.classList.remove("snap");
    $(".ticker").title = cur.textContent ?? "";
    tickBusy = false;
    if (tickTarget !== cur.textContent) {
      const t = tickTarget;
      tickTarget = "";
      rollTicker(t);
    }
  }, 600);
}

// ── rendering ───────────────────────────────────────────────────────────────

function worldsNow(): WorldStatus[] {
  return state.snapshot?.worlds ?? settings.worlds.map((w) => ({ ...w, state: "empty", headline: "", open: 0, items: [] }) as unknown as WorldStatus);
}

function renderIdle() {
  const worlds = worldsNow();
  if (def.id === "liquid") {
    swapText($(".idle-text"), summary());
    setHtml($(".mini-dots"), worlds.map((w) => `<i class="st-${w.state}"></i>`).join(""));
  }
  if (def.id === "terminal") {
    rollTicker(tickerLine());
    setHtml(
      $(".dots"),
      worlds
        .map(
          (w) => `<button type="button" role="listitem" class="dot st-${w.state}" data-world="${esc(w.id)}"
          title="${esc(w.name)}: ${esc(w.headline)}" aria-label="${esc(w.name)}: ${esc(w.headline)}">${esc(w.short)}</button>`,
        )
        .join(""),
    );
    renderMic();
  }
  if (def.id === "presence") renderHalo(worlds);
}

const HALO_COLOR: Record<string, string> = { alert: "#ff6b5d", attention: "#ffb547", active: "#3ddc84", quiet: "#7d8c9a", empty: "#4a5866" };

function renderHalo(worlds: WorldStatus[]) {
  const svg = dos?.querySelector(".halo") as HTMLElement | null;
  if (!svg || !worlds.length) return;
  const C = 2 * Math.PI * 47;
  const seg = C / worlds.length;
  setHtml(
    svg,
    worlds
      .map(
        (w, i) =>
          `<circle cx="50" cy="50" r="47" fill="none" stroke="${HALO_COLOR[w.state]}" stroke-width="2.6" stroke-linecap="round" stroke-dasharray="${(seg - 7).toFixed(2)} ${(C - seg + 7).toFixed(2)}" transform="rotate(${(i * 360) / worlds.length - 90} 50 50)"></circle>`,
      )
      .join(""),
  );
}

function renderMic() {
  const v = state.voice;
  const mic = $(".mic");
  if (!mic) return;
  mic.className = "mic";
  if (v.onCall) mic.classList.add("call");
  else if (v.dictating) mic.classList.add("hot");
  else if (v.listening) mic.classList.add("on");
  if (v.muted) mic.classList.add("muted");
  mic.title = v.onCall ? "On a call — Dos is quiet" : `Talk to Dos (${settings.voice.hotkey})`;
  setHtml(mic, `${ICON.mic}${v.onCall || v.muted ? '<i class="slash"></i>' : ""}`);
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
  return `<article class="line row k-${f.kind}${f.alert ? " is-alert" : ""}">
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
    ? `<ul class="items">${
        w.items
          .slice(0, 6)
          .map(
            (i) =>
              `<li><span class="stage-tag s-${i.stage}">${i.stage}</span><button type="button" class="link" data-open="${esc(i.url)}">${esc(i.name)}</button>${i.flags
                .filter((f) => f !== "on-hold")
                .map((f) => `<span class="flag f-${slug(f)}">${esc(f)}</span>`)
                .join("")}</li>`,
          )
          .join("") || '<li class="dim">nothing open</li>'
      }</ul>`
    : "";
  return `<div class="world row st-${w.state}${open ? " open" : ""}" style="--c:${esc(w.color)}">
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

function askCard(a: Ask) {
  const w = worldById(a.world);
  return `<div class="ask row">
    <p class="q">${esc(a.question)}</p>
    ${a.detail ? `<p class="d">${esc(a.detail)}</p>` : ""}
    <div class="ask-meta">${w ? `<span class="wtag" style="--c:${esc(w.color)}">${esc(w.short)}</span>` : ""}<span class="dim">from ${esc(a.source === "trello" ? "the Dos board" : a.source)}</span>${a.url ? `<button type="button" class="link" data-open="${esc(a.url)}">card</button>` : ""}</div>
    <div class="ask-acts">${decideButtons(a.id)}</div>
  </div>`;
}

function decideButtons(id: string) {
  return `<button type="button" class="pill go" data-decide="do_it" data-ask="${esc(id)}"><kbd>Y</kbd>Do it</button>
    <button type="button" class="pill" data-decide="hold" data-ask="${esc(id)}"><kbd>H</kbd>Hold</button>
    <button type="button" class="pill" data-decide="skip" data-ask="${esc(id)}"><kbd>S</kbd>Skip</button>`;
}

function positionTabPill() {
  const cur = shell.querySelector<HTMLElement>(".tabs [data-tab].on");
  const pill = shell.querySelector<HTMLElement>(".tabpill");
  if (!cur || !pill) return;
  pill.style.width = `${cur.offsetWidth}px`;
  pill.style.transform = `translateX(${cur.offsetLeft}px)`;
}

function renderView() {
  const view = $(".view");
  shell.querySelectorAll<HTMLButtonElement>(".tabs [data-tab]").forEach((b) => {
    const on = b.dataset.tab === tab;
    b.classList.toggle("on", on);
    b.setAttribute("aria-selected", String(on));
  });
  $(".tabs .count").textContent = state.asks.length ? ` ${state.asks.length}` : "";
  swapText($(".p-greet"), def.id === "presence" ? `${greeting()}.` : "dos");
  swapText($(".p-sum"), summary().replace(/^./, (c) => c.toUpperCase()));
  positionTabPill();

  if (!state.trelloReady) {
    setHtml(view, `<div class="empty"><p>Dos can't see your boards yet.</p><p class="dim">Add your Trello API key and token, and the pings, worlds and questions show up here.</p><button type="button" class="pill go" data-act="settings">Open settings</button></div>`);
    return;
  }
  if (tab === "feed") {
    setHtml(
      view,
      state.feed.length
        ? state.feed.slice(0, 40).map(feedLine).join("")
        : `<div class="empty"><p>Nothing yet.</p><p class="dim">Pings from your scheduled tasks land here as they arrive.</p></div>`,
    );
  } else if (tab === "worlds") {
    const worlds = state.snapshot?.worlds ?? [];
    setHtml(view, worlds.length ? `<div class="worlds">${worlds.map(worldRow).join("")}</div>` : `<div class="empty"><p>Reading the boards…</p></div>`);
  } else {
    setHtml(
      view,
      state.asks.length
        ? state.asks.map(askCard).join("")
        : `<div class="empty"><p>Nothing needs your call.</p><p class="dim">When Dos wants a go-ahead, the question shows up here and on the bar. Answer by clicking, or say “${esc(settings.voice.wakeWord)}, do it”.</p></div>`,
    );
  }
}

function renderAlert() {
  const el = $(".l-alert");
  if (!alert) {
    setHtml(el, "");
    return;
  }
  const term = def.id === "terminal";
  const now = clock(new Date().toISOString());
  let html: string;
  if (alert.kind === "ask") {
    const a = state.asks.find((x) => x.id === alert!.id) ?? {
      id: alert.id, question: alert.title, detail: alert.body, world: alert.world, url: alert.url ?? "", at: null, source: "dos",
    };
    const from = a.source === "trello" ? "Dos board" : a.source;
    const kick = term ? `? dos asks · ${from}` : def.id === "presence" ? "Dos needs a decision" : `Dos asks · ${from}`;
    html = `<div class="a-wrap"><span class="face-gap"></span><div class="a-main">
      <span class="kicker k-ask">${esc(kick)}</span>
      <h2 class="a-title">${esc(a.question)}</h2>
      ${a.detail ? `<p class="a-body">${esc(a.detail)}</p>` : ""}
      ${answered ? `<p class="a-done">${ICON.check}<span>${esc(answered)}</span></p>` : `<div class="a-acts">${decideButtons(a.id)}</div>`}
    </div></div>`;
  } else {
    const head = firstSentence(alert.body || alert.title);
    const rest = alert.body.slice(head.length).trim();
    const kick = term ? `[${now}] ${alert.title.toLowerCase()} · ${alert.kind}` : `${alert.title} · ${now}`;
    html = `<div class="a-wrap"><span class="face-gap"></span><div class="a-main">
      <span class="kicker">${esc(kick)}</span>
      <h2 class="a-title">${esc(head)}</h2>
      ${rest ? `<p class="a-body">${esc(rest)}</p>` : ""}
      <div class="a-acts">
        ${alert.url ? `<button type="button" class="pill go" data-open="${esc(alert.url)}"><kbd>O</kbd>Open</button>` : ""}
        <button type="button" class="pill" data-act="read-focus"><kbd>R</kbd>Read it</button>
        <button type="button" class="pill ghost" data-act="dismiss"><kbd>Esc</kbd>Later</button>
      </div>
    </div></div>`;
  }
  setHtml(el, html);
}

function renderWords() {
  if (!playing) return;
  const n = speech.words.length;
  const t = performance.now() - speech.start;
  const lit = speech.ms > 0 ? Math.min(n, Math.ceil((n * t * 1.05) / speech.ms)) : n;
  if (lit === speech.lit) return;
  speech.lit = lit;
  shell.querySelectorAll<HTMLElement>(".words .w").forEach((w, i) => w.classList.toggle("lit", i < lit));
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
  setHtml(
    $(".status"),
    `<div class="left">${parts.join("")}</div><div class="right dim">${state.paused ? "paused" : esc(right)}${settings.ui.stealth ? ` · <span title="Hidden from screen sharing and recordings">stealth</span>` : ""}</div>`,
  );
  const cap = shell.querySelector<HTMLElement>(".face-caption");
  if (cap) cap.textContent = caption();
  const li = shell.querySelector<HTMLElement>(".li-text");
  if (li) li.textContent = v.noteMode ? "Listening for your note…" : "Listening…";
}

function render() {
  if (!state) return;
  renderIdle();
  stage.classList.toggle("has-ask", state.asks.length > 0);
  if (mode === "panel") renderView();
  if (mode === "alert") renderAlert();
  renderStatus();
  renderWords();
  const m = mood();
  bigFace?.setMood(m);
  miniFace?.setMood(m);
  if (dos) {
    const t = tone();
    for (const c of ["green", "amber", "blue", "quiet"]) dos.classList.toggle(`tone-${c}`, c === t);
    dos.classList.toggle("talking", playing);
    dos.classList.toggle("listening", state.voice.dictating);
    dos.classList.toggle("thinking", Date.now() < thinkingUntil);
    dos.classList.toggle("happy", Date.now() < happyUntil);
  }
  layout();
  applyVis();
}

// ── alerts ──────────────────────────────────────────────────────────────────

function showAlert(a: Alert) {
  if (!settings.ui.expandOnAlert || mode === "panel") {
    render();
    return;
  }
  alert = a;
  answered = null;
  if (mode === "alert") render();
  else setMode("alert");
  armAlertTimer();
}

function armAlertTimer() {
  clearTimeout(alertTimer);
  const secs = alert?.kind === "ask" ? Math.max(45, settings.ui.collapseAfterSecs) : settings.ui.collapseAfterSecs;
  alertTimer = window.setTimeout(() => {
    if (!hovering && mode === "alert") setMode("bar");
  }, secs * 1000);
}

const ANSWERED: Record<string, string> = { do_it: "Done — on it.", hold: "Held. I’ll ask again later.", skip: "Skipped." };

// ── events ──────────────────────────────────────────────────────────────────

function bindEvents() {
  // Press and pull past a few pixels: Windows takes over and moves the window
  // (the bubble is thrown instead — its window covers the screen).
  stage.addEventListener("pointerdown", (e) => {
    suppressClick = false;
    press = null;
    if (e.button !== 0) return;
    const t = e.target as HTMLElement;
    if (bubble && t.closest(".dos-btn")) {
      startThrow(e);
      return;
    }
    if (!t.closest(".drag-zone, .dos-btn") || t.closest(".mic, [data-tab], .icon, input, .dot")) return;
    press = { x: e.screenX, y: e.screenY };
  });
  stage.addEventListener("pointermove", (e) => {
    if (!press) return;
    if (!(e.buttons & 1)) {
      press = null;
      return;
    }
    if (Math.hypot(e.screenX - press.x, e.screenY - press.y) < 5) return;
    press = null;
    suppressClick = true;
    dragging = true;
    stage.classList.add("dragging");
    void Bridge.beginDrag();
  });
  stage.addEventListener("pointerup", () => (press = null));

  stage.addEventListener("click", async (e) => {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    const t = e.target as HTMLElement;
    if (t.closest(".mic")) {
      cue("wake");
      void Bridge.pushToTalk();
      return;
    }
    const el = t.closest<HTMLElement>("[data-tab],[data-act],[data-open],[data-decide],[data-say],[data-world]");
    if (!el) return;
    if (el.dataset.tab) {
      tab = el.dataset.tab as Tab;
      renderView();
    } else if (el.dataset.world && el.classList.contains("dot")) {
      tab = "worlds";
      openWorld = el.dataset.world;
      setMode("panel");
      renderView();
    } else if (el.dataset.open) {
      void Bridge.openLink(el.dataset.open);
    } else if (el.dataset.decide) {
      const id = el.dataset.ask!;
      const d = el.dataset.decide as "do_it" | "hold" | "skip";
      el.closest(".ask")?.classList.add("answered");
      try {
        await Bridge.decide(id, d);
        happyUntil = Date.now() + 1400;
        if (mode === "alert") {
          answered = ANSWERED[d];
          render();
          clearTimeout(alertTimer);
          alertTimer = window.setTimeout(() => setMode("bar"), 1500);
        }
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
        case "open":
          miniFace?.poke();
          setMode(mode === "panel" ? "bar" : "panel");
          break;
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

  const enter = () => {
    hovering = true;
    clearTimeout(leaveTimer);
    wake();
  };
  const leave = () => {
    hovering = false;
    lastActive = Date.now();
    clearTimeout(leaveTimer);
    const input = $<HTMLInputElement>("#cmd-input");
    if (mode === "panel" && document.activeElement !== input) leaveTimer = window.setTimeout(() => setMode("bar"), 5000);
    else if (mode === "alert") armAlertTimer();
  };
  shell.addEventListener("pointerenter", enter);
  shell.addEventListener("pointerleave", leave);
  dos?.addEventListener("pointerenter", enter);
  dos?.addEventListener("pointerleave", leave);

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

  window.addEventListener("resize", () => layout());
}

function onCursor(c: CursorInfo) {
  cursor = { x: c.x, y: c.y };
  idleMs = c.idleMs;
  if (c.inside && vis === "tucked") wake();
  for (const [face, sel] of [[bigFace, ".p-side .face"], [miniFace, ".mini-face"]] as const) {
    if (!face) continue;
    const r = $(sel).getBoundingClientRect();
    face.lookAt(c.x - r.left, c.y - r.top);
  }
  if (eyes && dosBtn && !(bubble && !seat)) {
    const r = dosBtn.getBoundingClientRect();
    const dx = c.x - (r.left + r.width / 2);
    const dy = c.y - (r.top + r.height / 2);
    eyes.style.transform = `translate(${Math.max(-3, Math.min(3, dx / 40)).toFixed(1)}px, ${Math.max(-2, Math.min(2, dy / 40)).toFixed(1)}px)`;
  }
  if (bubble) {
    const roam = roaming();
    if (roam !== (seat === null) && scene === "idle") placeDos();
  }
}

// ── theme ───────────────────────────────────────────────────────────────────

const lightQuery = window.matchMedia("(prefers-color-scheme: light)");

function applyTheme() {
  const t = settings.ui.theme;
  const light = t === "light" || (t === "system" && lightQuery.matches);
  document.documentElement.classList.toggle("theme-light", light);
}

function applySettings(s: Settings) {
  settings = s;
  configureSound(s.ui.sound, s.ui.volume);
  setVoiceVolume(Math.min(1, 0.4 + s.ui.volume));
  applyTheme();
}

// ── boot ────────────────────────────────────────────────────────────────────

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
  def = skinOf(settings.ui.skin);
  build();
  bindEvents();
  docked = info.docked;
  stage.classList.toggle("floating", !docked);
  lightQuery.addEventListener("change", applyTheme);
  requestAnimationFrame(() => {
    bigFace?.resize();
    miniFace?.resize();
    bigFace?.start();
    miniFace?.start();
  });
  render();
  // First paint without a morph from nothing.
  requestAnimationFrame(() => stage.classList.add("ready"));

  void Bridge.onState((s) => {
    state = s;
    render();
  });
  void Bridge.onSettings((s) => {
    // A new look rebuilds the page; everything else applies in place.
    if (skinOf(s.ui.skin).id !== def.id) {
      location.reload();
      return;
    }
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
    lastActive = Date.now();
    render();
  });
  void Bridge.onUi((cmd) => {
    if (cmd === "toggle-hidden") {
      hiddenByUser = !hiddenByUser;
      if (hiddenByUser) {
        setMode("bar");
        lastActive = 0;
      } else lastActive = Date.now();
      cue(hiddenByUser ? "sleep" : "wake");
      applyVis();
    } else if (cmd === "show") {
      hiddenByUser = false;
      wake();
    } else if (cmd === "drag-end") {
      dragging = false;
      stage.classList.remove("dragging");
      wake();
      pushRects();
    } else if (cmd === "dock:top" || cmd === "dock:free") setDocked(cmd === "dock:top");
    else if (cmd === "expand") setMode("panel");
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
        bigFace?.setLevel(lvl);
        miniFace?.setLevel(lvl);
        if (dos) dos.style.setProperty("--lvl", lvl.toFixed(2));
      },
      (started) => {
        playing = true;
        thinkingUntil = 0;
        speakingText = started.text;
        speech = { words: started.text.split(/\s+/).filter(Boolean), start: performance.now(), ms: started.millis || 0, lit: -1 };
        setHtml($(".words"), speech.words.map((w) => `<span class="w">${esc(w)} </span>`).join(""));
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
  void Bridge.onCursor(onCursor);
  if (!inTauri && import.meta.env.DEV) {
    document.addEventListener("mousemove", (e) => onCursor({ x: e.clientX, y: e.clientY, inside: true, idleMs: 60_000 }));
    await mockScenes();
  }
  // Keep time-based moods (thinking, happy, words, roaming) moving between events.
  setInterval(render, 1000);
  setInterval(renderWords, 90);
}

/** Browser preview only: `?view=` jumps to a state for screenshots. */
async function mockScenes() {
  const { mockView, mockEmit } = await import("./mock");
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
    case "asks":
      tab = "asks";
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
        body: "Open day — just the noon BI standup. Nothing needs you this morning.",
        world: null,
        url: "https://claude.ai",
      });
      break;
    case "speaking": {
      const text = "Four things need you. Land and Housing first — the modular venture needs a look.";
      playing = true;
      speakingText = text;
      speech = { words: text.split(" "), start: performance.now(), ms: 5200, lit: -1 };
      setHtml($(".words"), speech.words.map((w) => `<span class="w">${esc(w)} </span>`).join(""));
      setInterval(() => {
        const v = 0.35 + Math.random() * 0.6;
        bigFace?.setLevel(v);
        miniFace?.setLevel(v);
      }, 90);
      break;
    }
  }
  mockEmit("state", state);
}

void boot();
