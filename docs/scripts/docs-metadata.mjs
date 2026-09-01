import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

export const DOCS_METADATA_KIND = 'baml.docs-metadata';
export const DOCS_METADATA_SCHEMA_VERSION = 1;

export function sha256Json(value) {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex');
}

function invariant(condition, message) {
  if (!condition) throw new Error(`Invalid BAML docs metadata: ${message}`);
}

// `$`-prefixed names are emitted for synthetic public toolchain symbols.
const identifierPattern = /^[$A-Za-z_][A-Za-z0-9_$]*$/;

function validateOptionalDocstring(value, label) {
  invariant(value === undefined || typeof value === 'string', `${label}.docstring must be a string`);
}

function validateType(type, label) {
  invariant(type && typeof type === 'object', `${label} must be an object`);
  invariant(typeof type.display === 'string' && type.display.length > 0, `${label}.display must be non-empty`);
}

function validateGenerics(generics, label) {
  invariant(Array.isArray(generics ?? []), `${label} must be an array`);
  for (const [index, generic] of (generics ?? []).entries()) {
    invariant(generic && identifierPattern.test(generic.name), `${label}[${index}].name must be an identifier`);
    invariant(Array.isArray(generic.bounds), `${label}[${index}].bounds must be an array`);
    invariant(
      generic.bounds.every((bound) => typeof (bound?.display ?? bound) === 'string'),
      `${label}[${index}].bounds must contain type displays`,
    );
  }
}

function validateCallable(callable, label, expectedId) {
  invariant(callable && typeof callable === 'object', `${label} must be an object`);
  invariant(identifierPattern.test(callable.name), `${label}.name must be an identifier`);
  if (expectedId) invariant(callable.id === expectedId, `${label}.id must be ${expectedId}`);
  validateOptionalDocstring(callable.docstring, label);
  const signature = callable.signature;
  invariant(signature && typeof signature === 'object', `${label}.signature is required`);
  validateGenerics(signature.generics, `${label}.signature.generics`);
  invariant(Array.isArray(signature.params), `${label}.signature.params must be an array`);
  for (const [index, param] of signature.params.entries()) {
    invariant(param && identifierPattern.test(param.name), `${label}.signature.params[${index}].name must be an identifier`);
    validateType(param.ty, `${label}.signature.params[${index}].ty`);
  }
  validateType(signature.returns, `${label}.signature.returns`);
  validateType(signature.throws, `${label}.signature.throws`);
}

