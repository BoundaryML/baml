import assert from 'node:assert/strict';
import test from 'node:test';
import { projectDocumentRelease } from '../lib/generated-content/document-projector.ts';
import {
  canonicalJson,
  jsonValueSchema,
  sha256,
} from '../lib/generated-content/json.ts';
import { buildReferencePages } from '../lib/generated-content/package-generator.ts';
import type { CompleteReleasePublicationInput } from '../lib/generated-content/release-generator.ts';
import {
  createMemberAnchors,
  deriveParentQualifiedName,
  qualifiedNameToRoutePath,
} from '../lib/generated-content/routes.ts';
import {
  cliArtifactPayloadSchema,
  referencePageDataSchema,
  referencePageRowSchema,
} from '../lib/generated-content/schemas.ts';

test('canonical JSON sorts object keys, preserves arrays, and normalizes negative zero', () => {
  assert.equal(
    canonicalJson({ a: [{ x: null, y: true }], z: -0 }),
    '{"a":[{"x":null,"y":true}],"z":0}',
  );
  assert.equal(sha256('BAML').length, 64);
});

test('canonical JSON rejects values that JSON would silently coerce', () => {
  assert.throws(() => canonicalJson({ value: Number.NaN }), /non-finite/);
  assert.equal(jsonValueSchema.safeParse(new Date()).success, false);
});

test('fully qualified names map directly to routes and derive parents', () => {
  assert.equal(qualifiedNameToRoutePath('baml.json.parse'), 'baml/json/parse');
  assert.equal(deriveParentQualifiedName('baml.json.parse'), 'baml.json');
  assert.equal(deriveParentQualifiedName('baml'), null);
  assert.throws(
    () => qualifiedNameToRoutePath('baml..parse'),
    /Invalid fully qualified/,
  );
});

test('member anchors use names when unique and stable exported-ID suffixes on collision', () => {
  const anchors = createMemberAnchors([
    { exportedId: 'M:baml.String.split:first', label: 'split' },
    { exportedId: 'M:baml.String.trim', label: 'trim' },
    { exportedId: 'M:baml.String.split:second', label: 'split' },
  ]);
  assert.equal(anchors[1].anchor, 'trim');
  assert.match(anchors[0].anchor, /^split-[0-9a-f]{8}$/);
  assert.match(anchors[2].anchor, /^split-[0-9a-f]{8}$/);
  assert.notEqual(anchors[0].anchor, anchors[2].anchor);
});

test('versioned page and CLI payload schemas accept the initial concrete envelopes', () => {
  const declaration = referencePageDataSchema.parse({
    cross_references: [],
    declaration: {
      docstring: 'Returns one value.',
      id: 'V:baml.json.parse',
      kind: 'function',
      methods: [],
      name: 'parse',
    },
    display_name: 'parse',
    exported_id: 'V:baml.json.parse',
    implementations: [],
    member_anchors: [],
    namespace_path: ['json'],
    package_name: 'baml',
    page_kind: 'function',
    qualified_name: 'baml.json.parse',
    schema_version: 1,
    summary: 'Returns one value.',
  });
  assert.equal(declaration.page_kind, 'function');

  const helpText = 'Build and run BAML projects.';
  const cli = cliArtifactPayloadSchema.parse({
    artifact_schema_version: 1,
    product_version: '0.18.1-nightly.20260828.a',
    raw_help: [
      {
        command_path: [],
        invocation: ['help'],
        sha256: sha256(helpText),
        text: helpText,
      },
    ],
    root: {
      arguments: [],
      command_path: [],
      description: 'Build and run BAML projects.',
      flags: [],
      name: 'baml',
      subcommands: [],
      usage: 'baml [OPTIONS] <COMMAND>',
    },
    wrapper_version: '0.2.4',
  });
  assert.equal(cli.root.name, 'baml');
});

