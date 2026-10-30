// Tiny DOM helpers: no framework, just elements.

/** h('div', { class: 'x', onclick }, child, ...) */
export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k === 'style' && typeof v === 'object') {
      // Custom properties (--c) only go through setProperty.
      for (const [sk, sv] of Object.entries(v)) {
        if (sk.startsWith('--')) el.style.setProperty(sk, sv);
        else el.style[sk] = sv;
      }
    }
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'html') el.innerHTML = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  append(el, children);
  return el;
}

function append(el, children) {
  for (const c of children.flat(Infinity)) {
    if (c == null || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

/** An SVG icon from a path list (24x24, stroked like the Android outlined icons). */
export function icon(paths, size = 22) {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('width', size);
  svg.setAttribute('height', size);
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('class', 'icon');
  for (const d of paths) {
    const p = document.createElementNS(ns, 'path');
    p.setAttribute('d', d);
    svg.append(p);
  }
  return svg;
}

/** A round icon button, like the Android RoundButton. */
export function roundButton(paths, label, onClick, { size = 44, cls = '' } = {}) {
  const b = h('button', { class: `round ${cls}`, title: label, 'aria-label': label, style: { width: `${size}px`, height: `${size}px` } }, icon(paths, Math.round(size * 0.5)));
  b.addEventListener('click', (e) => {
    e.stopPropagation();
    onClick(e);
  });
  return b;
}

export function setEnabled(el, on) {
  el.disabled = !on;
}

export function setSelected(el, on) {
  el.classList.toggle('selected', !!on);
}

/** A labelled horizontal slider with a value readout, like LabeledSlider. */
export function labeledSlider(label, value, onChange, { min = 0, max = 1, format = (v) => `${Math.round(v * 100)}%` } = {}) {
  const out = h('span', { class: 'value' }, format(value));
  const input = h('input', { type: 'range', min, max, step: (max - min) / 1000, value });
  input.addEventListener('input', () => {
    const v = parseFloat(input.value);
    out.textContent = format(v);
    onChange(v);
  });
  return h('label', { class: 'slider' }, h('div', { class: 'slider-head' }, h('span', {}, label), out), input);
}

/**
 * A slim vertical slider for the side rail. Drags are relative, so grabbing it
 * anywhere never makes the value jump.
 */
export function railSlider(label, height = 160) {
  const fill = h('div', { class: 'rail-fill' });
  const thumb = h('div', { class: 'rail-thumb' });
  const track = h('div', { class: 'rail-track', style: { height: `${height}px` } }, h('div', { class: 'rail-groove' }, fill), thumb);
  const text = h('div', { class: 'rail-label' }, label);
  const el = h('div', { class: 'rail-slider' }, track, text);
  let value = 0;
  let onChange = () => {};
  const draw = () => {
    fill.style.height = `${value * 100}%`;
    thumb.style.top = `calc(${(1 - value) * 100}% - 5px)`;
  };
  let start = null;
  track.addEventListener('pointerdown', (e) => {
    e.stopPropagation();
    track.setPointerCapture(e.pointerId);
    start = { y: e.clientY, v: value };
  });
  track.addEventListener('pointermove', (e) => {
    if (!start) return;
    value = Math.min(1, Math.max(0, start.v - (e.clientY - start.y) / height));
    draw();
    onChange(value);
  });
  const end = () => (start = null);
  track.addEventListener('pointerup', end);
  track.addEventListener('pointercancel', end);
  return {
    el,
    set(v, label, cb) {
      value = v;
      text.textContent = label;
      onChange = cb;
      draw();
    },
  };
}

/** How far the screen is turned from the device's natural orientation, in degrees. */
export function screenAngle() {
  return ((((screen.orientation?.angle ?? window.orientation ?? 0) % 360) + 360) % 360);
}

/** The screen's rotation in quarter turns (0..3). */
export const screenQuarter = () => Math.round(screenAngle() / 90) % 4;

/**
 * Quarter turns (0..3) a painting is drawn at on screen: it stays the way up it
 * was when created (`home`) whichever way the screen is turned.
 */
export const screenTurn = (home, quarter = screenQuarter()) => ((((home ?? 0) - quarter) % 4) + 4) % 4;

export const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
