#!/usr/bin/env node

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readDocsMetadata, validateDocsMetadata } from './docs-metadata.mjs';
import {
  DEFAULT_MANIFEST_BASE_URL,
  channelManifestUrl,
  docsVersionsIndexUrl,
  previewFallbackAllowed,
  selectIndexedDocsVersions,
  unavailableReferencePage,
  validateChannelManifest,
  versionMetadataUrl,
} from './docs-metadata-source.mjs';
import { writeGeneratedTree } from './generated-content.mjs';
import { buildVersionedReferences, versionDirectory } from './versioned-reference.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const generatedRoot = path.join(packageRoot, 'generated');

class HttpStatusError extends Error {
  constructor(url, status, statusText) {
    super(`${url} returned ${status} ${statusText}`);
    this.status = status;
  }
}

async function fetchJson(url) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
      if (!response.ok) throw new HttpStatusError(url, response.status, response.statusText);
      return await response.json();
    } catch (error) {
      lastError = error;
      if (error instanceof HttpStatusError && [404, 410].includes(error.status)) throw error;
      if (attempt < 3) console.warn(`Metadata fetch attempt ${attempt} failed: ${error.message}`);
    }
  }
  throw lastError;
}

function metadataUnavailable(error) {
  return error instanceof HttpStatusError || error?.name === 'TypeError' || error?.name === 'TimeoutError';
}

function list(value) {
  return (value ?? '').split(',').map((entry) => entry.trim()).filter(Boolean);
}

function selections(environment) {
  const selected = [];
  const singleFile = environment.BAML_DOCS_METADATA_FILE;
  const singleUrl = environment.BAML_DOCS_METADATA_URL;
  const singleVersion = environment.BAML_DOCS_VERSION;
  const versionsIndexFile = environment.BAML_DOCS_VERSIONS_INDEX_FILE;
  const files = [...(singleFile ? [singleFile] : []), ...list(environment.BAML_DOCS_METADATA_FILES)];
  const urls = [...(singleUrl ? [singleUrl] : []), ...list(environment.BAML_DOCS_METADATA_URLS)];
  const versions = list(environment.BAML_DOCS_VERSIONS);
  const channels = list(environment.BAML_DOCS_CHANNELS);

  if (files.length > 0) {
    files.forEach((file, index) => selected.push({
      type: 'file',
      value: file,
      expectedVersion: files.length === 1 && index === 0 ? singleVersion : undefined,
      explicit: true,
    }));
  } else if (urls.length > 0) {
    urls.forEach((url, index) => selected.push({
      type: 'url',
      value: url,
      expectedVersion: urls.length === 1 && index === 0 ? singleVersion : undefined,
      explicit: true,
    }));
  }

  const requestedVersions = versions.length > 0
    ? versions
    : files.length === 0 && urls.length === 0 && singleVersion ? [singleVersion] : [];
  requestedVersions.forEach((version) => selected.push({ type: 'version', value: version, explicit: true }));
  channels.forEach((channel) => selected.push({ type: 'channel', value: channel, explicit: true }));
  if (versionsIndexFile) {
    selected.push({ type: 'index-file', value: versionsIndexFile, explicit: true });
  }

  if (selected.length === 0) {
    selected.push({ type: 'index', value: docsVersionsIndexUrl(environment.BAML_DOCS_MANIFEST_BASE_URL ?? DEFAULT_MANIFEST_BASE_URL), explicit: false });
  }
  return selected;
}

async function cacheMetadata(metadata) {
  const file = path.join(generatedRoot, 'metadata', versionDirectory(metadata.version), 'stdlib.json');
  await mkdir(path.dirname(file), { recursive: true });
  await writeFile(file, `${JSON.stringify(metadata, null, 2)}\n`);
}

async function loadSelection(selection, manifestBaseUrl, environment) {
  if (selection.type === 'file') {
    const metadata = await readDocsMetadata(
      path.resolve(selection.value),
      selection.expectedVersion,
      environment.BAML_DOCS_CHANNEL,
    );
    return { metadata, url: path.resolve(selection.value) };
  }

  if (selection.type === 'url') {
    const metadata = validateDocsMetadata(
      await fetchJson(selection.value),
      selection.expectedVersion,
      environment.BAML_DOCS_CHANNEL,
    );
    return { metadata, url: selection.value };
  }

  if (selection.type === 'version') {
    const url = versionMetadataUrl(manifestBaseUrl, selection.value);
    const metadata = validateDocsMetadata(await fetchJson(url), selection.value);
    return { metadata, url };
  }

  if (selection.type === 'indexed') {
    const url = `${manifestBaseUrl}/docs/${selection.path}`;
    const metadata = validateDocsMetadata(await fetchJson(url), selection.value, selection.channel);
    if (metadata.payloadSha256 !== selection.payloadSha256) {
      throw new Error(`Docs versions index checksum for BAML ${selection.value} does not match its immutable artifact`);
    }
    return { metadata, url };
  }

  const manifestUrl = channelManifestUrl(manifestBaseUrl, selection.value);
  const version = validateChannelManifest(await fetchJson(manifestUrl), selection.value);
  const url = versionMetadataUrl(manifestBaseUrl, version);
  const metadata = validateDocsMetadata(await fetchJson(url), version, selection.value);
  return { metadata, url };
}

