/* joy — the Jar (take A: the physical jar).
   A tiny 2D slip simulation on canvas, the live "tonight" sentence, and the
   draw / put back / stamp choreography around htmx. No libraries. */
(() => {
  'use strict';

  const $ = (s, r = document) => r.querySelector(s);
  const RM = matchMedia('(prefers-reduced-motion: reduce)');
  const DARK = matchMedia('(prefers-color-scheme: dark)');
  const reduced = () => RM.matches;

  const jarEl = $('#jar');
  if (!jarEl) return;
  const canvas = jarEl.querySelector('canvas');
  const ctx = canvas.getContext('2d');
  const pick = $('#pick');
  const form = $('#tonight');
  const drawBtn = $('#draw');
  let slips = [];
  try { slips = JSON.parse($('#slips').textContent || '[]'); } catch (_) { slips = []; }
  const TOTAL = slips.length;
  const RAINY = jarEl.dataset.rain === '1';

  /* ── colours: canvas can't read CSS variables, so we do ──── */
  let C = {};
  const parseColor = (s) => {
    s = (s || '').trim();
    if (s[0] === '#') {
      let h = s.slice(1);
      if (h.length === 3) h = h.split('').map((c) => c + c).join('');
      return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16), 1];
    }
    const m = s.match(/[\d.]+/g);
    return m ? [+m[0], +m[1], +m[2], m[3] != null ? +m[3] : 1] : [128, 128, 128, 1];
  };
  const mix = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
  const rgba = (c, a = 1) => `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${a})`;
  function readColors() {
    const cs = getComputedStyle(document.documentElement);
    const g = (n) => parseColor(cs.getPropertyValue(n));
    C = {
      dark: DARK.matches,
      paper: g('--paper'), paper2: g('--paper-2'), slip: g('--slip'), ink: g('--ink'), glow: g('--glow'),
      tint: { home: g('--tint-home'), apart: g('--tint-apart'), local: g('--tint-local'), day_trip: g('--tint-day_trip'), travel: g('--tint-travel') },
    };
  }
  readColors();

  /* ── jar geometry (SVG viewBox units, 240 × 300) ─────────── */
  const W = 240, H = 300;
  const INSIDE = new Path2D('M76 60v10c0 12-10 17-25 21-18 5-25 14-25 31v140c0 15 8 24 24 24h140c16 0 24-9 24-24V122c0-17-7-26-25-31-15-4-25-9-25-21V60z');
  const WALL_L = 27, WALL_R = 213, FLOOR = 275, CORNER_R = 24, CORNER_Y = FLOOR - CORNER_R;
  const MOUTH = { x: 120, y: 50 };
  const SW = 30, SH = 12; // a folded slip
  const CAP = 80;          // slips we simulate; the rest are represented by the counter

  const insetAt = (y) => (y <= CORNER_Y ? 0 : CORNER_R - Math.sqrt(Math.max(0, CORNER_R * CORNER_R - (y - CORNER_Y) ** 2)));

  // deterministic RNG so the pile looks the same every visit
  const rng = (seed) => () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
  const hash = (n) => { n = (n ^ 61) ^ (n >>> 16); n = (n + (n << 3)) >>> 0; n ^= n >>> 4; n = Math.imul(n, 0x27d4eb2d) >>> 0; return (n ^ (n >>> 15)) >>> 0; };

  function makeSlots(n) {
    const r = rng(20261014);
    const slots = [];
    for (let row = 0; slots.length < n; row++) {
      let y = 268 - row * 9.4;
      if (y < 128) y = 128 + r() * 30; // overflow: pile up loosely near the shoulder
      const inset = insetAt(y + 6);
      const left = WALL_L + inset + SW / 2 - 2, right = WALL_R - inset - SW / 2 + 2;
      const count = Math.max(1, Math.floor((right - left) / 27) + 1);
      const step = count > 1 ? (right - left) / (count - 1) : 0;
      const rowSlots = [];
      for (let i = 0; i < count; i++) {
        const shift = row % 2 ? step * .45 : 0;
        let x = (count > 1 ? left + i * step + shift - step * .22 : (left + right) / 2) + (r() - .5) * 7;
        x = Math.min(right, Math.max(left, x));
        const steep = r() < .18;
        rowSlots.push({ x, y: y + (r() - .5) * 4, a: (r() - .5) * (steep ? 1.6 : .7) });
      }
      for (let i = rowSlots.length - 1; i > 0; i--) { const j = (r() * (i + 1)) | 0; [rowSlots[i], rowSlots[j]] = [rowSlots[j], rowSlots[i]]; }
      slots.push(...rowSlots);
    }
    return slots.slice(0, n);
  }

  /* ── the simulation ──────────────────────────────────────── */
  const shown = TOTAL > CAP ? [...slips].sort((a, b) => hash(a.id) - hash(b.id)).slice(0, CAP) : slips;
  const parts = shown.map((s, i) => ({
    id: s.id, setting: s.setting, outdoor: !!s.outdoor, rank: hash(s.id * 7 + 3),
    x: 0, y: 0, a: 0, vx: 0, vy: 0, va: 0, tx: 0, ty: 0, ta: 0,
    dim: 0, dimT: 0, delay: 0, k: 120, c: 14, out: false, falling: false, scribble: (hash(s.id) % 3),
  }));
  const slots = makeSlots(parts.length + 1);
  let matchSet = null; // null = everything fits

  function assign(animate) {
    const inJar = parts.filter((p) => !p.out);
    const fits = (p) => !matchSet || matchSet.has(p.id) || (p.subFor != null && matchSet.has(p.subFor));
    // rain likely tonight: cozy (at home) slips float up, outdoor ones sink
    const cozy = (p) => (!RAINY ? 0 : (p.setting === 'home' || p.setting === 'apart') ? 1 : p.outdoor ? -1 : 0);
    inJar.sort((a, b) => (fits(a) - fits(b)) || (cozy(a) - cozy(b)) || (a.rank - b.rank));
    inJar.forEach((p, i) => {
      const s = slots[i];
      const was = p.dimT;
      p.dimT = fits(p) ? 0 : 1;
      const moved = Math.abs(p.tx - s.x) + Math.abs(p.ty - s.y) > .5;
      p.tx = s.x; p.ty = s.y; p.ta = s.a;
      if (!animate || reduced()) {
        if (!p.falling) { p.x = s.x; p.y = s.y; p.a = s.a; p.vx = p.vy = p.va = 0; }
        if (!animate) p.dim = p.dimT;
        return;
      }
      if (moved || was !== p.dimT) {
        const sinking = p.dimT > was;
        p.k = sinking ? 70 : 130; p.c = sinking ? 12 : 13;
        p.delay = Math.random() * .16 + (sinking ? 0 : .06);
        if (!sinking && moved) { p.vy -= 40 + Math.random() * 40; p.va += (Math.random() - .5) * 4; }
      }
    });
    kick();
  }

  function jostle(strength = 1) {
    if (reduced()) return;
    for (const p of parts) {
      if (p.out) continue;
      const lift = 1 - (p.y - 120) / 160; // the top of the pile moves most
      p.vx += (Math.random() - .5) * 90 * strength;
      p.vy -= (30 + Math.random() * 110 * lift) * strength;
      p.va += (Math.random() - .5) * 6 * strength;
      p.delay = 0;
    }
    kick();
  }

  // lift a slip out of the jar: the drawn one if we simulate it, else the top slip that fits
  function takeOut(id, setting) {
    let p = parts.find((q) => !q.out && (q.id === id || q.subFor === id));
    if (!p) {
      const fit = parts.filter((q) => !q.out && q.dimT === 0);
      p = (fit.length ? fit : parts.filter((q) => !q.out)).sort((a, b) => a.y - b.y)[0];
      if (p) { p.subFor = id; p.setting = setting || p.setting; }
    }
    if (!p) return null;
    p.out = true;
    const at = { x: p.x, y: p.y, a: p.a, setting: p.setting };
    assign(true);
    return at;
  }

  function dropIn(id) {
    const p = parts.find((q) => q.out && (q.id === id || q.subFor === id)) || parts.find((q) => q.out);
    if (!p) return;
    p.out = false;
    if (p.subFor != null && p.id !== id) { const orig = shown.find((s) => s.id === p.id); if (orig) p.setting = orig.setting; }
    p.subFor = null;
    p.x = MOUTH.x + (Math.random() - .5) * 20; p.y = MOUTH.y + 6; p.vx = (Math.random() - .5) * 30; p.vy = 40; p.a = (Math.random() - .5); p.va = (Math.random() - .5) * 8;
    p.falling = !reduced();
    assign(true);
  }

  /* ── loop: runs only while something moves and the tab is visible ── */
  let raf = 0, last = 0, scale = 1;
  function kick() {
    if (raf || document.hidden) return;
    last = performance.now();
    raf = requestAnimationFrame(frame);
  }
  function frame(now) {
    raf = 0;
    const dt = Math.min(.034, (now - last) / 1000);
    last = now;
    let busyParts = false;
    const dimRate = reduced() ? dt / .15 : dt * 5;
    for (const p of parts) {
      if (p.out) continue;
      if (Math.abs(p.dimT - p.dim) > .002) { p.dim += Math.sign(p.dimT - p.dim) * Math.min(Math.abs(p.dimT - p.dim), dimRate); busyParts = true; } else p.dim = p.dimT;
      if (reduced()) { p.x = p.tx; p.y = p.ty; p.a = p.ta; continue; }
      if (p.delay > 0) { p.delay -= dt; busyParts = true; continue; }
      if (p.falling) {
        p.vy += 900 * dt;
        p.x += (p.tx - p.x) * Math.min(1, dt * 7);
        p.y += p.vy * dt;
        p.a += p.va * dt;
        if (p.y >= p.ty) { p.y = p.ty; p.vy *= -.28; if (Math.abs(p.vy) < 40) p.falling = false; }
        busyParts = true;
        continue;
      }
      const ax = p.k * (p.tx - p.x) - p.c * p.vx;
      const ay = p.k * (p.ty - p.y) - p.c * p.vy;
      const aa = 90 * (p.ta - p.a) - 10 * p.va;
      p.vx += ax * dt; p.vy += ay * dt; p.va += aa * dt;
      p.x += p.vx * dt; p.y += p.vy * dt; p.a += p.va * dt;
      if (p.y > FLOOR - 4) { p.y = FLOOR - 4; p.vy = Math.min(0, p.vy); }
      if (Math.abs(p.vx) + Math.abs(p.vy) > 1.5 || Math.abs(p.va) > .05 || Math.abs(p.tx - p.x) + Math.abs(p.ty - p.y) > .3) busyParts = true;
      else { p.x = p.tx; p.y = p.ty; p.vx = p.vy = 0; }
    }
    render();
    if (busyParts) kick();
  }
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) { cancelAnimationFrame(raf); raf = 0; } else kick();
  });

  function size() {
    const r = jarEl.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5);
    canvas.width = Math.round(r.width * dpr);
    canvas.height = Math.round(r.height * dpr);
    scale = (r.width * dpr) / W;
    render();
  }

  function drawSlip(p) {
    const t = C.tint[p.setting] || C.tint.local;
    const grey = C.dark ? mix(t, C.paper, .62) : mix(mix(t, C.paper2, .82), C.ink, .04);
    let body = mix(t, grey, p.dim);
    if (C.dark) body = mix(body, [255, 200, 130], .2 * (1 - p.dim)); // slips catch the lantern light
    const flap = mix(body, C.dark ? C.ink : C.slip, (C.dark ? .12 : .5) * (1 - p.dim * .6));
    const alpha = 1 - .3 * p.dim;
    ctx.save();
    ctx.translate(p.x, p.y);
    ctx.rotate(p.a);
    ctx.beginPath();
    ctx.roundRect(-SW / 2, -SH / 2, SW, SH, 1.6);
    ctx.fillStyle = rgba(body, alpha);
    ctx.fill();
    ctx.fillStyle = rgba(flap, alpha);
    ctx.fillRect(2, -SH / 2 + .3, SW / 2 - 2.3, SH - .6);
    ctx.lineWidth = .7;
    ctx.strokeStyle = rgba(C.ink, (C.dark ? .2 : .26) * (1 - p.dim * .5));
    ctx.stroke();
    ctx.beginPath(); ctx.moveTo(2, -SH / 2); ctx.lineTo(2, SH / 2); ctx.stroke();
    if (p.dim < .6) { // ink showing through the paper
      ctx.strokeStyle = rgba(C.ink, .3 * (1 - p.dim / .6));
      ctx.lineWidth = .8; ctx.lineCap = 'round';
      ctx.beginPath();
      ctx.moveTo(-11.5, -2); ctx.quadraticCurveTo(-7, -3 + p.scribble * .4, -1.5, -2.2);
      ctx.moveTo(-11.5, 1.6); ctx.lineTo(-5 - p.scribble * 1.5, 1.4);
      ctx.stroke();
    }
    ctx.restore();
  }

  function render() {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.save();
    ctx.clip(INSIDE);
    // greyed-out slips sit behind, the ones that fit come forward
    const order = parts.filter((p) => !p.out).sort((a, b) => (a.dim < .5) - (b.dim < .5) || a.ty - b.ty);
    for (const p of order) drawSlip(p);
    ctx.restore();
  }

  // the jar in page coordinates (for the slips that leave it)
  const jarToPage = (x, y) => {
    const r = jarEl.getBoundingClientRect();
    const s = r.width / W;
    return { x: r.left + scrollX + x * s, y: r.top + scrollY + y * s, s };
  };

  // initial: slips settle in with a small drop, top ones last
  parts.forEach((p) => { p.dim = 0; });
  assign(false);
  if (!reduced()) {
    for (const p of parts) { p.y -= 10 + Math.random() * 14; p.falling = true; p.vy = 0; p.delay = (270 - p.ty) / 260 * .35 + Math.random() * .08; }
  }
  new ResizeObserver(size).observe(jarEl);
  size();
  kick();
  DARK.addEventListener('change', () => { readColors(); render(); });
  if (document.fonts) document.fonts.ready.then(() => sizeBlanks());

  // a tap on the glass jostles the slips (decorative)
  jarEl.addEventListener('pointerdown', () => {
    if (busy || reduced()) return;
    jarEl.classList.remove('is-nudged'); void jarEl.offsetWidth; jarEl.classList.add('is-nudged');
    jostle(.55);
  });

  /* ── the tonight sentence + live match ───────────────────── */
  const tagN = $('#tag-n'), tagUnit = $('#tag-unit'), tagText = $('#tag-text'), tag = $('#tag');
  const fieldSizing = CSS.supports && CSS.supports('field-sizing', 'content');
  const measurer = document.createElement('canvas').getContext('2d');

  function sizeBlanks() {
    if (!form) return;
    form.querySelectorAll('.blank select').forEach((s) => s.classList.toggle('is-any', s.value === ''));
    if (fieldSizing) return;
    form.querySelectorAll('.blank select, .blank input').forEach((el) => {
      const cs = getComputedStyle(el);
      measurer.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const text = el.tagName === 'SELECT' ? (el.selectedOptions[0] || {}).text || '' : (el.value || el.placeholder);
      const pad = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
      el.style.width = Math.ceil(measurer.measureText(text).width + pad + 4) + 'px';
    });
  }

  let seq = 0, matchTimer = 0, lastCount = TOTAL;
  function scheduleMatch(ms = 200) { clearTimeout(matchTimer); matchTimer = setTimeout(runMatch, ms); }
  async function runMatch() {
    if (!form) return;
    const mine = ++seq;
    try {
      const res = await fetch('/match', { method: 'POST', body: new URLSearchParams(new FormData(form)), credentials: 'same-origin', headers: { Accept: 'application/json' } });
      if (!res.ok) return;
      const j = await res.json();
      if (mine !== seq) return;
      applyMatch(j);
    } catch (_) { /* session expired or offline: the jar just stays as it is */ }
  }
  function applyMatch(j) {
    matchSet = new Set(j.ids);
    document.documentElement.style.setProperty('--lit', j.total ? (j.count / j.total).toFixed(3) : 1);
    assign(true);
    if (j.count !== lastCount) {
      tagN.textContent = j.count;
      tagUnit.textContent = j.count === 1 ? 'slip' : 'slips';
      tagText.textContent = j.count === 0 ? 'fit tonight, yet' : 'could happen tonight';
      if (!reduced()) {
        tag.animate([{ transform: 'rotate(4deg)' }, { transform: 'rotate(10deg)' }, { transform: 'rotate(1deg)' }, { transform: 'rotate(5deg)' }, { transform: 'rotate(4deg)' }], { duration: 700, easing: 'ease-out' });
        tagN.animate([{ transform: 'translateY(-6px)', opacity: 0 }, { transform: 'none', opacity: 1 }], { duration: 220, easing: 'cubic-bezier(.2,.8,.2,1)' });
      }
      lastCount = j.count;
    }
  }
  if (form) {
    form.addEventListener('input', () => { sizeBlanks(); scheduleMatch(); });
    form.addEventListener('change', () => { sizeBlanks(); scheduleMatch(60); });
    sizeBlanks();
    // the browser may restore filters on back/forward: reflect them
    if ([...new FormData(form).values()].some((v) => v !== '')) scheduleMatch(0);
  }

  /* ── choreography: draw, put back, stamp ─────────────────── */
  let busy = false, mode = null, swallowUntil = 0, lastWasPutback = false;
  let anims = [], pending = [];
  const track = (a) => { anims.push(a); return a; };
  const later = (fn, ms) => { const p = { fn, t: setTimeout(() => { pending = pending.filter((x) => x !== p); fn(); }, ms) }; pending.push(p); };
  const wait = (ms) => new Promise((r) => later(r, ms));

  // a tap or a key skips any hero animation to its end state
  function skip() {
    for (let i = 0; i < 3 && (anims.length || pending.length); i++) {
      const ps = pending; pending = [];
      ps.forEach((p) => { clearTimeout(p.t); p.fn(); });
      const as = anims; anims = [];
      as.forEach((a) => { try { a.finish(); } catch (_) { a.cancel(); } });
    }
  }
  const animating = () => anims.some((a) => a.playState === 'running' || a.pending) || pending.length > 0;
  addEventListener('pointerdown', (e) => {
    if (busy && animating()) { skip(); swallowUntil = performance.now() + 700; }
  }, true);
  addEventListener('click', (e) => {
    if (performance.now() < swallowUntil) { e.preventDefault(); e.stopPropagation(); swallowUntil = 0; }
  }, true);
  addEventListener('keydown', (e) => {
    if (busy && animating() && !e.metaKey && !e.ctrlKey) {
      skip();
      if (e.key === 'Enter' || e.key === ' ') e.preventDefault();
    }
  }, true);

  const finished = (a) => a.finished.catch(() => {});

  function openLid(open) { jarEl.classList.toggle('is-open', open && !reduced()); }

  function shake() {
    if (reduced()) return;
    jarEl.classList.remove('is-shaking', 'is-nudged');
    void jarEl.offsetWidth;
    jarEl.classList.add('is-shaking');
    setTimeout(() => jarEl.classList.remove('is-shaking'), 400);
    jostle(1);
    setTimeout(() => jostle(.5), 170);
  }

  function hold() {
    pick.style.setProperty('--hold', pick.offsetHeight + 'px');
    pick.classList.add('is-holding');
  }
  function release() { pick.classList.remove('is-holding'); pick.style.removeProperty('--hold'); }

  // the slip folds and drops back into the jar
  function foldAway(old) {
    if (!old) return Promise.resolve();
    const r = old.getBoundingClientRect();
    const id = +old.dataset.id || null;
    const g = old.cloneNode(true);
    g.querySelectorAll('[id]').forEach((n) => n.removeAttribute('id'));
    g.removeAttribute('aria-labelledby');
    g.classList.add('ghost');
    g.setAttribute('aria-hidden', 'true');
    g.inert = true;
    Object.assign(g.style, { position: 'absolute', left: r.left + scrollX + 'px', top: r.top + scrollY + 'px', width: r.width + 'px', height: r.height + 'px' });
    document.body.appendChild(g);
    old.classList.add('is-leaving');
    old.inert = true;
    const done = () => { g.remove(); if (id) dropIn(id); };
    if (reduced()) return finished(track(g.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, fill: 'forwards' }))).then(done);
    const cover = document.createElement('div'); cover.className = 'slip-cover'; g.appendChild(cover);
    const m = jarToPage(MOUTH.x, MOUTH.y);
    const dx = m.x - (r.left + scrollX + r.width / 2);
    const dy = m.y - (r.top + scrollY);
    const base = getComputedStyle(old).transform;
    const start = base && base !== 'none' ? base : 'none';
    track(cover.animate([{ opacity: 0 }, { opacity: 1, offset: .35 }, { opacity: 1 }], { duration: 480, fill: 'forwards' }));
    const a = track(g.animate([
      { transform: start, opacity: 1 },
      { transform: 'translateY(-6px) scale(1, .07) rotate(-1.5deg)', offset: .38, opacity: 1 },
      { transform: `translate(${dx}px, ${dy - 46}px) scale(${34 / r.width}, ${14 / r.height}) rotate(10deg)`, offset: .78, opacity: 1 },
      { transform: `translate(${dx}px, ${dy + 14}px) scale(${30 / r.width}, ${12 / r.height}) rotate(24deg)`, opacity: 0 },
    ], { duration: 480, easing: 'cubic-bezier(.45,0,.3,1)', fill: 'forwards' }));
    later(() => jostle(.35), 420);
    return finished(a).then(done);
  }

  // one slip rises out of the mouth and unfolds into the card
  function arrive(el) {
    el.classList.add('is-arriving');
    pick.setAttribute('aria-busy', 'true');
    // put back: the peek you just used empties in front of you
    const used = lastWasPutback ? el.querySelector('.peek:not(.is-full)') : null;
    if (used) used.classList.add('is-full');
    const cover = document.createElement('div'); cover.className = 'slip-cover'; el.appendChild(cover);
    const title = el.querySelector('.slip-title');
    const end = () => {
      cover.remove();
      el.classList.remove('is-arriving');
      if (used) requestAnimationFrame(() => used.classList.remove('is-full'));
      openLid(false);
      unlock();
      if (title) title.focus({ preventScroll: true });
    };
    if (reduced()) {
      takeOut(+el.dataset.id, el.dataset.setting);
      track(cover.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, fill: 'forwards' }));
      return finished(track(el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150 }))).then(end);
    }
    const from = takeOut(+el.dataset.id, el.dataset.setting) || { x: MOUTH.x, y: MOUTH.y + 30, a: 0, setting: el.dataset.setting };
    const p0 = jarToPage(from.x, from.y), pm = jarToPage(MOUTH.x, MOUTH.y);
    const r = el.getBoundingClientRect();
    const cx = r.left + scrollX + r.width / 2, cy = r.top + scrollY + 10;
    // the region grows as the slip unfolds, so the sentence below glides instead of jumping
    const held = pick.classList.contains('is-holding') ? parseFloat(pick.style.getPropertyValue('--hold')) || 0 : 0;
    release();
    const overlap = -parseFloat(getComputedStyle(pick).marginTop) || 0;
    const startH = held || Math.max(0, overlap - 14);
    const endH = pick.offsetHeight;
    const fl = document.createElement('div');
    fl.className = 'flyer';
    fl.style.setProperty('--tint', `var(--tint-${from.setting || 'local'})`);
    fl.style.position = 'absolute';
    document.body.appendChild(fl);
    const at = (x, y, rot, s) => `translate(${x - 17}px, ${y - 7}px) rotate(${rot}rad) scale(${s})`;
    const k = p0.s * SW / 34;
    track(fl.animate([
      { transform: at(p0.x, p0.y, from.a, k) },
      { transform: at(pm.x, pm.y + 6, -.15, k * 1.05), offset: .34 },
      { transform: at(pm.x + 6, pm.y - 46 * pm.s, -.3, k * 1.7), offset: .58 },
      { transform: at(cx, cy, 0, Math.min(3, r.width / 34 * .3)), opacity: 1, offset: .92 },
      { transform: at(cx, cy, 0, Math.min(3, r.width / 34 * .3)), opacity: 0 },
    ], { duration: 560, easing: 'cubic-bezier(.33,.7,.25,1)', fill: 'forwards' }));
    later(() => fl.remove(), 620);
    const delay = 440;
    track(pick.animate([{ height: startH + 'px' }, { height: startH + 'px', offset: delay / (delay + 480), easing: 'cubic-bezier(.3,.7,.2,1)' }, { height: endH + 'px' }], { duration: delay + 480 }));
    track(cover.animate([{ opacity: 1 }, { opacity: 1, offset: .5 }, { opacity: 0 }], { duration: 480, delay, fill: 'both' }));
    const a = track(el.animate([
      { transform: 'translateY(-4px) scale(.3, .05) rotate(-3deg)', opacity: 0 },
      { opacity: 1, offset: .08 },
      { transform: 'translateY(-4px) scale(1, .06) rotate(-1.6deg)', offset: .38 },
      { transform: 'perspective(1200px) rotateX(-28deg) scale(1, .7) rotate(-1deg)', offset: .66 },
      { transform: 'rotate(-.8deg)', opacity: 1 },
    ], { duration: 480, delay, easing: 'cubic-bezier(.2,.8,.2,1)', fill: 'backwards' }));
    return finished(a).then(() => { fl.remove(); end(); });
  }

  function noteArrives(el) {
    openLid(false);
    const end = () => unlock();
    if (reduced()) return finished(track(el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150 }))).then(end);
    finished(track(el.animate([
      { transform: 'translateY(-14px) rotate(-2deg)', opacity: 0 },
      { transform: 'rotate(1.2deg)', opacity: 1 },
    ], { duration: 420, easing: 'cubic-bezier(.34,1.56,.64,1)' }))).then(end);
  }

  const HEART = '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 14.2C4.6 11.6 1.7 9.2 1.8 6.1c.1-2.2 1.7-3.6 3.5-3.5 1.2 0 2.1.7 2.7 1.6.6-.9 1.6-1.7 2.9-1.6 1.9.1 3.4 1.6 3.3 3.7-.1 3-3.1 5.4-6.2 7.9z"/></svg>';
  function stampIt(el) {
    const stamp = el.querySelector('.stamp');
    const buzz = () => { try { navigator.vibrate && navigator.vibrate(12); } catch (_) {} };
    const end = () => unlock();
    if (!stamp) return end();
    pick.setAttribute('aria-busy', 'true');
    if (reduced()) { buzz(); return finished(track(stamp.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150 }))).then(end); }
    el.classList.add('is-arriving');
    const a = track(stamp.animate([
      { transform: 'translate(14px, -46px) scale(1.9) rotate(-17deg)', opacity: 0 },
      { opacity: .85, offset: .3 },
      { transform: 'translate(0, 0) scale(.93) rotate(-9deg)', opacity: 1, offset: .5, easing: 'cubic-bezier(.34,1.56,.64,1)' },
      { transform: 'rotate(-9deg)', opacity: 1 },
    ], { duration: 640, easing: 'cubic-bezier(.6,0,.9,.45)' }));
    later(() => {
      buzz();
      track(el.animate([
        { transform: 'rotate(-.8deg)' },
        { transform: 'translateY(4px) rotate(-.15deg) scale(.994)', offset: .22 },
        { transform: 'translateY(-1px) rotate(-1deg)', offset: .6 },
        { transform: 'rotate(-.8deg)' },
      ], { duration: 420, easing: 'ease-out' }));
      const r = stamp.getBoundingClientRect();
      [[-1, .15, -38, -70, -40], [1, .1, 30, -84, 35], [1, .85, 52, -30, 60]].forEach(([side, fy, dx, dy, rot], i) => {
        const h = document.createElement('div');
        h.className = 'heart'; h.innerHTML = HEART;
        h.style.position = 'absolute';
        const x = r.left + scrollX + (side < 0 ? 4 : r.width - 18), y = r.top + scrollY + r.height * fy;
        h.style.left = x + 'px'; h.style.top = y + 'px';
        document.body.appendChild(h);
        const ha = track(h.animate([
          { transform: 'scale(.3)', opacity: 0 },
          { transform: `translate(${dx * .4}px, ${dy * .4}px) scale(${i === 2 ? .7 : 1}) rotate(${rot * .4}deg)`, opacity: 1, offset: .25 },
          { transform: `translate(${dx}px, ${dy}px) scale(${i === 2 ? .5 : .8}) rotate(${rot}deg)`, opacity: 0 },
        ], { duration: 820, delay: i * 40, easing: 'cubic-bezier(.2,.8,.2,1)', fill: 'both' }));
        finished(ha).then(() => h.remove());
      });
    }, 320);
    finished(a).then(() => wait(420)).then(() => { el.classList.remove('is-arriving'); end(); });
  }

  function unlock() {
    busy = false; mode = null; anims = []; release();
    pick.setAttribute('aria-busy', 'false');
    if (drawBtn) drawBtn.disabled = false;
  }

  function errorNote() {
    pick.innerHTML = '<article class="note" data-kind="note"><p class="note-text">The lid stuck for a second. Try that again?</p></article>';
    openLid(false);
    unlock();
  }

  // relax: write the loosened filters back into the sentence so it tells the truth
  function applyRelax(relaxForm) {
    if (!form) return;
    const vals = {}, settings = [];
    relaxForm.querySelectorAll('input[type=hidden]').forEach((i) => { if (i.name === 'settings') settings.push(i.value); else vals[i.name] = i.value; });
    for (const [k, v] of Object.entries(vals)) { const el = form.elements[k]; if (el && 'value' in el && !(el instanceof RadioNodeList)) el.value = v; }
    form.querySelectorAll('input[name=settings]').forEach((c) => { c.checked = settings.includes(c.value); });
    sizeBlanks();
    scheduleMatch(0);
  }

  document.addEventListener('htmx:beforeRequest', (e) => {
    const elt = e.detail.elt;
    // only requests that replace the drawn slip get the choreography (not "when is it?")
    if (e.detail.target !== pick) return;
    if (!elt || !elt.closest || !(elt.closest('#tonight') || elt.closest('#pick'))) return;
    if (busy) { e.preventDefault(); return; }
    busy = true;
    if (elt.classList.contains('f-accept')) { mode = 'accept'; return; }
    mode = 'draw';
    lastWasPutback = elt.classList.contains('f-putback');
    if (drawBtn) drawBtn.disabled = true;
    if (elt.classList.contains('f-relax')) applyRelax(elt);
    const stageTop = jarEl.getBoundingClientRect().top;
    if (stageTop < 40 || stageTop > innerHeight * .5) scrollTo({ top: Math.max(0, scrollY + stageTop - 72), behavior: reduced() ? 'auto' : 'smooth' });
    const old = pick.firstElementChild;
    if (old) { hold(); foldAway(old); }
    openLid(true);
    shake();
  });

  document.addEventListener('htmx:beforeSwap', (e) => {
    if (e.detail.target !== pick) return;
    if (reduced()) e.detail.swapOverride = 'innerHTML';
  });

  document.addEventListener('htmx:afterSwap', (e) => {
    if (e.detail.target !== pick) return;
    // a ghost still on its way back into the jar shouldn't block the new slip
    const el = pick.firstElementChild;
    const kind = el && el.dataset.kind;
    if (kind === 'card' && mode === 'draw') arrive(el);
    else if (kind === 'card') unlock();
    else if (kind === 'done') stampIt(el);
    else if (kind === 'note') noteArrives(el);
    else unlock();
  });

  const fail = (e) => { if (e.detail && e.detail.target === pick) errorNote(); };
  document.addEventListener('htmx:responseError', fail);
  document.addEventListener('htmx:sendError', fail);

  // "Back to the jar" after a stamped date: the slip goes back in
  document.addEventListener('click', (e) => {
    const b = e.target.closest && e.target.closest('[data-action="back"]');
    if (!b || busy) return;
    busy = true;
    const old = pick.firstElementChild;
    hold();
    openLid(true);
    foldAway(old).then(() => {
      pick.innerHTML = '';
      openLid(false);
      unlock();
      if (drawBtn) drawBtn.focus({ preventScroll: true });
    });
  });

  /* ── shake your phone to draw (progressive extra) ────────── */
  const shakeBtn = $('#shake');
  // iOS only delivers motion events on secure origins (https or localhost).
  if (shakeBtn && form && window.isSecureContext && 'DeviceMotionEvent' in window && matchMedia('(pointer: coarse)').matches) {
    shakeBtn.hidden = false;
    let on = false, hits = [], lastDraw = 0, lastJig = 0;
    const onMotion = (ev) => {
      const a = ev.acceleration && ev.acceleration.x != null ? ev.acceleration : null;
      const g = ev.accelerationIncludingGravity;
      let mag = 0;
      if (a) mag = Math.hypot(a.x || 0, a.y || 0, a.z || 0);
      else if (g) mag = Math.abs(Math.hypot(g.x || 0, g.y || 0, g.z || 0) - 9.81);
      const now = performance.now();
      if (mag > 5 && now - lastJig > 140) { lastJig = now; jostle(Math.min(1, mag / 20)); }
      if (mag > 14) {
        hits = hits.filter((t) => now - t < 700); hits.push(now);
        if (hits.length >= 3 && !busy && now - lastDraw > 2000) { hits = []; lastDraw = now; form.requestSubmit(drawBtn); }
      }
    };
    const enable = () => {
      on = true; addEventListener('devicemotion', onMotion);
      shakeBtn.setAttribute('aria-pressed', 'true'); shakeBtn.textContent = 'shake to draw is on';
      try { localStorage.setItem('joy.shake', '1'); } catch (_) {}
    };
    const disable = () => {
      on = false; removeEventListener('devicemotion', onMotion);
      shakeBtn.setAttribute('aria-pressed', 'false'); shakeBtn.textContent = 'or shake your phone';
      try { localStorage.removeItem('joy.shake'); } catch (_) {}
    };
    shakeBtn.addEventListener('click', async () => {
      if (on) return disable();
      try {
        if (typeof DeviceMotionEvent.requestPermission === 'function') {
          const r = await DeviceMotionEvent.requestPermission();
          if (r !== 'granted') { shakeBtn.textContent = 'motion is off, tap Draw instead'; return; }
        }
        enable();
      } catch (_) { shakeBtn.hidden = true; }
    });
    let saved = false;
    try { saved = localStorage.getItem('joy.shake') === '1'; } catch (_) {}
    if (saved && typeof DeviceMotionEvent.requestPermission !== 'function') enable();
  }
})();

