import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import postgres from 'postgres';

import { DOCUMENT_SCHEMA_VERSION } from '../lib/generated-content/constants.ts';
import {
  type DocumentReleaseBundle,
  type DocumentRouteMetadata,
  type DocumentSnapshot,
  hashDocumentSnapshot,
} from '../lib/generated-content/document-ir.ts';
import {
  closeDocumentStore,
  readDocumentRoute,
} from '../lib/generated-content/document-store.ts';
import { sha256 } from '../lib/generated-content/json.ts';
import { publishDocumentRelease } from '../lib/generated-content/publisher.ts';

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
  return {
    contentSchemaVersion: DOCUMENT_SCHEMA_VERSION,
    generatedAt: '2026-09-01T00:01:00.000Z',
    generatorVersion: '4'.repeat(40),
    manifestHash: sha256(`${version}:${path}:${contentHash}`),
    releasedAt: '2026-09-01T00:00:00.000Z',
    routes: [
      {
        contentHash,
        metadata,
        path,
        searchableText: 'BAML CLI Integration fixture',
        snapshot,
      },
    ],
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
    await publishDocumentRelease(databaseUrl, bundle(firstVersion), 'nightly');
    await publishDocumentRelease(databaseUrl, bundle(secondVersion), null);

    const snapshots = await sql`
      SELECT count(*)::integer AS count
      FROM developer_docs.doc_snapshots
      WHERE content_hash = ${bundle(firstVersion).routes[0]?.contentHash ?? ''}
    `;
    assert.equal(snapshots[0]?.count, 1);

    process.env.GENERATED_CONTENT_DATABASE_URL = databaseUrl;
    const loaded = await readDocumentRoute(`v${firstVersion}`, 'cli');
    assert.equal(loaded?.content.title, 'BAML CLI');

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

    await assert.rejects(
      publishDocumentRelease(
        databaseUrl,
        bundle(failedVersion, '/invalid'),
        null,
      ),
    );
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
