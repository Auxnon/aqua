// The floating panels: brush settings, layers and the mixing palette. Same
// contents as the Android BrushPanel, LayersPanel and PalettePanel.

import { AquaPalette, LayerOp, Pigment, Tool, presetPigments } from './engine.js';
import { I } from './icons.js';
import { getValue, setValue } from './store.js';
import { clamp, h, icon, labeledSlider, roundButton } from './ui.js';

function panel(cls, ...children) {
  const el = h('div', { class: `panel ${cls}` }, ...children);
  // Touches on a panel never reach the canvas.
  el.addEventListener('pointerdown', (e) => e.stopPropagation());
  return el;
}

const pct = (v) => `${Math.round(v * 100)}%`;

// ------------------------------------------------------------------ brush

const TITLES = { [Tool.Watercolor]: 'Watercolour brush', [Tool.Water]: 'Clear water', [Tool.Ink]: 'Ink', [Tool.Eraser]: 'Eraser' };

export function brushPanel(tool, params, onChange) {
  const p = { ...params };
  const s = (label, key, opts) => labeledSlider(label, p[key], (v) => {
    p[key] = v;
    onChange({ ...p });
  }, opts);
  const soft = { min: 0.02, max: 1 };
  const rows = {
    [Tool.Watercolor]: () => [
      s('Water', 'water'), s('Pigment', 'load'), s('Edge softness', 'softness', soft), s('Bristles', 'bristle'),
      s('Pressure → size', 'pressureSize'), s('Pressure → flow', 'pressureFlow'), s('Smoothing', 'smoothing'),
    ],
    [Tool.Water]: () => [s('Water', 'water'), s('Edge softness', 'softness', soft), s('Pressure → size', 'pressureSize'), s('Smoothing', 'smoothing')],
    [Tool.Ink]: () => [
      s('Pen  ←  bleed  →  India ink', 'bleed', { format: (v) => (v < 0.02 ? 'pen' : pct(v)) }),
      s('Density', 'load'),
      s('Splatter (quick flicks)', 'splatter', { format: (v) => (v < 0.02 ? 'off' : pct(v)) }),
      s('Pressure → size', 'pressureSize'),
      s('Smoothing', 'smoothing'),
    ],
    [Tool.Eraser]: () => [s('Strength', 'strength'), s('Edge softness', 'softness', soft), s('Pressure → size', 'pressureSize')],
  };
  return panel('brush-panel', h('div', { class: 'panel-title' }, TITLES[tool]), rows[tool]?.() ?? []);
}

// ------------------------------------------------------------------ layers

// Drying speed runs on a log scale from 100x slower than normal (a wash stays
// workable for most of an hour) to 4x faster.
const MIN_DRYING = 0.01;
const MAX_DRYING = 4;
const sliderToDrying = (t) => MIN_DRYING * (MAX_DRYING / MIN_DRYING) ** t;
const dryingToSlider = (d) => Math.log(clamp(d, MIN_DRYING, MAX_DRYING) / MIN_DRYING) / Math.log(MAX_DRYING / MIN_DRYING);
const formatDrying = (d) => (d >= 0.995 ? `${d.toFixed(1)}x` : `1/${Math.round(1 / d)}`);

