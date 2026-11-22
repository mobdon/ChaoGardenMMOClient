import './tiny.css';
import { createSaveSlots } from './save-slots.js';
import { createStatRow, updateStatRow } from './stat-ui.js';
import { chaosVariant, chaoFormLabel, drawTinyAvatar, nativeDirection, setOwnAtlas } from './tiny-avatar.js';
import { createTinyWatch } from './tiny-watch.js';
// The panel that speaks the server's training (care envelope, adapted-afk-training); tiny-training.js
// waits for a capability the Rust server never sends.
import { createTinyTraining } from './tiny-afk-training.js';
import { createTinyItems } from './tiny-items.js';
import { durableTinyResult, readTinyCache, writeTinyCache, observedTime } from './tiny-state.js';
import { createTinyAccount, supportsTinyAccounts } from './tiny-account.js';
import { tinyLogin } from './tiny-login.js';

const $ = selector => document.querySelector(selector);
const loopback = ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname);
const secureTransport = loopback || location.protocol === 'https:';
const games = {
  memory: { number: '01', title: 'Chao Memory', intro: 'Remember the cards, watch the shuffle, and find all seven pairs.', tip: 'Tap two cards to find a pair. Trip the shuffle once for a quick peek.', metrics: ['7 pairs', '3 mistakes'] },
  rock_paper_scissors: { number: '02', title: 'Rock, Paper, Chao!', intro: 'Choose a hand and time your shot at the rotating targets. A winning hand earns a ring.', tip: 'Choose a card, then shoot. Keys 1–3 select a card; Space shoots.', metrics: ['Choose a hand', 'Time your shot'] },
  bounce: { number: '03', title: 'Chao Bounce', intro: 'Slide the spring under your Chao, collect the rings, and keep the bounce going.', tip: 'Drag left or right to move the spring. Tap Spring during compression for a boost.', metrics: ['4 levels', '3 lives'] },
  ball: { number: '04', title: 'Chao Ball', intro: 'Shoot red or green balls. Connect ten of one color, then catch the released rings.', tip: 'Drag or hold an arrow to move. Tap a color to shoot; Z is red, X is green.', metrics: ['Match colors', 'Catch the rings'] },
  search: { number: '05', title: 'Chao Search', intro: 'Follow the hints, find the requested Chao, and bring them home to the entrance.', tip: 'Drag to walk. Tap a nearby Chao or hint orb. Carry your Chao to HOME, then Return.', metrics: ['7 Chao to find', 'Follow the hints'] },
};
const statNames = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
const colors = ['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white'];
const handSymbols = { rock: '✊', paper: '✋', scissors: '✌' };
const canvas = $('#tiny-canvas');
const ctx = canvas.getContext('2d');
const profileCanvas = $('#tiny-portrait');
const profileContext = profileCanvas.getContext('2d');
const faceArt = colors.map(color => { const image = new Image(); image.src = `/api/assets/emerald-${color}.png`; return image; });
for (const [asset, className] of [['chao-stat-panel.png', 'native-stat-panel-ready'], ['chao-stat-font.png', 'native-stat-font-ready']]) {
  const image = new Image(); image.onload = () => document.documentElement.classList.add(className); image.src = `/api/assets/${asset}`;
}
let storage;
try { storage = window.localStorage; } catch { storage = { getItem: () => null, setItem: () => {} }; }
const saves = createSaveSlots(storage);
let savedSlots = [];
let socket = null;
let token = null;
let player = null;
let tinyGame = null;
let selectedGame = 'memory';
let connected = false;
let connecting = false;
let connectionGeneration = 0;
let reconnectTimer;
let pendingTimer;
let pendingStart = false;
let pendingStop = false;
let controlsSession = '';
let lastResultId = null;
let lastSavedAt = null;
let lastSyncedAt = null;
let lastCachedAt = 0;
let serverTime = null;
let capabilities = {};
let careState = null;
let selectedScreen = 'garden';
let pendingSave = false;
let returnAfterSave = false;
let saveTimer;
let retryCount = 0;
let resumeAbort;
let accountV2 = false;
let knownAccountV2 = false;
let accountReady = false;
let handshakeTimer;
let lastRememberedAt = 0;
let avatarMotion = { direction: 0, changedAt: 0 };
let renderTime = 0;
let pointer = null;
let canvasMove = { x: 0, y: 0 };
let lastMove = { x: 0, y: 0 };
let hitTargets = [];
const heldMoves = new Map();
const heldKeys = new Set();
const statRows = new Map();
const gardenWatch = createTinyWatch({ send });
const training = createTinyTraining({ root: $('#tiny-training-panel'), send });
const items = createTinyItems({ root: $('#tiny-items-panel'), send });
const account = createTinyAccount({
  transmit: rawSend,
  onState(state) { accountReady = state.ready; renderGame(); },
  onError(text) { $('#tiny-session-notice').textContent = text; },
  onSnapshot(snapshot) { if (connected && socket?.readyState === WebSocket.OPEN) consumeSnapshot(snapshot); },
});

// Menu links select presentation only. An activity still needs its explicit
// control and the current server-owned account/lease checks.
const roundShortcut = document.createElement('div');
roundShortcut.id = 'tiny-active-round';
roundShortcut.className = 'tiny-panel-copy';
roundShortcut.hidden = true;
roundShortcut.setAttribute('role', 'status');
const roundShortcutCopy = document.createElement('p');
const returnToRound = document.createElement('button');
returnToRound.type = 'button'; returnToRound.className = 'small-button';
returnToRound.textContent = 'Return to round';
returnToRound.addEventListener('click', () => selectScreen('games', true));
const stopFromMenu = document.createElement('button');
stopFromMenu.type = 'button'; stopFromMenu.className = 'small-button';
stopFromMenu.textContent = 'Stop round';
stopFromMenu.addEventListener('click', () => $('#tiny-stop').click());
roundShortcut.append(roundShortcutCopy, returnToRound, stopFromMenu);
document.querySelector('.tiny-screen-nav').after(roundShortcut);

function tinyScreenFromHash(hash) {
  const screen = hash === '' ? 'garden' : typeof hash === 'string' && hash.startsWith('#') ? hash.slice(1) : null;
  return ['garden', 'stats', 'training', 'games', 'items'].includes(screen) ? screen : null;
}

function selectScreen(screen, focus = false) {
  if (!['garden', 'stats', 'training', 'games', 'items'].includes(screen)) return;
  resetMovement(); selectedScreen = screen;
  if (location.hash !== `#${screen}`) history.replaceState(null, '', `#${screen}`);
  document.querySelectorAll('[data-panel]').forEach(panel => { panel.hidden = panel.dataset.panel !== screen; });
  document.querySelectorAll('[data-screen]').forEach(button => {
    if (button.dataset.screen === screen) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  });
  renderGame();
  if (focus) {
    const panel = document.querySelector(`[data-panel="${screen}"]`);
    panel.tabIndex = -1; panel.focus({ preventScroll: true });
  }
}

