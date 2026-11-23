const RULES = 'adapted-tiny-owned-eggs-v1';
const integer = value => Number.isSafeInteger(value) && value >= 0;
const eggName = id => id === 0 ? 'Normal Egg' : 'Jewel Egg';
export function tinyEggView(care, connected = false) {
  const shop = care?.tinyEggShop, collection = care?.tinyCollection;
  const supported = care?.rulesVersion === 'adapted-care-v1' && integer(care.revision)
    && shop?.rulesVersion === RULES && collection?.rulesVersion === RULES;
  return { supported, revision: supported ? care.revision : null,
    shop: supported ? shop : null, collection: supported ? collection : null,
    offers: supported && Array.isArray(shop.offers) ? shop.offers.filter(egg => integer(egg?.eggId) && egg.eggId <= 11 && integer(egg.price)) : [],
    canBuy: !!(supported && connected && shop.enabled && shop.eligibility?.canPurchase === true),
    canHatch: !!(supported && connected && collection.eligibility?.canHatch === true && collection.pendingEgg),
  };
}
export function createTinyEggs({ root, send, requestId = () => crypto.randomUUID() }) {
  const doc = root.ownerDocument;
  const make = (tag, text, className) => { const element = doc.createElement(tag); if (text) element.textContent = text; if (className) element.className = className; return element; };
  let care, connected, pending = null, ack = null, profileId, feedback = '', key;
  function sprite(id) {
    const image = make('img'); image.alt = eggName(id); image.width = image.height = 64;
    image.src = `/phone-tiny/__tiny-shop/tiny-native-egg-${id}-a2ne.png`; image.style.imageRendering = 'pixelated'; return image;
  }
  function render() {
    const view = tinyEggView(care, connected);
    if (pending && ack !== null && care?.revision >= ack) { pending = null; ack = null; feedback = 'Saved.'; }
    const signature = JSON.stringify([view, pending, feedback]); if (key === signature) return; key = signature;
    root.replaceChildren(make('h2', 'Egg shop'));
    if (!view.supported) { root.append(make('p', 'The egg shop will open when your garden is ready.', 'tiny-panel-copy')); return; }
    const owned = view.collection.pendingEgg;
    if (owned) {
      const card = make('article', '', 'tiny-shop-item'); card.append(sprite(owned.nativeEggId), make('h3', `Your ${eggName(owned.nativeEggId)}`));
      card.append(make('p', 'Safe in your egg slot. Hatch it when you are ready.'));
      const hatch = make('button', 'Hatch egg', 'tiny-primary'); hatch.type = 'button'; hatch.disabled = !view.canHatch || !!pending;
      hatch.addEventListener('click', () => submit({ kind: 'tiny_egg_hatch', ownedEggId: owned.id })); card.append(hatch); root.append(card);
    } else {
      for (const offer of view.offers) {
        const card = make('article', '', 'tiny-shop-item'); card.append(sprite(offer.eggId), make('h3', eggName(offer.eggId)), make('p', offer.price ? `${offer.price.toLocaleString()} rings` : 'Free'));
        const buy = make('button', 'Get egg', 'tiny-primary'); buy.type = 'button'; buy.disabled = !view.canBuy || !!pending;
        buy.addEventListener('click', () => submit({ kind: 'tiny_egg_purchase', eggId: offer.eggId, stockEpoch: view.shop.epoch })); card.append(buy); root.append(card);
      }
      if (integer(view.shop.rotatesAt)) root.append(make('p', `Next egg at ${new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(view.shop.rotatesAt)}. One held egg at a time.`, 'tiny-panel-copy'));
    }
    const chao = Array.isArray(view.collection.chao) ? view.collection.chao : [];
    root.append(make('h3', `Your Chao · ${chao.length} / ${view.collection.maxChao}`));
    for (const child of chao) {
      const row = make('article', '', 'tiny-collection-chao'); row.append(make('strong', child.name || 'Chao'), make('span', child.active ? 'In the garden' : 'Owned hatchling'));
      if (!child.active && child.playable === false) row.append(make('p', 'Your hatchling is saved. Choosing another Chao is still being added.', 'tiny-panel-copy'));
      root.append(row);
    }
    const reasons = { 'egg-slot-full': 'Hatch your held egg before getting another.', 'collection-full': 'Your Chao collection is full.', 'insufficient-rings': 'Earn more rings to get this egg.', 'activity-conflict': 'Finish your current activity first.', 'native-data-unavailable': 'Eggs are unavailable right now.' };
    root.append(make('p', pending ? 'Saving your egg…' : feedback || reasons[view.shop.eligibility?.reason] || '', 'tiny-panel-copy'));
    if (pending) { const retry = make('button', 'Check the same request again', 'small-button'); retry.disabled = !connected; retry.addEventListener('click', () => send(pending)); root.append(retry); }
  }
  function submit(intent) {
    const view = tinyEggView(care, connected);
    if (pending || (intent.kind === 'tiny_egg_purchase' ? !view.canBuy : !view.canHatch)) return;
    const message = { type: 'care', requestId: requestId(), expectedRevision: view.revision, intent };
    if (send(message) === false) return;
    pending = message; ack = null; feedback = ''; render();
  }
  return {
    update(profile, state) {
      if (profileId !== profile?.id) { profileId = profile?.id; pending = null; ack = null; feedback = ''; }
      care = state.care; connected = state.connected && !state.accountsV2; render();
    },
    handleEvent(event) {
      if (!pending || event?.kind !== 'care-result' || event.requestId !== pending.requestId || event.rulesVersion !== 'adapted-care-v1' || !integer(event.revision)) return false;
      if (event.accepted === true) ack = event.revision;
      else { pending = null; ack = null; feedback = `The egg was not changed (${event.code || 'unavailable'}).`; }
      render(); return true;
    },
  };
}
