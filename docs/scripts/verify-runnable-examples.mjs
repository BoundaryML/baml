import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createSession } from '../lib/baml-runner/driver.mjs';
import { runnableExamples } from '../lib/baml-runner/examples.mjs';
import { formatValue } from '../lib/baml-runner/outbound.mjs';
import { BamlVfs } from '../lib/baml-runner/vfs.mjs';
import { resolveRuntimePath, verifyRuntimeFiles } from './runtime-artifact.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(packageRoot, '..');
const publicRoot = path.join(packageRoot, 'public');
const args = process.argv.slice(2);
const manifestIndex = args.indexOf('--manifest');
const manifestPath = manifestIndex === -1
  ? path.join(publicRoot, 'baml-runtime/manifest.json')
  : path.resolve(args[manifestIndex + 1]);
const manifest = JSON.parse(
  await readFile(manifestPath, 'utf8'),
);
const releaseArtifact = manifest.kind === 'baml.docs-runtime';
if (releaseArtifact) await verifyRuntimeFiles(manifest, path.dirname(manifestPath));
const runtimeModule = releaseArtifact
  ? resolveRuntimePath(path.dirname(manifestPath), manifest.module.path)
  : path.join(publicRoot, manifest.module.slice(1));
const runtimeWasm = releaseArtifact
  ? resolveRuntimePath(path.dirname(manifestPath), manifest.wasm.path)
  : path.join(publicRoot, manifest.wasm.slice(1));
const wasm = await import(pathToFileURL(runtimeModule).href);
await wasm.default({ module_or_path: await readFile(runtimeWasm) });

const bamlBinary =
  process.env.BAML_BIN ??
  path.join(repositoryRoot, 'baml_language/target/debug/baml-cli');

for (const example of runnableExamples) {
  const directory = await mkdtemp(path.join(os.tmpdir(), `baml-docs-${example.id}-`));
  let session;
  try {
    for (const [relativePath, contents] of Object.entries(example.files)) {
      const destination = path.join(directory, relativePath);
      await mkdir(path.dirname(destination), { recursive: true });
      await writeFile(destination, contents);
    }

    const native = spawnSync(
      bamlBinary,
      ['run', example.functionName, '--directory', directory, '--no-progress'],
      { encoding: 'utf8' },
    );
    if (native.status !== 0) {
      throw new Error(`native run failed for ${example.id}:\n${native.stderr}`);
    }
    const nativeOutput = native.stdout.trim();
    if (nativeOutput !== example.expected) {
      throw new Error(
        `${example.id}: expected ${example.expected}, native CLI returned ${nativeOutput}`,
      );
    }

    session = await createSession(wasm, BamlVfs, example.files, {
      root: `/verify/${example.id}`,
    });
    const result = await session.run(example.functionName);
    if (result.status !== 'succeeded') {
      throw new Error(
        `${example.id}: WASM returned ${result.status}: ${result.error?.message ?? ''}`,
      );
    }
    const wasmOutput = formatValue(result.value);
    if (wasmOutput !== nativeOutput) {
      throw new Error(
        `${example.id}: WASM returned ${wasmOutput}, native CLI returned ${nativeOutput}`,
      );
    }
    console.log(`ok — ${example.id}: ${wasmOutput}`);
  } finally {
    try {
      session?.free();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
}