/** The layers panel; call `update(info)` whenever the engine's state changes. */
export function layersPanel(engine, onChanged) {
  const op = (o, i, v = 0, name) => {
    engine.layerOp(o, i, v, name);
    onChanged();
  };
  const add = roundButton(I.add, 'Add layer', () => op(LayerOp.ADD, info.active), { size: 36 });
  const list = h('div', { class: 'layer-list' });
  const props = h('div', { class: 'layer-props' });
  const el = panel('layers-panel', h('div', { class: 'panel-head' }, h('div', { class: 'panel-title' }, 'Layers'), add), list, props);
  let info = { layers: [], active: 0 };
  let propsFor = null;

  function update(next) {
    info = next;
    const n = info.layers?.length ?? 0;
    add.disabled = n >= 24;
    list.replaceChildren(
      // Top of the stack first, like every painting app.
      ...[...(info.layers ?? [])].map((l, i) => ({ l, i })).reverse().map(({ l, i }) => {
        const vis = roundButton(l.visible ? I.eye : I.eyeOff, 'Visibility', () => op(LayerOp.VISIBLE, i, l.visible ? 0 : 1), { size: 30, cls: l.visible ? '' : 'dim' });
        const wet = l.wet ? h('span', { class: 'wet', title: 'Wet', style: { opacity: 0.35 + clamp(l.wetness, 0, 1) * 0.65 } }, icon(I.drop, 16)) : null;
        return h('div', { class: `layer-row${i === info.active ? ' active' : ''}`, onclick: () => op(LayerOp.SELECT, i) }, h('span', { class: 'name' }, l.name), wet, vis);
      }),
    );
    const layer = info.layers?.[info.active];
    if (!layer) return;
    // Rebuilt only when another layer is selected, so a slider being dragged survives updates.
    if (propsFor !== layer.id) {
      propsFor = layer.id;
      props.replaceChildren(
        labeledSlider('Opacity', layer.opacity, (v) => op(LayerOp.OPACITY, info.active, v)),
        labeledSlider('Drying speed', dryingToSlider(layer.drying), (v) => op(LayerOp.DRYING_SPEED, info.active, sliderToDrying(v)), { format: (v) => formatDrying(sliderToDrying(v)) }),
        labeledSlider('Flood flow  calm → wild', layer.flow, (v) => op(LayerOp.FLOW, info.active, v), { format: (v) => (v < 0.02 ? 'calm' : pct(v)) }),
        h('div', { class: 'layer-actions' },
          (props.dry = roundButton(I.dry, 'Dry layer now', () => op(LayerOp.DRY, info.active))),
          (props.up = roundButton(I.up, 'Move up', () => op(LayerOp.MOVE, info.active, info.active + 1))),
          (props.down = roundButton(I.down, 'Move down', () => op(LayerOp.MOVE, info.active, info.active - 1))),
          roundButton(I.clearLayer, 'Clear layer', () => op(LayerOp.CLEAR, info.active)),
          (props.del = roundButton(I.trash, 'Delete layer', () => op(LayerOp.REMOVE, info.active))),
        ),
      );
    }
    props.dry.disabled = !layer.wet;
    props.up.disabled = info.active >= n - 1;
    props.down.disabled = info.active <= 0;
    props.del.disabled = n <= 1;
  }

  return { el, update };
}

// ------------------------------------------------------------------ palette

export const PALETTE_SIZE = 256;

/** The shared mixing tray, kept between sessions. */
export const tray = {
  palette: null,
  async load() {
    this.palette = new AquaPalette(PALETTE_SIZE, PALETTE_SIZE);
    const saved = await getValue('palette').catch(() => null);
    if (saved) this.palette.load(saved);
  },
  save() {
    setValue('palette', this.palette.to_bytes()).catch(() => {});
  },
};

async function loadSwatches() {
  const list = (await getValue('swatches').catch(() => null)) ?? [];
  return list.map((k) => Float32Array.from(k));
}

/**
 * A Paper-style mixing palette: dab pigments into the tray, swirl them together
 * with a finger, then pick the mixture up as the brush colour. The tray mixes in
 * Kubelka-Munk space, so what you mix is what the canvas will paint.
 */
