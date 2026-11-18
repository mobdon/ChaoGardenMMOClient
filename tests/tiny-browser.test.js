import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createServer } from 'vite';
import { readFile, mkdir } from 'node:fs/promises';

// Optional presentation/protocol fixture suite. This does not simulate the Rust
// game rules or prove server persistence. Run with an installed Playwright:
// TINY_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs
// TINY_BROWSER_CHANNEL=chrome node --test tests/tiny-browser.test.js
let playwright;
try {
  playwright = await import(process.env.TINY_PLAYWRIGHT_MODULE
    ? pathToFileURL(process.env.TINY_PLAYWRIGHT_MODULE).href : 'playwright');
} catch (error) {
  if (process.env.TINY_PLAYWRIGHT_MODULE) throw error;
  // The ordinary source tests do not require a browser installation.
}

const token = 'a'.repeat(64);
const profile = {
  id: 'fixture-chao', name: 'Fixture Chao', form: 'chaos', alignment: -100,
  rings: 42, x: 0, z: 0, energy: 80, hunger: 70, happiness: 60, watch: null,
  stats: Object.fromEntries(['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence']
    .map(name => [name, { level: 12, points: 1234, grade: name === 'luck' || name === 'intelligence' ? 90 : 4 }])),
};
const care = { rulesVersion: 'adapted-care-v1', revision: 1, inventory: { 'garden-fruit': 2 },
  eligibility: { canCare: true, effectReadyAt: null }, catalog: { enabled: false, items: [] } };
const views = {
  memory: { phase: 'choose', columns: 6, matches: 0, mistakes: 0, rings: 0, trip_available: true,
    cards: Array.from({ length: 24 }, (_, index) => ({ occupied: index < 14, matched: false, face: null, value: 1 })) },
  rock_paper_scissors: { remaining_ms: 30000, targets: [], shots: [], cards: ['rock', 'paper', 'scissors'], selected: 0, replacements: 3, rings: 0 },
  bounce: { level: 1, lives: 3, earned_rings: 0, rings: [], balloons: [], spring_x: 50, chao_x: 50, chao_y: 40,
    compression_ms: 0, waiting: true, boost_level: 0 },
  ball: { stage: 1, cleared_balls: 0, rings: 0, columns: 8, grid: [], shots: [], player_x: 3,
    embedded_rings: [], falling_rings: [] },
  search: { remaining_ms: 60000, found: 0, request_name: 'Napping Chao', player_x: 50, player_y: 50,
    entrance_x: 50, entrance_y: 95, chao: [{ id: 3, x: 52, y: 51, activity: 'napping', terrain: 'grass' }],
    orbs: [{ id: 2, x: 49, y: 48 }], hints: [], carrying: null },
};

