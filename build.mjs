// Bundles src/ into dist/ (what Tauri ships). Usage: node build.mjs
import { build } from 'esbuild';
import { copyFile, mkdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, 'src');
const out = join(here, 'dist');

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
await build({
  entryPoints: [join(src, 'main.js')],
  outfile: join(out, 'app.js'),
  bundle: true,
  minify: true,
  format: 'iife',
  target: ['chrome120'],
  legalComments: 'none',
});
await build({
  entryPoints: [join(src, 'app.css')],
  outfile: join(out, 'app.css'),
  minify: true,
});
await copyFile(join(src, 'index.html'), join(out, 'index.html'));
console.log('built dist/ (index.html, app.js, app.css)');
