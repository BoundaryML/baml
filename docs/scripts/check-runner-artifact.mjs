import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const publicRoot = path.join(packageRoot, 'public');
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
if (wasm.length !== manifest.rawBytes) {
  throw new Error(`runtime has ${wasm.length} bytes; manifest records ${manifest.rawBytes}`);
}
if (digest(wasm) !== manifest.sha256) {
  throw new Error('runtime digest does not match the manifest');
}
if (digest(module) !== manifest.moduleSha256) {
  throw new Error('runtime JavaScript digest does not match the manifest');
}
if (!/^\d+\.\d+\.\d+/.test(manifest.runtimeVersion)) {
  throw new Error(`invalid runtime version: ${manifest.runtimeVersion}`);
}
if (!/^[0-9a-f]{40}$/.test(manifest.sourceCommit)) {
  throw new Error(`invalid source commit: ${manifest.sourceCommit}`);
}

console.log(
  `ok — runtime ${manifest.runtimeVersion} @ ${manifest.runtimeCommit} (${manifest.rawBytes} bytes)`,
);
