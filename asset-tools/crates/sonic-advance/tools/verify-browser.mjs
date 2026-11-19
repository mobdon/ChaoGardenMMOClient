// Exercise the shipped adapter in a separate Chromium test profile. Only the
// browser's local file input receives the ROM. Run with an installed Playwright
// core module and Chromium; the loopback server keeps its four static routes.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { createPreviewServer } from './serve-runtime.mjs';

const [rom, executable, modulePath] = process.argv.slice(2);
if (!rom || !executable) throw Error('Usage: node tools/verify-browser.mjs LOCAL_ROM CHROMIUM [PLAYWRIGHT_CORE_MODULE]');
const { chromium } = await import(modulePath ? pathToFileURL(resolve(modulePath)).href : 'playwright-core');
const receipt = JSON.parse(await readFile(new URL('../docs/RUNTIME-PARITY.json', import.meta.url)));
const initial = receipt.checkpointsMetadata[0];
const server = createPreviewServer();
await new Promise((done, reject) => server.once('error', reject).listen(0, '127.0.0.1', done));
const origin = 'http://127.0.0.1:' + server.address().port;
let browser;
try {
  browser = await chromium.launch({ executablePath: resolve(executable), headless: true });
  const context = await browser.newContext({ serviceWorkers: 'block' });
  const requests = [];
  const violations = [];
  const errors = [];
  const allowed = ['/', '/runtime.js', '/runtime.css', '/runtime.wasm'];
  await context.route('**/*', async route => {
    const req = route.request(), u = new URL(req.url());
    if (u.origin !== origin || !allowed.includes(u.pathname) || u.search || req.method() !== 'GET' || req.postData() !== null) {
      violations.push([u.pathname, req.method()]);
      return route.abort();
    }
    requests.push(u.pathname);
    return route.continue();
  });
  const page = await context.newPage();
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(origin);
  const file = page.locator('#rom-file');
  await page.waitForFunction(() => !document.querySelector('#rom-file').disabled);
  await file.setInputFiles({ name: 'wrong.gba', mimeType: 'application/octet-stream', buffer: Buffer.alloc(16) });
  await page.waitForFunction(() => document.querySelector('#status').classList.contains('error'));
  assert.equal(await page.locator('#pause').isDisabled(), true);
  await file.setInputFiles(resolve(rom));
  await page.waitForFunction(() => !document.querySelector('#pause').disabled, null, { timeout: 60000 });
  const button = page.locator('#pause');
  if (await button.textContent() === 'Resume') await button.click();
  await page.waitForFunction(() => document.querySelector('#action-value').textContent === 'Press Start');
  await page.locator('#stage').focus();
  await page.keyboard.down('Enter');
  await page.waitForFunction(() => document.querySelector('#action-value').textContent === 'mainMenuRequested');
  await page.keyboard.up('Enter');
  await page.locator('#practice-act').selectOption('0');
  await button.click();
  await page.waitForFunction(() => Number(document.querySelector('#frame-value').textContent) >= 16);
  await page.locator('#stage').focus();
  await page.keyboard.down('ArrowDown');
  await page.waitForFunction(() => document.querySelector('#action-value').textContent === 'Crouching');
  await page.keyboard.down('KeyZ');
  await page.waitForFunction(() => document.querySelector('#action-value').textContent === 'Charging');
  await page.keyboard.up('KeyZ');
  await page.keyboard.up('ArrowDown');
  await page.waitForFunction(() => parseFloat(document.querySelector('#position-value').textContent) > 110);
  await button.click();
  const stoppedFrame = Number(await page.locator('#frame-value').textContent());
  await page.waitForTimeout(120);
  assert.equal(Number(await page.locator('#frame-value').textContent()), stoppedFrame);
  await page.locator('#reset').click();
  assert.equal(await page.locator('#frame-value').textContent(), '0');
  assert.equal(await page.locator('#rings-value').textContent(), '0');
  const hash = await page.locator('#stage').evaluate(async canvas => {
    const rgba = canvas.getContext('2d').getImageData(0, 0, 240, 160).data;
    return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', rgba)), x => x.toString(16).padStart(2, '0')).join('');
  });
  assert.equal(hash, initial.rgbaSha256, 'reset canvas must equal native Rust frame zero');
  await page.locator('#practice-act').selectOption('1');
  assert.match(await page.locator('#stage-heading').textContent(), /Act 2/);
  assert.equal(await page.locator('#position-value').textContent(), '104.0, 684.0');
  await button.click();
  await page.waitForFunction(() => document.querySelector('#action-value').textContent === 'Standing');
  await page.locator('#stage').focus();
  await page.keyboard.down('ArrowRight');
  await page.waitForFunction(() => parseFloat(document.querySelector('#position-value').textContent) > 112);
  await page.keyboard.up('ArrowRight');
  await button.click();
  assert.deepEqual([...new Set(requests)].sort(), [...allowed].sort());
  assert.deepEqual(violations, []);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({result:'PASS', browser:browser.version(), stoppedFrame, resetRgba:hash, routes:[...new Set(requests)].sort()}));
} finally {
  try { await browser?.close(); } finally { await new Promise(done => server.close(done)); }
}