function renderRoundShortcut() {
  roundShortcut.hidden = !tinyGame || selectedScreen === 'games';
  if (roundShortcut.hidden) return;
  const saved = durableTinyResult(tinyGame.result, tinyGame) || lastResultId === tinyGame.id;
  const waiting = !saved && ['saving', 'finished'].includes(tinyGame.phase);
  roundShortcutCopy.textContent = saved ? 'Your saved round is waiting in Games.'
    : waiting ? 'Your round is saving on the server.'
      : `${games[tinyGame.gameId]?.title || 'Your round'} continues on the server. Return to play or stop the round.`;
  stopFromMenu.hidden = saved || waiting;
  stopFromMenu.disabled = $('#tiny-stop').disabled;
}

function renderConnection() {
  $('#tiny-sync-note').textContent = lastSyncedAt
    ? `${connected ? 'Live · Synced' : 'Last synced'} at ${observedTime(lastSyncedAt)} on this device${connected ? '.' : '. Reconnect before taking actions.'}`
    : 'Waiting for a canonical server snapshot.';
  $('#tiny-save-note').textContent = pendingSave ? 'Waiting for the server to save…'
    : lastSavedAt ? `Save confirmed at ${observedTime(lastSavedAt)} on this device.` : 'No save acknowledgement received this visit.';
  $('#tiny-save').disabled = !connected || pendingSave;
  $('#tiny-reconnect').disabled = !token || connecting;
  const foreignLease = accountV2 && account.identity?.lease && account.identity.lease.deviceId !== account.identity.deviceId;
  $('#tiny-take-control').hidden = !foreignLease;
  $('#tiny-take-control').disabled = !connected || !accountReady || account.pending;
  if (foreignLease) $('#tiny-session-notice').textContent = 'Another device holds control. The server decides when it can hand over.';
}

function requestSave(returnToDesktop = false) {
  if (!connected || pendingSave) return;
  resetMovement(); pendingSave = true; returnAfterSave = returnToDesktop;
  send({ type: 'action', action: 'save' }); renderConnection();
  clearTimeout(saveTimer); saveTimer = setTimeout(() => {
    pendingSave = false; returnAfterSave = false; renderConnection();
    $('#tiny-save-note').textContent = 'Save not confirmed. Reconnect or try Save again.';
  }, 10_000);
}

function notice(text = '', error = false) {
  $('#tiny-notice').textContent = text;
  $('#tiny-notice').classList.toggle('error', error);
}

function connectionLabel(status, text) {
  $('#tiny-connection').dataset.status = status;
  $('#tiny-connection>span').textContent = text;
}

function publicState() { return tinyGame?.view?.state || null; }
function mayAct() { return connected && (!accountV2 || (accountReady && (!account.identity?.lease || account.identity.lease.deviceId === account.identity.deviceId))); }
function playing() { return mayAct() && !document.hidden && selectedScreen === 'games' && tinyGame?.phase === 'playing' && !publicState()?.ended && !pendingStop; }
function mode() { return tinyGame?.gameId || tinyGame?.view?.game || selectedGame; }
function movementGame() { return ['bounce', 'ball', 'search'].includes(mode()); }
function rawSend(message) {
  if (socket?.readyState === WebSocket.OPEN && connected) { socket.send(JSON.stringify(message)); return true; }
  return false;
}
function send(message) {
  if (connected && message.type === 'action' && message.action === 'save') return accountV2 ? account.send(message) : rawSend(message);
  if (!mayAct()) return false;
  return accountV2 ? account.send(message) : rawSend(message);
}
function intent(input) {
  if (!playing()) return;
  if (accountV2 && account.pending && input.kind !== 'move') {
    const id = tinyGame.id;
    void account.whenIdle().then(ready => { if (ready && playing() && tinyGame?.id === id) send({ type: 'tiny-input', sessionId: id, input }); });
    return;
  }
  send({ type: 'tiny-input', sessionId: tinyGame.id, input });
}

function displayNumber(value) { return Number.isFinite(value) ? value.toLocaleString() : '—'; }
function seconds(value) { return Number.isFinite(value) ? `${Math.ceil(Math.max(0, value) / 1000)}s` : '—'; }

function renderSlots() {
  try { savedSlots = saves.list(); } catch { savedSlots = []; }
  return savedSlots;
}

function rememberPlayer(savedAt) {
  if (!token || !player) return;
  try { saves.record(token, player, savedAt); } catch { /* The server remains the save authority. */ }
}

function renderPlayer() {
  if (!player) {
    $('#tiny-name').textContent = 'Meet you in the garden'; $('#tiny-form').textContent = 'Connect your saved Chao.';
    $('#tiny-rings').textContent = '—'; $('#tiny-stats-empty').hidden = false;
    for (const row of statRows.values()) row.hidden = true;
    renderConnection(); return;
  }
  $('#tiny-name').textContent = player.name || 'Chao';
  $('#tiny-form').textContent = chaoFormLabel(player);
  $('#tiny-rings').textContent = displayNumber(player.rings);
  $('#tiny-auth').hidden = connected;
  $('#tiny-login').hidden = connected;
  $('#tiny-stats-empty').hidden = !!player.stats;
  for (const name of statNames) {
    const stat = player.stats?.[name];
    if (!stat || !Number.isFinite(stat.level) || !Number.isFinite(stat.points) || !Number.isFinite(stat.grade)) {
      if (statRows.has(name)) statRows.get(name).hidden = true;
      continue;
    }
    let row = statRows.get(name);
    if (!row) { row = createStatRow(name); statRows.set(name, row); $('#tiny-stats').append(row); }
    row.hidden = false;
    updateStatRow(row, name, stat);
  }
  profileCanvas.setAttribute('aria-label', `${player.name || 'Chao'} · ${chaoFormLabel(player)}`);
  profileCanvas.dataset.variant = chaosVariant(player) || 'normal';
  renderConnection();
}

function resetMovement(sendStop = true) {
  const wasMoving = lastMove.x || lastMove.y || canvasMove.x || canvasMove.y || heldMoves.size || heldKeys.size;
  if (sendStop && wasMoving && mayAct() && movementGame() && tinyGame?.phase === 'playing') send({ type: 'tiny-input', sessionId: tinyGame.id, input: { kind: 'move', x: 0, y: 0 } });
  heldMoves.clear(); heldKeys.clear();
  canvasMove = { x: 0, y: 0 }; lastMove = { x: 0, y: 0 }; pointer = null;
  document.querySelectorAll('[data-move]').forEach(button => button.classList.remove('active'));
}

