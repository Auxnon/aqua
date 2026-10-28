// Paintings and the palette, kept in IndexedDB. A painting is the same set of
// files the Android app writes into a directory (meta.json, layer_<id>.bin,
// thumb.png); the gallery only reads a small summary record.

const DB = 'aqua';

let dbPromise = null;

function db() {
  dbPromise ??= new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => {
      const d = req.result;
      d.createObjectStore('summaries', { keyPath: 'id' });
      d.createObjectStore('files');
      d.createObjectStore('kv');
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

function done(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = tx.onabort = () => reject(tx.error);
  });
}

function request(req) {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/** Summaries of every painting, newest first: { id, width, height, modified, thumb: Blob }. */
export async function listPaintings() {
  const d = await db();
  const all = await request(d.transaction('summaries').objectStore('summaries').getAll());
  return all.sort((a, b) => b.modified - a.modified);
}

/** A painting's files: { name: Uint8Array }. */
export async function readPainting(id) {
  const d = await db();
  return request(d.transaction('files').objectStore('files').get(id));
}

/** Stores a painting's files and refreshes its gallery summary. */
export async function writePainting(id, files) {
  const meta = JSON.parse(new TextDecoder().decode(files['meta.json']));
  const summary = {
    id,
    width: meta.width,
    height: meta.height,
    modified: Date.now(),
    thumb: new Blob([files['thumb.png']], { type: 'image/png' }),
  };
  const d = await db();
  const tx = d.transaction(['summaries', 'files'], 'readwrite');
  tx.objectStore('files').put(files, id);
  tx.objectStore('summaries').put(summary);
  await done(tx);
  return summary;
}

export async function deletePainting(id) {
  const d = await db();
  const tx = d.transaction(['summaries', 'files'], 'readwrite');
  tx.objectStore('files').delete(id);
  tx.objectStore('summaries').delete(id);
  await done(tx);
}

export async function duplicatePainting(id) {
  const files = await readPainting(id);
  if (!files) return;
  await writePainting(newId(), files);
}

export function newId() {
  return crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export async function getValue(key) {
  const d = await db();
  return request(d.transaction('kv').objectStore('kv').get(key));
}

export async function setValue(key, value) {
  const d = await db();
  const tx = d.transaction('kv', 'readwrite');
  tx.objectStore('kv').put(value, key);
  await done(tx);
}
