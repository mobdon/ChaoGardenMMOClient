// Opt-in recorded-observation presentation replay. This starts a separate
// loopback Vite instance; it never connects identity, saves or game actions.
// Reports stay in the browser DOM. Original traces/assets remain local.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import baseConfig from '../vite.config.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
assert.ok(args.length === 2 && args[0] === '--trace' && args[1],
  'Usage: node scripts/verify-slope-browser-replay.mjs --trace .local/reviews/slope-down-after.json');
const tracePath = resolve(root, args[1]);
const localTrace = relative(resolve(root, '.local/reviews'), tracePath);
assert.ok(localTrace && !localTrace.startsWith('..') && !localTrace.includes(':') && localTrace.endsWith('.json'),
  'An explicitly supplied JSON trace inside .local/reviews is required.');
const traceBytes = await readFile(tracePath);
const source = JSON.parse(traceBytes.toString('utf8'));
assert.ok(Array.isArray(source) && source.length >= 2 && source.length <= 10000, 'Expected 2–10000 trace rows.');
const rows = source.map((row, index) => {
  assert.ok(row && typeof row === 'object' && ['x', 'y', 'z', 'gap'].every(key =>
    typeof row[key] === 'number' || typeof row[key] === 'string' && row[key].trim()), `Missing raw data in row ${index}.`);
  const result = Object.fromEntries(['x', 'y', 'z', 'gap'].map(key => [key, Number(row[key])]));
  assert.ok(Object.values(result).every(Number.isFinite), `Nonfinite coordinates/gap in row ${index}.`);
  assert.ok(Math.max(Math.abs(result.x), Math.abs(result.y), Math.abs(result.z)) < 10000
    && result.gap >= 0 && result.gap <= 10, `Out-of-range coordinates/gap in row ${index}.`);
  return result;
});
const trace = { kind: 'recorded-observation presentation replay',
  source: relative(root, tracePath).replaceAll('\\', '/'),
  sha256: createHash('sha256').update(traceBytes).digest('hex'),
  nominalRowSeconds: 1 / 30, changedSnapshotMilliseconds: 100, rows };

const html = `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1"><title>Recorded slope renderer replay</title>
<style>html,body{margin:0;width:100%;height:100%;background:#071823;color:#e4f2ff;font:13px/1.45 system-ui}main{display:grid;grid-template-columns:minmax(0,1fr) 420px;width:100%;height:100%}canvas{width:100%;height:100%;touch-action:none}aside{padding:18px;overflow:auto}h1{font-size:19px;margin-top:0}pre{white-space:pre-wrap;overflow-wrap:anywhere}button{padding:8px 14px;background:#17446d;color:white;border:1px solid #72b8e7;border-radius:5px}p{color:#a9c8dc}#report[data-result=PASS]{color:#8fe4b8}#report[data-result=FAIL]{color:#ffacac}</style>
</head><body><main><canvas id="garden" aria-label="Native Chao recorded slope replay"></canvas><aside>
<h1>Recorded slope renderer replay</h1><p>Recorded-observation presentation replay using the current browser renderer and native assets. This is not a live service round trip.</p>
<p>No identity, session, websocket, action, reward or save requests are made. Movement shown here comes only from the explicitly selected local trace.</p>
<button id="replay" disabled>Replay again</button><pre id="report" data-result="RUNNING">Loading current renderer…</pre><pre id="diagnostics"></pre>
</aside></main><script type="module" src="/__qa/slope-replay.mjs"></script></body></html>`;

