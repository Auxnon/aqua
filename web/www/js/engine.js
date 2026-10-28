// The Rust engine (../pkg, built by wasm-pack from web/src/lib.rs) with frame
// scheduling. Android runs the engine on its own thread; here frames come from
// requestAnimationFrame, and any call that changes the picture asks for one.

import init, { AquaEngine, AquaPalette, pigment_from_rgb, pigment_to_rgb, preset_pigments } from '../pkg/aqua_web.js';

export { AquaPalette };

export async function loadWasm() {
  await init();
}

export async function startEngine(canvas) {
  if (!navigator.gpu) throw new Error('This browser has no WebGPU.');
  try {
    return new Engine(await AquaEngine.create(canvas));
  } catch (e) {
    // The first adapter request can come back empty while the browser's GPU
    // process is still starting; one more try settles it.
    await new Promise((r) => setTimeout(r, 300));
    return new Engine(await AquaEngine.create(canvas));
  }
}

export class Engine {
  constructor(raw) {
    this.raw = raw;
    this.raf = 0;
    this.timer = 0;
  }

  /** Makes sure a frame is coming. Input cuts a drying-rate wait short. */
  kick() {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = 0;
    }
    if (!this.raf) this.raf = requestAnimationFrame(() => this.tick());
  }

  tick() {
    this.raf = 0;
    if (!this.raw.frame()) return;
    // While paint is only drying, frames are spaced out to save power.
    const wait = this.raw.frame_delay_ms();
    if (wait > 1) this.timer = setTimeout(() => {
      this.timer = 0;
      this.kick();
    }, wait);
    else this.raf = requestAnimationFrame(() => this.tick());
  }

  resize(w, h) { this.raw.resize(w, h); this.kick(); }
  setView(v) { this.raw.set_view(v); this.kick(); }
  setTilt(x, y) { this.raw.set_tilt(x, y); this.kick(); }
  setBrush(tool, params) { this.raw.set_brush(tool, params); }
  stroke(phase, pts) { this.raw.stroke(phase, pts); this.kick(); }
  newCanvas(w, h, paper, seed) { this.raw.new_canvas(w, h, paper, seed); this.kick(); }
  load(files) { this.raw.load(files); this.kick(); }
  save() { return this.raw.save(); }
  exportPng() { return this.raw.export_png(); }
  sample(x, y) { return this.raw.sample(x, y); }
  layerOp(op, index, value = 0, name = undefined) { this.raw.layer_op(op, index, value, name); this.kick(); }
  undo() { this.raw.undo(); this.kick(); }
  redo() { this.raw.redo(); this.kick(); }
  query() {
    try {
      return JSON.parse(this.raw.query());
    } catch {
      return {};
    }
  }
  close() { this.raw.close(); this.kick(); }
}

export const Tool = { Watercolor: 0, Water: 1, Ink: 2, Eraser: 3, Eyedropper: -1 };

export const LayerOp = { ADD: 0, REMOVE: 1, MOVE: 2, SELECT: 3, OPACITY: 4, VISIBLE: 5, RENAME: 6, DRY: 7, CLEAR: 8, DRYING_SPEED: 9, FLOW: 10 };

/** A pigment: Kubelka-Munk absorption (rgb) and scattering at unit concentration. */
export const Pigment = {
  fromRgb: (r, g, b) => Float32Array.from(pigment_from_rgb(r, g, b)),
  fromHex(hex) {
    const n = parseInt(hex.replace('#', ''), 16);
    return Pigment.fromRgb(((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255);
  },
  css(p) {
    const [r, g, b] = pigment_to_rgb(p);
    const c = (v) => Math.round(Math.min(1, Math.max(0, v)) * 255);
    return `rgb(${c(r)}, ${c(g)}, ${c(b)})`;
  },
  rgb: (p) => Array.from(pigment_to_rgb(p)),
  same: (a, b) => !!a && !!b && a.length === b.length && a.every((v, i) => v === b[i]),
};

export function presetPigments() {
  return preset_pigments().map(([name, rgb]) => ({ name, pigment: Pigment.fromRgb(rgb[0], rgb[1], rgb[2]) }));
}

/** Brush settings per tool, same defaults and array layout as the Android app. */
export function brushParams(size, o = {}) {
  return {
    size,
    load: 0.6,
    water: 0.6,
    softness: 0.18,
    bristle: 0.5,
    bleed: 0,
    strength: 0.8,
    pressureSize: 0.6,
    pressureFlow: 0.5,
    smoothing: 0.3,
    splatter: 0,
    ...o,
  };
}

export function brushArray(b, p) {
  return Float32Array.from([
    b.size, p[0], p[1], p[2], p[3], b.load, b.water, b.softness, b.bristle, b.bleed, b.strength,
    b.pressureSize, b.pressureFlow, b.smoothing, b.splatter,
  ]);
}
