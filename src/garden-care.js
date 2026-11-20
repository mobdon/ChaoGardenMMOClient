// Desktop garden care: renders the server's canonical care/garden snapshot and
// submits bounded care intents. The server owns inventory, growth, fruit,
// proximity, effects and animal parts; this module never computes them.
export const CARE_WIRE = 'adapted-care-v1';
export const PLACE_RANGE = 2;

const FRUIT_RE = /^sa2-fruit:(?:[0-9]|1[0-9]|2[0-3])$/;
const NATIVE_RE = /^sa2:(?:[0-9]|1[0-9]|2[0-4])$/;
const SEED_RE = /^seed:(strong|tasty|hero|dark|round|triangle|square)$/;
export const PART_SOCKETS = {
  arms: ['leftArm', 'rightArm'], ears: ['leftEar', 'rightEar'], forehead: ['forehead'], horns: ['leftHorn', 'rightHorn'],
  legs: ['leftLeg', 'rightLeg'], tail: ['tail'], wings: ['leftWing', 'rightWing'], tongue: ['tongue'],
};

export function careItemKind(id) {
  if (id === 'garden-fruit') return 'food';
  if (FRUIT_RE.test(id)) return 'fruit';
  if (SEED_RE.test(id)) return 'seed';
  if (NATIVE_RE.test(id)) return Number(id.slice(4)) < 21 ? 'animal' : 'drive';
  return null;
}

const title = text => text.replaceAll('-', ' ').replace(/\b\w/g, letter => letter.toUpperCase());

export function careItemLabel(id, catalog) {
  const kind = careItemKind(id);
  if (kind === 'food') return 'Garden Fruit';
  if (kind === 'seed') return `${title(id.slice(5))} Seed`;
  const rows = kind === 'fruit' ? catalog?.fruits : catalog?.items;
  const number = Number(id.slice(id.indexOf(':') + 1));
  const row = Array.isArray(rows) ? rows.find(entry => entry?.id === number) : null;
  if (typeof row?.name === 'string' && row.name.length && row.name.length <= 80) return row.name;
  return kind === 'fruit' ? `Native fruit ${number}` : `Native item ${number}`;
}

function finite(...values) { return values.every(Number.isFinite); }

// Places (plots and groves) the server reports, each with its distance from the Chao.
export function gardenPlaces(care, player) {
  const garden = care?.garden;
  if (!garden || !Array.isArray(garden.plots) || !Array.isArray(garden.groves)) return [];
  const places = [];
  for (const plot of garden.plots) {
    if (typeof plot?.id !== 'string' || !finite(plot.x, plot.z)) continue;
    places.push({ kind: 'plot', id: plot.id, zone: plot.zone, x: plot.x, z: plot.z, tree: plot.tree && typeof plot.tree === 'object' ? plot.tree : null });
  }
  for (const grove of garden.groves) {
    if (typeof grove?.id !== 'string' || !finite(grove.x, grove.z)) continue;
    // Permanent native garden trees; the server seeds them on first connect.
    const tree = grove.tree && typeof grove.tree === 'object' ? grove.tree : { stage: 'mature', fruitReady: 0 };
    places.push({ kind: 'grove', id: grove.id, zone: grove.zone, x: grove.x, z: grove.z, tree: { ...tree, fruitId: tree.fruitId || grove.fruitId } });
  }
  return places.map(place => ({ ...place,
    distance: player && player.zone === place.zone && finite(player.x, player.z) ? Math.hypot(player.x - place.x, player.z - place.z) : Infinity }));
}

