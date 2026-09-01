#!/usr/bin/env node

import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { readDocsMetadata } from './docs-metadata.mjs';
import {
  checkGeneratedTree,
  writeGeneratedTree,
} from './generated-content.mjs';

const docsRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const check = process.argv.includes('--check');
const contentRoot = path.join(docsRoot, 'content', 'baml', 'language', 'reference');
const dataRoot = path.join(docsRoot, 'generated', 'baml');

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

function frontmatter(title, description) {
  return `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
}

function qualifySymbol(value, context, knownSymbols) {
  if (!value || knownSymbols.has(value)) return value;
  for (let depth = context.length; depth > 0; depth -= 1) {
    const candidate = [...context.slice(0, depth), value].join('.');
    if (knownSymbols.has(candidate)) return candidate;
  }
  return value;
}

function qualifyTypeDisplay(value, context, knownSymbols) {
  return value?.replace(
    /[$A-Za-z_][A-Za-z0-9_$]*(?:\.[$A-Za-z_][A-Za-z0-9_$]*)*/g,
    (token) => qualifySymbol(token, context, knownSymbols),
  );
}

function generics(value, context, knownSymbols) {
  const entries = value?.generics ?? [];
  if (entries.length === 0) return '';
  return `<${entries.map((entry) => {
    const bounds = entry.bounds
      ?.map((bound) => qualifyTypeDisplay(bound.display ?? bound, context, knownSymbols))
      .filter(Boolean) ?? [];
    return bounds.length > 0 ? `${entry.name} extends ${bounds.join(' + ')}` : entry.name;
  }).join(', ')}>`;
}

function signature(name, value, context, knownSymbols, prefix = 'function') {
  const params = (value?.signature?.params ?? value?.params ?? [])
    .map((param) => `${param.name}: ${qualifyTypeDisplay(param.ty.display, context, knownSymbols)}`)
    .join(', ');
  const returns = qualifyTypeDisplay(
    value?.signature?.returns?.display ?? value?.returns?.display ?? 'void',
    context,
    knownSymbols,
  );
  const throws = qualifyTypeDisplay(
    value?.signature?.throws?.display ?? value?.throws?.display,
    context,
    knownSymbols,
  );
  const throwsClause = throws && throws !== 'never' ? ` throws ${throws}` : '';
  return `${prefix} ${name}${generics(value?.signature ?? value, context, knownSymbols)}(${params}) -> ${returns}${throwsClause}`;
}

function code(value) {
  return `\n\`\`\`baml\n${value}\n\`\`\`\n`;
}

