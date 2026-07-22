import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const [input, output] = process.argv.slice(2);
if (!input || !output) {
  throw new Error('usage: generate-proto-dts.js <input.js> <output.d.ts>');
}

const require = createRequire(import.meta.url);
const pbtsPath = require.resolve('protobufjs-cli/pbts.js');
const jsdocPath = require.resolve('jsdoc/jsdoc.js', { paths: [pbtsPath] });
const protobufCliDir = path.dirname(pbtsPath);
const jsdocConfig = path.join(protobufCliDir, 'lib', 'tsd-jsdoc.json');

// protobufjs-cli's pbts wrapper captures JSDoc's stdout through a pipe. With
// JSDoc 4, its custom TypeScript template then emits an empty body while still
// exiting successfully. Give JSDoc a real output descriptor and prepend the
// two imports that pbts normally owns so proto-sync remains deterministic.
const outputFd = fs.openSync(output, 'w');
try {
  fs.writeSync(
    outputFd,
    'import * as $protobuf from "protobufjs";\n' +
      'import Long = require("long");\n',
  );
  const result = spawnSync(
    process.execPath,
    [
      jsdocPath,
      '-c',
      jsdocConfig,
      '-q',
      'module=null&comments=true',
      input,
    ],
    { stdio: ['ignore', outputFd, 'inherit'] },
  );
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`JSDoc exited with status ${result.status}`);
  }
} finally {
  fs.closeSync(outputFd);
}
