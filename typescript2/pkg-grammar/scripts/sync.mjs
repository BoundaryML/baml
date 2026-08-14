#!/usr/bin/env node
// Keep app-vscode-ext's committed grammar copies in lockstep with the canonical
// grammars in @b/pkg-grammar.
//
// Why a mirror exists at all: VS Code loads a grammar from the physical file
// named in `contributes.grammars[].path`, and app-vscode-ext is bundled with
// tsup (no node_modules shipped in the .vsix), so the extension can't point at
// @b/pkg-grammar at runtime — it needs the JSON committed under syntaxes/.
// Same story for language-configuration.json (`contributes.languages[].configuration`).
// app-promptfiddle has no such mirror; it imports the package directly.
//
// Usage:
//   node scripts/sync.mjs           Copy canonical files into the mirror.
//   node scripts/sync.mjs --check   Exit non-zero if the mirror has drifted
//                                    (used by the pre-commit hook / CI).

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = resolve(here, '..');
const extRoot = resolve(pkgRoot, '../app-vscode-ext');

// canonical path (relative to pkg-grammar) -> mirror path (relative to app-vscode-ext)
const MIRRORED = [
  ['baml.tmLanguage.json', 'syntaxes/baml.tmLanguage.json'],
  ['language-configuration.json', 'language-configuration.json'],
];

const check = process.argv.includes('--check');
let drifted = false;

for (const [name, mirrorRel] of MIRRORED) {
  const canonical = readFileSync(resolve(pkgRoot, name), 'utf8');
  const mirrorPath = resolve(extRoot, mirrorRel);

  if (check) {
    let mirror = null;
    try {
      mirror = readFileSync(mirrorPath, 'utf8');
    } catch {
      // missing mirror counts as drift
    }
    if (mirror !== canonical) {
      drifted = true;
      console.error(`✗ ${name}: app-vscode-ext/${mirrorRel} is out of sync with @b/pkg-grammar`);
    }
  } else {
    writeFileSync(mirrorPath, canonical);
    console.log(`✓ synced ${name} → app-vscode-ext/${mirrorRel}`);
  }
}

if (check && drifted) {
  console.error('\nThese files live in typescript2/pkg-grammar. Run:\n  pnpm --filter @b/pkg-grammar sync\nthen commit the regenerated copies in app-vscode-ext.');
  process.exit(1);
}
