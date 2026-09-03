import postgres from 'postgres';

import {
  GENERATED_CONTENT_DATABASE_ENVIRONMENT_VARIABLE,
  PAGE_SCHEMA_VERSION,
} from '@/lib/generated-content/constants';
import { assertSha256 } from '@/lib/generated-content/json';
import {
  type ChannelPointerRow,
  type CliArtifactPayload,
  type CliArtifactRow,
  channelPointerRowSchema,
  cliArtifactPayloadSchema,
  cliArtifactRowSchema,
  type PackageExportRow,
  packageDescribeExportSchema,
  packageExportRowSchema,
  type ReferencePageRow,
  type ReleaseRow,
  referencePageRowSchema,
  releaseRowSchema,
} from '@/lib/generated-content/schemas';

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

export interface GeneratedContentReader {
  close: () => Promise<void>;
  getCliArtifact: (
    version: string,
  ) => Promise<{ payload: CliArtifactPayload; row: CliArtifactRow } | null>;
  getReferencePage: (
    version: string,
    routePath: string,
  ) => Promise<ReferencePageRow | null>;
  listChannels: () => Promise<ChannelPointerRow[]>;
  listPackageExports: (version: string) => Promise<PackageExportRow[]>;
  listReferencePages: (version: string) => Promise<ReferencePageRow[]>;
  listReleases: () => Promise<ReleaseRow[]>;
}

export function createGeneratedContentReader(
  environment: NodeJS.ProcessEnv = process.env,
): GeneratedContentReader {
  const databaseUrl = requireGeneratedContentDatabaseUrl(environment);
  const sql = postgres(databaseUrl, {
    max: 4,
    prepare: false,
  });

  return {
    close: async () => sql.end(),
    getCliArtifact: async (version) => {
      const rows = await sql`
        SELECT
          release_version,
          wrapper_version,
          artifact_schema_version,
          source_sha256,
          payload_sha256,
          payload_json,
          generated_at
        FROM developer_docs.cli_artifacts
        WHERE release_version = ${version}
      `;
      if (rows.length === 0) {
        return null;
      }
      const row = cliArtifactRowSchema.parse(rows[0]);
      assertSha256(
        row.payload_json,
        row.payload_sha256,
        `CLI artifact ${version}`,
      );
      const payload = cliArtifactPayloadSchema.parse(
        JSON.parse(row.payload_json),
      );
      if (payload.product_version !== version) {
        throw new Error(
          `CLI payload version ${payload.product_version} does not match ${version}.`,
        );
      }
      if (payload.wrapper_version !== row.wrapper_version) {
        throw new Error(
          'CLI payload wrapper version does not match its database row.',
        );
      }
      return { payload, row };
    },
    getReferencePage: async (version, routePath) => {
      const rows = await sql`
        SELECT
          reference_pages.package_export_id,
          reference_pages.page_schema_version,
          reference_pages.qualified_name,
          reference_pages.page_kind,
          reference_pages.route_path,
          reference_pages.page_data,
          reference_pages.generated_at
        FROM developer_docs.reference_pages
        INNER JOIN developer_docs.package_exports
          ON package_exports.id = reference_pages.package_export_id
        WHERE package_exports.release_version = ${version}
          AND reference_pages.page_schema_version = ${PAGE_SCHEMA_VERSION}
          AND reference_pages.route_path = ${routePath}
      `;
      if (rows.length > 1) {
        throw new Error(
          `Reference route collision for ${version}/${routePath}.`,
        );
      }
      return rows.length === 0 ? null : referencePageRowSchema.parse(rows[0]);
    },
    listChannels: async () => {
      const rows = await sql`
        SELECT channel, release_version, updated_at
        FROM developer_docs.channel_pointers
        ORDER BY channel
      `;
      return channelPointerRowSchema.array().parse(rows);
    },
    listPackageExports: async (version) => {
      const rows = await sql`
        SELECT
          id,
          release_version,
          package_name,
          describe_format_version,
          describe_output_json,
          describe_sha256,
          generated_at
        FROM developer_docs.package_exports
        WHERE release_version = ${version}
        ORDER BY package_name
      `;
      const packageExports = packageExportRowSchema.array().parse(rows);
      for (const packageExport of packageExports) {
        assertSha256(
          packageExport.describe_output_json,
          packageExport.describe_sha256,
          `Package export ${version}/${packageExport.package_name}`,
        );
        const payload = packageDescribeExportSchema.parse(
          JSON.parse(packageExport.describe_output_json),
        );
        if (payload.package !== packageExport.package_name) {
          throw new Error(
            'Package export name does not match its database row.',
          );
        }
        if (payload.format_version !== packageExport.describe_format_version) {
          throw new Error(
            'Package export format version does not match its database row.',
          );
        }
      }
      return packageExports;
    },
    listReferencePages: async (version) => {
      const rows = await sql`
        SELECT
          reference_pages.package_export_id,
          reference_pages.page_schema_version,
          reference_pages.qualified_name,
          reference_pages.page_kind,
          reference_pages.route_path,
          reference_pages.page_data,
          reference_pages.generated_at
        FROM developer_docs.reference_pages
        INNER JOIN developer_docs.package_exports
          ON package_exports.id = reference_pages.package_export_id
        WHERE package_exports.release_version = ${version}
          AND reference_pages.page_schema_version = ${PAGE_SCHEMA_VERSION}
        ORDER BY reference_pages.route_path
      `;
      return referencePageRowSchema.array().parse(rows);
    },
    listReleases: async () => {
      const rows = await sql`
        SELECT
          version,
          source_commit,
          released_at,
          generated_at,
          generator_version,
          created_at
        FROM developer_docs.releases
        ORDER BY released_at DESC, version DESC
      `;
      return releaseRowSchema.array().parse(rows);
    },
  };
}
