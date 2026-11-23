import './style.css';
import './sonic.css';
import './title.css';
import { allowSavedFiles, canPickFolders, pickGameFiles, savedHandles } from './game-files.js';
import { applyGameTextureStyles } from './game-assets.js';
import { createSaveSlots, SESSION_KEY } from './save-slots.js';

const $ = (selector) => document.querySelector(selector);
const welcome = $('#welcome-dialog');
const saveSlots = createSaveSlots(localStorage);
let player;
let sessionToken = localStorage.getItem(SESSION_KEY);
let loggedIn = null; // the server-confirmed card code after a login; the Chao select shows only then
let sessionCard = null; // the memory card code, when this browser has it
let setup;
let joining = false;
let storageWarning = false;

function toast(text, kind = 'info') {
  const stack = $('#toast-stack');
  const item = document.createElement('div');
  item.className = `toast ${['error', 'success'].includes(kind) ? kind : ''}`;
  item.textContent = text;
  stack.prepend(item);
  while (stack.children.length > 3) stack.lastElementChild.remove();
  setTimeout(() => { item.classList.add('out'); setTimeout(() => item.remove(), 350); }, 4500);
}

function rememberChao(savedAt) {
  if (!player || !sessionToken) return;
  try { saveSlots.record(sessionToken, player, savedAt, sessionCard); }
  catch {
    if (!storageWarning) toast('This browser could not remember your Chao slot. Keep this session open.', 'error');
    storageWarning = true;
  }
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

// The login screen's big Chao: the picked (or last) Chao, else the base Chao.
let portraitKey = null, disposePortrait = () => {};
function showPortrait(slot) {
  const key = `${slot.token || ''}`;
  if (key === portraitKey) return;
  portraitKey = key; disposePortrait();
  void import('./chao-portrait.js').then(m => m.mountChaoPortrait($('#title-portrait'), slot)).then(d => { disposePortrait = d; }).catch(() => {});
}
function showTitle() {
  const container = $('#save-slots');
  container.replaceChildren();
  let slots = [];
  try { slots = saveSlots.list(); } catch { /* Storage may be unavailable in private browser settings. */ }
  // Logged in: only the Chao the server just confirmed for this account's card.
  if (loggedIn) slots = slots.filter(slot => slot.card === loggedIn);
  showPortrait((loggedIn && slots[0]) || {}); // a default Chao until a login
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
    button.addEventListener('click', () => void join(slot.name || 'Chao', slot.token, slot.card));
    for (const pick of ['pointerenter', 'focus']) button.addEventListener(pick, () => showPortrait(slot));
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
  // Slots only show once this browser has logged in to a Chao.
  $('.save-select').hidden = !slots.length || !loggedIn;
  // Logged in: the Chao select replaces the login forms.
  for (const box of document.querySelectorAll('.auth-tabs, .title-screen .new-chao')) box.hidden = !!loggedIn && slots.length > 0;
  if (!welcome.open) welcome.showModal();
}

// beforeEnter runs after the Chao exists and before the page leaves for the game; false keeps the player here.
async function join(name, token = sessionToken, card = null, owner = false, beforeEnter = null) {
  if (joining) return;
  // The game needs the player's own files: the local server's, or ones picked on this page.
  if (!(await gameFilesReady())) { $('#join-error').textContent = 'Pick your game files first.'; $('#pick-steam').focus(); return; }
  void allowSavedFiles().then(() => applyGameTextureStyles()); // still inside the click: renews read access for the game page
  joining = true;
  $('#join-button').disabled = true;
  document.querySelectorAll('.save-slot').forEach(button => { button.disabled = true; });
  $('#join-error').textContent = '';
  try {
    const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, token, resumeOnly: !!token, ...(card && !token ? { card } : {}), ...(owner ? { ownerLogin: true } : {}) }) });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Could not join the garden.');
    sessionToken = data.token;
    sessionCard = data.card || card;
    player = data.player;
    rememberChao();
    if (beforeEnter && !(await beforeEnter())) { showTitle(); return; }
    // The game's own Chao Lobby is the hub.
    window.location.assign(token || card ? '/sa2/?stage=90' : '/sa2/?stage=90&hatch=1');
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
// Sign up: the new Chao starts a memory card, then the account is made for that card.
$('#join-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const account = { username: $('#signup-name').value.trim(), email: $('#signup-email').value.trim(), password: $('#signup-password').value };
  // The account is made before the page leaves for the game, or the request would be dropped.
  await join($('#chao-name').value, null, null, false, async () => {
    try {
      const response = await fetch('/api/account/register', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...account, card: sessionCard, agree: $('#signup-agree').checked }) });
      if (response.ok) return true;
      toast(`${(await response.json().catch(() => ({}))).error || 'Could not make your account.'} Your Chao is saved in this browser's slots.`, 'error');
    } catch { toast('Could not reach the server to make your account.', 'error'); }
    return false;
  });
  $('#signup-password').value = '';
});
// Log in / Sign up tabs: one form shows at a time.
document.querySelectorAll('.auth-tabs button').forEach(tab => tab.addEventListener('click', () => {
  document.querySelectorAll('.auth-tabs button').forEach(other => {
    other.setAttribute('aria-selected', String(other === tab));
    $(`#${other.dataset.auth}`).hidden = other !== tab;
  });
}));
// Log in: the server opens the account's memory card and its Chao fill the slots.
$('#login-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (rememberedSlot) {
    // The saved card code is checked with the server, which lists the Chao on it.
    try {
      const response = await fetch('/api/card', { headers: { Authorization: `Bearer ${rememberedSlot.card}` }, cache: 'no-store' });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error);
      for (const chao of data.chao) saveSlots.record(chao.token, chao.player, undefined, rememberedSlot.card);
      loggedIn = rememberedSlot.card; showTitle();
    } catch {
      rememberedSlot = null; $('#login-password').value = '';
      $('#join-error').textContent = 'Your saved login ran out. Type your password once more.';
    }
    return;
  }
  try {
    const response = await fetch('/api/account/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, cache: 'no-store',
      body: JSON.stringify({ login: $('#login-name').value.trim(), password: $('#login-password').value }) });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Could not log in.');
    $('#login-password').value = '';
    // Remember me keeps the Chao's session key (never the password) so the next visit logs straight in.
    const remember = $('#remember-me').checked;
    try { if (remember) localStorage.setItem(REMEMBER_KEY, $('#login-name').value.trim()); else localStorage.removeItem(REMEMBER_KEY); } catch { /* storage blocked */ }
    if (remember) for (const chao of data.chao) saveSlots.record(chao.token, chao.player, undefined, data.card);
    for (const chao of data.chao) if (!remember) saveSlots.record(chao.token, chao.player, undefined, data.card);
    loggedIn = data.card; showTitle();
  } catch (error) { $('#join-error').textContent = error.message; }
});
welcome.addEventListener('cancel', (event) => event.preventDefault());
const REMEMBER_KEY = 'chao-garden-remember-v1';
showTitle();
// Remembered on this device: the login form comes filled in, so Log in is one click.
// The password box only shows dots; the saved Chao session key logs in, never a stored password.
let rememberedSlot = null;
try {
  const name = localStorage.getItem(REMEMBER_KEY);
  rememberedSlot = name && saveSlots.list()[0];
  if (rememberedSlot) { $('#login-name').value = name; $('#login-password').value = '••••••••'; showPortrait(rememberedSlot); }
} catch { /* storage blocked: an empty login */ }
for (const field of ['#login-name', '#login-password']) $(field).addEventListener('input', () => {
  if (rememberedSlot) { rememberedSlot = null; $('#login-password').value = ''; }
});

