// Cards, slips and keepsakes land on the page as they scroll into view, a few at a time, each
// from its own slight angle, like they're being dealt.
import { reduced, set, to } from './motion';

const PAGES: Record<string, { sel: string; deal?: boolean }> = {
  slips: { sel: '.stack > .slip' },
  kept: { sel: '.k-slip' },
  month: { sel: '.m-piece', deal: true },
  year: { sel: '.y-photos > .y-polaroid', deal: true },
};

// Same angle every visit: a stable number from 0 to 1 per index.
const seed = (i: number) => { const v = Math.sin(i * 12.9898 + 4.1) * 43758.5453; return v - Math.floor(v); };

export function initReveal() {
  const cfg = PAGES[document.body.dataset.page ?? ''];
  if (!cfg || reduced()) return;
  const items = Array.from(document.querySelectorAll<HTMLElement>(cfg.sel));
  if (!items.length) return;
  document.documentElement.classList.add('m-reveal'); // switches off the CSS entrances these replace

  let queue: HTMLElement[] = [];
  let raf = 0;
  const flush = () => {
    raf = 0;
    queue.sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
    queue.forEach((el, i) => to(el, { o: 1, x: 0, y: 0, r: 0, s: 1, delay: Math.min(i, 8) * 50, config: { tension: 260, friction: 20 } }));
    queue = [];
  };
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      io.unobserve(e.target);
      queue.push(e.target as HTMLElement);
    }
    if (queue.length && !raf) raf = requestAnimationFrame(flush);
  }, { rootMargin: '0px 0px -5% 0px' });

  items.forEach((el, i) => {
    const k = seed(i);
    set(el, { o: 0, y: cfg.deal ? 40 : 22, x: cfg.deal ? (k - 0.5) * 90 : 0, r: (k - 0.5) * (cfg.deal ? 14 : 5), s: 0.96 });
    io.observe(el);
  });
}