/* ── v3: sharing and "when is it?" (independent of the jar) ── */
(() => {
  'use strict';
  const canShareFiles = (f) => { try { return !!(navigator.canShare && navigator.canShare({ files: [f] })); } catch (_) { return false; } };

  // "Send to Neha": the native share sheet when there is one, else the wa.me link does its job
  document.addEventListener('click', (e) => {
    const a = e.target.closest && e.target.closest('a[data-share]');
    if (!a || !navigator.share) return;
    e.preventDefault();
    navigator.share({ text: a.dataset.share }).catch((err) => {
      if (err && err.name === 'AbortError') return; // they changed their mind
      window.open(a.href, '_blank', 'noopener');
    });
  });

  // the invite is fetched as soon as it exists, so sharing stays inside the tap's user gesture (Safari)
  const icsCache = new Map();
  function prepIcs(root) {
    const btn = root && root.querySelector && root.querySelector('[data-share-ics]');
    if (!btn || !navigator.share || !navigator.canShare) return;
    if (!canShareFiles(new File(['x'], 'joy-date.ics', { type: 'text/calendar' }))) return;
    const url = btn.dataset.shareIcs;
    fetch(url, { credentials: 'same-origin' })
      .then((r) => (r.ok ? r.blob() : Promise.reject(r.status)))
      .then((b) => {
        const f = new File([b], 'joy-date.ics', { type: 'text/calendar' });
        if (!canShareFiles(f)) return;
        icsCache.set(url, f);
        btn.hidden = false;
      })
      .catch(() => {});
  }
  document.addEventListener('click', (e) => {
    const a = e.target.closest && e.target.closest('[data-share-ics]');
    if (!a) return;
    const f = icsCache.get(a.dataset.shareIcs);
    if (!f) return; // falls back to the download link
    e.preventDefault();
    const when = (document.querySelector('.planned-label strong') || {}).textContent || '';
    navigator.share({ files: [f], title: 'Our date', text: when ? `Our date: ${when}` : 'Our date' }).catch((err) => {
      if (err && err.name === 'AbortError') return;
      location.href = a.href;
    });
  });

  document.addEventListener('htmx:afterSettle', () => {
    const p = document.getElementById('planned');
    if (p && p.classList.contains('is-set') && !p.dataset.prepped) {
      p.dataset.prepped = '1';
      p.classList.add('is-fresh');
      // the Save button that had focus is gone: land on the result instead of <body>
      const lbl = p.querySelector('.planned-label');
      if (lbl) { lbl.tabIndex = -1; lbl.focus({ preventScroll: true }); }
      prepIcs(p);
    }
  });

  // a 422 from /history/when ("That date and time didn't look right.") isn't swapped: say it inline
  document.addEventListener('htmx:responseError', (e) => {
    const t = e.detail && e.detail.target;
    if (!t || t.id !== 'planned') return;
    const err = e.detail.elt && e.detail.elt.querySelector('.when-err');
    if (err) err.textContent = (e.detail.xhr && e.detail.xhr.status === 422 && e.detail.xhr.responseText) || 'That didn\'t save. Try again?';
  });
  document.addEventListener('htmx:sendError', (e) => {
    const t = e.detail && e.detail.target;
    if (!t || t.id !== 'planned') return;
    const err = e.detail.elt && e.detail.elt.querySelector('.when-err');
    if (err) err.textContent = 'No connection. Try again in a moment.';
  });
})();
