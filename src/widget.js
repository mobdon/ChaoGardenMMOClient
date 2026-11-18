import './widget.css';
import { createSaveSlots } from './save-slots.js';
import { chaoFormLabel, drawTinyAvatar, nativeDirection } from './tiny-avatar.js';

const $ = selector => document.querySelector(selector);
const canvas = $('#widget-avatar'), context = canvas.getContext('2d');
const mascot = new Image(); mascot.src = '/api/assets/login-chao.png';
let storage;
try { storage = localStorage; } catch { storage = { getItem: () => null, setItem: () => {} }; }
const saves = createSaveSlots(storage);
let slots = [], profile = null, token = null, socket = null, generation = 0;
let reconnectTimer, connectionState = 'offline', lastSyncedAt = null, lastMotionAt = -Infinity;
let previous = null, direction = 0, retryDelay = 2000;

function setText(selector, value) { const node = $(selector); if (node.textContent !== value) node.textContent = value; }
function sync(state, label, note) {
  connectionState = state;
  $('#studio-sync').dataset.state = state; $('#widget-live').dataset.state = state;
  setText('#studio-sync-label', label);
  setText('#widget-live>span', state === 'live' ? 'LIVE' : state === 'connecting' ? 'SYNCING' : profile ? 'LAST SEEN' : 'CONNECT');
  if (note) setText('#widget-connection-note', note);
  render();
}
function rememberSelection() {
  // Browser credential selection only. The preview sends no gameplay intents.
  if (token && profile) try { saves.record(token, profile); } catch { /* Canonical save stays on the server. */ }
}
function render() {
  setText('#widget-name', profile?.name || 'Your Chao');
  setText('#widget-form', profile ? chaoFormLabel(profile) : 'A LITTLE WORLD OF YOUR OWN');
  setText('#widget-rings', Number.isFinite(profile?.rings) ? profile.rings.toLocaleString() : '—');
  $('#garden-widget').dataset.admin = String(!!profile?.admin); $('#widget-owner').hidden = !profile?.admin;
  for (const key of ['energy', 'hunger', 'happiness']) {
    const value = profile?.[key], valid = Number.isFinite(value);
    $(`#widget-${key}`).hidden = !valid;
    if (valid && $(`#widget-${key}`).value !== value) $(`#widget-${key}`).value = value;
    setText(`#widget-${key}-value`, valid ? `${Math.round(value)}%` : '—');
  }
  const routine = profile?.watch ? ({ wander: 'Exploring the garden', rest: 'Taking a little rest', idle: 'Enjoying the garden' })[profile.watch.activity] || 'In the shared garden'
    : profile ? 'Ready for a little adventure' : 'Connect your saved Chao';
  setText('#widget-routine', connectionState === 'offline' && profile ? 'Last known garden state' : routine);
  // This legacy browser snapshot does not currently carry the durable AFK contract.
  // Missing status must never be presented as an authoritative "no job".
  const training = profile?.training;
  const phase = training?.status;
  setText('#widget-training-status', phase === 'none' ? 'No AFK job · open app to train'
    : ['training', 'ready', 'settled', 'canceled'].includes(phase) ? `${training.skill || 'Training'} · ${phase}`
      : 'AFK status unavailable');
  setText('#widget-sync-label', lastSyncedAt ? `${connectionState === 'live' ? 'SYNCED' : 'LAST SYNC'} ${new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(lastSyncedAt)}` : 'SAVE NOT CONNECTED');
  const label = profile ? `${profile.name || 'Chao'} · ${chaoFormLabel(profile)}` : 'Garden mascot preview; connect to show your saved Chao';
  canvas.setAttribute('aria-label', label);
  $('#garden-widget').setAttribute('aria-label', profile ? `Open ${profile.name || 'your Chao'} in Tiny Garden` : 'Connect your saved Chao in Tiny Garden');
}
function consume(next) {
  if (!next || typeof next.id !== 'string') return;
  if (previous?.id === next.id && Number.isFinite(next.x) && Number.isFinite(next.z) && (previous.x !== next.x || previous.z !== next.z)) {
    direction = nativeDirection(next.x - previous.x, next.z - previous.z, direction); lastMotionAt = performance.now();
  }
  previous = { id: next.id, x: next.x, z: next.z }; profile = next; lastSyncedAt = new Date(); retryDelay = 2000;
  sync('live', 'Your garden is live', 'Rings and needs come directly from your saved Chao.');
}
async function resume(activeToken) {
  const currentGeneration = ++generation;
  clearTimeout(reconnectTimer);
  if (socket) { socket.onclose = null; socket.close(); socket = null; }
  const switched = token !== activeToken;
  token = activeToken;
  if (switched) { profile = null; previous = null; lastSyncedAt = null; lastMotionAt = -Infinity; direction = 0; }
  sync('connecting', 'Connecting to your garden', 'Resuming an existing save. No new Chao will be created.');
  try {
    const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ token: activeToken, resumeOnly: true }) });
    const data = await response.json();
    if (currentGeneration !== generation) return;
    if (!response.ok || !data.player || typeof data.token !== 'string') throw new Error(data.error || 'This saved Chao is not available on the garden server.');
    profile = data.player; token = data.token; lastSyncedAt = new Date(); render();
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const current = new WebSocket(`${protocol}//${location.host}/ws?token=${encodeURIComponent(token)}`); socket = current;
    current.onopen = () => { if (generation !== currentGeneration) current.close(); };
    current.onmessage = event => {
      if (generation !== currentGeneration) return;
      let message; try { message = JSON.parse(event.data); } catch { return; }
      if (message.type === 'snapshot') consume(message.players?.find(player => player.id === message.selfId));
    };
    current.onerror = () => { if (generation === currentGeneration) sync('offline', 'Garden connection interrupted', 'Showing the last data received. Reconnecting…'); };
    current.onclose = event => {
      if (generation !== currentGeneration) return;
      lastMotionAt = -Infinity;
      sync('offline', 'Garden connection paused', event.code === 1008 ? 'Load this save at the garden title to reconnect.' : 'Showing the last data received. Reconnecting…');
      if (event.code !== 1008) { reconnectTimer = setTimeout(() => resume(activeToken), retryDelay); retryDelay = Math.min(12000, retryDelay * 1.5); }
    };
  } catch (error) {
    if (generation !== currentGeneration) return;
    sync('offline', 'Save unavailable', error.message || 'The garden server is unavailable. Open Tiny Garden to reconnect.');
  }
}

