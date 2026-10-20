// joy — login. Progressive enhancement over a plain POST /login form.
// Right passcode (303): the lid twists and lifts, then we go in. A tap or key skips it.
// Wrong passcode (200 + re-rendered page): the lid wobbles "nope" and we show the server's words.
(() => {
  const form = document.getElementById('login-form');
  if (!form || !window.fetch || !window.URLSearchParams) return;

  const stage = document.getElementById('login');
  const input = form.querySelector('#passcode');
  const errorEl = form.querySelector('#login-error');
  const go = form.querySelector('.login-go');
  const idle = go.querySelector('[data-idle]');
  const busy = go.querySelector('[data-busy]');
  const opening = go.querySelector('[data-open]');
  const toggle = form.querySelector('.field-toggle');
  const reduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const HERO = 1100;

  let inFlight = false;
  let leaving = false;

  // ── show / hide toggle (only exists once JS is here) ──────────
  if (toggle) {
    toggle.hidden = false;
    form.querySelector('.field').classList.add('has-toggle');
    const label = toggle.querySelector('[data-label]');
    toggle.addEventListener('click', () => {
      const show = input.type === 'password';
      input.type = show ? 'text' : 'password';
      toggle.setAttribute('aria-pressed', String(show));
      label.textContent = show ? 'Hide passcode' : 'Show passcode';
      input.focus({ preventScroll: true });
    });
  }

  // Server-rendered error (no-JS path or first paint): let the wobble finish, then clear the class.
  if (stage.classList.contains('is-nope')) {
    setTimeout(() => stage.classList.remove('is-nope'), 1800);
  }

  // Keep the browser's own "required" check out of the way; we say it kindly instead.
  form.noValidate = true;

  const say = (text) => {
    errorEl.textContent = '';               // clear first so a repeat is re-announced
    requestAnimationFrame(() => { errorEl.textContent = text; });
    if (reduced() && errorEl.animate) {
      errorEl.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150, easing: 'ease-out' });
    }
  };

  const lock = (on) => {
    inFlight = on;
    go.disabled = on;
    go.setAttribute('aria-busy', String(on));
    idle.hidden = on;
    busy.hidden = !on;
    input.readOnly = on;
  };

  const wobble = () => {
    if (reduced()) return;
    stage.classList.remove('is-nope');
    void stage.offsetWidth;                 // restart the animation
    stage.classList.add('is-nope');
    clearTimeout(wobble.t);
    wobble.t = setTimeout(() => stage.classList.remove('is-nope'), 1800);
  };

  const enter = () => {
    if (leaving) return;
    leaving = true;
    // Same place the server's redirect points: ask who's holding this phone (logout forgets it).
    window.location.href = '/who';
  };

  const open = () => {
    input.blur();                            // drop the phone keyboard so the jar is in view
    errorEl.textContent = '';
    busy.hidden = true;
    opening.hidden = false;
    if (reduced()) {
      const a = document.body.animate
        ? document.body.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, easing: 'ease-out', fill: 'forwards' })
        : null;
      if (a) a.onfinish = enter; else enter();
      setTimeout(enter, 200);
      return;
    }
    stage.classList.remove('is-nope');
    stage.classList.add('is-open');
    const skip = (e) => { if (!e.repeat) enter(); };
    // attach after this frame so the submitting keypress/tap isn't counted as a skip
    setTimeout(() => {
      window.addEventListener('pointerdown', skip, { once: true });
      window.addEventListener('keydown', skip);
    }, 60);
    setTimeout(enter, HERO);
  };

  // Coming Back from inside restores this page from the bfcache: put the lid back on.
  window.addEventListener('pageshow', (e) => {
    if (!e.persisted) return;
    stage.classList.remove('is-open', 'is-nope');
    if (document.body.getAnimations) document.body.getAnimations().forEach((a) => a.cancel());
    leaving = false;
    lock(false);
    opening.hidden = true;
    input.value = '';
    errorEl.textContent = '';
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    if (inFlight || leaving) return;

    if (!input.value.trim()) {
      say('Write our passcode first.');
      wobble();
      input.focus();
      return;
    }

    const body = new URLSearchParams(new FormData(form)); // before locking: disabled fields aren't sent
    lock(true);

    let res;
    try {
      res = await fetch(form.action, {
        method: 'POST',
        body,
        redirect: 'manual',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      });
    } catch (_) {
      lock(false);
      say('Couldn’t reach the jar. Try once more?');
      return;
    }

    if (res.type === 'opaqueredirect' || res.status === 0 || res.status === 303 || res.redirected) {
      open();                                // the cookie is already set
      return;
    }

    if (res.status === 200) {
      let text = '';
      try {
        const html = await res.text();
        const doc = new DOMParser().parseFromString(html, 'text/html');
        const el = doc.getElementById('login-error');
        text = el ? el.textContent.trim() : '';
      } catch (_) { /* fall through to the default */ }
      lock(false);
      say(text || 'That’s not it.');
      wobble();
      input.focus();
      input.select();
      return;
    }

    lock(false);
    say('Something went wrong on our side. Try once more?');
  });
})();
