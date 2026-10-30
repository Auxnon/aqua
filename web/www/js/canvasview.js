// The painting surface: the WebGPU canvas, its view transform and the input on
// it. Ported from the Android CanvasSurfaceView: one finger / pen / mouse paints,
// two fingers pan-zoom-rotate, a two-finger tap undoes and a three-finger tap
// redoes. Once a pen has been seen, fingers only navigate. Pinching far out and
// letting go goes back to the gallery. With a mouse, the wheel zooms and the
// middle or right button (or space + drag) pans.

import { screenAngle, screenTurn } from './ui.js';

/** Pinching out below this fraction of the fitted size and letting go fits the view or opens the gallery. */
const OVERVIEW_ZOOM = 0.45;
/** A two-finger swipe up to the gallery: at least this fraction of the screen height... */
const SWIPE_DISTANCE = 0.18;
/** ...within this many milliseconds, so a slow two-finger pan stays a pan. */
const SWIPE_MS = 450;

/** 2D affine x' = a x + b y + c, y' = d x + e y + f (Android Matrix order). */
class Mat {
  constructor(m = [1, 0, 0, 0, 1, 0]) {
    this.m = m.slice();
  }
  copy() {
    return new Mat(this.m);
  }
  set(o) {
    this.m = o.m.slice();
  }
  /** this = t ∘ this */
  post(t) {
    const [a, b, c, d, e, f] = this.m;
    const [A, B, C, D, E, F] = t;
    this.m = [A * a + B * d, A * b + B * e, A * c + B * f + C, D * a + E * d, D * b + E * e, D * c + E * f + F];
  }
  postTranslate(x, y) {
    this.post([1, 0, x, 0, 1, y]);
  }
  postScale(s) {
    this.post([s, 0, 0, 0, s, 0]);
  }
  postRotate(deg) {
    const r = (deg * Math.PI) / 180;
    const c = Math.cos(r);
    const s = Math.sin(r);
    this.post([c, -s, 0, s, c, 0]);
  }
  invert() {
    const [a, b, c, d, e, f] = this.m;
    const det = a * e - b * d || 1e-9;
    return new Mat([e / det, -b / det, (b * f - c * e) / det, -d / det, a / det, (c * d - a * f) / det]);
  }
  map(x, y) {
    const [a, b, c, d, e, f] = this.m;
    return [a * x + b * y + c, d * x + e * y + f];
  }
  scale() {
    const [a, b, , d, e] = this.m;
    return Math.sqrt(Math.abs(a * e - b * d));
  }
  /** Rotation in radians. */
  angle() {
    return Math.atan2(this.m[3], this.m[0]);
  }
}

export class CanvasView {
  constructor(canvas, engine, cb) {
    this.canvas = canvas;
    this.engine = engine;
    this.cb = cb; // { onStrokeEnd, onUndo, onRedo, onSample, onTouchStart, onOverviewHint, onOverview, onTiltTurned }
    this.view = new Mat();
    this.inverse = new Mat();
    this.canvasW = 0;
    this.canvasH = 0;
    this.fitScale = 1;
    this.fitted = false;
    this.overviewHint = false;
    this.eyedropper = false;
    this.enabled = false;
    this.tilt = [0, 0];
    this.home = 0;
    // What pinching far out does: false fits the painting to the screen again, true opens the gallery.
    this.pinchOutToGallery = false;
    // Where a two-finger touch started, to tell a swipe up from a pan.
    this.swipe = null;
    this.fitAnim = 0;
    this.pointers = new Map();
    this.mode = 'none';
    this.drawPointer = -1;
    this.drawWithPen = false;
    this.penSeen = false;
    this.space = false;
    this.dpr = 1;
    this.slop = 8;

    const on = (t, f, o) => canvas.addEventListener(t, f, o);
    on('pointerdown', (e) => this.down(e));
    on('pointermove', (e) => this.move(e));
    on('pointerup', (e) => this.up(e, false));
    on('pointercancel', (e) => this.up(e, true));
    on('wheel', (e) => this.wheel(e), { passive: false });
    on('contextmenu', (e) => e.preventDefault());
    this.angle = screenAngle();
    new ResizeObserver(() => this.resize()).observe(canvas);
    // A half turn keeps the window's size, so only the orientation says it happened.
    if (screen.orientation) screen.orientation.addEventListener('change', () => this.resize());
    else window.addEventListener('orientationchange', () => this.resize());
    this.resize();
  }

