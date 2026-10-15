// joy: Slips + Write a slip. Progressive enhancement only: every form still posts without JS.
(() => {
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const $ = (s, el = document) => el.querySelector(s);

  /* ═════════ Slips (/wishlist) ═════════ */
  function initSlips() {
    if (!$('.slips-page')) return;
    const toastRegion = $('#toast');
    let toastTimer = 0;

    // Floating "Write a slip" once the header button has scrolled away (phones only via CSS).
    let fab = null;
    const syncFab = () => {
      const src = $('.write-btn');
      if (!src) { if (fab) fab.classList.remove('show'); return; }
      if (!fab) {
        fab = src.cloneNode(true);
        fab.classList.remove('write-btn');
        fab.classList.add('fab');
        fab.setAttribute('tabindex', '-1'); // the header button stays the keyboard path
        fab.setAttribute('aria-hidden', 'true');
        document.body.appendChild(fab);
      }
      fab.classList.toggle('show', src.getBoundingClientRect().bottom < 0);
    };
    addEventListener('scroll', syncFab, { passive: true });
    syncFab();

    async function swapFrom(res) {
      const html = await res.text();
      const doc = new DOMParser().parseFromString(html, 'text/html');
      const next = $('.slips-page', doc), cur = $('.slips-page');
      if (!next || !cur) { location.reload(); return; }
      const drawerOpen = $('#drawer')?.open;
      document.documentElement.classList.add('no-settle');
      cur.replaceWith(next);
      if (drawerOpen && $('#drawer')) $('#drawer').open = true;
      syncFab();
    }

    function toast(msg, undo) {
      clearTimeout(toastTimer);
      toastRegion.textContent = '';
      const t = document.createElement('div');
      t.className = 'toast';
      const m = document.createElement('span');
      m.className = 'toast-msg';
      m.textContent = msg;
      t.appendChild(m);
      let btn = null;
      if (undo) {
        btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = 'Undo';
        btn.addEventListener('click', undo);
        t.appendChild(btn);
      }
      toastRegion.appendChild(t);
      document.body.classList.add('has-toast');
      const dismiss = () => {
        if (t.contains(document.activeElement) || t.matches(':hover')) { toastTimer = setTimeout(dismiss, 1500); return; }
        t.remove();
        document.body.classList.remove('has-toast');
      };
      toastTimer = setTimeout(dismiss, 6500);
      return btn;
    }

    async function post(url) {
      const res = await fetch(url, { method: 'POST', credentials: 'same-origin', headers: { 'Content-Type': 'application/x-www-form-urlencoded' } });
      if (!res.ok) throw new Error(res.status);
      return res;
    }

    document.addEventListener('submit', async (e) => {
      const form = e.target;
      if (!(form instanceof HTMLFormElement)) return;
      const isTakeout = form.classList.contains('takeout');
      const isRestore = form.classList.contains('restore');
      if (!isTakeout && !isRestore) return; // delete posts normally behind its <details> confirm
      e.preventDefault();
      const btn = form.querySelector('button');
      if (btn.disabled) return;
      btn.disabled = true;
      const item = form.closest('li');
      const id = item?.dataset.id;
      const title = (item?.querySelector('.slip-title, .drawer-title')?.textContent || 'That slip').trim();
      try {
        if (isTakeout) {
          if (!reduced()) item.classList.add('leaving');
          const [res] = await Promise.all([post(form.action), wait(reduced() ? 0 : 220)]);
          await swapFrom(res);
          const undoBtn = toast(`Took out “${title}”. It’s in the drawer.`, async () => {
            undoBtn.disabled = true;
            try {
              await swapFrom(await post(`/wishlist/${id}/restore`));
              toast(`“${title}” is back in the jar.`);
              $(`.slip[data-id="${id}"] .slip-link`)?.focus({ preventScroll: false });
            } catch { location.reload(); }
          });
          undoBtn.focus({ preventScroll: true });
        } else {
          await swapFrom(await post(form.action));
          toast(`“${title}” is back in the jar.`);
          const back = $(`.slip[data-id="${id}"] .slip-link`);
          if (back) { back.focus({ preventScroll: true }); back.scrollIntoView({ block: 'center', behavior: reduced() ? 'auto' : 'smooth' }); }
        }
      } catch {
        HTMLFormElement.prototype.submit.call(form); // fall back to the plain POST
      }
    });
  }

  /* ═════════ Write a slip ═════════ */
  function initWrite() {
    const form = $('.write-form');
    if (!form) return;
    const slip = $('#paper-slip');
    const title = $('#f-title');
    const btn = $('#save-btn');
    let sending = false;

    // Like the old <select>: someone always wrote it.
    const people = form.querySelectorAll('input[name="added_by"]');
    if (people.length && ![...people].some((r) => r.checked)) people[0].checked = true;

    // The title is one line of thought: Enter saves, pasted newlines become spaces.
    const grow = () => {
      if (CSS.supports('field-sizing', 'content')) return;
      title.style.height = 'auto';
      title.style.height = title.scrollHeight + 'px';
    };
    title.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.isComposing) { e.preventDefault(); form.requestSubmit ? form.requestSubmit() : form.submit(); }
    });
    title.addEventListener('input', () => {
      if (/[\r\n]/.test(title.value)) {
        const p = title.selectionStart;
        title.value = title.value.replace(/\s*[\r\n]+\s*/g, ' ');
        title.setSelectionRange(p, p);
      }
      grow();
    });
    grow();

    // The slip takes the tint of where it happens.
    form.addEventListener('change', (e) => {
      if (e.target.name !== 'setting') return;
      slip.className = slip.className.replace(/\btint-\S+/g, '').trim() + ' tint-' + e.target.value;
    });

    form.addEventListener('submit', (e) => {
      if (sending) { e.preventDefault(); return; }
      title.value = title.value.replace(/\s+/g, ' ').trim();
      if (!form.checkValidity()) return; // let the browser explain what's missing
      sending = true;
      if (reduced()) { setTimeout(() => { btn.disabled = true; }, 0); return; }
      e.preventDefault();
      btn.disabled = true;
      form.classList.add('sending');
      const r = slip.getBoundingClientRect();
      if (r.bottom > 40 && r.top < innerHeight - 40) {
        slip.classList.add('folding');
      } else {
        // Slip is off-screen: fold a little copy of it just above the button instead.
        const g = document.createElement('div');
        g.className = 'ghost-slip ' + (slip.className.match(/\btint-\S+/) || [''])[0];
        g.textContent = title.value;
        g.setAttribute('aria-hidden', 'true');
        document.body.appendChild(g);
        const b = btn.getBoundingClientRect();
        g.style.left = Math.max(16, Math.min(b.left, innerWidth - g.offsetWidth - 16)) + 'px';
        g.style.top = (b.top - g.offsetHeight - 14) + 'px';
      }
      setTimeout(() => HTMLFormElement.prototype.submit.call(form), 360);
    });

    // Coming Back to a cached page: unfold and unlock.
    addEventListener('pageshow', (e) => {
      if (!e.persisted) return;
      sending = false;
      btn.disabled = false;
      form.classList.remove('sending');
      slip.classList.remove('folding');
      document.querySelectorAll('.ghost-slip').forEach((g) => g.remove());
    });
  }

  const start = () => { initSlips(); initWrite(); };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
