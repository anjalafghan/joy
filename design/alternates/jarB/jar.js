// joy — the Jar. The sentence is the interface; the jar listens.
// No rAF loops: motion is CSS + Web Animations, which the browser idles for us.
(() => {
  "use strict";

  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const stage = $("#stage");
  if (!stage) return;
  const form = $("#tonight");
  const pick = $("#pick");
  const jarWrap = $(".jar-wrap");
  const jarSvg = $("#jar");
  const slipLayer = $("#jar-slips");
  const RM = matchMedia("(prefers-reduced-motion: reduce)");
  const reduced = () => RM.matches;
  const currency = stage.dataset.currency || "";
  const sleepers = new Set();
  const sleep = (ms) => new Promise((r) => {
    if (skipping) return r();
    const t = setTimeout(() => { sleepers.delete(done); r(); }, ms);
    const done = () => { clearTimeout(t); r(); };
    sleepers.add(done);
  });

  /* ── slips data (with ?fake=N to preview 0/3/200) ───────── */
  let slips = [];
  try { slips = JSON.parse($("#slips").textContent || "[]"); } catch (_) { slips = []; }
  const fake = new URLSearchParams(location.search).get("fake");
  if (fake !== null) {
    const n = Math.max(0, Math.min(500, +fake || 0));
    const keys = ["home", "local", "day_trip", "travel"];
    slips = Array.from({ length: n }, (_, i) => ({ id: 100000 + i, title: "slip " + i, setting: keys[i % 4] }));
  }
  const total = slips.length;

  /* ── words ──────────────────────────────────────────────── */
  const money = (v) => {
    const n = +v;
    if (n === 0) return "nothing to spend";
    return currency + n.toLocaleString(currency === "₹" ? "en-IN" : "en-US");
  };
  const WHERE = { home: "at home", local: "around town", day_trip: "on a day trip", travel: "travelling" };
  const listJoin = (a) => a.length <= 1 ? (a[0] || "") : a.slice(0, -1).join(", ") + " or " + a[a.length - 1];

  // "cozy" → "a cozy", "outdoors" → "an outdoors"  (reads: "in a cozy mood")
  $$('select[name="tag"] option').forEach((o) => {
    if (o.value) o.textContent = (/^[aeiou]/i.test(o.value) ? "an " : "a ") + o.textContent;
  });

  const wordOf = (blank) => {
    if (blank.dataset.blank === "where") {
      const on = $$('input[name="settings"]:checked', form);
      if (!on.length || on.length === 4) return { text: on.length ? "anywhere at all" : "anywhere", any: !on.length };
      return { text: listJoin(on.map((c) => WHERE[c.value] || c.dataset.label.toLowerCase())), any: false };
    }
    const sel = $("select", blank);
    const opt = sel.options[sel.selectedIndex];
    return { text: opt ? opt.textContent : "", any: !sel.value };
  };

  const fromClause = $("#from-clause");
  function paint(changed) {
    $$(".blank", form).forEach((b) => {
      const w = wordOf(b);
      const word = $(".word", b);
      if (word.textContent !== w.text) word.textContent = w.text;
      if (b.dataset.blank !== "from") b.classList.toggle("is-any", w.any);
      if (changed && changed.includes(b)) nod(word);
    });
    const km = $('select[name="max_km"]', form);
    if (fromClause && km) fromClause.hidden = !km.value;
  }
  function nod(el) {
    el.classList.remove("wiggle");
    void el.offsetWidth;
    el.classList.add("wiggle");
  }

  /* ── where sheet ────────────────────────────────────────── */
  const whereBtn = $("#where-toggle");
  const sheet = $("#where-sheet");
  sheet.hidden = true;
  const setSheet = (open) => {
    sheet.hidden = !open;
    whereBtn.setAttribute("aria-expanded", String(open));
    if (open) $("input", sheet).focus({ preventScroll: true });
  };
  whereBtn.addEventListener("click", () => setSheet(sheet.hidden));
  $("#where-done").addEventListener("click", () => { setSheet(false); whereBtn.focus(); });
  sheet.addEventListener("keydown", (e) => { if (e.key === "Escape") { setSheet(false); whereBtn.focus(); } });

  /* ── the jar's slips ────────────────────────────────────── */
  const MAX_VIS = 48;
  const vis = slips.slice(0, MAX_VIS);
  const rand = (i, k) => { const x = Math.sin(i * 127.1 + k * 311.7) * 43758.5453; return x - Math.floor(x); };
  const SVGNS = "http://www.w3.org/2000/svg";
  const slipEls = new Map();
  vis.forEach((s, i) => {
    const row = Math.floor(i / 4), col = i % 4;
    const x = 34 + col * 23 + (row % 2) * 9 + (rand(i, 1) - .5) * 9;
    const y = 186 - row * 8.6 - rand(i, 2) * 3;
    const r = (rand(i, 3) - .5) * 70;
    const g = document.createElementNS(SVGNS, "g");
    g.setAttribute("class", "jslip s-" + s.setting);
    g.style.cssText = `--x:${x.toFixed(1)};--y:${y.toFixed(1)};--r:${r.toFixed(1)};--d:${(i % 12) * 14}ms`;
    g.innerHTML = '<rect x="-12" y="-4.5" width="24" height="9" rx="1"/><path d="M-1-4.5v9"/>';
    slipLayer.appendChild(g);
    slipEls.set(String(s.id), g);
  });

  /* ── live counter: POST /match ──────────────────────────── */
  const tallyN = $("#tally-n"), tallyTotal = $("#tally-total"), tallySr = $("#tally-sr"), tallyHand = $(".tally-hand"), tallyWords = $("#tally-words");
  let ctrl = null, timer = 0;
  function match() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (ctrl) ctrl.abort();
      ctrl = new AbortController();
      try {
        const res = await fetch("/match", {
          method: "POST",
          body: new URLSearchParams(new FormData(form)),
          signal: ctrl.signal,
          headers: { "Content-Type": "application/x-www-form-urlencoded" },
        });
        if (!res.ok) return;
        const m = await res.json();
        applyMatch(fake !== null ? fakeMatch() : m);
      } catch (_) { /* aborted or offline: leave the jar as it was */ }
    }, 140);
  }
  function fakeMatch() {
    const ids = slips.filter((_, i) => rand(i, $$("select", form).map((s) => s.selectedIndex).join("").length + i) > .45).map((s) => s.id);
    return { count: ids.length, total, ids };
  }
  function applyMatch(m) {
    const ok = new Set(m.ids.map(String));
    slipEls.forEach((g, id) => g.classList.toggle("is-out", !ok.has(id)));
    if (tallyN) {
      const before = tallyN.textContent;
      tallyN.textContent = m.count || "none";
      tallyTotal.textContent = m.total;
      tallyHand.classList.toggle("is-none", m.count === 0);
      tallyWords.textContent = m.count === 0 ? "fit… yet" : "could happen";
      const sr = m.count === 0
        ? `None of the ${m.total} slips fit yet. Draw anyway and the jar will suggest what to loosen.`
        : `${m.count} of ${m.total} slip${m.total === 1 ? "" : "s"} could happen tonight.`;
      if (tallySr.textContent !== sr) tallySr.textContent = sr;
      if (before !== String(m.count) && !reduced()) { tallyHand.classList.remove("bump"); void tallyHand.offsetWidth; tallyHand.classList.add("bump"); }
    }
  }

  form.addEventListener("change", (e) => {
    const blank = e.target.closest(".blank") || (e.target.name === "settings" ? whereBtn : null);
    paint(blank ? [blank] : []);
    match();
    if (!busy && pick.firstElementChild) clearCard();
  });

  /* ── hero orchestration ─────────────────────────────────── */
  let busy = false, skipping = false, flow = null;
  const running = new Set();

  function anim(el, frames, opts) {
    const a = el.animate(frames, { fill: "both", easing: "cubic-bezier(.2,.8,.2,1)", ...opts, duration: skipping ? 0 : opts.duration });
    running.add(a);
    a.finished.catch(() => {}).then(() => running.delete(a));
    return a;
  }
  function skip() {
    skipping = true;
    sleepers.forEach((f) => f()); sleepers.clear();
    jarWrap.classList.remove("shake", "nope", "plop");
    running.forEach((a) => { try { a.finish(); } catch (_) {} });
    pick.getAnimations({ subtree: true }).forEach((a) => {
      const t = a.effect && a.effect.getComputedTiming();
      if (t && isFinite(t.endTime)) try { a.finish(); } catch (_) {}
    });
  }
  // While a hero plays: any tap or key skips to the end, and is swallowed (so a skip-tap can't accept).
  const swallow = (e) => {
    if (!busy) return;
    if (e.type === "keydown" && ["Shift", "Control", "Alt", "Meta", "Tab"].includes(e.key)) return;
    if (e.type !== "pointerdown") { e.preventDefault(); e.stopImmediatePropagation(); }
    skip();
  };
  ["pointerdown", "click", "keydown"].forEach((t) => document.addEventListener(t, swallow, true));

  function lock(on) {
    busy = on;
    const d = $(".draw");
    if (d) d.disabled = on;
  }

  const mouth = () => {
    const r = jarSvg.getBoundingClientRect();
    const k = r.width / 160;
    return { x: r.left + scrollX + 80 * k, y: r.top + scrollY + 44 * k, k };
  };

  function shake(cls) {
    if (reduced()) return Promise.resolve();
    jarWrap.classList.remove("shake", "nope", "plop");
    void jarWrap.offsetWidth;
    jarWrap.classList.add(cls);
    const ms = cls === "shake" ? 480 : cls === "nope" ? 420 : 360;
    return sleep(ms).then(() => jarWrap.classList.remove(cls));
  }

  function settingOf(el) { return (el && el.dataset.setting) || "local"; }

  // Clone the slip that's leaving so htmx can swap the real one immediately.
  function detach(card) {
    if (!card) return null;
    const r = card.getBoundingClientRect();
    const c = card.cloneNode(true);
    c.removeAttribute("id");
    $$("[id]", c).forEach((n) => n.removeAttribute("id"));
    c.setAttribute("aria-hidden", "true");
    c.inert = true;
    c.classList.add("ghost-card");
    c.style.cssText += `left:${r.left + scrollX}px;top:${r.top + scrollY}px;width:${r.width}px;height:${r.height}px;`;
    document.body.appendChild(c);
    card.style.visibility = "hidden";
    return c;
  }

  async function foldAway(clone, intoJar) {
    if (!clone) return;
    if (reduced()) {
      await anim(clone, [{ opacity: 1 }, { opacity: 0 }], { duration: 140 }).finished.catch(() => {});
      clone.remove();
      return;
    }
    // fold: the slip creases shut (perspective rotate from the top)
    await anim(clone, [
      { transform: "perspective(900px) rotateX(0)", opacity: 1 },
      { transform: "perspective(900px) rotateX(78deg) scaleY(.9)", opacity: .9 },
    ], { duration: 260, easing: "cubic-bezier(.65,0,.35,1)" }).finished.catch(() => {});
    if (!intoJar) { clone.remove(); return; }
    // …then drops back into the jar as a small slip
    const r = clone.getBoundingClientRect();
    const m = mouth();
    const id = clone.dataset.id;
    clone.remove();
    const g = ghost(settingOf(clone), r.left + scrollX, r.top + scrollY + r.height * .1, r.width, Math.max(22, r.height * .12));
    const dx = m.x - (r.left + scrollX) - 28 * m.k, dy = m.y - (r.top + scrollY);
    const sx = (56 * m.k) / r.width;
    await anim(g, [
      { transform: "translate(0,0) rotate(0) scale(1)", opacity: 1 },
      { transform: `translate(${dx}px, ${dy - 40}px) rotate(-8deg) scale(${sx}, .6)`, opacity: 1, offset: .7 },
      { transform: `translate(${dx}px, ${dy + 34 * m.k}px) rotate(4deg) scale(${sx * .8}, .5)`, opacity: 0 },
    ], { duration: 380, easing: "cubic-bezier(.65,0,.35,1)" }).finished.catch(() => {});
    g.remove();
    const back = id && slipEls.get(id);
    if (back) back.classList.remove("is-drawn");
    shake("plop");
  }

  function ghost(setting, x, y, w, h) {
    const g = document.createElement("div");
    g.className = "ghost";
    g.setAttribute("aria-hidden", "true");
    g.style.cssText = `left:${x}px;top:${y}px;width:${w}px;height:${h}px;--tint:var(--tint-${setting});`;
    document.body.appendChild(g);
    return g;
  }

  function scrollToCard(card) {
    const r = card.getBoundingClientRect();
    const head = ($(".top") || { offsetHeight: 64 }).offsetHeight + 10;
    const needBottom = r.bottom + 16 - innerHeight;           // scroll needed to see its bottom
    const maxDown = r.top - head;                              // never push its top under the header
    const dy = Math.min(Math.max(needBottom, 0), Math.max(maxDown, 0));
    if (dy > 4) scrollBy({ top: dy, behavior: reduced() || skipping ? "auto" : "smooth" });
  }

  // The drawn slip rises from the jar and unfolds into the card.
  async function riseInto(card) {
    if (reduced()) {
      card.style.opacity = "";
      scrollToCard(card);
      await anim(card, [{ opacity: 0 }, { opacity: 1 }], { duration: 150, easing: "linear" }).finished.catch(() => {});
      return;
    }
    const id = card.dataset.id;
    const inJar = id && slipEls.get(id);
    const m = mouth();
    const gw = 56 * m.k, gh = 20 * m.k;
    const g = ghost(settingOf(card), m.x - gw / 2, m.y + 60 * m.k, gw, gh);
    if (inJar) inJar.classList.add("is-drawn");
    // 1 · rises out of the mouth
    await anim(g, [
      { transform: "translateY(0) rotate(12deg)", opacity: 0 },
      { transform: `translateY(${-50 * m.k}px) rotate(-4deg)`, opacity: 1, offset: .35 },
      { transform: `translateY(${-112 * m.k}px) rotate(-7deg)`, opacity: 1 },
    ], { duration: 360, easing: "cubic-bezier(.2,.8,.2,1)" }).finished.catch(() => {});
    // 2 · floats to where the card will be (page scrolls with it)
    card.style.opacity = "0";
    scrollToCard(card);
    const r = card.getBoundingClientRect();
    const tx = r.left + scrollX - (m.x - gw / 2);
    const ty = r.top + scrollY - (m.y + 60 * m.k);
    const sx = r.width / gw, sy = 64 / gh;
    await anim(g, [
      { transform: `translateY(${-112 * m.k}px) rotate(-7deg) scale(1,1)` },
      { transform: `translate(${tx}px, ${ty}px) rotate(0) scale(${sx}, ${sy})` },
    ], { duration: 340, easing: "cubic-bezier(.65,0,.35,1)" }).finished.catch(() => {});
    g.remove();
    // 3 · unfolds
    card.style.opacity = "";
    await anim(card, [
      { transform: "perspective(1000px) rotateX(-84deg)", opacity: .6, offset: 0 },
      { transform: "perspective(1000px) rotateX(10deg)", opacity: 1, offset: .62 },
      { transform: "perspective(1000px) rotateX(0)", opacity: 1 },
    ], { duration: 440, easing: "cubic-bezier(.2,.8,.2,1)" }).finished.catch(() => {});
    running.forEach((a) => { if (a.effect && a.effect.target === card) a.cancel(); });
  }

  async function noteIn(note) {
    note.style.opacity = "";
    scrollToCard(note);
    await anim(note, reduced()
      ? [{ opacity: 0 }, { opacity: 1 }]
      : [{ opacity: 0, transform: "translateY(14px)" }, { opacity: 1, transform: "none" }],
      { duration: reduced() ? 150 : 420, easing: "cubic-bezier(.34,1.56,.64,1)" }).finished.catch(() => {});
  }

  function polishTitle(root) {
    $$(".slip-title-text", root).forEach((t) => {
      const s = t.textContent.trim();
      if (s && !/[.!?…]$/.test(s)) t.textContent = s + ".";
    });
  }

  function clearCard() {
    const card = pick.firstElementChild;
    const id = card && card.dataset.id;
    if (card && card.dataset.kind === "pick" && id && slipEls.get(id)) slipEls.get(id).classList.remove("is-drawn");
    const clone = detach(card);
    pick.innerHTML = "";
    stage.classList.remove("has-card");
    foldAway(clone, false);
  }

  // Relax: write the server's loosened filters back into the sentence, so what you see is what was asked.
  function syncFrom(f) {
    const vals = {};
    $$("input[type=hidden]", f).forEach((i) => { (vals[i.name] = vals[i.name] || []).push(i.value); });
    const changed = [];
    ["budget", "max_duration", "max_km", "from", "tag", "energy"].forEach((name) => {
      const sel = $(`select[name="${name}"]`, form);
      if (!sel) return;
      const v = (vals[name] || [""])[0];
      if (sel.value === v) return;
      if (![...sel.options].some((o) => o.value === v)) {
        const o = new Option(name === "budget" ? money(v) : name === "max_km" ? `within ${v} km` : v, v);
        const after = [...sel.options].find((x) => x.value && +x.value > +v);
        sel.insertBefore(o, after || null);
      }
      sel.value = v;
      changed.push(sel.closest(".blank"));
    });
    const want = new Set((vals.settings || []).filter(Boolean));
    let whereChanged = false;
    $$('input[name="settings"]', form).forEach((c) => {
      if (c.checked !== want.has(c.value)) { c.checked = want.has(c.value); whereChanged = true; }
    });
    if (whereChanged) changed.push(whereBtn);
    paint(changed);
    match();
  }

  /* ── htmx lifecycle ─────────────────────────────────────── */
  const KINDS = ["draw", "putback", "accept", "relax"];
  document.body.addEventListener("htmx:beforeRequest", (e) => {
    const src = e.detail.elt && e.detail.elt.closest("[data-kind]");
    const kind = src && src.dataset.kind;
    if (!KINDS.includes(kind)) return;
    if (busy) { e.preventDefault(); return; }
    lock(true);
    skipping = false;
    const outgoing = pick.firstElementChild;
    flow = { kind, pre: Promise.resolve(), started: performance.now() };
    if (kind === "draw" || kind === "relax") {
      if (kind === "relax") syncFrom(src);
      setSheet(false);
      const clone = detach(outgoing);
      if (outgoing && outgoing.dataset.kind === "pick" && slipEls.get(outgoing.dataset.id)) slipEls.get(outgoing.dataset.id).classList.remove("is-drawn");
      stage.classList.add("has-card");
      flow.pre = Promise.all([foldAway(clone, false), shake("shake")]);
    } else if (kind === "putback") {
      flow.pre = foldAway(detach(outgoing), true);
    }
  });

  document.body.addEventListener("htmx:afterSwap", (e) => {
    if (e.detail.target !== pick || !flow) return;
    const f = flow;
    const el = pick.firstElementChild;
    if (!el) return;
    polishTitle(el);
    const k = el.dataset.kind;
    if (k === "pick" || k === "nomatch" || k === "empty") el.style.opacity = "0"; // revealed by the hero, never display:none (aria-live still reads it)
    (async () => {
      await f.pre;
      if (k === "pick") await riseInto(el);
      else if (k === "nomatch" || k === "empty") { await shake("nope"); await noteIn(el); }
      else if (k === "done") {
        stage.classList.add("has-card");
        scrollToCard(el);
        setTimeout(() => { if ("vibrate" in navigator) try { navigator.vibrate(12); } catch (_) {} }, skipping || reduced() ? 0 : 300);
        await sleep(reduced() ? 160 : 900);
      }
    })().finally(() => {
      el.style.opacity = "";
      flow = null;
      lock(false);
      skipping = false;
      const focusTo = $(".accept, .relax .btn, .note .btn, .again", pick);
      if (focusTo && document.activeElement && (document.activeElement === document.body || !document.contains(document.activeElement) || document.activeElement.closest(".ghost-card"))) focusTo.focus({ preventScroll: true });
    });
  });

  const fail = (e) => {
    if (!flow) return;
    flow = null;
    $$(".ghost, .ghost-card").forEach((n) => n.remove());
    lock(false);
    skipping = false;
    jarWrap.classList.remove("shake");
    const old = pick.firstElementChild;
    if (old) old.style.visibility = "";
    else {
      stage.classList.remove("has-card");
      pick.innerHTML = '<p class="oops">The lid’s stuck. Try that again?</p>';
    }
  };
  document.body.addEventListener("htmx:responseError", fail);
  document.body.addEventListener("htmx:sendError", fail);
  document.body.addEventListener("htmx:afterRequest", (e) => {
    // A request that ended without a swap into #pick (e.g. dropped by hx-sync) must not leave us locked.
    if (flow && !e.detail.successful) fail(e);
  });

  // "Draw another": back to the sentence.
  pick.addEventListener("click", (e) => {
    if (e.target.closest("[data-again]")) {
      clearCard();
      scrollTo({ top: 0, behavior: reduced() ? "auto" : "smooth" });
      const d = $(".draw");
      if (d) d.focus({ preventScroll: true });
    }
  });

  /* ── first paint ────────────────────────────────────────── */
  form.reset();
  paint();
  if (total) match();
  if (fake !== null) {
    applyMatch({ count: total, total, ids: slips.map((s) => s.id) });
  }
})();