  resize() {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(this.canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    const angle = screenAngle();
    if (w === this.canvas.width && h === this.canvas.height && dpr === this.dpr && angle === this.angle) return;
    const [ow, oh] = [this.canvas.width, this.canvas.height];
    const steps = (((angle - this.angle) / 90) % 4 + 4) % 4;
    this.angle = angle;
    this.dpr = dpr;
    this.slop = 8 * dpr;
    if (w !== ow || h !== oh) {
      this.canvas.width = w;
      this.canvas.height = h;
      this.engine.resize(w, h);
    }
    if (this.fitted) {
      // Keep what was in the middle of the screen in the middle. When the device
      // turns the window turns under the paper, so the view turns the other way
      // and the paper stays still in the hand; only the controls move.
      this.view.postTranslate(-ow / 2, -oh / 2);
      if (steps) this.view.postRotate(-90 * steps);
      this.view.postTranslate(w / 2, h / 2);
      if (steps && (this.tilt[0] || this.tilt[1])) {
        // The tilt is in screen space too; turn it with the paper.
        const a = (-90 * steps * Math.PI) / 180;
        const [x, y] = this.tilt;
        this.tilt = [x * Math.cos(a) - y * Math.sin(a), x * Math.sin(a) + y * Math.cos(a)];
        this.cb.onTiltTurned?.(this.tilt);
      }
      this.pushView();
    } else this.fitIfReady();
  }

  /** The painting's size, and which way up it is (`home`, see screenTurn). */
  setCanvasSize(w, h, home = 0) {
    if (w !== this.canvasW || h !== this.canvasH || home !== this.home) {
      this.canvasW = w;
      this.canvasH = h;
      this.home = home;
      this.fitted = false;
    }
    this.fitIfReady();
  }

  /** Fits the painting to the screen, the way up it was created; `animate` glides there. */
  fit(animate = false) {
    const { width, height } = this.canvas;
    if (!width || !this.canvasW) return;
    cancelAnimationFrame(this.fitAnim);
    const q = screenTurn(this.home);
    const [w, h] = q % 2 ? [this.canvasH, this.canvasW] : [this.canvasW, this.canvasH];
    const s = Math.min((width * 0.92) / w, (height * 0.84) / h);
    this.fitScale = s;
    this.angle = screenAngle();
    if (!animate) {
      this.place(width / 2, height / 2, s, 90 * q);
      return;
    }
    // From where the painting is now: its centre, scale and angle (the shorter way round).
    const [cx, cy] = this.view.map(this.canvasW / 2, this.canvasH / 2);
    const s0 = this.view.scale();
    const a0 = (this.view.angle() * 180) / Math.PI;
    const a1 = a0 + ((((90 * q - a0) % 360) + 540) % 360) - 180;
    const start = performance.now();
    const step = (now) => {
      const u = Math.min(1, (now - start) / 320);
      const t = 1 - (1 - u) ** 3;
      this.place(cx + (width / 2 - cx) * t, cy + (height / 2 - cy) * t, s0 * (s / s0) ** t, a0 + (a1 - a0) * t);
      if (u < 1) this.fitAnim = requestAnimationFrame(step);
    };
    this.fitAnim = requestAnimationFrame(step);
  }

  /** Puts the painting's centre at (cx, cy), at scale `s` and turned `deg` degrees. */
  place(cx, cy, s, deg) {
    this.view = new Mat([s, 0, (-this.canvasW * s) / 2, 0, s, (-this.canvasH * s) / 2]);
    this.view.postRotate(deg);
    this.view.postTranslate(cx, cy);
    this.pushView();
  }

  fitIfReady() {
    if (!this.fitted && this.canvas.width > 1 && this.canvasW > 0) {
      this.fitted = true;
      this.fit();
    }
  }

  pushView() {
    this.inverse = this.view.invert();
    this.engine.setView(Float32Array.from([...this.inverse.m, this.view.scale()]));
    if (this.tilt[0] || this.tilt[1]) this.pushTilt();
  }

  /** Tips the paper towards screen direction (x, y), length 0 (level) to 1 (steep). */
  setTilt(x, y) {
    this.tilt = [x, y];
    this.pushTilt();
  }

  /** The paper turns with the view, so the downhill direction is turned back into canvas space. */
  pushTilt() {
    const a = -this.view.angle();
    const [x, y] = this.tilt;
    this.engine.setTilt(x * Math.cos(a) - y * Math.sin(a), x * Math.sin(a) + y * Math.cos(a));
  }

  pastOverview() {
    return this.canvasW > 0 && this.view.scale() < this.fitScale * OVERVIEW_ZOOM;
  }

  setOverviewHint(on) {
    if (on !== this.overviewHint) {
      this.overviewHint = on;
      this.cb.onOverviewHint(on);
    }
  }

  /** Where the painting is on screen, in CSS pixels: centre, size and rotation in degrees. */
  frame() {
    if (!this.canvasW) return null;
    const [cx, cy] = this.view.map(this.canvasW / 2, this.canvasH / 2);
    const s = this.view.scale() / this.dpr;
    const r = this.canvas.getBoundingClientRect();
    return { cx: r.left + cx / this.dpr, cy: r.top + cy / this.dpr, w: this.canvasW * s, h: this.canvasH * s, rotation: (this.view.angle() * 180) / Math.PI };
  }

  // ------------------------------------------------------------------ input

  pos(e) {
    const r = this.canvas.getBoundingClientRect();
    return [(e.clientX - r.left) * this.dpr, (e.clientY - r.top) * this.dpr];
  }

  toCanvas(x, y) {
    return this.inverse.map(x, y);
  }

  pressure(e) {
    if (e.pointerType === 'pen') return Math.min(1, Math.max(0.02, e.pressure || 0.5));
    return 0.8;
  }

  sendPoints(e, phase) {
    const evs = phase === 1 && e.getCoalescedEvents ? e.getCoalescedEvents() : [];
    const list = evs.length ? evs : [e];
    const out = new Float32Array(list.length * 3);
    list.forEach((ev, i) => {
      const [x, y] = this.toCanvas(...this.pos(ev));
      out[i * 3] = x;
      out[i * 3 + 1] = y;
      out[i * 3 + 2] = this.pressure(ev);
    });
    this.engine.stroke(phase, out);
  }

  async sample(e) {
    const [x, y] = this.toCanvas(...this.pos(e));
    const c = await this.engine.sample(x, y);
    if (c) this.cb.onSample(c);
  }

  /** The two (or one) navigating pointers, in screen pixels. */
  gesturePoints() {
    return [...this.pointers.values()].slice(0, 2).map((p) => p.pos);
  }

  beginGesture(time = performance.now()) {
    this.gestureStart = this.view.copy();
    const pts = this.gesturePoints();
    this.gPointers = pts.length;
    if (pts.length >= 2) {
      const [[x0, y0], [x1, y1]] = pts;
      this.g0 = [(x0 + x1) / 2, (y0 + y1) / 2];
      this.gDist = Math.max(1, Math.hypot(x1 - x0, y1 - y0));
      this.gAngle = Math.atan2(y1 - y0, x1 - x0);
      this.swipe ??= { time, x: this.g0[0], y: this.g0[1], dist: this.gDist };
    } else if (pts.length === 1) {
      this.g0 = pts[0];
    }
  }

  updateGesture() {
    const pts = this.gesturePoints();
    if (pts.length < this.gPointers) return;
    this.view.set(this.gestureStart);
    if (this.gPointers >= 2) {
      const [[x0, y0], [x1, y1]] = pts;
      const cx = (x0 + x1) / 2;
      const cy = (y0 + y1) / 2;
      const base = this.gestureStart.scale();
      let s = Math.max(1, Math.hypot(x1 - x0, y1 - y0)) / this.gDist;
      s = Math.min(32, Math.max(0.05, base * s)) / base;
      let deg = ((Math.atan2(y1 - y0, x1 - x0) - this.gAngle) * 180) / Math.PI;
      if (Math.abs(deg) < 4) deg = 0;
      this.view.postTranslate(-this.g0[0], -this.g0[1]);
      this.view.postScale(s);
      this.view.postRotate(deg);
      this.view.postTranslate(cx, cy);
      if (Math.hypot(cx - this.g0[0], cy - this.g0[1]) > this.slop || Math.abs(s - 1) > 0.04 || deg !== 0) this.moved = true;
      this.setOverviewHint(this.pastOverview());
    } else {
      const [x, y] = pts[0];
      const dx = x - this.g0[0];
      const dy = y - this.g0[1];
      this.view.postTranslate(dx, dy);
      if (Math.hypot(dx, dy) > this.slop) this.moved = true;
    }
    this.pushView();
  }

  down(e) {
    if (!this.enabled) return;
    e.preventDefault();
    this.canvas.setPointerCapture(e.pointerId);
    this.pointers.set(e.pointerId, { pos: this.pos(e), type: e.pointerType });
    const isPen = e.pointerType === 'pen';
    if (this.pointers.size === 1) {
      this.cb.onTouchStart();
      cancelAnimationFrame(this.fitAnim);
      this.swipe = null;
      if (isPen) this.penSeen = true;
      this.downTime = e.timeStamp;
      this.maxPointers = 1;
      this.moved = false;
      const navButton = e.pointerType === 'mouse' && (e.button === 1 || e.button === 2 || this.space);
      if (navButton || (e.pointerType === 'touch' && this.penSeen)) {
        this.mode = 'gesture';
        this.beginGesture(e.timeStamp);
      } else if (this.eyedropper) {
        this.mode = 'eyedrop';
        this.sample(e);
      } else {
        this.mode = 'drawing';
        this.drawPointer = e.pointerId;
        this.drawWithPen = isPen;
        this.sendPoints(e, 0);
      }
      return;
    }
    this.maxPointers = Math.max(this.maxPointers, this.pointers.size);
    const newIsFinger = e.pointerType === 'touch';
    if (this.mode === 'drawing') {
      if (this.drawWithPen && newIsFinger) {
        this.pointers.delete(e.pointerId); // a resting palm
        return;
      }
      // A second finger right after the first means this was a gesture all along.
      const quick = e.timeStamp - this.downTime < 250;
      // The first finger stays down and becomes one of the two navigating.
      this.engine.stroke(quick ? 3 : 2, new Float32Array(0));
      this.drawPointer = -1;
      this.mode = 'gesture';
      this.moved = !quick;
      this.beginGesture(e.timeStamp);
    } else if (this.mode === 'gesture' || this.mode === 'eyedrop') {
      this.mode = 'gesture';
      this.beginGesture(e.timeStamp);
    }
  }

  /** Whether two fingers lifting now ends a quick swipe up (to the gallery) rather than a pan or pinch. */
  isSwipeUp(time) {
    const sw = this.swipe;
    if (!sw || this.pointers.size !== 2 || this.maxPointers !== 2 || time - sw.time > SWIPE_MS) return false;
    const [[x0, y0], [x1, y1]] = this.gesturePoints();
    const cx = (x0 + x1) / 2;
    const cy = (y0 + y1) / 2;
    const zoom = Math.hypot(x1 - x0, y1 - y0) / sw.dist;
    const up = sw.y - cy;
    return up > this.canvas.height * SWIPE_DISTANCE && Math.abs(cx - sw.x) < up * 0.6 && zoom > 0.75 && zoom < 1.33;
  }

  move(e) {
    const p = this.pointers.get(e.pointerId);
    if (!p) return;
    p.pos = this.pos(e);
    if (this.mode === 'drawing' && e.pointerId === this.drawPointer) this.sendPoints(e, 1);
    else if (this.mode === 'gesture') this.updateGesture();
    else if (this.mode === 'eyedrop') this.sample(e);
  }

  up(e, cancelled) {
    if (!this.pointers.has(e.pointerId)) return;
    if (!cancelled && this.mode === 'gesture' && this.isSwipeUp(e.timeStamp)) {
      this.pointers.delete(e.pointerId);
      this.mode = 'none';
      this.setOverviewHint(false);
      const f = this.frame();
      if (f) this.cb.onOverview(f);
      return;
    }
    this.pointers.delete(e.pointerId);
    if (this.mode === 'drawing' && e.pointerId === this.drawPointer) {
      if (cancelled) this.engine.stroke(3, new Float32Array(0));
      else {
        this.sendPoints(e, 2);
        this.cb.onStrokeEnd();
      }
      this.mode = 'none';
      return;
    }
    if (this.mode !== 'gesture') {
      if (this.pointers.size === 0) this.mode = 'none';
      return;
    }
    if (this.pointers.size > 0) {
      // Re-anchor on the pointers that remain, so lifting one finger doesn't jump.
      this.beginGesture(e.timeStamp);
      return;
    }
    this.mode = 'none';
    if (cancelled) {
      this.setOverviewHint(false);
    } else if (!this.moved && e.timeStamp - this.downTime < 350 && e.pointerType === 'touch') {
      if (this.maxPointers === 2) this.cb.onUndo();
      if (this.maxPointers === 3) this.cb.onRedo();
    } else if (this.overviewHint && this.pastOverview()) {
      if (this.pinchOutToGallery) {
        const f = this.frame();
        if (f) this.cb.onOverview(f);
      } else this.fit(true);
    }
    this.setOverviewHint(false);
  }

  /** Zooms around the cursor; a trackpad pinch arrives as a ctrl+wheel too. */
  wheel(e) {
    if (!this.enabled) return;
    e.preventDefault();
    const [x, y] = this.pos(e);
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    const base = this.view.scale();
    const s = Math.min(32, Math.max(0.05, base * Math.exp((-e.deltaY * unit) / (e.ctrlKey ? 100 : 500)))) / base;
    this.view.postTranslate(-x, -y);
    this.view.postScale(s);
    this.view.postTranslate(x, y);
    this.pushView();
  }

  /** Zoom by a factor around the middle of the screen (keyboard). */
  zoomBy(f) {
    const { width, height } = this.canvas;
    const base = this.view.scale();
    const s = Math.min(32, Math.max(0.05, base * f)) / base;
    this.view.postTranslate(-width / 2, -height / 2);
    this.view.postScale(s);
    this.view.postTranslate(width / 2, height / 2);
    this.pushView();
  }

  /** Abandons whatever the pointers were doing (e.g. a panel opened). */
  reset() {
    if (this.mode === 'drawing') this.engine.stroke(3, new Float32Array(0));
    this.pointers.clear();
    this.mode = 'none';
    this.setOverviewHint(false);
  }
}
