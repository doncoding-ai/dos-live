// Dos, drawn in code: a dark monolith head with a glass visor. The visor is the
// face — eyes that follow your cursor, a scanning dot while it thinks, a live
// waveform while it speaks. A thin Kenyan-flag band sits at the collar.
// No images, no animation libraries: Canvas 2D, one renderer, two sizes.

export type Mood =
  | "idle"
  | "listening"
  | "thinking"
  | "speaking"
  | "alert"
  | "ask"
  | "quiet"
  | "error"
  | "happy"
  | "offline";

const MOOD_COLOR: Record<Mood, string> = {
  idle: "#3ddc84",
  listening: "#4aa8ff",
  thinking: "#4aa8ff",
  speaking: "#3ddc84",
  alert: "#ffb547",
  ask: "#ffb547",
  quiet: "#56636f",
  error: "#ff5d5d",
  happy: "#3ddc84",
  offline: "#56636f",
};

const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

function rgba(hex: string, a: number) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  g.beginPath();
  g.moveTo(x + r, y);
  g.arcTo(x + w, y, x + w, y + h, r);
  g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r);
  g.arcTo(x, y, x + w, y, r);
  g.closePath();
}

export class DosFace {
  private g: CanvasRenderingContext2D;
  private w = 0;
  private h = 0;
  private mood: Mood = "idle";
  private color = MOOD_COLOR.idle;
  private shownColor = MOOD_COLOR.idle;
  private look = { x: 0, y: 0 };
  private lookNow = { x: 0, y: 0 };
  private level = 0;
  private levelNow = 0;
  private squash = 0;
  private squashV = 0;
  private blinkAt = performance.now() + 2500;
  private raf = 0;
  private last = 0;
  private t0 = performance.now();

  constructor(private canvas: HTMLCanvasElement, private mini = false) {
    this.g = canvas.getContext("2d")!;
    this.resize();
  }

