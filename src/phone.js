import './phone.css';

// Device chrome only. Embedded, existing Tiny Chao Garden owns its transport and
// submits intents to Rust. This shell never reads credentials or simulates
// care, movement, stats, scores, rewards, training or saves.
const app = document.querySelector('#phone-app');
const widget = document.querySelector('#phone-widget');
const tinyPath = '/phone-tiny/tiny.html?phone=1';
const widgetPath = '/phone-tiny/widget.html?phone=1&view=phone';
const views = new Set(['garden', 'stats', 'items', 'games', 'training', 'widget']);
const labels = { garden: 'YOUR GARDEN', stats: 'CHAO DATA', items: 'SHOP & ITEMS', games: 'FIVE LITTLE ADVENTURES', training: 'AFK TRAINING', widget: 'HOME SCREEN' };
const history = ['garden'];
let selected = 'garden';

function select(view, remember = true) {
  if (!views.has(view)) return;
  if (remember && view !== selected) history.push(view);
  selected = view;
  app.hidden = view === 'widget'; widget.hidden = view !== 'widget';
  if (view === 'widget' && widget.getAttribute('src') === 'about:blank') widget.src = widgetPath;
  if (view !== 'widget') app.contentWindow?.postMessage({ type: 'tiny-phone-view', view }, location.origin);
  document.querySelector('#phone-view-label').textContent = labels[view];
  for (const button of document.querySelectorAll('[data-phone-view]')) button.setAttribute('aria-pressed', String(button.dataset.phoneView === view));
}

document.querySelector('.phone-tabs').addEventListener('click', event => {
  const button = event.target.closest('[data-phone-view]');
  if (button) select(button.dataset.phoneView);
});
document.querySelector('#phone-home').addEventListener('click', () => select('widget'));
document.querySelector('#phone-settings').addEventListener('click', () => {
  if (selected === 'widget') select('garden');
  app.contentWindow?.postMessage({ type: 'tiny-phone-settings' }, location.origin);
});
document.querySelector('#phone-back').addEventListener('click', () => {
  if (history.length > 1) history.pop();
  select(history.at(-1), false);
});
document.querySelector('#phone-reload').addEventListener('click', () => {
  const frame = selected === 'widget' ? widget : app;
  frame.src = selected === 'widget' ? widgetPath : `${tinyPath}#${selected}`;
});
window.addEventListener('message', event => {
  if (event.origin !== location.origin || ![app.contentWindow, widget.contentWindow].includes(event.source)) return;
  if (event.data?.type === 'tiny-phone-open') select('garden');
  if (event.data?.type === 'tiny-phone-desktop' && event.source === app.contentWindow) location.assign('/');
  if (event.data?.type === 'tiny-phone-care' && event.source === widget.contentWindow) app.contentWindow?.postMessage({ type: 'tiny-phone-care', care: event.data.care }, location.origin);
  if (event.data?.type === 'tiny-phone-care-notice' && event.source === app.contentWindow) widget.contentWindow?.postMessage(event.data, location.origin);
  if (event.data?.type === 'tiny-phone-ready' && event.source === app.contentWindow) select(selected, false);
  if (event.data?.type === 'tiny-phone-status' && event.source === app.contentWindow) {
    const live = event.data.status === 'live';
    document.querySelector('.lab-connection').dataset.live = String(live);
    document.querySelector('#phone-connection').textContent = live ? 'Connected to the Rust garden' : event.data.status === 'connecting' ? 'Connecting to your saved Chao…' : 'Open an existing save to connect';
  }
});
function updateClock() {
  document.querySelector('#phone-clock').textContent = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date());
}
updateClock(); setInterval(updateClock, 30000);
