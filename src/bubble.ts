// The bubble's body: a bouncy ball with a mind of its own. Gravity, lively
// walls, a squash on every impact and a hop when it settles. When it has a
// seat (the notch, its resting spot) a spring pulls it there with a little
// overshoot instead. It keeps a short trail of where it has been — the tail.

import type { Slot } from "./skins";

export interface TailBit {
  x: number;
  y: number;
  s: number;
  o: number;
}

export const TAIL = 22;

export class Bubble {
  x: number;
  y: number;
  vx = 4.6;
  vy = -7;
  size: number;
  sqx = 0;
  sqy = 0;
  t = 0;
  held = false;
  private hist: [number, number][] = [];

  constructor(x: number, y: number, size = 46) {
    this.x = x;
    this.y = y;
    this.size = size;
  }

  /** Let go with a playful kick (leaving a seat to roam). */
  kick() {
    this.vx = (Math.random() < 0.5 ? -1 : 1) * (4 + Math.random() * 3);
    this.vy = -(3 + Math.random() * 3);
  }

  /** Picked up by the mouse: follow it, remember the throw. */
  hold(x: number, y: number) {
    this.vx = (x - this.x) * 0.6 + this.vx * 0.4;
    this.vy = (y - this.y) * 0.6 + this.vy * 0.4;
    this.x = x;
    this.y = y;
    this.held = true;
    this.trail();
  }

  release() {
    this.held = false;
    this.vx = Math.max(-18, Math.min(18, this.vx));
    this.vy = Math.max(-18, Math.min(18, this.vy));
  }

  /** True while it is visibly moving (the caller can rest the animation loop otherwise). */
  get moving() {
    return Math.abs(this.vx) + Math.abs(this.vy) > 0.05 || this.sqx + this.sqy > 0.01;
  }

  step(seat: Slot | null, W: number, H: number, cursor: { x: number; y: number } | null) {
    if (this.held) return;
    this.size += ((seat ? seat.s : 46) - this.size) * 0.12;
    const r = this.size / 2;
    if (!seat) {
      this.vy += 0.34;
      this.vx *= 0.999;
      // Curious, not skittish: it slows down near the mouse so it can be caught.
      if (cursor && Math.hypot(cursor.x - this.x, cursor.y - this.y) < 120) {
        this.vx *= 0.9;
        this.vy *= 0.94;
      }
      this.x += this.vx;
      this.y += this.vy;
      if (this.x < r) {
        this.x = r;
        this.sqx = Math.min(1, Math.abs(this.vx) / 9);
        this.vx = Math.abs(this.vx) * 0.92;
      }
      if (this.x > W - r) {
        this.x = W - r;
        this.sqx = Math.min(1, Math.abs(this.vx) / 9);
        this.vx = -Math.abs(this.vx) * 0.92;
      }
      if (this.y < r) {
        this.y = r;
        this.sqy = Math.min(1, Math.abs(this.vy) / 12);
        this.vy = Math.abs(this.vy) * 0.9;
      }
      if (this.y > H - r) {
        this.y = H - r;
        this.sqy = Math.min(1, Math.abs(this.vy) / 12);
        this.vy = -Math.abs(this.vy) * 0.82;
        if (Math.abs(this.vy) < 3.4) {
          this.vy = -(8 + Math.random() * 6);
          this.vx += (Math.random() - 0.5) * 5;
        }
      }
      if (Math.abs(this.vx) < 1.6) this.vx = this.vx < 0 ? -1.6 : 1.6;
      this.vx = Math.max(-9, Math.min(9, this.vx));
    } else {
      this.vx += (seat.cx - this.x) * 0.085 - this.vx * 0.3;
      this.vy += (seat.cy - this.y) * 0.085 - this.vy * 0.3;
      this.x += this.vx;
      this.y += this.vy;
      if (Math.abs(seat.cx - this.x) < 0.2 && Math.abs(seat.cy - this.y) < 0.2 && Math.abs(this.vx) + Math.abs(this.vy) < 0.05) {
        this.x = seat.cx;
        this.y = seat.cy;
        this.vx = 0;
        this.vy = 0;
      }
    }
    this.sqx *= 0.8;
    this.sqy *= 0.8;
    this.trail();
  }

  private trail() {
    this.hist.push([this.x, this.y]);
    if (this.hist.length > TAIL + 2) this.hist.shift();
    this.t++;
  }

  /** Scale for squash-and-stretch on impact. */
  squash(): [number, number] {
    return [1 - 0.3 * this.sqx + 0.3 * this.sqy, 1 + 0.3 * this.sqx - 0.3 * this.sqy];
  }

  /** The wispy tail: shrinking, fading blobs along the recent path, with a wiggle. */
  tail(): TailBit[] {
    const sp = Math.hypot(this.vx, this.vy);
    const nx = sp > 0.5 ? -this.vy / sp : 0;
    const ny = sp > 0.5 ? this.vx / sp : 0;
    const out: TailBit[] = [];
    for (let i = 0; i < TAIL; i++) {
      const h = this.hist[Math.max(0, this.hist.length - 2 - i)] ?? [this.x, this.y];
      const s = Math.max(3, this.size * (0.82 - i * 0.034));
      const wig = Math.sin(this.t * 0.3 + i * 0.5) * i * 0.26;
      out.push({ x: h[0] + nx * wig - s / 2, y: h[1] + ny * wig - s / 2, s, o: 1 - i / TAIL });
    }
    return out;
  }
}
