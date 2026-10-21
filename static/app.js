// joy on every page (except login): the service worker, offline copies, and the switch for them.
(() => {
  'use strict';
  if (!('serviceWorker' in navigator) || document.body.dataset.page === 'login') return;

  const KEY = 'joy.offline-copies';
  const wantOffline = () => { try { return localStorage.getItem(KEY) !== 'off'; } catch (_) { return true; } };

  // A page restored by Back from the back/forward cache may be from before someone switched
  // who's holding the phone: always fetch it again.
  addEventListener('pageshow', (e) => { if (e.persisted) location.reload(); });

  addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {});
  });

  const tellWorker = (msg) => navigator.serviceWorker.ready.then((r) => r.active && r.active.postMessage(msg)).catch(() => {});
  tellWorker({ type: 'joy-offline-copies', on: wantOffline() });

  // ── an offline copy: say so, and don't pretend buttons work ─────────────
  const bar = (text, cls) => {
    let el = document.querySelector('.joy-bar');
    if (!el) {
      el = document.createElement('p');
      el.className = 'joy-bar';
      el.setAttribute('role', 'status');
      document.body.prepend(el);
    }
    el.className = 'joy-bar ' + cls;
    el.textContent = text;
    return el;
  };

  const savedAt = Number(document.body.dataset.offlineAt || 0);
  const goOffline = () => {
    document.body.classList.add('is-offline');
    const when = savedAt
      ? new Date(savedAt).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })
      : '';
    bar(when ? `Offline: showing what this phone saw on ${when}. Changes need a connection.`
             : 'Offline: changes need a connection.', 'is-offline');
    document.querySelectorAll('main form button, main form input, main form select, main form textarea, #draw')
      .forEach((el) => { el.disabled = true; });
  };
  if (savedAt) goOffline();
  addEventListener('offline', goOffline);
  addEventListener('online', () => { if (document.body.classList.contains('is-offline')) location.reload(); });
  document.addEventListener('htmx:sendError', goOffline);

  // ── a fresher page arrived after the saved copy was shown ───────────────
  navigator.serviceWorker.addEventListener('message', (e) => {
    if ((e.data || {}).type !== 'joy-updated') return;
    const el = bar('Updated · tap to refresh', 'is-updated');
    el.tabIndex = 0;
    el.addEventListener('click', () => location.reload(), { once: true });
    el.addEventListener('keydown', (k) => { if (k.key === 'Enter') location.reload(); }, { once: true });
  });

  // ── the "keep offline copies on this phone" switch (on /who) ────────────
  const sw = document.getElementById('offline-copies');
  if (sw) {
    sw.checked = wantOffline();
    sw.addEventListener('change', () => {
      try { localStorage.setItem(KEY, sw.checked ? 'on' : 'off'); } catch (_) {}
      tellWorker({ type: 'joy-offline-copies', on: sw.checked });
      const note = document.getElementById('offline-copies-note');
      if (note) note.textContent = sw.checked ? 'Saved pages stay on this phone for up to a week.' : 'Nothing is kept on this phone. Offline, the jar just says so.';
    });
  }
})();
