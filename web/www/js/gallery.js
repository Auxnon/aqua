// The gallery of saved paintings, like the Android GalleryScreen: a grid of
// thumbnails, a + button for a new painting, and duplicate / delete from a
// card's menu (right-click or long-press). A painting coming back from the
// editor flies from where the canvas was into its card.

import { I } from './icons.js';
import { deletePainting, duplicatePainting, getValue, listPaintings, setValue } from './store.js';
import { h, icon, roundButton, screenTurn } from './ui.js';

/** Corner radius of a card; the flying painting rounds off to match. */
const CARD_CORNER = 6;

export function createGallery(root, { onOpen, onCreate }) {
  const grid = h('div', { class: 'grid' });
  const empty = h('div', { class: 'empty' }, 'Tap + to start a painting');
  const title = h('h1', {}, 'Aqua');
  const fab = h('button', { class: 'fab', title: 'New painting', 'aria-label': 'New painting', onclick: () => newDialog() }, icon(I.add, 24));
  const settings = roundButton(I.settings, 'Settings', () => settingsDialog(), { cls: 'dim' });
  root.append(h('div', { class: 'gallery-head' }, title, settings), grid, empty, fab);
  // Cards keep each painting the way up it was created, so they turn back when the device turns.
  const turnCards = () => { if (!root.classList.contains('off')) reload(); };
  if (screen.orientation) screen.orientation.addEventListener('change', turnCards);
  else window.addEventListener('orientationchange', turnCards);
  let urls = [];
  let busy = false;

  async function open(id) {
    if (busy) return;
    busy = true;
    try {
      await onOpen(id);
    } finally {
      busy = false;
    }
  }

  async function reload() {
    const items = await listPaintings().catch(() => []);
    urls.forEach(URL.revokeObjectURL);
    urls = [];
    empty.hidden = items.length > 0;
    grid.replaceChildren(...items.map((art) => card(art)));
    return items;
  }

  function card(art) {
    const url = URL.createObjectURL(art.thumb);
    urls.push(url);
    const q = screenTurn(art.home);
    const [w, ht] = [Math.max(1, art.width), Math.max(1, art.height)];
    // Turned a quarter, the image is laid out the painting's way round and
    // rotated into the card, whose sides are swapped.
    const img = h('img', {
      src: url,
      alt: '',
      draggable: 'false',
      style: q % 2
        ? { position: 'absolute', left: '50%', top: '50%', width: `${(100 * w) / ht}%`, height: `${(100 * ht) / w}%`, transform: `translate(-50%, -50%) rotate(${90 * q}deg)` }
        : { transform: q ? 'rotate(180deg)' : '' },
    });
    const frame = h('div', { class: 'thumb', style: { aspectRatio: q % 2 ? `${ht} / ${w}` : `${w} / ${ht}`, position: 'relative' } }, img);
    const date = h('div', { class: 'date' }, new Date(art.modified).toLocaleDateString(undefined, { dateStyle: 'medium' }));
    const el = h('div', { class: 'card', 'data-id': art.id }, frame, date);
    frame.addEventListener('click', () => open(art.id));
    frame.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      menu(e.clientX, e.clientY, art);
    });
    // Long-press for touch.
    let timer = 0;
    frame.addEventListener('pointerdown', (e) => {
      if (e.pointerType !== 'touch') return;
      timer = setTimeout(() => {
        timer = 0;
        frame.dataset.longpress = '1';
        menu(e.clientX, e.clientY, art);
      }, 500);
    });
    const cancel = () => clearTimeout(timer);
    frame.addEventListener('pointerup', cancel);
    frame.addEventListener('pointermove', cancel);
    frame.addEventListener('click', (e) => {
      if (frame.dataset.longpress) {
        delete frame.dataset.longpress;
        e.stopImmediatePropagation();
      }
    }, true);
    return el;
  }

  function menu(x, y, art) {
    const close = () => scrim.remove();
    const m = h('div', { class: 'menu', style: { left: `${Math.min(x, innerWidth - 180)}px`, top: `${Math.min(y, innerHeight - 110)}px` } },
      h('button', { onclick: async () => { close(); await duplicatePainting(art.id); reload(); } }, 'Duplicate'),
      h('button', { class: 'danger', onclick: () => { close(); confirmDelete(art); } }, 'Delete'),
    );
    const scrim = h('div', { class: 'scrim clear', onpointerdown: (e) => { if (e.target === scrim) close(); } }, m);
    document.body.append(scrim);
  }

  function confirmDelete(art) {
    dialog('Delete painting?', h('p', {}, "This can't be undone."), [
      ['Cancel', () => true],
      ['Delete', async () => { await deletePainting(art.id); reload(); return true; }, 'danger'],
    ]);
  }

  async function settingsDialog() {
    const pinch = (await getValue('pinchOut').catch(() => null)) === 'gallery' ? 'gallery' : 'fit';
    const choice = (value, label) => {
      const input = h('input', { type: 'radio', name: 'pinch', value, checked: pinch === value });
      input.addEventListener('change', () => setValue('pinchOut', value).catch(() => {}));
      return h('label', { class: 'choice' }, input, label);
    };
    dialog('Gestures', [
      h('div', { class: 'caption' }, 'Pinch far out and let go'),
      choice('fit', 'Fit the painting to the screen'),
      choice('gallery', 'Go back to the gallery'),
      h('p', { class: 'caption' }, 'Swipe up quickly with two fingers to go back to the gallery.'),
    ], [['Done', () => true]]);
  }

  function newDialog() {
    const dpr = devicePixelRatio || 1;
    // Screen-shaped, longest side capped to keep the wet simulation light.
    const s = Math.min(1, 2400 / Math.max(innerWidth * dpr, innerHeight * dpr, 1));
    const presets = [
      ['Screen', Math.max(512, Math.round(innerWidth * dpr * s)), Math.max(512, Math.round(innerHeight * dpr * s))],
      ['Square', 2048, 2048],
      ['Portrait', 1536, 2048],
      ['Landscape', 2048, 1536],
      ['Postcard', 1800, 1200],
    ];
    const papers = ['Cold press', 'Hot press', 'Rough'];
    let size = presets[0];
    let paper = 0;
    const sizes = h('div', { class: 'choices column' });
    const paperRow = h('div', { class: 'choices' });
    const draw = () => {
      sizes.replaceChildren(...presets.map((p) => h('button', { class: `chip${p === size ? ' on' : ''}`, onclick: () => { size = p; draw(); } }, `${p[0]}  ·  ${p[1]}×${p[2]}`)));
      paperRow.replaceChildren(...papers.map((name, i) => h('button', { class: `chip${i === paper ? ' on' : ''}`, onclick: () => { paper = i; draw(); } }, name)));
    };
    draw();
    dialog('New painting', [h('div', { class: 'caption' }, 'Size'), sizes, h('div', { class: 'caption' }, 'Paper'), paperRow], [
      ['Cancel', () => true],
      ['Create', async () => { await onCreate(size[1], size[2], paper); return true; }],
    ]);
  }

  /**
   * Flies the painting from `from` (centre, size and rotation on screen) into
   * its card while the rest of the gallery fades in around it.
   */
  async function arrive(id, from) {
    const items = await reload();
    const target = grid.querySelector(`[data-id="${CSS.escape(id)}"] .thumb`);
    const art = items.find((a) => a.id === id);
    if (!from || !target || !art) return;
    root.classList.add('arriving');
    target.style.visibility = 'hidden';
    const to = target.getBoundingClientRect();
    const url = URL.createObjectURL(art.thumb);
    const flyer = h('img', { class: 'flyer', src: url });
    // The card holds the painting turned to the way up it was created; the
    // flight unwinds the view's rotation to that, the shorter way round.
    const q = screenTurn(art.home);
    const end = 90 * q;
    const wrap = (a) => ((((a % 360) + 540) % 360) - 180);
    const own = wrap(from.rotation - end);
    const [cw, ch] = q % 2 ? [to.height, to.width] : [to.width, to.height];
    const ccx = to.left + to.width / 2;
    const ccy = to.top + to.height / 2;
    Object.assign(flyer.style, { left: `${ccx - cw / 2}px`, top: `${ccy - ch / 2}px`, width: `${cw}px`, height: `${ch}px`, borderRadius: `${CARD_CORNER}px` });
    const sx = from.w / cw;
    const sy = from.h / ch;
    const dx = from.cx - ccx;
    const dy = from.cy - ccy;
    const frames = [];
    for (let i = 0; i <= 16; i++) {
      const t = i / 16;
      const k = 1 - t;
      frames.push({
        offset: t,
        transform: `translate(${dx * k}px, ${dy * k}px) rotate(${end + own * k}deg) scale(${sx + (1 - sx) * t}, ${sy + (1 - sy) * t})`,
        borderRadius: `${CARD_CORNER * t}px`,
      });
    }
    document.body.append(flyer);
    await flyer.decode().catch(() => {});
    const anim = flyer.animate(frames, { duration: 520, easing: 'cubic-bezier(0.4, 0, 0.2, 1)' });
    anim.finished.finally(() => {
      target.style.visibility = '';
      flyer.remove();
      URL.revokeObjectURL(url);
      root.classList.remove('arriving');
    });
  }

  return { reload, arrive };
}

/** A small modal dialog. Each action returns true to close it. */
export function dialog(title, body, actions) {
  const close = () => scrim.remove();
  const box = h('div', { class: 'dialog', role: 'dialog' },
    h('h2', {}, title),
    h('div', { class: 'dialog-body' }, body),
    h('div', { class: 'dialog-actions' }, actions.map(([label, fn, cls]) => h('button', { class: `text ${cls ?? ''}`, onclick: async () => { if (await fn()) close(); } }, label))),
  );
  const scrim = h('div', { class: 'scrim', onpointerdown: (e) => { if (e.target === scrim) close(); } }, box);
  document.body.append(scrim);
}
