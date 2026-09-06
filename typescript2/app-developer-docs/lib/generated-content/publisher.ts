import postgres from 'postgres';

import { GENERATED_CONTENT_PUBLISHER_DATABASE_ENVIRONMENT_VARIABLE } from '@/lib/generated-content/constants';
import {
  type DocumentAliasRow,
  type DocumentReleaseBundle,
  type DocumentRouteInput,
  documentReleaseRowSchema,
  documentRouteMetadataSchema,
  documentSnapshotSchema,
  hashDocumentSnapshot,
} from '@/lib/generated-content/document-ir';
import { canonicalJson, jsonValueSchema } from '@/lib/generated-content/json';

export interface DocumentPublicationSummary {
  alias: DocumentAliasRow['alias'] | null;
  aliasChanged: boolean;
  manifestHash: string;
  mode: 'inserted' | 'verified-existing';
  routeCount: number;
  uniqueSnapshotCount: number;
  version: string;
}

export function requireGeneratedContentPublisherDatabaseUrl(
  environment: NodeJS.ProcessEnv = process.env,
): string {
  const databaseUrl =
    environment[GENERATED_CONTENT_PUBLISHER_DATABASE_ENVIRONMENT_VARIABLE];
  if (!databaseUrl) {
    throw new Error(
      `${GENERATED_CONTENT_PUBLISHER_DATABASE_ENVIRONMENT_VARIABLE} is required for document publication.`,
    );
  }
  return databaseUrl;
}

function routeFingerprint(route: {
  contentHash: string;
  metadata: unknown;
  path: string;
}): string {
  return canonicalJson(
    jsonValueSchema.parse({
      contentHash: route.contentHash,
      metadata: documentRouteMetadataSchema.parse(route.metadata),
      path: route.path,
    }),
  );
}

function storedRouteFingerprint(route: {
  content_hash: string;
  path: string;
  route_metadata: unknown;
}): string {
  return routeFingerprint({
    contentHash: route.content_hash,
    metadata: route.route_metadata,
    path: route.path,
  });
}

function assertEqual(
  actual: string | number,
  expected: string | number,
  label: string,
): void {
  if (actual !== expected) {
    throw new Error(
      `Immutable document release mismatch for ${label}: expected ${JSON.stringify(expected)}, found ${JSON.stringify(actual)}.`,
    );
  }
}

function validateBundle(bundle: DocumentReleaseBundle): void {
  if (bundle.routes.length === 0 || bundle.snapshots.size === 0) {
    throw new Error('A document release must contain routes and snapshots.');
  }
  if (bundle.snapshots.size > bundle.routes.length) {
    throw new Error('Unique snapshot count cannot exceed route count.');
  }
  const paths = new Set<string>();
  for (const route of bundle.routes) {
    if (paths.has(route.path)) {
      throw new Error(`Duplicate document route: ${route.path}.`);
    }
    paths.add(route.path);
    const snapshot = documentSnapshotSchema.parse(route.snapshot);
    assertEqual(
      hashDocumentSnapshot(snapshot),
      route.contentHash,
      `${route.path}.contentHash`,
    );
    if (!bundle.snapshots.has(route.contentHash)) {
      throw new Error(`Route ${route.path} refers to a missing snapshot.`);
    }
  }
}

function snapshotRows(bundle: DocumentReleaseBundle) {
  const searchableTextByHash = new Map<string, string>();
  for (const route of bundle.routes) {
    const previous = searchableTextByHash.get(route.contentHash);
    if (previous && previous !== route.searchableText) {
      throw new Error(
        `Snapshot ${route.contentHash} has inconsistent searchable text.`,
      );
    }
    searchableTextByHash.set(route.contentHash, route.searchableText);
  }
  return [...bundle.snapshots].map(([contentHash, content]) => ({
    content,
    content_hash: contentHash,
    schema_version: bundle.contentSchemaVersion,
    searchable_text: searchableTextByHash.get(contentHash) ?? '',
  }));
}

async function uploadMissingSnapshots(
  databaseUrl: string,
  bundle: DocumentReleaseBundle,
): Promise<void> {
  const sql = postgres(databaseUrl, { max: 1, prepare: false });
  const snapshots = snapshotRows(bundle);
  const snapshotsByHash = new Map(
    snapshots.map((snapshot) => [snapshot.content_hash, snapshot]),
  );
  try {
    const existingRows = await sql`
      SELECT content_hash, schema_version
      FROM developer_docs.doc_snapshots
      WHERE content_hash = ANY(${[...snapshotsByHash.keys()]})
    `;
    for (const existingRow of existingRows) {
      const contentHash = String(existingRow.content_hash);
      const expected = snapshotsByHash.get(contentHash);
      if (!expected)
        throw new Error(`Unexpected stored snapshot ${contentHash}.`);
      assertEqual(
        Number(existingRow.schema_version),
        expected.schema_version,
        `${contentHash}.schema_version`,
      );
    }
    const existingHashes = new Set(
      existingRows.map((row) => String(row.content_hash)),
    );
    const missingSnapshots = snapshots.filter(
      (snapshot) => !existingHashes.has(snapshot.content_hash),
    );

    for (let index = 0; index < missingSnapshots.length; index += 250) {
      const chunk = missingSnapshots.slice(index, index + 250);
      await sql`
        INSERT INTO developer_docs.doc_snapshots (
          content_hash,
          schema_version,
          content,
          searchable_text
        )
        SELECT
          snapshot.content_hash,
          snapshot.schema_version,
          snapshot.content,
          snapshot.searchable_text
        FROM jsonb_to_recordset(${sql.json(jsonValueSchema.parse(chunk))}::jsonb) AS snapshot(
          content_hash text,
          schema_version integer,
          content jsonb,
          searchable_text text
        )
        ON CONFLICT (content_hash) DO NOTHING
      `;
    }

    if (missingSnapshots.length > 0) {
      const uploadedRows = await sql`
        SELECT content_hash
        FROM developer_docs.doc_snapshots
        WHERE content_hash = ANY(${missingSnapshots.map((snapshot) => snapshot.content_hash)})
      `;
      assertEqual(
        uploadedRows.length,
        missingSnapshots.length,
        'uploaded snapshot count',
      );
    }
  } finally {
    await sql.end();
  }
}