test('reference page rows reject routes that diverge from the qualified name', () => {
  const row = {
    generated_at: new Date('2026-09-01T00:00:00Z'),
    package_export_id: 1,
    page_data: {
      children: [],
      describe_format_version: 1,
      display_name: 'baml',
      package_name: 'baml',
      page_kind: 'package',
      qualified_name: 'baml',
      schema_version: 1,
      summary: null,
    },
    page_kind: 'package',
    page_schema_version: 1,
    qualified_name: 'baml',
    route_path: 'baml',
  } as const;

  assert.equal(referencePageRowSchema.parse(row).route_path, 'baml');
  assert.throws(
    () => referencePageRowSchema.parse({ ...row, route_path: 'kind/baml' }),
    /route does not match/,
  );
});

test('only the compiler-allowlisted boundary.id namespace landing page is hidden', () => {
  const collidingItems = [
    {
      id: 'V:boundary.id',
      kind: 'function' as const,
      name: 'id',
    },
    {
      id: 'V:boundary.id.current',
      kind: 'function' as const,
      name: 'current',
      namespace: ['id'],
    },
  ];

  const boundaryPages = buildReferencePages('boundary', 1, collidingItems, []);
  assert.deepEqual(
    boundaryPages.map((page) => [page.qualifiedName, page.pageKind]),
    [
      ['boundary', 'package'],
      ['boundary.id', 'function'],
      ['boundary.id.current', 'function'],
    ],
  );

  assert.throws(
    () => buildReferencePages('not_boundary', 1, collidingItems, []),
    /Projected package route collision: not_boundary\/id/,
  );
});

function sampleRelease(version: string): CompleteReleasePublicationInput {
  const sourceHash = '1'.repeat(64);
  const payload = {
    artifact_schema_version: 1 as const,
    product_version: version,
    raw_help: [
      {
        command_path: [],
        invocation: ['help'],
        sha256: sourceHash,
        text: 'BAML help',
      },
    ],
    root: {
      arguments: [],
      command_path: [],
      description: 'Build and run BAML projects.',
      flags: [],
      name: 'baml',
      subcommands: [],
      usage: 'baml',
    },
    wrapper_version: '0.2.4',
  };
  const describeOutputJson =
    '{"format_version":1,"impls":[],"items":[],"package":"baml"}';
  return {
    cli: {
      artifactSchemaVersion: 1,
      payload,
      payloadJson: canonicalJson(payload),
      payloadSha256: sha256(canonicalJson(payload)),
      productVersion: version,
      sourceSha256: sourceHash,
      wrapperVersion: '0.2.4',
    },
    generatedAt: '2026-09-01T00:01:00.000Z',
    generatorVersion: '2'.repeat(40),
    packages: [
      {
        describeFormatVersion: 1,
        describeOutputJson,
        describeSha256: sha256(describeOutputJson),
        packageName: 'baml',
        pages: [
          {
            pageData: {
              children: [],
              describe_format_version: 1,
              display_name: 'baml',
              package_name: 'baml',
              page_kind: 'package',
              qualified_name: 'baml',
              schema_version: 1,
              summary: 'BAML standard package.',
            },
            pageKind: 'package',
            qualifiedName: 'baml',
            routePath: 'baml',
          },
        ],
      },
    ],
    releasedAt: '2026-09-01T00:00:00.000Z',
    sourceCommit: '3'.repeat(40),
    version,
    wrapperVersion: '0.2.4',
  };
}

test('document projection is deterministic and deduplicates unchanged releases', () => {
  const first = projectDocumentRelease(sampleRelease('0.18.0'));
  const repeated = projectDocumentRelease(sampleRelease('0.18.0'));
  const nextVersion = projectDocumentRelease(sampleRelease('0.18.1'));

  assert.equal(first.manifestHash, repeated.manifestHash);
  assert.deepEqual(
    first.routes.map((route) => route.path),
    ['baml/packages', 'baml/packages/baml', 'cli', 'cli/commands'],
  );
  assert.deepEqual(
    [...first.snapshots.keys()].sort(),
    [...nextVersion.snapshots.keys()].sort(),
  );
  assert.notEqual(first.manifestHash, nextVersion.manifestHash);
});
