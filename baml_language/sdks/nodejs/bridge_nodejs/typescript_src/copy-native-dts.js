// copy-native-dts.js - copy the napi-generated declaration file into
// typescript_src so TypeScript can compile against the latest native surface.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.join(__dirname, '..');

fs.copyFileSync(
  path.join(packageRoot, 'native.d.ts'),
  path.join(__dirname, 'native.d.ts'),
);
