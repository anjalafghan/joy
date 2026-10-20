// Our year: the share picture, drawn on the phone. See docs/year-in-review-and-offline.md §3.5–3.6.
// Tap 1 "Make our picture" does the slow part (fonts, photos, canvas, encode) and opens a preview.
// Tap 2 "Share" calls navigator.share({files}) first thing, so the tap's activation is still fresh.
// The picture only ever uses the #year-share island: no names, notes, homes or app address.
(() => {
  "use strict";
  const island = document.getElementById("year-share");
  const panel = document.getElementById("y-share");
  if (!island || !panel) return;
  let data;
  try { data = JSON.parse(island.textContent); } catch (_) { return; }
  if (!data || !data.dates) return;
  if (!("toBlob" in HTMLCanvasElement.prototype)) return; // no picture without toBlob; the page stays readable

  const $ = (id) => document.getElementById(id);
  const makeBtn = $("y-make"), status = $("y-status"), sheet = $("y-sheet");
  const preview = $("y-preview"), sendBtn = $("y-send"), save = $("y-save"), saveLink = $("y-save-link");
  const optPlaces = $("y-opt-places"), optSpent = $("y-opt-spent");
  const MAX_PHOTOS = 9;

  // ── reveal the controls (they do nothing without JS) ─────────────
  const places = [...new Set((data.places || []).filter(Boolean))];
  panel.hidden = false;
  if (!places.length) optPlaces.closest("label").hidden = true;
  if (!data.spent) optSpent.closest("label").hidden = true;
  const photoUrl = new Map((data.photos || []).map(([id, url]) => [String(id), url]));
  const picks = [...document.querySelectorAll(".y-pick-in")].filter((i) => photoUrl.has(i.value));
  picks.forEach((i) => { i.closest("label").hidden = false; });
  document.addEventListener("change", (e) => {
    if (!e.target.classList || !e.target.classList.contains("y-pick-in")) return;
    if (picks.filter((i) => i.checked).length > MAX_PHOTOS) {
      e.target.checked = false;
      say(`Up to ${MAX_PHOTOS} photos fit in the picture.`);
    }
  });

  const say = (t) => { status.textContent = t; };
  let file = null, url = "", busy = false;

  // ── tap 1: make the picture ──────────────────────────────────────
  makeBtn.addEventListener("click", async () => {
    if (busy) return;
    busy = true; makeBtn.disabled = true; say("Making our picture…");
    const chosen = picks.filter((i) => i.checked).slice(0, MAX_PHOTOS).map((i) => photoUrl.get(i.value));
    let bitmaps = [];
    const canvas = document.createElement("canvas");
    try {
      await loadFonts();
      bitmaps = (await Promise.all(chosen.map(decode))).filter(Boolean);
      canvas.width = W; canvas.height = H; // never devicePixelRatio: 1080 wide is the story size
      paint(canvas.getContext("2d"), {
        ...data,
        places: optPlaces.checked ? places : [],
        spent: optSpent.checked ? data.spent : "",
      }, bitmaps);
      const type = bitmaps.length ? "image/jpeg" : "image/png";
      const blob = await new Promise((ok, no) => canvas.toBlob((b) => (b ? ok(b) : no(new Error("toBlob"))), type, 0.9));
      const name = `our-${data.year}.${bitmaps.length ? "jpg" : "png"}`;
      file = new File([blob], name, { type });
      if (url) URL.revokeObjectURL(url);
      url = URL.createObjectURL(file);
      preview.src = url;
      saveLink.href = url;
      saveLink.download = name;
      let can = false;
      try { can = !!(navigator.share && navigator.canShare && navigator.canShare({ files: [file] })); } catch (_) {}
      sendBtn.hidden = !can;
      save.hidden = can;
      say("");
      open();
    } catch (err) {
      say("The picture didn't come out this time. Try once more?");
    } finally {
      canvas.width = canvas.height = 0;
      bitmaps.forEach((b) => b.close && b.close());
      busy = false; makeBtn.disabled = false;
    }
  });

  // ── tap 2: share (synchronously, first thing) ────────────────────
  sendBtn.addEventListener("click", () => {
    if (!file) return;
    let p;
    try { p = navigator.share({ files: [file] }); } catch (err) { fallback(); return; }
    p.then(() => close(), (err) => { if (err && err.name !== "AbortError") fallback(); });
  });
  const fallback = () => { sendBtn.hidden = true; save.hidden = false; saveLink.focus(); };

  const open = () => {
    if (sheet.showModal) sheet.showModal(); else sheet.setAttribute("open", "");
    (sendBtn.hidden ? saveLink : sendBtn).focus();
  };
  const close = () => { if (sheet.close) sheet.close(); else { sheet.removeAttribute("open"); cleanup(); } };
  const cleanup = () => {
    preview.removeAttribute("src");
    saveLink.setAttribute("href", "#");
    if (url) URL.revokeObjectURL(url);
    url = ""; file = null;
    makeBtn.focus();
  };
  $("y-close").addEventListener("click", close);
  sheet.addEventListener("close", cleanup);
  sheet.addEventListener("click", (e) => { if (e.target === sheet) close(); }); // tap the backdrop

  // ── loading ──────────────────────────────────────────────────────
  // document.fonts.ready only waits for faces the page already asked for; ask for each one.
  const loadFonts = () => {
    if (!document.fonts || !document.fonts.load) return Promise.resolve();
    return Promise.all([
      document.fonts.load('640 96px "Fraunces"'),
      document.fonts.load('italic 400 48px "Fraunces"'),
      document.fonts.load('600 32px "Instrument Sans"'),
      document.fonts.load('400 32px "Instrument Sans"'),
      document.fonts.load('600 40px "Caveat"'),
    ]).catch(() => {});
  };
  const decode = async (src) => {
    try {
      const r = await fetch(src, { credentials: "same-origin" });
      if (!r.ok) return null;
      const blob = await r.blob();
      try { return await createImageBitmap(blob, { resizeWidth: 720, resizeQuality: "high" }); }
      catch (_) { return await createImageBitmap(blob); }
    } catch (_) { return null; }
  };

  // ── drawing: one fixed palette, the jar's paper by day ───────────
  const W = 1080, H = 1920, M = 96;
  const C = {
    paper: "#f5eee3", paper2: "#ebe2d3", slip: "#fffaf2", ink: "#2b2433", soft: "#675d6e",
    rule: "#d8cab8", stamp: "#b9301b", tape: "rgba(242,207,198,.85)",
  };
  const TINT = { "At home": "#f3e2b3", "Around town": "#f2cfc6", "Day trip": "#d3e1c6", "Travel": "#cbdbea" };
  const STAR = "M12.3 2.4c.9 2 1.7 4 2.6 6.1 2.1.1 4.3.3 6.4.6-1.6 1.4-3.3 2.8-4.9 4.1.5 2.1 1 4.2 1.4 6.3-1.8-1.1-3.7-2.3-5.6-3.4-1.8 1.2-3.6 2.4-5.5 3.5.5-2.2 1-4.3 1.5-6.4C6.6 11.8 5 10.4 3.3 9c2.2-.2 4.4-.4 6.5-.5.8-2 1.6-4 2.5-6.1z";
  const MONTHS = "JFMAMJJASOND";
  const HEART = "M16.2 27.4C11.6 24.1 3.4 18.3 3.1 10.9 2.9 6.6 6 3.3 9.8 3.4c2.9.1 5 2.2 6.2 4.6 1.3-2.6 3.5-4.8 6.6-4.7 3.9.2 6.5 3.6 6.2 7.7-.5 7.2-8.4 12.6-12.6 16.4z";
  const FD = '"Fraunces", Georgia, serif', FU = '"Instrument Sans", system-ui, sans-serif', FH = '"Caveat", cursive';

  // seeded randomness, so the same year always draws the same paper
  const rng = (seed) => () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;

  const font = (ctx, f, spacing = 0) => {
    ctx.font = f;
    if ("letterSpacing" in ctx) ctx.letterSpacing = spacing + "px";
  };
  const wrap = (ctx, text, max, lines) => {
    const words = String(text).split(/\s+/).filter(Boolean), out = [];
    let line = "";
    for (const w of words) {
      const t = line ? line + " " + w : w;
      if (ctx.measureText(t).width <= max || !line) line = t;
      else { out.push(line); line = w; }
    }
    if (line) out.push(line);
    if (out.length > lines) {
      out.length = lines;
      let last = out[lines - 1];
      while (last && ctx.measureText(last + "…").width > max) last = last.replace(/\s*\S+$/, "");
      out[lines - 1] = (last || out[lines - 1]).replace(/[\s·,]+$/, "") + "…";
    }
    // a single word too wide for the line: shrink is the caller's call, but never overflow
    return out.map((l) => {
      while (ctx.measureText(l).width > max && l.length > 2) l = l.slice(0, -2) + "…";
      return l.replace(/^[\s·]+|[\s·]+$/g, "");
    });
  };
  const inkLine = (ctx, x1, y, x2, w, color) => {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = w; ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(x1, y + w * .5);
    ctx.bezierCurveTo(x1 + (x2 - x1) * .3, y - w * .9, x1 + (x2 - x1) * .62, y - w * .4, x2, y - w * 1.1);
    ctx.stroke();
    ctx.restore();
  };
  const star = (ctx, x, y, size, on, rot) => {
    ctx.save();
    ctx.translate(x + size / 2, y + size / 2); ctx.rotate(rot); ctx.translate(-size / 2, -size / 2);
    ctx.scale(size / 24, size / 24);
    const p = new Path2D(STAR);
    ctx.lineJoin = "round";
    if (on) { ctx.fillStyle = C.stamp; ctx.fill(p); }
    ctx.strokeStyle = on ? C.stamp : C.soft; ctx.lineWidth = 1.5; ctx.stroke(p);
    ctx.restore();
  };
  const tape = (ctx, x, y, w, h, rot) => {
    ctx.save();
    ctx.translate(x, y); ctx.rotate(rot);
    ctx.fillStyle = C.tape;
    ctx.beginPath();
    const teeth = 7;
    ctx.moveTo(0, 0);
    ctx.lineTo(w, 0);
    for (let i = 1; i <= teeth; i++) ctx.lineTo(w - (i % 2 ? 7 : 0), (h * i) / teeth);
    ctx.lineTo(0, h);
    for (let i = teeth - 1; i >= 0; i--) ctx.lineTo(i % 2 ? 7 : 0, (h * i) / teeth);
    ctx.closePath(); ctx.fill();
    ctx.globalAlpha = .25; ctx.fillStyle = C.slip;
    for (let i = 14; i < w - 10; i += 18) ctx.fillRect(i, 0, 4, h);
    ctx.restore();
  };

  // the postmark, drawn on its own small canvas so its ink can be made uneven
  const postmark = (year, r) => {
    const s = Math.ceil(r * 2 + 40), c = document.createElement("canvas");
    c.width = s; c.height = s;
    const g = c.getContext("2d"), cx = s / 2;
    g.strokeStyle = g.fillStyle = C.stamp;
    g.lineWidth = r * .045;
    g.beginPath(); g.arc(cx, cx, r, 0, Math.PI * 2); g.stroke();
    g.lineWidth = r * .022; g.setLineDash([r * .05, r * .055]);
    g.beginPath(); g.arc(cx, cx, r * .53, 0, Math.PI * 2); g.stroke();
    g.setLineDash([]);
    const text = "KEPT IN THE JAR · JUST US · ";
    font(g, `700 ${Math.round(r * .17)}px ${FU}`);
    const rr = r * .77, step = (Math.PI * 2) / text.length;
    g.textAlign = "center"; g.textBaseline = "middle";
    [...text].forEach((ch, i) => {
      g.save(); g.translate(cx, cx); g.rotate(-Math.PI / 2 + i * step); g.translate(0, -rr); g.fillText(ch, 0, 0); g.restore();
    });
    font(g, `650 ${Math.round(r * .4)}px ${FD}`);
    g.fillText(String(year), cx, cx + r * .03);
    // worn ink: knock out small specks
    const rnd = rng(year * 31 + 7);
    g.globalCompositeOperation = "destination-out";
    for (let i = 0; i < 420; i++) {
      g.globalAlpha = .35 + rnd() * .6;
      g.beginPath(); g.arc(rnd() * s, rnd() * s, rnd() * r * .028 + .6, 0, Math.PI * 2); g.fill();
    }
    return c;
  };

  // Lay out roomy first; if the extras don't fit above the sign-off, lay out again, tighter.
  const paint = (ctx, d, photos) => {
    let dense = photos.length > 0, end = draw(ctx, d, photos, dense, 0);
    if (!dense && end > H - 150) { dense = true; end = draw(ctx, d, photos, true, 0); }
    // without photos, share any spare height out between the sections so the page sits balanced
    const slack = H - 200 - end;
    if (!photos.length && slack > 24) draw(ctx, d, photos, dense, Math.min(slack, 380));
  };
  const draw = (ctx, d, photos, dense, extra) => {
    const gx = extra / 4;
    const rnd = rng(d.year * 97 + d.dates);

    // paper + a little grain
    ctx.fillStyle = C.paper; ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = C.ink;
    for (let i = 0; i < 2600; i++) { ctx.globalAlpha = rnd() * .05; ctx.fillRect(rnd() * W, rnd() * H, 2, 2); }
    ctx.globalAlpha = 1;
    ctx.textBaseline = "alphabetic"; ctx.textAlign = "left";

    // postmark + cancellation waves, top right
    const pr = dense ? 92 : 122, pcx = dense ? 868 : 846, pcy = dense ? 178 : 236;
    ctx.save();
    ctx.translate(pcx, pcy); ctx.rotate(-0.19);
    ctx.globalAlpha = .9;
    ctx.strokeStyle = C.stamp; ctx.lineWidth = 5; ctx.lineCap = "round";
    for (let k = -1; k <= 1; k++) {
      ctx.beginPath();
      const y = k * pr * .42;
      ctx.moveTo(-pr - 190, y);
      for (let x = -pr - 190; x < -pr - 10; x += 40) ctx.quadraticCurveTo(x + 10, y - 12, x + 20, y), ctx.quadraticCurveTo(x + 30, y + 12, x + 40, y);
      ctx.stroke();
    }
    const pm = postmark(d.year, pr);
    ctx.drawImage(pm, -pm.width / 2, -pm.height / 2);
    pm.width = pm.height = 0;
    ctx.restore();

    // eyebrow
    let y = dense ? 136 : 186;
    font(ctx, `600 30px ${FU}`, 5);
    ctx.fillStyle = C.soft;
    ctx.fillText("OUR YEAR IN THE JAR", M, y);

    // the big number, shrunk to keep clear of the postmark
    let size = dense ? 220 : 380;
    font(ctx, `640 ${size}px ${FD}`, -size * .04);
    const maxNum = dense ? 600 : 560;
    const nw0 = ctx.measureText(String(d.dates)).width;
    if (nw0 > maxNum) { size = Math.floor(size * maxNum / nw0); font(ctx, `640 ${size}px ${FD}`, -size * .04); }
    y += Math.round(size * .78) + (dense ? 18 : 30);
    ctx.fillStyle = C.ink;
    ctx.fillText(String(d.dates), M - size * .03, y);
    const nw = ctx.measureText(String(d.dates)).width;
    inkLine(ctx, M - 8, y + size * .07, M + nw * .97, size * .045, C.stamp);

    y += dense ? 84 : 130;
    font(ctx, `italic 420 ${dense ? 62 : 88}px ${FD}`, -1);
    ctx.fillText(`${d.dates === 1 ? "date" : "dates"} kept in ${d.year}`, M, y);

    // months ledger
    y += (dense ? 50 : 96) + gx;
    const barH = dense ? 96 : 190, base = y + barH + 34, colW = (W - 2 * M) / 12;
    const max = Math.max(1, ...d.months), top = Math.max(...d.months);
    d.months.forEach((n, i) => {
      const cx = M + colW * i + colW / 2;
      const busiest = n > 0 && n === top;
      if (n === 0) {
        ctx.strokeStyle = C.soft; ctx.lineWidth = 3;
        ctx.beginPath(); ctx.arc(cx, base - 10, 6, 0, Math.PI * 2); ctx.stroke();
      } else {
        const h = Math.max(8, (n / max) * barH), bw = dense ? 24 : 30;
        ctx.fillStyle = busiest ? C.stamp : C.ink;
        ctx.beginPath();
        if (ctx.roundRect) ctx.roundRect(cx - bw / 2, base - h, bw, h, [bw * .3, bw * .38, 2, 2]);
        else ctx.rect(cx - bw / 2, base - h, bw, h);
        ctx.fill();
        font(ctx, `600 ${dense ? 26 : 30}px ${FU}`);
        ctx.fillStyle = busiest ? C.stamp : C.soft; ctx.textAlign = "center";
        ctx.fillText(String(n), cx, base - h - 14);
      }
      font(ctx, `600 ${dense ? 26 : 30}px ${FU}`);
      ctx.fillStyle = C.soft; ctx.textAlign = "center";
      ctx.fillText(MONTHS[i], cx, base + (dense ? 40 : 48));
      ctx.textAlign = "left";
    });
    ctx.strokeStyle = C.ink; ctx.lineWidth = 3;
    ctx.beginPath(); ctx.moveTo(M, base + 1.5); ctx.lineTo(W - M, base + 1.5); ctx.stroke();
    y = base + (dense ? 86 : 120) + gx;

    // kind of night · streak · stars, as three little columns
    const cols = [];
    if (d.vibe) cols.push(["MOSTLY", d.vibe, d.tags && d.tags.length ? d.tags.slice(0, 3).join(" · ") : ""]);
    if (d.streak_weeks > 1) cols.push(["LONGEST RUN", `${d.streak_weeks} weeks`, "in a row"]);
    if (d.avg_rating) cols.push(["AVERAGE NIGHT", `${d.avg_rating} stars`, "out of five"]);
    if (cols.length) {
      const gap = 36, wt = cols.map((c, i) => (i === 0 && d.vibe ? 1.3 : 1)), tot = wt.reduce((a, b) => a + b, 0);
      const inner = W - 2 * M - gap * (cols.length - 1);
      let x = M;
      cols.forEach(([k, v, s], i) => {
        const cw = inner * wt[i] / tot;
        if (i) x += inner * wt[i - 1] / tot + gap;
        if (i === 0 && d.vibe) { ctx.fillStyle = TINT[d.vibe] || C.rule; ctx.fillRect(x, y - 30, 10, dense ? 150 : 180); }
        const tx = i === 0 && d.vibe ? x + 28 : x, tw = cw - (tx - x);
        font(ctx, `600 ${dense ? 22 : 24}px ${FU}`, 3.5); ctx.fillStyle = C.soft;
        ctx.fillText(wrap(ctx, k, tw, 1)[0], tx, y);
        let vs = dense ? 50 : 60;
        font(ctx, `600 ${vs}px ${FD}`, -1);
        while (ctx.measureText(v).width > tw && vs > 34) { vs -= 2; font(ctx, `600 ${vs}px ${FD}`, -1); }
        ctx.fillStyle = C.ink;
        ctx.fillText(wrap(ctx, v, tw, 1)[0], tx, y + (dense ? 62 : 74));
        if (s) {
          const ss = dense ? 25 : 28;
          font(ctx, `400 ${ss}px ${FU}`); ctx.fillStyle = C.soft;
          wrap(ctx, s, tw, 2).forEach((l, j) => ctx.fillText(l, tx, y + (dense ? 104 : 122) + j * ss * 1.3));
        }
      });
      y += (dense ? 172 : 240) + gx;
    }

    // the best night, a slip taped onto the page
    if (d.best_title) {
      const ts = dense ? 52 : 64;
      font(ctx, `600 ${ts}px ${FD}`, -1);
      const lines = wrap(ctx, d.best_title, W - 2 * M - 280, 2);
      const sh = (dense ? 140 : 196) + lines.length * ts * 1.12;
      ctx.save();
      ctx.translate(W / 2, y + sh / 2); ctx.rotate(0.012); ctx.translate(-W / 2, -(y + sh / 2));
      ctx.shadowColor = "rgba(43,36,51,.22)"; ctx.shadowBlur = 30; ctx.shadowOffsetY = 12;
      ctx.fillStyle = C.slip; ctx.fillRect(M - 8, y, W - 2 * M + 16, sh);
      ctx.shadowColor = "transparent";
      tape(ctx, M + 40, y - 18, 170, 40, -0.07);
      const px = M + 36;
      let ty = y + (dense ? 64 : 80);
      font(ctx, `600 ${dense ? 22 : 24}px ${FU}`, 3.5); ctx.fillStyle = C.soft;
      ctx.fillText("OUR BEST NIGHT", px, ty);
      font(ctx, `600 ${ts}px ${FD}`, -1); ctx.fillStyle = C.ink;
      lines.forEach((l) => { ty += ts * 1.12; ctx.fillText(l, px, ty); });
      ty += dense ? 26 : 34;
      const ss = dense ? 40 : 48;
      for (let i = 1; i <= 5; i++) star(ctx, px + (i - 1) * (ss + 8), ty, ss, i <= d.best_rating, (i % 2 ? -1 : 1) * .07);
      rubber(ctx, W - M - (dense ? 104 : 124), y + (dense ? 96 : 112), dense ? .85 : 1);
      ctx.restore();
      y += sh + (dense ? 56 : 70) + gx;
    }

    // opt-in extras: where we went, what we spent
    if (d.spent) {
      font(ctx, `600 ${dense ? 22 : 24}px ${FU}`, 3.5); ctx.fillStyle = C.soft;
      ctx.fillText("WHAT IT CAME TO", M, y);
      if (dense) {
        const lw = ctx.measureText("WHAT IT CAME TO").width;
        font(ctx, `600 40px ${FD}`, -1); ctx.fillStyle = C.ink;
        ctx.fillText(`${d.spent} on dates`, M + lw + 22, y + 4);
        y += 64;
      } else {
        font(ctx, `600 52px ${FD}`, -1); ctx.fillStyle = C.ink;
        ctx.fillText(`${d.spent} on dates`, M, y + 64);
        y += 124;
      }
    }
    if (d.places && d.places.length) {
      font(ctx, `600 ${dense ? 22 : 24}px ${FU}`, 3.5); ctx.fillStyle = C.soft;
      ctx.fillText("WHERE WE WENT", M, y);
      const ps = dense ? 30 : 34;
      font(ctx, `400 ${ps}px ${FU}`); ctx.fillStyle = C.ink;
      const room = Math.max(1, Math.floor((H - 250 - y - (dense ? 440 : 0)) / (ps * 1.4)));
      const lines = wrap(ctx, d.places.join("  ·  "), W - 2 * M, Math.min(dense ? 2 : 4, room));
      lines.forEach((l, i) => ctx.fillText(l, M, y + 50 + i * ps * 1.4));
      y += 50 + lines.length * ps * 1.4 + 30;
    }

    // photos, as polaroids, in whatever room is left above the sign-off
    const end = y;
    if (photos.length) {
      // a loose pile of polaroids: one or two rows, overlapping when there are many
      const n = photos.length, rows = n <= 3 ? 1 : 2, per = Math.ceil(n / rows);
      const areaTop = y + 6, areaH = H - 200 - areaTop;
      const pw = Math.max(120, Math.min(n === 1 ? 420 : n <= 3 ? 300 : 340, areaH / (rows * 1.45 - (rows - 1) * .3)));
      const fh = pw * 1.45, rowStep = fh * .7;
      const usedH = fh + (rows - 1) * rowStep, y0 = areaTop + Math.max(0, (areaH - usedH) / 2);
      photos.forEach((b, i) => {
        const r = Math.floor(i / per), c = i % per, inRow = Math.min(per, n - r * per);
        const step = inRow > 1 ? Math.min(pw + 28, (W - 2 * M - pw) / (inRow - 1)) : 0;
        const rowW = pw + step * (inRow - 1), x = (W - rowW) / 2 + c * step + (r % 2 ? step * .18 : 0) * (inRow < per ? 1 : 0);
        const yy = y0 + r * rowStep + ((c * 53) % 3) * 8;
        const pad = pw * .06, iw = pw - 2 * pad, ih = iw * 1.25;
        ctx.save();
        ctx.translate(x + pw / 2, yy + fh / 2); ctx.rotate((((i * 37) % 5) - 2) * .03); ctx.translate(-pw / 2, -fh / 2);
        ctx.shadowColor = "rgba(43,36,51,.28)"; ctx.shadowBlur = 26; ctx.shadowOffsetY = 10;
        ctx.fillStyle = C.slip; ctx.fillRect(0, 0, pw, fh);
        ctx.shadowColor = "transparent";
        // cover-crop: the middle of the photo at the tile's 4:5
        const sr = b.width / b.height, tr = iw / ih;
        let sw = b.width, sh2 = b.height, sx = 0, sy = 0;
        if (sr > tr) { sw = b.height * tr; sx = (b.width - sw) / 2; } else { sh2 = b.width / tr; sy = (b.height - sh2) / 2; }
        ctx.drawImage(b, sx, sy, sw, sh2, pad, pad, iw, ih);
        ctx.restore();
      });
    }

    // sign-off: no names, no address
    font(ctx, `600 ${dense ? 50 : 58}px ${FH}`);
    ctx.fillStyle = C.soft;
    ctx.fillText("one jar, just us", M, H - 104);
    ctx.save();
    ctx.translate(W - M - 52, H - 150); ctx.rotate(.12); ctx.scale(1.7, 1.7);
    ctx.fillStyle = C.stamp; ctx.fill(new Path2D(HEART));
    ctx.restore();
    return end;
  };
  // a little "BEST NIGHT" rubber stamp with a double border
  const rubber = (ctx, cx, cy, k) => {
    const c = document.createElement("canvas"), w = 190, h = 104;
    c.width = w; c.height = h;
    const g = c.getContext("2d");
    g.strokeStyle = g.fillStyle = C.stamp;
    g.lineWidth = 5; g.strokeRect(6, 6, w - 12, h - 12);
    g.lineWidth = 2; g.strokeRect(14, 14, w - 28, h - 28);
    font(g, `700 26px ${FU}`, 5); g.textAlign = "center";
    g.fillText("BEST", w / 2 + 2, 48); g.fillText("NIGHT", w / 2 + 2, 78);
    const rnd = rng(77);
    g.globalCompositeOperation = "destination-out";
    for (let i = 0; i < 160; i++) { g.globalAlpha = .3 + rnd() * .6; g.beginPath(); g.arc(rnd() * w, rnd() * h, rnd() * 2.6 + .5, 0, Math.PI * 2); g.fill(); }
    ctx.save(); ctx.translate(cx, cy); ctx.rotate(.14); ctx.scale(k, k); ctx.globalAlpha = .92;
    ctx.drawImage(c, -w / 2, -h / 2); ctx.restore();
    c.width = c.height = 0;
  };
})();
