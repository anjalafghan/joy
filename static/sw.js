// joy service worker, build __BUILD__ (filled in by the server at /sw.js).
// Opens instantly and keeps a read-only copy for offline, without ever showing one person's
// view to the other. Design: docs/year-in-review-and-offline.md, section 4.
'use strict';

const V = '__BUILD__';
const STATIC = 'joy-static-' + V;
const PAGES = 'joy-pages';
const PHOTOS = 'joy-photos';
const META = 'joy-meta';
const META_KEY = '/__joy-meta';
const MAX_AGE_MS = 7 * 24 * 3600 * 1000; // offline copies older than a week are never shown
const PHOTO_MAX_BYTES = 25 * 1024 * 1024;
const PHOTO_MAX_COUNT = 80;
const NETWORK_TIMEOUT_MS = 2000;

// Pages kept for offline reading (path only; pages with a query string are never stored).
const OFFLINE_PAGES = (p) => p === '/' || p === '/history' || p === '/wishlist' || p === '/year' || /^\/year\/\d{4}(\/\d{1,2})?$/.test(p);

const VERSIONED = [
  'tokens.css', 'jar.css', 'jar.js', 'kept.css', 'kept.js', 'slips.css', 'slips.js', 'who.css',
  'year.css', 'year.js', 'month.css', 'app.js', 'htmx.min.js', 'manifest.webmanifest', 'icons/icon-180.png',
].map((f) => `/static/${f}?v=${V}`);
const UNVERSIONED = [
  '/offline',
  '/static/fonts/Fraunces-normal.woff2', '/static/fonts/Fraunces-italic.woff2',
  '/static/fonts/InstrumentSans-normal.woff2', '/static/fonts/InstrumentSans-italic.woff2',
  '/static/fonts/Caveat-normal.woff2', '/static/icons/icon-192.png', '/static/icons/icon-512.png',
];

// ── install / activate ───────────────────────────────────────────────────

self.addEventListener('install', (e) => {
  e.waitUntil((async () => {
    const cache = await caches.open(STATIC);
    // Not cache.addAll: it would happily store a redirect to /login as the offline page.
    await Promise.all([...VERSIONED, ...UNVERSIONED].map(async (url) => {
      try {
        const res = await fetch(url, { cache: 'no-cache', credentials: 'same-origin' });
        if (res.ok && !res.redirected) await cache.put(url, res);
      } catch (_) { /* a missing file just isn't precached */ }
    }));
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names.filter((n) => n.startsWith('joy-static-') && n !== STATIC).map((n) => caches.delete(n)));
    await dropExpired(PAGES);
    await dropExpired(PHOTOS);
    if (self.registration.navigationPreload) await self.registration.navigationPreload.enable();
    await self.clients.claim();
  })());
});

// ── meta: who this phone's cache belongs to, and the "no offline copies" switch ──

async function getMeta() {
  const res = await (await caches.open(META)).match(META_KEY);
  try { return res ? await res.json() : {}; } catch (_) { return {}; }
}

async function setMeta(m) {
  await (await caches.open(META)).put(META_KEY, new Response(JSON.stringify(m), { headers: { 'content-type': 'application/json' } }));
}

// Keep the "no offline copies" choice; forget only whose copies these were.
async function forgetViewer() {
  const { noOffline } = await getMeta();
  await setMeta({ noOffline });
}

async function purge() {
  await Promise.all([caches.delete(PAGES), caches.delete(PHOTOS)]);
}

// ── routing ──────────────────────────────────────────────────────────────

self.addEventListener('fetch', (e) => {
  const req = e.request;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;

  // Switching who's holding the phone, or logging out: forget every personal copy first.
  if (req.method === 'POST' && (url.pathname === '/who' || url.pathname === '/logout')) {
    e.respondWith(purge().then(forgetViewer).then(() => fetch(req)));
    return;
  }
  // Only plain GETs are ever answered from a cache. htmx calls, posts, .ics, /pick, /match… pass through.
  if (req.method !== 'GET' || req.headers.get('HX-Request')) return;

  if (url.pathname.startsWith('/static/')) {
    e.respondWith(cacheFirst(req, STATIC));
    return;
  }
  // Versioned photos only; a reload (after a new upload) always goes to the network.
  if (url.pathname.startsWith('/photos/') && url.searchParams.has('v') && req.cache !== 'reload') {
    e.respondWith(photo(req));
    return;
  }
  if (req.mode === 'navigate') {
    if (OFFLINE_PAGES(url.pathname) && !url.search) {
      e.respondWith(page(e, req, url.pathname));
    } else {
      e.respondWith(fetch(req).catch(() => offlinePage()));
    }
  }
});

async function cacheFirst(req, name) {
  const cache = await caches.open(name);
  const hit = await cache.match(req);
  if (hit) return hit;
  const res = await fetch(req);
  if (res.ok && !res.redirected && res.type === 'basic') cache.put(req, res.clone());
  return res;
}

async function offlinePage() {
  return (await caches.match('/offline')) || new Response('Offline', { status: 503, headers: { 'content-type': 'text/plain' } });
}

const timeout = (p, ms) => Promise.race([p, new Promise((_, no) => setTimeout(() => no(new Error('timeout')), ms))]);

// ── pages: network first, this phone's copy when there's no signal ───────

