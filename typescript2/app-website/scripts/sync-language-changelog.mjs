import { copyFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const websiteDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const source = path.resolve(websiteDir, '../../baml_language/CHANGELOG.md');
const destination = path.join(websiteDir, 'data/changelog.md');

await mkdir(path.dirname(destination), { recursive: true });
await copyFile(source, destination);

console.log(`Copied ${source} to ${destination}`);
