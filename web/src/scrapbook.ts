// The year and month pages: the count runs up, the year's slips drop into the jar, the month's
// letter stickers press on one by one, and prints, tickets and receipts can be picked up and
// played with. Taped down, so they spring back when let go.
import { SpringValue } from '@react-spring/web';
import { draggable } from './drag';
import { buzz, pose, reduced, set, springs, to } from './motion';

function pickUp(sel: string) {
  draggable({
    sel,
    axis: 'free',
    start: (d) => {
      // picked up before it had finished landing: it's in your hand now
      pose(d.el).stop();
      set(d.el, { o: 1 });
      d.el.classList.add('m-lifted');
      buzz(8);
      to(d.el, { s: 1.06, config: springs.press });
    },
    move: (d) => set(d.el, { x: d.dx, y: d.dy, r: d.dx / 14 }),
    end: (d) => {
      to(d.el, { x: 0, y: 0, r: 0, s: 1, config: { tension: 170, friction: 11 } })
        .then(() => d.el.classList.remove('m-lifted'));
    },
  });
}

export function initYear() {
  // the big number counts up to the year's dates
  const count = document.querySelector<HTMLElement>('.y-count');
  const num = count?.firstChild;
  const n = num && num.nodeType === Node.TEXT_NODE ? parseInt(num.textContent || '', 10) : NaN;
  if (num && n > 1 && !reduced()) {
    num.textContent = '0';
    const v = new SpringValue(0);
    v.start({ to: n, delay: 250, config: { tension: 60, friction: 22 }, onChange: () => { num.textContent = String(Math.round(v.get())); } })
      .then(() => { num.textContent = String(n); });
  }

  // a slip per date drops in through the jar's mouth
  document.querySelectorAll<SVGGElement>('.y-jar-slip').forEach((g, i) => {
    set(g, { y: -230, r: (i % 2 ? 1 : -1) * (14 + (i * 7) % 20) });
    to(g, { y: 0, r: 0, delay: 450 + i * 110, config: { tension: 190, friction: 13 } });
  });

  pickUp('.y-photos > .y-polaroid, .y-sway');
}

export function initMonth() {
  document.querySelectorAll<HTMLElement>('.m-stickers span').forEach((s, i) => {
    set(s, { s: 0, r: (i % 2 ? 1 : -1) * 28 });
    to(s, { s: 1, r: 0, delay: 180 + i * 55, config: { tension: 420, friction: 13 } });
  });
  pickUp('.m-piece');
}
