#!/usr/bin/env node
// Assemble the full contents of the BoundaryML/textMate-baml mirror repo,
// which is both the npm package @boundaryml/baml-grammar (published by the
// mirror's own publish workflow) and the grammar source GitHub Linguist
// vendors for .baml highlighting on github.com.
//
// The output directory is the complete desired state of the mirror: the
// sync-grammar-mirror workflow rsyncs it over a mirror checkout with --delete
// (excluding .git), so anything not emitted here gets removed from the mirror.
//
// Usage:
//   node scripts/assemble-mirror.mjs --out <dir> [--version <semver>]
//
// Requires a prior `pnpm --filter @b/pkg-grammar build` (dist/ must exist).
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

const distDir = resolve(pkgRoot, 'dist');
if (!existsSync(resolve(distDir, 'index.js'))) {
  console.error('dist/index.js missing; run `pnpm --filter @b/pkg-grammar build` first');
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
mkdirSync(resolve(out, 'grammars'), { recursive: true });
mkdirSync(resolve(out, '.github/workflows'), { recursive: true });

// Package artifacts.
cpSync(distDir, resolve(out, 'dist'), { recursive: true });
cpSync(resolve(pkgRoot, 'baml.tmLanguage.json'), resolve(out, 'grammars/baml.tmLanguage.json'));
cpSync(resolve(pkgRoot, 'language-configuration.json'), resolve(out, 'language-configuration.json'));

// Derived grammar formats. The mirror paths are frozen API — bat/Package
// Control consume grammars/baml.sublime-syntax, and the KDE upstream PR
// tracks syntaxes/baml.xml.
cpSync(resolve(pkgRoot, 'baml.sublime-syntax'), resolve(out, 'grammars/baml.sublime-syntax'));
mkdirSync(resolve(out, 'syntaxes'), { recursive: true });
cpSync(resolve(pkgRoot, 'syntaxes/baml.xml'), resolve(out, 'syntaxes/baml.xml'));

// Canonical sample: the fixture every grammar port validates against, at a
// stable path registries (Shiki samples/, hljs demos) can reference.
mkdirSync(resolve(out, 'samples'), { recursive: true });
cpSync(
  resolve(pkgRoot, 'tests/fixtures/showcase__golden_sample.baml'),
  resolve(out, 'samples/baml.sample'),
);

// Repo scaffolding. Linguist's license check needs LICENSE; publish.yml is the
// mirror's own npm release workflow.
cpSync(resolve(repoRoot, 'LICENSE'), resolve(out, 'LICENSE'));
cpSync(resolve(pkgRoot, 'mirror/README.md'), resolve(out, 'README.md'));
cpSync(resolve(pkgRoot, 'mirror/SUPPORT.md'), resolve(out, 'SUPPORT.md'));
cpSync(resolve(pkgRoot, 'mirror/publish.yml'), resolve(out, '.github/workflows/publish.yml'));

// package.json, with the version stamped in.
const pkg = JSON.parse(readFileSync(resolve(pkgRoot, 'mirror/package.json'), 'utf8'));
if (values.version) {
  pkg.version = values.version;
}
writeFileSync(resolve(out, 'package.json'), `${JSON.stringify(pkg, null, 2)}\n`);

console.log(`assembled mirror at ${out} (version ${pkg.version})`);
