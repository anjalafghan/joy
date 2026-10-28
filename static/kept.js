// Kept: small kindnesses on top of plain forms. Everything works without this file.
(() => {
  const book = document.getElementById("k-book");
  const store = (k, v) => { try { v === undefined ? sessionStorage.removeItem(k) : sessionStorage.setItem(k, v); } catch (_) {} };
  const recall = (k) => { try { return sessionStorage.getItem(k); } catch (_) { return null; } };

  // Budget: landing on #budget (or #k-budget) opens the editor and puts the cursor in it.
  const openBudget = () => {
    if (!/^#(k-)?budget$/.test(location.hash)) return;
    const d = document.querySelector(".k-budget-edit");
    if (!d) return;
    d.open = true;
    document.getElementById("budget").focus({ preventScroll: true });
    d.scrollIntoView({ block: "center" });
  };
  openBudget();
  addEventListener("hashchange", openBudget);
  document.querySelector(".k-budget-edit")?.addEventListener("toggle", (e) => {
    if (e.target.open) document.getElementById("budget")?.focus();
  });

  // A no-JS photo error arrives as ?err=…; show it once, then tidy the address.
  const alertBox = document.querySelector(".k-alert");
  if (alertBox) {
    try { history.replaceState(null, "", location.pathname + location.hash); } catch (_) {}
    alertBox.querySelector(".k-alert-x")?.addEventListener("click", (e) => { e.preventDefault(); alertBox.remove(); });
  }

  if (!book) return;
  const reduce = matchMedia("(prefers-reduced-motion: reduce)");
  const fieldSizing = CSS.supports && CSS.supports("field-sizing", "content");

  // Notes grow with their words where field-sizing isn't available.
  const fit = (ta) => { ta.style.height = "auto"; ta.style.height = ta.scrollHeight + "px"; };
  if (!fieldSizing) {
    book.querySelectorAll(".k-note-in").forEach(fit);
    book.addEventListener("input", (e) => { if (e.target.matches(".k-note-in")) fit(e.target); });
  }

  // A note is one line to remember it by: Enter puts the pen down (and saves).
  book.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.shiftKey && e.target.matches(".k-note-in")) {
      e.preventDefault();
      e.target.blur();
      // Your own line has no autosave: Enter seals it.
      if (e.target.matches(".k-line-in")) e.target.form.requestSubmit();
    }
  });

  const timers = new WeakMap();
  const say = (form, text, tone) => {
    const s = form.querySelector(".k-status");
    if (!s) return;
    s.textContent = text;
    s.dataset.tone = tone || "";
    clearTimeout(timers.get(s));
    if (tone === "ok") timers.set(s, setTimeout(() => { s.textContent = ""; s.dataset.tone = ""; }, 2200));
  };

  const heartSvg = '<svg viewBox="0 0 32 30" aria-hidden="true"><use href="#k-heart"/></svg>';
  const pop = (label, n) => {
    if (reduce.matches || !label) return;
    label.classList.remove("k-pop");
    void label.offsetWidth;
    label.classList.add("k-pop");
    if (n < 5) return;
    // five hearts: a couple of tiny ink hearts lift off
    [[-14, -18], [12, 14], [2, -6]].forEach(([dx, dr], i) => {
      const s = document.createElement("span");
      s.innerHTML = heartSvg;
      const svg = s.firstChild;
      svg.classList.add("k-spark");
      svg.style.setProperty("--dx", dx + "px");
      svg.style.setProperty("--dr", dr + "deg");
      svg.style.animationDelay = i * 60 + "ms";
      label.appendChild(svg);
      svg.addEventListener("animationend", () => svg.remove(), { once: true });
    });
  };

  book.addEventListener("change", (e) => {
    const t = e.target;
    if (t.name === "rating") {
      const n = +t.value || 0;
      if (n) pop(t.nextElementSibling, n);
      if (navigator.vibrate && n) navigator.vibrate(8);
    }
  });

  book.addEventListener("focusin", (e) => {
    if (e.target.matches(".k-note-in")) say(e.target.form, "");
  });

  // htmx feedback: a quiet "kept" in the margin, or an honest "didn't save".
  book.addEventListener("htmx:afterRequest", (e) => {
    const form = e.target.closest && e.target.closest(".k-form");
    if (!form) return;
    if (e.detail.successful) say(form, "kept ♥", "ok");
    else say(form, "didn't save, try again", "err");
  });
  book.addEventListener("htmx:sendError", (e) => {
    const form = e.target.closest && e.target.closest(".k-form");
    if (form) say(form, "offline? didn't save", "err");
  });

  // ── re-render one slip from a fresh copy of the page ─────────────────
  const fitAll = (root) => { if (!fieldSizing) root.querySelectorAll(".k-note-in").forEach(fit); };
  const swapSlip = (id, html, focusSel) => {
    const doc = new DOMParser().parseFromString(html, "text/html");
    const fresh = doc.getElementById("k" + id);
    const old = document.getElementById("k" + id);
    if (!fresh || !old) { location.href = "/history#k" + id; location.reload(); return null; }
    fresh.classList.add("k-swapped");
    old.replaceWith(fresh);
    if (window.htmx) htmx.process(fresh);
    fitAll(fresh);
    const f = focusSel && fresh.querySelector(focusSel);
    if (f) f.focus({ preventScroll: false });
    return fresh;
  };
  const refetch = (id) => fetch("/history", { credentials: "same-origin" }).then((r) => r.text());

  // ── when is it? / remove photo: post, then swap just that slip ───────
  book.addEventListener("submit", (e) => {
    const form = e.target;
    if (form.matches(".k-reveal-form")) { store("k-revealed", form.dataset.id); return; }
    const id = form.dataset.swap;
    if (!id) return;
    e.preventDefault();
    const btn = form.querySelector("button[type=submit]");
    const err = form.querySelector(".k-form-err");
    if (btn) btn.disabled = true;
    if (err) err.textContent = "";
    fetch(form.action, {
      method: "POST", credentials: "same-origin",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams(new FormData(form)),
    }).then(async (r) => {
      if (!r.ok) throw new Error((await r.text()) || "That didn't save. Try again?");
      const html = r.url.includes("/history") && !r.url.includes("/when") ? await r.text() : await refetch(id);
      const isWhen = form.matches(".k-when-form");
      const fresh = swapSlip(id, html, isWhen ? ".k-when-edit > summary" : ".k-photo-form .k-file");
      const rf = fresh && fresh.querySelector(".k-form");
      if (rf) say(rf, isWhen ? (fresh.querySelector(".k-planned") ? "time saved ♥" : "time cleared") : "photo removed", "ok");
    }).catch((x) => {
      if (btn) btn.disabled = false;
      if (err) err.textContent = x.message || "Offline? That didn't save.";
      else form.submit();
    });
  });

  // ── photos: pick → (shrink) → upload with progress → developing → swap ──
  const MAX_EDGE = 2048;
  const shrink = async (file) => {
    // Big or unusual files get redrawn as a JPEG where the browser can decode them
    // (iPhone Safari decodes HEIC here). Anything that fails goes up as it is.
    try {
      if (!window.createImageBitmap || (file.size < 2.5e6 && /^image\/(jpeg|png|webp)$/.test(file.type))) return file;
      const bmp = await createImageBitmap(file);
      const k = Math.min(1, MAX_EDGE / Math.max(bmp.width, bmp.height));
      const c = document.createElement("canvas");
      c.width = Math.round(bmp.width * k); c.height = Math.round(bmp.height * k);
      c.getContext("2d").drawImage(bmp, 0, 0, c.width, c.height);
      bmp.close && bmp.close();
      const blob = await new Promise((res) => c.toBlob(res, "image/jpeg", 0.88));
      return blob ? new File([blob], "photo.jpg", { type: "image/jpeg" }) : file;
    } catch (_) { return file; }
  };

  const placeholder = (slip) => {
    let fig = slip.querySelector(".k-polaroid");
    if (fig) { fig.classList.add("is-developing"); return () => fig.classList.remove("is-developing"); }
    fig = document.createElement("figure");
    fig.className = "k-polaroid k-pending";
    fig.setAttribute("aria-hidden", "true");
    fig.innerHTML = '<div class="k-dev">developing…</div><figcaption class="hand">&nbsp;</figcaption>';
    const anchor = slip.querySelector(".k-form");
    anchor.parentNode.insertBefore(fig, anchor);
    return () => fig.remove();
  };

  book.addEventListener("change", (e) => {
    const input = e.target;
    if (!input.matches(".k-file") || !input.files || !input.files[0]) return;
    const form = input.form;
    const id = form.dataset.id;
    const slip = document.getElementById("k" + id);
    const status = form.querySelector(".k-photo-status");
    const tell = (t, tone) => { status.textContent = t; status.dataset.tone = tone || ""; };
    form.classList.add("is-busy");
    const undo = placeholder(slip);
    tell("Getting it ready…");
    shrink(input.files[0]).then((file) => {
      const fd = new FormData();
      fd.append("photo", file, file.name || "photo.jpg");
      const xhr = new XMLHttpRequest();
      xhr.open("POST", form.action);
      xhr.setRequestHeader("X-Requested-With", "fetch");
      xhr.upload.onprogress = (p) => {
        if (p.lengthComputable) tell(p.loaded < p.total ? `Uploading… ${Math.round((p.loaded / p.total) * 100)}%` : "Developing…");
      };
      const fail = (msg) => { undo(); form.classList.remove("is-busy"); input.value = ""; tell(msg, "err"); };
      xhr.onerror = () => fail("Offline? The photo didn't go up. Try again in a moment.");
      xhr.onload = () => {
        if (xhr.status === 204 || (xhr.status >= 200 && xhr.status < 300)) {
          tell("Developing…");
          // overwrite the day-long cache entry so later page loads show the new photo too
          fetch("/photos/" + id, { cache: "reload", credentials: "same-origin" }).catch(() => {})
            .then(() => refetch(id)).then((html) => {
            const fresh = swapSlip(id, html, null);
            const fig = fresh && fresh.querySelector(".k-polaroid");
            if (!fig) return;
            const img = fig.querySelector("img");
            img.loading = "eager";
            img.src = img.getAttribute("src").split("?")[0] + "?v=" + Date.now(); // the old one is cached for a day
            fig.classList.add("k-fresh");
            fresh.querySelector(".k-file")?.focus({ preventScroll: true });
            fig.scrollIntoView({ block: "nearest", behavior: reduce.matches ? "auto" : "smooth" });
            const st = fresh.querySelector(".k-photo-status");
            if (st) st.textContent = "Photo added.";
          }).catch(() => location.reload());
        } else if (xhr.status === 422) {
          fail(xhr.responseText || "That photo couldn't be used. Try another one.");
        } else {
          fail("That didn't work. Try again?");
        }
      };
      xhr.send(fd);
    });
  });

  // ── a surprise, just opened: lift the flap off the slip underneath ───
  const opened = recall("k-revealed");
  if (opened) {
    store("k-revealed");
    const slip = location.hash === "#k" + opened && document.getElementById("k" + opened);
    if (slip && !slip.classList.contains("k-env")) {
      slip.classList.add("k-swapped");
      const tada = document.createElement("span");
      tada.className = "k-tada hand";
      tada.setAttribute("aria-hidden", "true");
      tada.textContent = "surprise!";
      slip.appendChild(tada);
      if (!reduce.matches) {
        const cover = document.createElement("div");
        cover.className = "k-cover";
        cover.setAttribute("aria-hidden", "true");
        cover.innerHTML = '<span class="k-env-flap"></span><span class="k-wax"><svg width="22" height="20"><use href="#k-heart"/></svg></span>';
        slip.appendChild(cover);
        const done = () => cover.remove();
        cover.addEventListener("animationend", (ev) => { if (ev.target === cover) done(); });
        cover.addEventListener("pointerdown", done);
        addEventListener("keydown", done, { once: true });
        setTimeout(done, 1400);
      }
      if (navigator.vibrate) navigator.vibrate(12);
    }
  }

  // ── Add to calendar: hand the .ics to the share sheet where it can take files
  // (so it can go straight to the other one of us); otherwise it's a plain download.
  // Files are fetched ahead, because share() must run inside the tap itself.
  const icsFiles = new Map();
  const prepIcs = (a) => {
    const url = a.getAttribute("href");
    if (icsFiles.has(url)) return;
    icsFiles.set(url, null);
    fetch(url, { credentials: "same-origin" }).then((r) => (r.ok ? r.blob() : null)).then((b) => {
      if (!b) return;
      const f = new File([b], "joy-date.ics", { type: "text/calendar" });
      try { if (navigator.canShare({ files: [f] })) icsFiles.set(url, f); } catch (_) {}
    }).catch(() => {});
  };
  const scanIcs = () => { if (navigator.canShare) book.querySelectorAll("a[href$='/invite.ics']").forEach(prepIcs); };
  scanIcs();
  new MutationObserver(scanIcs).observe(book, { childList: true, subtree: true });
  book.addEventListener("click", (e) => {
    const a = e.target.closest && e.target.closest("a[href$='/invite.ics']");
    if (!a) return;
    const f = icsFiles.get(a.getAttribute("href"));
    if (!f || !navigator.share) return; // the download happens as usual
    e.preventDefault();
    navigator.share({ files: [f], title: "Our date" }).catch((x) => {
      if (x && x.name === "AbortError") return;
      location.href = a.href;
    });
  });
})();