export function palettePanel(current, onPick) {
  const pal = tray.palette;
  let mode = 'Paint';
  let well = current;
  let swatches = [];
  const presets = presetPigments();

  const tray_ = h('canvas', { class: 'tray', width: PALETTE_SIZE, height: PALETTE_SIZE });
  const ctx = tray_.getContext('2d');
  const image = ctx.createImageData(PALETTE_SIZE, PALETTE_SIZE);
  const redraw = () => {
    image.data.set(pal.render());
    ctx.putImageData(image, 0, 0);
  };

  const presetRow = h('div', { class: 'swatches' });
  const picker = h('div', { class: 'picker', hidden: true });
  const modes = h('div', { class: 'modes' });
  const mixed = h('div', { class: 'mixed' });

  const swatch = (p, size, selected, onclick) => h('button', { class: `swatch${selected ? ' selected' : ''}`, style: { width: `${size}px`, height: `${size}px`, '--c': Pigment.css(p) }, onclick });

  function render() {
    presetRow.replaceChildren(
      ...presets.map(({ name, pigment }) => {
        const b = swatch(pigment, 30, Pigment.same(pigment, well), () => {
          well = pigment;
          if (mode === 'Pick') mode = 'Paint';
          onPick(pigment);
          render();
        });
        b.title = name;
        return b;
      }),
      h('button', { class: 'swatch add', title: 'Custom colour', onclick: () => { picker.hidden = !picker.hidden; } }, icon(I.add, 18)),
    );
    modes.replaceChildren(
      ...['Paint', 'Mix', 'Pick'].map((m) => h('button', { class: `chip${m === mode ? ' on' : ''}`, onclick: () => { mode = m; render(); } }, m)),
      h('span', { class: 'grow' }),
      roundButton(I.trash, 'Clear tray', () => { pal.clear(); redraw(); tray.save(); }, { size: 36 }),
    );
    mixed.replaceChildren(
      ...(swatches.length ? [h('div', { class: 'caption' }, 'Mixed'), h('div', { class: 'swatches' }, swatches.map((p) => swatch(p, 26, Pigment.same(p, current), () => { well = p; current = p; onPick(p); render(); })))] : []),
    );
  }

  function pick(p) {
    current = p;
    onPick(p);
    swatches = [p, ...swatches.filter((s) => !Pigment.same(s, p))].slice(0, 16);
    setValue('swatches', swatches.map((s) => Array.from(s))).catch(() => {});
    render();
  }

  // The custom colour sliders (HSV).
  const hsv = { h: 0, s: 0.7, v: 0.6 };
  const dot = h('span', { class: 'swatch static' });
  const emit = () => {
    const [r, g, b] = hsvToRgb(hsv.h, hsv.s, hsv.v);
    dot.style.setProperty('--c', `rgb(${r * 255}, ${g * 255}, ${b * 255})`);
    well = Pigment.fromRgb(r, g, b);
    onPick(well);
  };
  picker.append(
    h('div', { class: 'picker-head' }, dot, h('span', { class: 'caption' }, 'Custom colour')),
    labeledSlider('Hue', hsv.h, (v) => { hsv.h = v; emit(); }, { min: 0, max: 360, format: (v) => `${Math.round(v)}°` }),
    labeledSlider('Saturation', hsv.s, (v) => { hsv.s = v; emit(); }),
    labeledSlider('Value', hsv.v, (v) => { hsv.v = v; emit(); }),
  );

  // Painting, mixing and picking in the tray.
  let last = null;
  const at = (e) => {
    const r = tray_.getBoundingClientRect();
    const k = PALETTE_SIZE / r.width;
    return [(e.clientX - r.left) * k, (e.clientY - r.top) * k];
  };
  tray_.addEventListener('pointerdown', (e) => {
    tray_.setPointerCapture(e.pointerId);
    const [x, y] = (last = at(e));
    if (mode === 'Paint') pal.dab(x, y, 10, well, 0.35);
    else if (mode === 'Mix') pal.smudge_begin(x, y, 14);
    else {
      const p = pal.pick(x, y, 4);
      if (p) onPick(Float32Array.from(p));
    }
    redraw();
  });
  tray_.addEventListener('pointermove', (e) => {
    if (!last) return;
    const [nx, ny] = at(e);
    const [px, py] = last;
    if (mode === 'Paint') {
      const steps = Math.max(1, Math.floor(Math.hypot(nx - px, ny - py) / 3));
      for (let s = 1; s <= steps; s++) {
        const t = s / steps;
        pal.dab(px + (nx - px) * t, py + (ny - py) * t, 10, well, 0.12);
      }
    } else if (mode === 'Mix') pal.smudge(px, py, nx, ny, 0.9);
    else {
      const p = pal.pick(nx, ny, 4);
      if (p) onPick(Float32Array.from(p));
    }
    last = [nx, ny];
    redraw();
  });
  const end = () => {
    if (!last) return;
    if (mode === 'Mix') pal.smudge_end();
    if (mode === 'Pick') {
      const p = pal.pick(last[0], last[1], 4);
      if (p) pick(Float32Array.from(p));
    }
    last = null;
    tray.save();
  };
  tray_.addEventListener('pointerup', end);
  tray_.addEventListener('pointercancel', end);

  loadSwatches().then((s) => {
    swatches = s;
    render();
  });
  render();
  redraw();
  return panel('palette-panel', h('div', { class: 'panel-title' }, 'Palette'), presetRow, picker, tray_, modes, mixed);
}

function hsvToRgb(hDeg, s, v) {
  const f = (n) => {
    const k = (n + hDeg / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [f(5), f(3), f(1)];
}
