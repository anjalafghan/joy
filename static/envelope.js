// The envelope: tap the seal, the flap lifts, the letter rises, and the date unfolds below.
(() => {
  'use strict';
  document.documentElement.classList.add('js');
  const env = document.getElementById('env');
  const seal = document.getElementById('env-open');
  const letter = document.getElementById('letter');
  const hint = document.getElementById('env-hint');
  if (!env || !seal || !letter) return;
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  seal.addEventListener('click', () => {
    env.classList.add('is-open');
    seal.setAttribute('aria-expanded', 'true');
    try { navigator.vibrate && navigator.vibrate(10); } catch (_) {}
    if (hint) hint.textContent = '';
    setTimeout(() => {
      letter.classList.add('is-out');
      letter.scrollIntoView({ behavior: still ? 'auto' : 'smooth', block: 'start' });
      const t = letter.querySelector('h1');
      if (t) { t.tabIndex = -1; t.focus({ preventScroll: true }); }
    }, still ? 0 : 1150);
  }, { once: true });
})();
