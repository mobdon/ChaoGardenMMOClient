import './style.css';
import './sonic.css';
import './title.css';
import { createGarden } from './scene.js';
import { createGameAudio } from './audio.js';
import { createStageSelect, SA2_PAGE } from './stages.js';
// ChaoMain area numbers: 1 Neutral, 2 Hero, 3 Dark, 5 race Entrance.
const chaoWorldAreas = { garden: 1, hero: 2, dark: 3, race: 5 };
import { createStatRow, updateStatRow } from './stat-ui.js';
import { createSaveSlots, SESSION_KEY } from './save-slots.js';
import { createGardenCare } from './garden-care.js';
import { createBreedingMenu } from './breeding-menu.js';
import { INTERACTION_ANCHORS } from '../shared/constants.js';

const $ = (selector) => document.querySelector(selector);
const welcome = $('#welcome-dialog');
const setupDialog = $('#setup-dialog');
const shopDialog = $('#shop-dialog');
const stagesDialog = $('#stages-dialog');
const tokenKey = SESSION_KEY;
const saveSlots = createSaveSlots(localStorage);
let socket;
let snapshot;
let player;
let sessionToken = localStorage.getItem(tokenKey);
let sessionCard = null; // the memory card code, when this browser has it
let reconnectTimer;
let currentPanel = 'world';
let setup;
let lastZone;
let joining = false;
let saving = false;
let pendingSave;
let lastSlotUpdate = 0;
let storageWarning = false;
let watchPending = null;
let watchTimer;
const audio = createGameAudio($('#audio-toggle'), $('#garden'), {
  volumeInput: $('#music-volume'), volumeOutput: $('#music-volume-value'), voiceButton: $('#voices-toggle'),
});
const gardenCare = createGardenCare({ root: $('#garden-care-panel'), send: message => {
  if (socket?.readyState !== WebSocket.OPEN) return false;
  socket.send(JSON.stringify(message)); return true;
}, hatch: hatchEgg });
// An egg (carried {itemId}, or bred in the nest {eggId}) hatches into a new slot on this memory card.
async function hatchEgg(egg, name) {
  // The egg hatches into a new slot on this memory card; this Chao keeps playing.
  if (!sessionCard) throw new Error('Insert your memory card first to hatch an egg.');
  const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, card: sessionCard, egg: { from: sessionToken, ...egg } }) });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || 'The egg could not hatch.');
  saveSlots.record(data.token, data.player, undefined, sessionCard);
  // After naming, the egg hatches as the new Chao's intro, under the inventory.
  const host = document.createElement('div'); host.className = 'hatch-scene';
  $('#garden-care-panel').after(host);
  await import('./hatch-scene.js').then(m => m.playHatchScene(host)).finally(() => host.remove());
}
const breeding = createBreedingMenu({ root: $('#breeding-panel'),
  playScene: host => import('./breeding-scene.js').then(m => m.playBreedingScene(host)), send: message => {
  if (socket?.readyState !== WebSocket.OPEN) return false;
  socket.send(JSON.stringify(message)); return true;
}, statRows: (stats, names) => {
  // The partner in the Doctor's own stat window.
  const list = document.createElement('div'); list.className = 'breeding-stats';
  for (const name of names) { const row = createStatRow(name); updateStatRow(row, name, stats[name] || {}); list.append(row); }
  return list;
} });
$('#breeding-open').addEventListener('click', () => {
  const open = $('#breeding-panel').hidden;
  $('#breeding-panel').hidden = !open; $('#breeding-open').setAttribute('aria-pressed', String(open));
});
function toggleCare(open = $('#garden-care-panel').hidden) {
  $('#garden-care-panel').hidden = !open;
  $('#care-open').setAttribute('aria-pressed', String(open));
}
$('#care-open').addEventListener('click', () => toggleCare());
const stageSelect = createStageSelect(stagesDialog, { onAction: action, onChange: updatePaused });
$('#stages-open').addEventListener('click', () => stageSelect.open());

function nearNPC(anchor) {
  return player?.zone === anchor.zone && Math.hypot(player.x - anchor.x, player.z - anchor.z) <= anchor.radius;
}

function send(message) {
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
}

function updateWatchButton() {
  const button = $('#watch-toggle');
  const available = !!player && Object.hasOwn(player, 'watch');
  const watching = available && !!player.watch;
  const eligible = watching || ['hub', 'garden', 'hero', 'dark'].includes(player?.zone);
  button.disabled = !available || !eligible || socket?.readyState !== WebSocket.OPEN || saving || !!watchPending;
  button.setAttribute('aria-pressed', String(watching));
  button.textContent = watchPending ? (watchPending.action === 'watch-stop' ? 'Taking control…' : 'Starting Watch…') : watching ? 'Watch on · Take control' : 'Watch off';
  button.title = watching ? 'Take keyboard control of your Chao' : eligible ? 'Let your Chao wander and rest in the shared garden' : 'Return to a garden before starting Watch';
  $('#garden').dataset.watchMode = watching ? 'watch' : 'manual';
}

