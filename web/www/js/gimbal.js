// The paper-tipping gimbal, ported from the Android app (Gimbal.kt). Dragging
// inside it tips the paper towards the finger; while it is open (or while
// "keep phone tilt on" is ticked) the device's tilt tips it further, measured
// from how it was held when that started or the last drag ended.

import { h } from './ui.js';

/** How far the device has to tip, from where it was held, for the steepest paper. */
const STEEPEST_DEG = 50;

/** Tilts shorter than this count as level, so a hand's tremor doesn't stir the paint. */
const DEAD_ZONE = 0.04;

/** Matches --accent in style.css (SVG attributes can't read CSS variables). */
export const ACCENT = '#8fd3cb';
export const TEXT = '#eceae6';

const STEEP = Math.sin((STEEPEST_DEG * Math.PI) / 180);

const len = ([x, y]) => Math.hypot(x, y);

function clampLength(t) {
  const l = len(t);
  return l > 1 ? [t[0] / l, t[1] / l] : t;
}

/** Steepness 0..1 as degrees, for the readout. */
export const degrees = (m) => Math.round((Math.asin(m * STEEP) * 180) / Math.PI);

/** Draws the gimbal into an <svg>: a ring leaning with the tilt, and a dot sliding downhill. */
export function drawGimbal(svg, tilt, { color, stroke, dot, axes }) {
  const size = parseFloat(svg.getAttribute('width'));
  const r = size / 2 - stroke;
  const c = size / 2;
  const m = len(tilt);
  const angle = (Math.atan2(tilt[1], tilt[0]) * 180) / Math.PI;
  // The paper seen tipped away: squashed along the slope.
  const squash = Math.cos((degrees(m) * Math.PI) / 180);
  const [px, py] = [c + tilt[0] * r, c + tilt[1] * r];
  svg.innerHTML = `
    <circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="${color}" stroke-opacity="0.45" stroke-width="${stroke}"/>
    ${axes ? `<path d="M${c - r} ${c}H${c + r}M${c} ${c - r}V${c + r}" stroke="${color}" stroke-opacity="0.15" stroke-width="${stroke / 2}"/>` : ''}
    <ellipse cx="${c}" cy="${c}" rx="${r * squash}" ry="${r}" transform="rotate(${angle} ${c} ${c})" fill="none" stroke="${color}" stroke-width="${stroke}"/>
    ${m > 0 ? `<line x1="${c}" y1="${c}" x2="${px}" y2="${py}" stroke="${color}" stroke-opacity="0.6" stroke-width="${stroke}"/>` : ''}
    <circle cx="${px}" cy="${py}" r="${dot}" fill="${color}"/>`;
}

export function gimbalSvg(size) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', size);
  svg.setAttribute('height', size);
  svg.setAttribute('viewBox', `0 0 ${size} ${size}`);
  return svg;
}

/**
 * Turns gravity readings into a screen-space tilt: the paper's tilt is `base`
 * plus however far the device has tipped since the reference reading.
 */
class GravityTilt {
  constructor() {
    this.base = [0, 0];
    this.dragging = false;
    this.ref = null;
    this.g = [0, 0, 0];
    this.primed = false;
  }

  start(t) {
    this.base = t;
    this.dragging = false;
    this.ref = null;
    this.primed = false;
  }

  /** Ends a drag at `t`: the device's current pose now means `t`. */
  release(t) {
    this.dragging = false;
    this.base = t;
    this.ref = this.g.slice();
  }

  update(v, rotation) {
    // Device axes into screen axes (x right, y up), for the screen's rotation.
    const [sx, sy] = rotation === 90 ? [-v[1], v[0]] : rotation === 180 ? [-v[0], -v[1]] : rotation === 270 ? [v[1], -v[0]] : [v[0], v[1]];
    const n = Math.hypot(sx, sy, v[2]);
    if (n < 1e-3) return null;
    const now = [sx / n, sy / n, v[2] / n];
    // Smoothed, so the sensor's noise doesn't shiver the paint.
    if (!this.primed) {
      this.g = now;
      this.primed = true;
    } else for (let i = 0; i < 3; i++) this.g[i] += (now[i] - this.g[i]) * 0.2;
    this.ref ??= this.g.slice();
    if (this.dragging) return null;
    const d = relative(this.g, this.ref);
    // Downhill is against the in-plane part of gravity; screen y points down.
    const t = clampLength([this.base[0] - d[0] / STEEP, this.base[1] + d[1] / STEEP]);
    return len(t) < DEAD_ZONE ? [0, 0] : t;
  }
}

