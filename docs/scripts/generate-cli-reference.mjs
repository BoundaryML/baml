#!/usr/bin/env node

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readDocsMetadata } from './docs-metadata.mjs';
import {
  checkGeneratedTree,
  writeGeneratedTree,
} from './generated-content.mjs';

const docsRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const check = process.argv.includes('--check');
const contentRoot = path.join(docsRoot, 'content', 'cli', 'commands');
const dataRoot = path.join(docsRoot, 'generated', 'cli');

function frontmatter(title, description) {
  return `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
}

function commandTitle(commandPath) {
  return commandPath.length === 0 ? 'Command reference' : `baml ${commandPath.join(' ')}`;
}

function renderCommand(commandPath, help, description, metadata) {
  const invocation = commandPath.length === 0 ? 'baml help' : `baml help ${commandPath.join(' ')}`;
  const intro = commandPath.length === 0
    ? `This reference is rendered during the docs build from immutable metadata produced by the BAML ${metadata.version} release.`
    : description;
  return [
    frontmatter(commandTitle(commandPath), description),
    intro,
    '',
    `BAML version: \`${metadata.version}\` (${metadata.channel})`,
    `CLI version: \`${metadata.toolchain}\``,
    `Source revision: \`${metadata.sourceRevision}\``,
    '',
    '```text',
    `$ ${invocation}`,
    help,
    '```',
    '',
  ].join('\n');
}

function pagePath(commandPath, hasChildren) {
  if (commandPath.length === 0) return 'index.md';
  if (hasChildren) return path.posix.join(...commandPath, 'index.md');
  return `${path.posix.join(...commandPath)}.md`;
}

function buildFiles(entries, metadata) {
  const content = new Map();
  for (const entry of entries) {
    content.set(
      pagePath(entry.path, entry.children.length > 0),
      renderCommand(entry.path, entry.help, entry.description, metadata),
    );
    if (entry.children.length > 0) {
      const directory = entry.path.join('/');
      const pages = ['index', ...entry.children.map((child) => child.name)];
      const title = entry.path.length === 0 ? 'Commands' : commandTitle(entry.path);
      content.set(path.posix.join(directory, 'meta.json'), `${JSON.stringify({ title, pages }, null, 2)}\n`);
    }
  }

  const manifest = {
    schemaVersion: 1,
    metadataSchemaVersion: metadata.schemaVersion,
    version: metadata.version,
    channel: metadata.channel,
    toolchain: metadata.toolchain,
    sourceRevision: metadata.sourceRevision,
    releasedAt: metadata.releasedAt,
    metadataSha256: metadata.payloadSha256,
    sha256: metadata.cli.sha256,
    commands: entries.length - 1,
    paths: entries.slice(1).map((entry) => entry.path.join(' ')),
  };
  return {
    content,
    data: new Map([
      ['manifest.json', `${JSON.stringify(manifest, null, 2)}\n`],
    ]),
  };
}

if (!process.env.BAML_DOCS_METADATA_FILE || !process.env.BAML_DOCS_VERSION) {
  throw new Error('BAML_DOCS_METADATA_FILE and BAML_DOCS_VERSION are required; run pnpm generate:derived');
}
const metadata = await readDocsMetadata(
  path.resolve(process.env.BAML_DOCS_METADATA_FILE),
  process.env.BAML_DOCS_VERSION,
);
const entries = metadata.cli.commands;
const expected = buildFiles(entries, metadata);

if (check) {
  const changed = [
    ...await checkGeneratedTree(contentRoot, expected.content, 'content'),
    ...await checkGeneratedTree(dataRoot, expected.data, 'generated'),
  ];
  if (changed.length > 0) {
    console.error('Generated CLI reference is stale. Run pnpm generate:cli-reference.');
    for (const name of changed.slice(0, 30)) console.error(`- ${name}`);
    if (changed.length > 30) console.error(`- …and ${changed.length - 30} more`);
    process.exitCode = 1;
  } else {
    console.log(`CLI reference is current (${entries.length - 1} commands).`);
  }
} else {
  await writeGeneratedTree(contentRoot, expected.content);
  await writeGeneratedTree(dataRoot, expected.data);
  console.log(`Generated ${entries.length - 1} CLI command pages for BAML ${metadata.version}.`);
}