function clearWatchPending() {
  clearTimeout(watchTimer); watchPending = null; updateWatchButton();
}

$('#watch-toggle').addEventListener('keydown', event => event.stopPropagation());
$('#watch-toggle').addEventListener('click', () => {
  if ($('#watch-toggle').disabled) return;
  const message = player.watch
    ? { type: 'action', action: 'watch-stop', sessionId: player.watch.id }
    : { type: 'action', action: 'watch-start' };
  watchPending = { action: message.action, playerId: player.id, previousId: player.watch?.id };
  garden?.clearInput();
  send(message); updateWatchButton();
  watchTimer = setTimeout(() => {
    clearWatchPending(); toast('The garden has not confirmed the Watch change. Please try again.', 'error');
  }, 5000);
});

function action(name) {
  if (!player) { toast('Enter the garden to start your adventure.'); return; }
  if (name.startsWith('train-') && nearNPC(INTERACTION_ANCHORS.trainer)) audio.speak('knuckles');
  if (name === 'race' && nearNPC({ zone: 'hub', x: 12, z: 6, radius: 2 })) audio.speak('tails');
  if (name === 'school') { openSchool(); return; }
  if (name === 'shop') { openShop(); return; }
  send({ type: 'action', action: name });
}

function toast(text, kind = 'info') {
  const stack = $('#toast-stack');
  const item = document.createElement('div');
  item.className = `toast ${['error', 'success'].includes(kind) ? kind : ''}`;
  item.textContent = text;
  stack.prepend(item);
  while (stack.children.length > 3) stack.lastElementChild.remove();
  setTimeout(() => { item.classList.add('out'); setTimeout(() => item.remove(), 350); }, 4500);
}

let garden;
try {
  garden = createGarden($('#garden'), {
    onInput: (input) => send({ type: 'input', ...input }),
    onAction: action,
  });
  garden.setPaused(true);
} catch (error) {
  console.error(error);
  toast('The 3D renderer could not start. Enable browser hardware acceleration and reload.', 'error');
}

function updatePaused() { garden?.setPaused(saving || $('#school-dialog').open || welcome.open || setupDialog.open || shopDialog.open || stagesDialog.open || !player); updateWatchButton(); }

function rememberChao(savedAt) {
  if (!player || !sessionToken) return;
  try { saveSlots.record(sessionToken, player, savedAt, sessionCard); }
  catch {
    if (!storageWarning) toast('This browser could not remember your Chao slot. Keep this session open.', 'error');
    storageWarning = true;
  }
  lastSlotUpdate = Date.now();
}

// The slot's memory card on the server: a look only, it never checks the save out.
async function showCard(line, token) {
  try {
    const response = await fetch('/api/game-save/status', { headers: { Authorization: `Bearer ${token}` }, cache: 'no-store' });
    if (!response.ok) return;
    const card = await response.json();
    line.textContent = card.inGame ? 'MEMORY CARD · IN THE GAME NOW'
      : card.savedAt ? `MEMORY CARD · GAME SAVE ${new Date(card.savedAt).toLocaleString()}`
      : 'MEMORY CARD · EMPTY UNTIL YOUR FIRST CHAO WORLD SAVE';
  } catch { /* No server reachable: the slot shows no card line. */ }
}