test('Tiny Garden phone presentation and input fixtures', { skip: !playwright, timeout: 90000 }, async t => {
  // Omit application proxying so this isolated suite can never reach live saves.
  const frontend = await createServer({ configFile: false, root: fileURLToPath(new URL('../', import.meta.url)),
    logLevel: 'silent', server: { host: '127.0.0.1', port: 0, strictPort: false, open: false } });
  let browser;
  try {
    await frontend.listen();
    browser = await playwright.chromium.launch({ headless: true,
      ...(process.env.TINY_BROWSER_CHANNEL ? { channel: process.env.TINY_BROWSER_CHANNEL } : {}) });
    const base = `http://127.0.0.1:${frontend.httpServer.address().port}`;

    async function fixture(viewport = { width: 390, height: 844 }, { v2 = false, foreignLease = false } = {}) {
      const context = await browser.newContext({ viewport, hasTouch: true, isMobile: true, reducedMotion: 'reduce' });
      const page = await context.newPage();
      page.setDefaultTimeout(5000);
      const errors = [];
      const accountRequests = [];
      let identity = { schemaVersion: 2, accountId: 'fixture-account', chaoId: profile.id,
        deviceId: 'fixture-device', revision: 1, devices: [{ id: 'fixture-device', kind: 'desktop', revoked: false }],
        lease: foreignLease ? { id: 'foreign-lease', deviceId: 'another-device', activity: 'tiny', expiresAt: 60000, lastSequence: 0 } : null };
      page.on('pageerror', error => errors.push(error.message));
      await page.route('**/api/**', async route => {
        const path = new URL(route.request().url()).pathname;
        if (path === '/api/status') return route.fulfill({ status: 200, contentType: 'application/json',
          body: JSON.stringify({ ok: true, tinyGarden: { games: Object.keys(views), rules: 'adapted' },
            ...(v2 ? { accounts: { version: 2, activityLeases: true, intentSequence: true, authentication: ['websocket-first-frame'] } } : {}) }) });
        if (path === '/api/session') return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ token, player: profile }) });
        if (path === '/api/account' || path === '/api/lease') {
          assert.equal(route.request().headers().authorization, `Bearer ${token}`);
          accountRequests.push({ path, method: route.request().method(), body: route.request().postDataJSON() });
          if (path === '/api/lease') {
            const body = route.request().postDataJSON();
            if (foreignLease) return route.fulfill({ status: 409, contentType: 'application/json', body: JSON.stringify({ error: 'activity_conflict' }) });
            identity = { ...identity, revision: identity.revision + 1,
              lease: body.operation === 'release' ? null : { id: 'fixture-lease', deviceId: identity.deviceId,
                activity: body.activity || identity.lease?.activity || 'tiny', expiresAt: 60000, lastSequence: 0 } };
            return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ identity }) });
          }
          return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ identity,
            capabilities: { version: 2 }, snapshot: { type: 'snapshot', protocolVersion: 2, self: profile, identity } }) });
        }
        const asset = path.slice('/api/assets/'.length);
        if (path.startsWith('/api/assets/') && /^(tiny-(neutral|hero|dark)-chaos|chao-flame|chao-stat-(panel|font)|hub-grass|emerald-(red|blue|green|cyan|purple|yellow|white))\.png$/.test(asset)) {
          try {
            const directory = process.env.TINY_NATIVE_ASSET_DIR
              ? pathToFileURL(`${process.env.TINY_NATIVE_ASSET_DIR}/`) : new URL('../../../imports/', import.meta.url);
            const body = await readFile(new URL(asset === 'hub-grass.png' ? 'nativehubgrass.png' : asset, directory));
            return route.fulfill({ status: 200, contentType: 'image/png', body });
          } catch { /* Native art is private and is not required by fixture tests. */ }
        }
        return route.fulfill({ status: 404, body: 'Fixture assets omitted' });
      });
      await page.route('https://fonts.googleapis.com/**', route => route.abort());
      await page.route('https://fonts.gstatic.com/**', route => route.abort());
      await page.addInitScript(({ token }) => {
        localStorage.setItem('chao-garden-session-v1', token);
        window.__tinySent = [];
        window.__tinySockets = [];
        class FixtureSocket {
          static OPEN = 1;
          constructor(url) {
            this.readyState = 0;
            this.url = String(url);
            if (new URL(this.url, location.href).pathname === '/ws') window.__tinySockets.push(this);
            setTimeout(() => { this.readyState = 1; this.onopen?.(); }, 10);
          }
          send(raw) { window.__tinySent.push(JSON.parse(raw)); }
          receive(data) { this.onmessage?.({ data: JSON.stringify(data) }); }
          close(code = 1000) { this.readyState = 3; this.onclose?.({ code }); }
        }
        window.WebSocket = FixtureSocket;
      }, { token });
      await page.goto(`${base}/tiny.html`);
      await page.waitForFunction(() => window.__tinySockets?.at(-1)?.readyState === 1);
      async function snapshot(gameId = null, extra = {}) {
        await page.evaluate(({ profile, gameId, view, extra, v2, identity }) => {
          window.__tinySockets.at(-1).receive({ type: 'snapshot', selfId: profile.id, players: [profile],
            tinyGame: gameId ? { id: `fixture-${gameId}`, gameId, phase: 'playing', view: { game: gameId, state: view }, result: null } : null,
            ...(v2 ? { protocolVersion: 2, self: profile, identity } : {}),
            ...extra });
        }, { profile, gameId, view: views[gameId], extra, v2, identity });
      }
      async function screen(name) {
        const nav = page.locator(`[data-screen="${name}"]`);
        if (await nav.count()) await nav.click();
      }
      async function sent() { return page.evaluate(() => window.__tinySent); }
      async function clearSent() { await page.evaluate(() => { window.__tinySent = []; }); }
      async function touch(selector, end = true) {
        const locator = page.locator(selector).first();
        await locator.scrollIntoViewIfNeeded();
        const rect = await locator.boundingBox();
        assert.ok(rect, `${selector} is visible`);
        const cdp = await context.newCDPSession(page);
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }] });
        if (end) { await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] }); await cdp.detach(); }
        return cdp;
      }
      return { page, context, snapshot, screen, sent, clearSent, touch, errors, accountRequests, identity: () => identity };
    }

    await t.test('socket open alone cannot enable actions; all seven stats use the canonical snapshot', async () => {
      const f = await fixture();
      try {
        await f.screen('games');
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        await f.snapshot();
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), false);
        await f.screen('stats');
        assert.equal(await f.page.locator('#tiny-stats .stat-row').count(), 7);
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    for (const [gameId, selector, expected] of [
      ['memory', '.memory-card:not(.empty)', { kind: 'memory_flip', cell: 0 }],
      ['rock_paper_scissors', '[data-rps-slot="2"]', { kind: 'rps_select', slot: 2 }],
      ['bounce', '#tiny-spring', { kind: 'bounce_spring' }],
      ['ball', '.shot-green', { kind: 'ball_shoot', color: 'green' }],
      ['search', '#tiny-action-controls button:first-child', { kind: 'search_pick_up', chao: 3 }],
    ]) {
      await t.test(`${gameId} touch sends a bounded session intent`, async () => {
        const f = await fixture();
        try {
          await f.snapshot(); await f.screen('games');
          await f.page.locator(`#tiny-games button[data-game="${gameId}"]`).click();
          await f.page.locator('#tiny-start').click();
          assert.ok((await f.sent()).some(message => message.action === 'tiny-start' && message.gameId === gameId));
          assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
          await f.snapshot(gameId); await f.clearSent();
          await f.touch(selector);
          const sent = await f.sent();
          assert.ok(sent.some(message => message.type === 'tiny-input' && message.sessionId === `fixture-${gameId}`
            && JSON.stringify(message.input) === JSON.stringify(expected)), JSON.stringify(sent));
          assert.ok(sent.every(message => !('score' in message) && !('reward' in message) && !('stats' in message)));
          assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
          const secondary = {
            memory: ['#tiny-trip', { kind: 'memory_trip' }],
            rock_paper_scissors: ['#tiny-action-controls>.emphasized', { kind: 'rps_shoot' }],
            ball: ['.shot-red', { kind: 'ball_shoot', color: 'red' }],
            search: ['#tiny-action-controls button:nth-child(2)', { kind: 'search_hint', orb: 2 }],
          }[gameId];
          if (secondary) {
            await f.touch(secondary[0]);
            assert.ok((await f.sent()).some(message => JSON.stringify(message.input) === JSON.stringify(secondary[1])));
          }
          if (gameId === 'search') {
            await f.touch('#tiny-action-controls button:nth-child(3)');
            assert.ok((await f.sent()).some(message => message.input?.kind === 'search_return'));
          }
          await f.page.locator('#tiny-stop').click();
          assert.ok((await f.sent()).some(message => message.action === 'tiny-stop' && message.sessionId === `fixture-${gameId}`));
          assert.equal(await f.page.locator('#tiny-stop').isDisabled(), true);
          await f.snapshot();
          assert.equal(await f.page.locator('#tiny-start').isDisabled(), false);
          assert.deepEqual(f.errors, []);
        } finally { await f.context.close(); }
      });
    }

    await t.test('touch cancellation releases movement and disconnected controls send nothing', async () => {
      const f = await fixture();
      try {
        await f.snapshot('search'); await f.screen('games'); await f.clearSent();
        const cdp = await f.touch('[data-move="1,0"]', false);
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
        await cdp.detach();
        let inputs = (await f.sent()).filter(message => message.type === 'tiny-input');
        assert.ok(inputs.some(message => message.input.kind === 'move' && message.input.x === 1));
        assert.ok(inputs.some(message => message.input.kind === 'move' && message.input.x === 0 && message.input.y === 0));
        await f.page.evaluate(() => window.__tinySockets.at(-1).close(1008));
        await f.clearSent();
        await f.page.keyboard.press('ArrowRight');
        inputs = (await f.sent()).filter(message => message.type === 'tiny-input');
        assert.deepEqual(inputs, []);
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('a result needs saved true and cannot locally increase the wallet', async () => {
      const f = await fixture();
      try {
        await f.snapshot('memory'); await f.screen('games');
        const emit = async saved => f.page.evaluate(saved => window.__tinySockets.at(-1).receive({ type: 'event',
          kind: 'tiny-game-result', id: 'fixture-memory', gameId: 'memory', rings: 88, completed: true, saved }), saved);
        await emit(false);
        assert.equal(await f.page.locator('#tiny-result').isHidden(), true);
        await emit('true');
        assert.equal(await f.page.locator('#tiny-result').isHidden(), true);
        await emit(true);
        assert.equal(await f.page.locator('#tiny-result').isHidden(), false);
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
        await f.snapshot(null, { players: [{ ...profile, rings: 130 }] });
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '130');
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('reconnect waits for canonical state before re-enabling play', async () => {
      const f = await fixture();
      try {
        await f.snapshot(); await f.screen('games');
        await f.page.evaluate(() => window.__tinySockets.at(-1).close(1006));
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        await f.page.waitForFunction(() => window.__tinySockets.length === 2 && window.__tinySockets.at(-1).readyState === 1);
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        await f.snapshot();
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), false);
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('v2 authenticates in the first frame and leaves a foreign activity lease untouched', async () => {
      const f = await fixture(undefined, { v2: true, foreignLease: true });
      try {
        await f.snapshot(); await f.screen('games');
        assert.equal(await f.page.evaluate(() => new URL(window.__tinySockets.at(-1).url).search), '');
        assert.deepEqual((await f.sent())[0], { type: 'authenticate', token });
        assert.ok(!f.page.url().includes(token));
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        assert.equal(f.accountRequests.filter(request => request.path === '/api/lease').length, 0);
        const response = f.page.waitForResponse('**/api/lease');
        await f.page.locator('#tiny-take-control').click();
        await response;
        assert.equal(f.accountRequests.filter(request => request.path === '/api/lease').length, 1);
        assert.equal(f.accountRequests.find(request => request.path === '/api/lease').body.operation, 'acquire');
        assert.equal(await f.page.locator('#tiny-start').isDisabled(), true);
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('v2 canvas tap sends Spring once and waits for the matching durable input acknowledgement', async () => {
      const f = await fixture(undefined, { v2: true });
      try {
        await f.snapshot('bounce'); await f.screen('games'); await f.clearSent();
        await f.touch('#tiny-canvas');
        await f.page.waitForFunction(() => window.__tinySent.some(message => message.type === 'tiny-input'));
        let inputs = (await f.sent()).filter(message => message.type === 'tiny-input');
        assert.equal(inputs.length, 1);
        assert.deepEqual(inputs[0].input, { kind: 'bounce_spring' });
        assert.equal(inputs[0].leaseId, 'fixture-lease');
        assert.equal(inputs[0].expectedRevision, 2);
        assert.equal(inputs[0].sequence, 1);
        assert.equal(await f.page.locator('#tiny-spring').isDisabled(), true);
        await f.page.evaluate(identity => window.__tinySockets.at(-1).receive({ type: 'event', kind: 'intent-accepted',
          protocolVersion: 2, sequence: 0, identity }), f.identity());
        assert.equal(await f.page.locator('#tiny-spring').isDisabled(), true);
        const acknowledged = { ...f.identity(), revision: 3, lease: { ...f.identity().lease, lastSequence: 1 } };
        await f.page.evaluate(identity => window.__tinySockets.at(-1).receive({ type: 'event', kind: 'intent-accepted',
          protocolVersion: 2, sequence: 1, identity }), acknowledged);
        await f.page.waitForFunction(() => !document.querySelector('#tiny-spring').disabled);
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('explicit Save waits for acknowledgement and rejects a failed transaction', async () => {
      const f = await fixture();
      try {
        await f.snapshot(); await f.page.locator('#tiny-save').click();
        assert.match(await f.page.locator('#tiny-save-note').textContent(), /Waiting for the server/);
        assert.equal(await f.page.locator('#tiny-save').isDisabled(), true);
        await f.page.evaluate(() => window.__tinySockets.at(-1).receive({ type: 'notice', kind: 'error', text: 'Fixture disk unavailable' }));
        assert.doesNotMatch(await f.page.locator('#tiny-save-note').textContent(), /Save confirmed/);
        assert.equal(await f.page.locator('#tiny-save').isDisabled(), false);
        await f.page.locator('#tiny-save').click();
        await f.page.evaluate(playerId => window.__tinySockets.at(-1).receive({ type: 'event', kind: 'saved', playerId }), profile.id);
        assert.match(await f.page.locator('#tiny-save-note').textContent(), /Save confirmed/);
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('care retries the same request and waits for the canonical inventory revision', async () => {
      const f = await fixture();
      try {
        await f.snapshot(null, { care, serverTime: 1000 }); await f.screen('items'); await f.clearSent();
        await f.page.getByRole('button', { name: 'Use one', exact: true }).click();
        const request = (await f.sent()).find(message => message.type === 'care');
        assert.ok(request?.requestId);
        assert.deepEqual(request.intent, { kind: 'consume', itemId: 'garden-fruit' });
        assert.equal(request.expectedRevision, 1);
        assert.match(await f.page.locator('#tiny-items-panel').textContent(), /Garden Fruit ×2/);
        await f.page.getByRole('button', { name: 'Retry same care request', exact: true }).click();
        assert.deepEqual((await f.sent()).filter(message => message.type === 'care'), [request, request]);
        await f.page.evaluate(requestId => window.__tinySockets.at(-1).receive({ type: 'event', kind: 'care-result',
          rulesVersion: 'adapted-care-v1', requestId, accepted: true, revision: 2 }), request.requestId);
        assert.equal(await f.page.getByRole('button', { name: 'Use one', exact: true }).isDisabled(), true);
        assert.match(await f.page.locator('#tiny-items-panel').textContent(), /Garden Fruit ×2/);
        await f.snapshot(null, { care: { ...care, revision: 2, inventory: { 'garden-fruit': 1 } }, serverTime: 1000 });
        assert.match(await f.page.locator('#tiny-items-panel').textContent(), /Garden Fruit ×1/);
        assert.equal(await f.page.getByRole('button', { name: 'Use one', exact: true }).isDisabled(), false);
        assert.equal(await f.page.locator('#tiny-rings').textContent(), '42');
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('v2 care stays unavailable until its ownership contract is integrated', async () => {
      const f = await fixture(undefined, { v2: true });
      try {
        await f.snapshot(null, { care, serverTime: 1000 }); await f.screen('items'); await f.clearSent();
        assert.equal(await f.page.getByRole('button', { name: 'Use one', exact: true }).isDisabled(), true);
        assert.equal(await f.page.getByRole('button', { name: 'Pet my Chao', exact: true }).isDisabled(), true);
        assert.deepEqual(await f.sent(), []);
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    await t.test('native Chaos identities keep distinct atlases in the phone garden', async () => {
      const f = await fixture();
      try {
        const output = new URL('../.local/screenshots/', import.meta.url);
        await mkdir(output, { recursive: true });
        for (const variant of ['neutral', 'hero', 'dark']) {
          await f.snapshot(null, { players: [{ ...profile, form: `${variant}_chaos`, alignment: variant === 'dark' ? 100 : -100 }] });
          await f.screen('garden');
          assert.equal(await f.page.locator('#tiny-portrait').getAttribute('data-variant'), variant);
          await f.page.waitForTimeout(80);
          await f.page.screenshot({ path: fileURLToPath(new URL(`tiny-phone-${variant}.png`, output)), fullPage: true });
        }
        for (const screen of ['stats', 'training', 'games', 'items']) {
          await f.snapshot(screen === 'games' ? 'memory' : null, { care, serverTime: 1000 });
          await f.screen(screen);
          await f.page.screenshot({ path: fileURLToPath(new URL(`tiny-phone-${screen}.png`, output)), fullPage: true });
        }
        assert.deepEqual(f.errors, []);
      } finally { await f.context.close(); }
    });

    for (const width of [320, 390, 430, 844]) {
      await t.test(`${width}px phone layout keeps the page within its viewport`, async () => {
        const f = await fixture({ width, height: width === 844 ? 390 : 844 });
        try {
          await f.snapshot();
          for (const screen of ['garden', 'stats', 'training', 'games', 'items']) {
            await f.screen(screen);
            assert.ok(await f.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `${screen} overflow at ${width}px`);
          }
          assert.deepEqual(f.errors, []);
        } finally { await f.context.close(); }
      });
    }
  } finally { await browser?.close(); await frontend.close(); }
});
