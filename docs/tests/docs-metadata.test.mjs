import assert from 'node:assert/strict';
import test from 'node:test';
import {
  DOCS_METADATA_KIND,
  DOCS_METADATA_SCHEMA_VERSION,
  docsMetadataChecksumPayload,
  sha256Json,
  validateDocsMetadata,
} from '../scripts/docs-metadata.mjs';
import {
  channelManifestUrl,
  previewFallbackAllowed,
  unavailableReferencePage,
  validateChannelManifest,
  versionMetadataUrl,
} from '../scripts/docs-metadata-source.mjs';
import { buildBamlReferenceFiles } from '../scripts/generate-baml-reference.mjs';

function metadata() {
  const emptySignature = () => ({
    params: [],
    returns: { display: 'void' },
    throws: { display: 'never' },
  });
  const bamlExport = {
    format_version: 1,
    package: 'baml',
    items: [
      { id: 'V:baml.greet', kind: 'function', name: 'greet', signature: emptySignature() },
      {
        id: 'T:baml.http.Request',
        kind: 'class',
        namespace: ['http'],
        name: 'Request',
        docstring: 'Passed to an `http.Server`; call `http.Server.bind` and use `fetch_sse` for streaming.\n```baml\nhttp.Server.bind()\n```',
        fields: [],
        methods: [],
      },
      {
        id: 'T:baml.http.Server',
        kind: 'class',
        namespace: ['http'],
        name: 'Server',
        docstring: 'Call `bind` before serving.',
        fields: [],
        methods: [{
          id: 'M:baml.http.Server.bind',
          name: 'bind',
          signature: emptySignature(),
        }],
      },
      {
        id: 'V:baml.http.fetch_sse',
        kind: 'function',
        namespace: ['http'],
        name: 'fetch_sse',
        signature: emptySignature(),
      },
    ],
    impls: [],
  };
  const aiExport = {
    format_version: 1,
    package: 'ai',
    items: [{ id: 'T:ai.Client', kind: 'class', name: 'Client', fields: [], methods: [] }],
    impls: [],
  };
  const commands = [
    {
      path: [],
      description: 'BAML command line',
      help: 'Usage: baml <COMMAND>',
      children: [{ name: 'check', description: 'Check a project' }],
    },
    {
      path: ['check'],
      description: 'Check a project',
      help: 'Usage: baml check',
      children: [],
    },
  ];
  const packages = [
    { name: 'baml', sha256: sha256Json(bamlExport), export: bamlExport },
    { name: 'ai', sha256: sha256Json(aiExport), export: aiExport },
  ];
  const language = {
    formatVersion: 1,
    sha256: sha256Json(packages),
    packages,
  };
  const cli = { formatVersion: 1, sha256: sha256Json(commands), commands };
  const value = {
    kind: DOCS_METADATA_KIND,
    schemaVersion: DOCS_METADATA_SCHEMA_VERSION,
    version: '1.2.3-nightly.20260831.a',
    channel: 'nightly',
    sourceRevision: 'a'.repeat(40),
    releasedAt: '2026-08-31T00:00:00.000Z',
    toolchain: 'baml-cli 1.2.3-nightly.20260831.a',
    language,
    cli,
  };
  value.payloadSha256 = sha256Json(docsMetadataChecksumPayload(value));
  return value;
}

test('accepts a complete versioned docs metadata envelope', () => {
  const value = metadata();
  assert.equal(validateDocsMetadata(value, value.version), value);
});

test('rejects a version other than the explicitly selected toolchain', () => {
  assert.throws(
    () => validateDocsMetadata(metadata(), '1.2.2'),
    /expected version 1\.2\.2/,
  );
});

test('rejects a channel other than the selected release channel', () => {
  const value = metadata();
  assert.throws(
    () => validateDocsMetadata(value, value.version, 'canary'),
    /expected channel canary, received nightly/,
  );
});

test('integrity-binds release provenance as well as rendered payloads', () => {
  const value = metadata();
  value.sourceRevision = 'b'.repeat(40);
  assert.throws(() => validateDocsMetadata(value, value.version, value.channel), /metadata SHA-256 mismatch/);
});

test('rejects mutated release payloads', () => {
  const value = metadata();
  value.cli.commands[1].help = 'tampered';
  assert.throws(() => validateDocsMetadata(value, value.version), /CLI payload SHA-256 mismatch/);
});

