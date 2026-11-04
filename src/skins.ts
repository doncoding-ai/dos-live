// The four looks of the island, as geometry. Every look shows the same scenes
// (idle, a ping, a question, the panel, listening, speaking); what differs is
// the shell's size and place for each scene and where Dos sits in it.
//
//   liquid   — one black (or white) shape that morphs, Dos's eyes inside it
//   terminal — the `dos:~$` bar that prints its drawers downward
//   presence — Dos as an orb; cards grow out from under it
//   bubble   — the orb roams the screen and docks into the liquid notch

import type { Skin } from "./types";

export type Scene = "idle" | "ping" | "ask" | "panel" | "listen" | "speak";

export interface Box {
  w: number;
  h: number;
  r: number;
  top: number;
  /** The shell is out of sight in this scene (Dos alone, or nothing). */
  hidden: boolean;
}

/** Where Dos's face sits: centre and diameter, in window pixels. */
export interface Slot {
  cx: number;
  cy: number;
  s: number;
}

export interface SkinDef {
  id: Skin;
  /** eyes: a capsule with two eyes · orb: the glossy sphere · inline: drawn inside the bar. */
  face: "eyes" | "orb" | "inline";
  /** `m` is the measured height of the scene's content, for scenes that size to it. */
  box(scene: Scene, m: number): Box;
  /** null: no moving face in this scene (inline faces, or the roaming bubble). */
  slot(scene: Scene, b: Box, left: number, winW: number): Slot | null;
  /** Panel width, so the header can leave room for the face. */
  panelW: number;
}

const PANEL_H = 452;

const box = (w: number, h: number, r: number, top: number, hidden = false): Box => ({ w, h, r, top, hidden });

const liquid: SkinDef = {
  id: "liquid",
  face: "eyes",
  panelW: 740,
  box(scene, m) {
    switch (scene) {
      case "idle": return box(252, 36, 18, 8);
      case "ping": return box(540, Math.max(96, m), 30, 8);
      case "ask": return box(560, Math.max(150, m), 30, 8);
      case "panel": return box(740, PANEL_H, 28, 8);
      case "listen": return box(300, 48, 24, 8);
      case "speak": return box(600, Math.max(64, m), 32, 8);
    }
  },
  slot(scene, b, L) {
    switch (scene) {
      case "idle": return { cx: L + 19, cy: b.top + 18, s: 26 };
      case "ping": return { cx: L + 44, cy: b.top + 48, s: 52 };
      case "ask": return { cx: L + 42, cy: b.top + 40, s: 40 };
      case "panel": return { cx: L + 33, cy: b.top + 30, s: 34 };
      case "listen": return { cx: L + 24, cy: b.top + 24, s: 32 };
      case "speak": return { cx: L + 32, cy: b.top + b.h / 2, s: 40 };
    }
  },
};

const terminal: SkinDef = {
  id: "terminal",
  face: "inline",
  panelW: 740,
  box(scene, m) {
    switch (scene) {
      case "idle": return box(500, 32, 16, 0);
      case "ping": return box(620, 32 + Math.max(64, m), 16, 0);
      case "ask": return box(640, 32 + Math.max(120, m), 16, 0);
      case "panel": return box(740, PANEL_H, 16, 0);
      case "listen": return box(500, 32, 16, 0);
      case "speak": return box(620, 32 + Math.max(56, m), 16, 0);
    }
  },
  slot: () => null,
};

const presence: SkinDef = {
  id: "presence",
  face: "orb",
  panelW: 740,
  box(scene, m) {
    switch (scene) {
      case "idle": return box(44, 44, 22, 20, true);
      case "ping": return box(480, Math.max(110, m), 22, 84);
      case "ask": return box(540, Math.max(150, m), 22, 90);
      case "panel": return box(740, PANEL_H, 24, 8);
      case "listen": return box(320, Math.max(64, m), 22, 132);
      case "speak": return box(560, Math.max(60, m), 22, 120);
    }
  },
  slot(scene, b, L, W) {
    switch (scene) {
      case "idle": return { cx: W / 2, cy: 42, s: 44 };
      case "ping": return { cx: W / 2, cy: 40, s: 44 };
      case "ask": return { cx: W / 2, cy: 40, s: 50 };
      case "panel": return { cx: L + 48, cy: b.top + 48, s: 52 };
      case "listen": return { cx: W / 2, cy: 66, s: 84 };
      case "speak": return { cx: W / 2, cy: 57, s: 70 };
    }
  },
};

const bubble: SkinDef = {
  id: "bubble",
  face: "orb",
  panelW: 740,
  box(scene, m) {
    switch (scene) {
      case "idle": return box(46, 46, 23, 8, true);
      case "ping": return box(540, Math.max(96, m), 30, 8);
      case "ask": return box(560, Math.max(150, m), 30, 8);
      case "panel": return box(740, PANEL_H, 28, 8);
      case "listen": return box(160, 38, 19, 122);
      case "speak": return box(600, Math.max(64, m), 32, 8);
    }
  },
  slot(scene, b, L, W) {
    switch (scene) {
      case "idle": return null;
      case "ping": return { cx: L + 44, cy: b.top + 48, s: 52 };
      case "ask": return { cx: L + 45, cy: b.top + 43, s: 46 };
      case "panel": return { cx: L + 34, cy: b.top + 30, s: 36 };
      case "listen": return { cx: W / 2, cy: 64, s: 78 };
      case "speak": return { cx: L + 32, cy: b.top + b.h / 2, s: 40 };
    }
  },
};

export const SKINS: Record<Skin, SkinDef> = { liquid, terminal, presence, bubble };

export function skinOf(name: string | undefined): SkinDef {
  return SKINS[(name ?? "liquid") as Skin] ?? liquid;
}
