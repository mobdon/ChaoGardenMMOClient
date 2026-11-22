// Chao MMO glue: the inventory on the Chao stats card over the game page (/sa2/), with
// breeding and Tiny Garden opened from inside it (mob 2026-11-21). It reuses the
// hub page's own modules and the game page's room socket.
import './style.css';
import './sonic.css';
import { createGardenCare } from './garden-care.js';
import { createBreedingMenu } from './breeding-menu.js';
import { createSaveSlots, SESSION_KEY } from './save-slots.js';
import { gradeCells, grades } from './stat-ui.js';
import { mountChaoPortrait } from './chao-portrait.js';
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
  // The game's own medical-chart record page (page A, crop origin x264,y40): name bar, attributes,
  // ability type, portrait box and the five skill rows (y260..388 step 32), filled from the snapshot.
  const at = (node, x, y, w, h) => { node.style.cssText += `;position:absolute;left:${x}px;top:${y}px` + (w ? `;width:${w}px;height:${h}px` : ''); return node; };
  const record = el('div'); record.id = 'character-panel'; record.style.cssText = 'position:absolute;inset:0';
  const nameBox = at(el('div'), 0, 40, 360, 28), attr = at(el('div'), 40, 104), type = at(el('div'), 40, 156);
  nameBox.style.textAlign = 'center';
  const portrait = at(Object.assign(el('canvas'), { width: 208, height: 216 }), 232, 84, 104, 108);
  const ROWS = ['swim', 'fly', 'run', 'power', 'stamina'];
  const rows = ROWS.map((name, i) => {
    const y = 220 + i * 32;
    const label = at(nativeText(name[0].toUpperCase() + name.slice(1), 'native-stat-name'), 40, y);
    const bar = at(el('div'), 120, y + 8, 80, 8); bar.style.background = 'transparent';
    const fill = el('div'); fill.style.cssText = 'height:100%;background:#ffe37a'; bar.append(fill);
    const points = at(el('div'), 220, y), grade = at(el('span', undefined, 'native-grade'), 300, y, 22, 22);
    record.append(label, bar, points, grade);
    return { name, fill, points, grade };
  });
  record.append(nameBox, attr, type, portrait);
  let portraitFor = null;
  function updateRecord(player, life) {
    nameBox.replaceChildren(nativeText(player.name, 'native-stat-name'));
    attr.replaceChildren(nativeText(player.alignment >= 60 ? 'Hero' : player.alignment <= -60 ? 'Dark' : 'Neutral', 'native-stat-name'));
    type.replaceChildren(nativeText(life?.stage === 'adult' || player.admin ? 'Adult' : 'Child', 'native-stat-name'));
    for (const row of rows) {
      const stat = player.stats?.[row.name]; if (!stat) continue;
      row.fill.style.width = `${Math.min(99, stat.level) / 99 * 100}%`;
      row.points.replaceChildren(nativeText(String(Math.min(9999, stat.points)).padStart(4, '0'), 'native-stat-name'));
      const [x, y] = gradeCells[grades[stat.grade] || 'E'];
      row.grade.style.setProperty('--grade-x', `${-x}px`); row.grade.style.setProperty('--grade-y', `${-y}px`);
    }
    if (portraitFor !== player.id) { portraitFor = player.id; void mountChaoPortrait(portrait, player).catch(() => {}); }
  }
  const care = el('div'), breedingRoot = el('div'); breedingRoot.hidden = true;
  const breedButton = el('button', 'Breeding');
  for (const b of [breedButton]) { b.tabIndex = -1; b.replaceChildren(nativeText(b.textContent, 'native-stat-name')); }
  breedButton.hidden = true; breedingRoot.hidden = false;
  // The Kindergarten medical chart as the menu: its pages (360x440, cropped from the game's 640x480
  // layout at x264,y40) are the background, its L/R tabs (top 16 px) flip Stats / Inventory / Breeding.
  const body = el('div'); body.style.cssText = 'position:absolute;left:16px;right:16px;top:72px;bottom:16px;overflow:auto';
  const tab = (left, width, step) => { const t = el('button'); t.style.cssText = `position:absolute;top:0;left:${left}px;width:${width}px;height:16px;background:none;border:0;cursor:pointer`; t.tabIndex = -1; t.addEventListener('click', () => show(page + step)); return t; };
  const pages = [['a', [record]], ['b', [care]], ['c', [breedingRoot]]];
  let page = 0;
  function show(n) {
    page = (n + pages.length) % pages.length;
    const [art, nodes] = pages[page];
    panel.style.background = `url(/api/assets/medchart-page-${art}.png) no-repeat 0 0 / 360px 440px`;
    body.style.display = page ? '' : 'none';
    if (page) body.replaceChildren(...nodes);
    record.hidden = page !== 0;
  }
  panel.style.cssText += ';width:360px;height:440px;max-height:none;padding:0;overflow:visible;border-radius:0;box-shadow:none;image-rendering:pixelated';
  panel.append(record, tab(0, 96, -1), tab(192, 96, 1), body);
  show(0);
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
        updateRecord(player, next.life);
      }
      lastNest = next.life?.nest || [];
      gardenCare.update(next.care, player, { connected: true, serverTime: next.serverTime, nest: next.life?.nest || [] });
      breeding.update(next);
    },
    event(message) { gardenCare.handleEvent(message); breeding.handleEvent(message); },
  };
  return mounted.api;
}