// Pure view of what the player may do; the server rechecks every rule.
export function gardenCareView(care, player, { connected = false, serverTime } = {}) {
  const valid = care?.rulesVersion === CARE_WIRE && Number.isSafeInteger(care.revision) && care.revision >= 0;
  const inventory = valid && care.inventory && typeof care.inventory === 'object' ? care.inventory : {};
  const readyAt = care?.eligibility?.effectReadyAt;
  const cooldownReady = readyAt == null || (Number.isFinite(readyAt) && Number.isFinite(serverTime) && serverTime >= readyAt);
  const mayCare = !!(valid && connected && care.eligibility?.canCare === true);
  const items = Object.entries(inventory)
    .filter(([id, quantity]) => careItemKind(id) && Number.isInteger(quantity) && quantity > 0 && quantity <= 99)
    .map(([id, quantity]) => {
      const kind = careItemKind(id);
      const enabled = kind === 'food' || (kind === 'fruit' ? care.catalog?.fruitsEnabled === true : kind === 'seed' ? care.catalog?.fruitsEnabled === true : care.catalog?.enabled === true);
      return { id, kind, quantity, label: careItemLabel(id, care.catalog), mayUse: mayCare && cooldownReady && enabled && kind !== 'seed' };
    })
    .sort((a, b) => a.kind.localeCompare(b.kind) || a.id.localeCompare(b.id, 'en', { numeric: true }));
  const seeds = items.filter(item => item.kind === 'seed');
  const places = gardenPlaces(valid ? care : null, player).filter(place => place.distance <= PLACE_RANGE)
    .sort((a, b) => a.distance - b.distance)
    .map(place => {
      const stage = place.tree?.stage || 'empty';
      const fruit = Number.isInteger(place.tree?.fruitReady) ? place.tree.fruitReady : 0;
      // Native rules (server CARE.md): a ripe fruit drops after several shakes;
      // watering fills a seed, a young tree or an adult (which also ripens fruit).
      const phase = typeof place.tree?.phase === 'string' ? place.tree.phase : stage === 'mature' ? 'adult' : stage;
      const actions = [];
      if (fruit > 0) actions.push({ kind: 'harvest', label: `Shake for ${careItemLabel(place.tree.fruitId, care.catalog)} (${fruit} ripe)` });
      if (place.tree && ['seed', 'young', 'adult'].includes(phase)) actions.push({ kind: 'water', label: 'Water' });
      if (place.kind === 'plot' && !place.tree) {
        for (const seed of seeds) actions.push({ kind: 'plant', itemId: seed.id, label: `Plant ${seed.label}` });
      }
      return { ...place, stage, fruit, actions: mayCare ? actions : [] };
    });
  // The server's hub shop point (CARE.md purchase rule); the server rechecks it.
  const atShop = player?.zone === 'hub' && finite(player.x, player.z) && Math.hypot(player.x + 4, player.z - 8) <= PLACE_RANGE;
  const offers = (valid && Array.isArray(care.shop) ? care.shop : [])
    .filter(offer => SEED_RE.test(offer?.itemId) && Number.isInteger(offer.price) && offer.price > 0)
    .map(offer => ({ itemId: offer.itemId, price: offer.price, label: careItemLabel(offer.itemId),
      mayBuy: mayCare && atShop && offer.enabled === true && Number.isFinite(player?.rings) && player.rings >= offer.price }));
  const parts = valid && care.animalParts?.shown && typeof care.animalParts.shown === 'object' ? care.animalParts.shown : {};
  return { valid, mayCare, cooldownReady, revision: valid ? care.revision : null, items, places, atShop, offers,
    parts: Object.entries(parts).filter(([category, id]) => PART_SOCKETS[category] && Number.isInteger(id) && id >= 0 && id <= 20)
      .map(([category, id]) => ({ category, species: id, label: careItemLabel(`sa2:${id}`, care.catalog) })) };
}

export function careIntent(place, action) {
  if (action.kind === 'plant') return { kind: 'plant', itemId: action.itemId, plotId: place.id };
  return { kind: action.kind, plotId: place.id };
}

export function matchingResult(request, event) {
  if (!request || event?.type !== 'event' || event.kind !== 'care-result' || event.rulesVersion !== CARE_WIRE
    || event.requestId !== request.requestId || typeof event.accepted !== 'boolean' || !Number.isSafeInteger(event.revision)) return null;
  return { accepted: event.accepted, replayed: event.replayed === true, revision: event.revision, code: typeof event.code === 'string' ? event.code : 'unavailable' };
}

const reasons = {
  'out-of-range': 'Walk closer first.', 'no-fruit': 'No fruit is ripe yet.', 'plot-occupied': 'Something already grows there.',
  'not-growing': 'This tree cannot be watered right now.', cooldown: 'Give your Chao a moment.', 'item-not-owned': 'You no longer have that item.',
  'inventory-full': 'You are carrying the most you can of that.', 'native-data-unavailable': 'Native item data is not loaded on this server.',
  'activity-conflict': 'Finish the current activity first.', 'insufficient-rings': 'You need more rings.', 'stale-revision': 'Your garden changed. Try again.',
};

