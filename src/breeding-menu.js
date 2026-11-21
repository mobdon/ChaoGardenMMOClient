// The breeding menu: pick a Chao in your garden from a Chao Select window,
// see it in the Doctor's stat window, pick it again to invite it. The server
// runs the game's own breeding rules (fn_60_15214 invite, fn_60_150FC answer,
// fn_60_40130 egg); this module only shows them and sends the choice.
import { drawChaoList, listSize, loadInventoryArt, rowAt } from './native-inventory.js';

const STATS = ['swim', 'fly', 'run', 'power', 'stamina'];
const codes = {
  'offer-sent': name => `${name} got your call.`, 'egg-laid': () => 'An egg was laid!', 'garden-full': () => 'The garden is full (8 Chao).',
  'not-ready': () => 'Your Chao isn’t in the mood yet.', 'partner-busy': name => `${name} is busy.`, 'stage-conflict': () => 'Only grown-up Chao can breed.',
  'out-of-range': name => `${name} isn’t in this garden.`, 'partner-nest-full': name => `${name}’s nest is full.`,
  'activity-conflict': () => 'Finish what you’re doing first.', 'partner-unavailable': name => `${name} left.`,
};

/** The Doctor-style check the game's breeding rules make on a Chao (all from the server). */
export function checkup(p) {
  const life = p?.life || {};
  return [['Grown up', life.stage === 'adult'], ['Healthy', life.ill === false], ['In the mood', life.inTheMood === true], ['Free', life.busy === false]];
}

/** Who can be picked: grown-up Chao in this garden, callers first. Eggs in the nest follow. */
export function breedingView(snapshot, offers = new Map(), now = Date.now()) {
  const self = snapshot?.players?.find(p => p.id === snapshot.selfId);
  if (!self || !snapshot.life) return { valid: false, rows: [] };
  const calling = id => (offers.get(id)?.expiresAt ?? 0) > (snapshot.serverTime ?? now);
  const partners = snapshot.players
    .filter(p => p.id !== self.id && p.zone === self.zone && p.life?.stage === 'adult')
    .sort((a, b) => Number(calling(b.id)) - Number(calling(a.id)) || a.name.localeCompare(b.name))
    .map(p => ({ kind: 'partner', id: p.id, label: p.name, note: calling(p.id) ? `${p.name} is calling you` : p.name, stats: p.stats, check: checkup(p) }));
  const eggs = (snapshot.life.nest || []).map(egg => ({ kind: 'egg', id: egg.id, label: 'Egg',
    note: `Egg of ${egg.motherName || '?'} and ${egg.fatherName || '?'}` }));
  return { valid: true, revision: snapshot.life.revision, canBreed: !!snapshot.life.eligibility?.canBreed, self: checkup(self), rows: [...partners, ...eggs] };
}

