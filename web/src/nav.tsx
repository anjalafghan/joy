// The ink line under the current page in the nav. It follows your finger or pointer to other
// pages like a pen that can't quite keep up (the leading end runs ahead, the tail catches up),
// and on a new page it slides over from where it was on the last one.
import { useEffect, useMemo } from 'react';
import { createRoot } from 'react-dom/client';
import { animated, to, useSpring } from '@react-spring/web';

// Which nav item each page belongs to.
const HOME: Record<string, string> = { jar: 'jar', slips: 'slips', kept: 'kept', year: 'kept', month: 'kept', who: 'who' };
const KEY = 'joy.nav-ink';
const fast = { tension: 420, friction: 30 }, slow = { tension: 170, friction: 24 };

function Ink({ nav }: { nav: HTMLElement }) {
  const links = useMemo(() => Array.from(nav.querySelectorAll<HTMLAnchorElement>('a[data-nav]')), [nav]);
  const home = links.find((a) => a.dataset.nav === HOME[document.body.dataset.page ?? '']) ?? null;
  const edges = (a: HTMLElement) => {
    const n = nav.getBoundingClientRect(), r = a.getBoundingClientRect();
    return { l: r.left - n.left + 10, r: r.right - n.left - 10 };
  };
  const saved = (() => { try { return JSON.parse(sessionStorage.getItem(KEY) || 'null') as { l: number; r: number } | null; } catch { return null; } })();

  const [ink, api] = useSpring(() => {
    const h = home ? edges(home) : null;
    const from = saved ?? h ?? { l: 0, r: 0 };
    return { from: { ...from, o: saved || h ? 1 : 0 }, to: { ...(h ?? from), o: h ? 1 : 0 }, config: slow };
  });

  useEffect(() => {
    const go = (a: HTMLElement | null) => {
      if (!a) { api.start({ o: 0 }); return; }
      const e = edges(a);
      const right = e.l > ink.l.get();
      // whichever end is heading the way we're going leads
      api.start({ ...e, o: 1, config: (k) => (k === (right ? 'r' : 'l') ? fast : slow) });
    };
    const over = (ev: Event) => { const a = (ev.target as Element).closest<HTMLAnchorElement>('a[data-nav]'); if (a) go(a); };
    const back = () => go(home);
    const leave = (ev: MouseEvent) => {
      const a = (ev.target as Element).closest<HTMLAnchorElement>('a[data-nav]');
      if (!a) return;
      try { sessionStorage.setItem(KEY, JSON.stringify(edges(a))); } catch { /* fine */ }
    };
    const out = (ev: FocusEvent) => { if (!nav.contains(ev.relatedTarget as Node)) back(); };
    const resize = () => { if (home) api.start({ ...edges(home), immediate: true }); };
    nav.addEventListener('pointerover', over);
    nav.addEventListener('pointerleave', back);
    nav.addEventListener('focusin', over);
    nav.addEventListener('focusout', out);
    nav.addEventListener('click', leave);
    addEventListener('resize', resize);
    document.fonts?.ready.then(resize);
    try { sessionStorage.removeItem(KEY); } catch { /* fine */ }
    return () => {
      nav.removeEventListener('pointerover', over);
      nav.removeEventListener('pointerleave', back);
      nav.removeEventListener('focusin', over);
      nav.removeEventListener('focusout', out);
      nav.removeEventListener('click', leave);
      removeEventListener('resize', resize);
    };
  }, [nav, home, api, ink]);

  return (
    <animated.svg
      className="m-ink"
      viewBox="0 0 60 6"
      preserveAspectRatio="none"
      style={{ left: ink.l, width: to([ink.l, ink.r], (l, r) => Math.max(0, r - l)), opacity: ink.o }}
    >
      <path d="M1 4.2C12 2.1 25 1.6 35 2.6s17 1.5 24-.4" />
    </animated.svg>
  );
}

export function mountNav() {
  const nav = document.querySelector<HTMLElement>('.top nav');
  if (!nav) return;
  const host = document.createElement('span');
  host.className = 'm-ink-host';
  host.setAttribute('aria-hidden', 'true');
  nav.appendChild(host);
  nav.classList.add('m-nav');
  createRoot(host).render(<Ink nav={nav} />);
}
