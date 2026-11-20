// Real Rust-service acceptance against a fresh isolated save. No live profile,
// installed asset, clock, seed, score or result is injected into the service.
// Set CHAO_TEST_SERVER to a tested executable; optional CHAO_TEST_SOURCE records
// the observed checkout HEAD/dirty hashes without building or changing it.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createTinyAccount, supportsTinyAccounts } from '../src/tiny-account.js';

if (!process.env.CHAO_TEST_SERVER) throw new Error('Set CHAO_TEST_SERVER to an existing tested Rust server executable.');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const executable = resolve(process.env.CHAO_TEST_SERVER);
const isolatedRoot = join(root, '.local');
await mkdir(isolatedRoot, { recursive: true });
const directory = await mkdtemp(join(isolatedRoot, 'tiny-live-'));
const gameIds = ['memory', 'rock_paper_scissors', 'bounce', 'ball', 'search'];
let running;
const sockets = new Set();
let stage = 'startup';
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const binarySha256 = sha256(await readFile(executable));
const adapterSha256 = sha256(await readFile(join(root, 'src/tiny-account.js')));
let sourceEvidence = null;
if (process.env.CHAO_TEST_SOURCE) {
  const sourceRoot = resolve(process.env.CHAO_TEST_SOURCE);
  const git = args => execFileSync('git', ['-c', `safe.directory=${sourceRoot}`, '-C', sourceRoot, ...args], { windowsHide: true });
  const head = git(['rev-parse', 'HEAD']).toString().trim();
  const status = git(['status', '--porcelain=v1', '-z']);
  const diff = git(['diff', '--binary', 'HEAD']);
  const untracked = git(['ls-files', '--others', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean).sort();
  const untrackedHashes = [];
  for (const path of untracked) untrackedHashes.push([path, sha256(await readFile(join(sourceRoot, path)))]);
  sourceEvidence = { root: sourceRoot, head, dirty: status.length !== 0, statusSha256: sha256(status),
    changesSha256: sha256(Buffer.concat([diff, Buffer.from(JSON.stringify(untrackedHashes))])) };
}
const pause = ms => new Promise(ready => setTimeout(ready, ms));

async function startServer() {
  const child = spawn(executable, [], { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, GAME_HOST: '127.0.0.1', GAME_PORT: '0', CHAO_DATA_DIR: directory,
      CHAO_OWNER_BOOTSTRAP: '0', CHAO_SHUTDOWN_STDIN: '1', CHAO_ALLOWED_ORIGINS: '' } });
  let stdout = '', stderr = '';
  child.stderr.on('data', bytes => { stderr += bytes; });
  const exited = new Promise(resolveExit => {
    child.once('exit', (code, signal) => resolveExit({ code, signal })); child.once('error', error => resolveExit({ code: null, error }));
  });
  let base;
  try { base = await new Promise((ready, reject) => {
    const timer = setTimeout(() => reject(new Error(`Rust startup timeout: ${stderr}`)), 8000);
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Rust exited before readiness (${code}): ${stderr}`)); });
    child.stdout.on('data', bytes => {
      stdout += bytes;
      const match = stdout.match(/Chao Rust game server: (http:\/\/127\.0\.0\.1:\d+)/);
      if (match) { clearTimeout(timer); ready(match[1]); }
    });
  }); } catch (error) { child.kill(); await exited; throw error; }
  return { child, exited, base };
}

async function stopServer(server) {
  if (!server || server.child.exitCode !== null) return;
  server.child.stdin.end('shutdown\n');
  let timer;
  try {
    const result = await Promise.race([server.exited, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Isolated Rust shutdown timeout')), 5000); })]);
    assert.equal(result.code, 0);
  } finally { clearTimeout(timer); }
}

async function sessionRequest(base, body) {
  const response = await fetch(`${base}/api/session`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  assert.equal(response.status, 200, 'canonical session request');
  return response.json();
}

async function connect(base, token, status) {
  const v2 = supportsTinyAccounts(status);
  let socket, account;
  const messages = [], observers = new Set();
  const accountErrors = [];
  let cursor = 0;
  if (v2) {
    account = createTinyAccount({ fetch: (path, options) => fetch(new URL(path, base), options),
      transmit: message => {
        if (socket?.readyState !== WebSocket.OPEN) return false;
        socket.send(JSON.stringify(message)); return true;
      }, onError: (_message, code) => accountErrors.push(code) });
    await account.connect({ token, status });
  }
  // Capability detection deliberately selects the existing Rust transports.
  // V2 credentials appear only in the first private frame and HTTP headers.
  socket = new WebSocket(`${base.replace('http:', 'ws:')}/ws${v2 ? '' : `?token=${encodeURIComponent(token)}`}`);
  sockets.add(socket);
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    account?.consume(message); // Update durable revision/counter before tests observe gameplay.
    messages.push(message);
    for (const observer of observers) observer(message);
  });
  await once(socket, 'open');
  if (v2) socket.send(JSON.stringify({ type: 'authenticate', token }));
  return { socket, messages, observers, account, accountErrors,
    until(predicate, action, timeout = 6000) {
      return new Promise((resolveMessage, reject) => {
        let timer;
        let matched, actionSettled = !action;
        const complete = () => {
          if (!matched || !actionSettled) return;
          clearTimeout(timer); observers.delete(inspect); resolveMessage(matched);
        };
        function inspect() {
          for (; cursor < messages.length; cursor++) {
            const message = messages[cursor];
            if (predicate(message)) { cursor++; matched = message; observers.delete(inspect); complete(); return; }
          }
        }
        timer = setTimeout(() => { observers.delete(inspect); reject(new Error(`Timed out waiting for authoritative Tiny Garden state at ${stage}.`)); }, timeout);
        observers.add(inspect);
        if (action) {
          cursor = messages.length;
          Promise.resolve().then(action).then(() => { actionSettled = true; complete(); })
            .catch(error => { clearTimeout(timer); observers.delete(inspect); reject(error); });
        }
        else inspect();
      });
    },
    async send(message) {
      if (!account) { socket.send(JSON.stringify(message)); return; }
      assert.equal(await account.whenIdle(), true, `account adapter ready at ${stage}`);
      assert.equal(await account.send(message), true, `fenced intent accepted at ${stage}: ${accountErrors.at(-1) || 'no reported error'}`);
      await account.whenIdle();
    },
    async close() {
      if (account) { await account.whenIdle(); account.disconnect(); }
      if (socket.readyState < WebSocket.CLOSING) { const closed = once(socket, 'close'); socket.close(); await closed; }
      sockets.delete(socket);
    },
  };
}

const self = (snapshot, id) => snapshot.self?.id === id ? snapshot.self : snapshot.players.find(player => player.id === id);
const snapshot = (connection, predicate, action) => connection.until(message => message.type === 'snapshot' && predicate(message), action);
// Respect the published host's 80ms generic action debounce. This schedules
// input only; every assertion still waits for canonical server state.
const action = async (connection, name, fields = {}) => { await pause(150); await connection.send({ type: 'action', action: name, ...fields }); };

try {
  running = await startServer();
  const status = await (await fetch(`${running.base}/api/status`)).json();
  const v2 = supportsTinyAccounts(status);
  if (status.accounts?.version === 2) assert.equal(v2, true, 'v2 must advertise the complete supported transport');
  assert.deepEqual([...status.tinyGarden.games].sort(), [...gameIds].sort());
  const session = await sessionRequest(running.base, { name: 'Isolated Tiny Garden acceptance' });
  const id = session.player.id;
  assert.equal(session.player.admin, false);
  let connection = await connect(running.base, session.token, status);
  const initial = await snapshot(connection, message => message.selfId === id);
  if (v2) {
    assert.equal(initial.protocolVersion, 2);
    assert.equal(initial.identity.chaoId, id);
    assert.equal(connection.socket.url.includes('?'), false, 'v2 authenticates without URL credentials');
  }
  const initialRings = self(initial, id).rings;
  let last = initial;
  for (const gameId of gameIds) {
    stage = `start-${gameId}`;
    const active = await snapshot(connection, message => message.tinyGame?.gameId === gameId,
      () => action(connection, 'tiny-start', { gameId }));
    assert.equal(active.tinyGame.phase, 'playing');
    assert.equal(active.tinyGame.view.game, gameId);
    stage = `stop-${gameId}`;
    last = await snapshot(connection, message => message.tinyGame === null,
      () => action(connection, 'tiny-stop', { sessionId: active.tinyGame.id }));
    assert.equal(self(last, id).rings, initialRings, 'stopped rounds pay nothing');
  }
  stage = 'memory-public-peek';
  const start = await snapshot(connection, message => message.tinyGame?.gameId === 'memory',
    () => action(connection, 'tiny-start', { gameId: 'memory' }));
  const runId = start.tinyGame.id;
  const known = start.tinyGame.view.state.cards.map(card => card.face);
  assert.equal(known.filter(Number.isInteger).length, 14, 'initial public peek');
  const trackPublicShuffle = message => {
    if (message.type !== 'snapshot' || message.tinyGame?.id !== runId) return;
    const cards = message.tinyGame.view.state.cards;
    const removed = known.flatMap((face, index) => Number.isInteger(face) && !cards[index].occupied ? [index] : []);
    const added = known.flatMap((face, index) => !Number.isInteger(face) && cards[index].occupied ? [index] : []);
    if (removed.length) {
      assert.equal(removed.length, 1, 'observe every public shuffle move'); assert.equal(added.length, 1);
      [known[removed[0]], known[added[0]]] = [known[added[0]], known[removed[0]]];
    }
  };
  connection.observers.add(trackPublicShuffle);
  stage = 'memory-public-shuffle';
  await snapshot(connection, message => message.tinyGame?.view.state.phase === 'choose');
  connection.observers.delete(trackPublicShuffle);
  for (let face = 0; face < 7; face++) {
    const pair = known.flatMap((value, index) => value === face ? [index] : []);
    assert.equal(pair.length, 2);
    for (const cell of pair) await snapshot(connection, message => message.tinyGame?.view.state.cards[cell].face === face,
      async () => { stage = `memory-flip-${face}-${cell}`; await pause(100); await connection.send({ type: 'tiny-input', sessionId: runId, input: { kind: 'memory_flip', cell } }); });
  }
  const isSavedResult = message => message.kind === 'tiny-game-result' && message.id === runId && message.saved === true;
  stage = 'memory-durable-result';
  const result = connection.messages.find(isSavedResult) || await connection.until(isSavedResult);
  assert.equal(result.completed, true); assert.ok(result.rings > 0);
  const finished = await snapshot(connection, message => message.tinyGame?.phase === 'finished');
  const earned = self(finished, id).rings;
  assert.equal(earned, initialRings + result.rings);
  const receipt = self(finished, id).tinyRecords.memory;
  assert.equal(receipt.plays, 1); assert.equal(receipt.lastResultId, runId);
  await connection.close();
  stage = 'reconnect';
  connection = await connect(running.base, session.token, status);
  const reconnected = await snapshot(connection, message => message.selfId === id);
  assert.equal(self(reconnected, id).rings, earned);
  await connection.send({ type: 'tiny-input', sessionId: runId, input: { kind: 'memory_flip', cell: 0 } });
  stage = 'explicit-save';
  await connection.until(message => message.kind === 'saved', () => action(connection, 'save'));
  const disk = JSON.parse(await readFile(join(directory, 'profiles.json'), 'utf8')).find(profile => profile.id === id);
  assert.equal(disk.rings, earned); assert.equal(disk.tinyRecords.memory.plays, 1);
  await connection.close();
  await stopServer(running); running = null;
  running = await startServer();
  const resumed = await sessionRequest(running.base, { token: session.token, resumeOnly: true });
  assert.equal(resumed.player.id, id); assert.equal(resumed.player.rings, earned);
  assert.equal(resumed.player.tinyRecords.memory.plays, 1);
  assert.equal(resumed.player.tinyRecords.memory.lastResultId, runId);
  const evidence = { passed: true, transport: v2 ? 'accounts-v2-tiny-adapter' : 'legacy-development',
    executable, binarySha256, adapterSha256, source: sourceEvidence,
    deployment: 'isolated-test-only-not-runtime-acceptance', gamesStartedAndStopped: gameIds,
    memory: { completed: true, saved: true, rings: result.rings, playsAfterReconnectAndRestart: 1 },
    canonicalIdentityPreserved: true, isolatedData: directory };
  assert.equal(sha256(await readFile(executable)), binarySha256, 'test executable did not change during validation');
  await writeFile(join(directory, 'result.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  for (const socket of sockets) socket.close();
  try { await stopServer(running); }
  catch (error) { running?.child.kill(); throw error; }
}
