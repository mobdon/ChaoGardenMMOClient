const RULES = 'adapted-tiny-shop-v1';
const validRevision = value => Number.isSafeInteger(value) && value >= 0;
const quantity = (care, id) => Number.isInteger(care?.inventory?.[id]) && care.inventory[id] >= 0 && care.inventory[id] <= 99 ? care.inventory[id] : 0;

// Prices, stock, eligibility and quantities are read from the Rust snapshot.
// This projection cannot grant items, subtract rings or apply food effects.
export function tinyShopView(profile, connection = {}) {
  const care = connection.care;
  const shop = care?.tinyShop;
  const supported = care?.rulesVersion === 'adapted-care-v1' && validRevision(care.revision)
    && shop?.rulesVersion === RULES && shop.enabled === true;
  const connected = connection.connected === true && connection.accountsV2 !== true;
  const rows = supported && Array.isArray(shop.items) ? shop.items : [];
  const items = rows.filter(item => typeof item?.itemId === 'string' && /^[a-z0-9:_-]{1,128}$/.test(item.itemId)
    && typeof item.name === 'string' && item.name.length <= 128
    && Number.isSafeInteger(item.price) && item.price > 0
    && Number.isInteger(item.maxQuantity) && item.maxQuantity > 0 && item.maxQuantity <= 99)
    .map(item => ({ ...item, quantity: quantity(care, item.itemId),
      canBuy: connected && shop.eligibility?.canPurchase === true && item.enabled === true
        && Number.isSafeInteger(profile?.rings) && profile.rings >= item.price && quantity(care, item.itemId) < item.maxQuantity,
      canFeed: connected && care.eligibility?.canCare === true && quantity(care, item.itemId) > 0
        && (care.eligibility.effectReadyAt === null || (Number.isFinite(care.eligibility.effectReadyAt)
          && Number.isFinite(connection.serverTime) && connection.serverTime >= care.eligibility.effectReadyAt)),
    }));
  return { supported, connected, revision: supported ? care.revision : null, items,
    rings: Number.isSafeInteger(profile?.rings) ? profile.rings : null,
    reason: shop?.eligibility?.reason || null };
}