function moveDirection() {
  let x = canvasMove.x, y = canvasMove.y;
  for (const move of heldMoves.values()) { x += move.x; y += move.y; }
  x += Number(heldKeys.has('ArrowRight') || heldKeys.has('KeyD')) - Number(heldKeys.has('ArrowLeft') || heldKeys.has('KeyA'));
  y += Number(heldKeys.has('ArrowDown') || heldKeys.has('KeyS')) - Number(heldKeys.has('ArrowUp') || heldKeys.has('KeyW'));
  return { x: Math.sign(x), y: mode() === 'search' ? Math.sign(y) : 0 };
}

function flushMove(force = false) {
  if (!playing() || !movementGame() || document.hidden) return;
  const direction = moveDirection();
  if (force || direction.x !== lastMove.x || direction.y !== lastMove.y) {
    intent({ kind: 'move', ...direction }); lastMove = direction;
  }
}

function actionButton(label, callback, className = '') {
  const button = document.createElement('button');
  button.type = 'button'; button.textContent = label; button.className = className;
  button.addEventListener('click', callback);
  return button;
}

function makeControls() {
  const sessionKey = `${tinyGame?.id || ''}:${mode()}`;
  if (controlsSession === sessionKey) return;
  controlsSession = sessionKey;
  resetMovement(false);
  const active = mode(), actions = $('#tiny-action-controls');
  actions.replaceChildren();
  const movePad = $('#tiny-move-controls');
  movePad.hidden = !['bounce', 'ball', 'search'].includes(active);
  movePad.classList.toggle('horizontal', active !== 'search');
  if (active === 'memory') {
    const button = actionButton('Trip the shuffle', () => intent({ kind: 'memory_trip' }));
    button.id = 'tiny-trip'; actions.append(button);
  } else if (active === 'rock_paper_scissors') {
    const selectors = document.createElement('div'); selectors.className = 'rps-selectors';
    for (let slot = 0; slot < 3; slot++) {
      const button = actionButton('', () => intent({ kind: 'rps_select', slot }));
      button.dataset.rpsSlot = String(slot);
      button.append(document.createElement('span'), document.createElement('small'));
      selectors.append(button);
    }
    actions.append(selectors, actionButton('Shoot ↑', () => intent({ kind: 'rps_shoot' }), 'emphasized'));
  } else if (active === 'bounce') {
    const button = actionButton('Spring ↟', () => intent({ kind: 'bounce_spring' }), 'emphasized');
    button.id = 'tiny-spring'; actions.append(button);
  } else if (active === 'ball') {
    actions.append(actionButton('● Red', () => intent({ kind: 'ball_shoot', color: 'red' }), 'shot-red'), actionButton('● Green', () => intent({ kind: 'ball_shoot', color: 'green' }), 'shot-green'));
  } else if (active === 'search') {
    actions.append(actionButton('Pick up', () => {
      const state = publicState();
      const nearest = nearestPoint(state?.chao?.filter(chao => !chao.carried), state?.player_x, state?.player_y);
      if (nearest) intent({ kind: 'search_pick_up', chao: nearest.id });
    }), actionButton('Get hint ◈', () => {
      const state = publicState(), nearest = nearestPoint(state?.orbs, state?.player_x, state?.player_y);
      if (nearest) intent({ kind: 'search_hint', orb: nearest.id });
    }), actionButton('Return HOME', () => intent({ kind: 'search_return' }), 'emphasized'));
  }
}

function nearestPoint(points, x, y) {
  if (!Array.isArray(points) || !Number.isFinite(x) || !Number.isFinite(y)) return null;
  return points.reduce((nearest, point) => !nearest || Math.hypot(point.x - x, point.y - y) < Math.hypot(nearest.x - x, nearest.y - y) ? point : nearest, null);
}

function renderMemory(state) {
  const board = $('#tiny-memory-board');
  const cards = state?.cards || [];
  if (board.children.length !== cards.length) {
    board.replaceChildren();
    cards.forEach((_card, cell) => {
      const button = document.createElement('button'); button.className = 'memory-card'; button.type = 'button';
      button.addEventListener('click', () => intent({ kind: 'memory_flip', cell })); board.append(button);
    });
  }
  cards.forEach((card, index) => {
    const button = board.children[index];
    const revealed = Number.isInteger(card.face) && card.face >= 0 && card.face < colors.length;
    button.className = `memory-card${card.occupied ? '' : ' empty'}${card.matched ? ' matched' : ''}${revealed ? ' revealed' : ''}${state.moving_card?.includes(index) && state.phase === 'shuffle' ? ' moving' : ''}`;
    button.disabled = !playing() || (accountV2 && account.pending) || state.phase !== 'choose' || !card.occupied || card.matched || revealed;
    button.setAttribute('aria-label', card.matched ? `Matched ${colors[card.face] || ''} card` : revealed ? `${colors[card.face]} emerald card` : `Hidden card ${index + 1}`);
    // Replace the face entirely when the server hides it; no secret board or
    // previous reveal is retained in DOM attributes or client game state.
    button.replaceChildren();
    if (revealed) { const image = document.createElement('img'); image.src = faceArt[card.face].src; image.alt = ''; button.append(image); }
    else { const back = document.createElement('span'); back.className = 'card-back'; back.textContent = '◈'; button.append(back); }
    const value = document.createElement('small'); value.textContent = String(card.value); button.append(value);
  });
}

function metricItems(values) {
  $('#tiny-game-metrics').replaceChildren(...values.map(value => { const span = document.createElement('span'); span.textContent = value; return span; }));
}

