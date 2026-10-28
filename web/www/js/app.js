// Starts the engine and switches between the gallery and the editor.

import { createEditor } from './editor.js';
import { loadWasm, startEngine } from './engine.js';
import { createGallery } from './gallery.js';
import { tray } from './panels.js';
import { newId, readPainting, writePainting } from './store.js';
import { h } from './ui.js';

const galleryRoot = document.getElementById('gallery');
const editorRoot = document.getElementById('editor');
const canvas = document.getElementById('paper');

function show(screen) {
  galleryRoot.classList.toggle('off', screen !== 'gallery');
  editorRoot.classList.toggle('off', screen !== 'editor');
}

async function main() {
  // The surface is created at the canvas's current size, so give it one first.
  show('editor');
  const dpr = devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(canvas.clientWidth * dpr));
  canvas.height = Math.max(1, Math.round(canvas.clientHeight * dpr));
  await loadWasm();
  const engine = await startEngine(canvas);
  await tray.load();

  const editor = createEditor(editorRoot, engine, canvas, {
    async onExit(id, from) {
      show('gallery');
      await gallery.arrive(id, from);
    },
  });

  const gallery = createGallery(galleryRoot, {
    async onOpen(id) {
      const files = await readPainting(id);
      if (!files) return;
      engine.load(files);
      show('editor');
      editor.open(id);
    },
    async onCreate(w, h, paper) {
      const id = newId();
      engine.newCanvas(w, h, paper, 1 + Math.floor(Math.random() * 0x7ffffffe));
      await writePainting(id, await engine.save());
      show('editor');
      editor.open(id);
    },
  });

  // For poking at from the devtools console.
  window.aqua = { engine, editor, gallery };

  // Painting is lost if the tab closes mid-stroke, but not what was saved.
  window.addEventListener('pagehide', () => editor.save());

  await gallery.reload();
  show('gallery');
}

main().catch((e) => {
  console.error(e);
  show('gallery');
  galleryRoot.replaceChildren(
    h('div', { class: 'unsupported' },
      h('h1', {}, 'Aqua'),
      navigator.gpu
        ? h('p', {}, 'Aqua couldn\'t start on this computer\'s GPU through WebGPU.')
        : h('p', {}, 'Aqua paints on the GPU with WebGPU, which this browser doesn\'t offer.'),
      h('p', { class: 'caption' }, navigator.gpu
        ? 'Check that WebGPU is hardware accelerated in chrome://gpu, or try chrome://flags/#enable-unsafe-webgpu.'
        : 'Try a recent Chrome, Edge or Safari, or Firefox on Windows.'),
      h('p', { class: 'caption mono' }, String(e?.message ?? e)),
    ),
  );
});
