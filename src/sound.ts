// Dos's cues, synthesized — no sound files. Short, low, and quiet: a sonar
// ping for news, a double knock for a question, a rising blip when it listens.

let ctx: AudioContext | null = null;
let enabled = true;
let volume = 0.5;

export function audio(): AudioContext {
  ctx ??= new AudioContext();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

export function configureSound(on: boolean, vol: number) {
  enabled = on;
  volume = Math.max(0, Math.min(1, vol));
}

function tone(freq: number, start: number, dur: number, type: OscillatorType = "sine", gain = 0.2, glideTo?: number) {
  const a = audio();
  const t = a.currentTime + start;
  const osc = a.createOscillator();
  const g = a.createGain();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, t);
  if (glideTo) osc.frequency.exponentialRampToValueAtTime(glideTo, t + dur);
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(gain * volume, t + 0.012);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  osc.connect(g).connect(a.destination);
  osc.start(t);
  osc.stop(t + dur + 0.02);
}

export function cue(name: string) {
  if (!enabled) return;
  try {
    switch (name) {
      case "ping":
        tone(880, 0, 0.5, "sine", 0.18);
        tone(1320, 0.09, 0.6, "sine", 0.08);
        break;
      case "ask":
        tone(330, 0, 0.12, "triangle", 0.3);
        tone(330, 0.16, 0.12, "triangle", 0.3);
        tone(495, 0.34, 0.35, "sine", 0.14);
        break;
      case "wake":
        tone(520, 0, 0.16, "sine", 0.2, 880);
        break;
      case "sleep":
        tone(880, 0, 0.18, "sine", 0.14, 480);
        break;
      case "miss":
        tone(620, 0, 0.18, "sine", 0.14, 380);
        break;
      case "done":
        tone(660, 0, 0.18, "sine", 0.14);
        tone(990, 0.07, 0.24, "sine", 0.1);
        break;
      case "think":
        tone(1400, 0, 0.05, "square", 0.03);
        break;
      case "error":
        tone(220, 0, 0.25, "sawtooth", 0.06, 180);
        break;
    }
  } catch {
    /* audio unavailable — cues are optional */
  }
}