function renderGame() {
  training.update(player, { connected: mayAct(), reconciled: connected, busy: !!tinyGame || pendingStart || !!player?.watch || (accountV2 && account.pending), capabilities, serverTime, care: careState });
  gardenWatch.update(player, { connected: mayAct(), reconciled: connected, busy: !!tinyGame || pendingStart || training.blocksActivity || (accountV2 && account.pending) });
  items.update(player, { connected: mayAct(), care: careState, accountsV2: accountV2, serverTime,
    careMutations: !accountV2 && careState?.rulesVersion === 'adapted-care-v1' });
  renderConnection();
  const active = mode(), definition = games[active] || games.memory, state = publicState();
  $('#tiny-game-number').textContent = `GAME ${definition.number}`;
  $('#tiny-game-title').textContent = definition.title;
  $('#tiny-instructions').textContent = definition.intro;
  $('#tiny-control-tip').textContent = definition.tip;
  $('#tiny-stage').dataset.game = active;
  document.querySelectorAll('#tiny-games button[data-game]').forEach(button => {
    button.classList.toggle('selected', button.dataset.game === active);
    button.setAttribute('aria-pressed', String(button.dataset.game === active));
  });
  const hasGame = !!tinyGame;
  const saved = !!tinyGame && (durableTinyResult(tinyGame.result, tinyGame) || lastResultId === tinyGame.id);
  const waiting = !saved && (tinyGame?.phase === 'saving' || tinyGame?.phase === 'finished');
  $('#tiny-stage-cover').hidden = connected && hasGame && !!state;
  $('#tiny-canvas').hidden = active === 'memory' && hasGame && !!state;
  $('#tiny-memory-board').hidden = active !== 'memory' || !hasGame || !state;
  $('#tiny-game-controls').hidden = !hasGame || !state;
  $('#tiny-stop').hidden = !hasGame;
  $('#tiny-stop').disabled = !mayAct() || pendingStop || waiting || saved;
  renderRoundShortcut();
  $('#tiny-start').disabled = !mayAct() || !player || hasGame || pendingStart || connecting || !!player?.watch || training.blocksActivity;
  $('#tiny-start').textContent = pendingStart ? 'Starting your round…' : !connected ? 'Connect a saved Chao' : player?.watch ? 'Take control to play' : hasGame ? (saved ? 'Round complete' : waiting ? 'Saving result…' : 'Round in progress') : `Play ${definition.title}`;
  $('#tiny-phase').textContent = pendingStop ? 'Stopping round…' : saved ? 'Result saved' : waiting ? 'Saving result…' : !hasGame ? 'Ready when you are' : 'Playing';
  $('#tiny-cover-title').textContent = connected ? definition.title : 'A little challenge awaits.';
  $('#tiny-cover-copy').textContent = connected ? 'Tap Play to start a fresh round.' : 'Connect your saved Chao and start a round.';
  if (!state || !hasGame) { metricItems(definition.metrics); return; }
  makeControls();
  $('#tiny-action-controls').querySelectorAll('button').forEach(button => { button.disabled = !playing() || (accountV2 && account.pending); });
  $('#tiny-move-controls').querySelectorAll('button').forEach(button => { button.disabled = !playing(); });
  if (active === 'memory') {
    const names = { peek: 'Remember these!', shuffle: 'Watch the shuffle', mismatch: 'Try another pair', choose: 'Find a pair', ended: 'Round complete' };
    $('#tiny-phase').textContent = saved ? 'Result saved' : waiting ? 'Saving result…' : names[state.phase] || 'Playing';
    metricItems([`${state.matches}/7 pairs`, `${state.mistakes}/3 mistakes`, `${displayNumber(state.rings)} run rings`]);
    renderMemory(state);
    $('#tiny-trip').disabled = !playing() || (accountV2 && account.pending) || !state.trip_available;
  } else if (active === 'rock_paper_scissors') {
    metricItems([`${seconds(state.remaining_ms)} left`, `${state.targets?.length || 0} targets`, `${displayNumber(state.rings)} run rings`]);
    document.querySelectorAll('[data-rps-slot]').forEach(button => {
      const index = Number(button.dataset.rpsSlot), hand = state.cards?.[index];
      button.querySelector('span').textContent = handSymbols[hand] || '…';
      button.querySelector('small').textContent = hand ? hand[0].toUpperCase() + hand.slice(1) : 'In flight';
      button.classList.toggle('selected', state.selected === index);
      button.setAttribute('aria-pressed', String(state.selected === index));
      button.disabled = !playing() || (accountV2 && account.pending) || !hand;
    });
  } else if (active === 'bounce') {
    metricItems([`Level ${state.level}/4`, `${state.lives} lives`, `${displayNumber(state.earned_rings)} run rings`]);
    const spring = $('#tiny-spring'), ready = state.compression_ms >= 40 && state.compression_ms <= 80;
    spring.classList.toggle('spring-ready', ready); spring.textContent = ready ? 'Spring NOW! ↟' : state.waiting ? 'Launch ↟' : 'Spring ↟';
  } else if (active === 'ball') metricItems([`Stage ${state.stage}`, `${state.cleared_balls} balls cleared`, `${displayNumber(state.rings)} run rings`]);
  else if (active === 'search') {
    metricItems([`Find ${state.request_name}`, `${state.found}/7 home`, `${seconds(state.remaining_ms)} left`]);
    if (state.hints?.length) notice(state.hints.at(-1));
  }
}

function showResult(result, fromSnapshot = false) {
  if (!connected || (!fromSnapshot && !tinyGame) || !durableTinyResult(result, tinyGame) || result.id === lastResultId) return;
  lastResultId = result.id; lastSavedAt = Date.now();
  $('#tiny-result').hidden = false;
  $('#tiny-result-title').textContent = result.completed ? 'A lovely little victory.' : 'Round complete.';
  const duration = Number.isFinite(result.elapsedMs) ? `${(result.elapsedMs / 1000).toFixed(1)}s` : '';
  $('#tiny-result-detail').textContent = [String(result.reason || '').replaceAll('_', ' '), duration].filter(Boolean).join(' · ');
  $('#tiny-result-rings').textContent = Number.isFinite(result.rings) ? `+${displayNumber(result.rings)} ◎` : 'Saved';
  notice('Your result is saved. Your ring balance comes from the server.');
  rememberPlayer(); renderPlayer(); renderGame();
}

function consumeSnapshot(next) {
  const previousState = publicState();
  const self = next.self || next.players?.find(row => row.id === next.selfId);
  if (accountV2 && (!next.identity || next.identity.revision < account.identity?.revision)) return;
  if (!self || (player?.id && self.id !== player.id)) return;
  player = self; connected = true; connecting = false; retryCount = 0;
  clearTimeout(handshakeTimer);
  connectionLabel('live', 'Garden connected');
  serverTime = Number.isFinite(next.serverTime) ? next.serverTime : null;
  careState = next.care || null;
  lastSyncedAt = Date.now();
  if (lastSyncedAt - lastCachedAt >= 5_000) { writeTinyCache(storage, player, lastSyncedAt); lastCachedAt = lastSyncedAt; }
  const before = tinyGame?.id;
  tinyGame = next.tinyGame || null;
  if (tinyGame) { selectedGame = tinyGame.gameId || tinyGame.view?.game || selectedGame; if (pendingStart) selectScreen('games'); pendingStart = false; clearTimeout(pendingTimer); }
  if (!tinyGame) pendingStop = false;
  if (before !== tinyGame?.id) { pendingStop = false; resetMovement(false); controlsSession = ''; avatarMotion = { direction: 0, changedAt: 0 }; }
  else if (previousState && publicState()) {
    const state = publicState();
    const dx = mode() === 'bounce' ? state.chao_x - previousState.chao_x : state.player_x - previousState.player_x;
    const dy = mode() === 'search' ? state.player_y - previousState.player_y : 0;
    if (Number.isFinite(dx) && Number.isFinite(dy) && (dx || dy)) avatarMotion = { direction: nativeDirection(dx, dy, avatarMotion.direction), changedAt: performance.now() };
  }
  if (Date.now() - lastRememberedAt > 5_000) { rememberPlayer(); lastRememberedAt = Date.now(); }
  renderPlayer(); renderGame();
  if (next.tinyResult) showResult(next.tinyResult, true);
}

