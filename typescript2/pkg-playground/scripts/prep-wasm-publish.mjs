#!/usr/bin/env node
/**
 * Stage the locally built `wasm/` artifacts into `wasm-publish/` as the
 * publishable npm package `@boundaryml/bridge-wasm`.
 *
 * Why a staging dir: wasm-pack regenerates `wasm/package.json` on every
 * `pnpm build:wasm` (as `@b/bridge_wasm`, a workspace-private name), so the
 * publish metadata can't live there. Local consumers keep their
 * `"@b/bridge_wasm": "link:../pkg-playground/wasm"` deps — pnpm places link:
 * deps by dependency key, so the published twin having a different inner name
 * is fine.
 *
 * Usage:
 *   pnpm build:wasm                       # make sure the artifact is fresh
 *   node scripts/prep-wasm-publish.mjs [version]
 *   cd wasm-publish && npm publish
 *
 * Without [version], a unique canary version is derived from the git HEAD:
 * 0.1.0-canary.<shortsha>. The chosen version is also written to
 * `wasm.version` (committed), which scripts/vercel-build.sh uses to restore
 * the artifact from npm instead of compiling Rust on Vercel.
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const wasmDir = path.join(pkgRoot, 'wasm');
const outDir = path.join(pkgRoot, 'wasm-publish');
const pinFile = path.join(pkgRoot, 'wasm.version');

const ARTIFACTS = [
  'bridge_wasm.js',
  'bridge_wasm.d.ts',
  'bridge_wasm_bg.wasm',
  'bridge_wasm_bg.wasm.d.ts',
];

const srcPkgPath = path.join(wasmDir, 'package.json');
if (!fs.existsSync(srcPkgPath)) {
  console.error('wasm/package.json not found — run `pnpm build:wasm` first.');
  process.exit(1);
}
for (const f of ARTIFACTS) {
  if (!fs.existsSync(path.join(wasmDir, f))) {
    console.error(`wasm/${f} missing — run \`pnpm build:wasm\` first.`);
    process.exit(1);
  }
}

const ageHours =
  (Date.now() - fs.statSync(path.join(wasmDir, 'bridge_wasm_bg.wasm')).mtimeMs) / 3.6e6;
if (ageHours > 24) {
  console.warn(
    `warning: bridge_wasm_bg.wasm was built ${Math.round(ageHours)}h ago — ` +
      'consider `pnpm build:wasm` for a fresh artifact.',
  );
}

const shortSha = execSync('git rev-parse --short HEAD', { cwd: pkgRoot })
  .toString()
  .trim();
const version = process.argv[2] ?? `0.1.0-canary.${shortSha}`;

const srcPkg = JSON.parse(fs.readFileSync(srcPkgPath, 'utf8'));
const outPkg = {
  ...srcPkg,
  name: '@boundaryml/bridge-wasm',
  version,
  description:
    'Prebuilt BAML bridge_wasm (wasm-pack --target web) — the playground runtime for web apps.',
  license: 'Apache-2.0',
  repository: {
    type: 'git',
    url: 'https://github.com/boundaryml/baml',
    directory: 'baml_language/crates/bridge_wasm',
  },
  files: ARTIFACTS,
  publishConfig: { access: 'public' },
};
// wasm-pack sometimes emits collaborators/scripts that don't belong on npm.
delete outPkg.collaborators;
delete outPkg.scripts;

fs.rmSync(outDir, { recursive: true, force: true });
fs.mkdirSync(outDir, { recursive: true });
for (const f of ARTIFACTS) {
  fs.copyFileSync(path.join(wasmDir, f), path.join(outDir, f));
}
fs.writeFileSync(
  path.join(outDir, 'package.json'),
  `${JSON.stringify(outPkg, null, 2)}\n`,
);
fs.writeFileSync(
  path.join(outDir, 'README.md'),
  [
    '# @boundaryml/bridge-wasm',
    '',
    'Prebuilt `bridge_wasm` artifact (wasm-pack `--target web`) from',
    'https://github.com/boundaryml/baml — published so web deploys can skip the',
    'Rust toolchain. Built from `baml_language/crates/bridge_wasm`.',
    '',
    `Built at commit \`${shortSha}\`.`,
    '',
  ].join('\n'),
);

fs.writeFileSync(pinFile, `${version}\n`);

console.log(`staged ${outDir.replace(`${process.cwd()}/`, '')} as @boundaryml/bridge-wasm@${version}`);
console.log(`pinned version in pkg-playground/wasm.version (commit this file)`);
console.log('');
console.log('to publish:');
console.log('  cd typescript2/pkg-playground/wasm-publish && npm publish');
