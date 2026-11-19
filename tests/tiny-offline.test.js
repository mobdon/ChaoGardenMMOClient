import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { readFile } from 'node:fs/promises';

const worker = await readFile(new URL('../public/tiny-sw.js', import.meta.url), 'utf8');

function harness() {
  const listeners = new Map(), saved = new Map(), requests = [];
  let offline = false;
  const cache = { addAll: async paths => requests.push(...paths), put: async (request, response) => saved.set(request.url, response), match: async request => saved.get(request.url) };
  vm.runInNewContext(worker, { URL, self: { location: { origin: 'https://garden.test' }, addEventListener: (kind, callback) => listeners.set(kind, callback) },
    caches: { open: async () => cache, keys: async () => [], delete: async () => true },
    fetch: async request => { if (offline) throw Error('offline'); return { ok: true, type: 'basic', clone() { return this; }, url: request.url }; } });
  return { listeners, requests, saved, goOffline() { offline = true; },
    load(path, method = 'GET') { let response; listeners.get('fetch')({ request: { url: new URL(path, 'https://garden.test').href, method }, respondWith(value) { response = value; } }); return response; } };
}

test('Tiny offline shell never intercepts auth, API state, mutations or credential URLs', () => {
  const h = harness();
  for (const url of ['/api/account', '/api/session', '/api/intent', '/api/lease', '/api/pairing/claim', '/api/status', '/ws?token=secret', '/tiny.html?token=secret', '/api/assets/unknown.png', 'https://external.test/assets/example.js']) assert.equal(h.load(url), undefined, url);
  assert.equal(h.load('/tiny.html', 'POST'), undefined);
});

test('Tiny offline shell retains approved presentation and uses it only on network failure', async () => {
  const h = harness();
  const first = await h.load('/assets/tiny-version.js');
  await h.load('/api/assets/tiny-dark-chaos.png');
  h.goOffline();
  assert.equal(await h.load('/assets/tiny-version.js'), first);
  assert.ok(await h.load('/api/assets/tiny-dark-chaos.png'));
  await assert.rejects(h.load('/assets/not-yet-cached.js'), /offline/);
  assert.equal(h.saved.size, 2);
});

test('install scope and entry carry no identity or credential', async () => {
  const manifest = JSON.parse(await readFile(new URL('../public/tiny.webmanifest', import.meta.url), 'utf8'));
  assert.equal(manifest.scope, '/tiny.html');
  assert.equal(manifest.start_url, '/tiny.html');
  assert.doesNotMatch(JSON.stringify(manifest), /token|secret|accountId/);
  const h = harness(); let install;
  h.listeners.get('install')({ waitUntil(value) { install = value; } }); await install;
  assert.deepEqual(h.requests, ['/tiny.html', '/tiny.webmanifest']);
});
