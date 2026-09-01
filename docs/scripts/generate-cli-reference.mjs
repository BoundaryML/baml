#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const docsRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const bamlExecutable = process.env.BAML_BIN ?? 'baml';
const check = process.argv.includes('--check');
const contentRoot = path.join(docsRoot, 'content', 'cli', 'commands');
const dataRoot = path.join(docsRoot, 'generated', 'cli');

function runBaml(args) {
  const result = spawnSync(bamlExecutable, args, {
    cwd: path.resolve(docsRoot, '..'),
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`${bamlExecutable} ${args.join(' ')} failed:\n${result.stderr || result.stdout}`);
  }
  return result.stdout
    .replace(/\x1b\[[0-9;]*m/g, '')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .trim();
}

function frontmatter(title, description) {
  return `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
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

function safeSegment(value) {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) {
    throw new Error(`Unsafe CLI path segment: ${JSON.stringify(value)}`);
  }
  return value;
}

function helpFor(commandPath) {
  return runBaml(['help', ...commandPath, '--color', 'never', '--no-progress']);
}

function commandTitle(commandPath) {
  return commandPath.length === 0 ? 'Command reference' : `baml ${commandPath.join(' ')}`;
}

function renderCommand(commandPath, help, description, version) {
  const invocation = commandPath.length === 0 ? 'baml help' : `baml help ${commandPath.join(' ')}`;
  const intro = commandPath.length === 0
    ? `This reference is generated directly from the CLI's public Clap command tree. It is checked in with the CLI version and help-content hash so each docs version remains reproducible.`
    : description;
  return [
    frontmatter(commandTitle(commandPath), description),
    intro,
    '',
    `CLI version: \`${version}\``,
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

async function collectCommands() {
  const queue = [[]];
  const entries = [];
  while (queue.length > 0) {
    const commandPath = queue.shift();
    const help = helpFor(commandPath);
    const children = parseSubcommands(help);
    const description = parseDescription(help) || `Reference for ${commandTitle(commandPath)}.`;
    entries.push({ path: commandPath, description, help, children });
    for (const child of children) queue.push([...commandPath, safeSegment(child.name)]);
  }
  return entries.sort((a, b) => a.path.join(' ').localeCompare(b.path.join(' ')));
}

function buildFiles(entries, version) {
  const content = new Map();
  for (const entry of entries) {
    content.set(
      pagePath(entry.path, entry.children.length > 0),
      renderCommand(entry.path, entry.help, entry.description, version),
    );
    if (entry.children.length > 0) {
      const directory = entry.path.join('/');
      const pages = ['index', ...entry.children.map((child) => child.name)];
      const title = entry.path.length === 0 ? 'Commands' : commandTitle(entry.path);
      content.set(path.posix.join(directory, 'meta.json'), `${JSON.stringify({ title, pages }, null, 2)}\n`);
    }
  }

  const snapshot = {
    schemaVersion: 1,
    version,
    commands: entries.map((entry) => ({
      path: entry.path,
      description: entry.description,
      help: entry.help,
    })),
  };
  const rawSnapshot = `${JSON.stringify(snapshot, null, 2)}\n`;
  const manifest = {
    schemaVersion: 1,
    version,
    sha256: createHash('sha256').update(rawSnapshot).digest('hex'),
    commands: entries.length - 1,
    paths: entries.slice(1).map((entry) => entry.path.join(' ')),
  };
  return {
    content,
    data: new Map([
      ['help.json', rawSnapshot],
      ['manifest.json', `${JSON.stringify(manifest, null, 2)}\n`],
    ]),
  };
}

async function currentFiles(root) {
  const files = new Map();
  async function visit(directory, prefix = '') {
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      if (error.code === 'ENOENT') return;
      throw error;
    }
    for (const entry of entries) {
      const relative = path.posix.join(prefix, entry.name);
      if (entry.isDirectory()) await visit(path.join(directory, entry.name), relative);
      else files.set(relative, await readFile(path.join(directory, entry.name), 'utf8'));
    }
  }
  await visit(root);
  return files;
}

function diffFiles(expected, actual) {
  const names = new Set([...expected.keys(), ...actual.keys()]);
  return [...names].sort().filter((name) => expected.get(name) !== actual.get(name));
}

async function writeFiles(root, files) {
  await rm(root, { recursive: true, force: true });
  for (const [relative, contents] of files) {
    const destination = path.join(root, relative);
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, contents);
  }
}

const version = runBaml(['--version']).split('\n').join('; ');
const entries = await collectCommands();
const expected = buildFiles(entries, version);

if (check) {
  const contentDiff = diffFiles(expected.content, await currentFiles(contentRoot));
  const dataDiff = diffFiles(expected.data, await currentFiles(dataRoot));
  const changed = [...contentDiff.map((name) => `content/${name}`), ...dataDiff.map((name) => `generated/${name}`)];
  if (changed.length > 0) {
    console.error('Generated CLI reference is stale. Run pnpm generate:cli-reference.');
    for (const name of changed.slice(0, 30)) console.error(`- ${name}`);
    if (changed.length > 30) console.error(`- …and ${changed.length - 30} more`);
    process.exitCode = 1;
  } else {
    console.log(`CLI reference is current (${entries.length - 1} commands).`);
  }
} else {
  await writeFiles(contentRoot, expected.content);
  await writeFiles(dataRoot, expected.data);
  console.log(`Generated ${entries.length - 1} CLI command pages with ${version}.`);
}
