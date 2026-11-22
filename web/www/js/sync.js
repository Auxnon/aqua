// Pushes saved paintings to aqua-sync daemons (sync/src/lib.rs has the
// protocol): the primary at once, the backup five seconds later. A newer save
// before then replaces the backup's copy, so it gets the latest.
//
// The page must itself be on http (localhost is fine): a page served over
// https may not reach a plain-http address on the network.

import { getValue, setValue } from './store.js';

export const PORT = 7717;
export const BACKUP_DELAY_MS = 5000;

const settings = { on: false, primary: '', backup: '', key: '' };
/** Per daemon: { state: 'idle' | 'sending' | 'done' | 'failed', at, message }. */
export const status = { primary: { state: 'idle' }, backup: { state: 'idle' } };
const listeners = new Set();
let backupTimer = 0;

export const sync = {
  settings,
  status,
  async load() {
    Object.assign(settings, (await getValue('sync').catch(() => null)) ?? {});
  },
  async set(v) {
    Object.assign(settings, v);
    await setValue('sync', { ...settings }).catch(() => {});
    tell();
  },
  onChange(fn) {
    listeners.add(fn);
    return () => listeners.delete(fn);
  },

  /** Sends a painting's files ({ name: Uint8Array }) if sync is on, or `force`d. */
  push(id, files, force = false) {
    if (!settings.on && !force) return;
    const body = encode(files);
    const key = settings.key;
    if (settings.backup) {
      clearTimeout(backupTimer);
      const addr = settings.backup;
      backupTimer = setTimeout(() => send('backup', addr, key, id, body), BACKUP_DELAY_MS);
    }
    if (settings.primary) send('primary', settings.primary, key, id, body);
  },

  /** Resolves to a daemon's greeting, or throws. */
  async hello(addr) {
    const r = await fetch(url(addr, '/v1/hello'), { signal: AbortSignal.timeout(4000) });
    if (!r.ok) throw new Error('not an aqua-sync daemon');
    return (await r.json()).message;
  },
};

function tell() {
  for (const fn of listeners) fn();
}

async function send(which, addr, key, id, body) {
  status[which] = { state: 'sending' };
  tell();
  try {
    const r = await fetch(url(addr, `/v1/paintings/${encodeURIComponent(id)}`), {
      method: 'POST',
      headers: { 'Content-Type': 'application/octet-stream', 'X-Aqua-Key': key },
      body,
      signal: AbortSignal.timeout(60_000),
    });
    if (r.status === 401) throw new Error('wrong sync key');
    if (!r.ok) throw new Error(`refused (${r.status})`);
    status[which] = { state: 'done', at: Date.now() };
  } catch (e) {
    const offline = e instanceof TypeError || e?.name === 'TimeoutError';
    status[which] = { state: 'failed', message: offline ? `${addr}: no answer` : String(e.message ?? e) };
  }
  tell();
}

export function url(addr, path) {
  const a = addr.trim().replace(/^https?:\/\//, '').replace(/\/+$/, '');
  const port = /:\d+$/.test(a) && !a.endsWith(']');
  return `http://${port ? a : `${a}:${PORT}`}${path}`;
}

/** "AQS1", then per file: u16 name length, name, u32 data length, data (little endian). */
export function encode(files) {
  const enc = new TextEncoder();
  // meta.json last, as when saving.
  const names = Object.keys(files).sort((a, b) => (a === 'meta.json') - (b === 'meta.json'));
  const parts = names.map((n) => [enc.encode(n), files[n]]);
  const out = new Uint8Array(4 + parts.reduce((s, [n, d]) => s + 6 + n.length + d.length, 0));
  const dv = new DataView(out.buffer);
  out.set(enc.encode('AQS1'));
  let at = 4;
  for (const [n, d] of parts) {
    dv.setUint16(at, n.length, true);
    out.set(n, at + 2);
    at += 2 + n.length;
    dv.setUint32(at, d.length, true);
    out.set(d, at + 4);
    at += 4 + d.length;
  }
  return out;
}
