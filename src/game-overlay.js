// Chao MMO glue: the inventory on the Chao stats card over the game page (/sa2/), with
// breeding and Tiny Garden opened from inside it (mob 2026-11-21). It reuses the
// hub page's own modules and the game page's room socket.
import './style.css';
import './sonic.css';
import { createGardenCare } from './garden-care.js';
import { createBreedingMenu } from './breeding-menu.js';
import { createSaveSlots, SESSION_KEY } from './save-slots.js';
import { createStatRow, updateStatRow } from './stat-ui.js';
import { nativeText } from './native-text.js';

let mounted = null, lastNest = [];
// One overlay per page; a reconnected room socket takes over sending.
export function mountOverlay(nextSocket) {
  if (mounted) { mounted.socket = nextSocket; return mounted.api; }
  mounted = { socket: nextSocket };
  const el = (tag, text, className) => { const n = document.createElement(tag); if (text) n.textContent = text; if (className) n.className = className; return n; };
  const send = message => mounted.socket.readyState === WebSocket.OPEN && (mounted.socket.send(JSON.stringify(message)), true);
  // The inventory lives on the Chao stats card (mob): click the card or press Tab.
  const card = document.getElementById('mmo-card');
  const panel = el('section', undefined, 'garden-care-panel'); panel.hidden = true; panel.style.cssText = 'position:fixed;top:auto;right:auto;left:16px;bottom:72px';
  // The game's own stat window rows (stat-ui.js: native frame, font and grade cells), then the native inventory.
  const title = el('div'), stats = el('div', undefined, 'stats-list');
  const STATS = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];
  for (const name of STATS) stats.append(createStatRow(name));
  const care = el('div'), breedingRoot = el('div'); breedingRoot.hidden = true;
  const breedButton = el('button', 'Breeding');
  for (const b of [breedButton]) { b.tabIndex = -1; b.replaceChildren(nativeText(b.textContent, 'native-stat-name')); }
  // The hub's native stat-row styles are scoped to #character-panel (sonic.css); without it each row's frame spreads over the panel.
  const statBox = el('div'); statBox.id = 'character-panel'; statBox.append(stats);
  panel.append(title, statBox, care, el('div'), breedButton, breedingRoot);
  document.body.append(panel);
  const slots = createSaveSlots(localStorage);
  const gardenCare = createGardenCare({ root: care, send, hatch: async (egg, name) => {
    const token = localStorage.getItem(SESSION_KEY), card = slots.list().find(s => s.token === token)?.card;
    if (!card) throw new Error('Insert your memory card first to hatch an egg.');
    const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, card, egg: { from: token, ...egg } }) });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'The egg could not hatch.');
    slots.record(data.token, data.player, undefined, card);
    // The game lays it at the player's feet in the lobby, with the bred egg's own gene when it has one.
    const bred = lastNest.find(e => e.id === egg.eggId);
    window.sa2HatchEgg?.(bred ? { ...egg, gene: bred.gene, eggColor: bred.eggColor } : egg);
  } });
  const breeding = createBreedingMenu({ root: breedingRoot, send, playScene: host => import('./breeding-scene.js').then(m => m.playBreedingScene(host)) });
  const toggle = () => { panel.hidden = !panel.hidden; document.getElementById('screen')?.focus(); };
  if (card) { card.style.pointerEvents = 'auto'; card.style.cursor = 'pointer'; card.title = 'Inventory (Tab)'; card.addEventListener('click', toggle); }
  addEventListener('keydown', event => { if (event.key === 'Tab') { event.preventDefault(); toggle(); } }, true);
  breedButton.addEventListener('click', () => { breedingRoot.hidden = !breedingRoot.hidden; });
  mounted.api = {
    snapshot(next) {
      const player = next.players.find(p => p.id === next.selfId);
      if (player) {
        title.replaceChildren(nativeText(player.name, 'native-stat-name'));
        for (const name of STATS) { const row = stats.querySelector(`[data-stat="${name}"]`); if (row && player.stats?.[name]) updateStatRow(row, name, player.stats[name]); }
      }
      lastNest = next.life?.nest || [];
      gardenCare.update(next.care, player, { connected: true, serverTime: next.serverTime, nest: next.life?.nest || [] });
      breeding.update(next);
    },
    event(message) { gardenCare.handleEvent(message); breeding.handleEvent(message); },
  };
  return mounted.api;
}
