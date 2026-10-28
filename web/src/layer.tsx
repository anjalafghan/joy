// A see-through layer over the whole page for things that fly: hearts off the stamp, paper
// confetti, ink flecks. Bursts are fire-and-forget; each one removes itself once it has fallen.
import { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { animated, easings, to, useSprings } from '@react-spring/web';
import { reduced } from './motion';

type Kind = 'hearts' | 'confetti' | 'ink';
export type Burst = {
  x: number; y: number; kind: Kind; colors: string[];
  n?: number; spread?: number; around?: boolean; fall?: number;
};
type Live = Burst & { id: number };

let push: ((b: Burst) => void) | null = null;
/** Throw some bits from (x, y), in viewport pixels. Nothing flies under reduced motion. */
export const burst = (b: Burst) => { if (!reduced()) push?.(b); };

const HEART = 'M8 14.2C4.6 11.6 1.7 9.2 1.8 6.1c.1-2.2 1.7-3.6 3.5-3.5 1.2 0 2.1.7 2.7 1.6.6-.9 1.6-1.7 2.9-1.6 1.9.1 3.4 1.6 3.3 3.7-.1 3-3.1 5.4-6.2 7.9z';

function Bits({ b, gone }: { b: Live; gone: () => void }) {
  const n = b.n ?? 14;
  const bits = useMemo(() => Array.from({ length: n }, (_, i) => {
    const a = b.around ? Math.random() * Math.PI * 2 : -Math.PI / 2 + (Math.random() - 0.5) * Math.PI * 1.25;
    const d = (b.spread ?? 110) * (0.45 + Math.random() * 0.75);
    const size = b.kind === 'ink' ? 3 + Math.random() * 5 : 8 + Math.random() * 9;
    return {
      dx: Math.cos(a) * d, dy: Math.sin(a) * d, spin: (Math.random() - 0.5) * 720, size,
      color: b.colors[i % b.colors.length], fall: (b.fall ?? 90) * (0.6 + Math.random() * 0.8),
      strip: b.kind === 'confetti' && i % 2 === 1,
    };
  }), [b, n]);

  const [springs] = useSprings(n, (i) => ({
    from: { x: 0, y: 0, r: 0, s: 0.2, o: 1 },
    to: async (next) => {
      const t = bits[i];
      await next({ x: t.dx, y: t.dy, r: t.spin * 0.4, s: 1, config: { tension: 300, friction: 20 } });
      await next({ x: t.dx * 1.15, y: t.dy + t.fall, r: t.spin, o: 0, config: { duration: 650, easing: easings.easeInQuad } });
    },
    delay: i * 12,
  }), []);

  useEffect(() => { const t = setTimeout(gone, 2200); return () => clearTimeout(t); }, [gone]);

  return <>{springs.map((s, i) => {
    const t = bits[i];
    return (
      <animated.span
        key={i}
        className={`m-bit m-bit-${b.kind}${t.strip ? ' is-strip' : ''}`}
        style={{
          left: b.x, top: b.y, width: t.size, height: t.strip ? t.size * 0.45 : t.size, color: t.color,
          opacity: s.o,
          transform: to([s.x, s.y, s.r, s.s], (x, y, r, k) => `translate(-50%, -50%) translate(${x}px, ${y}px) rotate(${r}deg) scale(${k})`),
        }}
      >
        {b.kind === 'hearts' && <svg viewBox="0 0 16 16"><path d={HEART} /></svg>}
      </animated.span>
    );
  })}</>;
}

function Layer() {
  const [live, setLive] = useState<Live[]>([]);
  useEffect(() => {
    let id = 0;
    push = (b) => setLive((l) => [...l.slice(-4), { ...b, id: ++id }]);
    return () => { push = null; };
  }, []);
  return <>{live.map((b) => <Bits key={b.id} b={b} gone={() => setLive((l) => l.filter((x) => x.id !== b.id))} />)}</>;
}

export function mountLayer() {
  const host = document.createElement('div');
  host.className = 'm-layer';
  host.setAttribute('aria-hidden', 'true');
  document.body.appendChild(host);
  createRoot(host).render(<Layer />);
}