function openSocket(activeToken, generation) {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const current = new WebSocket(`${protocol}//${location.host}/ws?tiny=1${accountV2 ? '' : `&token=${encodeURIComponent(activeToken)}`}`);
  socket = current;
  current.onopen = () => {
    if (generation !== connectionGeneration) { current.close(); return; }
    connected = false; connectionLabel('connecting', 'Syncing your Chao…');
    if (accountV2) current.send(JSON.stringify({ type: 'authenticate', token: activeToken }));
    clearTimeout(handshakeTimer); handshakeTimer = setTimeout(() => { if (generation === connectionGeneration && !connected) current.close(); }, 10_000);
    notice(''); renderPlayer(); renderGame();
  };
  current.onmessage = event => {
    if (generation !== connectionGeneration) return;
    let message;
    try { message = JSON.parse(event.data); } catch { return; }
    if (accountV2) account.consume(message);
    if (message.type === 'error') { resetMovement(false); pendingStart = false; pendingStop = false; clearTimeout(pendingTimer); renderGame(); }
    if (message.type === 'snapshot') consumeSnapshot(message);
    if (message.type === 'notice') { gardenWatch.handleNotice?.(message); training.handleEvent(message); pendingStart = false; pendingStop = false; if (message.kind === 'error' && pendingSave) { clearTimeout(saveTimer); pendingSave = false; returnAfterSave = false; } notice(message.text || 'The server could not complete that action.', message.kind === 'error'); renderGame(); }
    if (message.type === 'event') {
      training.handleEvent(message);
      if (items.handleEvent(message) && message.accepted === true) lastSavedAt = Date.now();
      if (message.kind === 'watch-saved' && message.accepted === true) lastSavedAt = Date.now();
      if (message.kind === 'saved' && message.playerId === player?.id && pendingSave && connected) {
        clearTimeout(saveTimer); pendingSave = false; lastSavedAt = Date.now(); rememberPlayer(lastSavedAt);
        if (returnAfterSave) { returnAfterSave = false; location.assign('/'); }
      }
      renderConnection();
    }
    if (message.type === 'event' && message.kind === 'tiny-game-result') showResult(message);
  };
  current.onerror = () => connectionLabel('offline', 'Connection interrupted');
  current.onclose = event => {
    if (generation !== connectionGeneration) return;
    resetMovement(false); connected = false; connecting = false; tinyGame = null; controlsSession = ''; pendingStart = false; pendingStop = false;
    clearTimeout(handshakeTimer); account.disconnect();
    clearTimeout(saveTimer); pendingSave = false; returnAfterSave = false;
    connectionLabel('offline', 'Disconnected'); renderGame();
    notice('Connection lost. Controls stopped. An unfinished round may be canceled by the server.', true);
    if (event.code === 4001) { $('#tiny-auth').hidden = false; $('#tiny-auth-message').textContent = event.reason || 'Another Chao from this memory card is out now.'; return; }
    if (event.code !== 1008) scheduleReconnect(activeToken);
    else { $('#tiny-auth').hidden = false; $('#tiny-auth-message').textContent = 'Log in again to bring your Chao back.'; $('#tiny-login').hidden = false; }
  };
}

function scheduleReconnect(activeToken) {
  clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => resume(activeToken), Math.min(15_000, 2_000 * 2 ** Math.min(retryCount++, 3)));
}

// The player's own Chao pictures, when their PC stored a picture sheet.
async function useOwnChao(response, id) {
  const sheet = await response.blob?.();
  if (sheet?.type === 'image/png') setOwnAtlas(id, sheet);
}

function selectAccountProtocol(status) {
  const supported = supportsTinyAccounts(status);
  if ((knownAccountV2 && !supported) || (status && Object.hasOwn(status, 'accounts') && !supported)) {
    const error = new Error('This server uses an unsupported account protocol. Controls remain paused.');
    error.retry = false; throw error;
  }
  knownAccountV2 ||= supported;
  accountV2 = supported;
}

async function resume(activeToken) {
  const generation = ++connectionGeneration;
  clearTimeout(reconnectTimer);
  resumeAbort?.abort(); resumeAbort = new AbortController();
  const abort = resumeAbort;
  const fetchTimeout = setTimeout(() => abort.abort(), 10_000);
  resetMovement();
  account.disconnect();
  clearTimeout(saveTimer); pendingSave = false; returnAfterSave = false;
  if (socket) { socket.onclose = null; socket.close(); socket = null; }
  connected = false; connecting = true; tinyGame = null; controlsSession = ''; pendingStart = false; pendingStop = false;
  if (token !== activeToken) { player = null; careState = null; lastSyncedAt = null; lastCachedAt = 0; lastSavedAt = null; lastResultId = null; $('#tiny-result').hidden = true; }
  token = activeToken; renderPlayer();
  const slot = savedSlots.find(row => row.token === activeToken);
  const cached = readTinyCache(storage, slot?.id);
  if (!player && cached) { player = cached.player; lastSyncedAt = cached.syncedAt; renderPlayer(); }
  connectionLabel('connecting', 'Connecting…'); renderGame();
  try {
    const statusResponse = await fetch('/api/status', { signal: abort.signal, cache: 'no-store', redirect: 'error' });
    if (!statusResponse.ok) throw new Error('The server capabilities could not be confirmed. Reconnect before playing.');
    const status = await statusResponse.json();
    if (status?.ok !== true) throw new Error('The server capabilities could not be confirmed. Reconnect before playing.');
    if (!secureTransport) { const error = new Error('This server needs HTTPS before this device can connect to its saved Chao.'); error.retry = false; throw error; }
    if (generation !== connectionGeneration) return;
    selectAccountProtocol(status);
    capabilities = status?.tinyGarden || {};
    if (accountV2) {
      const result = await account.connect({ token: activeToken, status });
      if (generation !== connectionGeneration || !result) return;
      player = result.snapshot.self;
    } else {
      const response = await fetch('/api/session', { method: 'POST', signal: abort.signal, redirect: 'error', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: activeToken, resumeOnly: true }) });
      const data = await response.json();
      if (generation !== connectionGeneration) return;
      if (!response.ok || !data.player || typeof data.token !== 'string') { const error = new Error(data.error || 'This saved Chao is not available on this server.'); error.retry = response.status >= 500; throw error; }
      player = data.player; token = data.token;
      // The phone works once the same account has played on its PC: that stores the player's own Chao model.
      const model = await fetch('/api/tiny-chao', { signal: abort.signal, cache: 'no-store', redirect: 'error', headers: { Authorization: `Bearer ${token}` } });
      if (generation !== connectionGeneration) return;
      if (!model.ok) { const error = new Error(model.status === 404 ? 'Play once on your PC first. Then your Chao is here.' : 'Your Chao could not be loaded. Reconnect to try again.'); error.retry = model.status >= 500; throw error; }
      void useOwnChao(model, player.id);
    }
    rememberPlayer(); renderSlots(); renderPlayer(); openSocket(token, generation);
  } catch (error) {
    if (generation !== connectionGeneration) return;
    connecting = false; connectionLabel('offline', 'Save unavailable');
    $('#tiny-auth').hidden = false; $('#tiny-login').hidden = false; $('#tiny-auth-message').textContent = error.message;
    notice(error.message, true); renderGame();
    if (error.retry !== false) scheduleReconnect(activeToken);
  } finally {
    clearTimeout(fetchTimeout);
  }
}