function showTitle() {
  const container = $('#save-slots');
  container.replaceChildren();
  let slots = [];
  try { slots = saveSlots.list(); } catch { /* Storage may be unavailable in private browser settings. */ }
  const shownCards = new Set();
  for (const slot of slots) {
    const button = document.createElement('button'); button.type = 'button'; button.className = 'save-slot';
    const icon = document.createElement('span'); icon.className = 'save-slot-icon';
    // Big: the slot's own Chao as the game's Chao Select draws it.
    const portrait = Object.assign(document.createElement('canvas'), { width: 240, height: 300 });
    portrait.style.cssText = 'width:120px;height:150px;display:block';
    icon.append(portrait);
    void import('./chao-portrait.js').then(m => m.mountChaoPortrait(portrait, slot)).catch(() => {});
    const meta = document.createElement('span'); meta.className = 'save-slot-meta';
    const name = document.createElement('strong'); name.textContent = slot.name || 'Saved Chao';
    const info = document.createElement('small');
    const form = slot.alignment <= -60 ? 'DARK' : slot.alignment >= 60 ? 'HERO' : 'NEUTRAL';
    info.textContent = slot.form ? `${form} ${slot.form === 'chaos' ? 'CHAOS CHAO' : 'CHAO'} · ${slot.rings ?? 0} RINGS${slot.admin ? ' · ADMIN' : ''}` : 'Resume your existing server save';
    if (slot.savedAt) { const saved = document.createElement('small'); saved.textContent = `Saved ${new Date(slot.savedAt).toLocaleString()}`; meta.append(name, info, saved); }
    else meta.append(name, info);
    const card = document.createElement('small'); card.className = 'save-slot-card'; meta.append(card);
    void showCard(card, slot.token);
    const arrow = document.createElement('span'); arrow.className = 'save-slot-arrow'; arrow.textContent = '↗';
    const remove = document.createElement('span'); remove.className = 'save-slot-delete'; remove.role = 'button'; remove.tabIndex = 0;
    remove.textContent = '✕'; remove.title = 'Delete this Chao';
    remove.addEventListener('click', event => {
      event.stopPropagation();
      if (!confirm(`Delete ${slot.name || 'this Chao'} forever? It is removed from the server and its memory card code can't bring it back.`)) return;
      void fetch('/api/chao', { method: 'DELETE', headers: { Authorization: `Bearer ${slot.token}` } }).then(async response => {
        // 401: the server no longer knows this token, so only the local slot is left to clear.
        if (!response.ok && response.status !== 401) { alert((await response.json().catch(() => ({}))).error || 'Could not delete this Chao.'); return; }
        saveSlots.remove(slot.token); showTitle();
      }, () => alert('Could not reach the server.'));
    });
    button.append(icon, meta, remove, arrow);
    button.addEventListener('click', () => void join(slot.name || 'Chao', slot.token, slot.card, false, slot.id));
    const cardCode = slot.card || slot.token; // Chao from before cards: the token is the card
    if (shownCards.has(cardCode)) { container.append(button); continue; }
    shownCards.add(cardCode);
    const code = document.createElement('details'); code.className = 'card-code';
    const summary = document.createElement('summary'); summary.textContent = 'MEMORY CARD CODE';
    const value = document.createElement('code'); value.textContent = cardCode.match(/.{4}/g).join(' ');
    const copy = document.createElement('button'); copy.type = 'button'; copy.textContent = 'Copy';
    copy.addEventListener('click', () => void navigator.clipboard?.writeText(cardCode).then(() => { copy.textContent = 'Copied'; }, () => {}));
    const warn = document.createElement('small'); warn.textContent = 'Keep this private. Anyone with this code owns every Chao on this card. Insert it on another browser to bring them all back.';
    code.append(summary, value, copy, warn);
    container.append(button, code);
  }
  if (!slots.length) {
    const empty = document.createElement('p'); empty.className = 'save-empty'; empty.textContent = 'A new story is waiting. Name your first Chao below.'; container.append(empty);
  }
  $('.new-chao').open = !slots.length;
  if (!welcome.open) welcome.showModal();
  updatePaused();
}

function panel(name) {
  currentPanel = name;
  $('#character-panel').hidden = name !== 'character';
  $('#journal-panel').hidden = name !== 'journal';
  $('#quest-card').hidden = name !== 'world';
  document.querySelectorAll('.nav-button').forEach((button) => button.classList.toggle('active', button.dataset.panel === name));
  updatePaused();
}

document.querySelectorAll('[data-panel]').forEach((button) => button.addEventListener('click', () => panel(currentPanel === button.dataset.panel ? 'world' : button.dataset.panel)));
document.querySelectorAll('[data-close-panel]').forEach((button) => button.addEventListener('click', () => panel('world')));
document.querySelectorAll('[data-action]').forEach((button) => button.addEventListener('click', () => action(button.dataset.action)));
window.addEventListener('keydown', (event) => {
  if (saving || welcome.open || setupDialog.open || /INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName)) return;
  if ($('#school-dialog').open) return;
  if (event.code === 'KeyB') { if (shopDialog.open) shopDialog.close(); else openShop(); return; }
  if (shopDialog.open || stagesDialog.open || $('#school-dialog').open) return;
  if (event.code === 'KeyC') panel(currentPanel === 'character' ? 'world' : 'character');
  if (event.code === 'KeyJ') panel(currentPanel === 'journal' ? 'world' : 'journal');
  if (event.code === 'KeyG') toggleCare();
  if (event.code === 'Escape') panel('world');
});

function connected(isConnected) {
  if (!isConnected) gardenCare.update(snapshot?.care, player, { connected: false });
  $('#connection-dot').className = `status-dot ${isConnected ? 'connected' : 'error'}`;
  $('#connection-text').textContent = isConnected ? 'Local garden · connected' : 'Reconnecting…';
  if (!isConnected && watchPending) clearWatchPending();
  else updateWatchButton();
}

