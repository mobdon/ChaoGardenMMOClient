// Read-only projection of the canonical profile. A shop catalog is not an
// inventory, and omitted quantities must never become an invented zero.
const itemNames = {
  eggshell: 'Eggshell Cap',
  'shadow-air-shoes': 'Shadow Air Shoes',
  'knuckles-hammer-gloves': 'Knuckles Hammer Gloves',
  'dark-chao-walker': 'Dark Chao Walker',
  'garden-fruit': 'Garden Fruit',
};

function itemId(value) {
  return typeof value === 'string' && /^[a-z0-9][a-z0-9_-]{0,127}$/i.test(value) ? value : null;
}

export function itemLabel(value) {
  const id = itemId(value);
  return id ? itemNames[id] || id.replaceAll(/[-_]+/g, ' ').replace(/\b\w/g, letter => letter.toUpperCase()) : 'Not reported';
}

function equipmentSlot(key, label, value) {
  const id = itemId(value);
  return { key, label, id, reported: value === null || !!id, value: value === null ? 'None equipped' : itemLabel(id) };
}

export function tinyInventoryView(profile, { connected = false } = {}) {
  if (!profile || typeof profile.id !== 'string') return {
    connected: false, hasProfile: false, cosmeticsStatus: 'unreported', cosmetics: [], equipment: [],
  };
  const rawCosmetics = profile.ownedCosmetics;
  const cosmeticsReported = Array.isArray(rawCosmetics) && rawCosmetics.every(value => itemId(value));
  const cosmeticIds = cosmeticsReported ? [...new Set(rawCosmetics)] : [];
  return {
    connected: !!connected, hasProfile: true,
    cosmeticsStatus: !cosmeticsReported ? 'unreported' : cosmeticIds.length ? 'reported' : 'empty',
    cosmetics: cosmeticIds.map(id => ({ id, label: itemLabel(id), equipped: profile.equipped === id })),
    equipment: [
      equipmentSlot('head', 'Head', profile.equipped),
      equipmentSlot('footwear', 'Footwear', profile.gear?.footwear),
      equipmentSlot('hands', 'Hands', profile.gear?.hands),
    ],
  };
}

export function tinyCareView(care, { connected = false, careMutations = false, accountsV2 = false, serverTime } = {}) {
  const nativeId = key => /^sa2:(?:[0-9]|1[0-9]|2[0-4])$/.test(key);
  const valid = care?.rulesVersion === 'adapted-care-v1' && Number.isSafeInteger(care.revision) && care.revision >= 0;
  const inventory = care?.inventory;
  const reported = valid && inventory && typeof inventory === 'object' && !Array.isArray(inventory)
    && Object.entries(inventory).every(([id, quantity]) => (id === 'garden-fruit' || nativeId(id)) && Number.isInteger(quantity) && quantity >= 0 && quantity <= 99);
  const rows = Array.isArray(care?.catalog?.items) ? care.catalog.items : [];
  const native = new Map(rows.filter(row => Number.isInteger(row?.id) && row.id >= 0 && row.id < 25).map(row => [`sa2:${row.id}`, row]));
  const readyAt = care?.eligibility?.effectReadyAt;
  const cooldownReady = readyAt === null || (Number.isFinite(readyAt) && Number.isFinite(serverTime) && serverTime >= readyAt);
  const mayCare = !!(reported && connected && careMutations === true && accountsV2 !== true
    && care.eligibility?.canCare === true && cooldownReady);
  return {
    valid, reported: !!reported, revision: valid ? care.revision : null, mayCare,
    items: reported ? Object.entries(inventory).map(([id, quantity]) => {
      const source = native.get(id);
      const named = typeof source?.name === 'string' && source.name.length > 0 && source.name.length <= 200;
      const available = id === 'garden-fruit' || (care.catalog?.enabled === true && !!source);
      return { id, quantity, label: id === 'garden-fruit' ? 'Garden Fruit' : named ? source.name : `Native item ${id.slice(4)}`,
        detail: id === 'garden-fruit' ? 'Adapted food' : source?.kind === 'animal' ? 'Animal · adapted effects' : source?.kind === 'drive' ? 'Drive · adapted effects' : 'Item details unavailable',
        mayConsume: mayCare && quantity > 0 && available };
    }) : [],
  };
}

export function matchingCareResult(request, event) {
  if (!request || event?.type !== 'event' || event.kind !== 'care-result' || event.rulesVersion !== 'adapted-care-v1'
    || event.requestId !== request.requestId || typeof event.accepted !== 'boolean'
    || !Number.isSafeInteger(event.revision) || event.revision < 0) return null;
  return { accepted: event.accepted, replayed: event.replayed === true, revision: event.revision, code: typeof event.code === 'string' ? event.code : 'unavailable' };
}

