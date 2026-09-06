import postgres, { type Sql } from 'postgres';

import { GENERATED_CONTENT_DATABASE_ENVIRONMENT_VARIABLE } from '@/lib/generated-content/constants';
import {
  type DocumentAliasRow,
  type DocumentReleaseRow,
  documentAliasRowSchema,
  documentReleaseRowSchema,
  type StoredDocumentRoute,
  type StoredDocumentRouteIndex,
  storedDocumentRouteIndexSchema,
  storedDocumentRouteSchema,
  verifyDocumentSnapshotHash,
} from '@/lib/generated-content/document-ir';
import {
  canonicalVersionToRouteVersion,
  routeVersionToCanonicalVersion,
} from '@/lib/generated-content/versions';

export const EXPLICIT_DOCUMENT_QUERY = `
SELECT
  routes.version,
  routes.path,
  routes.content_hash,
  routes.route_metadata,
  snapshots.schema_version,
  snapshots.content,
  snapshots.searchable_text
FROM developer_docs.doc_routes AS routes
INNER JOIN developer_docs.doc_snapshots AS snapshots
  ON snapshots.content_hash = routes.content_hash
WHERE routes.version = $1
  AND routes.path = $2
LIMIT 2
`;

export interface DocumentReleaseSummary {
  aliases: DocumentAliasRow['alias'][];
  release: DocumentReleaseRow;
  routeVersion: string;
}

declare global {
  // eslint-disable-next-line no-var -- one pool is shared across hot reloads.
  var developerDocsSql: Sql | undefined;
}

export function requireGeneratedContentDatabaseUrl(
  environment: NodeJS.ProcessEnv = process.env,
): string {
  const databaseUrl =
    environment[GENERATED_CONTENT_DATABASE_ENVIRONMENT_VARIABLE];
  if (!databaseUrl) {
    throw new Error(
      `${GENERATED_CONTENT_DATABASE_ENVIRONMENT_VARIABLE} is required for generated-content database access.`,
    );
  }
  return databaseUrl;
}

function runtimeSql(): Sql {
  globalThis.developerDocsSql ??= postgres(
    requireGeneratedContentDatabaseUrl(),
    {
      max: 3,
      prepare: false,
    },
  );
  return globalThis.developerDocsSql;
}

export async function closeDocumentStore(): Promise<void> {
  const sql = globalThis.developerDocsSql;
  globalThis.developerDocsSql = undefined;
  if (sql) await sql.end();
}

export async function readDocumentRoute(
  routeVersion: string,
  path: string,
): Promise<StoredDocumentRoute | null> {
  const version = routeVersionToCanonicalVersion(routeVersion);
  if (!version) return null;

  const rows = await runtimeSql().unsafe(EXPLICIT_DOCUMENT_QUERY, [
    version,
    path,
  ]);
  if (rows.length > 1) {
    throw new Error(`Document route collision for ${version}/${path}.`);
  }
  if (rows.length === 0) return null;

  const document = storedDocumentRouteSchema.parse(rows[0]);
  verifyDocumentSnapshotHash(document.content, document.content_hash);
  return document;
}

export async function listDocumentReleaseSummaries(): Promise<
  DocumentReleaseSummary[]
> {
  const sql = runtimeSql();
  const [releaseRows, aliasRows] = await Promise.all([
    sql`
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
      ORDER BY released_at DESC, version DESC
    `,
    sql`
      SELECT alias, version, updated_at
      FROM developer_docs.doc_aliases
      ORDER BY alias
    `,
  ]);
  const releases = documentReleaseRowSchema.array().parse(releaseRows);
  const aliases = documentAliasRowSchema.array().parse(aliasRows);
  return releases.map((release) => ({
    aliases: aliases
      .filter((alias) => alias.version === release.version)
      .map((alias) => alias.alias),
    release,
    routeVersion: canonicalVersionToRouteVersion(release.version),
  }));
}

export async function listDocumentVersionOptions(path: string): Promise<
  {
    aliases: DocumentAliasRow['alias'][];
    href: string;
    routeVersion: string;
  }[]
> {
  const rows = await runtimeSql()`
    SELECT
      routes.version,
      routes.route_metadata,
      COALESCE(
        array_agg(aliases.alias ORDER BY aliases.alias)
          FILTER (WHERE aliases.alias IS NOT NULL),
        ARRAY[]::text[]
      ) AS aliases
    FROM developer_docs.doc_routes AS routes
    INNER JOIN developer_docs.doc_releases AS releases
      ON releases.version = routes.version
    LEFT JOIN developer_docs.doc_aliases AS aliases
      ON aliases.version = routes.version
    WHERE routes.path = ${path}
    GROUP BY routes.version, routes.route_metadata, releases.released_at
    ORDER BY releases.released_at DESC, routes.version DESC
  `;

  return rows.map((row) => {
    const routeVersion = canonicalVersionToRouteVersion(String(row.version));
    const metadata = storedDocumentRouteSchema.shape.route_metadata.parse(
      row.route_metadata,
    );
    return {
      aliases: documentAliasRowSchema.shape.alias.array().parse(row.aliases),
      href: metadata.publicPath,
      routeVersion,
    };
  });
}

export async function listAllStoredRoutes(): Promise<StoredDocumentRoute[]> {
  const rows = await runtimeSql()`
    SELECT
      routes.version,
      routes.path,
      routes.content_hash,
      routes.route_metadata,
      snapshots.schema_version,
      snapshots.content,
      snapshots.searchable_text
    FROM developer_docs.doc_routes AS routes
    INNER JOIN developer_docs.doc_snapshots AS snapshots
      ON snapshots.content_hash = routes.content_hash
    INNER JOIN developer_docs.doc_releases AS releases
      ON releases.version = routes.version
    ORDER BY releases.released_at DESC, routes.path
  `;
  const documents = storedDocumentRouteSchema.array().parse(rows);
  for (const document of documents) {
    verifyDocumentSnapshotHash(document.content, document.content_hash);
  }
  return documents;
}

export async function listStoredRouteIndex(): Promise<
  StoredDocumentRouteIndex[]
> {
  const rows = await runtimeSql()`
    SELECT
      routes.version,
      routes.path,
      routes.content_hash,
      routes.route_metadata
    FROM developer_docs.doc_routes AS routes
    INNER JOIN developer_docs.doc_releases AS releases
      ON releases.version = routes.version
    ORDER BY releases.released_at DESC, routes.path
  `;
  return storedDocumentRouteIndexSchema.array().parse(rows);
}
