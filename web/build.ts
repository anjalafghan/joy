// Bundles src/main.tsx into ../static/motion.js. React's API comes from preact/compat: same
// components and hooks, about a fifth of the size. `bun run build`, or `bun run dev` to watch.
import type { BunPlugin } from 'bun';

const compat: BunPlugin = {
  name: 'preact-compat',
  setup(b) {
    const to = (p: string) => ({ path: Bun.resolveSync(p, import.meta.dir) });
    b.onResolve({ filter: /^react-dom\/client$/ }, () => to('preact/compat/client'));
    b.onResolve({ filter: /^react(-dom)?$/ }, () => to('preact/compat'));
    b.onResolve({ filter: /^react\/jsx-(dev-)?runtime$/ }, () => to('preact/jsx-runtime'));
  },
};

const out = await Bun.build({
  entrypoints: ['src/main.tsx'],
  outdir: '../static',
  naming: 'motion.js',
  target: 'browser',
  format: 'esm',
  minify: true,
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [compat],
});
if (!out.success) {
  for (const l of out.logs) console.error(l);
  process.exit(1);
}
console.log(`static/motion.js  ${(out.outputs[0].size / 1024).toFixed(1)} KB`);
