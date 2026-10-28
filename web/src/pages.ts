// Moving between pages: the page lifts away as you leave and settles in piece by piece as you
// arrive. Month spreads turn like pages instead.
import { reduced, set, springs, to } from './motion';

const TURN = 'joy.turn';
const store = (k: string, v?: string) => { try { v === undefined ? sessionStorage.removeItem(k) : sessionStorage.setItem(k, v); } catch { /* fine */ } };
const recall = (k: string) => { try { return sessionStorage.getItem(k); } catch { return null; } };

// The page's big pieces: main's children, or its one wrapper's children.
function blocks(main: HTMLElement): HTMLElement[] {
  let kids = Array.from(main.children) as HTMLElement[];
  if (kids.length === 1 && kids[0].children.length) kids = Array.from(kids[0].children) as HTMLElement[];
  return kids.filter((el) => {
    if (el.matches('script, template, svg[width="0"], .visually-hidden')) return false;
    const r = el.getBoundingClientRect();
    return r.height > 0 && r.top < innerHeight;
  }).slice(0, 8);
}

export function initPages() {
  const root = document.documentElement;
  const main = document.getElementById('main');
  const page = document.body.dataset.page ?? '';
  // Arrived too late to animate without a flash (slow network): just show the page.
  const late = performance.now() > 1200;
  if (!main || page === 'login' || reduced() || late) { root.classList.remove('m-in'); store(TURN); return; }

  const turn = recall(TURN);
  store(TURN);
  const sheet = page === 'month' ? document.querySelector<HTMLElement>('.m-sheet') : null;
  const pieces = blocks(main).filter((el) => !(turn && el === sheet));
  pieces.forEach((el) => set(el, { o: 0, y: 16 }));
  if (turn && sheet) {
    sheet.style.transformOrigin = turn === 'next' ? '100% 50%' : '0 50%';
    set(sheet, { ry: turn === 'next' ? -55 : 55, o: 0 });
  }
  root.classList.remove('m-in');
  pieces.forEach((el, i) => to(el, { o: 1, y: 0, delay: i * 40, config: { tension: 300, friction: 26 } }));
  if (turn && sheet) {
    to(sheet, { ry: 0, o: 1, delay: 60, config: { tension: 170, friction: 22 } })
      .then(() => sheet.style.removeProperty('transform-origin'));
  }

  // Leaving: window-level, so any page script that handles the click itself goes first.
  addEventListener('click', (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const a = (e.target as Element).closest?.<HTMLAnchorElement>('a[href]');
    if (!a || a.target || a.hasAttribute('download') || a.closest('[data-no-leave]')) return;
    const url = new URL(a.href, location.href);
    if (url.origin !== location.origin || url.pathname.startsWith('/static/') || url.pathname.endsWith('.ics')) return;
    if (url.pathname === location.pathname && url.search === location.search) return; // a jump within the page
    if (reduced()) return;
    e.preventDefault();
    const go = () => location.assign(url.href);
    const flip = a.closest('.m-pages') && sheet ? (a.classList.contains('m-next') ? 'next' : 'prev') : null;
    if (flip && sheet) {
      store(TURN, flip);
      sheet.style.transformOrigin = flip === 'next' ? '0 50%' : '100% 50%';
      to(sheet, { ry: flip === 'next' ? 70 : -70, o: 0.2, config: springs.quick });
    } else {
      blocks(main).forEach((el, i) => to(el, { o: 0, y: -10, delay: i * 20, config: springs.quick }));
    }
    setTimeout(go, flip ? 220 : 150);
  });
}
