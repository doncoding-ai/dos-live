// Settings window. Changes save as you make them; keys go straight to the
// Windows Credential Manager and are never shown again.

import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/600.css";
import "@fontsource/jetbrains-mono/800.css";
import "./settings.css";

import { Bridge } from "../bridge";
import type { Keys, Settings } from "../types";

let s: Settings;
let keys: Keys;
let saveTimer = 0;
const root = document.getElementById("settings")!;

function esc(v: string) {
  return v.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

function field(label: string, control: string, hint = "") {
  return `<label class="field"><span class="lbl">${label}</span>${control}${hint ? `<span class="hint">${hint}</span>` : ""}</label>`;
}

function text(path: string, value: string, attrs = "") {
  return `<input type="text" data-path="${path}" value="${esc(value)}" spellcheck="false" ${attrs}/>`;
}

function num(path: string, value: number, min: number, max: number, step = 1) {
  return `<input type="number" data-path="${path}" value="${value}" min="${min}" max="${max}" step="${step}"/>`;
}

function check(path: string, value: boolean, label: string, hint = "") {
  return `<label class="check"><input type="checkbox" data-path="${path}" ${value ? "checked" : ""}/><span>${label}${hint ? `<small>${hint}</small>` : ""}</span></label>`;
}

function secret(key: string, saved: boolean, placeholder: string) {
  return `<span class="secret"><input type="password" data-secret="${key}" placeholder="${saved ? "saved — type to replace" : placeholder}" autocomplete="off"/>${saved ? `<button type="button" class="ghost" data-clear="${key}">remove</button>` : ""}</span>`;
}

function render() {
  const tokenUrl = `https://trello.com/1/authorize?expiration=never&scope=read,write&response_type=token&name=Dos%20Live&key=`;
  root.innerHTML = `
  <header class="top">
    <h1><b>dos</b> live</h1>
    <p class="dim">Everything here is stored on this PC. Keys live in Windows Credential Manager.</p>
  </header>

  <section>
    <h2>Trello</h2>
    <p class="lead">Dos reads your Dos, Don and Boom boards, and writes back only when you answer a question or save a note.</p>
    <ol class="steps">
      <li>Open <button type="button" class="link" data-open="https://trello.com/power-ups/admin">trello.com/power-ups/admin</button>, create a Power-Up for your workspace, then copy its <em>API key</em>.</li>
      <li>Paste the key below, then <button type="button" class="link" data-token="${tokenUrl}">get a token</button> (read and write, never expires) and paste it too.</li>
    </ol>
    ${field("API key", secret("trello-key", keys.trelloKey, "32 characters"))}
    ${field("Token", secret("trello-token", keys.trelloToken, "starts with ATTA…"))}
    <div class="row">
      <button type="button" class="btn go" data-run="testTrello">Test connection</button>
      <button type="button" class="btn" data-run="setupLists">Create the “Dos asks” lists</button>
    </div>
    <p class="result" data-result="trello"></p>
    <div class="grid3">
      ${field("Dos board", text("boards.dos", s.boards.dos))}
      ${field("Don board", text("boards.don", s.boards.don))}
      ${field("Boom board", text("boards.boom", s.boards.boom))}
    </div>
    <p class="hint">The short code after <code>/b/</code> in each board's address.</p>
  </section>

  <section>
    <h2>Voice</h2>
    ${field(
      "Listening",
      `<select data-path="voice.mode">
        <option value="wake" ${s.voice.mode === "wake" ? "selected" : ""}>Always listening for “${esc(s.voice.wakeWord)}”</option>
        <option value="hotkey" ${s.voice.mode === "hotkey" ? "selected" : ""}>Only when I press the hotkey</option>
        <option value="off" ${s.voice.mode === "off" ? "selected" : ""}>Off</option>
      </select>`,
      "Wake-word listening matches a short list of phrases on this PC. Free sentences (notes, questions) use Windows dictation, which needs Online speech recognition on in Settings → Privacy & security → Speech.",
    )}
    <div class="grid2">
      ${field("Wake word", text("voice.wakeWord", s.voice.wakeWord))}
      ${field("Push-to-talk", text("voice.hotkey", s.voice.hotkey), "e.g. Ctrl+Alt+D")}
    </div>
    <div class="grid2">
      ${field("Dos's voice", `<select data-path="voice.voiceName"><option value="">Best male English voice</option></select>`, "Install more under Settings → Time & language → Speech.")}
      ${field("Speed", `<input type="range" data-path="voice.rate" min="0.7" max="1.5" step="0.05" value="${s.voice.rate}"/>`)}
    </div>
    <div class="row">
      <button type="button" class="btn" data-run="hello">Hear Dos</button>
    </div>
    ${check("voice.speakPings", s.voice.speakPings, "Read new pings out loud")}
    ${check("voice.speakAsks", s.voice.speakAsks, "Read questions out loud")}
    ${check("callGuard", s.callGuard, "Go quiet during calls", "When Teams, Meet, Zoom or WhatsApp is using the microphone, Dos stops talking and stops listening.")}
  </section>

  <section>
    <h2>Hermes</h2>
    <p class="lead">Questions that aren't about the boards go to your local Hermes agent. Anything about work is answered from the Work pill and never sent.</p>
    ${check("hermes.enabled", s.hermes.enabled, "Send other questions to Hermes")}
    <div class="grid2">
      ${field("API server", text("hermes.url", s.hermes.url), "Must be on this PC (127.0.0.1).")}
      ${field("Model", text("hermes.model", s.hermes.model))}
    </div>
    ${field("API key", secret("hermes-key", keys.hermesKey, "API_SERVER_KEY from ~/.hermes"))}
    <div class="row"><button type="button" class="btn" data-run="testHermes">Test Hermes</button></div>
    <p class="result" data-result="hermes"></p>
    <p class="hint">Hermes talks back with <code>dosctl say …</code> and <code>dosctl ask …</code> — see the README.</p>
    <div class="row"><button type="button" class="btn" data-run="testRelay">Test the relay</button></div>
    <p class="result" data-result="relay"></p>
  </section>

  <section>
    <h2>On screen</h2>
    ${check("ui.stealth", s.ui.stealth, "Hide from screen sharing and recordings", "You still see it. Teams, Meet, Zoom and OBS don't.")}
    ${check("ui.discreetWork", s.ui.discreetWork, "Keep the Work pill discreet", "Counts only, no ticket titles on screen.")}
    ${check("ui.expandOnAlert", s.ui.expandOnAlert, "Drop open when a ping or question arrives")}
    ${check("ui.sound", s.ui.sound, "Sound cues")}
    ${check("ui.autostart", s.ui.autostart, "Start with Windows")}
    <div class="grid3">
      ${field("Volume", `<input type="range" data-path="ui.volume" min="0" max="1" step="0.05" value="${s.ui.volume}"/>`)}
      ${field("Alert stays", num("ui.collapseAfterSecs", s.ui.collapseAfterSecs, 4, 120), "seconds")}
      ${field(
        "Screen",
        `<select data-path="ui.screen"><option value="primary" ${s.ui.screen === "primary" ? "selected" : ""}>Main display</option><option value="cursor" ${s.ui.screen === "cursor" ? "selected" : ""}>Where the mouse is</option></select>`,
      )}
    </div>
  </section>

  <section>
    <h2>Rhythm</h2>
    <div class="grid3">
      ${field("Check boards every", num("pollSecs", s.pollSecs, 20, 900), "seconds")}
      ${field("Stale after", num("staleDays", s.staleDays, 1, 365), "days quiet")}
      ${field("Due soon", num("dueSoonDays", s.dueSoonDays, 0, 30), "days ahead")}
    </div>
    <p class="hint">Worlds, labels and keywords live in <code>%APPDATA%\\Dos Live\\settings.json</code>. Edit that file to re-map a pill, then restart Dos Live.</p>
  </section>

  <footer class="bottom"><span class="saved dim"></span><button type="button" class="ghost" data-run="quit">Quit Dos Live</button></footer>`;
  void Bridge.listVoices();
}

function setPath(obj: Record<string, unknown>, path: string, value: unknown) {
  const parts = path.split(".");
  let cur = obj;
  for (const p of parts.slice(0, -1)) cur = cur[p] as Record<string, unknown>;
  cur[parts[parts.length - 1]] = value;
}

function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    const el = root.querySelector(".saved")!;
    try {
      await Bridge.saveSettings(s);
      el.textContent = "saved";
    } catch (e) {
      el.textContent = `couldn't save: ${String(e)}`;
    }
    setTimeout(() => (el.textContent = ""), 1800);
  }, 350);
}

