// Buttons and chips squash when pressed and bounce back when let go.
import { springs, to } from './motion';

const SEL = '.btn, .chip, .pill, .share, .takeout-btn, .k-btn, .m-page-a, .y-make, .keepsake, .top nav a, .top nav button, .group-index a';

export function initPress() {
  let held: HTMLElement | null = null;
  const up = () => {
    if (!held) return;
    to(held, { s: 1, config: springs.boing });
    held = null;
  };
  document.addEventListener('pointerdown', (e) => {
    const el = (e.target as Element).closest?.<HTMLElement>(SEL);
    if (!el || (el as HTMLButtonElement).disabled) return;
    held = el;
    to(el, { s: 0.93, config: springs.press });
  }, { passive: true });
  addEventListener('pointerup', up);
  addEventListener('pointercancel', up);
  document.addEventListener('pointerout', (e) => { if (held && e.target === held && !held.contains(e.relatedTarget as Node)) up(); });
}