const browserModule = String.raw`
import { createGarden } from '/src/scene.js';
const canvas = document.getElementById('garden');
const report = document.getElementById('report');
const diagnostics = document.getElementById('diagnostics');
const replayButton = document.getElementById('replay');
let garden, running = false, ignoredInputs = 0, ignoredActions = 0;
const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
const afterRender = async () => { await frame(); await frame(); };
const waitUntil = async deadline => { while (performance.now() < deadline) await frame(); };
const finite = value => Number.isFinite(Number(value)) ? Number(value) : null;
function readDiagnostics(index, started, submitted) {
  const d = canvas.dataset;
  const screen = (d.headScreenPosition || '').split(',').map(Number);
  return { index, elapsedMilliseconds: performance.now() - started, submitted,
    screen: screen.length === 2 && screen.every(Number.isFinite) ? screen : null,
    headStabilized: d.headStabilized === 'true', focusErrorY: finite(d.cameraFocusErrorY),
    actorY: finite(d.renderActorY), grounded: d.observedGrounded === 'true',
    airGap: finite(d.observedAirGap), skateState: d.skateState,
    blend: finite(d.skateBlend), speed: finite(d.skateSpeed), jets: finite(d.shoeExhaust),
    effects: d.nativeShoeEffects || '', nativeRun: d.nativeRun || '',
    footwearSockets: d.footwearSockets || '', chaoAsset: d.chaoAsset || '' };
}
function assetsReady() {
  const d = canvas.dataset;
  return d.chaoAsset === 'loaded' && d.nativeRun === 'native119-loaded'
    && d.footwearSockets === '006,013' && d.headStabilized === 'true'
    && !['unavailable', undefined].includes(d.skateState);
}
async function run() {
  if (running) return;
  running = true; replayButton.disabled = true; report.dataset.result = 'RUNNING';
  report.textContent = 'Loading native rig, run motion, skate source and shoes…';
  const trace = await fetch('/__qa/slope-trace.json', { cache: 'no-store', credentials: 'omit' }).then(response => {
    if (!response.ok) throw new Error('Trace endpoint failed.'); return response.json();
  });
  garden?.dispose();
  garden = createGarden(canvas, { onInput() { ignoredInputs++; }, onAction() { ignoredActions++; } });
  let serverTime = 100000, last = null, snapshotCount = 0;
  const submit = row => {
    serverTime += 100; snapshotCount++;
    garden.setSnapshot({ selfId: 'qa-recorded-slope', zone: 'garden', serverTime,
      players: [{ id: 'qa-recorded-slope', name: 'Recorded slope QA', zone: 'garden',
        x: row.x, y: row.y, z: row.z, groundY: row.y - row.gap, jumpOffset: row.gap,
        facing: Math.PI, form: 'dark-chaos', alignment: -100, admin: true,
        gear: { footwear: 'shadow-air-shoes', hands: null }, vehicle: null, hp: 100 }],
      rings: [], enemies: [] });
    last = row;
  };
  const warmRow = { ...trace.rows[0], gap: 0 };
  const loadStarted = performance.now(); let readySince = null, nextWarm = 0;
  while (performance.now() - loadStarted < 30000) {
    if (performance.now() >= nextWarm) { submit(warmRow); nextWarm = performance.now() + 100; }
    await afterRender();
    const ready = assetsReady();
    if (ready && readySince === null) readySince = performance.now();
    if (!ready) readySince = null;
    if (readySince !== null && performance.now() - readySince >= 2000) break;
  }
  if (readySince === null || performance.now() - readySince < 2000) {
    garden.setPaused(true); throw new Error('Native assets did not become ready and warm for two seconds: ' + JSON.stringify({ ...canvas.dataset }));
  }
  // The stationary warmup settles the PI-facing turn. Only changed raw XYZ
  // samples receive a new 100ms observation; duplicate trace rows merely draw.
  const started = performance.now(), samples = [];
  const replaySnapshotStart = snapshotCount;
  for (let index = 0; index < trace.rows.length; index++) {
    await waitUntil(started + index * 1000 / 30);
    const row = trace.rows[index];
    const changed = !last || ['x', 'y', 'z'].some(key => row[key] !== last[key]);
    if (changed) submit(row);
    await afterRender();
    const sample = readDiagnostics(index, started, changed); samples.push(sample);
    diagnostics.textContent = JSON.stringify(sample, null, 2);
    report.textContent = 'Replaying post-turn row ' + (index + 1) + '/' + trace.rows.length;
  }
  garden.setPaused(true);
  const heads = samples.filter(sample => sample.screen && sample.headStabilized);
  const span = axis => heads.length ? Math.max(...heads.map(sample => sample.screen[axis])) - Math.min(...heads.map(sample => sample.screen[axis])) : Infinity;
  const screenRange = { x: span(0), y: span(1) };
  const air = samples.filter(sample => sample.airGap > .001);
  const coasting = air.filter(sample => sample.skateState === 'coasting' && sample.blend >= .99 && sample.jets === 0);
  const coastSnapshots = new Set(coasting.map(sample => trace.rows[sample.index].z)).size;
  const airJetFaults = air.filter(sample => sample.jets !== 0);
  const pass = assetsReady() && heads.length === samples.length && air.length > 0
    && coasting.length > 0 && coastSnapshots > 0 && airJetFaults.length === 0
    && screenRange.x < .01 && screenRange.y < .01;
  const result = { result: pass ? 'PASS' : 'FAIL', kind: trace.kind,
    limitation: 'Recorded-observation presentation replay; not a real service round trip or authority validation.',
    trace: trace.source, traceSha256: trace.sha256,
    viewport: { width: canvas.width, height: canvas.height },
    nativeAssets: { chao: canvas.dataset.chaoAsset, run: canvas.dataset.nativeRun,
      footwearSockets: canvas.dataset.footwearSockets, garden: canvas.dataset.nativeGarden },
    sampleCount: samples.length, changedSnapshots: snapshotCount - replaySnapshotStart,
    nominalRowSeconds: 1 / 30, changedSnapshotMilliseconds: 100,
    actualReplayMilliseconds: performance.now() - started,
    selection: 'All post-turn rows, after native assets warmed for two seconds at facing PI.',
    headScreenRangePixels: screenRange, headTolerancePixels: .01,
    positiveAirSamples: air.length, highBlendZeroJetCoastingSamples: coasting.length,
    distinctCoastingZSamples: coastSnapshots, airborneJetFaults: airJetFaults.length,
    maxObservedAirGap: Math.max(...samples.map(sample => sample.airGap || 0)),
    maxAbsCameraFocusErrorY: Math.max(...samples.map(sample => Math.abs(sample.focusErrorY || 0))),
    ignoredPresentationInputCallbacks: ignoredInputs, ignoredActionCallbacks: ignoredActions,
    paused: true, samples };
  report.dataset.result = result.result;
  report.textContent = JSON.stringify(result, null, 2);
  diagnostics.textContent = 'Replay finished. Result and all samples remain in this page DOM only.';
  running = false; replayButton.disabled = false;
}
function fail(error) {
  garden?.setPaused(true); running = false; replayButton.disabled = false;
  report.dataset.result = 'FAIL'; report.textContent = String(error?.stack || error);
}
replayButton.addEventListener('click', () => run().catch(fail));
run().catch(fail);
`;