const zones = {
  hub: { title: 'CHAO <em>LOBBY</em>', label: 'Main Hub', description: 'YOUR WORLD. YOUR ADVENTURE.' },
  garden: { title: 'NEUTRAL <em>GARDEN</em>', label: 'Neutral Garden', description: 'TRAIN. RACE. BATTLE. GROW.' },
  hero: { title: 'HERO <em>GARDEN</em>', label: 'Hero Garden', description: 'A HIGHER CALLING.' },
  dark: { title: 'DARK <em>GARDEN</em>', label: 'Dark Garden', description: 'UNLEASH YOUR DARK SIDE.' },
  mission: { title: 'MISSION <em>01</em>', label: 'The Outpost · Mission 01', description: 'CLEAR THE ROBOTS. CLAIM YOUR RINGS.' },
  race: { title: 'GARDEN <em>CIRCUIT</em>', label: 'Garden Circuit', description: 'FOUR CHECKPOINTS. ONE LITTLE LEGEND.' },
};

function goal(text, done) {
  const element = document.createElement('div');
  element.className = `goal ${done ? 'done' : ''}`;
  const marker = document.createElement('i');
  marker.textContent = done ? '✓' : '';
  element.append(marker, document.createTextNode(text));
  return element;
}

let statsCreated = false;
function buildStats() {
  for (const name of ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence']) {
    const row = createStatRow(name);
    (['luck', 'intelligence'].includes(name) ? $('#hidden-stats-list') : $('#stats-list')).append(row);
  }
  statsCreated = true;
}

