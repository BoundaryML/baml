// copy-proto.js — copy the generated protobufjs runtime
// (typescript_src/proto/baml_cffi.{js,d.ts}) to a REAL root `proto/`
// directory.
//
// Why: tsc compiles `typescript_src/proto.ts` → root `proto.js`, keeping the
// relative `import "./proto/baml_cffi.js"`. That path resolves to root
// `proto/baml_cffi.js`. The protobufjs output is a `.js` (not a `.ts`), so tsc
// never emits it to root. This used to be bridged by a checked-in `proto`
// symlink → `typescript_src/proto`, but symlinks don't survive `npm pack` /
// pnpm `file:` installs, so a consumer of the packed package could not resolve
// `./proto/baml_cffi.js`. Copying into a real directory makes the package
// self-contained and portable across platforms (the release matrix includes
// Windows, where symlinks are unreliable).

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const srcDir = path.join(__dirname, 'proto');
const destDir = path.join(__dirname, '..', 'proto');

fs.mkdirSync(destDir, { recursive: true });
for (const f of ['baml_cffi.js', 'baml_cffi.d.ts']) {
    fs.copyFileSync(path.join(srcDir, f), path.join(destDir, f));
}