function docstring(value, context, knownSymbols, fallback = 'No description is available yet.') {
  const text = value?.trim() || fallback;
  return text
    .replace(/```([^\n]*)\n([\s\S]*?)```/g, (_match, info, body) => {
      const qualified = body.replace(
        /[$A-Za-z_][A-Za-z0-9_$]*(?:\.[$A-Za-z_][A-Za-z0-9_$]*)+/g,
        (symbol) => qualifySymbol(symbol, context, knownSymbols),
      );
      return `\`\`\`${info}\n${qualified}\`\`\``;
    })
    .replace(/`([$A-Za-z_][A-Za-z0-9_$]*(?:\.[$A-Za-z_][A-Za-z0-9_$]*)*)`/g, (_match, symbol) => (
      `\`${qualifySymbol(symbol, context, knownSymbols)}\``
    ))
    .replace(/\ban (`baml\.)/g, 'a $1')
    .replace(/^(#{1,4}) /gm, '$1## ');
}

function sourceNote(item) {
  if (!item.source) return '';
  return `\n_Source: \`${item.source.file}:${item.source.start}\`_\n`;
}

function renderMethods(methods, heading, owner, context, knownSymbols) {
  if (!methods?.length) return '';
  return `\n## ${heading}\n${methods.map((method) => [
    `\n### ${owner}.${method.name}\n`,
    code(signature(`${owner}.${method.name}`, method, context, knownSymbols)),
    `\n${docstring(method.docstring, context, knownSymbols)}\n`,
  ].join('')).join('')}`;
}

function renderItem(item, packageName, knownSymbols) {
  const qualifiedName = [packageName, ...(item.namespace ?? []), item.name].join('.');
  const context = [packageName, ...(item.namespace ?? []), item.name];
  const description = `${kindTitles[item.kind]} ${qualifiedName} from the generated ${packageName} package reference.`;
  let body = frontmatter(qualifiedName, description);
  body += `${docstring(item.docstring, context, knownSymbols)}\n`;

  if (item.kind === 'function') {
    body += code(signature(qualifiedName, item, context, knownSymbols));
  } else if (item.kind === 'type_alias') {
    body += code(`type ${qualifiedName}${generics(item, context, knownSymbols)} = ${qualifyTypeDisplay(item.resolved.display, context, knownSymbols)}`);
  } else if (item.kind === 'class') {
    body += code(`class ${qualifiedName}${generics(item, context, knownSymbols)}`);
    if (item.fields?.length) {
      body += `\n## Fields\n${item.fields.map((field) => `\n### ${qualifiedName}.${field.name}\n${code(`${qualifiedName}.${field.name}: ${qualifyTypeDisplay(field.ty.display, context, knownSymbols)}`)}\n${docstring(field.docstring, context, knownSymbols)}\n`).join('')}`;
    }
    body += renderMethods(item.methods, 'Methods', qualifiedName, context, knownSymbols);
  } else if (item.kind === 'enum') {
    body += code(`enum ${qualifiedName}`);
    body += `\n## Variants\n${item.variants.map((variant) => `\n### ${qualifiedName}.${variant.name}\n\n${docstring(variant.docstring, context, knownSymbols)}\n`).join('')}`;
  } else if (item.kind === 'interface') {
    body += code(`interface ${qualifiedName}${generics(item, context, knownSymbols)}`);
    if (item.assoc_types?.length) {
      body += `\n## Associated types\n${item.assoc_types.map((assoc) => `\n### ${qualifiedName}.${assoc.name}\n${code(`type ${qualifiedName}.${assoc.name}`)}\n${docstring(assoc.docstring, context, knownSymbols)}\n`).join('')}`;
    }
    body += renderMethods(item.required_methods, 'Required methods', qualifiedName, context, knownSymbols);
    body += renderMethods(item.default_methods, 'Default methods', qualifiedName, context, knownSymbols);
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
    const parts = directory.split('/');
    const category = parts[1];
    const title = parts.length === 1
      ? `${parts[0]} package`
      : parts.length === 2
        ? `${parts[0]} ${categoryTitles[category] ?? category}`
        : `${parts[0]}.${parts.slice(2).join('.')}`;
    const metaPath = path.posix.join(directory, 'meta.json');
    if (!files.has(metaPath)) {
      files.set(metaPath, `${JSON.stringify({ title, pages: [...children].sort((a, b) => a.localeCompare(b)) }, null, 2)}\n`);
    }
  }
}

