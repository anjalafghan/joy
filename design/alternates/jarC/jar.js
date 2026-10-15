// joy — the Jar, take C. Looking down into the jar's mouth.
// Filters are a sentence; slips that don't fit sink. Drawing lifts the actual
// slip out of the heap toward you and unfolds it along its creases.
(() => {
  "use strict";
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const reduceMQ = matchMedia("(prefers-reduced-motion: reduce)");
  const reduced = () => reduceMQ.matches;

  const form = $("#tonight");
  const stage = $("#stage");
  const jar = $("#jar");
  const heap = $("#heap");
  const pick = $("#pick");
  const counter = $("#counter");
  const drawBtn = $("#draw");
  const mouth = $(".jar-mouth");
  if (!form || !stage || !heap) return;

  /* ── the heap ─────────────────────────────────────────── */
  const els = new Map();
  const hash = (n) => {
    let x = (n * 2654435761) >>> 0;
    x ^= x >>> 15; x = Math.imul(x, 2246822507) >>> 0;
    x ^= x >>> 13; x = Math.imul(x, 3266489909) >>> 0;
    return (x ^ (x >>> 16)) >>> 0;
  };
  const rnd = (id, k) => hash(id * 131 + k * 7919 + 17) / 4294967296;

  function render(list) {
    heap.textContent = "";
    els.clear();
    const n = list.length;
    // slips shrink a little once the jar gets full
    const w = n <= 50 ? 15 : Math.max(9.5, 15 - (n - 50) * 0.037);
    heap.style.setProperty("--w", w + "cqw");
    const order = [...list].sort((a, b) => hash(a.id) - hash(b.id));
    const frag = document.createDocumentFragment();
    // outermost first so the middle of the heap sits on top, like a real pile
    for (let i = n - 1; i >= 0; i--) {
      const s = order[i];
      const t = (i + 0.5) / Math.max(n, 1);
      const rad = Math.sqrt(t) * (n < 6 ? 0.18 : 0.38) + (rnd(s.id, 3) - 0.5) * 0.04;
      const ang = i * 2.39996 + rnd(s.id, 1) * 0.6;
      const x = 50 + Math.cos(ang) * rad * 100;
      const y = 50 + Math.sin(ang) * rad * 100;
      const el = document.createElement("i");
      el.className = "hs s-" + String(s.setting).replace(/[^a-z_]/g, "");
      el.style.setProperty("--x", x.toFixed(2) + "%");
      el.style.setProperty("--y", y.toFixed(2) + "%");
      const r = (rnd(s.id, 2) - 0.5) * 140;
      el.style.setProperty("--r", r.toFixed(1) + "deg");
      // where it settles when it doesn't fit: deeper (toward the middle)
      el.style.setProperty("--kx", (-(x - 50) / 50 * 0.9).toFixed(3));
      el.style.setProperty("--ky", (-(y - 50) / 50 * 0.9 + 0.12).toFixed(3));
      el.dataset.r = r;
      els.set(s.id, el);
      frag.appendChild(el);
    }
    heap.appendChild(frag);
  }

  let slips = [];
  try { slips = JSON.parse(($("#slips") || {}).textContent || "[]"); } catch (_) { slips = []; }
  render(slips);

  /* ── the sentence ─────────────────────────────────────── */
  const measure = document.createElement("canvas").getContext("2d");
  function sizeControl(c) {
    const blank = c.closest(".blank");
    if (c.tagName === "SELECT") {
      const cs = getComputedStyle(c);
      measure.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const text = c.options[c.selectedIndex] ? c.options[c.selectedIndex].text : "";
      c.style.width = Math.ceil(measure.measureText(text).width + 19) + "px";
    } else if (c.type === "number") {
      const len = (c.value || c.placeholder).length;
      c.style.width = Math.max(2, len + (c.value ? 0.6 : 0.2)) + "ch";
    }
    if (blank) blank.classList.toggle("set", c.value !== "");
  }
  const controls = () => $$(".blank select, .blank input", form);
  function syncSentence() {
    controls().forEach(sizeControl);
    const km = form.elements.max_km;
    const from = $("[data-from]", form);
    if (from && km) from.hidden = km.value === "";
  }
  syncSentence();
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(syncSentence);

  /* ── live: which slips could happen tonight ───────────── */
  let matchSeq = 0, matchTimer = 0, lastCount = slips.length;
  function setCounter(count, total) {
    if (!counter) return;
    const b = counter.querySelector("b") || document.createElement("b");
    const text = count === 0 ? " Nothing fits tonight, yet" : count === 1 ? " slip could happen tonight" : " slips could happen tonight";
    if (count !== lastCount && !reduced()) {
      counter.classList.add("tick");
      requestAnimationFrame(() => requestAnimationFrame(() => counter.classList.remove("tick")));
    }
    lastCount = count;
    b.textContent = count === 0 ? "0" : String(count);
    counter.textContent = "";
    counter.append(b, text);
    counter.classList.toggle("none", count === 0);
    counter.title = `${count} of ${total} slips fit`;
  }
  function applyMatch(j) {
    const ok = new Set(j.ids);
    els.forEach((el, id) => el.classList.toggle("out", !ok.has(id)));
    setCounter(j.count, j.total);
  }
  function refresh(delay = 140) {
    clearTimeout(matchTimer);
    matchTimer = setTimeout(async () => {
      const my = ++matchSeq;
      try {
        const r = await fetch("/match", { method: "POST", body: new URLSearchParams(new FormData(form)), credentials: "same-origin" });
        if (!r.ok) return;
        const j = await r.json();
        if (my === matchSeq) applyMatch(j);
      } catch (_) { /* the counter just stays as it was */ }
    }, delay);
  }
  form.addEventListener("input", (e) => { if (e.target.matches(".blank select, .blank input")) sizeControl(e.target); syncSentence(); refresh(); });
  form.addEventListener("change", () => { syncSentence(); refresh(); });
  if (slips.length) refresh(0);

  /* ── reaching in ──────────────────────────────────────── */
  let reach = null, moveFrame = 0;
  const canReach = () => slips.length && !busy && drawBtn && !stage.classList.contains("card-pick");
  function pushAround(px, py) {
    const hr = heap.getBoundingClientRect();
    const R = hr.width * 0.24;
    els.forEach((el) => {
      const ex = hr.left + parseFloat(el.style.getPropertyValue("--x")) / 100 * hr.width;
      const ey = hr.top + parseFloat(el.style.getPropertyValue("--y")) / 100 * hr.height;
      const dx = ex - px, dy = ey - py, d = Math.hypot(dx, dy);
      if (d < R && d > 0.1) {
        const f = (1 - d / R) * hr.width * 0.07;
        el.style.setProperty("--px", (dx / d * f).toFixed(1) + "px");
        el.style.setProperty("--py", (dy / d * f).toFixed(1) + "px");
      } else if (el.style.getPropertyValue("--px")) {
        el.style.removeProperty("--px"); el.style.removeProperty("--py");
      }
    });
  }
  function letGo() {
    reach = null;
    cancelAnimationFrame(moveFrame);
    jar.classList.remove("rummage");
    els.forEach((el) => { el.style.removeProperty("--px"); el.style.removeProperty("--py"); });
  }
  if (mouth) {
    mouth.addEventListener("pointerdown", (e) => {
      if (!canReach() || e.button > 0) return;
      reach = { id: e.pointerId };
      if (!reduced()) { jar.classList.add("rummage"); pushAround(e.clientX, e.clientY); }
    });
    mouth.addEventListener("pointermove", (e) => {
      if (!reach || reach.id !== e.pointerId || reduced()) return;
      cancelAnimationFrame(moveFrame);
      moveFrame = requestAnimationFrame(() => pushAround(e.clientX, e.clientY));
    });
    mouth.addEventListener("pointerup", (e) => {
      if (!reach || reach.id !== e.pointerId) return;
      letGo();
      if (canReach()) form.requestSubmit(drawBtn);
    });
    mouth.addEventListener("pointercancel", letGo);
    mouth.addEventListener("pointerleave", (e) => { if (reach && e.pointerType === "mouse") letGo(); });
  }

  /* ── one gate for draw, put back and accept ───────────── */
  let busy = false;
  let run = null; // the current ceremony

  const sleep = (ms) => new Promise((res) => {
    if (!run || run.skipped || ms <= 0) return res();
    const t = setTimeout(done, ms);
    function done() { clearTimeout(t); run && run.wakers.delete(done); res(); }
    run.wakers.add(done);
  });
  function anim(el, kf, opts) {
    if (!el) return Promise.resolve();
    const o = typeof opts === "number" ? { duration: opts } : { ...opts };
    o.fill = o.fill || "both";
    const a = el.animate(kf, o);
    if (run) { run.anims.add(a); if (run.skipped) a.finish(); }
    return a.finished.catch(() => {});
  }
  function skip() {
    if (!run || run.skipped) return;
    run.skipped = true;
    run.anims.forEach((a) => { try { a.finish(); } catch (_) {} });
    run.wakers.forEach((w) => w());
    jar.classList.remove("stir");
  }
  function onSkipKey(e) {
    if (e.repeat || ["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
    if (run && !run.skipped) { e.preventDefault(); skip(); }
  }
  // the tap that skips must not also click whatever is underneath it
  function swallowNextClick() {
    const eat = (ev) => { ev.preventDefault(); ev.stopPropagation(); done(); };
    const done = () => { document.removeEventListener("click", eat, true); clearTimeout(t); };
    const t = setTimeout(done, 500);
    document.addEventListener("click", eat, true);
  }
  function onSkipTap(e) {
    if (run && !run.skipped) { e.preventDefault(); e.stopPropagation(); swallowNextClick(); skip(); }
  }
  function armSkip() {
    // next frame, so the tap or Enter that started the draw doesn't skip it
    requestAnimationFrame(() => {
      if (!run) return;
      document.addEventListener("pointerdown", onSkipTap, true);
      document.addEventListener("keydown", onSkipKey, true);
    });
  }
  function disarmSkip() {
    document.removeEventListener("pointerdown", onSkipTap, true);
    document.removeEventListener("keydown", onSkipKey, true);
  }
  function lock(on) {
    busy = on;
    stage.setAttribute("aria-busy", on ? "true" : "false");
    if (drawBtn) drawBtn.disabled = on;
    if (pick) $$("button", pick).forEach((b) => { b.disabled = on; });
  }

  const kindOf = (elt) => {
    if (!elt || !elt.classList) return null;
    if (elt === form) return "draw";
    if (elt.classList.contains("relax")) return "draw";
    if (elt.classList.contains("accept")) return "accept";
    if (elt.classList.contains("putback")) return "putback";
    return null;
  };

  document.addEventListener("htmx:beforeRequest", (e) => {
    const elt = e.detail.elt;
    const kind = kindOf(elt);
    if (!kind) return;
    if (busy) { e.preventDefault(); return; }
    if (elt.classList.contains("relax")) adoptRelax(elt);
    lock(true);
    letGo();
    run = { kind, skipped: reduced(), anims: new Set(), wakers: new Set(), html: null };
    run.response = new Promise((res, rej) => { run.resolve = res; run.reject = rej; });
    armSkip();
    ceremony(run).catch(() => fail()).finally(() => {
      disarmSkip();
      jar.classList.remove("stir");
      run = null;
      lock(false);
      settleState();
    });
  });
  document.addEventListener("htmx:beforeSwap", (e) => {
    if (!run || e.detail.target !== pick) return;
    e.detail.shouldSwap = false;
    if (e.detail.isError) run.reject(new Error("server"));
    else run.resolve(e.detail.serverResponse);
  });
  document.addEventListener("htmx:sendError", () => { if (run) run.reject(new Error("network")); });
  document.addEventListener("htmx:timeout", () => { if (run) run.reject(new Error("timeout")); });

  // the relax button posts loosened filters; make the sentence say what was drawn
  function adoptRelax(f) {
    const vals = {};
    $$("input[type=hidden]", f).forEach((h) => { (vals[h.name] = vals[h.name] || []).push(h.value); });
    ["budget", "max_duration", "tag", "energy", "max_km", "from"].forEach((name) => {
      const c = form.elements[name];
      if (!c || !vals[name]) return;
      const v = vals[name][0];
      if (c.tagName === "SELECT" && ![...c.options].some((o) => o.value === v)) return;
      c.value = v;
    });
    const want = new Set(vals.settings || []);
    $$("input[name=settings]", form).forEach((cb) => { cb.checked = want.has(cb.value); });
    syncSentence();
    refresh(0);
  }

  function settleState() {
    const has = !!(pick && pick.firstElementChild);
    stage.classList.toggle("has-card", has);
    const holding = !!(pick && $(".slip-card.is-pick", pick));
    stage.classList.toggle("card-pick", holding);
    // while a slip is in your hands, tonight's sentence is what it was drawn with
    const sentence = $(".sentence", form);
    if (sentence) { sentence.inert = holding; sentence.classList.toggle("held", holding); }
  }

  function fail() {
    jar.classList.remove("stir");
    $$(".fold", stage).forEach((f) => f.remove());
    if (pick) {
      $$(".slip-card", pick).forEach((c) => { c.style.visibility = ""; c.style.opacity = ""; });
      $$(".jar-error", stage).forEach((n) => n.remove());
      const p = document.createElement("p");
      p.className = "jar-error";
      p.setAttribute("role", "alert");
      p.textContent = "The lid stuck for a second. Try that again?";
      (pick.firstElementChild ? pick : stage).appendChild(p);
      setTimeout(() => p.remove(), 6000);
    }
  }

  /* ── ceremonies ───────────────────────────────────────── */
  async function ceremony(r) {
    if (r.kind === "draw") {
      // anything already lying on the table goes away first
      if (pick.firstElementChild) {
        await anim(pick, [{ opacity: 1 }, { opacity: 0 }], reduced() ? 120 : 200);
        pick.textContent = "";
        pick.getAnimations().forEach((a) => a.cancel());
        els.forEach((el) => el.classList.remove("gone"));
        settleState();
      }
      bringJarIntoView();
      if (!reduced()) jar.classList.add("stir");
      await sleep(300); // the little shake, even when the server is instant
      const html = await r.response;
      jar.classList.remove("stir");
      swapIn(html);
      await reveal(r);
    } else if (r.kind === "putback") {
      const old = $(".slip-card", pick);
      await foldAway(old);
      const html = await r.response;
      swapIn(html);
      const fresh = $$(".slip-card .token", pick).filter((t) => !t.classList.contains("full"));
      if (fresh.length) fresh[0].classList.add("just-used");
      await reveal(r);
    } else {
      const html = await r.response;
      swapIn(html);
      await stampIt();
    }
  }

  // the draw happens in the jar: make sure the jar (and the slip about to land on it) is in view
  function bringJarIntoView() {
    const top = jar.getBoundingClientRect().top;
    const want = 62;
    if (Math.abs(top - want) > 24) scrollBy({ top: top - want, behavior: reduced() ? "auto" : "smooth" });
  }

  function swapIn(html) {
    $$(".jar-error", stage).forEach((n) => n.remove());
    if (window.htmx && htmx.swap) htmx.swap(pick, html, { swapStyle: "innerHTML" });
    else { pick.innerHTML = html; if (window.htmx) htmx.process(pick); }
    settleState();
    if (busy) $$("button", pick).forEach((b) => { b.disabled = true; });
  }

  function focusTitle(el) {
    const t = el && $(".slip-title, .note-text", el);
    if (t) t.focus({ preventScroll: true });
    const card = el;
    if (!card) return;
    const top = card.getBoundingClientRect().top;
    const headerH = 70;
    if (top < headerH || top > innerHeight * 0.7) {
      scrollBy({ top: top - headerH - 12, behavior: reduced() ? "auto" : "smooth" });
    }
  }

  function geom(card) {
    const s = stage.getBoundingClientRect();
    const r = card.getBoundingClientRect(); // rotated by 1deg; close enough for placement
    const W = card.offsetWidth, H = card.offsetHeight;
    return { left: r.left - s.left + (r.width - W) / 2, top: r.top - s.top + (r.height - H) / 2, W, H };
  }

  // Build the slip as three panels hinged at its creases, from a clone of the card.
  function buildFold(card) {
    const g = geom(card);
    const paper = $(".paper", card);
    const third = g.H / 3;
    const fold = document.createElement("div");
    fold.className = "fold " + ([...card.classList].find((c) => c.startsWith("set-")) || "");
    fold.setAttribute("aria-hidden", "true");
    fold.inert = true;
    Object.assign(fold.style, { left: g.left + "px", top: g.top + "px", width: g.W + "px", height: g.H + "px" });
    fold.style.filter = getComputedStyle(card).filter;
    const face = (i) => {
      const f = document.createElement("div");
      f.className = "face front";
      const c = paper.cloneNode(true);
      c.querySelectorAll("[id]").forEach((n) => n.removeAttribute("id"));
      Object.assign(c.style, { width: g.W + "px", height: g.H + "px", top: -i * third + "px" });
      f.appendChild(c);
      const sh = document.createElement("i"); sh.className = "shade"; f.appendChild(sh);
      return f;
    };
    const panel = (i, cls, origin) => {
      const p = document.createElement("div");
      p.className = "fp " + cls;
      Object.assign(p.style, { top: i * third + "px", height: third + "px", transformOrigin: origin });
      p.appendChild(face(i));
      if (cls === "hinge") {
        const b = document.createElement("div");
        b.className = "face back";
        if (i === 0 && stage.dataset.couple) {
          // the outside of the folded note, addressed to them
          const to = document.createElement("span");
          to.className = "to hand";
          to.textContent = "for " + stage.dataset.couple;
          b.appendChild(to);
        }
        const sh = document.createElement("i"); sh.className = "shade"; b.appendChild(sh);
        p.appendChild(b);
      }
      return p;
    };
    const mid = panel(1, "mid", "50% 50%");
    const bot = panel(2, "hinge", "50% 0%");
    const top = panel(0, "hinge", "50% 100%");
    const sheen = document.createElement("i"); sheen.className = "sheen";
    fold.append(mid, bot, top, sheen);
    stage.appendChild(fold);
    return { fold, mid, top, bot, sheen, g };
  }

  const FOLD = 179.4;
  const P = "perspective(1100px) ";

  async function reveal(r) {
    const card = $(".slip-card", pick);
    const note = $(".note", pick);
    if (note) {
      if (reduced() || r.skipped) await anim(note, [{ opacity: 0 }, { opacity: 1 }], 150);
      else await anim(note, [
        { opacity: 0, transform: "translateY(-18px) rotate(-3deg) scale(.96)" },
        { opacity: 1, transform: "none" },
      ], { duration: 420, easing: "cubic-bezier(.34,1.56,.64,1)" });
      focusTitle(note);
      return;
    }
    if (!card) return;
    const id = +card.dataset.id;
    const hs = els.get(id);
    if (reduced() || r.skipped) {
      if (hs) hs.classList.add("gone");
      await anim(card, [{ opacity: 0 }, { opacity: 1 }], 150);
      card.getAnimations().forEach((a) => a.cancel());
      focusTitle(card);
      return;
    }
    const quick = r.kind === "putback";
    const U = quick ? 260 : 300, STAG = quick ? 80 : 100;
    card.style.visibility = "hidden";
    const asm = buildFold(card);
    const { fold, top, bot, g } = asm;
    const sr = stage.getBoundingClientRect();
    // start exactly where the slip lies in the heap
    let start = "translate(0px, 30px) scale(.12)";
    if (hs) {
      const hr = hs.getBoundingClientRect();
      const cx = hr.left + hr.width / 2 - sr.left, cy = hr.top + hr.height / 2 - sr.top;
      const fx = g.left + g.W / 2, fy = g.top + g.H / 2;
      const sc = (parseFloat(getComputedStyle(hs).width) || 40) / g.W;
      start = `translate(${cx - fx}px, ${cy - fy}px) rotate(${(+hs.dataset.r || 0) + 1}deg) scale(${sc.toFixed(3)})`;
      hs.classList.add("gone");
    }
    top.style.transform = P + `rotateX(${-FOLD}deg)`;
    bot.style.transform = P + `rotateX(${FOLD}deg)`;
    const shadowNear = "drop-shadow(0 34px 30px rgba(20,12,30,.32))";
    const base = fold.style.filter;
    // rise: up out of the mouth, toward you
    await anim(fold, [
      { transform: start, filter: "drop-shadow(0 1px 1px rgba(20,12,30,.3))", offset: 0 },
      { transform: "translate(0, -10px) rotate(-2.5deg) scale(1.06)", filter: shadowNear, offset: .62, easing: "cubic-bezier(.3,.7,.3,1)" },
      { transform: "none", filter: base, offset: 1 },
    ], { duration: quick ? 340 : 400, easing: "cubic-bezier(.2,.8,.2,1)" });
    // unfold along the creases, light catching each panel as it opens
    const topShade = $(".front .shade", top), botShade = $(".front .shade", bot);
    const ease = "cubic-bezier(.3,.6,.2,1)";
    const a1 = anim(top, [{ transform: P + `rotateX(${-FOLD}deg)` }, { transform: P + "rotateX(0deg)" }], { duration: U, easing: ease });
    anim(topShade, [{ opacity: .5 }, { opacity: 0 }], { duration: U, easing: ease });
    await sleep(STAG);
    const a2 = anim(bot, [{ transform: P + `rotateX(${FOLD}deg)` }, { transform: P + "rotateX(0deg)" }], { duration: U, easing: ease });
    anim(botShade, [{ opacity: .45 }, { opacity: 0 }], { duration: U, easing: ease });
    await Promise.all([a1, a2]);
    card.style.visibility = "";
    fold.remove();
    if (!run.skipped) {
      // a last glint as it lies flat
      const glint = document.createElement("i");
      glint.className = "sheen";
      glint.setAttribute("aria-hidden", "true");
      $(".paper", card).appendChild(glint);
      anim(glint, [{ opacity: 0, transform: "translateX(-60%)" }, { opacity: 1, offset: .4 }, { opacity: 0, transform: "translateX(60%)" }], { duration: 520, easing: "ease-out" })
        .then(() => glint.remove());
    }
    focusTitle(card);
  }

  async function foldAway(card) {
    if (!card) return;
    const id = +card.dataset.id;
    const hs = els.get(id);
    if (reduced() || (run && run.skipped)) {
      await anim(card, [{ opacity: 1 }, { opacity: 0 }], 120);
      if (hs) hs.classList.remove("gone");
      return;
    }
    const asm = buildFold(card);
    const { fold, top, bot, g } = asm;
    card.style.visibility = "hidden";
    const ease = "cubic-bezier(.5,0,.6,1)";
    const b = anim(bot, [{ transform: P + "rotateX(0deg)" }, { transform: P + `rotateX(${FOLD}deg)` }], { duration: 160, easing: ease });
    anim($(".front .shade", bot), [{ opacity: 0 }, { opacity: .45 }], { duration: 160, easing: ease });
    await sleep(70);
    const t = anim(top, [{ transform: P + "rotateX(0deg)" }, { transform: P + `rotateX(${-FOLD}deg)` }], { duration: 160, easing: ease });
    anim($(".front .shade", top), [{ opacity: 0 }, { opacity: .5 }], { duration: 160, easing: ease });
    await Promise.all([b, t]);
    // and down it goes, back into the jar
    const sr = stage.getBoundingClientRect();
    let end = "translate(0px, 40px) scale(.1)";
    if (hs) {
      hs.classList.add("gone");
      const hr = hs.getBoundingClientRect();
      const cx = hr.left + hr.width / 2 - sr.left, cy = hr.top + hr.height / 2 - sr.top;
      const sc = (parseFloat(getComputedStyle(hs).width) || 40) / g.W;
      end = `translate(${cx - (g.left + g.W / 2)}px, ${cy - (g.top + g.H / 2)}px) rotate(${(+hs.dataset.r || 0) + 1}deg) scale(${sc.toFixed(3)})`;
    }
    stage.classList.remove("has-card"); // the jar comes back into focus as it falls
    await anim(fold, [
      { transform: "none", opacity: 1 },
      { transform: "translate(0,-8px) scale(1.03)", opacity: 1, offset: .25 },
      { transform: end, opacity: 1, offset: .96 },
      { transform: end, opacity: 0 },
    ], { duration: 260, easing: "cubic-bezier(.55,0,.75,.4)" });
    fold.remove();
    card.style.opacity = "0";
    if (hs) {
      hs.classList.remove("gone");
      hs.classList.add("home-again");
      setTimeout(() => hs.classList.remove("home-again"), 600);
    }
    jar.classList.add("plop");
    setTimeout(() => jar.classList.remove("plop"), 320);
    await sleep(40);
  }

  async function stampIt() {
    const card = $(".slip-card.is-done", pick);
    const stamp = card && $(".stamp", card);
    if (!stamp) { focusTitle(card); return; }
    const buzz = () => { try { navigator.vibrate && navigator.vibrate(12); } catch (_) {} };
    if (reduced() || (run && run.skipped)) {
      buzz();
      await anim(stamp, [{ opacity: 0 }, { opacity: 1 }], 150);
      focusTitle(card);
      return;
    }
    const paper = $(".paper", card);
    const landing = anim(stamp, [
      { opacity: 0, transform: "translate(-14px,-30px) rotate(-10deg) scale(2.3)", filter: "blur(5px)", offset: 0 },
      { opacity: .5, transform: "translate(-6px,-10px) rotate(-4deg) scale(1.5)", filter: "blur(2px)", offset: .38, easing: "cubic-bezier(.6,0,.9,.5)" },
      { opacity: 1, transform: "rotate(0deg) scale(.94)", filter: "blur(0)", offset: .52, easing: "cubic-bezier(.34,1.56,.64,1)" },
      { opacity: 1, transform: "scale(1.02)", offset: .7 },
      { opacity: 1, transform: "none", filter: "blur(0)", offset: 1 },
    ], { duration: 760 });
    await sleep(395);
    // thump
    buzz();
    anim(paper, [
      { transform: "none" },
      { transform: "translateY(3px) rotate(.35deg) scale(.992)", offset: .3 },
      { transform: "translateY(-1px)", offset: .7 },
      { transform: "none" },
    ], { duration: 300, easing: "cubic-bezier(.2,.8,.2,1)" });
    hearts(card, stamp);
    await landing;
    stamp.getAnimations().forEach((a) => a.cancel());
    focusTitle(card);
  }

  function hearts(card, stamp) {
    // measure the untransformed zone the stamp sits in, not the stamp mid-flight
    const cr = card.getBoundingClientRect(), zr = stamp.parentElement.getBoundingClientRect();
    const sw = stamp.offsetWidth, sh = stamp.offsetHeight;
    const sr = { left: zr.left + (zr.width - sw) / 2, top: zr.top + (zr.height - sh) / 2, width: sw, height: sh };
    const paths = "M8 14.2C3.6 11 1.4 8.4 1.6 5.6 1.8 3.4 3.5 2 5.4 2.1c1.2.1 2 .8 2.6 1.9.6-1.1 1.5-1.8 2.7-1.9 2-.1 3.6 1.4 3.6 3.5 0 2.9-2.3 5.5-6.3 8.6z";
    const spots = [[.04, .25, -46, -58, -26], [.96, .05, 40, -70, 24], [.7, 1, 30, 34, 16]];
    spots.forEach(([fx, fy, dx, dy, rot], i) => {
      const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      s.setAttribute("viewBox", "0 0 16 16");
      s.setAttribute("aria-hidden", "true");
      s.classList.add("ink-heart");
      const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
      p.setAttribute("d", paths);
      s.appendChild(p);
      s.style.left = (sr.left - cr.left + sr.width * fx - 8) + "px";
      s.style.top = (sr.top - cr.top + sr.height * fy - 8) + "px";
      if (i === 2) { s.style.width = s.style.height = "13px"; }
      card.appendChild(s);
      anim(s, [
        { opacity: 0, transform: "translate(0,0) scale(.4) rotate(0deg)" },
        { opacity: 1, transform: `translate(${dx * .5}px,${dy * .5}px) scale(1) rotate(${rot / 2}deg)`, offset: .3 },
        { opacity: 0, transform: `translate(${dx}px,${dy}px) scale(.85) rotate(${rot}deg)` },
      ], { duration: 640, delay: i * 40, easing: "cubic-bezier(.2,.8,.2,1)" }).then(() => s.remove());
    });
  }

  /* ── housekeeping ─────────────────────────────────────── */
  document.addEventListener("visibilitychange", () => {
    document.documentElement.classList.toggle("hidden-tab", document.hidden);
  });
  addEventListener("pageshow", (e) => { if (e.persisted) { syncSentence(); refresh(0); } });

  // test hook: render a synthetic jar (used for 3/200-slip checks)
  window.__jar = { render: (list) => { slips = list; render(list); }, refresh };
})();
