// The painting screen, laid out like the Android CanvasScreen: navigation and
// history top left, actions top right, size and wetness on the left rail, tools
// in the bottom dock, panels floating over the canvas.

import { Pigment, Tool, brushArray, brushParams, presetPigments, LayerOp } from './engine.js';
import { CanvasView } from './canvasview.js';
import { ACCENT, PhoneTilt, TEXT, drawGimbal, gimbalOverlay, gimbalSvg } from './gimbal.js';
import { I } from './icons.js';
import { brushPanel, layersPanel, palettePanel } from './panels.js';
import { writePainting } from './store.js';
import { h, railSlider, roundButton, setSelected } from './ui.js';

const MAX_BRUSH = 300;
const sizeToRail = (s) => Math.log(Math.min(MAX_BRUSH, Math.max(1, s))) / Math.log(MAX_BRUSH);
const railToSize = (v) => Math.min(MAX_BRUSH, Math.max(1, MAX_BRUSH ** v));

/** Saves this often while painting, as well as when leaving or hiding the tab. */
const AUTOSAVE_MS = 30_000;

export function createEditor(root, engine, canvas, { onExit }) {
  // ---------------------------------------------------------------- state
  let id = null;
  let info = {};
  let tool = Tool.Watercolor;
  let brushTool = Tool.Watercolor;
  const params = {
    [Tool.Watercolor]: brushParams(28),
    [Tool.Water]: brushParams(40, { water: 0.8, softness: 0.3 }),
    [Tool.Ink]: brushParams(3, { load: 0.8, softness: 0.05, bristle: 0, pressureSize: 0.8, smoothing: 0.5, splatter: 0.35 }),
    [Tool.Eraser]: brushParams(40, { softness: 0.4, bristle: 0 }),
  };
  let paint = presetPigments()[4].pigment;
  let ink = Pigment.fromHex('#111114');
  let panel = null; // 'layers' | 'palette' | 'brush'
  let exiting = false;
  let tilt = [0, 0];
  let gimbalOpen = false;
  let keepTilt = false;
  let savedRevision = -1;
  let saving = null;

  const color = () => (brushTool === Tool.Ink ? ink : paint);
  const bp = () => params[brushTool];

  // ---------------------------------------------------------------- view
  const view = new CanvasView(canvas, engine, {
    onStrokeEnd: () => refresh(),
    onUndo: () => { engine.undo(); refresh(); },
    onRedo: () => { engine.redo(); refresh(); },
    onSample: (rgb) => setColor(Pigment.fromRgb(rgb[0], rgb[1], rgb[2])),
    onTouchStart: () => setPanel(null),
    onOverviewHint: (on) => root.classList.toggle('overview-hint', on),
    onOverview: (frame) => leave(frame),
    onTiltTurned: (t) => {
      setTilt(t);
      // Phone tilt is read in screen axes, which just turned; start again from here.
      phone.gyro.start(t);
    },
  });

  const phone = new PhoneTilt((t) => setTilt(t));

  // ---------------------------------------------------------------- chrome
  const back = roundButton(I.back, 'Gallery', () => leave());
  const undo = roundButton(I.undo, 'Undo (Ctrl+Z)', () => { engine.undo(); refresh(); });
  const redo = roundButton(I.redo, 'Redo (Ctrl+Shift+Z)', () => { engine.redo(); refresh(); });
  const dry = roundButton(I.dry, 'Dry layer now', () => { engine.layerOp(LayerOp.DRY, info.active ?? 0); refresh(); }, { cls: 'fade' });
  const gimbalIcon = gimbalSvg(22);
  const gimbalButton = h('button', { class: 'round', title: 'Tip the paper', 'aria-label': 'Tip the paper', onclick: (e) => { e.stopPropagation(); toggleGimbal(); } }, gimbalIcon);
  const exportB = roundButton(I.share, 'Export PNG', () => exportPng());
  const layersB = roundButton(I.layers, 'Layers', () => setPanel(panel === 'layers' ? null : 'layers'));
  const swatch = h('button', { class: 'swatch top', title: 'Palette', onclick: (e) => { e.stopPropagation(); setPanel(panel === 'palette' ? null : 'palette'); } });

  const topbar = h('div', { class: 'topbar' },
    h('div', { class: 'bar' }, back, undo, redo),
    h('div', { class: 'grow' }),
    h('div', { class: 'bar' }, dry, gimbalButton, exportB, layersB, swatch),
  );

  const sizeRail = railSlider('');
  const mainRail = railSlider('', 120);
  const rail = h('div', { class: 'rail' }, sizeRail.el, h('div', { style: { height: '14px' } }), mainRail.el);
  rail.addEventListener('pointerdown', (e) => e.stopPropagation());

  const tools = [
    [Tool.Watercolor, I.brush, 'Watercolour (B)'],
    [Tool.Water, I.drop, 'Clear water (W)'],
    [Tool.Ink, I.quill, 'Ink (I)'],
    [Tool.Eraser, I.eraser, 'Eraser (E)'],
    [Tool.Eyedropper, I.dropper, 'Eyedropper (K)'],
  ].map(([t, ic, label]) => ({ t, b: roundButton(ic, label, () => selectTool(t), { size: 48 }) }));
  const dock = h('div', { class: 'dock' }, tools.map((x) => x.b));
  dock.addEventListener('pointerdown', (e) => e.stopPropagation());

  const panelHost = h('div', { class: 'panel-host' });
  const layers = layersPanel(engine, () => refresh());
  const chrome = h('div', { class: 'chrome' }, topbar, rail, dock, panelHost);
  const hint = h('div', { class: 'pill hint' }, 'Release for gallery');
  const gimbalHost = h('div', { class: 'gimbal-host' });
  root.append(chrome, hint, gimbalHost);

  // ---------------------------------------------------------------- behaviour
  function pushBrush() {
    engine.setBrush(brushTool, brushArray(bp(), color()));
  }

  function setParams(p) {
    params[brushTool] = p;
    pushBrush();
    render();
  }

  function selectTool(t) {
    if (t === tool && t !== Tool.Eyedropper) {
      setPanel(panel === 'brush' ? null : 'brush');
      return;
    }
    tool = t;
    if (t !== Tool.Eyedropper) brushTool = t;
    if (panel === 'brush') setPanel(t === Tool.Eyedropper ? null : 'brush', true);
    view.eyedropper = t === Tool.Eyedropper;
    pushBrush();
    render();
  }

  function setColor(p) {
    if (brushTool === Tool.Ink) ink = p;
    else paint = p;
    if (brushTool === Tool.Eraser || brushTool === Tool.Water) {
      paint = p;
      brushTool = Tool.Watercolor;
      tool = Tool.Watercolor;
      view.eyedropper = false;
    }
    pushBrush();
    render();
  }

  function setPanel(p, force = false) {
    if (p === panel && !force) return;
    panel = p;
    panelHost.replaceChildren();
    if (p === 'layers') {
      layers.update(info);
      panelHost.append(layers.el);
    } else if (p === 'palette') panelHost.append(palettePanel(color(), (pg) => setColor(pg)));
    else if (p === 'brush') panelHost.append(brushPanel(brushTool, bp(), setParams));
    panelHost.className = `panel-host ${p ?? ''}`;
    render();
  }

  function setTilt(t) {
    tilt = t;
    view.setTilt(t[0], t[1]);
    gimbal?.refresh();
    drawGimbalIcon();
  }

  /** The top-bar button is a little gimbal whose dot shows which way the paper is tipped. */
  function drawGimbalIcon() {
    const live = gimbalOpen || keepTilt || tilt[0] || tilt[1];
    drawGimbal(gimbalIcon, tilt, { color: live ? ACCENT : TEXT, stroke: 1.6, dot: 2.6, axes: false });
  }

  let gimbal = null;
  function syncPhoneTilt() {
    phone.setRunning((gimbalOpen || keepTilt) && !exiting && !document.hidden, tilt);
  }

  async function toggleGimbal() {
    gimbalOpen = !gimbalOpen;
    if (gimbalOpen) {
      setPanel(null);
      await PhoneTilt.permit();
      gimbal = gimbalOverlay({
        getTilt: () => tilt,
        setTilt,
        phone,
        keepOn: () => keepTilt,
        onKeepOn: (on) => { keepTilt = on; syncPhoneTilt(); render(); },
        onDismiss: () => toggleGimbal(),
      });
      gimbalHost.replaceChildren(gimbal.el);
    } else {
      gimbal = null;
      gimbalHost.replaceChildren();
    }
    syncPhoneTilt();
    render();
  }

  function refresh() {
    if (!id) return;
    info = engine.query();
    if (info.width) view.setCanvasSize(info.width, info.height, info.home ?? 0);
    if (panel === 'layers') layers.update(info);
    render();
  }

  function render() {
    const b = bp();
    undo.disabled = !info.canUndo;
    redo.disabled = !info.canRedo;
    dry.classList.toggle('shown', !!info.layers?.[info.active]?.wet);
    setSelected(layersB, panel === 'layers');
    setSelected(gimbalButton, gimbalOpen || keepTilt);
    swatch.style.setProperty('--c', Pigment.css(color()));
    setSelected(swatch, panel === 'palette');
    for (const { t, b: btn } of tools) setSelected(btn, t === tool);
    sizeRail.set(sizeToRail(b.size), `${Math.round(b.size)}`, (v) => setParams({ ...bp(), size: railToSize(v) }));
    const main = {
      [Tool.Watercolor]: ['water', 'water'],
      [Tool.Water]: ['water', 'water'],
      [Tool.Ink]: ['bleed', 'bleed'],
      [Tool.Eraser]: ['strength', 'strength'],
    }[brushTool];
    mainRail.set(b[main[0]], main[1], (v) => setParams({ ...bp(), [main[0]]: v }));
    root.classList.toggle('exiting', exiting);
    drawGimbalIcon();
  }

  async function save() {
    if (!id) return;
    // One save at a time; the latest state wins.
    while (saving) await saving;
    const target = id;
    const rev = engine.query().revision;
    saving = (async () => {
      try {
        const files = await engine.save();
        await writePainting(target, files);
        savedRevision = rev;
      } catch (e) {
        console.error('save failed', e);
      }
    })();
    await saving;
    saving = null;
  }

  async function exportPng() {
    try {
      const png = await engine.exportPng();
      const url = URL.createObjectURL(new Blob([png], { type: 'image/png' }));
      h('a', { href: url, download: `Aqua_${Date.now()}.png` }).click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (e) {
      console.error('export failed', e);
    }
  }

  /** Saves and goes back to the gallery, which flies the painting from `from` into its card. */
  async function leave(from = view.frame()) {
    if (exiting || !id) return;
    exiting = true;
    view.enabled = false;
    setPanel(null);
    if (gimbalOpen) toggleGimbal();
    syncPhoneTilt();
    render();
    await save();
    const leaving = id;
    await onExit(leaving, from);
    // Only once the gallery has taken over: closing earlier blanks the canvas while it's on screen.
    engine.close();
    id = null;
  }

  // Autosave while painting, and whenever the tab is hidden (it may never come back).
  setInterval(() => {
    if (id && !exiting && engine.query().revision !== savedRevision) save();
  }, AUTOSAVE_MS);
  document.addEventListener('visibilitychange', () => {
    syncPhoneTilt();
    if (document.hidden && id && !exiting) save();
  });

  // Keyboard, for desktop.
  window.addEventListener('keydown', (e) => {
    if (!id || exiting || e.target.closest?.('input')) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (mod && k === 'z') {
      e.preventDefault();
      if (e.shiftKey) engine.redo(); else engine.undo();
      refresh();
    } else if (mod && k === 'y') {
      e.preventDefault();
      engine.redo();
      refresh();
    } else if (mod || e.altKey) {
      return;
    } else if (k === ' ') {
      view.space = true;
    } else if (k === '[' || k === ']') {
      setParams({ ...bp(), size: railToSize(sizeToRail(bp().size) + (k === ']' ? 0.04 : -0.04)) });
    } else if (k === 'b') selectTool(Tool.Watercolor);
    else if (k === 'w') selectTool(Tool.Water);
    else if (k === 'i') selectTool(Tool.Ink);
    else if (k === 'e') selectTool(Tool.Eraser);
    else if (k === 'k') selectTool(Tool.Eyedropper);
    else if (k === '0') view.fit();
    else if (k === '=' || k === '+') view.zoomBy(1.25);
    else if (k === '-') view.zoomBy(0.8);
    else if (k === 'escape') {
      if (gimbalOpen) toggleGimbal();
      else if (panel) setPanel(null);
    }
  });
  window.addEventListener('keyup', (e) => {
    if (e.key === ' ') view.space = false;
  });

  setInterval(() => {
    if (id && !exiting) refresh();
  }, 500);

  return {
    /** Shows a painting the engine has just loaded or created. */
    open(paintingId) {
      id = paintingId;
      exiting = false;
      savedRevision = -1;
      info = {};
      view.fitted = false;
      view.canvasW = 0;
      view.enabled = true;
      tilt = [0, 0];
      view.tilt = [0, 0];
      keepTilt = false;
      gimbalOpen = false;
      gimbalHost.replaceChildren();
      setPanel(null);
      pushBrush();
      refresh();
    },
    save,
    leave,
  };
}
