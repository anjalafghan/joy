// Springs on server-rendered elements. Each element gets one Controller holding its pose; we paint
// the pose with the individual transform properties (translate / rotate / scale), so it stacks on
// top of whatever transform the page's own CSS and keyframes already use instead of fighting them.
import { Controller, Globals, type SpringConfig } from '@react-spring/web';

export const RM = matchMedia('(prefers-reduced-motion: reduce)');
export const reduced = () => RM.matches;
// Reduced motion: every spring jumps straight to where it was going.
const sync = () => Globals.assign({ skipAnimation: RM.matches });
sync();
RM.addEventListener('change', sync);

export type Pose = { x: number; y: number; r: number; ry: number; s: number; o: number };
const REST: Pose = { x: 0, y: 0, r: 0, ry: 0, s: 1, o: 1 };
type El = HTMLElement | SVGElement;
const ctrls = new WeakMap<El, Controller<Pose>>();

export const springs = {
  snap: { tension: 380, friction: 18 } as SpringConfig, // back where it was, with a little wobble
  soft: { tension: 220, friction: 20 } as SpringConfig, // things arriving
  quick: { tension: 420, friction: 32 } as SpringConfig, // things leaving, no overshoot
  press: { tension: 650, friction: 30 } as SpringConfig,
  boing: { tension: 520, friction: 11 } as SpringConfig,
};

function paint(el: El, p: Pose) {
  const st = el.style;
  const near = (a: number, b: number, e = 0.05) => Math.abs(a - b) < e;
  if (near(p.x, 0) && near(p.y, 0)) st.removeProperty('translate');
  else st.translate = `${p.x}px ${p.y}px`;
  if (!near(p.ry, 0, 0.01)) st.rotate = `y ${p.ry}deg`;
  else if (!near(p.r, 0, 0.01)) st.rotate = `${p.r}deg`;
  else st.removeProperty('rotate');
  if (near(p.s, 1, 0.001)) st.removeProperty('scale');
  else st.scale = `${p.s}`;
  if (p.o > 0.999) st.removeProperty('opacity');
  else st.opacity = `${Math.max(0, p.o)}`;
}

export function pose(el: El): Controller<Pose> {
  let c = ctrls.get(el);
  if (!c) {
    c = new Controller<Pose>({ ...REST });
    ctrls.set(el, c);
  }
  return c;
}

/** Jump there now (dragging, setting up an entrance). */
export function set(el: El, p: Partial<Pose>) {
  const c = pose(el);
  c.set(p);
  paint(el, c.get());
}

type Go = Partial<Pose> & { config?: SpringConfig; delay?: number };
/** Spring there. Resolves when it comes to rest. */
export function to(el: El, p: Go): Promise<unknown> {
  const c = pose(el);
  const { config = springs.soft, delay, ...vals } = p;
  return c.start({ ...vals, config, delay, onChange: () => paint(el, c.get()) }).then(() => paint(el, c.get()));
}

export const cssVar = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
export const buzz = (ms: number) => { try { navigator.vibrate?.(ms); } catch { /* no buzz */ } };
