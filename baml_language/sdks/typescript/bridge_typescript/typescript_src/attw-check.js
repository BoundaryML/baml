// attw-check.js - run are-the-types-wrong over the packed package, minus the
// native addon.
//
// `attw --pack` packs the package before analyzing it, and a local
// `pnpm build:debug` leaves a debug-profile `dist/baml_node.<triple>.node`
// behind: 630 MB of a 632 MB `dist/`. Gzipping it single-threaded is the whole
// cost of the check — 59.4s with the addon, 4.2s without, byte-identical
// output either way. attw resolves and type-checks the package entrypoints; it
// never loads the addon.
//
// Dropping it also matches what consumers actually install. `napi artifacts`
// moves each platform binary into its own `npm/<platform>/` sub-package and the
// umbrella package publishes `dist/` plus optionalDependencies pointing at
// those (see .github/workflows/publish2-nodejs-sdk.yaml), so the published
// tarball has no `.node` in it.
//
// The addon is excluded by staging a copy rather than by moving it aside: the
// sdk_tests harness runs this check concurrently with fixture vitest suites
// that do load the addon, so the real tree has to stay intact.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.join(__dirname, '..');
const distDir = path.join(packageRoot, 'dist');

if (!fs.existsSync(distDir)) {
  console.error('dist/ is missing; run `pnpm build:debug` first.');
  process.exit(1);
}

// `attw` is resolved to its JS entrypoint and run under this node, so the
// check needs neither a shell nor the platform-specific `node_modules/.bin`
// shim (setup.ps1 runs this suite on Windows too).
const attwDir = path.join(packageRoot, 'node_modules', '@arethetypeswrong', 'cli');
const attwManifest = JSON.parse(fs.readFileSync(path.join(attwDir, 'package.json'), 'utf8'));
const attwBin = attwManifest.bin;
const attwEntry = path.resolve(attwDir, typeof attwBin === 'string' ? attwBin : attwBin.attw);

const stagingDir = fs.mkdtempSync(path.join(os.tmpdir(), 'baml-bridge-attw-'));
try {
  // Everything `files: ["dist"]` would pack, minus the addon. The manifest is
  // copied verbatim so attw reads the same exports/types it would in place.
  fs.cpSync(distDir, path.join(stagingDir, 'dist'), {
    recursive: true,
    filter: (source) => !source.endsWith('.node'),
  });
  fs.copyFileSync(path.join(packageRoot, 'package.json'), path.join(stagingDir, 'package.json'));

  // Same argv as a bare `attw --pack`, just pointed at the staging copy, so
  // the check itself is unchanged.
  const result = spawnSync(
    process.execPath,
    [attwEntry, '--pack', '--profile', 'esm-only', ...process.argv.slice(2)],
    { cwd: stagingDir, stdio: 'inherit' },
  );
  if (result.error) {
    throw result.error;
  }
  process.exitCode = result.status ?? 1;
} finally {
  fs.rmSync(stagingDir, { recursive: true, force: true });
}