export function createTinyShop({ root, send, requestId = () => crypto.randomUUID() }) {
  const document = root.ownerDocument;
  let profile = null, connection = {}, pending = null, confirmedRevision = 0, feedback = '', signature = '';
  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }
  const garden=document.querySelector?.('#tiny-watch-care');
  let feedChoice,feedButton,feedKey;
  if(garden){
    const group=element('div',undefined,'garden-food');feedChoice=element('select');feedChoice.setAttribute('aria-label','Fruit from your bag');
    feedButton=element('button','Feed','small-button');feedButton.type='button';feedButton.onclick=()=>submit('consume',feedChoice.value);
    group.append(feedChoice,feedButton);garden.after(group);
  }
  function gardenFood(view,synced){
    if(!feedChoice)return;
    const foods=view.items.filter(i=>i.quantity>0),k=JSON.stringify(foods.map(i=>[i.itemId,i.quantity]));
    if(feedKey!==k){const old=feedChoice.value;feedKey=k;feedChoice.replaceChildren();
      if(!foods.length)feedChoice.append(element('option','No fruit in bag'));
      for(const f of foods){const opt=element('option',f.name+'  x'+f.quantity);opt.value=f.itemId;feedChoice.append(opt);}
      if(foods.some(f=>f.itemId===old))feedChoice.value=old;
    }
    feedChoice.disabled=!foods.length;
    const refresh=()=>{feedButton.disabled=!!pending||!synced||!foods.some(f=>f.itemId===feedChoice.value&&f.canFeed);};
    feedChoice.onchange=refresh;refresh();
  }
  function render() {
    const view = tinyShopView(profile, connection);
    const synced = view.revision !== null && view.revision >= confirmedRevision;
    gardenFood(view,synced);
    const key = JSON.stringify([view, pending, feedback, synced]);
    if (key === signature) return;
    signature = key;
    const heading = element('div', undefined, 'tiny-panel-heading');
    heading.append(element('h2', 'Fruit shop'), element('span', view.rings === null ? 'Connect your Chao' : `◎ ${view.rings}`, 'phase-pill'));
    root.replaceChildren(heading);
    if (!view.supported) {
      root.append(element('p', 'This garden hasn’t opened its Tiny shop yet. Your saved items and rings stay with your Chao.', 'tiny-panel-copy'));
      return;
    }
    const grid = element('div', undefined, 'tiny-shop-grid');
    for (const item of view.items) {
      const card = element('article', undefined, 'tiny-shop-item');
      card.setAttribute('data-stock-key', item.itemId);
      const nativeId = /^tiny-fruit:([0-6])$/.exec(item.itemId)?.[1];
      if (nativeId !== undefined) {
        const sprite = element('img'); sprite.alt = ''; sprite.width = sprite.height = 48;
        sprite.src = `/phone-tiny/__tiny-shop/tiny-native-fruit-${nativeId}-a2ne.png`;
        sprite.addEventListener('error', () => { sprite.hidden = true; }); card.append(sprite);
      }
      card.append(element('h3', item.name), element('p', `◎ ${item.price} · In your bag: ${item.quantity} / ${item.maxQuantity}`));
      const actions = element('div', undefined, 'tiny-shop-actions');
      const buy = element('button', `Buy one · ${item.price} rings`, 'small-button');
      buy.disabled = !!pending || !synced || !item.canBuy;
      buy.addEventListener('click', () => submit('tiny_purchase', item.itemId));
      const feed = element('button', 'Feed one', 'small-button');
      feed.disabled = !!pending || !synced || !item.canFeed;
      feed.addEventListener('click', () => submit('consume', item.itemId));
      actions.append(buy, feed); card.append(actions); grid.append(card);
    }
    root.append(grid);
    const reasons = { 'activity-conflict': 'Finish your current activity before shopping.', 'receipt-capacity': 'Your garden needs attention before another purchase.', 'insufficient-rings': 'Play a minigame to earn more rings.', 'inventory-full': 'Your fruit bag is full. Feed a fruit before buying more.' };
    const note = element('p', pending ? 'Waiting for your purchase or care to be saved…' : feedback || (!view.connected ? 'Reconnect before shopping or feeding.' : reasons[view.reason] || 'Buy fruit for your bag, then feed it when your Chao is ready.'), 'tiny-panel-copy');
    note.setAttribute('role', 'status'); root.append(note);
    if (pending) {
      const retry = element('button', 'Check the same request again', 'small-button');
      retry.disabled = !view.connected;
      retry.addEventListener('click', () => { if (!retry.disabled) send(pending); }); root.append(retry);
    }
  }
  function submit(kind, itemId) {
    const view = tinyShopView(profile, connection);
    const item = view.items.find(row => row.itemId === itemId);
    if (pending || view.revision < confirmedRevision || !item || !(kind === 'tiny_purchase' ? item.canBuy : item.canFeed)) return;
    const message = { type: 'care', requestId: requestId(), expectedRevision: view.revision, intent: { kind, itemId } };
    if (send(message) === false) return;
    pending = message; feedback = ''; render();
  }
  return {
    update(next, state = {}) {
      if (next?.id !== profile?.id) { pending = null; confirmedRevision = 0; feedback = ''; }
      profile = next; connection = state; render();
    },
    handleEvent(event) {
      if (!pending || event?.type !== 'event' || event.kind !== 'care-result' || event.rulesVersion !== 'adapted-care-v1'
        || event.requestId !== pending.requestId || typeof event.accepted !== 'boolean' || !validRevision(event.revision)) return false;
      confirmedRevision = Math.max(confirmedRevision, event.revision);
      feedback = event.accepted ? 'Saved. Syncing your fruit bag and rings.' : event.code === 'chao-too-full' ? 'Your Chao is full. The fruit is still in your bag.' : `Nothing was applied (${event.code || 'unavailable'}).`;
      pending = null; render(); return true;
    },
  };
}
