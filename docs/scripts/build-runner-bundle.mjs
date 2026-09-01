import { build } from 'esbuild';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(packageRoot, '..');
const check = process.argv.includes('--check');
const scratch = await mkdtemp(path.join(os.tmpdir(), 'baml-docs-runner-'));

const outputs = [
  {
    entry: path.join(repositoryRoot, 'typescript2/app-website/playground/vfs.ts'),
    output: path.join(packageRoot, 'lib/baml-runner/vfs.mjs'),
    temporary: path.join(scratch, 'vfs.mjs'),
    platform: 'neutral',
  },
  {
    entry: path.join(packageRoot, 'lib/baml-runner/worker.mjs'),
    output: path.join(packageRoot, 'public/baml-runtime/runner-worker.mjs'),
    temporary: path.join(scratch, 'runner-worker.mjs'),
    platform: 'browser',
  },
];

try {
  for (const item of outputs) {
    await build({
      bundle: true,
      entryPoints: [item.entry],
      format: 'esm',
      legalComments: 'none',
      minify: false,
      outfile: item.temporary,
      platform: item.platform,
      target: 'es2022',
    });

    const generated = await readFile(item.temporary);
    if (check) {
      let current;
      try {
        current = await readFile(item.output);
      } catch {
        throw new Error(`${path.relative(packageRoot, item.output)} is missing; run runner:bundle`);
      }
      if (!generated.equals(current)) {
        throw new Error(`${path.relative(packageRoot, item.output)} is stale; run runner:bundle`);
      }
    } else {
      await mkdir(path.dirname(item.output), { recursive: true });
      await writeFile(item.output, generated);
      console.log(`wrote ${path.relative(packageRoot, item.output)}`);
    }
  }
} finally {
  await rm(scratch, { recursive: true, force: true });
}
