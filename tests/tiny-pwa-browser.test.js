import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { randomBytes } from 'node:crypto';

// Optional production-shell acceptance. All APIs and credentials below belong
// to an isolated in-process fixture; this never proxies to the game preview.
let playwright;
try {
  playwright = await import(process.env.TINY_PLAYWRIGHT_MODULE
    ? pathToFileURL(process.env.TINY_PLAYWRIGHT_MODULE).href : 'playwright');
} catch (error) {
  if (process.env.TINY_PLAYWRIGHT_MODULE) throw error;
  // Browser installation remains optional for ordinary source tests.
}
const dist = new URL('../dist/', import.meta.url);
let built = false;
try { built = (await stat(new URL('tiny.html', dist))).isFile(); } catch { /* Run npm run build first. */ }
if (process.env.TINY_PLAYWRIGHT_MODULE && !built) throw new Error('Production PWA acceptance requires npm run build before the configured browser run.');

test('production Tiny Garden shell stays readable offline without caching account state', {
  skip: !playwright ? 'Set TINY_PLAYWRIGHT_MODULE to an installed Playwright.' : !built ? 'Build production dist first.' : false,
  timeout: 90000,
}, async t => {
  const token = randomBytes(32).toString('hex');
  const profile = { id: 'pwa-fixture-chao', name: 'Offline Garden Fixture', form: 'dark_chaos', alignment: -100,
    rings: 42, x: 0, z: 0, energy: 81, hunger: 72, happiness: 63, watch: null,
    stats: Object.fromEntries(['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence']
      .map(name => [name, { level: 12, points: 1234, grade: ['luck', 'intelligence'].includes(name) ? 90 : 4 }])) };
  const identity = { schemaVersion: 2, accountId: 'pwa-account', chaoId: profile.id, deviceId: 'pwa-device',
    revision: 7, lease: null, devices: [{ id: 'pwa-device', kind: 'android', revoked: false }] };
  const capabilities = { version: 2, activityLeases: true, intentSequence: true, httpIntents: true,
    durableTraining: false, authentication: ['bearer-header', 'websocket-first-frame'] };
  const snapshot = { type: 'snapshot', protocolVersion: 2, identity, self: profile,
    selfId: profile.id, players: [profile], serverTime: 100000, tinyGame: null, tinyResult: null };
  const apiRequests = [];
  const server = http.createServer(async (request, response) => {
    try {
      const pathname = new URL(request.url, 'http://fixture.invalid').pathname;
      const json = value => { response.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); response.end(JSON.stringify(value)); };
      if (pathname.startsWith('/api/')) {
        apiRequests.push({ method: request.method, pathname });
        if (pathname === '/api/status') return json({ ok: true, accounts: capabilities,
          tinyGarden: { games: ['memory', 'rock_paper_scissors', 'bounce', 'ball', 'search'], rules: 'adapted' } });
        if (pathname === '/api/account') {
          if (request.headers.authorization !== `Bearer ${token}`) { response.writeHead(401).end(); return; }
          return json({ identity, snapshot, capabilities });
        }
        // Deliberately successful sensitive fixture responses let the test prove
        // the worker excludes them even when an endpoint returns HTTP 200.
        if (pathname === '/api/session' || pathname === '/api/intent') return json({ fixture: true, saved: false });
        response.writeHead(404).end('Native fixture assets omitted.'); return;
      }
      if (pathname === '/') { response.writeHead(200, { 'Content-Type': 'text/html' }).end('<!doctype html><title>Desktop scope fixture</title>'); return; }
      if (!['/tiny.html', '/tiny-sw.js', '/tiny.webmanifest'].includes(pathname)
        && !/^\/assets\/[\w.-]+\.(?:js|css)$/.test(pathname)) { response.writeHead(404).end(); return; }
      const contents = await readFile(new URL(pathname.slice(1), dist));
      const type = pathname.endsWith('.html') ? 'text/html' : pathname.endsWith('.css') ? 'text/css'
        : pathname.endsWith('.webmanifest') ? 'application/manifest+json' : 'application/javascript';
      response.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-cache' }); response.end(contents);
    } catch { response.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  let browser;
  try {
    browser = await playwright.chromium.launch({ headless: true,
      ...(process.env.TINY_BROWSER_CHANNEL ? { channel: process.env.TINY_BROWSER_CHANNEL } : {}) });
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true,
      hasTouch: true, reducedMotion: 'reduce', serviceWorkers: 'allow' });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    // External fonts are presentation-only and irrelevant to offline authority.
    await context.route('https://fonts.googleapis.com/**', route => route.abort());
    await context.route('https://fonts.gstatic.com/**', route => route.abort());
    await page.addInitScript(({ token, snapshot }) => {
      localStorage.setItem('chao-garden-session-v1', token);
      window.__pwaSentTypes = [];
      class FixtureSocket {
        static OPEN = 1;
        constructor() { this.readyState = 0; setTimeout(() => { this.readyState = 1; this.onopen?.(); }, 10); }
        send(raw) {
          const message = JSON.parse(raw); window.__pwaSentTypes.push(message.type);
          if (message.type === 'authenticate') setTimeout(() => this.onmessage?.({ data: JSON.stringify(snapshot) }), 10);
        }
        close(code = 1000) { this.readyState = 3; this.onclose?.({ code }); }
      }
      window.WebSocket = FixtureSocket;
    }, { token, snapshot });

    await t.test('production worker controls Tiny only and caches the built dependency graph', async () => {
      await page.goto(`${origin}/tiny.html`);
      await page.waitForFunction(() => document.querySelector('#tiny-rings')?.textContent === '42');
      const registration = await page.evaluate(async () => {
        const registration = await navigator.serviceWorker.ready;
        return { scope: registration.scope, script: registration.active?.scriptURL };
      });
      assert.equal(registration.scope, `${origin}/tiny.html`);
      assert.equal(registration.script, `${origin}/tiny-sw.js`);
      await page.reload();
      await page.waitForFunction(() => !!navigator.serviceWorker.controller && document.querySelector('#tiny-rings')?.textContent === '42');
      const required = await page.evaluate(() => ['/tiny.html', '/tiny.webmanifest',
        ...[...document.querySelectorAll('script[src],link[rel="modulepreload"],link[rel="stylesheet"]')]
          .map(node => new URL(node.src || node.href).pathname).filter(path => path.startsWith('/assets/'))]);
      assert.ok(required.some(path => /\/assets\/tiny-[\w-]+\.js$/.test(path)), 'production Tiny entry is hashed');
      assert.ok(required.length >= 5, 'production shared JS and stylesheet dependencies are present');
      await page.waitForFunction(async required => {
        const cachesForOrigin = await caches.keys();
        const urls = (await Promise.all(cachesForOrigin.map(async name => (await (await caches.open(name)).keys()).map(request => new URL(request.url).pathname)))).flat();
        return required.every(path => urls.includes(path));
      }, required);
      const desktop = await context.newPage();
      await desktop.goto(origin);
      assert.equal(await desktop.evaluate(() => navigator.serviceWorker.controller), null);
      await desktop.close();
      assert.deepEqual(pageErrors, []);
    });

    await t.test('successful account, session and intent responses never enter Cache Storage', async () => {
      await page.evaluate(async token => {
        await fetch('/api/account', { headers: { Authorization: `Bearer ${token}` }, cache: 'no-store' });
        await fetch('/api/session', { cache: 'no-store' });
        await fetch('/api/intent', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ fixture: true }) });
      }, token);
      const entries = await page.evaluate(async () => {
        const result = [];
        for (const name of await caches.keys()) for (const request of await (await caches.open(name)).keys()) {
          result.push({ path: new URL(request.url).pathname, text: await (await (await caches.open(name)).match(request)).text() });
        }
        return result;
      });
      assert.equal(entries.some(entry => ['/api/account', '/api/status', '/api/session', '/api/intent'].includes(entry.path)), false);
      assert.equal(entries.some(entry => entry.text.includes(token)), false, 'device credentials never enter cached responses');
      assert.ok(apiRequests.some(request => request.pathname === '/api/account'));
      assert.ok(apiRequests.some(request => request.pathname === '/api/session'));
      assert.ok(apiRequests.some(request => request.pathname === '/api/intent'));
    });

    await t.test('offline reload shows last synced seven stats and cannot send or queue mutations', async () => {
      await context.setOffline(true);
      await page.reload({ waitUntil: 'domcontentloaded' });
      await page.waitForFunction(() => /Last synced/.test(document.querySelector('#tiny-sync-note')?.textContent || ''));
      assert.equal(await page.locator('#tiny-name').textContent(), profile.name);
      assert.equal(await page.locator('#tiny-rings').textContent(), '42');
      await page.locator('[data-screen="stats"]').click();
      assert.equal(await page.locator('#tiny-stats .stat-row').count(), 7);
      await page.locator('[data-screen="games"]').click();
      assert.equal(await page.locator('#tiny-start').isDisabled(), true);
      assert.equal(await page.locator('#tiny-save').isDisabled(), true);
      await page.keyboard.press('ArrowRight');
      await page.evaluate(() => document.querySelector('#tiny-start').dispatchEvent(new MouseEvent('click', { bubbles: true })));
      await page.locator('[data-screen="garden"]').click();
      assert.equal(await page.locator('#tiny-watch-toggle').isDisabled(), true);
      const stored = await page.evaluate(async () => ({
        sent: window.__pwaSentTypes,
        view: JSON.parse(localStorage.getItem('chao-tiny-view-v1')),
        syncTags: (await navigator.serviceWorker.ready).sync ? await (await navigator.serviceWorker.ready).sync.getTags() : [],
      }));
      assert.deepEqual(stored.sent, []);
      assert.deepEqual(stored.syncTags, []);
      assert.equal(stored.view.player.id, profile.id);
      assert.equal(stored.view.player.watch, undefined); assert.equal(stored.view.player.training, undefined);
      assert.equal(stored.view.player.token, undefined); assert.equal(stored.view.player.identity, undefined);
      assert.deepEqual(pageErrors, []);
      // Reconnection reconciles state without uploading any offline action.
      await context.setOffline(false);
      await page.locator('#tiny-reconnect').click();
      await page.waitForFunction(() => document.querySelector('#tiny-connection')?.dataset.status === 'live');
      const sentAfterReconnect = await page.evaluate(() => window.__pwaSentTypes);
      assert.ok(sentAfterReconnect.length >= 1);
      assert.ok(sentAfterReconnect.every(type => type === 'authenticate'), 'reconnect sends authentication only, with no offline mutation replay');
      assert.deepEqual(pageErrors, []);
    });
    await context.close();
  } finally {
    await browser?.close();
    await new Promise(resolve => server.close(resolve));
    server.closeAllConnections();
  }
});
