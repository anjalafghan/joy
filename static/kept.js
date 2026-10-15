// Kept: small kindnesses on top of plain forms. Everything works without this file.
(() => {
  const book = document.getElementById("k-book");
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
})();
