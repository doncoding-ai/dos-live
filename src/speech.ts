// Plays Dos's voice (WAV from Windows' speech engine, sent by Rust) and reports
// its loudness so the visor moves with the words. One at a time, in order.

import { audio } from "./sound";
import type { Speech } from "./types";

type Level = (v: number) => void;

let queue: Speech[] = [];
let current: { src: AudioBufferSourceNode; id: number } | null = null;
let volume = 0.8;

export function setVoiceVolume(v: number) {
  volume = Math.max(0, Math.min(1, v));
}

function decode(b64: string): ArrayBuffer {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
}

export function enqueue(s: Speech, onLevel: Level, onStart: (s: Speech) => void, onEnd: (id: number) => void) {
  queue.push(s);
  if (!current) void playNext(onLevel, onStart, onEnd);
}

async function playNext(onLevel: Level, onStart: (s: Speech) => void, onEnd: (id: number) => void) {
  const next = queue.shift();
  if (!next) return;
  const a = audio();
  let buffer: AudioBuffer;
  try {
    buffer = await a.decodeAudioData(decode(next.wav));
  } catch {
    onEnd(next.id);
    return playNext(onLevel, onStart, onEnd);
  }
  const src = a.createBufferSource();
  src.buffer = buffer;
  const gain = a.createGain();
  gain.gain.value = volume;
  const analyser = a.createAnalyser();
  analyser.fftSize = 512;
  src.connect(gain).connect(analyser).connect(a.destination);
  current = { src, id: next.id };
  const data = new Uint8Array(analyser.fftSize);
  let raf = 0;
  const meter = () => {
    analyser.getByteTimeDomainData(data);
    let sum = 0;
    for (const v of data) sum += ((v - 128) / 128) ** 2;
    onLevel(Math.min(1, Math.sqrt(sum / data.length) * 4));
    raf = requestAnimationFrame(meter);
  };
  src.onended = () => {
    cancelAnimationFrame(raf);
    onLevel(0);
    current = null;
    onEnd(next.id);
    void playNext(onLevel, onStart, onEnd);
  };
  onStart(next);
  src.start();
  meter();
}

export function stopAll() {
  queue = [];
  if (current) {
    try {
      current.src.stop();
    } catch {
      /* already stopped */
    }
  }
}

export function isPlaying() {
  return current !== null;
}