function routeRows(routes: readonly DocumentRouteInput[]) {
  return routes.map((route) => ({
    content_hash: route.contentHash,
    path: route.path,
    route_metadata: route.metadata,
  }));
}

export async function publishDocumentRelease(
  databaseUrl: string,
  bundle: DocumentReleaseBundle,
  alias: DocumentAliasRow['alias'] | null,
): Promise<DocumentPublicationSummary> {
  validateBundle(bundle);
  await uploadMissingSnapshots(databaseUrl, bundle);
  const sql = postgres(databaseUrl, { max: 1, prepare: false });

  try {
    return await sql.begin(async (transaction) => {
      await transaction`
        SELECT pg_advisory_xact_lock(hashtextextended(${bundle.version}, 0))
      `;
      const releaseRows = await transaction`
        SELECT
          version,
          source_revision,
          released_at,
          generator_version,
          wrapper_version,
          content_schema_version,
          manifest_hash,
          route_count,
          unique_snapshot_count,
          published_at
        FROM developer_docs.doc_releases
        WHERE version = ${bundle.version}
        FOR UPDATE
      `;
      const existingRelease =
        releaseRows.length === 0
          ? null
          : documentReleaseRowSchema.parse(releaseRows[0]);
      const mode = existingRelease ? 'verified-existing' : 'inserted';

      if (existingRelease) {
        assertEqual(
          existingRelease.source_revision,
          bundle.sourceRevision,
          'source_revision',
        );
        assertEqual(
          existingRelease.released_at.toISOString(),
          new Date(bundle.releasedAt).toISOString(),
          'released_at',
        );
        assertEqual(
          existingRelease.generator_version,
          bundle.generatorVersion,
          'generator_version',
        );
        assertEqual(
          existingRelease.wrapper_version,
          bundle.wrapperVersion,
          'wrapper_version',
        );
        assertEqual(
          existingRelease.content_schema_version,
          bundle.contentSchemaVersion,
          'content_schema_version',
        );
        assertEqual(
          existingRelease.manifest_hash,
          bundle.manifestHash,
          'manifest_hash',
        );
        assertEqual(
          existingRelease.route_count,
          bundle.routes.length,
          'route_count',
        );
        assertEqual(
          existingRelease.unique_snapshot_count,
          bundle.snapshots.size,
          'unique_snapshot_count',
        );

        const storedRoutes = await transaction`
          SELECT path, content_hash, route_metadata
          FROM developer_docs.doc_routes
          WHERE version = ${bundle.version}
          ORDER BY path
        `;
        assertEqual(
          storedRoutes.length,
          bundle.routes.length,
          'stored route count',
        );
        assertEqual(
          canonicalJson(
            storedRoutes
              .map((route) =>
                storedRouteFingerprint({
                  content_hash: String(route.content_hash),
                  path: String(route.path),
                  route_metadata: route.route_metadata,
                }),
              )
              .sort(),
          ),
          canonicalJson(bundle.routes.map(routeFingerprint).sort()),
          'routes',
        );
      } else {
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
            ${bundle.version},
            ${bundle.sourceRevision},
            ${bundle.releasedAt},
            ${bundle.generatorVersion},
            ${bundle.wrapperVersion},
            ${bundle.contentSchemaVersion},
            ${bundle.manifestHash},
            ${bundle.routes.length},
            ${bundle.snapshots.size}
          )
        `;
        await transaction`
          INSERT INTO developer_docs.doc_routes (
            version,
            path,
            content_hash,
            route_metadata
          )
          SELECT
            ${bundle.version},
            route.path,
            route.content_hash,
            route.route_metadata
          FROM jsonb_to_recordset(${transaction.json(jsonValueSchema.parse(routeRows(bundle.routes)))}::jsonb) AS route(
            path text,
            content_hash text,
            route_metadata jsonb
          )
        `;
      }

      let aliasChanged = false;
      if (alias) {
        const aliasRows = await transaction`
          SELECT version
          FROM developer_docs.doc_aliases
          WHERE alias = ${alias}
          FOR UPDATE
        `;
        aliasChanged =
          aliasRows.length === 0 || aliasRows[0]?.version !== bundle.version;
        await transaction`
          INSERT INTO developer_docs.doc_aliases (alias, version)
          VALUES (${alias}, ${bundle.version})
          ON CONFLICT (alias) DO UPDATE SET
            version = EXCLUDED.version,
            updated_at = now()
          WHERE developer_docs.doc_aliases.version IS DISTINCT FROM EXCLUDED.version
        `;
      }

      return {
        alias,
        aliasChanged,
        manifestHash: bundle.manifestHash,
        mode,
        routeCount: bundle.routes.length,
        uniqueSnapshotCount: bundle.snapshots.size,
        version: bundle.version,
      };
    });
  } finally {
    await sql.end();
  }
}