function render(next) {
  snapshot = next;
  player = next.players.find((p) => p.id === next.selfId);
  if (!player) return;
  if (watchPending && (watchPending.playerId !== player.id || (Object.hasOwn(player, 'watch') &&
    (watchPending.action === 'watch-stop' ? player.watch === null : !!player.watch && player.watch.id !== watchPending.previousId)))) {
    const takingControl = watchPending.action === 'watch-stop';
    clearWatchPending();
    if (takingControl) $('#garden').focus({ preventScroll: true });
  }
  updateWatchButton();
  gardenCare.update(next.care, player, { connected: socket?.readyState === WebSocket.OPEN, serverTime: next.serverTime, nest: next.life?.nest || [] });
  breeding.update(next);
  if (Date.now() - lastSlotUpdate > 10000) rememberChao();
  $('#garden').dataset.playerX = player.x.toFixed(3);
  $('#garden').dataset.playerZ = player.z.toFixed(3);
  $('#garden').dataset.playerY = (player.y || 0).toFixed(3);
  $('#garden').dataset.zone = player.zone;
  const mounted = player.vehicle === 'dark-chao-walker';
  $('#garden').dataset.vehicle = player.vehicle || 'none';
  $('#mech-hud').hidden = !mounted;
  const boost = Math.max(0, Math.min(100, player.mech?.boostEnergy ?? 100));
  $('#boost-status').textContent = `${player.mech?.boosting ? 'JETPACK' : 'BOOST'} ${Math.round(boost)}%`;
  $('#garden').dataset.boostEnergy = String(Math.round(boost));
  $('#garden').dataset.boosting = String(!!player.mech?.boosting);
  if ($('#context-controls').dataset.mounted !== String(mounted)) {
    $('#context-controls').innerHTML = mounted ? '<kbd>SPACE</kbd> jetpack <kbd>F</kbd> fire' : '<kbd>SPACE</kbd> jump <kbd>F</kbd> attack';
    $('#context-controls').dataset.mounted = String(mounted);
    const attackButton = document.querySelector('button[data-action="attack"]');
    attackButton.querySelector('small').textContent = mounted ? 'Fire' : 'Fight';
    attackButton.title = mounted ? 'Fire the Dark Chao Walker cannon (F)' : 'Attack a nearby robot (F)';
  }
  $('#school-open').hidden=player.zone!=='hub';
  // A hub portal into a garden or the race opens the game's own Chao World area (sa2_boot::start_stage).
  if (lastZone === 'hub' && chaoWorldAreas[player.zone]) location.assign(`${SA2_PAGE}?stage=90&area=${chaoWorldAreas[player.zone]}`);
  if ($('#school-dialog').open) renderSchool();
  garden?.setSnapshot(next);
  garden?.setCare?.(next.care, player);
  audio.setZone(player.zone);
  const zone = zones[player.zone] || zones.garden;
  if (lastZone !== player.zone) {
    $('#zone-title').innerHTML = zone.title;
    $('#zone-label').textContent = zone.label;
    $('#zone-description').innerHTML = zone.description;
    lastZone = player.zone;
  }
  const online = next.players.filter((p) => p.zone === player.zone).length;
  $('#online-count').textContent = `${online} Chao`;
  const form = ({ normal: 'Neutral', hero: 'Hero', dark: 'Dark', chaos: 'Chaos' })[player.form] || 'Neutral';
  $('#hud-name').textContent = player.name;
  $('#hud-icon').textContent = player.admin ? '✦' : '◉'; // same icon as the Chao select
  $('#ring-count').textContent = player.rings.toLocaleString();
  $('#profile-name').textContent = player.name;
  $('#profile-form').textContent = `${player.form === 'chaos' && player.alignment <= -60 ? 'Dark Chaos' : form} · Happiness ${player.happiness} · Energy ${player.energy}`;
  $('#owner-badge').hidden = !player.admin;
  $('#owner-badge').textContent = 'ADMIN';
  $('#owner-dark-form').hidden = !player.admin;
  $('.owner-loadout').hidden = !player.admin;
  const walkerButton = $('#owner-walker');
  walkerButton.hidden = !player.admin;
  walkerButton.setAttribute('aria-pressed', String(player.vehicle === 'dark-chao-walker'));
  walkerButton.textContent = player.vehicle === 'dark-chao-walker' ? 'Leave Dark Chao Walker ↙' : 'Pilot Dark Chao Walker ↗';
  for (const [selector, slot, item, label] of [
    ['#owner-air-shoes', 'footwear', 'shadow-air-shoes', 'Shadow’s Air Shoes'],
    ['#owner-hammer-gloves', 'hands', 'knuckles-hammer-gloves', 'Knuckles’ Hammer Gloves'],
    ['#owner-metal-chao', 'look', 'metal-sonic-chao', 'Metal Sonic Chao look'],
  ]) {
    const button = $(selector);
    const equipped = player.gear?.[slot] === item;
    button.hidden = !player.admin;
    button.setAttribute('aria-pressed', String(equipped));
    button.textContent = `${equipped ? 'Remove' : 'Equip'} ${label} ${equipped ? '↙' : '↗'}`;
  }
  if (!statsCreated) buildStats();
  for (const [name, stat] of Object.entries(player.stats)) {
    const row = document.querySelector(`[data-stat="${name}"]`);
    if (!row) continue;
    updateStatRow(row, name, stat);
  }
  $('#alignment-value').textContent = `${player.alignment >= 60 ? 'Hero' : player.alignment <= -60 ? 'Dark' : 'Neutral'} · ${player.alignment > 0 ? '+' : ''}${player.alignment}`;
  $('#alignment-marker').style.left = `${(player.alignment + 100) / 2}%`;
  const goals = $('#quest-goals');
  const goalData = [
    [`Complete a mission (${Math.min(3, player.missionsCompleted)}/3)`, player.missionsCompleted >= 3],
    [`Finish your first race (${Math.min(1, player.racesCompleted)}/1)`, player.racesCompleted >= 1],
    ['Reach Run, Swim & Power Lv.10', ['run', 'swim', 'power'].every((s) => player.stats[s].level >= 10)],
  ];
  const goalKey = JSON.stringify(goalData);
  if (goals.dataset.key !== goalKey) { goals.replaceChildren(...goalData.map(([text, done]) => goal(text, done))); goals.dataset.key = goalKey; }
  const banner = $('#activity-banner');
  banner.hidden = !['mission', 'race'].includes(player.zone);
  if (player.zone === 'mission') banner.textContent = next.mission?.completed ? 'Outpost cleared · Rings awarded! Return home when ready.' : `THE OUTPOST · ${next.mission?.remaining ?? 5} robots remaining · F to fight`;
  if (player.zone === 'race' && next.race) {
    const elapsed = Math.max(0, (next.serverTime - next.race.startedAt) / 1000);
    banner.textContent = `GARDEN CIRCUIT · ${elapsed.toFixed(1)}s · Checkpoint ${Math.min(4, next.race.nextCheckpoint)}/4`;
  }
  if (shopDialog.open) renderShop();
}