for (const button of document.querySelectorAll('[data-size]')) {
  if (button.tagName !== 'BUTTON') continue;
  button.addEventListener('click', () => {
    const size = button.dataset.size;
    for (const item of document.querySelectorAll('.size-selector button')) {
      item.classList.toggle('selected', item === button); item.setAttribute('aria-pressed', String(item === button));
    }
    $('#garden-widget').dataset.size = size; $('#home-widget-position').dataset.size = size;
    setText('#widget-size-caption', `TINY GARDEN · ${size.toUpperCase()}`);
  });
}
$('#widget-save').addEventListener('change', event => {
  const selected = slots[Number(event.target.value)]; if (selected) void resume(selected.token);
});
for (const link of document.querySelectorAll('a[href="/tiny.html"]')) link.addEventListener('click', rememberSelection);
try { slots = saves.list(); } catch { slots = []; }
if (slots.length) {
  const select = $('#widget-save'); select.replaceChildren(); select.disabled = false;
  slots.forEach((slot, index) => { const option = document.createElement('option'); option.value = String(index); option.textContent = slot.name || 'Saved Chao'; select.append(option); });
  void resume(slots[0].token);
} else sync('offline', 'Connect a saved Chao', 'Open Tiny Garden and load an existing save first. This preview never creates a Chao.');

function clock() {
  const now = new Date();
  setText('#phone-date', new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(now));
  setText('#phone-time', new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(now));
  $('#phone-time').dateTime = now.toISOString();
}
clock(); setInterval(clock, 30000);
function draw(time = 0) {
  if (!document.hidden) {
    context.clearRect(0, 0, canvas.width, canvas.height);
    if (!profile && mascot.complete && mascot.naturalWidth) context.drawImage(mascot, 36, 0, 288, 360);
    else drawTinyAvatar(context, profile, 180, 196, 292, { time: connectionState === 'live' ? time : 0, direction,
      walking: connectionState === 'live' && profile?.watch?.activity === 'wander' && time - lastMotionAt < 350 });
  }
  requestAnimationFrame(draw);
}
requestAnimationFrame(draw);
window.addEventListener('pagehide', () => { generation++; clearTimeout(reconnectTimer); socket?.close(); });
