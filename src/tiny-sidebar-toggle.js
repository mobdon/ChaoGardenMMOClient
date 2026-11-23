export function addTinySidebarToggle(layout, sidebar) {
  const button = document.createElement('button'); button.type = 'button'; button.className = 'tiny-sidebar-tab';
  button.textContent = 'Stats ▥'; button.setAttribute('aria-expanded', 'false'); button.setAttribute('aria-label', 'Open Chao stats');
  const id = `tiny-sidebar-${document.querySelectorAll('.tiny-sidebar-tab').length + 1}`; sidebar.id = id; button.setAttribute('aria-controls', id);
  sidebar.hidden = true; layout.dataset.stats = 'closed'; layout.before(button);
  button.addEventListener('click', event => {
    event.preventDefault(); event.stopPropagation();
    const expanded = button.getAttribute('aria-expanded') !== 'true';
    button.setAttribute('aria-expanded', String(expanded)); button.setAttribute('aria-label', expanded ? 'Close Chao stats' : 'Open Chao stats');
    button.textContent = expanded ? 'Close stats ×' : 'Stats ▥'; sidebar.hidden = !expanded; layout.dataset.stats = expanded ? 'open' : 'closed';
  });
}
