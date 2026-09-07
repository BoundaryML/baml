import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import postgres from 'postgres';

import { DOCUMENT_SCHEMA_VERSION } from '../lib/generated-content/constants.ts';
import {
  type DocumentReleaseBundle,
  type DocumentRouteMetadata,
  type DocumentSnapshot,
  hashDocumentManifest,
  hashDocumentSnapshot,
} from '../lib/generated-content/document-ir.ts';
import {
  closeDocumentStore,
  listStoredRoutesForVersion,
  readDocumentRoute,
} from '../lib/generated-content/document-store.ts';
import {
  promoteDocumentAlias,
  publishDocumentRelease,
} from '../lib/generated-content/publisher.ts';
import { verifyGeneratedRelease } from '../lib/generated-content/verify.ts';

const databaseUrl = process.env.DEVELOPER_DOCS_TEST_DATABASE_URL;

function bundle(version: string, path = 'cli'): DocumentReleaseBundle {
  const routeVersion = `v${version}`;
  const snapshot: DocumentSnapshot = {
    blocks: [
      {
        root: {
          arguments: [],
          command_path: [],
          description: 'Integration fixture.',
          flags: [],
          name: 'baml',
          subcommands: [],
          usage: 'baml',
        },
        type: 'cliOverview',
      },
    ],
    description: 'Integration fixture.',
    headings: [{ depth: 2, id: 'usage', label: 'Usage' }],
    schemaVersion: DOCUMENT_SCHEMA_VERSION,
    title: 'BAML CLI',
  };
  const contentHash = hashDocumentSnapshot(snapshot);
  const metadata: DocumentRouteMetadata = {
    canonicalVersion: version,
    description: 'Integration fixture.',
    generatorVersion: '4'.repeat(40),
    kind: 'cliOverview',
    publicPath: `/cli/${routeVersion}`,
    releasedAt: '2026-09-01T00:00:00.000Z',
    routeVersion,
    searchEntries: [
      { anchor: null, keywords: 'integration', label: 'BAML CLI' },
    ],
    sourceRevision: '5'.repeat(40),
    surface: 'cli',
    title: `BAML CLI ${routeVersion}`,
    wrapperVersion: '0.2.4',
  };
  const routes = [
    {
      contentHash,
      metadata,
      path,
      searchableText: 'BAML CLI Integration fixture',
      snapshot,
    },
  ];
  return {
    contentSchemaVersion: DOCUMENT_SCHEMA_VERSION,
    generatedAt: '2026-09-01T00:01:00.000Z',
    generatorVersion: '4'.repeat(40),
    manifestHash: hashDocumentManifest({
      contentSchemaVersion: DOCUMENT_SCHEMA_VERSION,
      routes,
      sourceRevision: '5'.repeat(40),
      version,
    }),
    releasedAt: '2026-09-01T00:00:00.000Z',
    routes,
    snapshots: new Map([[contentHash, snapshot]]),
    sourceRevision: '5'.repeat(40),
    version,
    wrapperVersion: '0.2.4',
  };
}

