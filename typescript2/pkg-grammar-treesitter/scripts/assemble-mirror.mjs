#!/usr/bin/env node
// Assemble the full contents of the BoundaryML/baml-treesitter mirror repo:
// the tree-sitter grammar plus the generated parser (src/), which consumers
// (nvim-treesitter, Zed, Helix) compile directly from the repo. Same contract
// as pkg-grammar/scripts/assemble-mirror.mjs: the output directory is the
// complete desired state of the mirror; the sync-grammar-mirror workflow
// rsyncs it over a mirror checkout with --delete.
//
// Usage:
//   node scripts/assemble-mirror.mjs --out <dir>
//
// Runs `tree-sitter generate` first, so the emitted src/parser.c always
// matches grammar.js.

import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(here, '..');
const repoRoot = resolve(pkgRoot, '../..');

const { values } = parseArgs({
  options: {
    out: { type: 'string' },
  },
});

if (!values.out) {
  console.error('usage: assemble-mirror.mjs --out <dir>');
  process.exit(1);
}

// Regenerate the parser from grammar.js. The generated src/ is gitignored in
// the monorepo but committed in the mirror (that is what consumers build).
execFileSync(resolve(pkgRoot, 'node_modules/.bin/tree-sitter'), ['generate'], {
  cwd: pkgRoot,
  stdio: 'inherit',
});
if (!existsSync(resolve(pkgRoot, 'src/parser.c'))) {
  console.error('tree-sitter generate did not produce src/parser.c');
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
mkdirSync(out, { recursive: true });

// Grammar sources and the generated parser.
cpSync(resolve(pkgRoot, 'grammar.js'), resolve(out, 'grammar.js'));
cpSync(resolve(pkgRoot, 'tree-sitter.json'), resolve(out, 'tree-sitter.json'));
cpSync(resolve(pkgRoot, 'queries'), resolve(out, 'queries'), { recursive: true });
cpSync(resolve(pkgRoot, 'src'), resolve(out, 'src'), { recursive: true });
cpSync(resolve(pkgRoot, 'test'), resolve(out, 'test'), { recursive: true });

// Keep the generated parser from drowning out real changes in mirror diffs
// and language stats.
writeFileSync(resolve(out, '.gitattributes'), 'src/** linguist-generated=true\n');

// Repo scaffolding. The package.json is private: the mirror is consumed as a
// git repo (nvim-treesitter revisions, Zed grammar pins), not from npm.
cpSync(resolve(repoRoot, 'LICENSE'), resolve(out, 'LICENSE'));
cpSync(resolve(pkgRoot, 'mirror/README.md'), resolve(out, 'README.md'));
writeFileSync(
  resolve(out, 'package.json'),
  `${JSON.stringify(
    {
      name: '@boundaryml/baml-treesitter',
      version: '0.0.0-dev',
      private: true,
      description: 'tree-sitter grammar for BAML (read-only mirror of typescript2/pkg-grammar-treesitter in BoundaryML/baml)',
      license: 'Apache-2.0',
      repository: 'github:BoundaryML/baml-treesitter',
    },
    null,
    2,
  )}\n`,
);

console.log(`assembled mirror at ${out}`);