function connect(token) {
  clearTimeout(reconnectTimer);
  if (socket) { socket.onclose = null; socket.close(); }
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  socket = new WebSocket(`${protocol}//${location.host}/ws?token=${encodeURIComponent(token)}`);
  socket.onopen = () => { connected(true); updatePaused(); };
  socket.onmessage = ({ data }) => {
    let message;
    try { message = JSON.parse(data); } catch { return; }
    if (message.type === 'snapshot') render(message);
    if (message.type === 'notice') toast(message.text, message.kind);
    if (message.type === 'notice' && message.kind === 'error' && watchPending) clearWatchPending();
    if (message.type === 'notice' && message.kind === 'error' && pendingSave) pendingSave.reject(new Error(message.text));
    if (message.type === 'event') {
      if (message.kind === 'watch-saved' && watchPending && message.accepted === false) clearWatchPending();
      if (message.kind === 'saved' && message.playerId === player?.id) {
        rememberChao(message.serverTime);
        pendingSave?.resolve();
      }
      if (message.kind === 'mech-shot') $('#garden').dataset.mechShots = String(Number($('#garden').dataset.mechShots || 0) + 1);
      gardenCare.handleEvent(message);
      breeding.handleEvent(message);
      garden?.handleEvent?.(message);
    }
  };
  socket.onclose = (event) => {
    connected(false);
    pendingSave?.reject(new Error('The connection closed before the save was confirmed.'));
    if (event.code === 1008) { toast('The connection was paused. Reload to rejoin.', 'error'); return; }
    // Another Chao from this memory card was selected; this one went back to the card.
    if (event.code === 4001) { toast(event.reason || 'Another Chao from this memory card is out now.', 'info'); showTitle(); return; }
    reconnectTimer = setTimeout(() => connect(token), 2000);
  };
  socket.onerror = () => connected(false);
}

async function join(name, token = sessionToken, card = null, owner = false, slotId = null) {
  if (joining) return;
  joining = true;
  $('#join-button').disabled = true;
  document.querySelectorAll('.save-slot').forEach(button => { button.disabled = true; });
  $('#join-error').textContent = '';
  try {
    const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, token, resumeOnly: !!token, ...(card && !token ? { card } : {}), ...(owner ? { ownerLogin: true } : {}) }) });
    let data = await response.json();
    // A saved slot whose key the server no longer knows (each owner login
    // issues a new one): on this PC the owner login gets a fresh key, and if
    // it is the same Chao the slot takes it (rememberChao) and play goes on.
    if (!response.ok && token && slotId && $('#owner-login')) {
      const fresh = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, ownerLogin: true }) });
      const next = await fresh.json().catch(() => ({}));
      if (fresh.ok && next.player?.id === slotId) data = next;
      else throw new Error(data.error || 'Could not join the garden.');
    } else if (!response.ok) throw new Error(data.error || 'Could not join the garden.');
    sessionToken = data.token;
    sessionCard = data.card || card;
    player = data.player;
    rememberChao();
    // The game's own Chao Lobby is the hub now (mob 2026-11-21); ?oldhub keeps this page as a reference.
    if (!new URLSearchParams(location.search).has('oldhub')) { window.location.assign(token || card || owner ? '/sa2/?stage=90&autostart' : '/sa2/?stage=90&hatch=1&autostart'); return; }
    welcome.close();
    connect(sessionToken);
    updatePaused();
  } catch (error) {
    if (!welcome.open) welcome.showModal();
    $('#join-error').textContent = `${error.message} The Chao server may be down. Try again in a moment.`;
  } finally { joining = false; $('#join-button').disabled = false; document.querySelectorAll('.save-slot').forEach(button => { button.disabled = false; }); }
}

$('#card-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const code = $('#card-code').value.replace(/\s+/g, '').toLowerCase();
  if (!/^[a-f0-9]{64}$/.test(code)) { $('#join-error').textContent = 'That memory card code isn\u2019t complete. It has 64 letters and numbers.'; return; }
  void insertCard(code);
});

// Inserting a card brings every Chao on it into this browser's slots.
async function insertCard(code) {
  try {
    const response = await fetch('/api/card', { headers: { Authorization: `Bearer ${code}` }, cache: 'no-store' });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Memory card was not found.');
    for (const chao of data.chao) saveSlots.record(chao.token, chao.player, undefined, code);
    $('#card-code').value = ''; $('#join-error').textContent = '';
    showTitle();
  } catch (error) { $('#join-error').textContent = error.message; }
}
$('#join-form').addEventListener('submit', (event) => {
  event.preventDefault();
  let newest; try { newest = saveSlots.list()[0]; } catch { /* no slots */ }
  void join($('#chao-name').value, null, newest && (newest.card || newest.token));
});
// Local owner login: only answered by a loopback server started with CHAO_OWNER_BOOTSTRAP.
$('#owner-login').addEventListener('click', () => void join('Chao', null, null, true));
welcome.addEventListener('cancel', (event) => event.preventDefault());
showTitle();
// Back from the game because its memory card couldn't be loaded (sa2 page).
{ const card = new URLSearchParams(location.search).get('card');
  if (card) $('#join-error').textContent = card === '409' ? 'Your Chao is still open in another game tab. Close it, then pick your Chao again.' : "Your Chao's save could not be loaded. Pick your Chao again to log back in."; }

