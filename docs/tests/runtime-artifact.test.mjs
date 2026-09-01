import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  packageRuntimeArtifact,
  validateRuntimeManifest,
  verifyRuntimeFiles,
} from '../scripts/runtime-artifact.mjs';

async function fixture(version = '1.2.3') {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'baml-runtime-artifact-'));
  const inputRoot = path.join(temporaryRoot, 'input');
  const outputRoot = path.join(temporaryRoot, 'output');
  await mkdir(inputRoot, { recursive: true });
  await writeFile(path.join(inputRoot, 'package.json'), '{"type":"module"}\n');
  await writeFile(path.join(inputRoot, 'bridge_wasm.js'), [
    'export default async () => {};',
    `export const version = () => ${JSON.stringify(version)};`,
    `export const commitHash = () => ${JSON.stringify('a'.repeat(7))};`,
    'export const getBuildTime = () => "1788224998";',
    '',
  ].join('\n'));
  await writeFile(path.join(inputRoot, 'bridge_wasm_bg.wasm'), Buffer.from('deterministic wasm fixture'));
  return { inputRoot, outputRoot };
}

test('packages a release runtime under its content digest', async () => {
  const { inputRoot, outputRoot } = await fixture();
  const manifest = await packageRuntimeArtifact({
    inputRoot,
    outputRoot,
    sourceRevision: 'a'.repeat(40),
    toolchain: 'baml-cli 1.2.3',
    version: '1.2.3',
  });
  assert.equal(validateRuntimeManifest(manifest, { version: '1.2.3', sourceRevision: 'a'.repeat(40) }), manifest);
  assert.match(manifest.module.path, /^runtime\/[0-9a-f]{16}\/bridge_wasm\.js$/);
  assert.equal(path.dirname(manifest.module.path), path.dirname(manifest.wasm.path));
  await assert.doesNotReject(verifyRuntimeFiles(manifest, outputRoot));
  assert.deepEqual(JSON.parse(await readFile(path.join(outputRoot, 'runtime.json'), 'utf8')), manifest);
});

test('rejects provenance mismatches and tampered release files', async () => {
  const { inputRoot, outputRoot } = await fixture('1.2.2');
  await assert.rejects(
    packageRuntimeArtifact({
      inputRoot,
      outputRoot,
      sourceRevision: 'a'.repeat(40),
      toolchain: 'baml-cli 1.2.3',
      version: '1.2.3',
    }),
    /Runtime version does not match/,
  );

  const valid = await fixture();
  const manifest = await packageRuntimeArtifact({
    ...valid,
    sourceRevision: 'a'.repeat(40),
    toolchain: 'baml-cli 1.2.3',
    version: '1.2.3',
  });
  await writeFile(path.join(valid.outputRoot, manifest.wasm.path), Buffer.from('tampered'));
  await assert.rejects(verifyRuntimeFiles(manifest, valid.outputRoot), /WASM size does not match/);

  const unsafe = structuredClone(manifest);
  unsafe.module.path = '../bridge_wasm.js';
  assert.throws(() => validateRuntimeManifest(unsafe), /safe content-addressed path/);
});