test('rejects duplicate or mismatched standard-library packages', () => {
  const duplicate = metadata();
  duplicate.language.packages[1].name = 'baml';
  duplicate.language.sha256 = sha256Json(duplicate.language.packages);
  duplicate.payloadSha256 = sha256Json(docsMetadataChecksumPayload(duplicate));
  assert.throws(() => validateDocsMetadata(duplicate, duplicate.version), /package names must be unique/);

  const mismatched = metadata();
  mismatched.language.packages[1].export.package = 'reflect';
  mismatched.language.packages[1].sha256 = sha256Json(mismatched.language.packages[1].export);
  mismatched.language.sha256 = sha256Json(mismatched.language.packages);
  mismatched.payloadSha256 = sha256Json(docsMetadataChecksumPayload(mismatched));
  assert.throws(() => validateDocsMetadata(mismatched, mismatched.version), /ai export package does not match/);
});

test('rejects incomplete CLI trees even when their checksum is valid', () => {
  const value = metadata();
  value.cli.commands.pop();
  value.cli.sha256 = sha256Json(value.cli.commands);
  value.payloadSha256 = sha256Json(docsMetadataChecksumPayload(value));
  assert.throws(() => validateDocsMetadata(value, value.version), /has no command payload/);
});

test('rejects malformed nested language items even when all hashes match', () => {
  const value = metadata();
  value.language.packages[0].export.items[1].methods.push({
    id: 'M:baml.http.Request.bad',
    name: 'bad',
    signature: { params: [{ name: 'input' }], returns: { display: 'void' }, throws: { display: 'never' } },
  });
  value.language.packages[0].sha256 = sha256Json(value.language.packages[0].export);
  value.language.sha256 = sha256Json(value.language.packages);
  value.payloadSha256 = sha256Json(docsMetadataChecksumPayload(value));
  assert.throws(() => validateDocsMetadata(value, value.version), /params\[0\]\.ty must be an object/);
});

test('renders every package with fully qualified symbol UI', () => {
  const value = metadata();
  const rendered = buildBamlReferenceFiles(value);
  assert.match(rendered.content.get('baml/functions/greet.md'), /title: "baml\.greet"/);
  assert.match(rendered.content.get('baml/functions/greet.md'), /function baml\.greet\(\) -> void/);
  assert.match(
    rendered.content.get('baml/classes/http/Request.md'),
    /`baml\.http\.Server`; call `baml\.http\.Server\.bind` and use `baml\.http\.fetch_sse`/,
  );
  assert.match(rendered.content.get('baml/classes/http/Request.md'), /baml\.http\.Server\.bind\(\)/);
  assert.match(rendered.content.get('baml/classes/http/Server.md'), /Call `baml\.http\.Server\.bind`/);
  assert.match(rendered.content.get('ai/classes/Client.md'), /title: "ai\.Client"/);
  assert.match(rendered.content.get('ai/classes/Client.md'), /class ai\.Client/);
  assert.deepEqual(JSON.parse(rendered.content.get('meta.json')).pages, ['index', 'baml', 'ai']);
  assert.deepEqual(
    JSON.parse(rendered.data.get('manifest.json')).packages.map((entry) => entry.package),
    ['baml', 'ai'],
  );
});

test('resolves a mutable channel only to its exact immutable metadata URL', () => {
  const base = 'https://pkg.boundaryml.com/manifest/v1/';
  const version = validateChannelManifest({ schema: 1, channel: 'canary', version: '1.2.3' }, 'canary');
  assert.equal(channelManifestUrl(base, 'canary'), `${base}canary.json`);
  assert.equal(versionMetadataUrl(base, version), `${base}docs/v1.2.3/stdlib.json`);
});

test('unavailable references are allowed only for implicit previews or explicit local development', () => {
  assert.equal(previewFallbackAllowed({ environment: { VERCEL_ENV: 'preview' } }), true);
  assert.equal(previewFallbackAllowed({ args: ['--allow-unavailable'], environment: {} }), true);
  assert.equal(previewFallbackAllowed({ environment: { CI: 'true' } }), false);
  assert.equal(
    previewFallbackAllowed({ environment: { VERCEL_ENV: 'preview' }, explicitSelection: true }),
    false,
  );
});

test('preview fallback copy is truthful about the missing immutable artifact', () => {
  const page = unavailableReferencePage(
    'Language reference unavailable',
    '1.2.3',
    'canary',
    'https://pkg.boundaryml.com/manifest/v1/docs/v1.2.3/stdlib.json',
  );
  assert.match(page, /not available in this preview yet/);
  assert.match(page, /immutable metadata produced by the BAML release pipeline/);
  assert.match(page, /docs\/v1\.2\.3\/stdlib\.json/);
});
