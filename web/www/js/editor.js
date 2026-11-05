// The painting screen, laid out like the Android CanvasScreen: navigation and
// history top left, actions top right, size and wetness on the left rail, tools
// in the bottom dock, panels floating over the canvas.

import { Pigment, Tool, brushArray, brushParams, presetPigments, LayerOp } from './engine.js';
import { CanvasView } from './canvasview.js';
import { ACCENT, PhoneTilt, TEXT, drawGimbal, gimbalOverlay, gimbalSvg } from './gimbal.js';
import { I } from './icons.js';
import { brushPanel, layersPanel, palettePanel, pickerPanel } from './panels.js';
import { getValue, setValue, writePainting } from './store.js';
import { h, icon, railSlider, roundButton, setSelected } from './ui.js';

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
    [Tool.Ink]: brushParams(3, { load: 0.8, softness: 0.05, bristle: 0, pressureSize: 0.8, smoothing: 0.5, splatter: 0.35, tilt: 0.25 }),
    [Tool.Eraser]: brushParams(40, { softness: 0.4, bristle: 0 }),
    [Tool.Push]: brushParams(36, { softness: 0.4, bristle: 0 }),
  };
  // How the eyedropper reads the canvas: 'wash' (whiter = thinner paint) or 'classic'.
  let pickKind = 'wash';
  let penEraser = false;
  // The brush runs out of water and has to be dipped in the cup.
  let dip = false;
  getValue('dip').catch(() => null).then((v) => { if (v === true) setDip(true, false); });
  getValue('pickKind').catch(() => null).then((v) => { if (v === 'classic') { pickKind = v; view.pickWash = false; render(); } });
  let paint = presetPigments()[4].pigment;
  let ink = Pigment.fromHex('#111114');
  let panel = null; // 'layers' | 'palette' | 'brush' | 'picker'
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
    onSample: (p) => setColor(p),
    onTouchStart: () => setPanel(null),
    onPenEraser: (on) => {
      if (on === penEraser) return;
      penEraser = on;
      pushBrush();
      render();
    },
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
  const dipB = roundButton(I.endless, 'Brush always wet', () => setDip(!dip));
  const swatch = h('button', { class: 'swatch top', title: 'Palette', onclick: (e) => { e.stopPropagation(); setPanel(panel === 'palette' ? null : 'palette'); } });

  const topbar = h('div', { class: 'topbar' },
    h('div', { class: 'bar' }, back, undo, redo),
    h('div', { class: 'grow' }),
    h('div', { class: 'bar' }, dry, dipB, gimbalButton, exportB, layersB, swatch),
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
    [Tool.Push, I.push, 'Push (P)'],
    [Tool.Eyedropper, I.dropper, 'Eyedropper (K)'],
  ].map(([t, ic, label]) => ({ t, b: roundButton(ic, label, () => selectTool(t), { size: 48 }) }));
  const dock = h('div', { class: 'dock' }, tools.map((x) => x.b));
  dock.addEventListener('pointerdown', (e) => e.stopPropagation());

  // The cup: rim, water, a highlight, ripples from a dip, and round the rim what
  // the brush still holds.
  const LEVEL = 2 * Math.PI * 26.2;
  const cup = h('button', { class: 'cup', title: 'Water cup: dip the brush', 'aria-label': 'Water cup: dip the brush', hidden: true });
  cup.innerHTML = `<svg viewBox="0 0 60 60" width="60" height="60">
    <defs><radialGradient id="cupwater" cx="0.4" cy="0.4" r="0.65">
      <stop offset="0" stop-color="#9cc9e8" stop-opacity="0.85"/><stop offset="1" stop-color="#cfe6f5" stop-opacity="0.7"/>
    </radialGradient></defs>
    <radialGradient id="wellglass" cx="0.35" cy="0.35" r="0.75"><stop offset="0" stop-color="#5a6068"/><stop offset="1" stop-color="#2a2d33"/></radialGradient>
    <radialGradient id="wellink" cx="0.42" cy="0.42" r="0.6"><stop class="ink-hi" offset="0"/><stop class="ink-lo" offset="1"/></radialGradient>
    <circle cx="30" cy="30" r="30" fill="#1a1a1e"/>
    <g class="water">
      <circle cx="30" cy="30" r="25.8" fill="rgba(255,255,255,0.88)"/>
      <circle cx="30" cy="30" r="23.4" fill="url(#cupwater)"/>
      <circle cx="21" cy="20.4" r="4.8" fill="rgba(255,255,255,0.55)"/>
    </g>
    <g class="well">
      <circle cx="30" cy="30" r="25.8" fill="url(#wellglass)" stroke="rgba(255,255,255,0.18)" stroke-width="1.2"/>
      <circle cx="30" cy="30" r="18" fill="rgba(0,0,0,0.6)"/>
      <circle cx="30" cy="30" r="15.6" fill="url(#wellink)"/>
      <path d="M8.6 22.2a22 22 0 0 1 13.6-13.6" fill="none" stroke="rgba(255,255,255,0.35)" stroke-width="2.4" stroke-linecap="round"/>
      <circle cx="24" cy="23.4" r="2.7" fill="rgba(255,255,255,0.45)"/>
    </g>
    <circle class="ripple" cx="30" cy="30" r="23" fill="none" stroke="#fff" stroke-width="2"/>
    <circle class="ripple late" cx="30" cy="30" r="23" fill="none" stroke="#fff" stroke-width="2"/>
    <circle cx="30" cy="30" r="26.2" fill="none" stroke="rgba(154,152,148,0.25)" stroke-width="3.6"/>
    <circle class="level" cx="30" cy="30" r="26.2" fill="none" stroke="#8fd3cb" stroke-width="3.6" stroke-linecap="round"
      transform="rotate(-90 30 30)" stroke-dasharray="${LEVEL}" stroke-dashoffset="0"/>
  </svg>`;
  const level = cup.querySelector('.level');
  const inkHi = cup.querySelector('.ink-hi');
  const inkLo = cup.querySelector('.ink-lo');
  // On touch down, as a brush is dipped, not on release.
  cup.addEventListener('pointerdown', (e) => { e.stopPropagation(); e.preventDefault(); dipBrush(); });
  cup.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); dipBrush(); } });
  const toast = h('div', { class: 'pill hint toast' });
  let toastTimer = 0;

  const panelHost = h('div', { class: 'panel-host' });
  const layers = layersPanel(engine, () => refresh());
  const chrome = h('div', { class: 'chrome' }, topbar, rail, dock, cup, panelHost, toast);
  const hint = h('div', { class: 'pill hint' }, 'Release for gallery');
  const gimbalHost = h('div', { class: 'gimbal-host' });
  root.append(chrome, hint, gimbalHost);

  // ---------------------------------------------------------------- behaviour
  function pushBrush() {
    // While a pen is turned round to its eraser end, strokes erase.
    const t = penEraser ? Tool.Eraser : brushTool;
    engine.setBrush(t, brushArray(params[t], color()));
  }

  function setParams(p) {
    params[brushTool] = p;
    pushBrush();
    render();
  }

  function selectTool(t) {
    if (t === tool) {
      // Tapping the selected tool again opens its panel: brush settings, or
      // for the eyedropper which kind of picker it is.
      const own = t === Tool.Eyedropper ? 'picker' : 'brush';
      setPanel(panel === own ? null : own);
      return;
    }
    tool = t;
    if (t !== Tool.Eyedropper) brushTool = t;
    if (panel === 'brush' || panel === 'picker') setPanel(t === Tool.Eyedropper ? null : 'brush', true);
    view.eyedropper = t === Tool.Eyedropper;
    pushBrush();
    render();
  }

  function say(text) {
    toast.textContent = text;
    toast.classList.add('shown');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.classList.remove('shown'), 1600);
  }

  /** Switches between brushes that are always wet and brushes that run dry and are dipped. */
  function setDip(on, tell = true) {
    dip = on;
    // A fresh brush (and pen) starts full.
    engine.setCharge(Tool.Watercolor, on ? 1 : -1);
    info = { ...info, charge: on ? 1 : null, inkCharge: 1 };
    // Dipping is how the brush gets its water, and a dry brush pushes wet
    // paint by itself, so those two tools aren't needed.
    if (on && (tool === Tool.Water || tool === Tool.Push)) selectTool(Tool.Watercolor);
    if (tell) {
      setValue('dip', on).catch(() => {});
      say(on ? 'Brush runs dry: dip it in the cup' : 'Brush always wet');
    }
    render();
  }

  /** Dips the pen in the ink well, or any other brush in the water cup. */
  function dipBrush() {
    if (brushTool === Tool.Ink) {
      engine.setCharge(Tool.Ink, 1);
      info = { ...info, inkCharge: 1 };
    } else {
      engine.setCharge(Tool.Watercolor, 1);
      info = { ...info, charge: 1 };
    }
    cup.classList.remove('dipped');
    void cup.offsetWidth; // restart the ripple
    cup.classList.add('dipped');
    render();
  }

  function setColor(p) {
    if (brushTool === Tool.Ink) ink = p;
    else paint = p;
    if (brushTool === Tool.Eraser || brushTool === Tool.Water || brushTool === Tool.Push) {
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
    else if (p === 'picker') {
      panelHost.append(pickerPanel(pickKind, (k) => {
        pickKind = k;
        view.pickWash = k === 'wash';
        setValue('pickKind', k).catch(() => {});
        setPanel(null);
      }));
    }
    panelHost.className = `panel-host ${p === 'picker' ? 'brush' : p ?? ''}`;
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
    Pigment.paintSwatch(swatch, color());
    setSelected(swatch, panel === 'palette');
    for (const { t, b: btn } of tools) {
      setSelected(btn, t === tool || (t === Tool.Eraser && penEraser));
      btn.hidden = dip && (t === Tool.Water || t === Tool.Push);
    }
    if (dipB.dataset.dip !== String(dip)) {
      dipB.dataset.dip = String(dip);
      dipB.replaceChildren(icon(dip ? I.cup : I.endless, 22));
      const label = dip ? 'Brush runs dry: dip it in the cup' : 'Brush always wet';
      dipB.title = label;
      dipB.setAttribute('aria-label', label);
    }
    setSelected(dipB, dip);
    cup.hidden = !dip;
    // With the pen it is an ink well, of the pen's ink.
    const well = brushTool === Tool.Ink;
    cup.classList.toggle('ink', well);
    const cupLabel = well ? 'Ink well: dip the pen' : 'Water cup: dip the brush';
    if (cup.title !== cupLabel) { cup.title = cupLabel; cup.setAttribute('aria-label', cupLabel); }
    if (well) {
      const [r, g, b] = Pigment.rgb(Pigment.masstone(ink));
      const mix = (t, to) => `rgb(${[r, g, b].map((c) => Math.round((c + (to - c) * t) * 255)).join(' ')})`;
      inkHi.setAttribute('stop-color', mix(0.18, 1));
      inkLo.setAttribute('stop-color', mix(0.45, 0));
    }
    const charge = Math.min(1, Math.max(0, (well ? info.inkCharge : info.charge) ?? 1));
    level.style.strokeDashoffset = `${LEVEL * (1 - charge)}`;
    level.style.stroke = charge < 0.15 ? '#d08040' : '';
    level.style.opacity = charge < 0.005 ? '0' : '1';
    const dropper = tools.find((x) => x.t === Tool.Eyedropper).b;
    if (dropper.dataset.kind !== pickKind) {
      dropper.dataset.kind = pickKind;
      dropper.replaceChildren(icon(pickKind === 'wash' ? I.washPick : I.dropper, 24));
    }
    sizeRail.set(sizeToRail(b.size), `${Math.round(b.size)}`, (v) => setParams({ ...bp(), size: railToSize(v) }));
    const main = {
      [Tool.Watercolor]: ['water', 'water'],
      [Tool.Water]: ['water', 'water'],
      [Tool.Ink]: ['bleed', 'bleed'],
      [Tool.Eraser]: ['strength', 'strength'],
      [Tool.Push]: ['strength', 'strength'],
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
    else if (k === 'w' && !dip) selectTool(Tool.Water);
    else if (k === 'i') selectTool(Tool.Ink);
    else if (k === 'e') selectTool(Tool.Eraser);
    else if (k === 'p' && !dip) selectTool(Tool.Push);
    else if (k === 'd' && dip) dipBrush();
    else if (k === 'k') selectTool(Tool.Eyedropper);
    else if (k === '0') view.fit(true);
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
      // What pinching far out does, as set in the gallery's settings.
      getValue('pinchOut').catch(() => null).then((v) => {
        view.pinchOutToGallery = v === 'gallery';
        root.classList.toggle('pinch-gallery', view.pinchOutToGallery);
        hint.textContent = view.pinchOutToGallery ? 'Release for gallery' : 'Release to fit';
      });
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