// ---- the player's game files (src/game-files.js) ----
async function gameFilesReady() {
  if (setup?.found) return true;
  const saved = await savedHandles();
  return !!(saved.steam || saved.disc);
}
async function showGameFiles() {
  const saved = await savedHandles();
  const picked = saved.steam?.name || saved.disc?.name;
  $('#game-files').hidden = !!setup?.found; // the local server already has them
  $('#game-files').dataset.picked = String(!!picked);
  $('#game-files-state').textContent = picked ? `PICKED · ${picked.toUpperCase()}` : 'NOT PICKED';
  if (!canPickFolders) $('#game-files p').textContent = 'Picking your game files needs Chrome or Edge on a computer.';
  $('#pick-steam').disabled = $('#pick-disc').disabled = !canPickFolders;
}
for (const [button, kind] of [['#pick-steam', 'steam'], ['#pick-disc', 'disc']]) $(button).addEventListener('click', async () => {
  try { await pickGameFiles(kind); $('#join-error').textContent = ''; } catch (error) { if (error?.name !== 'AbortError') $('#join-error').textContent = 'Could not open those files.'; }
  void showGameFiles();
});
async function loadStatus() {
  try {
    const response = await fetch('/api/status');
    if (!response.ok) throw new Error('Bridge unavailable');
    const data = await response.json();
    setup = data.setup;
    $('#install-status').textContent = setup.found ? 'SA2 files found' : 'Local SA2 installation not found';
    $('#install-status-dot').className = `status-dot ${setup.found ? 'connected' : 'error'}`;
    void showGameFiles();
  } catch {
    void showGameFiles();
    $('.welcome-requirements').hidden = true; // only the local dev bridge reports install files
  }
}
void loadStatus();
