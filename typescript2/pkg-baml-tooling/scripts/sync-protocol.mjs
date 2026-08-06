import { copyFile, mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = resolve(
  here,
  '../../pkg-proto/src/generated/baml/tooling/v1/tooling.ts',
);
const destination = resolve(here, '../src/generated/tooling.ts');
await mkdir(dirname(destination), { recursive: true });
await copyFile(source, destination);
