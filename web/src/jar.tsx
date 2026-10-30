// The jar page, on top of jar.js (which still runs the draw, put back and stamp):
// - swipe the drawn slip right for "It's a date", left to put it back
// - grab the jar and shake it side to side to draw
// - the stamp throws hearts and paper, and the page takes the thump
import { createRoot, type Root } from 'react-dom/client';
import { animated, type SpringValue } from '@react-spring/web';
import { draggable } from './drag';
import { burst } from './layer';
import { buzz, clamp, cssVar, pose, reduced, set, springs, tints, to } from './motion';

const YES = 110; // px of swipe that commits

type JarApi = { jostle?: (k: number) => void };
const jarApi = () => (window as unknown as { joyJar?: JarApi }).joyJar;
function Hints({ x, yes, back }: { x: SpringValue<number>; yes: boolean; back: boolean }) {
  return (
    <>
      {yes && (
        <animated.span className="m-hint m-hint-yes" style={{ opacity: x.to([20, YES], [0, 1], 'clamp'), scale: x.to([20, YES], [0.7, 1], 'clamp') }}>
          it's a date
        </animated.span>
      )}
      {back && (
        <animated.span className="m-hint m-hint-back" style={{ opacity: x.to([-YES, -20], [1, 0], 'clamp'), scale: x.to([-YES, -20], [1, 0.7], 'clamp') }}>
          put it back
        </animated.span>
      )}
    </>
  );
}

export function initJar() {
  const pick = document.getElementById('pick');
  const jar = document.getElementById('jar');
  const form = document.getElementById('tonight') as HTMLFormElement | null;
  const drawBtn = document.getElementById('draw') as HTMLButtonElement | null;
  if (!pick) return;

  const acceptForm = (el: Element) => el.querySelector<HTMLFormElement>('.slip-actions .f-accept');
  const backForm = (el: Element) => el.querySelector<HTMLFormElement>('.slip-actions .f-putback');
  const roots = new Map<Element, Root>();

  // Each drawn slip gets its two hint stamps, bound to the slip's own sideways spring.
  const dress = (el: HTMLElement) => {
    if (roots.has(el) || el.dataset.kind !== 'card') return;
    const yes = !!acceptForm(el), back = !!backForm(el);
    if (!yes && !back) return;
    const host = document.createElement('span');
    host.className = 'm-hints';
    host.setAttribute('aria-hidden', 'true');
    el.appendChild(host);
    const root = createRoot(host);
    root.render(<Hints x={pose(el).springs.x} yes={yes} back={back} />);
    roots.set(el, root);
  };
  document.addEventListener('htmx:beforeSwap', (e) => {
    if ((e as CustomEvent).detail.target !== pick) return;
    roots.forEach((r) => r.unmount());
    roots.clear();
  });

  // Show once or twice how the slip swipes, the first times one arrives.
  const TAUGHT = 'joy.swipe-taught';
  const taught = () => { try { return Number(localStorage.getItem(TAUGHT) || 0); } catch { return 2; } };
  document.addEventListener('joy:card', (e) => {
    const el = (e as CustomEvent<HTMLElement>).detail;
    dress(el);
    if (reduced() || taught() >= 2 || !roots.has(el)) return;
    try { localStorage.setItem(TAUGHT, String(taught() + 1)); } catch { /* fine */ }
    setTimeout(async () => {
      if (!el.isConnected || document.documentElement.classList.contains('m-dragging')) return;
      await to(el, { x: 46, r: 2.4, config: { tension: 200, friction: 16 } });
      await to(el, { x: backForm(el) ? -34 : 0, r: backForm(el) ? -1.8 : 0, config: { tension: 200, friction: 16 } });
      to(el, { x: 0, r: 0, config: springs.snap });
    }, 650);
  });

  draggable({
    sel: '#pick > .slip[data-kind="card"]',
    axis: 'x',
    ok: (el) => !el.classList.contains('is-arriving') && pick.getAttribute('aria-busy') !== 'true',
    start: (d) => { dress(d.el); pose(d.el).stop(); },
    move: (d) => {
      const can = d.dx > 0 ? !!acceptForm(d.el) : !!backForm(d.el);
      const x = can ? d.dx : d.dx * 0.2;
      set(d.el, { x, y: Math.abs(x) * 0.05, r: x / 22 });
    },
    end: (d, cancelled) => {
      const x = pose(d.el).springs.x.get();
      const yes = acceptForm(d.el), back = backForm(d.el);
      if (!cancelled && yes && (x > YES || (x > 50 && d.vx > 0.5))) {
        to(d.el, { x: 0, y: 0, r: 0, config: springs.snap });
        yes.requestSubmit();
      } else if (!cancelled && back && (x < -YES || (x < -50 && d.vx < -0.5))) {
        // jar.js folds it from right here back into the jar
        to(d.el, { x: Math.max(x, -150), r: -6, config: springs.quick });
        back.requestSubmit();
      } else {
        to(d.el, { x: 0, y: 0, r: 0, config: springs.snap });
      }
    },
  });

  // Grab the jar and shake it: a few quick back-and-forths and it gives up a slip.
  if (jar && form && drawBtn) {
    let flips = 0, dir = 0, last = 0;
    draggable({
      sel: '#jar',
      axis: 'x',
      ok: () => !drawBtn.disabled,
      start: () => { flips = 0; dir = 0; last = performance.now(); },
      move: (d) => {
        const x = clamp(d.dx * 0.45, -38, 38);
        set(jar, { x, r: x / 3 });
        const now = performance.now();
        const s = Math.sign(d.vx);
        if (Math.abs(d.vx) > 0.35 && s !== 0 && s !== dir) {
          if (dir !== 0) {
            flips = now - last < 600 ? flips + 1 : 1;
            last = now;
            jarApi()?.jostle?.(0.8);
            buzz(6);
          }
          dir = s;
        }
      },
      end: () => {
        to(jar, { x: 0, r: 0, config: springs.boing });
        if (flips >= 3 && !drawBtn.disabled) form.requestSubmit(drawBtn);
      },
    });
  }

  // The stamp lands: hearts and confetti fly, the page jolts.
  document.addEventListener('joy:stamp', (e) => {
    const { x, y } = (e as CustomEvent<{ x: number; y: number }>).detail;
    burst({ x, y, kind: 'hearts', colors: [cssVar('--stamp')], n: 9, spread: 150 });
    burst({ x, y, kind: 'confetti', colors: tints(), n: 22, spread: 190, fall: 160 });
    const main = document.getElementById('main');
    if (main && !reduced()) { set(main, { y: 6 }); to(main, { y: 0, config: springs.boing }); }
  });

  // A slip unfolding out of the jar shakes a little paper dust off.
  document.addEventListener('joy:card', (e) => {
    const r = (e as CustomEvent<HTMLElement>).detail.getBoundingClientRect();
    burst({ x: r.left + r.width / 2, y: r.top + 8, kind: 'ink', colors: tints(), n: 10, spread: 70, around: true, fall: 40 });
  });
}
