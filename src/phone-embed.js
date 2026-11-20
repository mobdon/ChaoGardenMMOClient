// Optional presentation adapter for the playable phone shell. Normal Tiny
// Garden and widget visits retain their existing UI and protocol behavior.
if (new URLSearchParams(location.search).get('phone') === '1' && window.parent !== window) {
  const stylesheet = document.createElement('link');
  stylesheet.rel = 'stylesheet'; stylesheet.href = new URL('./phone-embed.css', import.meta.url).href;
  document.head.append(stylesheet);
  const tiny = Boolean(document.querySelector('#tiny-watch-canvas'));
  document.documentElement.dataset.phoneEmbed = tiny ? 'app' : 'widget';
  const views = new Set(['garden', 'stats', 'games', 'training', 'items']);
  const nativeScreens = Boolean(document.querySelector('#tiny-garden-panel'));
  document.documentElement.dataset.phoneNative = String(nativeScreens);
  const send = data => window.parent.postMessage(data, location.origin);
  function setView(view) {
    if (!views.has(view)) return;
    document.body.dataset.phoneScreen = view;
    if (nativeScreens) {
      const button = document.querySelector(`[data-screen="${view}"]`);
      if (button?.getAttribute('aria-current') !== 'page') button?.click();
    }
    const stats = document.querySelector('#tiny-stats-details');
    if (stats) stats.open = view === 'stats';
    window.scrollTo(0, 0);
  }
  if (tiny) {
    const settings = document.createElement('dialog'); settings.className = 'tiny-phone-settings';
    const heading = document.createElement('h2'); heading.textContent = 'Settings';
    const close = document.createElement('button'); close.type = 'button'; close.textContent = 'Done'; close.className = 'small-button';
    close.addEventListener('click', () => settings.close());
    const header = document.createElement('header'); header.append(heading, close); settings.append(header);
    for (const selector of ['#tiny-sync-note', '#tiny-save-note', '.tiny-session-actions', '#tiny-watch-art-note']) {
      const node = document.querySelector(selector); if (node) settings.append(node);
    }
    document.body.append(settings);
    window.addEventListener('message', event => {
      if (event.origin === location.origin && event.source === window.parent && event.data?.type === 'tiny-phone-settings') settings.showModal();
    });
    document.querySelector('#tiny-watch-note')?.setAttribute('hidden', '');
    document.querySelector('#tiny-garden-panel > .tiny-panel-copy')?.setAttribute('hidden', '');
    // Retain the existing care receipt target for its confirmed-action bridge,
    // while replacing the repetitive routine prose with actual activity below.
    document.querySelector('#tiny-watch-copy')?.setAttribute('hidden', '');
    if (!nativeScreens) { const training = document.createElement('section');
    training.className = 'phone-training';
    const title = document.createElement('h2'); title.textContent = 'AFK training';
    const status = document.createElement('strong'); status.textContent = 'Service not connected yet';
    const copy = document.createElement('p'); copy.textContent = 'Durable Rust training jobs are still being integrated. Your Chao can follow its garden routine, and all five minigames are available. This preview does not award offline training gains.';
    training.append(title, status, copy); document.querySelector('.tiny-app').append(training); }
    setView(location.hash.slice(1) || 'garden');
    let pendingCare = null;
    const careNotice = text => send({ type: 'tiny-phone-care-notice', text });
    const watchCopy = document.querySelector('#tiny-watch-copy');
    const watchToggle = document.querySelector('#tiny-watch-toggle');
    const gardenPanel = document.querySelector('#tiny-garden-panel');
    const careObserver = new MutationObserver(() => {
      if (!pendingCare) return;
      const copy = watchCopy.textContent;
      const confirmation = { pet: 'Pet received.', rest: 'Rest requested.', wake: 'Wake-up received.' }[pendingCare.kind];
      if (confirmation && copy.startsWith(confirmation)) { pendingCare = null; careNotice(confirmation); }
      else if (pendingCare.kind === 'toggle' && watchToggle.textContent === pendingCare.expected) { pendingCare = null; careNotice('Garden routine updated.'); }
      else if (copy.includes('has not confirmed') || copy.includes('could not complete')) { pendingCare = null; careNotice(copy); }
    });
    if (gardenPanel) careObserver.observe(gardenPanel, { subtree: true, childList: true, attributes: true, attributeFilter: ['disabled'] });
    window.addEventListener('message', event => {
      if (event.origin === location.origin && event.source === window.parent && event.data?.type === 'tiny-phone-view') setView(event.data.view);
      if (event.origin !== location.origin || event.source !== window.parent || event.data?.type !== 'tiny-phone-care') return;
      const kind = event.data.care;
      const button = kind === 'toggle' ? watchToggle : ['pet', 'rest', 'wake'].includes(kind) ? document.querySelector(`#tiny-watch-care [data-care="${kind}"]`) : null;
      if (pendingCare || !button || button.disabled || (kind !== 'toggle' && button.parentElement.hidden)) return careNotice('Your Chao is unavailable for that care right now.');
      pendingCare = { kind, expected: watchToggle.textContent === 'Take control' ? 'Let my Chao play' : 'Take control' };
      button.click();
    });
    const connection = document.querySelector('#tiny-connection');
    const report = () => send({ type: 'tiny-phone-status', status: connection?.dataset.status || 'offline' });
    if (connection) new MutationObserver(report).observe(connection, { attributes: true, attributeFilter: ['data-status'] });
    report();
  } else {
    const training = document.querySelector('.widget-training b');
    if (training) training.textContent = 'Service not connected';
    document.addEventListener('click', event => {
      const link = event.target.closest('a[href="/tiny.html"], a[href="/phone-tiny/tiny.html"]');
      if (link) { event.preventDefault(); send({ type: 'tiny-phone-open' }); }
    });
  }
  send({ type: 'tiny-phone-ready' });
}