document.querySelector('.tiny-screen-nav').addEventListener('click', event => {
  const button = event.target.closest('[data-screen]');
  if (button) {
    if (location.hash !== `#${button.dataset.screen}`) location.hash = button.dataset.screen;
    selectScreen(button.dataset.screen, true);
  }
});
window.addEventListener('hashchange', () => {
  const screen = tinyScreenFromHash(location.hash);
  if (screen) selectScreen(screen, true);
});
$('#tiny-save').addEventListener('click', () => requestSave());
$('#tiny-reconnect').addEventListener('click', () => { if (token) void resume(token); });
$('#tiny-take-control').addEventListener('click', async () => {
  if (!connected || !accountV2) return;
  await account.takeControl(player?.watch ? 'watch' : tinyGame ? 'tiny' : 'direct'); renderGame();
});
$('.tiny-desktop-link').addEventListener('click', event => { event.preventDefault(); requestSave(true); });
window.addEventListener('online', () => { if (token && !connected) void resume(token); });

$('#tiny-games').addEventListener('click', event => {
  const button = event.target.closest('[data-game]');
  if (!button) return;
  if (tinyGame) { notice('Finish or stop this round before choosing another game.'); return; }
  selectedGame = button.dataset.game; $('#tiny-result').hidden = true; notice(''); renderGame();
});
$('#tiny-start').addEventListener('click', async () => {
  if (!mayAct() || !player || tinyGame || pendingStart || player.watch || training.blocksActivity) return;
  pendingStart = true; $('#tiny-result').hidden = true; notice('');
  const sent = send({ type: 'action', action: 'tiny-start', gameId: selectedGame }); renderGame();
  clearTimeout(pendingTimer); pendingTimer = setTimeout(() => { if (pendingStart) { pendingStart = false; notice('The server has not started the round yet. Try again once connected.', true); renderGame(); } }, 8_000);
  if (await sent === false) { pendingStart = false; clearTimeout(pendingTimer); notice('The start was not confirmed. Check the synced state before trying again.', true); renderGame(); }
});
$('#tiny-stop').addEventListener('click', async () => {
  if (!tinyGame || !connected || pendingStop) return;
  resetMovement(false); const id = tinyGame.id; pendingStop = true; renderGame();
  if (accountV2 && !(await account.whenIdle())) { pendingStop = false; renderGame(); return; }
  if (!mayAct() || tinyGame?.id !== id) { pendingStop = false; renderGame(); return; }
  if (await send({ type: 'action', action: 'tiny-stop', sessionId: id }) === false && tinyGame?.id === id) { pendingStop = false; notice('The stop was not confirmed. Check the synced state before trying again.', true); renderGame(); }
});
$('#tiny-result-close').addEventListener('click', async () => {
  $('#tiny-result').hidden = true;
  if (tinyGame && connected) { const id = tinyGame.id; pendingStop = true; if (accountV2) await account.whenIdle(); if (tinyGame?.id === id && await send({ type: 'action', action: 'tiny-stop', sessionId: id }) === false) pendingStop = false; }
  notice(''); renderGame();
});

for (const button of document.querySelectorAll('[data-move]')) {
  button.addEventListener('pointerdown', event => {
    if (!playing()) return;
    event.preventDefault(); button.setPointerCapture(event.pointerId);
    const [x, y] = button.dataset.move.split(',').map(Number);
    heldMoves.set(event.pointerId, { x, y }); button.classList.add('active'); flushMove(true);
  });
  const release = event => { heldMoves.delete(event.pointerId); button.classList.remove('active'); flushMove(true); };
  button.addEventListener('pointerup', release); button.addEventListener('pointercancel', release); button.addEventListener('lostpointercapture', release);
}

function canvasPoint(event) {
  const rect = canvas.getBoundingClientRect();
  return { x: (event.clientX - rect.left) * canvas.width / rect.width, y: (event.clientY - rect.top) * canvas.height / rect.height };
}
canvas.addEventListener('pointerdown', event => {
  if (!playing()) return;
  event.preventDefault(); canvas.focus({ preventScroll: true }); canvas.setPointerCapture(event.pointerId);
  const point = canvasPoint(event); pointer = { id: event.pointerId, start: point, moved: false };
});
canvas.addEventListener('pointermove', event => {
  if (!pointer || pointer.id !== event.pointerId || !playing() || !movementGame()) return;
  const point = canvasPoint(event), dx = point.x - pointer.start.x, dy = point.y - pointer.start.y;
  if (Math.hypot(dx, dy) < 10 && !pointer.moved) return;
  pointer.moved = true;
  canvasMove = { x: Math.abs(dx) > 13 ? Math.sign(dx) : 0, y: mode() === 'search' && Math.abs(dy) > 13 ? Math.sign(dy) : 0 };
  flushMove();
});
function releaseCanvas(event, canceled = false) {
  if (!pointer || pointer.id !== event.pointerId) return;
  const tap = !pointer.moved && !canceled;
  pointer = null; canvasMove = { x: 0, y: 0 }; if (!tap) flushMove(true);
  if (!tap || !playing()) return;
  const point = canvasPoint(event);
  const hit = hitTargets.find(target => Math.hypot(point.x - target.x, point.y - target.y) <= target.radius);
  if (hit) intent(hit.input);
  else if (mode() === 'bounce') intent({ kind: 'bounce_spring' });
}
canvas.addEventListener('pointerup', event => releaseCanvas(event));
canvas.addEventListener('pointercancel', event => releaseCanvas(event, true));
canvas.addEventListener('lostpointercapture', event => releaseCanvas(event, true));