function qaPlugin() {
  return { name: 'recorded-slope-presentation-replay',
    resolveId(id) { if (id === '/__qa/slope-replay.mjs') return '\0qa-slope-replay'; },
    load(id) { if (id === '\0qa-slope-replay') return browserModule; },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        let pathname;
        try {
          pathname = new URL(request.url || '/', 'http://127.0.0.1').pathname;
          for (let pass = 0; pass < 3; pass++) pathname = decodeURIComponent(pathname);
        } catch { response.writeHead(400).end(); return; }
        if (pathname.startsWith('/api/') || pathname === '/api' || pathname.startsWith('/ws')) {
          if (!/^\/api\/assets\/[a-z0-9_-]+\.(?:glb|png|json)$/i.test(pathname)
            || !['GET', 'HEAD'].includes(request.method)) {
            response.writeHead(403).end('Replay permits read-only native asset requests only.'); return;
          }
          delete request.headers.cookie; delete request.headers.authorization;
          return next();
        }
        if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405).end(); return; }
        if (pathname === '/' || pathname === '/__qa/slope-replay') {
          response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
          response.end(request.method === 'HEAD' ? undefined : html); return;
        }
        if (pathname === '/__qa/slope-trace.json') {
          response.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' });
          response.end(request.method === 'HEAD' ? undefined : JSON.stringify(trace)); return;
        }
        next();
      });
    },
  };
}

const server = await createServer({ ...baseConfig, configFile: false, root,
  plugins: [qaPlugin(), ...(baseConfig.plugins || [])],
  server: { ...baseConfig.server, host: '127.0.0.1', port: 0, strictPort: false, open: false, hmr: false,
    proxy: { '/api/assets/': { target: 'http://127.0.0.1:3001',
      configure(proxy) {
        proxy.on('proxyReq', outgoing => { outgoing.removeHeader('cookie'); outgoing.removeHeader('authorization'); });
        proxy.on('proxyRes', incoming => { delete incoming.headers['set-cookie']; });
      },
    } },
  },
});
await server.listen();
const address = server.httpServer.address();
console.log(`Recorded-observation presentation replay: http://127.0.0.1:${address.port}/__qa/slope-replay`);
console.log(`Trace: ${trace.source} (${rows.length} rows, sha256 ${trace.sha256})`);
console.log('GET/HEAD assets only → existing Rust port3001. No auth/session/WS/actions/saves. Browser DOM reports only.');
console.log('Press Enter, close stdin, or send SIGINT/SIGTERM to stop this isolated instance.');
let closing = false;
async function close() { if (closing) return; closing = true; await server.close(); process.exit(0); }
process.stdin.resume();
process.stdin.once('data', close);
process.stdin.once('end', close);
process.once('SIGINT', close);
process.once('SIGTERM', close);