async function writeUnavailableReferences({ channel, url, version }) {
  const provenance = {
    schemaVersion: 1,
    available: false,
    version: version ?? null,
    channel,
    metadataUrl: url,
  };
  await Promise.all([
    writeGeneratedTree(
      path.join(packageRoot, 'content', 'baml', 'language', 'reference'),
      new Map([
        ['index.md', unavailableReferencePage('Language reference unavailable', version, channel, url)],
        ['meta.json', `${JSON.stringify({ title: 'Language reference', pages: ['index'] }, null, 2)}\n`],
      ]),
    ),
    writeGeneratedTree(
      path.join(packageRoot, 'content', 'cli', 'commands'),
      new Map([
        ['index.md', unavailableReferencePage('CLI reference unavailable', version, channel, url)],
        ['meta.json', `${JSON.stringify({ title: 'Commands', pages: ['index'] }, null, 2)}\n`],
      ]),
    ),
    writeGeneratedTree(path.join(generatedRoot, 'baml'), new Map([['manifest.json', `${JSON.stringify(provenance, null, 2)}\n`]])),
    writeGeneratedTree(path.join(generatedRoot, 'cli'), new Map([['manifest.json', `${JSON.stringify(provenance, null, 2)}\n`]])),
    writeFile(path.join(generatedRoot, 'docs-versions.json'), `${JSON.stringify({ schemaVersion: 1, defaultVersion: null, versions: [] }, null, 2)}\n`),
  ]);
  console.warn(`Rendered preview placeholders because ${url} could not be loaded.`);
}

const environment = process.env;
const manifestBaseUrl = (environment.BAML_DOCS_MANIFEST_BASE_URL ?? DEFAULT_MANIFEST_BASE_URL).replace(/\/$/, '');
let requested = selections(environment);
const loaded = [];

let indexedDefaultVersion;
const explicitlySelectedVersions = new Set(requested.flatMap((selection) => {
  if (selection.type === 'version') return [selection.value];
  if (selection.expectedVersion) return [selection.expectedVersion];
  return [];
}));
const expanded = [];
for (const selection of requested) {
  if (!['index', 'index-file'].includes(selection.type)) {
    expanded.push(selection);
    continue;
  }

  try {
    const rawIndex = selection.type === 'index-file'
      ? JSON.parse(await readFile(path.resolve(selection.value), 'utf8'))
      : await fetchJson(selection.value);
    const selectedIndex = selectIndexedDocsVersions(rawIndex, explicitlySelectedVersions);
    indexedDefaultVersion ??= selectedIndex.defaultVersion;
    expanded.push(...selectedIndex.versions
      .map((entry) => ({
        type: 'indexed',
        value: entry.version,
        channel: entry.channel,
        path: entry.artifacts.stdlib.path,
        payloadSha256: entry.artifacts.stdlib.payloadSha256,
        explicit: selection.explicit,
      })));
  } catch (error) {
    const fallback = selection.type === 'index' && metadataUnavailable(error) && previewFallbackAllowed({
      args: process.argv.slice(2),
      environment,
      explicitSelection: selection.explicit,
    });
    if (!fallback) throw error;
    await mkdir(generatedRoot, { recursive: true });
    await writeUnavailableReferences({
      channel: environment.BAML_DOCS_CHANNEL ?? 'canary',
      url: selection.value,
      version: undefined,
    });
    process.exit(0);
  }
}
requested = expanded;

for (const selection of requested) {
  try {
    loaded.push(await loadSelection(selection, manifestBaseUrl, environment));
  } catch (error) {
    const fallback = !selection.explicit && metadataUnavailable(error) && previewFallbackAllowed({
      args: process.argv.slice(2),
      environment,
      explicitSelection: false,
    });
    if (!fallback) throw error;
    const channel = selection.type === 'channel'
      ? selection.value
      : selection.channel ?? environment.BAML_DOCS_CHANNEL ?? 'canary';
    const version = ['version', 'indexed'].includes(selection.type) ? selection.value : undefined;
    const url = selection.type === 'indexed'
      ? `${manifestBaseUrl}/docs/${selection.path}`
      : version ? versionMetadataUrl(manifestBaseUrl, version) : channelManifestUrl(manifestBaseUrl, channel);
    await mkdir(generatedRoot, { recursive: true });
    await writeUnavailableReferences({ channel, url, version });
    process.exit(0);
  }
}

const byVersion = new Map();
for (const entry of loaded) {
  const previous = byVersion.get(entry.metadata.version);
  if (previous && previous.payloadSha256 !== entry.metadata.payloadSha256) {
    throw new Error(`BAML ${entry.metadata.version} was selected with conflicting metadata payloads`);
  }
  byVersion.set(entry.metadata.version, entry.metadata);
}
const metadataEntries = [...byVersion.values()];
const defaultVersion = environment.BAML_DOCS_DEFAULT_VERSION
  ?? environment.BAML_DOCS_VERSION
  ?? indexedDefaultVersion
  ?? metadataEntries[0]?.version;
if (!defaultVersion) throw new Error('No BAML docs metadata versions were loaded');

for (const metadata of metadataEntries) await cacheMetadata(metadata);
const generated = buildVersionedReferences(metadataEntries, defaultVersion);
await Promise.all([
  writeGeneratedTree(path.join(packageRoot, 'content', 'baml', 'language', 'reference'), generated.bamlContent),
  writeGeneratedTree(path.join(packageRoot, 'content', 'cli', 'commands'), generated.cliContent),
  writeGeneratedTree(path.join(generatedRoot, 'baml'), generated.bamlData),
  writeGeneratedTree(path.join(generatedRoot, 'cli'), generated.cliData),
  writeFile(path.join(generatedRoot, 'docs-versions.json'), `${JSON.stringify(generated.catalog, null, 2)}\n`),
]);

console.log(`Rendered ${metadataEntries.length} immutable BAML docs version${metadataEntries.length === 1 ? '' : 's'} (${metadataEntries.map((entry) => entry.version).join(', ')}); default is ${defaultVersion}.`);
