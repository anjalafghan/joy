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
    // An observer rather than a scroll listener: no layout read on every scroll frame.
    let fab = null, fabIo = null;
    const syncFab = () => {
      fabIo?.disconnect();
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
      fabIo = new IntersectionObserver(([e]) => fab.classList.toggle('show', !e.isIntersecting && e.boundingClientRect.bottom < 0));
      fabIo.observe(src);
    };
    syncFab();

    // Search: every word must appear in a slip's data-search. The query lives out here, not in the DOM, so it survives swapFrom.
    let query = '';
    const hayCache = new WeakMap();
    const norm = (s) => s.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
    const hay = (li) => {
      let h = hayCache.get(li);
      if (h === undefined) { h = norm(li.dataset.search || ''); hayCache.set(li, h); }
      return h;
    };

    // Filters whatever is in the DOM right now, so it also works right after a swap.
    function filterSlips() {
      const page = $('.slips-page');
      if (!page) return;
      const words = norm(query).trim().split(/\s+/).filter(Boolean);
      const on = words.length > 0;
      page.classList.toggle('searching', on);
      const hint = $('.slips-hint');
      if (hint) hint.hidden = on;
      const byId = new Map();
      let all = 0;
      page.querySelectorAll('section.group').forEach((sec) => {
        const slips = sec.querySelectorAll('li.slip');
        let m = 0;
        slips.forEach((li) => {
          const ok = !on || words.every((w) => hay(li).includes(w));
          li.hidden = !ok;
          if (ok) m++;
        });
        sec.hidden = on && m === 0;
        const cnt = sec.querySelector('.group-n');
        if (cnt) {
          if (!cnt.dataset.orig) cnt.dataset.orig = cnt.textContent;
          cnt.textContent = on ? `${m} of ${slips.length} ${slips.length === 1 ? 'slip' : 'slips'}` : cnt.dataset.orig;
        }
        byId.set(sec.id, { m, n: slips.length });
        all += m;
      });
      page.querySelectorAll('.group-index li').forEach((li) => {
        const a = li.querySelector('a[href^="#"]');
        const g = a && byId.get(a.getAttribute('href').slice(1));
        if (!g) return;
        li.hidden = on && g.m === 0;
        const n = a.querySelector('.n');
        if (n) {
          if (!n.dataset.orig) n.dataset.orig = n.textContent;
          n.textContent = on ? `${g.m} of ${g.n}` : n.dataset.orig;
        }
      });
      const status = $('#search-status');
      if (status) {
        status.textContent = !on ? '' : all === 0 ? `Nothing in the jar matches “${query.trim()}”.`
          : all === 1 ? '1 slip matches' : `${all} slips match`;
      }
    }

    // Runs on every (re)rendered .slips-page. The old input and its listeners go away with the old DOM.
    function initSearch() {
      const q = $('#slip-q');
      if (!q) return;
      q.closest('.slip-search').hidden = false;
      q.value = query;
      q.addEventListener('input', () => { query = q.value; filterSlips(); });
      q.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && q.value) { e.preventDefault(); q.value = query = ''; filterSlips(); }
      });
      filterSlips();
    }

    // "/" jumps to the search box unless focus is already in a field. Bound once; the box is looked up at press time.
    addEventListener('keydown', (e) => {
      if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.isComposing) return;
      const t = e.target;
      if (t instanceof HTMLElement && (t.matches('input, textarea, select') || t.isContentEditable)) return;
      const q = $('#slip-q');
      if (!q) return;
      e.preventDefault();
      q.focus({ preventScroll: true });
    });
    initSearch();

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
      initSearch();
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
