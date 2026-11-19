// Opt-in protocol proof using a frozen, already-built Rust service and new
// disposable profiles. No live preview, credentials, assets or saves are read.
// This verifies snapshot-driven eligibility; rendered shoes/effects have a
// separate native-asset and browser proof.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chooseNativeGait } from '../src/chao-locomotion.js';
import { observeSkateMotion, SkateCadence } from '../src/shadow-skate.js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const expectedSha256 = '51a07f9c382ecb70785654297180349dd2e34f677d5921818ea2b9a6ecec3100';
const executable = resolve(process.env.CHAO_PROOF_SERVER_EXE || resolve(root, '.local/runtime/chao-game-server-51a07f9c382e.exe'));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const executableSha256 = sha256(await readFile(executable));
assert.equal(executableSha256, expectedSha256, 'Proof requires the recorded, frozen Rust executable.');
const output = resolve(root, '.local/reviews/footwear-service-proof.json');
await mkdir(dirname(output), { recursive: true });
const dataDirectory = await mkdtemp(resolve(root, '.local/footwear-service-'));
const sourceSha256 = Object.fromEntries(await Promise.all([
  'src/scene.js', 'src/shadow-skate.js', 'src/chao-locomotion.js', 'scripts/verify-footwear-service.mjs',
].map(async name => [name, sha256(await readFile(resolve(root, name)))])));
const backend = spawn(executable, [], {
  cwd: dataDirectory, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  env: { ...process.env, CHAO_DATA_DIR: dataDirectory, GAME_HOST: '127.0.0.1', GAME_PORT: '0',
    CHAO_OWNER_BOOTSTRAP: 'true', CHAO_SHUTDOWN_STDIN: '1', CHAO_ALLOWED_ORIGINS: '' },
});
const samples = [], checks = [], snapshotListeners = new Set();
const cadence = new SkateCadence();
let socket, visitor, heartbeat, phase = 'idle', previous, gait = 'Idle';
let observerSnapshots = 0, observerConfirmedOwnerGear = false, failure = null;
let processError = null, stderrBytes = 0;
backend.once('error', error => { processError = error; });
backend.stderr.on('data', bytes => { stderrBytes += bytes.length; });
const serverExit = new Promise(done => backend.once('close', (code, signal) => done({ code, signal })));
const opened = connection => new Promise((done, reject) => {
  const finish = event => {
    clearTimeout(timer);
    for (const name of ['open', 'error', 'close']) connection.removeEventListener(name, finish);
    event.type === 'open' ? done() : reject(new Error('Proof WebSocket did not open.'));
  };
  const timer = setTimeout(() => finish({ type: 'timeout' }), 5000);
  for (const name of ['open', 'error', 'close']) connection.addEventListener(name, finish);
});
const check = (name, predicate) => { assert.ok(predicate, name); checks.push(name); };
const self = snapshot => snapshot.players.find(player => player.id === snapshot.selfId);
const rows = name => samples.filter(row => row.phase === name && row.valid);
try {
  const address = await new Promise((done, reject) => {
    let output = '';
    const timer = setTimeout(() => fail(new Error('Rust proof service did not announce its loopback port.')), 10000);
    const fail = error => { clearTimeout(timer); reject(error); };
    backend.once('error', fail);
    backend.once('exit', () => fail(new Error('Rust proof service exited during startup.')));
    backend.stdout.on('data', bytes => {
      output += bytes;
      const found = output.match(/http:\/\/127\.0\.0\.1:(\d+)/);
      if (found) { clearTimeout(timer); done(`http://127.0.0.1:${found[1]}`); }
    });
  });
  const response = await fetch(`${address}/api/session`, { method: 'POST',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Footwear Proof Fixture' }) });
  assert.equal(response.status, 200);
  const session = await response.json();
  check('new isolated Rust owner session', session.player.admin === true);
  socket = new WebSocket(`${address.replace('http:', 'ws:')}/ws?token=${encodeURIComponent(session.token)}`);
  socket.addEventListener('message', event => {
    const snapshot = JSON.parse(event.data);
    if (snapshot.type !== 'snapshot') return;
    const player = self(snapshot);
    if (!player) return;
    const observed = observeSkateMotion(previous, player, snapshot.serverTime, performance.now());
    previous = observed;
    const fresh = observed.valid;
    cadence.update(.1, { ...observed, fresh, available: true,
      footwear: player.admin && player.gear?.footwear === 'shadow-air-shoes', mounted: !!player.vehicle });
    gait = chooseNativeGait(fresh && observed.grounded && !player.vehicle ? observed.speed : 0, gait);
    samples.push({ phase, serverTime: snapshot.serverTime, observedAt: observed.observedAt,
      speed: observed.speed, valid: observed.valid, grounded: observed.grounded,
      jumpOffset: player.jumpOffset, gait, skate: cadence.active, blend: cadence.blend,
      cadence: cadence.rate, cadenceState: cadence.state, vehicle: player.vehicle,
      x: player.x, y: player.y, z: player.z });
    for (const listener of snapshotListeners) listener(snapshot);
  });
  await opened(socket);
  const send = value => {
    assert.equal(socket.readyState, WebSocket.OPEN, 'Proof owner socket must be open.');
    socket.send(JSON.stringify(value));
  };
  const until = predicate => new Promise((done, reject) => {
    const listener = snapshot => {
      if (predicate(snapshot)) { clearTimeout(timer); snapshotListeners.delete(listener); done(snapshot); }
    };
    const timer = setTimeout(() => {
      snapshotListeners.delete(listener);
      reject(new Error(`No acknowledged snapshot during ${phase}; samples=${samples.length}.`));
    }, 5000);
    snapshotListeners.add(listener);
  });
  let input = { type: 'input', dx: 0, dz: 0, jump: false, boost: false };
  heartbeat = setInterval(() => { if (socket.readyState === WebSocket.OPEN) send(input); }, 50);
  await until(snapshot => self(snapshot).jumpOffset === 0);
  phase = 'equip-shoes'; send({ type: 'action', action: 'equip-owner-gear', item: 'shadow-air-shoes' });
  await until(snapshot => self(snapshot).gear?.footwear === 'shadow-air-shoes');
  // Acknowledgments can precede the service's80ms bounded-action throttle.
  const shoesAt = samples.at(-1).serverTime;
  await until(snapshot => snapshot.serverTime - shoesAt >= 150);
  phase = 'equip-gloves'; send({ type: 'action', action: 'equip-owner-gear', item: 'knuckles-hammer-gloves' });
  await until(snapshot => self(snapshot).gear?.hands === 'knuckles-hammer-gloves'
    && self(snapshot).gear?.footwear === 'shadow-air-shoes');
  checks.push('Rust acknowledgments retain shoes and gloves together');
  const guestResponse = await fetch(`${address}/api/session`, { method: 'POST',
    headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: 'Footwear Observer Fixture' }) });
  assert.equal(guestResponse.status, 200);
  const guest = await guestResponse.json();
  check('second real session has no owner privilege', guest.player.admin === false);
  visitor = new WebSocket(`${address.replace('http:', 'ws:')}/ws?token=${encodeURIComponent(guest.token)}`);
  visitor.addEventListener('message', event => {
    const snapshot = JSON.parse(event.data);
    if (snapshot.type !== 'snapshot') return;
    observerSnapshots++;
    const owner = snapshot.players.find(player => player.id === session.player.id);
    const observing = self(snapshot);
    observerConfirmedOwnerGear ||= !!owner?.admin && owner.gear?.footwear === 'shadow-air-shoes'
      && owner.gear?.hands === 'knuckles-hammer-gloves' && observing?.admin === false && !observing.gear?.footwear;
  });
  await opened(visitor);
  phase = 'two-client';
  await until(snapshot => observerConfirmedOwnerGear && snapshot.players.length === 2);
  checks.push('observer sees owner gear without inheriting it');
  const travel = async (name, dx, duration = 800) => {
    assert.ok(Number.isFinite(dx) && Math.abs(dx) <= 1, 'Only bounded analog input is submitted.');
    phase = name; input = { ...input, dx }; send(input);
    const start = samples.at(-1).serverTime;
    await until(snapshot => snapshot.serverTime - start >= duration);
  };
  await travel('walk', .4);
  await travel('run', .62);
  await travel('skate', 1);
  phase = 'jump'; input = { ...input, jump: true }; send(input);
  await until(snapshot => self(snapshot).jumpOffset > .05);
  input = { ...input, jump: false }; send(input);
  await until(snapshot => self(snapshot).jumpOffset === 0);
  await travel('stop', 0);
  phase = 'mount'; send({ type: 'action', action: 'mount-dark-walker' });
  await until(snapshot => !!self(snapshot).vehicle);
  await travel('mech-move', .5, 600);
  phase = 'dismount'; input = { ...input, dx: 0 }; send(input);
  send({ type: 'action', action: 'dismount-vehicle' });
  await until(snapshot => !self(snapshot).vehicle);
  check('bounded walk input selects native Walk without skating', rows('walk').some(row => row.gait === 'Walk' && !row.skate));
  check('bounded run input selects native Run below skating', rows('run').some(row => row.gait === 'Run' && !row.skate));
  check('full bounded input reaches skating threshold', rows('skate').some(row => row.skate && row.speed > 1.55));
  const skating = rows('skate').filter(row => row.skate);
  check('skate cadence ramps inside native state8–10 timing bounds', skating.length > 2
    && skating.every(row => row.cadence > 0 && row.cadence <= .20)
    && skating.at(-1).cadence > skating[0].cadence
    && skating.at(-1).cadence > .12);
  const airborne = rows('jump').filter(row => !row.grounded && row.jumpOffset > .001);
  check('actual jump disables active skating without coasting', airborne.length > 0
    && airborne.every(row => !row.skate && row.cadenceState === 'airborne'));
  check('actual jump fades out the skate pose', airborne.every((row, index) => row.blend < 1
    && (index === 0 || row.blend <= airborne[index - 1].blend))
    && (airborne.length < 3 || airborne.at(-1).blend === 0));
  const mounted = rows('mech-move');
  check('actual moving mech keeps pilot Idle and skating disabled', mounted.length > 2
    && mounted.every(row => row.vehicle && row.gait === 'Idle' && !row.skate && row.blend === 0)
    && Math.hypot(mounted.at(-1).x - mounted[0].x, mounted.at(-1).z - mounted[0].z) > .2);
  check('stopping returns to Idle and clears skating', rows('stop').some(row => row.gait === 'Idle' && !row.skate && row.blend === 0));
  await travel('pre-disconnect-skate', 1);
  check('fresh service observations restore skating after dismount', cadence.active && cadence.blend === 1);
  clearInterval(heartbeat); heartbeat = null;
  socket.close();
  await new Promise(done => setTimeout(done, 400));
  const ageMs = performance.now() - previous.observedAt;
  const fresh = previous.valid && ageMs < 350;
  cadence.update(.1, { ...previous, speed: fresh ? previous.speed : 0,
    grounded: previous.grounded, footwear: true, mounted: false, fresh, available: true });
  gait = chooseNativeGait(fresh && previous.grounded ? previous.speed : 0, gait);
  check('actual disconnect crosses350ms freshness cutoff and clears skating', ageMs >= 350
    && !fresh && !cadence.active && cadence.blend === 0 && cadence.state === 'stale' && gait === 'Idle');
  samples.push({ phase: 'disconnected-stale', ageMs, fresh, gait,
    skate: cadence.active, blend: cadence.blend, cadenceState: cadence.state });
} catch (error) {
  failure = error;
} finally {
  clearInterval(heartbeat);
  for (const connection of [socket, visitor]) if (connection?.readyState === WebSocket.OPEN) connection.close();
  if (backend.stdin.writable) backend.stdin.end('shutdown\n');
  const forced = setTimeout(() => backend.kill(), 3000);
  const stopped = await serverExit;
  clearTimeout(forced);
  if (processError && !failure) failure = processError;
  if (stopped.code !== 0 && !failure) failure = new Error('Rust proof service did not shut down cleanly.');
  if (stopped.code === 0) checks.push('isolated Rust service shuts down cleanly');
  const report = { result: failure ? 'FAIL' : 'PASS', executableSha256, sourceSha256,
    dataDirectory, isolatedLoopback: true, newSyntheticProfilesOnly: true,
    twoRealClients: observerSnapshots > 0, observerSnapshots, observerConfirmedOwnerGear,
    checks, samples, stopped, stderrBytes,
    error: failure?.message || null,
    limit: 'Frozen Rust HTTP/WebSocket/input/snapshot proof and current presentation selection. Browser transforms/materials are verified separately. No current-server-source build, live rollout, save migration or full engine-port claim.' };
  await writeFile(output, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ result: report.result, executableSha256, checks: checks.length,
    samples: samples.length, observerSnapshots, output, error: report.error }));
}
if (failure) throw failure;
