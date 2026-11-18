// Actual Rust-service acceptance using a fresh synthetic Chao. This never
// opens the live profile directory and never substitutes a mocked service.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { acceptWidgetAccountSnapshot, authenticateWidgetSocket, readWidgetObservation, widgetSocketUrl } from '../../src/widget-observer.js';

const { values } = parseArgs({ options: { server: { type: 'string' }, receipt: { type: 'string' } } });
if (!values.server) throw new Error('Use --server with the exact Rust accounts-v2 executable to test.');
if (typeof WebSocket !== 'function') throw new Error('This test requires Node with the real WebSocket API.');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const executable = resolve(values.server);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const serverSha256 = hash(await readFile(executable));
const fixtureRoot = join(root, '.local', 'verification');
await mkdir(fixtureRoot, { recursive: true });
const dataDirectory = await mkdtemp(join(fixtureRoot, 'widget-observer-'));
const receiptPath = values.receipt ? resolve(values.receipt) : join(dataDirectory, 'receipt.json');
const skills = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
const observerCalls = [], exitCodes = [];
let running, socket, stage = 'startup', authenticationFrames = 0;

async function startServer() {
  const child = spawn(executable, [], { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_DATA_DIR: dataDirectory,
      CHAO_OWNER_BOOTSTRAP: '0', CHAO_SHUTDOWN_STDIN: '1', CHAO_ALLOWED_ORIGINS: '' } });
  const exited = new Promise(done => {
    child.once('exit', (code, signal) => done({ code, signal }));
    child.once('error', () => done({ code: null, signal: null }));
  });
  child.stderr.on('data', () => {}); // Do not publish process/request diagnostics.
  let buffer = '', timer;
  try {
    const base = await new Promise((ready, reject) => {
      timer = setTimeout(() => reject(new Error('Isolated service startup deadline exceeded.')), 10000);
      child.once('error', () => reject(new Error('Isolated service could not start.')));
      child.once('exit', () => reject(new Error('Isolated service exited before readiness.')));
      child.stdout.on('data', bytes => {
        buffer = (buffer + bytes.toString()).slice(-8192);
        const match = buffer.match(/Chao Rust game server: (http:\/\/127\.0\.0\.1:\d+)/);
        if (match) ready(match[1]);
      });
    });
    return { child, exited, base };
  } catch (error) {
    child.kill(); await exited; throw error;
  } finally { clearTimeout(timer); }
}
async function stopServer(server) {
  if (!server) return;
  if (server.child.exitCode === null && server.child.signalCode === null) server.child.stdin.end('shutdown\n');
  let timer;
  try {
    const result = await Promise.race([server.exited, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('Isolated service shutdown deadline exceeded.')), 6000);
    })]);
    assert.ok(result.code === 0, 'isolated Rust service exits cleanly'); exitCodes.push(result.code);
  } catch (error) {
    server.child.kill(); await server.exited; throw error;
  } finally { clearTimeout(timer); }
}
async function request(path, options = {}, observer = false) {
  const call = { path, method: options.method || 'GET' };
  if (observer) {
    observerCalls.push(call);
    assert.ok(['/api/status', '/api/session', '/api/account'].includes(path), 'observer uses only agreed read/resume endpoints');
    if (path === '/api/session') assert.ok(JSON.parse(options.body).resumeOnly === true, 'observer cannot create a replacement Chao');
    else assert.ok(call.method === 'GET', 'observer sends no mutation to account endpoints');
  }
  return fetch(new URL(path, running.base), { ...options, signal: options.signal || AbortSignal.timeout(10000) });
}
async function account(token) {
  const response = await request('/api/account', { headers: { Authorization: `Bearer ${token}` } });
  assert.ok(response.status === 200, 'real bearer account read succeeds');
  return response.json();
}
function verifyProfile(player) {
  assert.ok(player && typeof player.id === 'string', 'canonical player identity is present');
  for (const skill of skills) {
    assert.ok(['level', 'points', 'grade'].every(field => Number.isFinite(player.stats?.[skill]?.[field])), `canonical ${skill} stat is present`);
  }
  assert.ok(Number.isFinite(player.rings), 'canonical wallet is present');
  assert.ok(['energy', 'hunger', 'happiness'].every(field => Number.isFinite(player[field])), 'canonical needs are present');
}
async function readSocketObservation(base, token, identity) {
  const url = widgetSocketUrl(new URL(base), 2, token);
  assert.ok(new URL(url).pathname === '/ws' && new URL(url).search === '', 'v2 observer URL has no query');
  socket = new WebSocket(url);
  let timer;
  try {
    return await new Promise((ready, reject) => {
      timer = setTimeout(() => reject(new Error('Authenticated snapshot deadline exceeded.')), 10000);
      socket.addEventListener('error', () => reject(new Error('Real observer WebSocket failed.')));
      socket.addEventListener('close', () => reject(new Error('Observer closed before a valid snapshot.')));
      socket.addEventListener('open', () => {
        try {
          authenticateWidgetSocket({ send(frame) {
            const message = JSON.parse(frame);
            assert.ok(message.type === 'authenticate' && message.token === token
              && Object.keys(message).length === 2, 'production adapter sends the exact first authentication frame');
            authenticationFrames++; socket.send(frame);
          } }, 2, token);
        } catch (error) { reject(error); }
      });
      socket.addEventListener('message', event => {
        try {
          const message = JSON.parse(event.data);
          if (message.type === 'error') throw new Error('Real Rust service rejected observer authentication.');
          if (message.type !== 'snapshot') return;
          const observed = acceptWidgetAccountSnapshot(message, identity);
          assert.ok(observed, 'real canonical snapshot is not stale'); ready(observed);
        } catch (error) { reject(error); }
      });
    });
  } finally { clearTimeout(timer); }
}
async function closeSocket() {
  if (!socket || socket.readyState === WebSocket.CLOSED) return;
  let timer;
  try {
    await Promise.race([new Promise(done => { socket.addEventListener('close', done, { once: true }); socket.close(1000, 'fixture complete'); }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Observer close deadline exceeded.')), 3000); })]);
  } finally { clearTimeout(timer); socket = null; }
}

try {
  running = await startServer(); stage = 'synthetic-fixture-create';
  const createdResponse = await request('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'Widget Observer Fixture' }) });
  assert.ok(createdResponse.status === 200, 'new synthetic fixture is created by the real Rust service');
  const created = await createdResponse.json();
  assert.ok(typeof created.token === 'string' && created.player?.id === created.identity?.chaoId, 'fixture has an authentic device credential and canonical identity');
  const before = await account(created.token);
  assert.ok(before.identity.lease === null, 'fixture initially has no activity lease');
  stage = 'production-http-observer';
  const observation = await readWidgetObservation((path, options) => request(path, options, true), created.token);
  assert.ok(observation.version === 2 && observation.player.id === created.player.id, 'production adapter observes the original synthetic Chao');
  verifyProfile(observation.player);
  stage = 'production-websocket-observer';
  const streamed = await readSocketObservation(running.base, observation.token, observation.identity);
  verifyProfile(streamed.player);
  assert.ok(streamed.player.id === created.player.id, 'real WebSocket retains the same canonical Chao');
  const after = await account(created.token);
  assert.ok(after.identity.lease === null && after.identity.revision === before.identity.revision, 'HTTP and WebSocket observation never acquire a lease or advance durable identity');
  assert.ok(after.snapshot.self.id === created.player.id && after.snapshot.self.rings === before.snapshot.self.rings, 'observation preserves canonical identity and wallet');
  await closeSocket(); stage = 'graceful-restart'; await stopServer(running); running = null;
  running = await startServer();
  const restarted = await readWidgetObservation((path, options) => request(path, options, true), created.token,
    { knownV2: true, identity: after.identity });
  verifyProfile(restarted.player);
  assert.ok(restarted.player.id === created.player.id && restarted.identity.accountId === before.identity.accountId,
    'credential resumes the same canonical Chao and account after restart');
  assert.ok(restarted.identity.lease === null && restarted.player.rings === before.snapshot.self.rings, 'restart preserves the wallet and observer remains without a lease');
  stage = 'final-shutdown'; await stopServer(running); running = null;
  const sourceFiles = ['src/widget-observer.js', 'src/widget-state.js', 'android/preview/verify-observer-roundtrip.mjs'];
  const sourceSha256 = Object.fromEntries(await Promise.all(sourceFiles.map(async path => [path, hash(await readFile(join(root, path)))])));
  const receipt = { schemaVersion: 1, passed: true, generatedAt: new Date().toISOString(),
    fixture: 'Fresh synthetic Chao in a new ignored directory; not the user profile',
    service: 'Actual isolated Rust accounts-v2 executable', nodeVersion: process.version, serverBinarySha256: serverSha256, sourceSha256,
    checks: { resumeOnlyHttp: true, canonicalPrivateSelf: true, sevenStatsAndNeeds: true, querylessWebSocket: true,
      exactAuthenticationFirstFrame: authenticationFrames === 1, noLeaseAcquired: true, noObserverGameplayIntents: true,
      durableRevisionUnchangedByObservation: true, canonicalIdentityAndWalletAfterRestart: true, liveProfileUntouched: true },
    observerHttpCalls: observerCalls, serviceProcessCount: 2, gracefulExitCodes: exitCodes };
  await mkdir(dirname(receiptPath), { recursive: true });
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify(receipt, null, 2));
} catch {
  // Never dump credential-bearing HTTP objects, authentication frames or saves.
  console.error(JSON.stringify({ passed: false, stage, message: 'Isolated Rust observer acceptance failed; no credentials exported.' }));
  process.exitCode = 1;
} finally {
  try { await closeSocket(); } catch { process.exitCode = 1; }
  try { if (running) await stopServer(running); } catch { process.exitCode = 1; }
}
