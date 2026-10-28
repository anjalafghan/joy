// Slips: swipe a slip off to the left to take it out (slips.js still does the asking and the
// undo). Kept: rating a date throws a few hearts, more for more stars.
import { draggable } from './drag';
import { burst } from './layer';
import { cssVar, set, springs, to, pose } from './motion';

export function initLists() {
  draggable({
    sel: '.stack > .slip',
    axis: 'x',
    ok: (el) => !!el.querySelector('form.takeout'),
    start: (d) => pose(d.el).stop(),
    move: (d) => {
      const x = d.dx < 0 ? d.dx : d.dx * 0.15;
      set(d.el, { x, r: x / 40, o: 1 - Math.min(0.6, Math.abs(Math.min(0, x)) / 360) });
    },
    end: (d, cancelled) => {
      const x = pose(d.el).springs.x.get();
      const form = d.el.querySelector<HTMLFormElement>('form.takeout');
      if (!cancelled && form && (x < -120 || (x < -50 && d.vx < -0.5))) {
        to(d.el, { x: -innerWidth, r: -8, o: 0, config: springs.quick });
        form.requestSubmit();
      } else {
        to(d.el, { x: 0, r: 0, o: 1, config: springs.snap });
      }
    },
  });

  document.addEventListener('change', (e) => {
    const input = e.target as HTMLInputElement;
    if (!input.matches?.('.k-heart-in') || !input.checked || !input.value) return;
    const label = document.querySelector(`label[for="${CSS.escape(input.id)}"]`) ?? input.closest('label');
    if (!label) return;
    const r = label.getBoundingClientRect();
    const n = Number(input.value);
    burst({ x: r.left + r.width / 2, y: r.top + r.height / 2, kind: 'hearts', colors: [cssVar('--stamp')], n: 2 + n * 2, spread: 40 + n * 14, fall: 50 });
  });
}