test(
  'document store publishes atomically, deduplicates, reads, and rejects mutation',
  { skip: databaseUrl ? false : 'DEVELOPER_DOCS_TEST_DATABASE_URL is unset' },
  async () => {
    if (!databaseUrl) return;
    const sql = postgres(databaseUrl, { max: 1, prepare: false });
    const migration = await readFile(
      new URL('../migrations/0002-live-document-store.sql', import.meta.url),
      'utf8',
    );
    await sql.unsafe(migration);

    const firstVersion = '99.0.0-test.document-store-a';
    const secondVersion = '99.0.0-test.document-store-b';
    const failedVersion = '99.0.0-test.document-store-failed';
    const invalidManifestVersion =
      '99.0.0-test.document-store-invalid-manifest';
    const invalidSnapshotMapVersion =
      '99.0.0-test.document-store-invalid-snapshot-map';
    const storedManifestMismatchVersion =
      '99.0.0-test.document-store-stored-manifest-mismatch';
    const firstPublication = await publishDocumentRelease(
      databaseUrl,
      bundle(firstVersion),
      'nightly',
    );
    assert.equal(firstPublication.snapshotUploadedCount, 1);
    assert.equal(firstPublication.snapshotReusedCount, 0);
    assert.equal(firstPublication.snapshotDeduplicationRatio, 0);
    assert.match(firstPublication.publishedAt, /^\d{4}-\d{2}-\d{2}T/);
    const secondPublication = await publishDocumentRelease(
      databaseUrl,
      bundle(secondVersion),
      null,
    );
    assert.equal(secondPublication.snapshotUploadedCount, 0);
    assert.equal(secondPublication.snapshotReusedCount, 1);
    assert.equal(secondPublication.snapshotDeduplicationRatio, 1);
    process.env.GENERATED_CONTENT_DATABASE_URL = databaseUrl;
    const secondVersionRoutes = await listStoredRoutesForVersion(secondVersion);
    assert.equal(secondVersionRoutes.length, 1);
    assert.equal(secondVersionRoutes[0]?.version, secondVersion);
    assert.deepEqual(
      await promoteDocumentAlias(databaseUrl, secondVersion, 'canary'),
      {
        alias: 'canary',
        aliasChanged: true,
        version: secondVersion,
      },
    );
    assert.deepEqual(
      await promoteDocumentAlias(databaseUrl, secondVersion, 'canary'),
      {
        alias: 'canary',
        aliasChanged: false,
        version: secondVersion,
      },
    );

    const snapshots = await sql`
      SELECT count(*)::integer AS count
      FROM developer_docs.doc_snapshots
      WHERE content_hash = ${bundle(firstVersion).routes[0]?.contentHash ?? ''}
    `;
    assert.equal(snapshots[0]?.count, 1);

    const loaded = await readDocumentRoute(`v${firstVersion}`, 'cli');
    assert.equal(loaded?.content.title, 'BAML CLI');

    await assert.rejects(
      publishDocumentRelease(
        databaseUrl,
        {
          ...bundle(invalidManifestVersion),
          manifestHash: '0'.repeat(64),
        },
        null,
      ),
      /manifest_hash/,
    );

    const invalidSnapshotMapBundle = bundle(invalidSnapshotMapVersion);
    const invalidSnapshotMapHash =
      invalidSnapshotMapBundle.routes[0]?.contentHash;
    assert.ok(invalidSnapshotMapHash);
    const invalidSnapshotMapValue = invalidSnapshotMapBundle.snapshots.get(
      invalidSnapshotMapHash,
    );
    assert.ok(invalidSnapshotMapValue);
    invalidSnapshotMapBundle.snapshots.set(invalidSnapshotMapHash, {
      ...invalidSnapshotMapValue,
      title: 'Tampered snapshot map value',
    });
    await assert.rejects(
      publishDocumentRelease(databaseUrl, invalidSnapshotMapBundle, null),
      /snapshot/,
    );

    const mismatchedBundle = bundle(storedManifestMismatchVersion);
    const mismatchedRoute = mismatchedBundle.routes[0];
    assert.ok(mismatchedRoute);
    await sql.begin(async (transaction) => {
      await transaction`
        INSERT INTO developer_docs.doc_releases (
          version,
          source_revision,
          released_at,
          generator_version,
          wrapper_version,
          content_schema_version,
          manifest_hash,
          route_count,
          unique_snapshot_count
        ) VALUES (
          ${mismatchedBundle.version},
          ${mismatchedBundle.sourceRevision},
          ${mismatchedBundle.releasedAt},
          ${mismatchedBundle.generatorVersion},
          ${mismatchedBundle.wrapperVersion},
          ${mismatchedBundle.contentSchemaVersion},
          ${'0'.repeat(64)},
          1,
          1
        )
      `;
      await transaction`
        INSERT INTO developer_docs.doc_routes (
          version,
          path,
          content_hash,
          route_metadata
        ) VALUES (
          ${mismatchedBundle.version},
          ${mismatchedRoute.path},
          ${mismatchedRoute.contentHash},
          ${transaction.json(mismatchedRoute.metadata)}
        )
      `;
    });
    await assert.rejects(
      verifyGeneratedRelease(storedManifestMismatchVersion),
      /manifest hash does not match/,
    );

    await assert.rejects(
      sql`
        UPDATE developer_docs.doc_releases
        SET route_count = 2
        WHERE version = ${firstVersion}
      `,
      /immutable after publication/,
    );
    await assert.rejects(
      sql`
        INSERT INTO developer_docs.doc_routes (
          version,
          path,
          content_hash,
          route_metadata
        )
        SELECT version, 'cli/appended', content_hash, route_metadata
        FROM developer_docs.doc_routes
        WHERE version = ${firstVersion} AND path = 'cli'
      `,
      /immutable after publication/,
    );

    await assert.rejects(async () => {
      await publishDocumentRelease(
        databaseUrl,
        bundle(failedVersion, '/invalid'),
        null,
      );
    });
    const failedRows = await sql`
      SELECT version
      FROM developer_docs.doc_releases
      WHERE version = ${failedVersion}
    `;
    assert.equal(failedRows.length, 0);
    await closeDocumentStore();
    await sql.end();
  },
);