function validateItem(item, index, packageName) {
  const label = `${packageName}.items[${index}]`;
  invariant(item && typeof item === 'object', `${label} must be an object`);
  invariant(['class', 'enum', 'function', 'interface', 'type_alias'].includes(item.kind), `${label}.kind is unsupported`);
  invariant(identifierPattern.test(item.name), `${label}.name must be an identifier`);
  invariant(Array.isArray(item.namespace ?? []), `${label}.namespace must be an array`);
  invariant((item.namespace ?? []).every((segment) => identifierPattern.test(segment)), `${label}.namespace is unsafe`);
  validateOptionalDocstring(item.docstring, label);
  const qualifiedName = [packageName, ...(item.namespace ?? []), item.name].join('.');
  const idPrefix = item.kind === 'function' ? 'V' : 'T';
  invariant(item.id === `${idPrefix}:${qualifiedName}`, `${label}.id must be fully qualified as ${idPrefix}:${qualifiedName}`);

  if (item.kind === 'function') {
    validateCallable(item, label, item.id);
  } else if (item.kind === 'type_alias') {
    validateGenerics(item.generics, `${label}.generics`);
    validateType(item.resolved, `${label}.resolved`);
  } else if (item.kind === 'class') {
    validateGenerics(item.generics, `${label}.generics`);
    invariant(Array.isArray(item.fields), `${label}.fields must be an array`);
    for (const [fieldIndex, field] of item.fields.entries()) {
      invariant(field && identifierPattern.test(field.name), `${label}.fields[${fieldIndex}].name must be an identifier`);
      invariant(field.id === `F:${qualifiedName}.${field.name}`, `${label}.fields[${fieldIndex}].id must be fully qualified`);
      validateOptionalDocstring(field.docstring, `${label}.fields[${fieldIndex}]`);
      validateType(field.ty, `${label}.fields[${fieldIndex}].ty`);
    }
    invariant(Array.isArray(item.methods), `${label}.methods must be an array`);
    item.methods.forEach((method, methodIndex) => validateCallable(
      method,
      `${label}.methods[${methodIndex}]`,
      `M:${qualifiedName}.${method?.name}`,
    ));
  } else if (item.kind === 'enum') {
    invariant(Array.isArray(item.variants), `${label}.variants must be an array`);
    for (const [variantIndex, variant] of item.variants.entries()) {
      invariant(variant && identifierPattern.test(variant.name), `${label}.variants[${variantIndex}].name must be an identifier`);
      invariant(variant.id === `E:${qualifiedName}.${variant.name}`, `${label}.variants[${variantIndex}].id must be fully qualified`);
      validateOptionalDocstring(variant.docstring, `${label}.variants[${variantIndex}]`);
    }
  } else if (item.kind === 'interface') {
    invariant(Array.isArray(item.assoc_types), `${label}.assoc_types must be an array`);
    for (const [assocIndex, assoc] of item.assoc_types.entries()) {
      invariant(assoc && identifierPattern.test(assoc.name), `${label}.assoc_types[${assocIndex}].name must be an identifier`);
      invariant(assoc.id === `A:${qualifiedName}.${assoc.name}`, `${label}.assoc_types[${assocIndex}].id must be fully qualified`);
      validateOptionalDocstring(assoc.docstring, `${label}.assoc_types[${assocIndex}]`);
    }
    for (const methodGroup of ['required_methods', 'default_methods']) {
      invariant(Array.isArray(item[methodGroup]), `${label}.${methodGroup} must be an array`);
      item[methodGroup].forEach((method, methodIndex) => validateCallable(
        method,
        `${label}.${methodGroup}[${methodIndex}]`,
        `M:${qualifiedName}.${method?.name}`,
      ));
    }
  }
}

export function docsMetadataChecksumPayload(metadata) {
  const {
    kind,
    schemaVersion,
    version,
    channel,
    sourceRevision,
    releasedAt,
    toolchain,
    language,
    cli,
  } = metadata;
  return { kind, schemaVersion, version, channel, sourceRevision, releasedAt, toolchain, language, cli };
}

function validateCommand(command, index) {
  invariant(command && typeof command === 'object', `cli.commands[${index}] must be an object`);
  invariant(Array.isArray(command.path), `cli.commands[${index}].path must be an array`);
  for (const segment of command.path) {
    invariant(/^[A-Za-z0-9_-]+$/.test(segment), `unsafe CLI path segment ${JSON.stringify(segment)}`);
  }
  invariant(typeof command.description === 'string', `cli.commands[${index}].description must be a string`);
  invariant(typeof command.help === 'string' && command.help.length > 0, `cli.commands[${index}].help must be non-empty`);
  invariant(Array.isArray(command.children), `cli.commands[${index}].children must be an array`);
  for (const child of command.children) {
    invariant(child && typeof child === 'object', `cli.commands[${index}] child must be an object`);
    invariant(/^[A-Za-z0-9_-]+$/.test(child.name), `unsafe CLI child ${JSON.stringify(child.name)}`);
    invariant(typeof child.description === 'string', `CLI child ${child.name} needs a description`);
  }
}

