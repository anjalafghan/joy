// One pointer, one drag at a time, delegated from the document so it survives htmx swaps.
// axis 'x': a sideways swipe; vertical moves are left to the page so it still scrolls.
// axis 'free': pick something up. On touch that takes a short press first, so a plain
// swipe over it still scrolls the page.
import { reduced } from './motion';

export type Drag = { el: HTMLElement; dx: number; dy: number; vx: number; vy: number };
type Opts = {
  sel: string;
  axis: 'x' | 'free';
  hold?: number;
  ok?: (el: HTMLElement) => boolean;
  start?: (d: Drag) => void;
  move: (d: Drag) => void;
  end: (d: Drag, cancelled: boolean) => void;
};

// Starting on these is a tap on them, never a drag. A sideways swipe may start on a link (a
// slip's whole face often is one); the click at the end of it is swallowed below.
const INTERACTIVE = 'a, button, input, select, textarea, label, summary, [contenteditable], audio, video';
const FIELDS = 'button, input, select, textarea, label, summary, [contenteditable], audio, video';
const all: Opts[] = [];
let p: {
  o: Opts; el: HTMLElement; id: number; x0: number; y0: number; touch: boolean;
  started: boolean; timer: number; d: Drag; lx: number; ly: number; lt: number;
} | null = null;
let swallowUntil = 0;

export function draggable(o: Opts) {
  if (!all.length) listen();
  all.push(o);
}

function begin() {
  if (!p || p.started) return;
  p.started = true;
  clearTimeout(p.timer);
  document.documentElement.classList.add('m-dragging');
  try { p.el.setPointerCapture(p.id); } catch { /* already gone */ }
  p.o.start?.(p.d);
}

function finish(cancelled: boolean) {
  if (!p) return;
  clearTimeout(p.timer);
  if (p.started) {
    document.documentElement.classList.remove('m-dragging');
    swallowUntil = performance.now() + 350;
    p.o.end(p.d, cancelled);
  }
  p = null;
}

function listen() {
  document.addEventListener('pointerdown', (e) => {
    if (p || e.button !== 0 || !e.isPrimary || reduced()) return;
    const t = e.target as Element;
    for (const o of all) {
      const el = t.closest<HTMLElement>(o.sel);
      if (!el) continue;
      const hit = t.closest(o.axis === 'x' ? FIELDS : INTERACTIVE);
      if (hit && el.contains(hit)) return;
      if (o.ok && !o.ok(el)) return;
      const now = performance.now();
      p = {
        o, el, id: e.pointerId, x0: e.clientX, y0: e.clientY, touch: e.pointerType === 'touch',
        started: false, timer: 0, d: { el, dx: 0, dy: 0, vx: 0, vy: 0 }, lx: e.clientX, ly: e.clientY, lt: now,
      };
      if (o.axis === 'free' && p.touch) p.timer = window.setTimeout(begin, o.hold ?? 260);
      return;
    }
  }, { passive: true });

  addEventListener('pointermove', (e) => {
    if (!p || e.pointerId !== p.id) return;
    const now = performance.now();
    const dt = Math.max(1, now - p.lt);
    const d = p.d;
    d.vx = 0.7 * ((e.clientX - p.lx) / dt) + 0.3 * d.vx;
    d.vy = 0.7 * ((e.clientY - p.ly) / dt) + 0.3 * d.vy;
    p.lx = e.clientX; p.ly = e.clientY; p.lt = now;
    d.dx = e.clientX - p.x0;
    d.dy = e.clientY - p.y0;
    if (!p.started) {
      const ax = Math.abs(d.dx), ay = Math.abs(d.dy);
      if (p.o.axis === 'x') {
        if (ax > 8 && ax > ay * 1.25) begin();
        else if (ay > 10) { p = null; return; }
      } else if (p.touch) {
        if (ax + ay > 8) { clearTimeout(p.timer); p = null; return; } // that was a scroll
      } else if (ax + ay > 5) begin();
      if (!p?.started) return;
    }
    p.o.move(d);
  });

  addEventListener('pointerup', (e) => { if (p && e.pointerId === p.id) finish(false); });
  addEventListener('pointercancel', (e) => { if (p && e.pointerId === p.id) finish(true); });
  // Once something is picked up, the finger moves it, not the page.
  addEventListener('touchmove', (e) => { if (p?.started && e.cancelable) e.preventDefault(); }, { passive: false });
  // A drag that ends over a link or button is not a click on it.
  addEventListener('click', (e) => {
    if (e.isTrusted && performance.now() < swallowUntil) { e.preventDefault(); e.stopPropagation(); swallowUntil = 0; }
  }, true);
  addEventListener('contextmenu', (e) => { if (p?.started) e.preventDefault(); });
  // Not the browser's own drag of a link or a photo.
  addEventListener('dragstart', (e) => { if (p) e.preventDefault(); });
}
