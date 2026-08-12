// Tags generated files with a header comment and strips trailing whitespace.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const raw = fs.readFileSync(path.join(__dirname, 'generated-header.txt'), 'utf8').trimEnd();
const header = '/**\n' + raw.split('\n').map(l => (' * ' + l).trimEnd()).join('\n') + '\n */\n';

function collectGeneratedFiles(dir, relPrefix = '') {
  const files = [];
  if (!fs.existsSync(dir)) {
    return files;
  }
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const rel = path.join(relPrefix, entry.name);
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectGeneratedFiles(full, rel));
    } else if (entry.name.endsWith('.js') || entry.name.endsWith('.d.ts')) {
      files.push(path.join('dist', rel));
    }
  }
  return files;
}

const files = [
  ...collectGeneratedFiles(path.join(__dirname, '..', 'dist')),
  'typescript_src/proto/baml_cffi.js',
  'typescript_src/proto/baml_cffi.d.ts',
];

for (const f of files) {
  const fp = path.join(__dirname, '..', f);
  let c = fs.readFileSync(fp, 'utf8');

  if (f.endsWith('baml_cffi.js')) {
    c = c.replaceAll(
      'import * as $protobuf from "protobufjs/minimal";',
      'import $protobuf from "protobufjs/minimal.js";',
    );
  }
  if (f.endsWith('baml_cffi.d.ts')) {
    c = c.replaceAll('import Long = require("long");', 'import Long from "long";');
  }

  // Prepend header if not already present
  if (!c.startsWith('/**\n')) {
    c = header + c;
  }

  fs.writeFileSync(fp, c);
}
