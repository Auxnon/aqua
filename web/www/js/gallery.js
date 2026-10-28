// The gallery of saved paintings, like the Android GalleryScreen: a grid of
// thumbnails, a + button for a new painting, and duplicate / delete from a
// card's menu (right-click or long-press). A painting coming back from the
// editor flies from where the canvas was into its card.

import { I } from './icons.js';
import { deletePainting, duplicatePainting, listPaintings } from './store.js';
import { h, icon } from './ui.js';

/** Corner radius of a card; the flying painting rounds off to match. */
const CARD_CORNER = 6;

export function createGallery(root, { onOpen, onCreate }) {
  const grid = h('div', { class: 'grid' });
  const empty = h('div', { class: 'empty' }, 'Tap + to start a painting');
  const title = h('h1', {}, 'Aqua');
  const fab = h('button', { class: 'fab', title: 'New painting', 'aria-label': 'New painting', onclick: () => newDialog() }, icon(I.add, 24));
  root.append(title, grid, empty, fab);
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
    const img = h('img', { src: url, alt: '', draggable: 'false' });
    const frame = h('div', { class: 'thumb', style: { aspectRatio: `${art.width} / ${Math.max(1, art.height)}` } }, img);
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
    // Unwind the shorter way round.
    const rot = ((((from.rotation % 360) + 540) % 360) - 180);
    Object.assign(flyer.style, { left: `${to.left}px`, top: `${to.top}px`, width: `${to.width}px`, height: `${to.height}px`, borderRadius: `${CARD_CORNER}px` });
    const sx = from.w / to.width;
    const sy = from.h / to.height;
    const dx = from.cx - (to.left + to.width / 2);
    const dy = from.cy - (to.top + to.height / 2);
    document.body.append(flyer);
    await flyer.decode().catch(() => {});
    const anim = flyer.animate(
      [
        { transform: `translate(${dx}px, ${dy}px) rotate(${rot}deg) scale(${sx}, ${sy})`, borderRadius: '0px' },
        { transform: 'none', borderRadius: `${CARD_CORNER}px` },
      ],
      { duration: 520, easing: 'cubic-bezier(0.4, 0, 0.2, 1)' },
    );
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