window.addEventListener('keydown', event => {
  if (!playing() || document.hidden || event.target.matches('input,select,textarea') || event.target.isContentEditable) return;
  if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'KeyW', 'KeyA', 'KeyS', 'KeyD'].includes(event.code) && movementGame()) {
    event.preventDefault(); heldKeys.add(event.code); flushMove();
  }
  if (event.repeat) return;
  if (mode() === 'rock_paper_scissors' && ['Digit1', 'Digit2', 'Digit3'].includes(event.code)) intent({ kind: 'rps_select', slot: Number(event.code.at(-1)) - 1 });
  if (mode() === 'ball' && ['KeyZ', 'KeyX'].includes(event.code)) intent({ kind: 'ball_shoot', color: event.code === 'KeyZ' ? 'red' : 'green' });
  if (event.code === 'Space' && !event.target.matches('button,a,summary')) {
    event.preventDefault();
    if (mode() === 'rock_paper_scissors') intent({ kind: 'rps_shoot' });
    if (mode() === 'bounce') intent({ kind: 'bounce_spring' });
    if (mode() === 'ball') intent({ kind: 'ball_shoot', color: 'red' });
    if (mode() === 'search') intent({ kind: 'search_return' });
  }
});
window.addEventListener('keyup', event => { heldKeys.delete(event.code); flushMove(); });
window.addEventListener('blur', () => resetMovement());
document.addEventListener('visibilitychange', () => {
  if (document.hidden) resetMovement();
  else if (socket?.readyState === WebSocket.OPEN) { connected = false; connectionLabel('connecting', 'Syncing your Chao…'); renderGame(); }
  else if (token) void resume(token);
});
window.addEventListener('pageshow', event => { if (event.persisted && token) void resume(token); });
window.addEventListener('pagehide', () => { resetMovement(); if (connected) writeTinyCache(storage, player, lastSyncedAt); clearTimeout(reconnectTimer); socket?.close(); });
setInterval(() => {
  if (!playing() || !movementGame()) return;
  const direction = moveDirection();
  if (direction.x || direction.y || lastMove.x || lastMove.y) flushMove(true);
}, 100);

// Everything below presents the latest sanitized server view. There is no
// local physics, hidden board, seed, collision, timer progression or reward.
function rounded(x, y, width, height, radius, color) {
  ctx.fillStyle = color; ctx.beginPath(); ctx.roundRect(x, y, width, height, radius); ctx.fill();
}
function text(value, x, y, size = 18, color = '#e4edf9', align = 'center') {
  ctx.font = `600 ${size}px "DM Sans",system-ui,sans-serif`; ctx.fillStyle = color; ctx.textAlign = align; ctx.textBaseline = 'middle'; ctx.fillText(String(value), x, y);
}
function circle(x, y, radius, fill, border = null, lineWidth = 2) {
  ctx.beginPath(); ctx.arc(x, y, radius, 0, Math.PI * 2); ctx.fillStyle = fill; ctx.fill();
  if (border) { ctx.strokeStyle = border; ctx.lineWidth = lineWidth; ctx.stroke(); }
}
function ring(x, y, radius = 8) {
  ctx.strokeStyle = '#f2d77e'; ctx.lineWidth = 4; ctx.beginPath(); ctx.ellipse(x, y, radius * 0.75, radius, 0, 0, Math.PI * 2); ctx.stroke();
}
function portrait(x, y, radius = 18) {
  ctx.save(); ctx.fillStyle = '#020c1855'; ctx.beginPath(); ctx.ellipse(x, y + radius * 0.8, radius * 0.65, radius * 0.2, 0, 0, Math.PI * 2); ctx.fill(); ctx.restore();
  drawTinyAvatar(ctx, player, x, y, radius * 2.7, { time: renderTime, direction: avatarMotion.direction, walking: ['ball', 'search'].includes(mode()) && renderTime - avatarMotion.changedAt < 180 });
}
function field(background = '#102139') {
  rounded(0, 0, 600, 600, 0, background);
  ctx.strokeStyle = '#7796ba13'; ctx.lineWidth = 1;
  for (let i = 50; i <= 550; i += 50) { ctx.beginPath(); ctx.moveTo(i, 25); ctx.lineTo(i, 575); ctx.stroke(); ctx.beginPath(); ctx.moveTo(25, i); ctx.lineTo(575, i); ctx.stroke(); }
  ctx.strokeStyle = '#6685a944'; ctx.lineWidth = 2; ctx.strokeRect(25, 25, 550, 550);
}
const pointDown = (x, y) => ({ x: 50 + x * 5, y: 35 + y * 5 });
const pointUp = (x, y) => ({ x: 50 + x * 5, y: 560 - y * 5 });

function drawRps(state) {
  field();
  const center = pointDown(50, 40);
  ctx.setLineDash([4, 10]); ctx.strokeStyle = '#b6a16b55'; ctx.lineWidth = 2; ctx.beginPath(); ctx.arc(center.x, center.y, 120, 0, Math.PI * 2); ctx.stroke(); ctx.setLineDash([]);
  text('TIME YOUR SHOT', 300, 58, 13, '#91a9c5');
  for (const target of state.targets || []) { const p = pointDown(target.x, target.y); circle(p.x, p.y, 23, '#283f5b', '#9db6cc'); text(handSymbols[target.hand] || '?', p.x, p.y, 27); }
  for (const shot of state.shots || []) { const p = pointDown(shot.x, shot.y); circle(p.x, p.y, 13, '#635836', '#f5db88'); text(handSymbols[shot.hand] || '↑', p.x, p.y, 20); }
  const launcher = pointDown(50, 94); portrait(launcher.x, launcher.y, 23);
  text(`${state.replacements} replacement cards`, 300, 426, 15, '#a8bed7');
}

function drawBounce(state) {
  field('#13283c');
  for (const target of state.rings || []) { const p = pointUp(target.x, target.y); ring(p.x, p.y, 9); }
  for (const balloon of state.balloons || []) {
    const p = pointUp(balloon.x, balloon.y); circle(p.x, p.y, 23, '#c78ba6', '#f0bfd0');
    ctx.strokeStyle = '#e8c9d0'; ctx.lineWidth = 2; ctx.beginPath(); ctx.moveTo(p.x, p.y + 23); ctx.bezierCurveTo(p.x - 10, p.y + 34, p.x + 10, p.y + 38, p.x, p.y + 44); ctx.stroke();
  }
  const spring = pointUp(state.spring_x, 6), compression = Number(state.compression_ms) || 0;
  const height = compression ? 12 : 24;
  rounded(spring.x - 30, spring.y + 9, 60, 8, 3, '#c9a563');
  ctx.strokeStyle = compression >= 40 && compression <= 80 ? '#9bf2cc' : '#e1e9f4'; ctx.lineWidth = 4; ctx.beginPath();
  for (let i = 0; i <= 5; i++) { const x = spring.x + (i % 2 ? 17 : -17), y = spring.y + 8 - i * height / 5; if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); } ctx.stroke();
  rounded(spring.x - 31, spring.y - height, 62, 7, 3, '#e7ca7b');
  const spare = pointUp(state.spring_x - 9, 3); rounded(spare.x - 12, spare.y, 24, 5, 2, '#5a7c98');
  const chao = pointUp(state.chao_x, state.chao_y); portrait(chao.x, chao.y - 14, 19);
  text(`BOOST ${state.boost_level}/4`, 300, 51, 14, '#b5dbdf');
}