export function buildBamlReferenceFiles(metadata) {
  const content = new Map();
  const itemPaths = [];
  const packageManifests = [];
  const knownSymbols = new Set();
  for (const packageEntry of metadata.language.packages) {
    for (const item of packageEntry.export.items) {
      const owner = [packageEntry.name, ...(item.namespace ?? []), item.name].join('.');
      knownSymbols.add(owner);
      for (const member of [
        ...(item.fields ?? []),
        ...(item.methods ?? []),
        ...(item.variants ?? []),
        ...(item.assoc_types ?? []),
        ...(item.required_methods ?? []),
        ...(item.default_methods ?? []),
      ]) {
        knownSymbols.add(`${owner}.${member.name}`);
      }
    }
  }
  for (const packageEntry of metadata.language.packages) {
    const packageName = safeSegment(packageEntry.name);
    const exported = packageEntry.export;
    const counts = Object.fromEntries(Object.keys(categoryForKind).map((kind) => [kind, 0]));
    for (const item of [...exported.items].sort((a, b) => a.id.localeCompare(b.id))) {
      const category = categoryForKind[item.kind];
      if (!category) throw new Error(`Unsupported item kind: ${item.kind}`);
      counts[item.kind] += 1;
      const namespace = (item.namespace ?? []).map(safeSegment);
      const relativePath = path.posix.join(packageName, category, ...namespace, `${safeSegment(item.name)}.md`);
      if (content.has(relativePath)) throw new Error(`Duplicate generated page: ${relativePath}`);
      content.set(relativePath, renderItem(item, packageName, knownSymbols));
      itemPaths.push(relativePath);
    }

    const categories = Object.entries(categoryForKind)
      .filter(([kind]) => counts[kind] > 0)
      .map(([, category]) => category);
    const packageSummary = [
      frontmatter(`${packageName} package`, `Generated reference for the ${packageName} standard-library package.`),
      `This package reference is rendered from immutable BAML ${metadata.version} release metadata.`,
      '',
      `- Source SHA-256: \`${packageEntry.sha256}\``,
      `- Items: ${exported.items.length}`,
      `- Implementations: ${exported.impls?.length ?? 0}`,
      '',
      '## Browse by kind',
      '',
      ...Object.entries(categoryForKind)
        .filter(([kind]) => counts[kind] > 0)
        .map(([kind, category]) => `- [${packageName} ${categoryTitles[category]}](./${category}): ${counts[kind]}`),
      '',
    ].join('\n');
    content.set(path.posix.join(packageName, 'index.md'), packageSummary);
    content.set(
      path.posix.join(packageName, 'meta.json'),
      `${JSON.stringify({ title: `${packageName} package`, pages: ['index', ...categories] }, null, 2)}\n`,
    );
    packageManifests.push({
      package: packageName,
      exportFormatVersion: exported.format_version,
      sha256: packageEntry.sha256,
      items: exported.items.length,
      impls: exported.impls?.length ?? 0,
      counts,
    });
  }

  const items = packageManifests.reduce((total, entry) => total + entry.items, 0);
  const impls = packageManifests.reduce((total, entry) => total + entry.impls, 0);
  const manifest = {
    schemaVersion: 1,
    metadataSchemaVersion: metadata.schemaVersion,
    version: metadata.version,
    channel: metadata.channel,
    toolchain: metadata.toolchain,
    sourceRevision: metadata.sourceRevision,
    releasedAt: metadata.releasedAt,
    metadataSha256: metadata.payloadSha256,
    sha256: metadata.language.sha256,
    packages: packageManifests,
    items,
    impls,
  };

  const summary = [
    frontmatter('Standard library reference', `Generated reference for every package in the BAML ${metadata.version} standard library.`),
    `This reference is rendered during the docs build from immutable metadata produced by the BAML ${metadata.version} release. The package list comes directly from the toolchain.`,
    '',
    `- BAML version: \`${metadata.version}\` (${metadata.channel})`,
    `- Toolchain: \`${metadata.toolchain}\``,
    `- Source revision: \`${metadata.sourceRevision}\``,
    `- Package-set SHA-256: \`${metadata.language.sha256}\``,
    `- Packages: ${packageManifests.length}`,
    `- Items: ${items}`,
    `- Implementations: ${impls}`,
    '',
    '## Packages',
    '',
    ...packageManifests.map((entry) => `- [${entry.package}](./${entry.package}): ${entry.items} items`),
    '',
  ].join('\n');
  content.set('index.md', summary);
  addMetaFiles(content, itemPaths);
  content.set('meta.json', `${JSON.stringify({ title: 'Standard library', pages: ['index', ...packageManifests.map((entry) => entry.package)] }, null, 2)}\n`);

  return {
    content,
    data: new Map([
      ['manifest.json', `${JSON.stringify(manifest, null, 2)}\n`],
    ]),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  if (!process.env.BAML_DOCS_METADATA_FILE || !process.env.BAML_DOCS_VERSION) {
    throw new Error('BAML_DOCS_METADATA_FILE and BAML_DOCS_VERSION are required; run pnpm generate:derived');
  }
  const metadata = await readDocsMetadata(
    path.resolve(process.env.BAML_DOCS_METADATA_FILE),
    process.env.BAML_DOCS_VERSION,
  );
  const expected = buildBamlReferenceFiles(metadata);

  if (check) {
    const changed = [
      ...await checkGeneratedTree(contentRoot, expected.content, 'content'),
      ...await checkGeneratedTree(dataRoot, expected.data, 'generated'),
    ];
    if (changed.length > 0) {
      console.error('Generated BAML reference is stale. Run pnpm generate:reference.');
      for (const name of changed.slice(0, 30)) console.error(`- ${name}`);
      if (changed.length > 30) console.error(`- …and ${changed.length - 30} more`);
      process.exitCode = 1;
    } else {
      console.log(`BAML reference is current (${metadata.language.packages.length} packages, ${expected.content.size} content files).`);
    }
  } else {
    await writeGeneratedTree(contentRoot, expected.content);
    await writeGeneratedTree(dataRoot, expected.data);
    console.log(`Generated ${metadata.language.packages.length} standard-library package references for BAML ${metadata.version}.`);
  }
}
