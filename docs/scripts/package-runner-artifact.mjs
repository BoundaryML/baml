#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { packageRuntimeArtifact } from './runtime-artifact.mjs';

function required(args, name) {
  const index = args.indexOf(name);
  if (index === -1 || !args[index + 1]) throw new Error(`Pass ${name} <value>`);
  return args[index + 1];
}

function capture(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(' ')} failed:\n${result.stderr ?? ''}`);
  return result.stdout.trim();
}

async function main() {
  const args = process.argv.slice(2);
  const inputRoot = path.resolve(required(args, '--input'));
  const outputRoot = path.resolve(required(args, '--output'));
  const sourceRevision = required(args, '--source-revision');
  const version = required(args, '--version');
  const bamlBinary = process.env.BAML_BIN ?? 'baml';
  const toolchain = capture(bamlBinary, ['--version']);
  const manifest = await packageRuntimeArtifact({ inputRoot, outputRoot, sourceRevision, toolchain, version });
  console.log(`Packaged docs runtime ${manifest.version} at ${outputRoot}/runtime.json`);
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