export function createGardenCare({ root, send = () => false, requestId = () => globalThis.crypto.randomUUID() }) {
  if (!root) throw new TypeError('Garden care needs a panel root.');
  const document = root.ownerDocument;
  let care = null, player = null, connected = false, serverTime, pending = null, feedback = '', previous = '';
  const el = (tag, text, className) => {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  };
  function submit(intent) {
    const view = gardenCareView(care, player, { connected, serverTime });
    if (pending || !view.mayCare) return;
    const request = { type: 'care', requestId: requestId(), expectedRevision: view.revision, intent };
    if (send(request) === false) return;
    pending = request; feedback = ''; render();
  }
  function render() {
    const view = gardenCareView(care, player, { connected, serverTime });
    const signature = JSON.stringify([view, pending?.requestId, feedback]);
    if (signature === previous) return;
    previous = signature;
    root.replaceChildren(el('h2', 'Garden care'));
    if (!view.valid) { root.append(el('p', 'This server does not report garden care yet.', 'garden-care-note')); return; }
    root.append(el('h3', 'Nearby'));
    if (!view.places.length) root.append(el('p', 'Walk up to a tree, a grove or an empty plot to tend it.', 'garden-care-note'));
    for (const place of view.places) {
      const row = el('div', undefined, 'garden-care-place');
      const family = place.tree?.family ? `${title(place.tree.family)} tree · ` : place.kind === 'grove' ? 'Garden grove · ' : '';
      row.append(el('strong', `${family}${place.stage === 'empty' ? 'Empty plot' : place.stage}`));
      for (const action of place.actions) {
        const button = el('button', action.label, 'small-button');
        button.disabled = !!pending;
        button.addEventListener('click', () => submit(careIntent(place, action)));
        row.append(button);
      }
      root.append(row);
    }
    if (view.atShop && view.offers.length) {
      root.append(el('h3', 'Seeds for sale'));
      const shop = el('ul', undefined, 'garden-care-items');
      for (const offer of view.offers) {
        const row = el('li');
        row.append(el('span', `${offer.label} · ${offer.price} rings`));
        const buy = el('button', 'Buy', 'small-button');
        buy.disabled = !offer.mayBuy || !!pending;
        buy.addEventListener('click', () => submit({ kind: 'purchase', itemId: offer.itemId }));
        row.append(buy); shop.append(row);
      }
      root.append(shop);
    }
    root.append(el('h3', 'Carried'));
    if (!view.items.length) root.append(el('p', 'Nothing carried. Shake trees for fruit or find animals in missions.', 'garden-care-note'));
    const list = el('ul', undefined, 'garden-care-items');
    for (const item of view.items) {
      const row = el('li');
      row.append(el('span', `${item.label} ×${item.quantity}`));
      if (item.kind !== 'seed') {
        const use = el('button', item.kind === 'animal' || item.kind === 'drive' ? 'Give' : 'Feed', 'small-button');
        use.disabled = !item.mayUse || !!pending;
        use.addEventListener('click', () => submit({ kind: 'consume', itemId: item.id }));
        row.append(use);
      }
      list.append(row);
    }
    root.append(list);
    if (view.parts.length) {
      root.append(el('h3', 'Animal parts'));
      root.append(el('p', view.parts.map(part => `${title(part.category)}: ${part.label}`).join(' · '), 'garden-care-note'));
    }
    root.append(el('p', 'Trees, fruit and animal parts follow the original game\'s rules. Seeds are not sold until their original prices are traced.', 'garden-care-note'));
    if (pending || feedback) {
      const note = el('p', pending ? 'Saving…' : feedback, 'garden-care-note');
      note.setAttribute('role', 'status'); root.append(note);
    }
  }
  return {
    update(nextCare, nextPlayer, options = {}) {
      const reconnected = !connected && !!options.connected;
      care = nextCare; player = nextPlayer; connected = !!options.connected; serverTime = options.serverTime;
      // A lost acknowledgement is retried with the identical request, never a new one.
      if (reconnected && pending) send(pending);
      render();
    },
    retryPending() { if (pending) send(pending); },
    handleEvent(event) {
      const result = matchingResult(pending, event);
      if (!result) return false;
      pending = null;
      const dropped = event.effects?.shaken === true ? event.effects.dropped : null;
      feedback = !result.accepted ? reasons[result.code] || `Not applied (${result.code}).`
        : dropped === 0 ? 'You shook the tree. Keep shaking.' : Number.isInteger(dropped) ? `${dropped} fruit fell.` : 'Saved.';
      render(); return true;
    },
  };
}