async function saveAndLeave(destination) {
  if (saving || !player) return;
  if (socket?.readyState !== WebSocket.OPEN) { toast('Reconnect before saving and returning to the title.', 'error'); return; }
  saving = true;
  const button = destination ? $('#tiny-open') : $('#save-title');
  const originalLabel = button.textContent; button.disabled = true; button.textContent = 'Saving…';
  send({ type: 'input', dx: 0, dz: 0, jump: false, boost: false });
  updatePaused();
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('The server has not confirmed the save. Your garden stays open.')), 10000);
      pendingSave = { resolve: () => { clearTimeout(timer); resolve(); }, reject: error => { clearTimeout(timer); reject(error); } };
      send({ type: 'action', action: 'save' });
    });
    clearTimeout(reconnectTimer);
    socket.onclose = null; socket.onmessage = null; socket.close(); socket = null;
    player = null; snapshot = null; lastZone = undefined;
    connected(false); $('#connection-text').textContent = 'Title screen';
    for (const dialog of [shopDialog, stagesDialog, setupDialog]) if (dialog.open) dialog.close();
    if (destination) { location.assign(destination); return; }
    panel('world'); showTitle();
  } catch (error) { toast(error.message, 'error'); }
  finally { pendingSave = null; saving = false; button.disabled = false; button.textContent = originalLabel; updatePaused(); }
}
$('#save-title').addEventListener('click', () => void saveAndLeave());
$('#tiny-open').addEventListener('click', () => void saveAndLeave('/tiny.html'));

function renderSetup(data) {
  const container = $('#setup-content');
  container.replaceChildren();
  function item(title, text, small) {
    const box = document.createElement('div'); box.className = 'setup-item';
    const heading = document.createElement('strong'); heading.textContent = title;
    box.append(heading, document.createTextNode(text));
    if (small) { const sub = document.createElement('small'); sub.textContent = small; box.append(sub); }
    container.append(box);
  }
  item(data.found ? '✓ Sonic Adventure 2 found' : 'SA2 installation needed', data.found ? 'Local Steam PC resources are available.' : 'Install SA2 through Steam, or set SA2_GAME_PATH before starting.', data.gamePath);
  item('BATTLE DLC · required', data.battle?.evidence?.length ? `${data.battle.evidence.length} karate resource files found. Confirm BATTLE is enabled in Steam.` : 'Enable the BATTLE DLC in Steam. Resource discovery is not an ownership check.');
  if (data.inventory) {
    const grid = document.createElement('div'); grid.className = 'asset-grid';
    for (const [label, number] of [['Resources', data.inventory.total], ['Chao candidates', data.inventory.chao], ['Audio candidates', data.inventory.audio]]) {
      const count = document.createElement('div'); count.className = 'asset-count';
      const strong = document.createElement('strong'); strong.textContent = Number(number || 0).toLocaleString();
      const span = document.createElement('span'); span.textContent = label;
      count.append(strong, span); grid.append(count);
    }
    container.append(grid);
  }
  if (data.nativeData?.specialModels?.length) item('Special Chao model candidates', data.nativeData.specialModels.filter((m) => m.pointerPresent).map((m) => m.name).join(' · '));
  item('Local model conversion', data.imports?.chao ? 'Original Chao GLB is ready for the browser loader.' : 'The garden currently uses procedural proxy art. Native asset conversion is the next step.', data.imports?.garden ? 'Original Neutral garden geometry also converted; scene scale and collision validation pending.' : null);
}

async function loadStatus() {
  try {
    const response = await fetch('/api/status');
    if (!response.ok) throw new Error('Bridge unavailable');
    const data = await response.json();
    setup = data.setup;
    $('#install-status').textContent = setup.found ? 'SA2 files found' : 'Local SA2 installation not found';
    $('#install-status-dot').className = `status-dot ${setup.found ? 'connected' : 'error'}`;
    renderSetup(setup);
  } catch {
    $('.welcome-requirements').hidden = true; // only the local dev bridge reports install files
    $('#setup-content').textContent = 'The local bridge is unavailable. Start it with npm run dev.';
  }
}
void loadStatus();
$('#setup-open').addEventListener('click', () => { setupDialog.showModal(); updatePaused(); void loadStatus(); });
$('#setup-close').addEventListener('click', () => setupDialog.close());
setupDialog.addEventListener('close', updatePaused);

