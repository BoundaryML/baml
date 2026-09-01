import { createHash } from 'node:crypto';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { validateRuntimeManifest } from './runtime-artifact.mjs';

function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function fetchResponse(url) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
      if (!response.ok) throw new Error(`${url} returned ${response.status} ${response.statusText}`);
      return response;
    } catch (error) {
      lastError = error;
      if (attempt < 3) console.warn(`Runtime fetch attempt ${attempt} failed: ${error.message}`);
    }
  }
  throw lastError;
}

function validateBytes(bytes, file, label) {
  if (bytes.length !== file.rawBytes) throw new Error(`Runtime ${label} size does not match its manifest`);
  if (digest(bytes) !== file.sha256) throw new Error(`Runtime ${label} checksum does not match its manifest`);
}

export async function readRuntimeSource(source, expected = {}) {
  let manifest;
  let root;
  if (source.type === 'file') {
    const manifestPath = path.resolve(source.value);
    manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    root = path.dirname(manifestPath);
  } else if (source.type === 'url') {
    manifest = await (await fetchResponse(source.value)).json();
    root = new URL('.', source.value);
  } else {
    throw new Error(`Unsupported runtime source: ${source.type}`);
  }
  validateRuntimeManifest(manifest, expected);
  if (source.payloadSha256 && manifest.payloadSha256 !== source.payloadSha256) {
    throw new Error(`Runtime index checksum for BAML ${manifest.version} does not match its immutable artifact`);
  }

  const readArtifact = async (file) => source.type === 'file'
    ? readFile(path.resolve(root, file.path))
    : Buffer.from(await (await fetchResponse(new URL(file.path, root))).arrayBuffer());
  const [moduleBytes, wasmBytes] = await Promise.all([
    readArtifact(manifest.module),
    readArtifact(manifest.wasm),
  ]);
  validateBytes(moduleBytes, manifest.module, 'module');
  validateBytes(wasmBytes, manifest.wasm, 'WASM');
  return { manifest, moduleBytes, wasmBytes };
}

export async function materializeRuntime({ loaded, publicRoot }) {
  const { manifest, moduleBytes, wasmBytes } = loaded;
  const artifactName = manifest.wasm.sha256.slice(0, 16);
  const artifactRoot = path.join(publicRoot, 'artifacts', artifactName);
  await rm(path.join(publicRoot, 'artifacts'), { recursive: true, force: true });
  await mkdir(artifactRoot, { recursive: true });
  await writeFile(path.join(artifactRoot, 'bridge_wasm.js'), moduleBytes);
  await writeFile(path.join(artifactRoot, 'bridge_wasm_bg.wasm'), wasmBytes);
  const servingManifest = {
    schemaVersion: 1,
    runtimeVersion: manifest.runtimeVersion,
    runtimeCommit: manifest.runtimeCommit,
    runtimeBuildTimeUnix: manifest.runtimeBuildTimeUnix,
    sourceCommit: manifest.sourceRevision,
    toolchain: manifest.toolchain,
    sha256: manifest.wasm.sha256,
    moduleSha256: manifest.module.sha256,
    rawBytes: manifest.wasm.rawBytes,
    gzipBytes: manifest.wasm.gzipBytes,
    brotliBytes: manifest.wasm.brotliBytes,
    module: `/baml-runtime/artifacts/${artifactName}/bridge_wasm.js`,
    wasm: `/baml-runtime/artifacts/${artifactName}/bridge_wasm_bg.wasm`,
  };
  await writeFile(path.join(publicRoot, 'manifest.json'), `${JSON.stringify(servingManifest, null, 2)}\n`);
  return servingManifest;
}

export async function clearMaterializedRuntime(publicRoot) {
  await Promise.all([
    rm(path.join(publicRoot, 'artifacts'), { recursive: true, force: true }),
    rm(path.join(publicRoot, 'manifest.json'), { force: true }),
  ]);
}
