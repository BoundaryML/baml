#!/usr/bin/env node
// Assemble the full contents of the BoundaryML/baml-highlightjs mirror repo,
// the npm package @boundaryml/baml-highlightjs (published by the mirror's own
// publish workflow). Same contract as pkg-grammar/scripts/assemble-mirror.mjs:
// the output directory is the complete desired state of the mirror; the
// sync-grammar-mirror workflow rsyncs it over a mirror checkout with --delete.
//
// Usage:
//   node scripts/assemble-mirror.mjs --out <dir> [--version <semver>]
//
// Without --version the template's 0.0.0-dev placeholder is kept; the sync
// workflow always stamps a real version.

import { cpSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(here, '..');
const repoRoot = resolve(pkgRoot, '../..');

const { values } = parseArgs({
  options: {
    out: { type: 'string' },
    version: { type: 'string' },
  },
});

if (!values.out) {
  console.error('usage: assemble-mirror.mjs --out <dir> [--version <semver>]');
  process.exit(1);
}
if (values.version && !/^\d+\.\d+\.\d+$/.test(values.version)) {
  console.error(`invalid --version: ${values.version}`);
  process.exit(1);
}

const out = resolve(process.cwd(), values.out);

// The rmSync below is recursive: refuse any --out that overlaps the repo
// checkout (`--out .`, a parent, or a subdirectory of the package) so a typo
// can never delete source files. Compare canonical paths — resolve symlinks
// via the nearest existing ancestor, so a link like /tmp/repo-link/pkg can't
// smuggle the checkout past a lexical check.
const canonical = (path) => {
  let existing = path;
  let remainder = '';
  while (!existsSync(existing)) {
    remainder = join(basename(existing), remainder);
    existing = dirname(existing);
  }
  return join(realpathSync(existing), remainder);
};
const overlaps = (a, b) => {
  const rel = relative(a, b);
  return rel === '' || (rel !== '..' && !rel.startsWith(`..${sep}`) && !isAbsolute(rel));
};
const realOut = canonical(out);
const realRepo = canonical(repoRoot);
if (overlaps(realRepo, realOut) || overlaps(realOut, realRepo)) {
  console.error(`--out must not overlap the repository: ${out}`);
  process.exit(1);
}
rmSync(out, { recursive: true, force: true });
mkdirSync(resolve(out, '.github/workflows'), { recursive: true });

// The language definition itself.
cpSync(resolve(pkgRoot, 'src'), resolve(out, 'src'), { recursive: true });

// Browser/CDN build: the ESM definition wrapped so a plain <script> tag
// self-registers the language on a global hljs (the convention highlight.js
// third-party language repos follow). Derived textually from src/baml.js so
// the mirror needs no bundler; fail loudly if the source shape changes.
const esm = readFileSync(resolve(pkgRoot, 'src/baml.js'), 'utf8');
// The wrapper below registers a binding literally named `baml`, so require
// exactly that export shape (not just any default function anywhere).
if (!/^export default function baml\(/m.test(esm)) {
  console.error('src/baml.js no longer `export default function baml(...)`; fix the dist wrapper');
  process.exit(1);
}
const iife = `// Generated from src/baml.js by assemble-mirror.mjs. Do not edit.
// Browser/CDN build: load after highlight.js and the language self-registers.
// Node/bundler consumers should import the package's ESM export instead.
(function () {
  ${esm.replace('export default function', 'function').trimEnd().split('\n').join('\n  ')}
  if (typeof globalThis !== 'undefined' && globalThis.hljs) {
    globalThis.hljs.registerLanguage('baml', baml);
  }
})();
`;
mkdirSync(resolve(out, 'dist'), { recursive: true });
writeFileSync(resolve(out, 'dist/baml.js'), iife);

// Repo scaffolding.
cpSync(resolve(repoRoot, 'LICENSE'), resolve(out, 'LICENSE'));
cpSync(resolve(pkgRoot, 'mirror/README.md'), resolve(out, 'README.md'));
cpSync(resolve(pkgRoot, 'mirror/publish.yml'), resolve(out, '.github/workflows/publish.yml'));

// package.json, with the version stamped in.
const pkg = JSON.parse(readFileSync(resolve(pkgRoot, 'mirror/package.json'), 'utf8'));
if (values.version) {
  pkg.version = values.version;
}
writeFileSync(resolve(out, 'package.json'), `${JSON.stringify(pkg, null, 2)}\n`);

console.log(`assembled mirror at ${out} (version ${pkg.version})`);
