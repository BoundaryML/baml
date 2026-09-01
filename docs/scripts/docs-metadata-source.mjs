export const DEFAULT_MANIFEST_BASE_URL = 'https://pkg.boundaryml.com/manifest/v1';

export function validateChannelManifest(manifest, expectedChannel) {
  if (!manifest || typeof manifest !== 'object') throw new Error('Release channel manifest must be an object');
  if (manifest.schema !== 1) throw new Error(`Unsupported release channel manifest schema: ${manifest.schema}`);
  if (manifest.channel !== expectedChannel) {
    throw new Error(`Expected ${expectedChannel} channel manifest, received ${manifest.channel}`);
  }
  if (typeof manifest.version !== 'string' || !/^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/.test(manifest.version)) {
    throw new Error('Release channel manifest has no canonical version');
  }
  return manifest.version;
}

export function channelManifestUrl(baseUrl, channel) {
  if (!['stable', 'canary', 'nightly'].includes(channel)) {
    throw new Error(`BAML_DOCS_CHANNEL must be stable, canary, or nightly; received ${channel}`);
  }
  return `${baseUrl.replace(/\/$/, '')}/${channel}.json`;
}

export function versionMetadataUrl(baseUrl, version) {
  return `${baseUrl.replace(/\/$/, '')}/docs/v${encodeURIComponent(version)}/stdlib.json`;
}

export function versionRuntimeUrl(baseUrl, version) {
  return `${baseUrl.replace(/\/$/, '')}/docs/v${encodeURIComponent(version)}/runtime.json`;
}

export function docsVersionsIndexUrl(baseUrl) {
  return `${baseUrl.replace(/\/$/, '')}/docs/versions.json`;
}

export function validateDocsVersionsIndex(index) {
  if (!index || typeof index !== 'object') throw new Error('Docs versions index must be an object');
  if (index.schema !== 1) throw new Error(`Unsupported docs versions index schema: ${index.schema}`);
  if (!Array.isArray(index.versions) || index.versions.length === 0) {
    throw new Error('Docs versions index must contain at least one version');
  }
  const versionPattern = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/;
  const names = new Set();
  for (const [position, entry] of index.versions.entries()) {
    if (!entry || !versionPattern.test(entry.version)) throw new Error(`Docs versions index entry ${position} has an invalid version`);
    if (names.has(entry.version)) throw new Error(`Docs versions index contains duplicate version ${entry.version}`);
    names.add(entry.version);
    if (!['stable', 'canary', 'nightly'].includes(entry.channel)) {
      throw new Error(`Docs versions index entry ${entry.version} has an invalid channel`);
    }
    if (typeof entry.releasedAt !== 'string' || Number.isNaN(Date.parse(entry.releasedAt))) {
      throw new Error(`Docs versions index entry ${entry.version} has an invalid release date`);
    }
    if (!/^[0-9a-f]{40}$/.test(entry.sourceRevision)) {
      throw new Error(`Docs versions index entry ${entry.version} has an invalid source revision`);
    }
    if (entry.artifacts?.stdlib?.path !== `v${entry.version}/stdlib.json`) {
      throw new Error(`Docs versions index entry ${entry.version} has an invalid stdlib artifact path`);
    }
    if (!/^[0-9a-f]{64}$/.test(entry.artifacts.stdlib.payloadSha256)) {
      throw new Error(`Docs versions index entry ${entry.version} has an invalid stdlib payload checksum`);
    }
    // Runtime publishing starts after stdlib publishing, so retained historical
    // versions may legitimately predate this artifact. Every newly promoted
    // release includes it; if present, it must be immutable and integrity-bound.
    if (entry.artifacts.runtime !== undefined) {
      if (entry.artifacts.runtime?.path !== `v${entry.version}/runtime.json`) {
        throw new Error(`Docs versions index entry ${entry.version} has an invalid runtime artifact path`);
      }
      if (!/^[0-9a-f]{64}$/.test(entry.artifacts.runtime.payloadSha256)) {
        throw new Error(`Docs versions index entry ${entry.version} has an invalid runtime payload checksum`);
      }
    }
  }
  if (!names.has(index.defaultVersion)) {
    throw new Error(`Docs versions index default ${index.defaultVersion} is not present`);
  }
  if (!index.aliases || typeof index.aliases !== 'object' || Array.isArray(index.aliases)) {
    throw new Error('Docs versions index aliases must be an object');
  }
  for (const [alias, version] of Object.entries(index.aliases)) {
    if (!['stable', 'canary', 'nightly'].includes(alias)) throw new Error(`Docs versions index alias ${alias} is invalid`);
    if (!names.has(version)) throw new Error(`Docs versions index alias ${alias} points to unknown version ${version}`);
  }
  return index;
}

export function selectIndexedDocsVersions(index, excludedVersions = []) {
  const validated = validateDocsVersionsIndex(index);
  const excluded = new Set(excludedVersions);
  return {
    defaultVersion: validated.defaultVersion,
    versions: validated.versions.filter((entry) => !excluded.has(entry.version)),
  };
}

export function previewFallbackAllowed({ args = [], environment = process.env, explicitSelection = false } = {}) {
  if (explicitSelection) return false;
  if (args.includes('--allow-unavailable') || environment.BAML_DOCS_ALLOW_UNAVAILABLE === '1') return true;
  return environment.VERCEL_ENV === 'preview';
}

export function unavailableReferencePage(title, version, channel, url) {
  const selected = version ? `BAML ${version}` : `the ${channel} BAML channel`;
  return [
    '---',
    `title: ${JSON.stringify(title)}`,
    `description: ${JSON.stringify(`The generated ${selected} reference is not available in this preview yet.`)}`,
    '---',
    '',
    `The generated reference for ${selected} is not available in this preview yet.`,
    '',
    'Reference pages are rendered from immutable metadata produced by the BAML release pipeline. This preview will populate automatically after that artifact is published.',
    '',
    `Expected metadata: ${url}`,
    '',
  ].join('\n');
}
