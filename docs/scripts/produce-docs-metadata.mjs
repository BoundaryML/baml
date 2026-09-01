#!/usr/bin/env node

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  DOCS_METADATA_KIND,
  DOCS_METADATA_SCHEMA_VERSION,
  docsMetadataChecksumPayload,
  sha256Json,
  validateDocsMetadata,
} from './docs-metadata.mjs';
import { run } from './generated-content.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(packageRoot, '..');
const bamlExecutable = process.env.BAML_BIN ?? 'baml';

function argument(name, fallback) {
  const index = process.argv.indexOf(name);
  if (index === -1) return fallback;
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`${name} requires a value`);
  return value;
}

function required(name, fallback) {
  const value = argument(name, fallback);
  if (!value) throw new Error(`Pass ${name} or set its corresponding environment variable`);
  return value;
}

function runBaml(args) {
  return run(bamlExecutable, args, {
    cwd: repositoryRoot,
    env: { ...process.env, BAML_CLI_ALLOW_DIRECT: '1' },
    maxBuffer: 64 * 1024 * 1024,
  })
    .replace(/\x1b\[[0-9;]*m/g, '')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .trim();
}

function parseDescription(help) {
  const prefix = help.split(/^Usage:/m)[0].trim();
  const firstParagraph = prefix.split(/\n\s*\n/)[0] ?? '';
  return firstParagraph.replace(/\s+/g, ' ').trim();
}

function parseSubcommands(help) {
  const lines = help.split('\n');
  const start = lines.findIndex((line) => line === 'Commands:');
  if (start === -1) return [];
  const block = [];
  for (const line of lines.slice(start + 1)) {
    if (line.trim() === '') break;
    block.push(line);
  }
  return block.flatMap((line) => {
    const match = line.match(/^  ([A-Za-z0-9_-]+)\s{2,}(.+)$/);
    return match ? [{ name: match[1], description: match[2].trim() }] : [];
  });
}

async function collectCommands() {
  const queue = [[]];
  const entries = [];
  while (queue.length > 0) {
    const commandPath = queue.shift();
    const help = runBaml(['help', ...commandPath, '--color', 'never', '--no-progress']);
    const children = parseSubcommands(help);
    const title = commandPath.length === 0 ? 'Command reference' : `baml ${commandPath.join(' ')}`;
    const description = parseDescription(help) || `Reference for ${title}.`;
    entries.push({ path: commandPath, description, help, children });
    for (const child of children) queue.push([...commandPath, child.name]);
  }
  return entries.sort((a, b) => a.path.join(' ').localeCompare(b.path.join(' ')));
}

const version = required('--version', process.env.BAML_DOCS_VERSION);
const channel = required('--channel', process.env.BAML_DOCS_CHANNEL);
const sourceRevision = required('--source-revision', process.env.BAML_DOCS_SOURCE_REVISION);
const releasedAt = required('--released-at', process.env.BAML_DOCS_RELEASED_AT);
const output = path.resolve(required('--output', process.env.BAML_DOCS_METADATA_OUTPUT));
const toolchain = runBaml(['--version']).split('\n').join('; ');
if (!toolchain.split(/\s+/).includes(version)) {
  throw new Error(`Source-built CLI reports ${JSON.stringify(toolchain)}, not requested version ${version}`);
}

const packageListing = JSON.parse(runBaml(['describe', '--packages', '--json', '--no-progress', '--color', 'never']));
if (packageListing.format_version !== 1 || !Array.isArray(packageListing.packages) || packageListing.packages.length === 0) {
  throw new Error('baml describe --packages --json returned an unsupported package listing');
}
const packageNames = packageListing.packages;
if (new Set(packageNames).size !== packageNames.length) {
  throw new Error('baml describe --packages --json returned duplicate package names');
}
const packages = packageNames.map((name) => {
  const exported = JSON.parse(runBaml(['describe', name, '--export', '--no-progress', '--color', 'never']));
  if (exported.package !== name) throw new Error(`Requested package ${name}, received ${exported.package}`);
  return { name, sha256: sha256Json(exported), export: exported };
});
const commands = await collectCommands();
const language = {
  formatVersion: 1,
  sha256: sha256Json(packages),
  packages,
};
const cli = {
  formatVersion: 1,
  sha256: sha256Json(commands),
  commands,
};
const metadata = {
  kind: DOCS_METADATA_KIND,
  schemaVersion: DOCS_METADATA_SCHEMA_VERSION,
  version,
  channel,
  sourceRevision,
  releasedAt,
  toolchain,
  language,
  cli,
};
metadata.payloadSha256 = sha256Json(docsMetadataChecksumPayload(metadata));
validateDocsMetadata(metadata, version, channel);
await mkdir(path.dirname(output), { recursive: true });
await writeFile(output, `${JSON.stringify(metadata, null, 2)}\n`);
const itemCount = packages.reduce((total, entry) => total + entry.export.items.length, 0);
console.log(`Produced BAML docs metadata ${version} (${packages.length} packages, ${itemCount} language items, ${commands.length - 1} CLI commands) at ${output}.`);
