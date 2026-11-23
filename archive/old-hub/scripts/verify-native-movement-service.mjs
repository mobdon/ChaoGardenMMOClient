// Opt-in protocol proof against an explicitly selected, already-built Rust
// service. A new ignored data directory and OS-assigned loopback port prevent
// this tool from touching the playable preview, credentials or canonical save.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { observeSkateMotion, SkateCadence } from '../src/shadow-skate.js';
import { createNativeMotionState, selectNativeMotion } from '../src/chao-locomotion.js';

assert.ok(process.env.CHAO_PROOF_SERVER_EXE, 'Set CHAO_PROOF_SERVER_EXE to the verified Rust executable.');
const executable = resolve(process.env.CHAO_PROOF_SERVER_EXE);
const executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
await mkdir('.local/imports', { recursive: true });
const dataDirectory = await mkdtemp(resolve('.local/movement-service-'));
const backend = spawn(executable, [], {
  cwd: dataDirectory, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  env: { ...process.env, CHAO_DATA_DIR: dataDirectory, GAME_HOST: '127.0.0.1', GAME_PORT: '0',
    CHAO_OWNER_BOOTSTRAP: 'true', CHAO_SHUTDOWN_STDIN: '1', CHAO_ALLOWED_ORIGINS: '' },
});
let socket, visitor, heartbeat, report, stderr = '', phase = 'idle', previous;
let observerSnapshots = 0, observerConfirmedOwnerGear = false;
const samples = [], cadence = new SkateCadence(), nativeState = createNativeMotionState();
const nativeModes = new Set();
const snapshotListeners = new Set();
backend.stderr.on('data', bytes => { stderr += bytes; });
const serverExit = new Promise(done => backend.once('close', (code, signal) => done({ code, signal })));
const opened = connection => new Promise((done, reject) => {
  const clean = () => { clearTimeout(timer); for (const event of ['open', 'error', 'close']) connection.removeEventListener(event, finish); };
  const finish = event => { clean(); event.type === 'open' ? done() : reject(new Error('Proof WebSocket failed to open.')); };
  const timer = setTimeout(() => { clean(); reject(new Error('Proof WebSocket opening timed out.')); }, 5000);
  for (const event of ['open', 'error', 'close']) connection.addEventListener(event, finish);
});
try {
  const address = await new Promise((done, reject) => {
    let output = '';
    const fail = error => { clearTimeout(timer); reject(error); };
    const timer = setTimeout(() => fail(new Error('Rust service did not announce a loopback port.')), 10000);
    backend.once('error', fail);
    backend.once('exit', () => fail(new Error('Rust service exited before proof startup.')));
    backend.stdout.on('data', bytes => {
      output += bytes;
      const found = output.match(/http:\/\/127\.0\.0\.1:(\d+)/);
      if (found) { clearTimeout(timer); done(`http://127.0.0.1:${found[1]}`); }
    });
  });
  const response = await fetch(`${address}/api/session`, { method: 'POST',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Movement Proof Fixture' }) });
  assert.equal(response.status, 200);
  const session = await response.json();
  assert.equal(session.player.admin, true);
  socket = new WebSocket(`${address.replace('http:', 'ws:')}/ws?token=${encodeURIComponent(session.token)}`);
  const self = snapshot => snapshot.players.find(player => player.id === snapshot.selfId);
  socket.addEventListener('message', event => {
    const snapshot = JSON.parse(event.data);
    if (snapshot.type !== 'snapshot') return;
    const player = self(snapshot);
    if (!player) return;
    const observed = observeSkateMotion(previous, player, snapshot.serverTime, performance.now());
    previous = observed;
    const native = selectNativeMotion(nativeState, .1, { ...observed, fresh: observed.valid, mounted: !!player.vehicle });
    cadence.update(.1, { ...observed, fresh: observed.valid, available: true,
      footwear: player.admin && player.gear?.footwear === 'shadow-air-shoes',
      mounted: !!player.vehicle, grounded: observed.grounded && native.mode !== 'Land' });
    nativeModes.add(native.mode);
    samples.push({ phase, serverTime: snapshot.serverTime, speed: observed.speed,
      valid: observed.valid, grounded: observed.grounded, jumpOffset: observed.jumpOffset,
      nativeMode: native.mode, nativeState: native.nativeState, skate: cadence.active, cadence: cadence.rate,
      vehicle: player.vehicle, x: player.x, y: player.y, z: player.z });
    for (const listener of snapshotListeners) listener(snapshot);
  });
  await opened(socket);
  const send = value => socket.send(JSON.stringify(value));
  const until = predicate => new Promise((done, reject) => {
    const listener = value => { if (predicate(value)) { clearTimeout(timer); snapshotListeners.delete(listener); done(value); } };
    const timer = setTimeout(() => { snapshotListeners.delete(listener); reject(new Error(`No acknowledged snapshot during ${phase}; samples=${samples.length}; last=${JSON.stringify(samples.at(-1))}`)); }, 5000);
    snapshotListeners.add(listener);
  });
  let input = { type: 'input', dx: 0, dz: 0, jump: false, boost: false };
  heartbeat = setInterval(() => send(input), 50);
  await until(snapshot => self(snapshot).jumpOffset === 0);
  phase = 'equip-shoes'; send({ type: 'action', action: 'equip-owner-gear', item: 'shadow-air-shoes' });
  await until(snapshot => self(snapshot).gear?.footwear === 'shadow-air-shoes');
  // The service has an80ms bounded-action throttle. A state acknowledgment
  // can arrive sooner; wait for a later server tick before the next action.
  const shoesAt = samples.at(-1).serverTime;
  await until(snapshot => snapshot.serverTime - shoesAt >= 150);
  phase = 'equip-gloves';
  send({ type: 'action', action: 'equip-owner-gear', item: 'knuckles-hammer-gloves' });
  await until(snapshot => self(snapshot).gear?.hands === 'knuckles-hammer-gloves' && self(snapshot).gear?.footwear === 'shadow-air-shoes');
  const guestResponse = await fetch(`${address}/api/session`, { method: 'POST',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Movement Observer Fixture' }) });
  assert.equal(guestResponse.status, 200);
  const guest = await guestResponse.json(); assert.equal(guest.player.admin, false);
  visitor = new WebSocket(`${address.replace('http:', 'ws:')}/ws?token=${encodeURIComponent(guest.token)}`);
  visitor.addEventListener('message', event => {
    const snapshot = JSON.parse(event.data);
    if (snapshot.type !== 'snapshot') return;
    observerSnapshots++;
    const owner = snapshot.players.find(player => player.id === session.player.id);
    const observing = snapshot.players.find(player => player.id === snapshot.selfId);
    observerConfirmedOwnerGear ||= !!owner?.admin && owner.gear?.footwear === 'shadow-air-shoes'
      && owner.gear?.hands === 'knuckles-hammer-gloves' && observing?.admin === false && !observing.gear?.footwear;
  });
  await opened(visitor);
  phase = 'two-client';
  await until(snapshot => observerConfirmedOwnerGear && snapshot.players.length === 2 && snapshot.players.some(player => !player.admin && !player.gear?.footwear));
  const travel = async (name, dx) => {
    phase = name; input = { ...input, dx }; send(input);
    const start = samples.at(-1).serverTime;
    await until(snapshot => snapshot.serverTime - start >= 800);
  };
  await travel('walk', .4);
  await travel('run', .62);
  await travel('skate', 1);
  phase = 'jump'; input = { ...input, jump: true }; send(input);
  await until(snapshot => self(snapshot).jumpOffset > .05);
  input = { ...input, jump: false }; send(input);
  await until(snapshot => self(snapshot).jumpOffset === 0);
  phase = 'landing';
  const landedAt = samples.at(-1).serverTime;
  await until(snapshot => snapshot.serverTime - landedAt >= 600);
  await travel('stop', 0);
  phase = 'mount'; send({ type: 'action', action: 'mount-dark-walker' });
  await until(snapshot => !!self(snapshot).vehicle);
  phase = 'mech-move'; input = { ...input, dx: .5 }; send(input);
  const mountedAt = samples.at(-1).serverTime;
  await until(snapshot => snapshot.serverTime - mountedAt >= 600);
  phase = 'dismount'; input = { ...input, dx: 0 }; send(input);
  send({ type: 'action', action: 'dismount-vehicle' });
  await until(snapshot => !self(snapshot).vehicle);
  const rows = name => samples.filter(row => row.phase === name && row.valid);
  assert.ok(rows('walk').some(row => row.nativeMode === 'Walk' && !row.skate));
  assert.ok(rows('run').some(row => row.nativeMode === 'Run' && !row.skate));
  assert.ok(rows('skate').some(row => row.skate && row.speed > 1.55));
  for (const mode of ['Idle', 'Walk', 'Run', 'Jump', 'Fall', 'Land']) assert.ok(nativeModes.has(mode), `${mode} was not acknowledged`);
  assert.ok(samples.filter(row => !row.grounded || row.vehicle).every(row => !row.skate));
  const mounted = rows('mech-move');
  assert.ok(mounted.length > 2 && mounted.every(row => row.vehicle && row.nativeMode === 'Idle'));
  assert.ok(Math.hypot(mounted.at(-1).x - mounted[0].x, mounted.at(-1).z - mounted[0].z) > .2, 'mech must actually move');
  assert.ok(rows('stop').some(row => row.nativeMode === 'Idle' && !row.skate));
  report = { result: 'PASS', executableSha256, dataDirectory, isolatedLoopback: true,
    fixture: 'New Rust-owned profile; no live canonical profile, tokens or saves are read.',
    nativeModes: [...nativeModes], twoRealClients: true, observerSnapshots, observerConfirmedOwnerGear, samples,
    limit: 'Actual Rust HTTP/WebSocket/input/snapshot round trip and presentation selection. Browser transforms/materials are checked separately; no source-build or rollout claim.' };
} finally {
  clearInterval(heartbeat);
  if (socket?.readyState === WebSocket.OPEN) socket.close();
  if (visitor?.readyState === WebSocket.OPEN) visitor.close();
  if (backend.stdin.writable) backend.stdin.end('shutdown\n');
  const forced = setTimeout(() => backend.kill(), 3000);
  const stopped = await serverExit;
  clearTimeout(forced);
  if (stderr.trim()) console.error(stderr.trim());
  assert.equal(stopped.code, 0, 'Rust proof service must shut down cleanly');
}
await writeFile('.local/imports/native-movement-service-proof.json', `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify({ ...report, samples: samples.length }));