function renderSchool(){
  $('#school-report').textContent=player.name+' · Health '+player.hp+'/'+player.maxHp+' · Energy '+player.energy+' · Happiness '+player.happiness;
}
function openSchool(){
  if(!player || $('#school-dialog').open)return;
  renderSchool();$('#school-name').value=player.name;
  $('#school-dialog').showModal();updatePaused();
}
// The game's own Kindergarten (Chao World area 6) via the shared Chao World entry.
$('#school-open').addEventListener('click',()=>window.location.assign('/sa2/?stage=90&area=6'));
$('#school-close').addEventListener('click',()=>$('#school-dialog').close());
$('#school-dialog').addEventListener('close',updatePaused);
$('#school-name-form').addEventListener('submit',event=>{
  event.preventDefault();const name=$('#school-name').value.trim();
  if(name && player && name!==player.name)send({type:'action',action:'rename',name});
});
$('#school-market').addEventListener('click',()=>{$('#school-dialog').close();openShop();});
function openShop() {
  if(shopDialog.open)return;
  if (!player) { toast('Enter the garden first.'); return; }
  if (nearNPC(INTERACTION_ANCHORS.shop)) audio.speak('amy');
  renderShop();
  shopDialog.showModal();
  updatePaused();
}
let shopKey;
function renderShop() {
  $('#shop-balance').textContent = player.rings.toLocaleString();
  const catalog = snapshot?.shop?.catalog || [];
  const canShop = nearNPC(INTERACTION_ANCHORS.shop);
  const key = JSON.stringify([catalog, player.rings, player.equipped, player.ownedCosmetics, canShop]);
  if (key === shopKey) return;
  shopKey = key;
  $('.shop-note').textContent = canShop
    ? 'Earn rings in missions and races. Purchases and your equipped cap are saved with your Chao.'
    : 'Visit Amy Chao in the main hub to buy items. Close the shop and walk closer to her market stand.';
  const icons = { 'garden-fruit': '●', 'run-drive': '➟', 'power-drive': '✹', 'eggshell-cap': '◓' };
  const container = $('#shop-items');
  container.replaceChildren();
  if (!catalog.length) { container.textContent = 'The shop is loading. Rejoin after the server restarts.'; return; }
  for (const entry of catalog) {
    const item = document.createElement('article'); item.className = `shop-item shop-item-${entry.id}`;
    const icon = document.createElement('div'); icon.className = 'shop-item-icon'; icon.textContent = icons[entry.id] || '✦';
    const info = document.createElement('div'); info.className = 'shop-item-info';
    const name = document.createElement('h3'); name.textContent = entry.name;
    const description = document.createElement('p'); description.textContent = entry.description;
    info.append(name, description);
    const buy = document.createElement('button'); buy.className = 'buy-button';
    const owned = entry.id === 'eggshell-cap' && player.ownedCosmetics?.includes('eggshell');
    buy.textContent = owned ? 'EQUIPPED ✓' : `${entry.price} RINGS`;
    buy.disabled = owned || player.rings < entry.price || !canShop;
    buy.title = !canShop ? 'Visit Amy Chao in the main hub to shop.' : player.rings < entry.price ? 'Earn more rings in missions and races.' : entry.description;
    buy.addEventListener('click', () => send({ type: 'action', action: 'buy', item: entry.id }));
    item.append(icon, info, buy); container.append(item);
  }
}
$('#shop-open').addEventListener('click', openShop);
$('#shop-close').addEventListener('click', () => shopDialog.close());
shopDialog.addEventListener('close', updatePaused);
$('#owner-dark-form').addEventListener('click', () => action('admin-dark-chaos'));
$('#owner-walker').addEventListener('click', () => action(player?.vehicle === 'dark-chao-walker' ? 'dismount-vehicle' : 'mount-dark-walker'));
for (const [selector, slot, item] of [
  ['#owner-air-shoes', 'footwear', 'shadow-air-shoes'],
  ['#owner-hammer-gloves', 'hands', 'knuckles-hammer-gloves'],
  ['#owner-metal-chao', 'look', 'metal-sonic-chao'],
]) {
  $(selector).addEventListener('click', () => {
    if (!player) return;
    send(player.gear?.[slot] === item
      ? { type: 'action', action: 'unequip-owner-gear', slot }
      : { type: 'action', action: 'equip-owner-gear', item });
  });
}
new MutationObserver(() => {
  const real = $('#garden').dataset.chaoAsset === 'loaded';
  $('.build-label').textContent = real ? 'LOCAL PROTOTYPE • ORIGINAL SA2 CHAO ASSETS' : 'LOCAL PROTOTYPE • PROCEDURAL PREVIEW';
}).observe($('#garden'), { attributes: true, attributeFilter: ['data-chao-asset'] });

const direction = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] };
document.querySelectorAll('[data-direction]').forEach((button) => {
  button.addEventListener('pointerdown', (event) => { event.preventDefault(); button.setPointerCapture(event.pointerId); garden?.setTouchInput(...direction[button.dataset.direction]); });
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) button.addEventListener(type, () => garden?.setTouchInput(0, 0));
});
window.addEventListener('beforeunload', () => { clearTimeout(reconnectTimer); clearTimeout(watchTimer); socket?.close(); garden?.dispose(); audio.dispose(); });
if (import.meta.hot) import.meta.hot.dispose(() => { clearTimeout(reconnectTimer); clearTimeout(watchTimer); socket?.close(); garden?.dispose(); audio.dispose(); });
