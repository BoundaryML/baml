// Run are-the-types-wrong over the packed package, minus the native addon.
// A local build leaves a 630 MB debug .node in dist/ that attw never loads
// but `attw --pack` gzips: 59.4s with it, 4.2s without, same output. The
// published tarball has no .node either (napi artifacts ships binaries as
// per-platform sub-packages), so the staged copy matches what npm installs.
// Staged rather than moved aside: fixture suites load the real addon
// concurrently.
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

// Run attw's JS entrypoint under this node: no shell, no platform-specific
// node_modules/.bin shim (this suite runs on Windows too).
const attwDir = path.join(packageRoot, 'node_modules', '@arethetypeswrong', 'cli');
const attwManifest = JSON.parse(fs.readFileSync(path.join(attwDir, 'package.json'), 'utf8'));
const attwBin = attwManifest.bin;
const attwEntry = path.resolve(attwDir, typeof attwBin === 'string' ? attwBin : attwBin.attw);

const stagingDir = fs.mkdtempSync(path.join(os.tmpdir(), 'baml-bridge-attw-'));
try {
  // Everything `files: ["dist"]` would pack, minus the addon; the manifest
  // is copied verbatim so attw reads the real exports/types.
  fs.cpSync(distDir, path.join(stagingDir, 'dist'), {
    recursive: true,
    filter: (source) => !source.endsWith('.node'),
  });
  fs.copyFileSync(path.join(packageRoot, 'package.json'), path.join(stagingDir, 'package.json'));

  // Same argv as a bare `attw --pack`, pointed at the staging copy.
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
