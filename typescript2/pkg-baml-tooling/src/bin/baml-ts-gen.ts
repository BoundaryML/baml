#!/usr/bin/env node
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { loadProject } from '../node.js';
import { sidecarPath } from '../sidecar.js';

const project = await loadProject({ backend: 'auto', cwd: process.cwd() });
const check = project.check();
const errors = check.diagnostics.filter(
  (diagnostic) => diagnostic.severity === 'error',
);
if (errors.length > 0) {
  for (const diagnostic of errors)
    console.error(
      `${diagnostic.location?.path ?? 'baml'}: ${diagnostic.message}`,
    );
  process.exitCode = 1;
} else {
  for (const source of project.layout().sourceFiles) {
    const output = sidecarPath(source);
    const declaration = project.resolveDts(source, source).code;
    await mkdir(dirname(output), { recursive: true });
    await writeFile(output, declaration);
    console.log(output);
  }
}
project.dispose();