function drawBall(state) {
  field('#15273a');
  const columns = state.columns || 8, cell = 52, left = (600 - columns * cell) / 2, top = 42, rowHeight = 35;
  const position = (x, y) => ({ x: left + (x + 0.5) * cell, y: top + (y + 0.5) * rowHeight });
  for (let row = 0; row < (state.grid || []).length; row++) for (let column = 0; column < state.grid[row].length; column++) {
    const color = state.grid[row][column]; if (!color) continue;
    const p = position(column, row + (state.board_offset || 0));
    circle(p.x, p.y, 15, color === 'red' ? '#e8787d' : '#65bda4', color === 'red' ? '#ffc2b9' : '#abedce');
    circle(p.x - 4, p.y - 4, 4, '#ffffff44');
  }
  for (const shot of state.shots || []) { const p = position(shot.x, shot.y); circle(p.x, p.y, 10, shot.color === 'red' ? '#f47d84' : '#79d5ae', '#eaf4ef'); }
  for (const item of [...(state.embedded_rings || []), ...(state.falling_rings || [])]) { const p = position(item.x, item.y); ring(p.x, p.y, 8); }
  const chao = position(state.player_x, 12.85); portrait(chao.x, chao.y, 21);
  ctx.setLineDash([5, 8]); ctx.strokeStyle = '#d6b66a88'; ctx.lineWidth = 2; ctx.beginPath(); ctx.moveTo(left, top + 12.8 * rowHeight); ctx.lineTo(left + columns * cell, top + 12.8 * rowHeight); ctx.stroke(); ctx.setLineDash([]);
  text('CATCH THE FALLING RINGS', 300, 566, 12, '#b9c7d9');
}

function drawSearch(state) {
  field('#1b3539');
  rounded(50 + 42 * 5, 35, 16 * 5, 500, 0, '#526568'); rounded(50, 35 + 85 * 5, 500, 75, 0, '#526568');
  for (const orb of state.orbs || []) {
    const p = pointDown(orb.x, orb.y); circle(p.x, p.y, 17, '#385f66', '#9be0da'); text('?', p.x, p.y, 19, '#bdf5ea');
    hitTargets.push({ ...p, radius: 26, input: { kind: 'search_hint', orb: orb.id } });
  }
  const home = pointDown(state.entrance_x, state.entrance_y);
  rounded(home.x - 40, home.y - 18, 80, 36, 9, '#9d874e'); text('HOME', home.x, home.y, 12, '#fff1bd');
  hitTargets.push({ ...home, radius: 34, input: { kind: 'search_return' } });
  for (const chao of state.chao || []) {
    if (chao.carried) continue;
    const p = pointDown(chao.x, chao.y);
    circle(p.x, p.y, 13, chao.terrain === 'grass' ? '#8fc9ad' : '#a7b8d4', '#d7e5d9');
    const symbol = { napping: 'z', dancing: '♪', walking: '↗', sitting: '·' }[chao.activity] || '·';
    text(symbol, p.x, p.y - 1, 17, '#1b3944');
    hitTargets.push({ ...p, radius: 22, input: { kind: 'search_pick_up', chao: chao.id } });
  }
  const p = pointDown(state.player_x, state.player_y); portrait(p.x, p.y, 17);
  if (state.carrying !== null && state.carrying !== undefined) { circle(p.x + 15, p.y - 18, 10, '#f0d58e', '#fff4bf'); text('♥', p.x + 15, p.y - 18, 11, '#6a4f38'); }
}

function draw(time = 0) {
  if (selectedScreen === 'garden') gardenWatch.draw(time);
  renderTime = time;
  profileContext.clearRect(0, 0, profileCanvas.width, profileCanvas.height);
  drawTinyAvatar(profileContext, player, 100, 113, 158, { time: connected ? time : 0 });
  const state = publicState(); hitTargets = [];
  if (selectedScreen === 'games' && state && tinyGame) {
    const active = mode();
    if (active === 'rock_paper_scissors') drawRps(state);
    else if (active === 'bounce') drawBounce(state);
    else if (active === 'ball') drawBall(state);
    else if (active === 'search') drawSearch(state);
    else field();
  } else field();
  requestAnimationFrame(draw);
}

selectScreen(tinyScreenFromHash(location.hash) || 'garden');
draw();
const initialSlots = renderSlots();
if (initialSlots.length) void resume(initialSlots[0].token);
else {
  $('#tiny-auth-message').textContent = 'Log in to see your Chao.'; connectionLabel('offline', 'Not logged in');
  void fetch('/api/status', { cache: 'no-store', redirect: 'error' }).then(response => response.ok ? response.json() : null).then(status => {
    selectAccountProtocol(status); capabilities = status?.tinyGarden || {}; renderGame();
  }).catch(() => {});
}

// The same login as the PC title page: username or email with a password, or the memory card code.
let rememberLogin = true;
document.querySelectorAll('#tiny-login .auth-tabs button').forEach(tab => tab.addEventListener('click', () => {
  document.querySelectorAll('#tiny-login .auth-tabs button').forEach(other => {
    other.setAttribute('aria-selected', String(other === tab)); $(`#${other.dataset.auth}`).hidden = other !== tab;
  });
}));
async function logIn(form, input) {
  const submit = form.querySelector('button');
  if (submit.disabled) return;
  submit.disabled = true; $('#tiny-login-error').textContent = '';
  try {
    const chao = await tinyLogin(fetch, input);
    form.reset(); rememberLogin = $('#tiny-remember').checked;
    try { saves.record(chao.token, chao.player); } catch { /* This visit still works without storage. */ }
    renderSlots(); await resume(chao.token);
  } catch (error) { $('#tiny-login-error').textContent = error.message || 'Could not log in.'; }
  finally { submit.disabled = false; }
}
$('#tiny-login-form').addEventListener('submit', event => { event.preventDefault(); void logIn(event.currentTarget, { login: $('#tiny-login-name').value.trim(), password: $('#tiny-login-password').value }); });
$('#tiny-card-form').addEventListener('submit', event => { event.preventDefault(); void logIn(event.currentTarget, { code: $('#tiny-card-code').value }); });
// Without "Remember me" the login ends with the visit.
window.addEventListener('pagehide', () => { if (!rememberLogin) { try { for (const slot of saves.list()) saves.remove(slot.token); } catch { /* storage blocked */ } } });

let installPrompt;
window.addEventListener('beforeinstallprompt', event => { event.preventDefault(); installPrompt = event; $('#tiny-install').hidden = false; });
$('#tiny-install').addEventListener('click', async () => { if (!installPrompt) return; await installPrompt.prompt(); installPrompt = null; $('#tiny-install').hidden = true; });
window.addEventListener('appinstalled', () => { installPrompt = null; $('#tiny-install').hidden = true; });
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => { navigator.serviceWorker.register('/tiny-sw.js', { scope: '/tiny.html' }).catch(() => { /* A failed shell cache does not affect authoritative play. */ }); });
}
