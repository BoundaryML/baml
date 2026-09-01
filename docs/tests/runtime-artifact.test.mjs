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
import { materializeRuntime, readRuntimeSource } from '../scripts/runtime-consumer.mjs';

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

test('materializes the selected immutable release as the same-origin worker runtime', async () => {
  const { inputRoot, outputRoot } = await fixture();
  const publicRoot = path.join(path.dirname(outputRoot), 'public-runtime');
  const release = await packageRuntimeArtifact({
    inputRoot,
    outputRoot,
    sourceRevision: 'a'.repeat(40),
    toolchain: 'baml-cli 1.2.3',
    version: '1.2.3',
  });
  const loaded = await readRuntimeSource({ type: 'file', value: path.join(outputRoot, 'runtime.json') }, {
    version: '1.2.3',
    sourceRevision: 'a'.repeat(40),
  });
  const serving = await materializeRuntime({ loaded, publicRoot });
  assert.equal(serving.runtimeVersion, release.version);
  assert.equal(serving.sourceCommit, release.sourceRevision);
  assert.match(serving.wasm, /^\/baml-runtime\/artifacts\/[0-9a-f]{16}\/bridge_wasm_bg\.wasm$/);
  assert.equal(
    (await readFile(path.join(publicRoot, serving.wasm.replace('/baml-runtime/', '')))).length,
    release.wasm.rawBytes,
  );

  const mismatched = structuredClone(release);
  mismatched.sourceRevision = 'b'.repeat(40);
  assert.throws(
    () => validateRuntimeManifest(mismatched, { version: '1.2.3', sourceRevision: 'a'.repeat(40) }),
    /does not match docs metadata/,
  );
});

test('pull request previews pass the exact source-built runtime into the TypeScript-only deploy', async () => {
  const workflow = await readFile(path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../.github/workflows/developer-docs.yml'), 'utf8');
  assert.match(workflow, /wasm-pack build[\s\S]*baml_language\/crates\/bridge_wasm/);
  assert.match(workflow, /produce:runner-artifact/);
  assert.match(workflow, /BAML_DOCS_RUNTIME_FILE=\$runtime_root\/runtime\.json/);
  assert.match(workflow, /\$\{\{ env\.BAML_DOCS_RUNTIME_ROOT \}\}/);
  assert.match(workflow, /BAML_DOCS_RUNTIME_FILE="\$runtime_manifest"[\s\S]*vercel build/);
});
