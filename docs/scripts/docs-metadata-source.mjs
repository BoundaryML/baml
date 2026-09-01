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
