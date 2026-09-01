#!/usr/bin/env node

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readDocsMetadata, validateDocsMetadata } from './docs-metadata.mjs';
import {
  DEFAULT_MANIFEST_BASE_URL,
  channelManifestUrl,
  previewFallbackAllowed,
  unavailableReferencePage,
  validateChannelManifest,
  versionMetadataUrl,
} from './docs-metadata-source.mjs';
import { run, writeGeneratedTree } from './generated-content.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repositoryRoot = path.resolve(packageRoot, '..');
const metadataCache = path.join(packageRoot, 'generated', 'docs-metadata.json');

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

async function cacheMetadata(metadata) {
  await mkdir(path.dirname(metadataCache), { recursive: true });
  await writeFile(metadataCache, `${JSON.stringify(metadata, null, 2)}\n`);
  return metadataCache;
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
    writeGeneratedTree(
      path.join(packageRoot, 'generated', 'baml'),
      new Map([['manifest.json', `${JSON.stringify(provenance, null, 2)}\n`]]),
    ),
    writeGeneratedTree(
      path.join(packageRoot, 'generated', 'cli'),
      new Map([['manifest.json', `${JSON.stringify(provenance, null, 2)}\n`]]),
    ),
  ]);
  console.warn(`Rendered preview placeholders because ${url} could not be loaded.`);
}

const explicitFile = process.env.BAML_DOCS_METADATA_FILE;
const explicitUrl = process.env.BAML_DOCS_METADATA_URL;
const explicitVersion = process.env.BAML_DOCS_VERSION;
const explicitSelection = Boolean(explicitFile || explicitUrl || explicitVersion);
const manifestBaseUrl = (process.env.BAML_DOCS_MANIFEST_BASE_URL ?? DEFAULT_MANIFEST_BASE_URL).replace(/\/$/, '');
const channel = process.env.BAML_DOCS_CHANNEL ?? 'canary';
const expectedChannel = process.env.BAML_DOCS_CHANNEL ?? (explicitSelection ? undefined : channel);

let version = explicitVersion;
let metadataFile;
let metadataUrl = explicitUrl;

if (explicitFile) {
  metadataFile = path.resolve(explicitFile);
  const metadata = await readDocsMetadata(metadataFile, version, expectedChannel);
  version = metadata.version;
} else {
  try {
    if (!version && !metadataUrl) {
      metadataUrl = channelManifestUrl(manifestBaseUrl, channel);
      const releaseManifest = await fetchJson(metadataUrl);
      version = validateChannelManifest(releaseManifest, channel);
      metadataUrl = versionMetadataUrl(manifestBaseUrl, version);
    } else {
      metadataUrl ??= versionMetadataUrl(manifestBaseUrl, version);
    }
    const metadata = validateDocsMetadata(await fetchJson(metadataUrl), version, expectedChannel);
    version = metadata.version;
    metadataFile = await cacheMetadata(metadata);
  } catch (error) {
    if (!metadataUnavailable(error) || !previewFallbackAllowed({
      args: process.argv.slice(2),
      environment: process.env,
      explicitSelection,
    })) {
      throw error;
    }
    await writeUnavailableReferences({ channel, url: metadataUrl, version });
    process.exit(0);
  }
}

const environment = {
  ...process.env,
  BAML_DOCS_METADATA_FILE: metadataFile,
  BAML_DOCS_VERSION: version,
};
for (const generator of ['generate-baml-reference.mjs', 'generate-cli-reference.mjs']) {
  run(process.execPath, [path.join(packageRoot, 'scripts', generator)], {
    cwd: repositoryRoot,
    env: environment,
    stdio: 'inherit',
  });
}
console.log(`Rendered all derived references from immutable metadata for BAML ${version}.`);