async function page(e, req, key) {
  const meta = await getMeta();
  const net = (async () => (await e.preloadResponse) || fetch(req))();
  const fresh = net.then(async (res) => {
    await store(key, res.clone());
    return res;
  });
  try {
    return await timeout(fresh, NETWORK_TIMEOUT_MS);
  } catch (_) {
    const copy = await cachedPage(key, meta);
    if (copy) {
      // Slow network: show the copy now; if the fresh page lands, offer it (never swap mid-read).
      e.waitUntil(fresh.then(() => tell({ type: 'joy-updated' })).catch(() => {}));
      return copy;
    }
    try { return await fresh; } catch (_) { return offlinePage(); }
  }
}

async function store(key, res) {
  if (!res.ok || res.redirected || res.type !== 'basic') return;
  const meta = await getMeta();
  if (meta.noOffline) return;
  const viewer = res.headers.get('x-joy-viewer') || 'none';
  // A page made for someone else than this cache's owner: start over, never mix the two.
  if (meta.viewer !== undefined && meta.viewer !== viewer) await purge();
  if (meta.viewer !== viewer) await setMeta({ ...meta, viewer });
  const body = await res.text();
  const headers = new Headers(res.headers);
  headers.set('x-joy-fetched', String(Date.now()));
  headers.set('x-joy-viewer', viewer);
  await (await caches.open(PAGES)).put(key, new Response(body, { status: 200, headers }));
  if (key === '/history' || key.startsWith('/year')) await prunePhotos(body);
}

async function cachedPage(key, meta) {
  const hit = await (await caches.open(PAGES)).match(key);
  if (!hit) return null;
  const fetched = Number(hit.headers.get('x-joy-fetched') || 0);
  const viewer = hit.headers.get('x-joy-viewer');
  if (meta.noOffline || viewer !== meta.viewer || Date.now() - fetched > MAX_AGE_MS) return null;
  // Mark it, so the page can say it's an offline copy and disable buttons.
  const html = (await hit.text()).replace('<body ', `<body data-offline-at="${fetched}" `);
  const headers = new Headers(hit.headers);
  return new Response(html, { status: 200, headers });
}

// ── photos: cache first, capped, and forgotten when the page stops linking them ──

async function photo(req) {
  const meta = await getMeta();
  const cache = await caches.open(PHOTOS);
  const hit = await cache.match(req);
  if (hit && Date.now() - Number(hit.headers.get('x-joy-fetched') || 0) <= MAX_AGE_MS) return hit;
  try {
    const res = await fetch(req);
    if (res.ok && !res.redirected && res.type === 'basic' && !meta.noOffline) {
      const blob = await res.clone().blob();
      const headers = new Headers(res.headers);
      headers.set('x-joy-fetched', String(Date.now()));
      headers.set('x-joy-size', String(blob.size));
      await cache.put(req, new Response(blob, { status: 200, headers }));
      await trimPhotos(cache);
    }
    return res;
  } catch (err) {
    if (hit) return hit;
    throw err;
  }
}

async function trimPhotos(cache) {
  const keys = await cache.keys();
  const entries = await Promise.all(keys.map(async (k) => {
    const r = await cache.match(k);
    return { k, at: Number(r.headers.get('x-joy-fetched') || 0), size: Number(r.headers.get('x-joy-size') || 0) };
  }));
  entries.sort((a, b) => a.at - b.at); // oldest first
  let total = entries.reduce((s, x) => s + x.size, 0);
  while (entries.length && (entries.length > PHOTO_MAX_COUNT || total > PHOTO_MAX_BYTES)) {
    const old = entries.shift();
    total -= old.size;
    await cache.delete(old.k);
  }
}

// A deleted or replaced photo shouldn't linger: keep only what Kept/Year still link to.
async function prunePhotos(html) {
  const pages = await caches.open(PAGES);
  let linked = new Set((html.match(/\/photos\/\d+\?v=\d+/g) || []));
  for (const other of ['/history', ...(await pages.keys()).map((k) => new URL(k.url).pathname).filter((p) => p.startsWith('/year'))]) {
    const r = await pages.match(other);
    if (r) (await r.text()).match(/\/photos\/\d+\?v=\d+/g)?.forEach((u) => linked.add(u));
  }
  const cache = await caches.open(PHOTOS);
  for (const k of await cache.keys()) {
    const u = new URL(k.url);
    if (!linked.has(u.pathname + u.search)) await cache.delete(k);
  }
}

async function dropExpired(name) {
  const cache = await caches.open(name);
  for (const k of await cache.keys()) {
    const r = await cache.match(k);
    if (!r || Date.now() - Number(r.headers.get('x-joy-fetched') || 0) > MAX_AGE_MS) await cache.delete(k);
  }
}

async function tell(msg) {
  for (const c of await self.clients.matchAll({ type: 'window' })) c.postMessage(msg);
}

// ── messages from the page ───────────────────────────────────────────────

self.addEventListener('message', (e) => {
  const m = e.data || {};
  if (m.type === 'joy-offline-copies') {
    e.waitUntil((async () => {
      const meta = await getMeta();
      await setMeta({ ...meta, noOffline: !m.on });
      if (!m.on) await purge();
    })());
  } else if (m.type === 'joy-purge') {
    e.waitUntil(purge());
  }
});
