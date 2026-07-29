// Lifeline service worker — enables installability (PWA / "mobile app") and an
// offline app shell. Deliberately conservative: the app is a live WebSocket
// client against the local node, so we NEVER cache API or WS traffic — only the
// static shell, and always network-first so a redeploy is picked up immediately.
const SHELL = 'lifeline-shell-v1';
const ASSETS = ['/', '/manifest.webmanifest', '/icon.svg'];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(SHELL).then((c) => c.addAll(ASSETS)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== SHELL).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  // Never intercept API or WebSocket calls — they must hit the live node.
  if (e.request.method !== 'GET' || url.pathname.startsWith('/api/')) return;
  // Network-first for the shell, falling back to cache when offline.
  e.respondWith(
    fetch(e.request)
      .then((res) => {
        const copy = res.clone();
        caches.open(SHELL).then((c) => c.put(e.request, copy)).catch(() => {});
        return res;
      })
      .catch(() => caches.match(e.request).then((r) => r || caches.match('/')))
  );
});