export function createTinyItems({ root, send = () => false, requestId = () => globalThis.crypto.randomUUID() }) {
  if (!root) throw new TypeError('Tiny Garden items need a panel root.');
  const document = root.ownerDocument;
  let previous = '';
  let profile = null, connection = {}, pending = null, confirmedRevision = 0, feedback = '';
  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }

  function render() {
      const view = tinyInventoryView(profile, connection);
      const care = tinyCareView(connection.care, connection);
      const reconciled = !pending && care.revision >= confirmedRevision;
      const signature = JSON.stringify([view, care, pending, feedback, reconciled]);
      if (signature === previous) return;
      previous = signature;
      const heading = element('div', undefined, 'tiny-panel-heading');
      heading.append(element('h2', 'Items & equipment'));
      const status = element('span', view.connected ? 'Live profile' : view.hasProfile ? 'Last synced items' : 'Connect to view', 'phase-pill');
      status.setAttribute('role', 'status');
      heading.append(status);
      root.replaceChildren(heading);
      if (!view.hasProfile) {
        root.append(element('p', 'Connect your saved Chao to see their owned items and equipment.', 'tiny-panel-copy'));
        return;
      }
      if (!view.connected) root.append(element('p', 'Showing the last received profile. Reconnect before using items.', 'tiny-panel-copy'));
      root.append(element('h3', 'Food, animals & drives', 'tiny-items-subheading'));
      if (care.reported) {
        if (!care.items.length) root.append(element('p', 'No food, animals or drives owned in this profile.', 'tiny-panel-copy'));
        else {
          const list = element('ul', undefined, 'tiny-items-list');
          for (const item of care.items) {
            const row = element('li');
            row.append(element('strong', `${item.label} ×${item.quantity}`), element('span', item.detail));
            const use = element('button', 'Use one', 'small-button');
            use.disabled = !item.mayConsume || !reconciled;
            use.addEventListener('click', () => submit({ kind: 'consume', itemId: item.id }));
            row.append(use); list.append(row);
          }
          root.append(list);
        }
        const pet = element('button', 'Pet my Chao', 'small-button');
        pet.disabled = !care.mayCare || !reconciled;
        pet.addEventListener('click', () => submit({ kind: 'pet' })); root.append(pet);
        if (!care.mayCare) root.append(element('p', connection.accountsV2 || !connection.careMutations ? 'Care actions are not available on this garden yet.' : 'Care is unavailable during the current activity or cooldown.', 'tiny-panel-copy'));
      } else root.append(element('p', 'Food, animals and drives: inventory is not reported by this server.', 'tiny-panel-copy'));
      if (feedback || pending) {
        const note = element('p', pending ? 'Waiting for saved care confirmation. Inventory updates after syncing.' : feedback, 'tiny-panel-copy');
        note.setAttribute('role', 'status'); root.append(note);
      }
      if (pending) {
        const retry = element('button', 'Retry same care request', 'small-button');
        retry.disabled = !view.connected || !connection.careMutations || connection.accountsV2 === true;
        retry.addEventListener('click', () => { if (!retry.disabled) send(pending); }); root.append(retry);
      }
      root.append(element('h3', 'Owned cosmetics', 'tiny-items-subheading'));
      if (view.cosmeticsStatus === 'reported') {
        const list = element('ul', undefined, 'tiny-items-list');
        for (const item of view.cosmetics) {
          const row = element('li');
          row.append(element('strong', item.label), element('span', item.equipped ? 'Equipped' : 'Owned'));
          list.append(row);
        }
        root.append(list);
      } else root.append(element('p', view.cosmeticsStatus === 'empty' ? 'No cosmetics owned in this profile.' : 'Cosmetic ownership is not reported by this server.', 'tiny-panel-copy'));
      root.append(element('h3', 'Equipped on your Chao', 'tiny-items-subheading'));
      const slots = element('dl', undefined, 'tiny-items-equipment');
      for (const slot of view.equipment) slots.append(element('dt', slot.label), element('dd', slot.value));
      root.append(slots);
  }
  function submit(intent) {
    const care = tinyCareView(connection.care, connection);
    if (pending || !care.mayCare || care.revision < confirmedRevision) return;
    if (intent.kind === 'consume' && !care.items.some(item => item.id === intent.itemId && item.mayConsume)) return;
    const request = { type: 'care', requestId: requestId(), expectedRevision: care.revision, intent };
    if (send(request) === false) return;
    pending = request; feedback = ''; render();
  }
  return {
    update(nextProfile, nextConnection = {}) {
      if (nextProfile?.id !== profile?.id) { pending = null; confirmedRevision = 0; feedback = ''; }
      profile = nextProfile; connection = nextConnection; render();
    },
    handleEvent(event) {
      const result = matchingCareResult(pending, event);
      if (!result) return false;
      confirmedRevision = Math.max(confirmedRevision, result.revision);
      pending = null;
      feedback = result.accepted ? result.replayed ? 'Care was already saved. Syncing the current inventory.' : 'Care saved. Syncing the current inventory.'
        : `Care was not applied (${result.code}).`;
      render(); return true;
    },
  };
}
