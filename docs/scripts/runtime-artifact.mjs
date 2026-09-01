import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

export const DOCS_RUNTIME_KIND = 'baml.docs-runtime';
export const DOCS_RUNTIME_SCHEMA_VERSION = 1;
export const MAX_RUNTIME_GZIP_BYTES = 5_000_000;

export function sha256Bytes(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

export function runtimeChecksumPayload(manifest) {
  return {
    kind: manifest.kind,
    schemaVersion: manifest.schemaVersion,
    version: manifest.version,
    sourceRevision: manifest.sourceRevision,
    runtimeVersion: manifest.runtimeVersion,
    runtimeCommit: manifest.runtimeCommit,
    runtimeBuildTimeUnix: manifest.runtimeBuildTimeUnix,
    toolchain: manifest.toolchain,
    module: manifest.module,
    wasm: manifest.wasm,
  };
}

export function runtimePayloadSha256(manifest) {
  return sha256Bytes(JSON.stringify(runtimeChecksumPayload(manifest)));
}

function validateFile(file, label, suffix) {
  if (!file || typeof file !== 'object') throw new Error(`Runtime ${label} metadata must be an object`);
  if (!/^runtime\/[0-9a-f]{16}\/[a-zA-Z0-9._-]+$/.test(file.path) || !file.path.endsWith(suffix)) {
    throw new Error(`Runtime ${label} path is not a safe content-addressed path: ${file.path}`);
  }
  if (!/^[0-9a-f]{64}$/.test(file.sha256)) throw new Error(`Runtime ${label} checksum is invalid`);
  if (!Number.isSafeInteger(file.rawBytes) || file.rawBytes <= 0) throw new Error(`Runtime ${label} size is invalid`);
}

export function validateRuntimeManifest(manifest, expected = {}) {
  if (!manifest || typeof manifest !== 'object') throw new Error('Runtime manifest must be an object');
  if (manifest.kind !== DOCS_RUNTIME_KIND || manifest.schemaVersion !== DOCS_RUNTIME_SCHEMA_VERSION) {
    throw new Error('Unsupported docs runtime manifest');
  }
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(manifest.version)) {
    throw new Error(`Runtime manifest version is invalid: ${manifest.version}`);
  }
  if (expected.version && manifest.version !== expected.version) {
    throw new Error(`Runtime manifest has version ${manifest.version}, expected ${expected.version}`);
  }
  if (!/^[0-9a-f]{40}$/.test(manifest.sourceRevision)) throw new Error('Runtime source revision is invalid');
  if (expected.sourceRevision && manifest.sourceRevision !== expected.sourceRevision) {
    throw new Error(`Runtime source revision ${manifest.sourceRevision} does not match docs metadata ${expected.sourceRevision}`);
  }
  if (manifest.runtimeVersion !== manifest.version) throw new Error('Runtime version does not match its release version');
  if (!/^[0-9a-f]{7,40}$/.test(manifest.runtimeCommit) || !manifest.sourceRevision.startsWith(manifest.runtimeCommit)) {
    throw new Error('Runtime commit does not match its source revision');
  }
  if (!/^\d+$/.test(String(manifest.runtimeBuildTimeUnix))) throw new Error('Runtime build time is invalid');
  if (manifest.toolchain !== `baml-cli ${manifest.version}`) throw new Error('Runtime toolchain does not match its release version');
  validateFile(manifest.module, 'module', '/bridge_wasm.js');
  validateFile(manifest.wasm, 'WASM', '/bridge_wasm_bg.wasm');
  const artifactRoot = `runtime/${manifest.wasm.sha256.slice(0, 16)}`;
  if (manifest.module.path !== `${artifactRoot}/bridge_wasm.js` || manifest.wasm.path !== `${artifactRoot}/bridge_wasm_bg.wasm`) {
    throw new Error('Runtime files do not share the WASM content-addressed directory');
  }
  if (!Number.isSafeInteger(manifest.wasm.gzipBytes) || manifest.wasm.gzipBytes <= 0) throw new Error('Runtime gzip size is invalid');
  if (!Number.isSafeInteger(manifest.wasm.brotliBytes) || manifest.wasm.brotliBytes <= 0) throw new Error('Runtime Brotli size is invalid');
  if (manifest.wasm.gzipBytes > MAX_RUNTIME_GZIP_BYTES) {
    throw new Error(`Runtime gzip size is ${manifest.wasm.gzipBytes} bytes; budget is ${MAX_RUNTIME_GZIP_BYTES}`);
  }
  if (manifest.payloadSha256 !== runtimePayloadSha256(manifest)) throw new Error('Runtime manifest payload checksum does not match');
  return manifest;
}

