#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const docsRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const packageName = process.env.BAML_REFERENCE_PACKAGE ?? 'baml';
const bamlExecutable = process.env.BAML_BIN ?? 'baml';
const check = process.argv.includes('--check');
const contentRoot = path.join(docsRoot, 'content', 'baml', 'language', 'reference');
const dataRoot = path.join(docsRoot, 'generated', packageName);

const categoryForKind = {
  class: 'classes',
  enum: 'enums',
  function: 'functions',
  interface: 'interfaces',
  type_alias: 'type-aliases',
};

const categoryTitles = {
  classes: 'Classes',
  enums: 'Enums',
  functions: 'Functions',
  interfaces: 'Interfaces',
  'type-aliases': 'Type aliases',
};

const kindTitles = {
  class: 'Class',
  enum: 'Enum',
  function: 'Function',
  interface: 'Interface',
  type_alias: 'Type alias',
};

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: path.resolve(docsRoot, '..'),
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed:\n${result.stderr || result.stdout}`);
  }
  return result.stdout.trim();
}

function frontmatter(title, description) {
  return `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
}

function generics(value) {
  const entries = value?.generics ?? [];
  if (entries.length === 0) return '';
  return `<${entries.map((entry) => {
    const bounds = entry.bounds?.map((bound) => bound.display ?? bound).filter(Boolean) ?? [];
    return bounds.length > 0 ? `${entry.name} extends ${bounds.join(' + ')}` : entry.name;
  }).join(', ')}>`;
}

function signature(name, value, prefix = 'function') {
  const params = (value?.signature?.params ?? value?.params ?? [])
    .map((param) => `${param.name}: ${param.ty.display}`)
    .join(', ');
  const returns = value?.signature?.returns?.display ?? value?.returns?.display ?? 'void';
  const throws = value?.signature?.throws?.display ?? value?.throws?.display;
  const throwsClause = throws && throws !== 'never' ? ` throws ${throws}` : '';
  return `${prefix} ${name}${generics(value?.signature ?? value)}(${params}) -> ${returns}${throwsClause}`;
}

function code(value) {
  return `\n\`\`\`baml\n${value}\n\`\`\`\n`;
}

function docstring(value, fallback = 'No description is available yet.') {
  const text = value?.trim() || fallback;
  return text.replace(/^(#{1,4}) /gm, '$1## ');
}

function sourceNote(item) {
  if (!item.source) return '';
  return `\n_Source: \`${item.source.file}:${item.source.start}\`_\n`;
}

function renderMethods(methods, heading) {
  if (!methods?.length) return '';
  return `\n## ${heading}\n${methods.map((method) => [
    `\n### ${method.name}\n`,
    code(signature(method.name, method)),
    `\n${docstring(method.docstring)}\n`,
  ].join('')).join('')}`;
}

function renderItem(item) {
  const qualifiedName = [...(item.namespace ?? []), item.name].join('.');
  const description = `${kindTitles[item.kind]} ${qualifiedName} from the generated ${packageName} package reference.`;
  let body = frontmatter(qualifiedName, description);
  body += `${docstring(item.docstring)}\n`;

  if (item.kind === 'function') {
    body += code(signature(qualifiedName, item));
  } else if (item.kind === 'type_alias') {
    body += code(`type ${qualifiedName}${generics(item)} = ${item.resolved.display}`);
  } else if (item.kind === 'class') {
    body += code(`class ${qualifiedName}${generics(item)}`);
    if (item.fields?.length) {
      body += `\n## Fields\n${item.fields.map((field) => `\n### ${field.name}\n${code(`${field.name}: ${field.ty.display}`)}\n${docstring(field.docstring)}\n`).join('')}`;
    }
    body += renderMethods(item.methods, 'Methods');
  } else if (item.kind === 'enum') {
    body += code(`enum ${qualifiedName}`);
    body += `\n## Variants\n${item.variants.map((variant) => `\n### ${variant.name}\n\n${docstring(variant.docstring)}\n`).join('')}`;
  } else if (item.kind === 'interface') {
    body += code(`interface ${qualifiedName}${generics(item)}`);
    if (item.assoc_types?.length) {
      body += `\n## Associated types\n${item.assoc_types.map((assoc) => `\n### ${assoc.name}\n${code(`type ${assoc.name}`)}\n${docstring(assoc.docstring)}\n`).join('')}`;
    }
    body += renderMethods(item.required_methods, 'Required methods');
    body += renderMethods(item.default_methods, 'Default methods');
  }

  body += sourceNote(item);
  return `${body.trimEnd()}\n`;
}

function safeSegment(value) {
  // `$stream` is a compiler-generated, public type suffix in describe output.
  if (!/^[A-Za-z0-9_$-]+$/.test(value)) {
    throw new Error(`Unsafe generated path segment: ${JSON.stringify(value)}`);
  }
  return value;
}

