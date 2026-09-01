import { readFile } from 'node:fs/promises';
import { brotliCompressSync, constants, gzipSync } from 'node:zlib';

const artifactPath = process.argv[2];
if (!artifactPath) {
  console.error('usage: pnpm runner:measure <path-to-bridge_wasm_bg.wasm>');
  process.exit(2);
}

const bytes = await readFile(artifactPath);
const result = {
  artifactPath,
  rawBytes: bytes.length,
  gzipBytes: gzipSync(bytes, { level: 9 }).length,
  brotliBytes: brotliCompressSync(bytes, {
    params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
  }).length,
};

console.log(JSON.stringify(result, null, 2));

const maxGzipBytes = Number(process.env.BAML_RUNNER_MAX_GZIP_BYTES ?? 0);
if (maxGzipBytes > 0 && result.gzipBytes > maxGzipBytes) {
  console.error(
    `compressed runner is ${result.gzipBytes} bytes; budget is ${maxGzipBytes} bytes`,
  );
  process.exit(1);
}