/** `v` expressed in a frame where the reference `r` points straight up (+z). */
function relative(v, r) {
  // Axis r × z and angle between them (Rodrigues' rotation).
  const kx = r[1];
  const ky = -r[0];
  const s = Math.hypot(kx, ky);
  const c = r[2];
  if (s < 1e-4) return c > 0 ? v : [v[0], -v[1], -v[2]];
  const ux = kx / s;
  const uy = ky / s;
  const cx = uy * v[2];
  const cy = -ux * v[2];
  const cz = ux * v[1] - uy * v[0];
  const dot = ux * v[0] + uy * v[1];
  return [v[0] * c + cx * s + ux * dot * (1 - c), v[1] * c + cy * s + uy * dot * (1 - c), v[2] * c + cz * s];
}

// iOS reports gravity pointing down rather than up (the opposite of the spec
// and of Android); flip it so tipping goes the same way everywhere.
const IOS = /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);

/** Device tilt through `devicemotion`. iOS needs permission from a tap first. */
export class PhoneTilt {
  constructor(onTilt) {
    this.onTilt = onTilt;
    this.gyro = new GravityTilt();
    this.running = false;
    this.listener = (e) => {
      const a = e.accelerationIncludingGravity;
      if (!a || a.x == null) return;
      PhoneTilt.seen = true;
      const s = IOS ? -1 : 1;
      const rot = screen.orientation?.angle ?? window.orientation ?? 0;
      const t = this.gyro.update([a.x * s, a.y * s, a.z * s], ((rot % 360) + 360) % 360);
      if (t) this.onTilt(t);
    };
  }

  /** Whether this device may be able to report tilt at all. */
  static available() {
    return 'DeviceMotionEvent' in window && (PhoneTilt.seen || navigator.maxTouchPoints > 0);
  }

  /** Must be called from a tap on iOS. */
  static async permit() {
    const D = window.DeviceMotionEvent;
    if (typeof D?.requestPermission !== 'function') return true;
    try {
      return (await D.requestPermission()) === 'granted';
    } catch {
      return false;
    }
  }

  setRunning(on, tilt) {
    if (on === this.running) return;
    this.running = on;
    if (on) {
      this.gyro.start(tilt);
      window.addEventListener('devicemotion', this.listener);
    } else window.removeEventListener('devicemotion', this.listener);
  }
}
PhoneTilt.seen = false;

/**
 * The gimbal overlay over the middle of the canvas. Tapping outside closes it;
 * the paper stays tipped.
 */
export function gimbalOverlay({ getTilt, setTilt, phone, keepOn, onKeepOn, onDismiss }) {
  const SIZE = 220;
  const svg = gimbalSvg(SIZE * 0.84 + 8);
  const disc = h('div', { class: 'gimbal-disc' }, svg);
  const readout = h('div', { class: 'pill' });
  const level = h('button', { class: 'pill action' }, 'Level');
  const keep = PhoneTilt.available()
    ? h('label', { class: 'pill check' }, h('input', { type: 'checkbox' }), 'Keep phone tilt on when closed')
    : null;
  const el = h('div', { class: 'gimbal-overlay' }, h('div', { class: 'gimbal-col' }, disc, readout, level, keep));

  const refresh = () => {
    const t = getTilt();
    const m = len(t);
    drawGimbal(svg, t, { color: ACCENT, stroke: 2, dot: 9, axes: true });
    readout.textContent = m === 0 ? 'Level · drag or tip the phone' : `Tipped ${degrees(m)}°`;
    level.disabled = m === 0;
    if (keep) keep.querySelector('input').checked = keepOn();
  };

  el.addEventListener('pointerdown', (e) => {
    if (e.target === el) onDismiss();
  });
  level.addEventListener('click', () => {
    phone.gyro.release([0, 0]);
    setTilt([0, 0]);
  });
  keep?.querySelector('input').addEventListener('change', async (e) => {
    const on = e.target.checked;
    if (on && !(await PhoneTilt.permit())) {
      e.target.checked = false;
      return;
    }
    onKeepOn(on);
  });

  const radius = (SIZE / 2) * 0.84;
  const at = (e) => {
    const r = disc.getBoundingClientRect();
    const t = clampLength([(e.clientX - r.left - r.width / 2) / radius, (e.clientY - r.top - r.height / 2) / radius]);
    return len(t) < DEAD_ZONE ? [0, 0] : t;
  };
  let dragging = false;
  disc.addEventListener('pointerdown', (e) => {
    e.stopPropagation();
    disc.setPointerCapture(e.pointerId);
    dragging = true;
    phone.gyro.dragging = true;
    setTilt(at(e));
  });
  disc.addEventListener('pointermove', (e) => {
    if (dragging) setTilt(at(e));
  });
  const end = () => {
    if (!dragging) return;
    dragging = false;
    phone.gyro.release(getTilt());
  };
  disc.addEventListener('pointerup', end);
  disc.addEventListener('pointercancel', end);

  refresh();
  return { el, refresh };
}