root.addEventListener("change", async (e) => {
  const t = e.target as HTMLInputElement;
  if (t.dataset.path) {
    let v: unknown = t.value;
    if (t.type === "checkbox") v = t.checked;
    else if (t.type === "number" || t.type === "range") v = Number(t.value);
    setPath(s as unknown as Record<string, unknown>, t.dataset.path, v);
    scheduleSave();
  } else if (t.dataset.secret && t.value.trim()) {
    try {
      await Bridge.secretSet(t.dataset.secret, t.value.trim());
      t.value = "";
      keys = await Bridge.keyStatus();
      render();
    } catch (err) {
      alert(String(err));
    }
  }
});

function result(name: string, ok: boolean, msg: string) {
  const el = root.querySelector<HTMLElement>(`[data-result="${name}"]`);
  if (!el) return;
  el.textContent = msg;
  el.className = `result ${ok ? "ok" : "bad"}`;
}

root.addEventListener("click", async (e) => {
  const t = (e.target as HTMLElement).closest<HTMLElement>("[data-run],[data-open],[data-token],[data-clear]");
  if (!t) return;
  if (t.dataset.open) return void Bridge.openLink(t.dataset.open);
  if (t.dataset.token !== undefined) {
    const key = (root.querySelector<HTMLInputElement>('[data-secret="trello-key"]')?.value || "").trim();
    if (!key && !keys.trelloKey) return result("trello", false, "Paste the API key first — the token link needs it.");
    if (!key) return result("trello", false, "For safety the saved key isn't shown. Paste it again above, then click “get a token”.");
    return void Bridge.openLink(t.dataset.token + encodeURIComponent(key));
  }
  if (t.dataset.clear) {
    await Bridge.secretClear(t.dataset.clear);
    keys = await Bridge.keyStatus();
    return render();
  }
  const run = t.dataset.run;
  try {
    switch (run) {
      case "testTrello":
        result("trello", true, "checking…");
        result("trello", true, await Bridge.testTrello());
        break;
      case "setupLists":
        result("trello", true, await Bridge.setupLists());
        break;
      case "testHermes":
        result("hermes", true, "asking…");
        result("hermes", true, `Hermes says: ${await Bridge.testHermes()}`);
        break;
      case "testRelay":
        result("relay", true, `The pipe answered: ${await Bridge.testRelay()}`);
        break;
      case "hello":
        await Bridge.say("Dos here. I'm watching the boards.");
        break;
      case "quit":
        await Bridge.quit();
        break;
    }
  } catch (err) {
    const name = run === "testHermes" ? "hermes" : run === "testRelay" ? "relay" : "trello";
    result(name, false, String(err));
  }
});

async function boot() {
  let info: Awaited<ReturnType<typeof Bridge.boot>> | null = null;
  while (!info) {
    try {
      info = await Bridge.boot();
    } catch {
      await new Promise((r) => setTimeout(r, 300));
    }
  }
  s = info.settings;
  keys = info.keys;
  render();
  void Bridge.onSettings((next) => {
    s = next;
  });
  void Bridge.onVoices((voices) => {
    const sel = root.querySelector<HTMLSelectElement>('[data-path="voice.voiceName"]');
    if (!sel) return;
    const sorted = [...voices].sort((a, b) => Number(b.male) - Number(a.male) || a.language.localeCompare(b.language));
    sel.innerHTML =
      `<option value="">Best male English voice</option>` +
      sorted
        .map((v) => `<option value="${esc(v.name)}" ${s.voice.voiceName === v.name ? "selected" : ""}>${esc(v.name)} — ${esc(v.language)}${v.male ? "" : " (female)"}</option>`)
        .join("");
  });
}

void boot();