export function validateDocsMetadata(metadata, expectedVersion, expectedChannel) {
  invariant(metadata && typeof metadata === 'object', 'root must be an object');
  invariant(metadata.kind === DOCS_METADATA_KIND, `kind must be ${DOCS_METADATA_KIND}`);
  invariant(
    metadata.schemaVersion === DOCS_METADATA_SCHEMA_VERSION,
    `unsupported schemaVersion ${JSON.stringify(metadata.schemaVersion)}`,
  );
  invariant(
    typeof metadata.version === 'string' && /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(metadata.version),
    'version must be a canonical semver',
  );
  if (expectedVersion) {
    invariant(metadata.version === expectedVersion, `expected version ${expectedVersion}, received ${metadata.version}`);
  }
  invariant(['stable', 'canary', 'nightly'].includes(metadata.channel), 'channel must be stable, canary, or nightly');
  if (expectedChannel) {
    invariant(metadata.channel === expectedChannel, `expected channel ${expectedChannel}, received ${metadata.channel}`);
  }
  invariant(/^[0-9a-f]{40}$/.test(metadata.sourceRevision), 'sourceRevision must be a full git SHA');
  invariant(typeof metadata.releasedAt === 'string' && !Number.isNaN(Date.parse(metadata.releasedAt)), 'releasedAt must be an ISO date');
  invariant(typeof metadata.toolchain === 'string' && metadata.toolchain.length > 0, 'toolchain must be non-empty');
  invariant(metadata.toolchain.split(/\s+/).includes(metadata.version), `toolchain identity does not contain release version ${metadata.version}`);

  const language = metadata.language;
  invariant(language && typeof language === 'object', 'language payload is required');
  invariant(language.formatVersion === 1, `unsupported language formatVersion ${language?.formatVersion}`);
  invariant(Array.isArray(language.packages) && language.packages.length > 0, 'language packages must be non-empty');
  const packageNames = language.packages.map((entry) => entry?.name);
  invariant(packageNames.every((name) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name)), 'language package names must be safe identifiers');
  invariant(new Set(packageNames).size === packageNames.length, 'language package names must be unique');
  const allItemIds = [];
  for (const entry of language.packages) {
    invariant(entry.export?.format_version === 1, `${entry.name} export format_version must be 1`);
    invariant(entry.export?.package === entry.name, `${entry.name} export package does not match envelope`);
    invariant(Array.isArray(entry.export?.items), `${entry.name} export items must be an array`);
    invariant(Array.isArray(entry.export?.impls), `${entry.name} export impls must be an array`);
    entry.export.items.forEach((item, index) => validateItem(item, index, entry.name));
    const itemIds = entry.export.items.map((item) => item.id);
    invariant(new Set(itemIds).size === itemIds.length, `${entry.name} item ids must be unique`);
    invariant(entry.sha256 === sha256Json(entry.export), `${entry.name} payload SHA-256 mismatch`);
    allItemIds.push(...itemIds);
  }
  invariant(new Set(allItemIds).size === allItemIds.length, 'language item ids must be globally unique');
  invariant(language.sha256 === sha256Json(language.packages), 'language package-set SHA-256 mismatch');

  const cli = metadata.cli;
  invariant(cli && typeof cli === 'object', 'cli payload is required');
  invariant(cli.formatVersion === 1, `unsupported CLI formatVersion ${cli?.formatVersion}`);
  invariant(Array.isArray(cli.commands) && cli.commands.length > 0, 'cli.commands must be non-empty');
  cli.commands.forEach(validateCommand);
  invariant(cli.commands[0].path.length === 0, 'first CLI command must be the root command');
  const paths = cli.commands.map((command) => command.path.join(' '));
  invariant(new Set(paths).size === paths.length, 'CLI command paths must be unique');
  const commandPaths = new Set(paths);
  for (const command of cli.commands) {
    for (const child of command.children) {
      const childPath = [...command.path, child.name].join(' ');
      invariant(commandPaths.has(childPath), `CLI child ${childPath} has no command payload`);
    }
    if (command.path.length > 0) {
      const parentPath = command.path.slice(0, -1);
      const parent = cli.commands.find((candidate) => candidate.path.join(' ') === parentPath.join(' '));
      invariant(parent, `CLI command ${command.path.join(' ')} has no parent`);
      invariant(
        parent.children.some((child) => child.name === command.path.at(-1)),
        `CLI parent does not list ${command.path.join(' ')}`,
      );
    }
  }
  invariant(cli.sha256 === sha256Json(cli.commands), 'CLI payload SHA-256 mismatch');

  invariant(
    metadata.payloadSha256 === sha256Json(docsMetadataChecksumPayload(metadata)),
    'metadata SHA-256 mismatch',
  );
  return metadata;
}

export async function readDocsMetadata(file, expectedVersion, expectedChannel) {
  let parsed;
  try {
    parsed = JSON.parse(await readFile(file, 'utf8'));
  } catch (error) {
    throw new Error(`Unable to read BAML docs metadata from ${file}: ${error.message}`);
  }
  return validateDocsMetadata(parsed, expectedVersion, expectedChannel);
}
