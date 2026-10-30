// Kept, after a form comes back (each one reloads the page): note what was just done on submit,
// then play it once the page is back.
// - your line unseals theirs: their line unfolds onto the slip
// - money goes in the savings jar: the bar fills from where it was
// - the jar pays for its slip: the stamp thumps down and throws hearts and paper
import { Controller } from '@react-spring/web';
import { burst } from './layer';
import { buzz, cssVar, reduced, set, springs, tints, to } from './motion';

const store = (k: string, v?: string) => { try { v === undefined ? sessionStorage.removeItem(k) : sessionStorage.setItem(k, v); } catch { /* private mode */ } };
const recall = (k: string) => { try { const v = sessionStorage.getItem(k); sessionStorage.removeItem(k); return v; } catch { return null; } };

const LINE = 'm-line';
const BAR = 'm-savings-from';
const KEPT = 'm-savings-kept';

const bar = () => document.querySelector<HTMLElement>('.k-savings .budget-bar');
const pct = (el: HTMLElement | null) => Number(el?.style.getPropertyValue('--pct') || 0);
const centre = (el: Element) => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
// After the slip has been dealt in (reveal.ts) and the page has scrolled to it.
const settled = (fn: () => void) => setTimeout(fn, 420);

export function initKept() {
  document.addEventListener('submit', (e) => {
    const form = e.target as HTMLFormElement;
    if (form.matches('.k-line-form')) {
      const slip = form.closest('.k-slip');
      // Only worth a moment if theirs is still sealed now.
      if (slip && !slip.querySelector('.k-line-theirs')) store(LINE, slip.id);
    }
    if (form.closest('.k-budget')) store(BAR, String(pct(bar())));
    if (form.getAttribute('action') === '/savings/keep') store(KEPT, '1');
  }, true);

  const line = recall(LINE);
  const from = recall(BAR);
  const kept = recall(KEPT);
  if (reduced()) return;

  if (line) {
    const theirs = document.querySelector<HTMLElement>(`#${CSS.escape(line)} .k-line-theirs`);
    if (theirs) {
      set(theirs, { o: 0, y: -12, r: -2.5, s: 0.94 });
      settled(() => {
        to(theirs, { o: 1, y: 0, r: 0, s: 1, config: springs.soft });
        const { x, y } = centre(theirs);
        burst({ x, y, kind: 'ink', colors: tints(), n: 8, spread: 60, around: true, fall: 30 });
        buzz(10);
      });
    }
  }

  const b = bar();
  if (from !== null && b && Number(from) !== pct(b)) {
    const end = pct(b);
    b.classList.add('m-filling'); // the CSS width transition would trail the spring
    b.style.setProperty('--pct', from);
    const c = new Controller({ p: Number(from) });
    settled(() => c.start({
      p: end,
      config: { tension: 120, friction: 18 },
      onChange: () => b.style.setProperty('--pct', c.springs.p.get().toFixed(2)),
    }).then(() => { b.style.setProperty('--pct', String(end)); b.classList.remove('m-filling'); }));
  }

  if (kept) {
    const slip = location.hash.startsWith('#k') ? document.getElementById(location.hash.slice(1)) : null;
    const stamp = slip?.querySelector<HTMLElement>('.k-stamp');
    if (stamp) {
      set(stamp, { o: 0, s: 1.8, r: -10 });
      // Ink is either on the page or not: only the size and angle bounce.
      settled(() => { set(stamp, { o: 1 }); to(stamp, { s: 1, r: 0, config: springs.boing }); });
      setTimeout(() => {
        const { x, y } = centre(stamp);
        burst({ x, y, kind: 'hearts', colors: [cssVar('--stamp')], n: 9, spread: 150 });
        burst({ x, y, kind: 'confetti', colors: tints(), n: 22, spread: 190, fall: 160 });
        const main = document.getElementById('main');
        if (main) { set(main, { y: 6 }); to(main, { y: 0, config: springs.boing }); }
        buzz(12);
      }, 560);
    }
  }
}