function addMetaFiles(files, itemPaths) {
  const directories = new Map();
  for (const itemPath of itemPaths) {
    const parts = itemPath.split('/');
    const file = parts.pop().replace(/\.md$/, '');
    for (let depth = 0; depth <= parts.length; depth += 1) {
      const directory = parts.slice(0, depth).join('/');
      const child = depth === parts.length ? file : parts[depth];
      if (!directories.has(directory)) directories.set(directory, new Set());
      directories.get(directory).add(child);
    }
  }
  for (const [directory, children] of directories) {
    if (directory === '') continue;
    const basename = directory.split('/').at(-1);
    const title = categoryTitles[basename] ?? basename;
    files.set(path.posix.join(directory, 'meta.json'), `${JSON.stringify({ title, pages: [...children].sort((a, b) => a.localeCompare(b)) }, null, 2)}\n`);
  }
}

function buildFiles(exported, toolchain) {
  if (exported.format_version !== 1) {
    throw new Error(`Unsupported baml describe export format: ${exported.format_version}`);
  }
  if (exported.package !== packageName) {
    throw new Error(`Expected package ${packageName}, received ${exported.package}`);
  }

  const counts = Object.fromEntries(Object.keys(categoryForKind).map((kind) => [kind, 0]));
  const content = new Map();
  const itemPaths = [];
  for (const item of [...exported.items].sort((a, b) => a.id.localeCompare(b.id))) {
    const category = categoryForKind[item.kind];
    if (!category) throw new Error(`Unsupported item kind: ${item.kind}`);
    counts[item.kind] += 1;
    const namespace = (item.namespace ?? []).map(safeSegment);
    const relativePath = path.posix.join(category, ...namespace, `${safeSegment(item.name)}.md`);
    if (content.has(relativePath)) throw new Error(`Duplicate generated page: ${relativePath}`);
    content.set(relativePath, renderItem(item));
    itemPaths.push(relativePath);
  }

  const rawExport = `${JSON.stringify(exported, null, 2)}\n`;
  const sha256 = createHash('sha256').update(rawExport).digest('hex');
  const manifest = {
    schemaVersion: 1,
    package: packageName,
    exportFormatVersion: exported.format_version,
    toolchain,
    sha256,
    items: exported.items.length,
    impls: exported.impls?.length ?? 0,
    counts,
  };

  const summary = [
    frontmatter(`${packageName} package reference`, `Generated reference for the ${packageName} standard package.`),
    `This reference is generated from \`baml describe ${packageName} --export\`. It is checked in so every docs version stays paired with the compiler surface that produced it.`,
    '',
    `- Toolchain: \`${toolchain}\``,
    `- Export format: \`${exported.format_version}\``,
    `- Source SHA-256: \`${sha256}\``,
    `- Items: ${exported.items.length}`,
    `- Implementations: ${manifest.impls}`,
    '',
    '## Browse by kind',
    '',
    ...Object.entries(categoryForKind).map(([kind, category]) => `- [${categoryTitles[category]}](./${category}): ${counts[kind]}`),
    '',
  ].join('\n');
  content.set('index.md', summary);
  addMetaFiles(content, itemPaths);
  content.set('meta.json', `${JSON.stringify({ title: `${packageName} package`, pages: ['index', ...Object.values(categoryForKind)] }, null, 2)}\n`);

  return {
    content,
    data: new Map([
      ['export.json', rawExport],
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

const raw = run(bamlExecutable, ['describe', packageName, '--export', '--no-progress', '--color', 'never']);
const exported = JSON.parse(raw);
const toolchain = run(bamlExecutable, ['--version']).split('\n').join('; ');
const expected = buildFiles(exported, toolchain);

if (check) {
  const contentDiff = diffFiles(expected.content, await currentFiles(contentRoot));
  const dataDiff = diffFiles(expected.data, await currentFiles(dataRoot));
  const changed = [...contentDiff.map((name) => `content/${name}`), ...dataDiff.map((name) => `generated/${name}`)];
  if (changed.length > 0) {
    console.error('Generated BAML reference is stale. Run pnpm generate:reference.');
    for (const name of changed.slice(0, 30)) console.error(`- ${name}`);
    if (changed.length > 30) console.error(`- …and ${changed.length - 30} more`);
    process.exitCode = 1;
  } else {
    console.log(`BAML reference is current (${exported.items.length} items, ${expected.data.size} data files).`);
  }
} else {
  await writeFiles(contentRoot, expected.content);
  await writeFiles(dataRoot, expected.data);
  console.log(`Generated ${exported.items.length} ${packageName} reference pages with ${toolchain}.`);
}
