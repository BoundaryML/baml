// Tags generated files with a header comment and strips trailing whitespace.
const fs = require('fs');
const path = require('path');

const raw = fs.readFileSync(path.join(__dirname, 'generated-header.txt'), 'utf8').trimEnd();
const header = '/**\n' + raw.split('\n').map(l => (' * ' + l).trimEnd()).join('\n') + '\n */\n';

const files = [
  ...fs.readdirSync(path.join(__dirname, '..')).filter(f => f.endsWith('.js') || f.endsWith('.d.ts')),
  'typescript_src/proto/baml_cffi.js',
  'typescript_src/proto/baml_cffi.d.ts',
];

for (const f of files) {
  const fp = path.join(__dirname, '..', f);
  let c = fs.readFileSync(fp, 'utf8');

  // Prepend header if not already present
  if (!c.startsWith('/**\n')) {
    c = header + c;
  }

  fs.writeFileSync(fp, c);
}
