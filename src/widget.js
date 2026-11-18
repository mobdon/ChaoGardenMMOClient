import './widget.css';
import { createSaveSlots } from './save-slots.js';
import { chaoFormLabel, chaosVariant, drawTinyAvatar, nativeDirection } from './tiny-avatar.js';
import { WIDGET_SKILLS, widgetSnapshotView } from './widget-state.js';
import { acceptWidgetAccountSnapshot, acceptWidgetIdentity, authenticateWidgetSocket, readWidgetObservation, widgetSocketUrl } from './widget-observer.js';
import { drawWidgetEmeraldLayer, WIDGET_EMERALD_COLORS, widgetArtSubject, widgetCanvasPlacement, widgetEmeraldImagesReady, widgetEmeraldLayout } from './widget-art.js';

const $ = selector => document.querySelector(selector);
const canvas = $('#widget-avatar'), context = canvas.getContext('2d');
const bodyLayer = document.createElement('canvas'); bodyLayer.width = canvas.width; bodyLayer.height = canvas.height;
const bodyContext = bodyLayer.getContext('2d');
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
const emeraldImages = new Map(WIDGET_EMERALD_COLORS.map(color => {
  const image = new Image(); image.src = `/__widget-art/widget-emerald-${color}.png`; return [color, image];
}));
let storage;
try { storage = localStorage; } catch { storage = { getItem: () => null, setItem: () => {} }; }
const saves = createSaveSlots(storage);
let slots = [], profile = null, token = null, socket = null, generation = 0, requestAbort;
let reconnectTimer, connectionTimer, connectionState = 'offline', lastSyncedAt = null, lastMotionAt = -Infinity;
let previous = null, direction = 0, retryDelay = 2000;
let knownV2 = false, accountIdentity = null;
let previewVariant = 'dark';
const gardenWindow = $('.widget-garden-window');
function positionAvatar() {
  const art = widgetArtSubject(profile, previewVariant);
  const placement = widgetCanvasPlacement(gardenWindow.clientWidth, gardenWindow.clientHeight,
    chaosVariant(art.subject));
  for (const [key, value] of Object.entries({ left: placement.left, top: placement.top, width: placement.size, height: placement.size })) {
    const pixels = `${value}px`; if (canvas.style[key] !== pixels) canvas.style[key] = pixels;
  }
}
const gardenResize = new ResizeObserver(positionAvatar); gardenResize.observe(gardenWindow);
const statRows = WIDGET_SKILLS.map(([key, label]) => {
  const row = document.createElement('div');
  row.setAttribute('role', 'group');
  const name = document.createElement('span'); name.textContent = label;
  const value = document.createElement('b'); value.textContent = '—';
  row.append(name, value); $('#widget-stats').append(row);
  return { key, row, value };
});
function setText(selector, value) { const node = $(selector); if (node.textContent !== value) node.textContent = value; }
function sync(state, label, note) {
  connectionState = state;
  $('#studio-sync').dataset.state = state; $('#widget-live').dataset.state = state;
  setText('#studio-sync-label', label);
  setText('#widget-live>span', state === 'live' ? 'LIVE' : state === 'connecting' ? 'SYNCING' : profile ? 'LAST READ' : 'ART PREVIEW');
  $('#widget-retry').disabled = !slots.length || state === 'connecting';
  if (note) setText('#widget-connection-note', note);
  render();
}
function rememberSelection() {
  // Browser credential selection only. The preview sends no gameplay intents.
  if (token && profile) try { saves.record(token, profile); } catch { /* Canonical save stays on the server. */ }
}
function render() {
  const view = widgetSnapshotView(profile);
  const art = widgetArtSubject(profile, previewVariant);
  setText('#widget-name', profile ? profile.name || 'Your Chao' : chaoFormLabel(art.subject).replace(/ Chao$/, ''));
  setText('#widget-form', profile ? chaoFormLabel(profile) : 'ART PREVIEW · ORIGINAL SA2');
  $('#widget-art-selector').hidden = !art.preview; $('#widget-art-variant').disabled = !art.preview;
  $('#widget-subject-label').hidden = !art.preview;
  setText('#widget-rings', view.rings === null ? '—' : view.rings.toLocaleString());
  $('#garden-widget').dataset.admin = String(profile?.admin === true); $('#widget-owner').hidden = profile?.admin !== true;
  for (const key of ['energy', 'hunger', 'happiness']) {
    const value = view.needs[key];
    $(`#widget-${key}`).hidden = value === null;
    if (value !== null) $(`#widget-${key}`).value = value;
    setText(`#widget-${key}-value`, value === null ? '—' : `${Math.round(value)}%`);
  }
  for (const { key, row, value } of statRows) {
    const stat = view.stats.find(item => item.key === key);
    value.textContent = stat.level === null ? '—' : stat.level.toLocaleString();
    const fullName = key[0].toUpperCase() + key.slice(1);
    row.title = row.ariaLabel = `${fullName}: level ${stat.level ?? 'not reported'}, points ${stat.points ?? 'not reported'}, grade ${stat.grade ?? 'not reported'}`;
  }
  setText('#widget-routine', !profile ? 'Connect your save for live data' : connectionState !== 'live' ? `Last read · ${view.routine}` : view.routine);
  setText('#widget-training-status', view.training);
  setText('#widget-sync-label', lastSyncedAt
    ? `${connectionState === 'live' ? 'READ' : 'LAST READ'} ${new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(lastSyncedAt)}` : 'NOT CONNECTED');
  canvas.hidden = false;
  canvas.setAttribute('aria-label', profile ? `${profile.name || 'Chao'} · ${chaoFormLabel(profile)}`
    : `${chaoFormLabel(art.subject)} artwork preview; not your saved Chao`);
  $('#garden-widget').setAttribute('aria-label', profile ? `Open ${profile.name || 'your Chao'} in Tiny Garden` : 'Connect your saved Chao in Tiny Garden');
  positionAvatar();
}
function consume(next) {
  if (!next || typeof next.id !== 'string' || next.id !== profile?.id) return;
  if (previous?.id === next.id && Number.isFinite(next.x) && Number.isFinite(next.z) && (previous.x !== next.x || previous.z !== next.z)) {
    direction = nativeDirection(next.x - previous.x, next.z - previous.z, direction); lastMotionAt = performance.now();
  }
  previous = { id: next.id, x: next.x, z: next.z }; profile = next; lastSyncedAt = new Date(); retryDelay = 2000;
  armSnapshotDeadline(socket, generation);
  sync('live', 'Your garden is live', 'Needs, levels and rings are read from your canonical Rust server profile.');
}
function armSnapshotDeadline(current, currentGeneration) {
  clearTimeout(connectionTimer);
  connectionTimer = setTimeout(() => {
    if (generation !== currentGeneration || socket !== current) return;
    current.close();
    sync('offline', 'Live updates paused', 'Showing the last confirmed read. Reconnecting…');
    scheduleReconnect(token, currentGeneration);
  }, 10000);
}
function scheduleReconnect(activeToken, currentGeneration) {
  clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => { if (generation === currentGeneration) void resume(activeToken); }, retryDelay);
  retryDelay = Math.min(12000, retryDelay * 1.5);
}
async function resume(activeToken) {
  const currentGeneration = ++generation;
  clearTimeout(reconnectTimer); clearTimeout(connectionTimer); requestAbort?.abort();
  requestAbort = new AbortController();
  if (socket) { socket.onclose = null; socket.close(); socket = null; }
  const switched = token !== activeToken;
  token = activeToken;
  if (switched) { profile = null; previous = null; accountIdentity = null; lastSyncedAt = null; lastMotionAt = -Infinity; direction = 0; }
  sync('connecting', 'Connecting to your garden', 'Resuming your existing save. This preview never creates a Chao.');
  try {
    const data = await readWidgetObservation(fetch, activeToken, { signal: requestAbort.signal, knownV2, identity: accountIdentity,
      onProtocol(version) { if (generation === currentGeneration && version === 2) knownV2 = true; } });
    if (currentGeneration !== generation) return;
    profile = data.player; token = data.token; accountIdentity = data.identity; lastSyncedAt = new Date(); render();
    const current = new WebSocket(widgetSocketUrl(location, data.version, token)); socket = current;
    armSnapshotDeadline(current, currentGeneration);
    current.onopen = () => {
      if (generation !== currentGeneration) current.close();
      else authenticateWidgetSocket(current, data.version, token);
    };
    current.onmessage = event => {
      if (generation !== currentGeneration) return;
      let message; try { message = JSON.parse(event.data); } catch { return; }
      try {
        if (data.version === 2) {
          if (message.type === 'error') throw new Error('The account service paused observation. Reconnect to confirm current access.');
          if (message.type === 'snapshot') {
            const observation = acceptWidgetAccountSnapshot(message, accountIdentity);
            if (observation) { accountIdentity = observation.identity; consume(observation.player); }
          } else if (message.identity) accountIdentity = acceptWidgetIdentity(message.identity, accountIdentity);
        } else if (message.type === 'snapshot' && Array.isArray(message.players)) consume(message.players.find(player => player.id === message.selfId));
      } catch (error) {
        current.onclose = null; current.close(); clearTimeout(connectionTimer);
        sync('offline', 'Account observation paused', error.message);
      }
    };
    current.onerror = () => { if (generation === currentGeneration) sync('offline', 'Garden connection interrupted', 'Showing the last confirmed read. Reconnecting…'); };
    current.onclose = event => {
      if (generation !== currentGeneration) return;
      clearTimeout(connectionTimer); lastMotionAt = -Infinity;
      sync('offline', 'Garden connection paused', event.code === 1008 ? 'Load this save at the garden title to reconnect.' : 'Showing the last confirmed read. Reconnecting…');
      if (event.code !== 1008) scheduleReconnect(token, currentGeneration);
    };
  } catch (error) {
    if (generation !== currentGeneration) return;
    sync('offline', 'Save not connected', error.message || 'The garden server is unavailable. Retry to reconnect.');
    if (error.retry !== false) scheduleReconnect(activeToken, currentGeneration);
  }
}
for (const button of document.querySelectorAll('.size-selector button')) {
  button.addEventListener('click', () => {
    const size = button.dataset.size;
    for (const item of document.querySelectorAll('.size-selector button')) {
      item.classList.toggle('selected', item === button); item.setAttribute('aria-pressed', String(item === button));
    }
    $('#garden-widget').dataset.size = size; $('#home-widget-position').dataset.size = size;
    setText('#widget-size-caption', `TINY GARDEN · ${size.toUpperCase()}`);
    positionAvatar();
  });
}
$('#widget-save').addEventListener('change', event => {
  const selected = slots[Number(event.target.value)]; if (selected) void resume(selected.token);
});
$('#widget-retry').addEventListener('click', () => {
  const selected = slots[Number($('#widget-save').value)]; if (selected) void resume(selected.token);
});
$('#widget-art-variant').addEventListener('change', event => {
  if (profile) return;
  previewVariant = event.target.value; render();
});
for (const link of document.querySelectorAll('a[href="/tiny.html"]')) link.addEventListener('click', rememberSelection);
const nativeArt = $('#widget-garden-art');
function updateArtState() {
  const missing = nativeArt.complete && !nativeArt.naturalWidth;
  nativeArt.hidden = missing; $('#widget-art-missing').hidden = !missing;
}
nativeArt.addEventListener('error', updateArtState); nativeArt.addEventListener('load', updateArtState); updateArtState();
try { slots = saves.list(); } catch { slots = []; }
if (slots.length) {
  const select = $('#widget-save'); select.replaceChildren(); select.disabled = false;
  slots.forEach((slot, index) => { const option = document.createElement('option'); option.value = String(index); option.textContent = slot.name || 'Saved Chao'; select.append(option); });
  void resume(slots[0].token);
} else sync('offline', 'Connect a saved Chao', 'Load your existing save at the garden title in this browser, then return here. The original garden art is available without a save.');
function clock() {
  const now = new Date();
  setText('#phone-date', new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' }).format(now));
  setText('#phone-time', new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(now));
  $('#phone-time').dateTime = now.toISOString();
}
clock(); const clockTimer = setInterval(clock, 30000);
function draw(time = 0) {
  if (!document.hidden) {
    context.clearRect(0, 0, canvas.width, canvas.height);
    const art = widgetArtSubject(profile, previewVariant);
    const artTime = reducedMotion.matches ? 0 : (art.preview || connectionState === 'live' ? time : 0);
    const owner = !art.preview && profile?.admin === true;
    bodyContext.clearRect(0, 0, bodyLayer.width, bodyLayer.height);
    // The owner portrait keeps its native front anchor so the fixed chest gem
    // stays on the body. Preview selection never replaces canonical identity.
    const drawn = drawTinyAvatar(bodyContext, art.subject, 180, 196, 292, { time: artTime,
      direction: owner || art.preview ? 0 : direction,
      walking: !art.preview && connectionState === 'live' && profile.watch?.activity === 'wander' && time - lastMotionAt < 350 });
    const ornamentsReady = owner && widgetEmeraldImagesReady(emeraldImages);
    const ornaments = drawn && ornamentsReady ? widgetEmeraldLayout(profile, { canonical: true, x: 180, y: 196, size: 292, time: artTime }) : [];
    drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'far');
    context.drawImage(bodyLayer, 0, 0);
    drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'chest');
    drawWidgetEmeraldLayer(context, emeraldImages, ornaments, 'near');
    setText('#widget-avatar-note', !drawn ? 'Original art for this form is unavailable' : owner && !ornamentsReady ? 'Owner ornaments are unavailable' : '');
    $('#widget-avatar-note').hidden = drawn && (!owner || ornamentsReady);
  }
  requestAnimationFrame(draw);
}
requestAnimationFrame(draw);
window.addEventListener('pagehide', () => {
  generation++; clearTimeout(reconnectTimer); clearTimeout(connectionTimer); clearInterval(clockTimer); requestAbort?.abort(); socket?.close(); gardenResize.disconnect();
});
// Back from the title/Tiny Garden can restore a page with old storage and a
// closed socket. Reload the preview so it discovers the existing saved slot.
window.addEventListener('pageshow', event => { if (event.persisted) location.reload(); });
