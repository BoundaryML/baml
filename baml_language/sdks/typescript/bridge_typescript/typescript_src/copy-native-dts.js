// copy-native-dts.js - copy napi-generated artifacts into the places used by
// TypeScript source and the packed dist package.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.join(__dirname, '..');

fs.copyFileSync(
  path.join(packageRoot, 'dist', 'native.d.ts'),
  path.join(__dirname, 'native.d.ts'),
);

for (const entry of fs.readdirSync(packageRoot)) {
  if (entry.startsWith('baml_node.') && entry.endsWith('.node')) {
    // Replace the inode: overwriting a previously loaded Mach-O can leave
    // macOS's cached code signature stale and kill the next Node process.
    const destination = path.join(packageRoot, 'dist', entry);
    const temporary = `${destination}.${process.pid}.tmp`;
    fs.copyFileSync(path.join(packageRoot, entry), temporary);
    fs.renameSync(temporary, destination);
  }
}
