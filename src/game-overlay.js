// Chao MMO glue: the one Inventory button over the game page (/sa2/), with
// breeding and Tiny Garden opened from inside it (mob 2026-11-21). It reuses the
// hub page's own modules and the game page's room socket.
import './style.css';
import './sonic.css';
import { createGardenCare } from './garden-care.js';
import { createBreedingMenu } from './breeding-menu.js';
import { createSaveSlots, SESSION_KEY } from './save-slots.js';

let mounted = null;
// One overlay per page; a reconnected room socket takes over sending.
export function mountOverlay(nextSocket) {
  if (mounted) { mounted.socket = nextSocket; return mounted.api; }
  mounted = { socket: nextSocket };
  const el = (tag, text, className) => { const n = document.createElement(tag); if (text) n.textContent = text; if (className) n.className = className; return n; };
  const send = message => mounted.socket.readyState === WebSocket.OPEN && (mounted.socket.send(JSON.stringify(message)), true);
  // The hub page's own top bar look (mob: "the top bar is good"), with only Inventory in it.
  const bar = el('header', undefined, 'topbar'); bar.style.cssText = 'position:fixed;z-index:7';
  bar.innerHTML = '<span class="wordmark"><span class="brand-symbol">\u25c9</span><span>CHAO<span class="wordmark-light">GARDEN</span><small>ADVENTURE \u00b7 RACE \u00b7 BATTLE</small></span></span><nav class="top-links"></nav>';
  const button = el('button', 'Inventory', 'nav-button mmo-overlay-button');
  bar.querySelector('nav').append(button);
  button.tabIndex = -1; // never takes the game's keyboard focus
  const panel = el('section', undefined, 'garden-care-panel'); panel.hidden = true; panel.style.cssText = 'position:fixed;top:110px';
  const care = el('div'), breedingRoot = el('div'); breedingRoot.hidden = true;
  const breedButton = el('button', 'Breeding'), tinyButton = el('button', 'Tiny Garden');
  for (const b of [breedButton, tinyButton]) b.tabIndex = -1;
  panel.append(care, el('div'), breedButton, tinyButton, breedingRoot);
  document.body.append(bar, panel);
  const slots = createSaveSlots(localStorage);
  const gardenCare = createGardenCare({ root: care, send, hatch: async (egg, name) => {
    const token = localStorage.getItem(SESSION_KEY), card = slots.list().find(s => s.token === token)?.card;
    if (!card) throw new Error('Insert your memory card first to hatch an egg.');
    const response = await fetch('/api/session', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, card, egg: { from: token, ...egg } }) });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'The egg could not hatch.');
    slots.record(data.token, data.player, undefined, card);
  } });
  const breeding = createBreedingMenu({ root: breedingRoot, send, playScene: host => import('./breeding-scene.js').then(m => m.playBreedingScene(host)) });
  button.addEventListener('click', () => { panel.hidden = !panel.hidden; document.getElementById('screen')?.focus(); });
  breedButton.addEventListener('click', () => { breedingRoot.hidden = !breedingRoot.hidden; });
  tinyButton.addEventListener('click', () => window.open('/tiny.html', '_blank'));
  mounted.api = {
    snapshot(next) {
      const player = next.players.find(p => p.id === next.selfId);
      gardenCare.update(next.care, player, { connected: true, serverTime: next.serverTime, nest: next.life?.nest || [] });
      breeding.update(next);
    },
    event(message) { gardenCare.handleEvent(message); breeding.handleEvent(message); },
  };
  return mounted.api;
}
