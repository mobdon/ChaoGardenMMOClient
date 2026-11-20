import './tiny-widget-layout.css';
import { createTinyNativeSidebar } from './tiny-native-sidebar.js';
import { addTinySidebarToggle } from './tiny-sidebar-toggle.js';

// Browser widget presentation only. Quick care is forwarded to the embedded
// Tiny app, which uses its existing Rust Watch intents and saved acknowledgments.
export function createTinyWidgetLayout() {
  const garden = document.querySelector('.widget-garden-window');
  // The card contains its own Stats and care controls. Keep them outside an
  // enclosing link so each control remains independently accessible.
  const link = document.querySelector('#garden-widget');
  const widget = document.createElement('section');
  for (const attribute of link.attributes) {
    if (attribute.name !== 'href') widget.setAttribute(attribute.name, attribute.value);
  }
  widget.append(...link.childNodes); link.replaceWith(widget);
  const layout = document.createElement('div'); layout.className = 'widget-native-layout';
  garden.before(layout); layout.append(garden);
  const sidebar = createTinyNativeSidebar(); layout.append(sidebar.element);
  addTinySidebarToggle(layout, sidebar.element);
  const actions = document.createElement('div'); actions.className = 'widget-quick-actions'; actions.setAttribute('aria-label', 'Quick Chao care');
  const activity = document.createElement('p'); activity.className = 'widget-quick-status'; activity.hidden = true;
  const status = document.createElement('p'); status.className = 'widget-quick-status'; status.setAttribute('role', 'status');
  const buttons = new Map();
  for (const [kind, label] of [['pet', 'Pet'], ['rest', 'Rest'], ['wake', 'Wake']]) {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = label; button.disabled = true;
    button.addEventListener('click', () => {
      if (button.disabled) return;
      status.textContent = 'Waiting for your garden…';
      window.parent.postMessage({ type: 'tiny-phone-care', care: kind }, location.origin);
    });
    actions.append(button); buttons.set(kind, button);
  }
  const open = document.createElement('button'); open.type = 'button'; open.textContent = 'Open';
  open.addEventListener('click', () => {
    if (window.parent !== window) window.parent.postMessage({ type: 'tiny-phone-open' }, location.origin);
    else location.assign('/phone-tiny/tiny.html#garden');
  }); actions.append(open);
  widget.after(activity, actions, status);
  window.addEventListener('message', event => {
    if (event.origin !== location.origin || event.source !== window.parent || event.data?.type !== 'tiny-phone-care-notice') return;
    status.textContent = event.data.text;
  });
  return {
    update(profile, connected, care) {
      sidebar.update(profile, connected);
      const job = care?.training?.job;
      const active = job && ['training', 'ready'].includes(job.phase);
      activity.hidden = !active && !care?.tinyCollection?.pendingEgg;
      activity.textContent = active ? `${job.stat[0].toUpperCase() + job.stat.slice(1)} training · ${Math.ceil((job.remainingMs || 0) / 1000)}s at last sync` : care?.tinyCollection?.pendingEgg ? 'Your egg is waiting in Tiny Chao Garden.' : '';
      const ready = connected && !!profile?.watch && window.parent !== window;
      for (const button of buttons.values()) button.disabled = !ready;
      buttons.get('rest').hidden = profile?.watch?.activity === 'rest';
      buttons.get('wake').hidden = profile?.watch?.activity !== 'rest';
    },
  };
}
