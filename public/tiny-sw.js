// Only the Tiny Garden document is controlled. API state and credentials are
// always network-only; an offline shell cannot mint or queue gameplay results.
const CACHE = 'tiny-garden-shell-v1';
const art = new Set(['tiny-neutral-chaos.png', 'tiny-hero-chaos.png', 'tiny-dark-chaos.png',
  'chao-flame.png', 'hub-grass.png', 'chao-stat-panel.png', 'chao-stat-font.png', 'discord-icon.png',
  ...['red', 'blue', 'green', 'cyan', 'purple', 'yellow', 'white'].map(color => `emerald-${color}.png`)]);

function publicFile(url) {
  return url.origin === self.location.origin && !url.search
    && (url.pathname === '/tiny.html' || url.pathname === '/tiny.webmanifest'
      || /^\/assets\/[\w.-]+\.(?:js|css)$/.test(url.pathname)
      || (url.pathname.startsWith('/api/assets/') && art.has(url.pathname.slice('/api/assets/'.length))));
}

self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(['/tiny.html', '/tiny.webmanifest'])));
  // Let an active round finish before a new worker replaces its shell.
});
self.addEventListener('activate', event => {
  event.waitUntil(caches.keys().then(keys => Promise.all(keys.filter(key => key.startsWith('tiny-garden-shell-') && key !== CACHE).map(key => caches.delete(key)))));
});
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || !publicFile(url)) return;
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    try {
      const response = await fetch(event.request);
      if (response.ok && response.type === 'basic') await cache.put(event.request, response.clone());
      return response;
    } catch (error) {
      const cached = await cache.match(event.request);
      if (cached) return cached;
      throw error;
    }
  })());
});
