// copy-proto.js - copy the generated protobufjs runtime
// (typescript_src/proto/baml_cffi.{js,d.ts}) to dist/proto/.
//
// Why: tsc compiles `typescript_src/proto.ts` to `dist/proto.js`, keeping the
// relative `import "./proto/baml_cffi.js"`. The protobufjs output is a `.js`
// (not a `.ts`), so tsc never emits it. Copying it into dist makes the packed
// package self-contained and portable across platforms.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const srcDir = path.join(__dirname, 'proto');
const destDir = path.join(__dirname, '..', 'dist', 'proto');

fs.mkdirSync(destDir, { recursive: true });
for (const f of ['baml_cffi.js', 'baml_cffi.d.ts']) {
    fs.copyFileSync(path.join(srcDir, f), path.join(destDir, f));
}