export function createBreedingMenu({ root, send = () => false, requestId = () => globalThis.crypto.randomUUID(),
  hatch = async () => {}, statRows = null, playScene = null }) {
  const document = root.ownerDocument;
  const el = (tag, text, className) => { const n = document.createElement(tag); if (text !== undefined) n.textContent = text; if (className) n.className = className; return n; };
  let snapshot = null, selected = 0, pending = null, feedback = '', naming = null, previous = '';
  const offers = new Map();
  // The game's breeding scene plays here, small, kept across re-renders.
  const sceneHost = el('div', undefined, 'breeding-scene-host');
  const egg = () => { if (playScene && !sceneHost.children?.length) void playScene(sceneHost); };

  function render() {
    const view = breedingView(snapshot, offers);
    const signature = JSON.stringify([view, selected, pending?.requestId, feedback, naming]);
    if (signature === previous) return;
    previous = signature;
    root.replaceChildren(el('h2', 'Breeding'), sceneHost);
    if (!view.valid) { root.append(el('p', 'Join a garden first.', 'garden-care-note')); return; }
    if (!view.rows.length) { root.append(el('p', 'No grown-up Chao in this garden yet.', 'garden-care-note')); return; }
    selected = Math.min(selected, view.rows.length - 1);
    const rows = view.rows.map(row => ({ ...row }));
    if (naming) rows[selected].note = naming.name || 'Name?';
    const size = listSize(rows.length);
    const box = el('div', undefined, 'garden-care-carried');
    const canvas = el('canvas'); canvas.width = size.width * 2; canvas.height = size.height * 2; canvas.setAttribute('aria-hidden', 'true');
    Object.assign(canvas.style, { width: `${size.width}px`, height: `${size.height}px` });
    box.append(canvas);
    const draw = () => loadInventoryArt(document).then(art => {
      const ctx = art && canvas.getContext?.('2d'); if (!ctx) return;
      ctx.setTransform(2, 0, 0, 2, 0, 0); drawChaoList(ctx, art, rows, selected);
    });
    rows.forEach((row, n) => {
      const r = rowAt(n);
      const pick = el('button', undefined, 'garden-care-slot');
      Object.assign(pick.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.width}px`, height: `${r.height}px` });
      pick.setAttribute('aria-label', row.note);
      pick.setAttribute('aria-pressed', String(n === selected));
      pick.disabled = !!pending;
      pick.addEventListener('click', () => {
        if (n !== selected) { selected = n; naming = null; render(); return; }
        if (row.kind === 'egg') { naming = { eggId: row.id, name: '' }; render(); return; }
        if (!view.canBreed) { feedback = 'Only a grown-up Chao that isn’t busy can breed.'; render(); return; }
        pending = { type: 'life', requestId: requestId(), expectedRevision: view.revision, intent: { kind: 'breed', partnerId: row.id } };
        pending.partnerName = row.label;
        const { partnerName, ...message } = pending;
        if (send(message) === false) { pending = null; feedback = 'Not connected.'; }
        else feedback = '';
        render();
      });
      box.append(pick);
    });
    if (naming) {
      const input = el('input', undefined, 'garden-care-name');
      input.maxLength = 7; input.value = naming.name;
      input.setAttribute('aria-label', 'Name your new Chao');
      Object.assign(input.style, { top: `${size.window}px`, height: `${size.height - size.window}px` });
      input.addEventListener('input', () => { naming.name = input.value; rows[selected].note = input.value || 'Name?'; draw(); });
      input.addEventListener('keydown', event => {
        if (event.key === 'Escape') { naming = null; render(); }
        if (event.key !== 'Enter' || !naming.name.trim()) return;
        const { eggId, name } = naming; naming = null; feedback = 'Hatching…'; render();
        hatch({ eggId }, name.trim()).then(() => { feedback = `${name.trim()} hatched into a new slot on your memory card.`; },
          error => { feedback = error.message; }).finally(render);
      });
      box.append(input);
      globalThis.queueMicrotask?.(() => input.focus?.());
    }
    root.append(box);
    draw();
    const chosen = view.rows[selected];
    if (chosen?.kind === 'partner') {
      if (statRows) root.append(statRows(chosen.stats || {}, STATS));
      for (const [who, check] of [['Your Chao', view.self], [chosen.label, chosen.check]]) {
        const list = el('ul', undefined, 'breeding-check'); list.setAttribute('aria-label', `${who} check-up`);
        list.append(el('li', who, 'breeding-check-name'));
        for (const [label, ok] of check) list.append(el('li', `${ok ? '\u2714' : '\u2716'} ${label}`, ok ? 'breeding-ok' : 'breeding-not'));
        root.append(list);
      }
    }
    if (pending || feedback) { const note = el('p', pending ? 'Calling…' : feedback, 'garden-care-note'); note.setAttribute('role', 'status'); root.append(note); }
  }

  return {
    update(next) { snapshot = next; render(); },
    handleEvent(event) {
      if (event?.kind === 'life-breed-offer') { offers.set(event.fromId, { expiresAt: event.expiresAt }); render(); return true; }
      if (event?.kind === 'life-egg-laid') { feedback = `An egg was laid with ${event.partnerName}!`; egg(); render(); return true; }
      if (event?.kind !== 'life-result' || !pending || event.requestId !== pending.requestId) return false;
      const name = pending.partnerName; pending = null;
      feedback = (codes[event.code] || (() => event.accepted ? 'Done.' : `Not now (${event.code}).`))(name);
      if (event.code === 'egg-laid') egg();
      render(); return true;
    },
  };
}