export function resolveRuntimePath(root, relativePath) {
  const resolved = path.resolve(root, relativePath);
  if (resolved !== root && !resolved.startsWith(`${path.resolve(root)}${path.sep}`)) {
    throw new Error(`Runtime artifact escapes its root: ${relativePath}`);
  }
  return resolved;
}

export async function verifyRuntimeFiles(manifest, root) {
  validateRuntimeManifest(manifest);
  for (const [label, file] of [['module', manifest.module], ['WASM', manifest.wasm]]) {
    const bytes = await readFile(resolveRuntimePath(root, file.path));
    if (bytes.length !== file.rawBytes) throw new Error(`Runtime ${label} size does not match its manifest`);
    if (sha256Bytes(bytes) !== file.sha256) throw new Error(`Runtime ${label} checksum does not match its manifest`);
  }
  const wasmBytes = await readFile(resolveRuntimePath(root, manifest.wasm.path));
  if (gzipSync(wasmBytes, { level: 9 }).length !== manifest.wasm.gzipBytes) throw new Error('Runtime gzip size does not match its manifest');
  if (brotliCompressSync(wasmBytes, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length !== manifest.wasm.brotliBytes) {
    throw new Error('Runtime Brotli size does not match its manifest');
  }
  return manifest;
}

export async function packageRuntimeArtifact({ inputRoot, outputRoot, sourceRevision, toolchain, version }) {
  if (!/^[0-9a-f]{40}$/.test(sourceRevision)) throw new Error(`Invalid source revision: ${sourceRevision}`);
  const moduleInput = path.join(inputRoot, 'bridge_wasm.js');
  const wasmInput = path.join(inputRoot, 'bridge_wasm_bg.wasm');
  const moduleBytes = await readFile(moduleInput);
  const wasmBytes = await readFile(wasmInput);
  const wasmSha256 = sha256Bytes(wasmBytes);
  const artifactRoot = path.posix.join('runtime', wasmSha256.slice(0, 16));

  const runtime = await import(`${pathToFileURL(moduleInput).href}?artifact=${wasmSha256}`);
  await runtime.default({ module_or_path: wasmBytes });
  const manifest = {
    kind: DOCS_RUNTIME_KIND,
    schemaVersion: DOCS_RUNTIME_SCHEMA_VERSION,
    version,
    sourceRevision,
    runtimeVersion: runtime.version(),
    runtimeCommit: runtime.commitHash(),
    runtimeBuildTimeUnix: runtime.getBuildTime(),
    toolchain,
    module: {
      path: path.posix.join(artifactRoot, 'bridge_wasm.js'),
      sha256: sha256Bytes(moduleBytes),
      rawBytes: moduleBytes.length,
    },
    wasm: {
      path: path.posix.join(artifactRoot, 'bridge_wasm_bg.wasm'),
      sha256: wasmSha256,
      rawBytes: wasmBytes.length,
      gzipBytes: gzipSync(wasmBytes, { level: 9 }).length,
      brotliBytes: brotliCompressSync(wasmBytes, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length,
    },
  };
  manifest.payloadSha256 = runtimePayloadSha256(manifest);
  validateRuntimeManifest(manifest, { version, sourceRevision });

  const artifactOutput = path.join(outputRoot, artifactRoot);
  await mkdir(artifactOutput, { recursive: true });
  await copyFile(moduleInput, path.join(artifactOutput, 'bridge_wasm.js'));
  await copyFile(wasmInput, path.join(artifactOutput, 'bridge_wasm_bg.wasm'));
  await writeFile(path.join(outputRoot, 'runtime.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  await verifyRuntimeFiles(manifest, outputRoot);
  return manifest;
}
