import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const publicRoot = path.join(packageRoot, 'public');
const maxGzipBytes = 5_000_000;
const manifest = JSON.parse(
  await readFile(path.join(publicRoot, 'baml-runtime/manifest.json'), 'utf8'),
);

function filePath(publicUrl) {
  if (typeof publicUrl !== 'string' || !publicUrl.startsWith('/baml-runtime/')) {
    throw new Error(`invalid runtime URL in manifest: ${publicUrl}`);
  }
  return path.join(publicRoot, publicUrl.slice(1));
}

function digest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

const wasm = await readFile(filePath(manifest.wasm));
const module = await readFile(filePath(manifest.module));
if (manifest.schemaVersion !== 1) {
  throw new Error(`unsupported runtime manifest schema: ${manifest.schemaVersion}`);
}
if (wasm.length !== manifest.rawBytes) {
  throw new Error(`runtime has ${wasm.length} bytes; manifest records ${manifest.rawBytes}`);
}
if (digest(wasm) !== manifest.sha256) {
  throw new Error('runtime digest does not match the manifest');
}
if (digest(module) !== manifest.moduleSha256) {
  throw new Error('runtime JavaScript digest does not match the manifest');
}
const gzipBytes = gzipSync(wasm, { level: 9 }).length;
if (gzipBytes !== manifest.gzipBytes) {
  throw new Error(`runtime gzip size is ${gzipBytes} bytes; manifest records ${manifest.gzipBytes}`);
}
if (gzipBytes > maxGzipBytes) {
  throw new Error(`runtime gzip size is ${gzipBytes} bytes; budget is ${maxGzipBytes}`);
}
if (!Number.isSafeInteger(manifest.brotliBytes) || manifest.brotliBytes <= 0) {
  throw new Error(`invalid runtime Brotli size: ${manifest.brotliBytes}`);
}
if (!/^\d+\.\d+\.\d+/.test(manifest.runtimeVersion)) {
  throw new Error(`invalid runtime version: ${manifest.runtimeVersion}`);
}
if (!/^[0-9a-f]{40}$/.test(manifest.sourceCommit)) {
  throw new Error(`invalid source commit: ${manifest.sourceCommit}`);
}
if (!/^[0-9a-f]{7,40}$/.test(manifest.runtimeCommit)) {
  throw new Error(`invalid runtime commit: ${manifest.runtimeCommit}`);
}
if (!manifest.sourceCommit.startsWith(manifest.runtimeCommit)) {
  throw new Error('runtime commit does not match the source commit');
}
if (manifest.toolchain !== `baml-cli ${manifest.runtimeVersion}`) {
  throw new Error(
    `runtime ${manifest.runtimeVersion} does not match toolchain ${manifest.toolchain}`,
  );
}

const artifactRoot = `/baml-runtime/artifacts/${manifest.sha256.slice(0, 16)}`;
if (manifest.module !== `${artifactRoot}/bridge_wasm.js`) {
  throw new Error(`runtime module is not stored under its content digest: ${manifest.module}`);
}
if (manifest.wasm !== `${artifactRoot}/bridge_wasm_bg.wasm`) {
  throw new Error(`runtime WASM is not stored under its content digest: ${manifest.wasm}`);
}

console.log(
  `ok — runtime ${manifest.runtimeVersion} @ ${manifest.runtimeCommit} (${manifest.rawBytes} raw, ${gzipBytes} gzip)`,
);