  resize() {
    const dpr = window.devicePixelRatio || 1;
    const r = this.canvas.getBoundingClientRect();
    this.w = Math.max(1, r.width);
    this.h = Math.max(1, r.height);
    this.canvas.width = Math.round(this.w * dpr);
    this.canvas.height = Math.round(this.h * dpr);
    this.g.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  setMood(m: Mood) {
    if (m === this.mood) return;
    this.mood = m;
    this.color = MOOD_COLOR[m];
    if (m === "alert" || m === "ask") this.poke(0.6);
  }

  /** Where to look, in canvas CSS pixels (can be far outside the canvas). */
  lookAt(x: number, y: number) {
    const cx = this.w / 2;
    const cy = this.h * 0.4;
    const dx = x - cx;
    const dy = y - cy;
    const d = Math.hypot(dx, dy) || 1;
    const reach = Math.min(1, d / 260);
    this.look = { x: (dx / d) * reach, y: (dy / d) * reach };
  }

  /** 0..1 loudness while speaking. */
  setLevel(v: number) {
    this.level = Math.max(0, Math.min(1, v));
  }

  poke(strength = 1) {
    this.squashV -= 0.9 * strength;
  }

  start() {
    const tick = (now: number) => {
      this.raf = requestAnimationFrame(tick);
      const busy = this.mood === "speaking" || this.mood === "listening" || this.mood === "thinking" || Math.abs(this.squash) > 0.002;
      const interval = reduceMotion ? 200 : busy ? 16 : this.mini ? 66 : 33;
      if (now - this.last < interval) return;
      const dt = Math.min(0.1, (now - this.last) / 1000 || 0.016);
      this.last = now;
      this.step(dt, now);
      this.draw(now);
    };
    this.raf = requestAnimationFrame(tick);
  }

  stop() {
    cancelAnimationFrame(this.raf);
  }

  private step(dt: number, now: number) {
    const k = 1 - Math.pow(0.001, dt);
    this.lookNow.x += (this.look.x - this.lookNow.x) * k;
    this.lookNow.y += (this.look.y - this.lookNow.y) * k;
    this.levelNow += (this.level - this.levelNow) * Math.min(1, dt * 18);
    // Spring for the squash.
    this.squashV += (-this.squash * 90 - this.squashV * 9) * dt;
    this.squash += this.squashV * dt;
    this.shownColor = this.color;
    if (now > this.blinkAt + 140) this.blinkAt = now + 2600 + Math.random() * 3800;
  }

  private draw(now: number) {
    const g = this.g;
    const { w, h } = this;
    g.clearRect(0, 0, w, h);
    const t = (now - this.t0) / 1000;
    const c = this.shownColor;
    const mini = this.mini;

    // The head occupies a 100 × 118 design box, scaled to fit.
    const s = Math.min(w / (mini ? 104 : 132), h / (mini ? 118 : 150));
    const breath = reduceMotion ? 0 : Math.sin(t * 1.6) * 0.8;
    const sq = this.squash;
    g.save();
    g.translate(w / 2, h / 2 + (mini ? 2 : 6) * s + breath * s * 0.4);
    g.scale(s * (1 - sq * 0.35), s * (1 + sq));

    // Listening ring.
    if (this.mood === "listening" && !reduceMotion) {
      for (let i = 0; i < 2; i++) {
        const p = ((t * 0.9 + i * 0.5) % 1);
        g.beginPath();
        g.arc(0, -6, 62 + p * 22, 0, Math.PI * 2);
        g.strokeStyle = rgba(c, 0.35 * (1 - p));
        g.lineWidth = 1.5;
        g.stroke();
      }
    }

    // Head: chamfered top, straight sides, tapering jaw.
    g.beginPath();
    g.moveTo(-34, -58);
    g.lineTo(34, -58);
    g.lineTo(50, -42);
    g.lineTo(50, 18);
    g.quadraticCurveTo(48, 40, 26, 54);
    g.lineTo(-26, 54);
    g.quadraticCurveTo(-48, 40, -50, 18);
    g.lineTo(-50, -42);
    g.closePath();
    const body = g.createLinearGradient(0, -58, 0, 54);
    body.addColorStop(0, "#16202a");
    body.addColorStop(1, "#070b10");
    g.fillStyle = body;
    g.fill();
    g.lineWidth = mini ? 3 : 1.5;
    g.strokeStyle = "#243241";
    g.stroke();

    // Rim light along the top edge, tinted by mood.
    g.beginPath();
    g.moveTo(-32, -57);
    g.lineTo(32, -57);
    g.strokeStyle = rgba(c, mini ? 0.7 : 0.55);
    g.lineWidth = mini ? 3 : 1.5;
    g.stroke();

    // Brows: attentive when something needs you, level otherwise.
    if (!mini) {
      const lift = this.mood === "alert" || this.mood === "ask" ? 1 : this.mood === "happy" ? -0.5 : 0;
      g.strokeStyle = lift ? rgba(c, 0.8) : "#2c3b4b";
      g.lineWidth = 3;
      g.lineCap = "round";
      for (const side of [-1, 1]) {
        g.beginPath();
        g.moveTo(side * 30, -35 + lift * 1);
        g.lineTo(side * 12, -35 - lift * 4);
        g.stroke();
      }
    }

    // Visor.
    const vx = -42, vy = -26, vw = 84, vh = 28;
    roundRect(g, vx, vy, vw, vh, 9);
    g.fillStyle = "#020508";
    g.fill();
    g.save();
    g.clip();
    const glow = g.createRadialGradient(0, vy + vh / 2, 2, 0, vy + vh / 2, 52);
    const pulse = this.mood === "idle" && !reduceMotion ? 0.12 + Math.sin(t * 1.6) * 0.04 : 0.2;
    glow.addColorStop(0, rgba(c, pulse));
    glow.addColorStop(1, rgba(c, 0));
    g.fillStyle = glow;
    g.fillRect(vx, vy, vw, vh);
    this.drawVisor(g, t, now, vx, vy, vw, vh, c);
    // Glass highlight.
    g.fillStyle = "rgba(255,255,255,0.05)";
    g.fillRect(vx, vy, vw, 5);
    g.restore();
    roundRect(g, vx, vy, vw, vh, 9);
    g.strokeStyle = rgba(c, 0.35);
    g.lineWidth = mini ? 2.5 : 1;
    g.stroke();

    if (!mini) {
      // Chin status light.
      g.beginPath();
      g.arc(0, 30, 2.2, 0, Math.PI * 2);
      g.fillStyle = rgba(c, this.mood === "idle" && !reduceMotion ? 0.5 + Math.sin(t * 2) * 0.3 : 0.9);
      g.fill();
      // Collar band: black, red, green with white fimbriation.
      const bw = 44, by = 42;
      const stripes: [string, number][] = [["#000000", 2], ["#e8e8e8", 0.7], ["#bb1e2d", 2], ["#e8e8e8", 0.7], ["#0f8a3c", 2]];
      let y = by;
      g.globalAlpha = 0.85;
      for (const [col, hgt] of stripes) {
        g.fillStyle = col;
        g.fillRect(-bw / 2, y, bw, hgt);
        y += hgt;
      }
      g.globalAlpha = 1;
    }
    g.restore();
  }

  private drawVisor(g: CanvasRenderingContext2D, t: number, now: number, vx: number, vy: number, vw: number, vh: number, c: string) {
    const cy = vy + vh / 2;
    const lx = this.lookNow.x * 8;
    const ly = this.lookNow.y * 3.5;
    g.shadowColor = c;
    g.shadowBlur = this.mini ? 4 : 10;
    g.fillStyle = c;
    g.strokeStyle = c;
    g.lineCap = "round";

    switch (this.mood) {
      case "speaking": {
        const bars = this.mini ? 7 : 15;
        const gap = vw / (bars + 1);
        const lvl = this.levelNow;
        for (let i = 0; i < bars; i++) {
          const x = vx + gap * (i + 1);
          const centre = 1 - Math.abs(i - (bars - 1) / 2) / ((bars - 1) / 2);
          const jitter = reduceMotion ? 0.5 : 0.55 + 0.45 * Math.sin(t * 17 + i * 1.7) * Math.sin(t * 7.3 + i);
          const hgt = Math.max(2, (4 + centre * 16) * (0.15 + lvl * 1.1) * jitter);
          g.fillRect(x - 1.2, cy - hgt / 2, 2.4, hgt);
        }
        break;
      }
      case "thinking": {
        const p = reduceMotion ? 0.5 : (Math.sin(t * 3.2) + 1) / 2;
        const x = vx + 10 + p * (vw - 20);
        for (let i = 0; i < 5; i++) {
          g.globalAlpha = 0.18 * (5 - i) / 5;
          const tx = x - Math.sign(Math.cos(t * 3.2)) * i * 4;
          g.beginPath();
          g.arc(tx, cy, 3.2, 0, Math.PI * 2);
          g.fill();
        }
        g.globalAlpha = 1;
        g.beginPath();
        g.arc(x, cy, 3.6, 0, Math.PI * 2);
        g.fill();
        break;
      }
      case "happy": {
        g.lineWidth = 3;
        for (const side of [-1, 1]) {
          g.beginPath();
          g.arc(side * 17 + lx, cy + 3 + ly, 6, Math.PI * 1.1, Math.PI * 1.9);
          g.stroke();
        }
        break;
      }
      case "quiet":
      case "offline": {
        g.shadowBlur = 0;
        g.lineWidth = 2.5;
        for (const side of [-1, 1]) {
          g.beginPath();
          g.moveTo(side * 17 - 7, cy);
          g.lineTo(side * 17 + 7, cy);
          g.stroke();
        }
        break;
      }
      default: {
        const blinking = now > this.blinkAt && now < this.blinkAt + 140;
        const listening = this.mood === "listening";
        const flicker = this.mood === "error" && !reduceMotion && Math.sin(t * 40) > 0.6;
        if (flicker) break;
        const ew = listening ? 9 : 15;
        let eh = listening ? 11 : this.mood === "alert" || this.mood === "ask" ? 8 : 6;
        if (blinking) eh = 1.5;
        for (const side of [-1, 1]) {
          roundRect(g, side * 17 - ew / 2 + lx, cy - eh / 2 + ly, ew, eh, Math.min(ew, eh) / 2);
          g.fill();
        }
      }
    }
    g.shadowBlur = 0;
  }
}
