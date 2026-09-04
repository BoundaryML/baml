import postgres from 'postgres';

import { PAGE_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import {
  assertSha256,
  canonicalJson,
  jsonValueSchema,
} from '@/lib/generated-content/json';
import type { CompleteReleasePublicationInput } from '@/lib/generated-content/release-generator';
import {
  type ChannelPointerRow,
  cliArtifactRowSchema,
  packageExportRowSchema,
  referencePageDataSchema,
  referencePageRowSchema,
  releaseRowSchema,
} from '@/lib/generated-content/schemas';

export interface PublicationSummary {
  channel: ChannelPointerRow['channel'] | null;
  channel_changed: boolean;
  cli_artifacts: number;
  mode: 'inserted' | 'verified-existing';
  package_exports: number;
  reference_pages: number;
  version: string;
}

function assertEqual(
  actual: string | number,
  expected: string | number,
  label: string,
): void {
  if (actual !== expected) {
    throw new Error(
      `Immutable release mismatch for ${label}: expected ${JSON.stringify(expected)}, found ${JSON.stringify(actual)}.`,
    );
  }
}

function pageFingerprint(page: {
  pageData: unknown;
  pageKind: string;
  qualifiedName: string;
  routePath: string;
}): string {
  return canonicalJson(
    jsonValueSchema.parse({
      page_data: referencePageDataSchema.parse(page.pageData),
      page_kind: page.pageKind,
      page_schema_version: PAGE_SCHEMA_VERSION,
      qualified_name: page.qualifiedName,
      route_path: page.routePath,
    }),
  );
}

function storedPageFingerprint(page: {
  page_data: unknown;
  page_kind: string;
  page_schema_version: number;
  qualified_name: string;
  route_path: string;
}): string {
  return canonicalJson(
    jsonValueSchema.parse({
      page_data: referencePageDataSchema.parse(page.page_data),
      page_kind: page.page_kind,
      page_schema_version: page.page_schema_version,
      qualified_name: page.qualified_name,
      route_path: page.route_path,
    }),
  );
}

function validatePublicationInput(
  release: CompleteReleasePublicationInput,
): void {
  const packageNames = new Set<string>();
  const releaseRoutes = new Set<string>();
  for (const packageInput of release.packages) {
    if (packageNames.has(packageInput.packageName)) {
      throw new Error(
        `Duplicate package publication input: ${packageInput.packageName}.`,
      );
    }
    packageNames.add(packageInput.packageName);
    assertSha256(
      packageInput.describeOutputJson,
      packageInput.describeSha256,
      `Package export ${packageInput.packageName}`,
    );
    for (const page of packageInput.pages) {
      referencePageDataSchema.parse(page.pageData);
      if (releaseRoutes.has(page.routePath)) {
        throw new Error(`Release-wide route collision: ${page.routePath}.`);
      }
      releaseRoutes.add(page.routePath);
    }
  }
  assertSha256(
    release.cli.payloadJson,
    release.cli.payloadSha256,
    `CLI payload ${release.version}`,
  );
}

export async function publishCompleteRelease(
  databaseUrl: string,
  release: CompleteReleasePublicationInput,
  channel: ChannelPointerRow['channel'] | null,
): Promise<PublicationSummary> {
  validatePublicationInput(release);
  const sql = postgres(databaseUrl, { max: 1, prepare: false });

  try {
    return await sql.begin(async (transaction) => {
      await transaction`
        SELECT pg_advisory_xact_lock(hashtextextended(${release.version}, 0))
      `;
      const releaseRows = await transaction`
        SELECT
          version,
          source_commit,
          released_at,
          generated_at,
          generator_version,
          created_at
        FROM developer_docs.releases
        WHERE version = ${release.version}
        FOR UPDATE
      `;
      const existingRelease =
        releaseRows.length === 0
          ? null
          : releaseRowSchema.parse(releaseRows[0]);
      const mode = existingRelease ? 'verified-existing' : 'inserted';

      if (existingRelease) {
        assertEqual(
          existingRelease.source_commit,
          release.sourceCommit,
          'source_commit',
        );
        assertEqual(
          existingRelease.released_at.toISOString(),
          new Date(release.releasedAt).toISOString(),
          'released_at',
        );
        assertEqual(
          existingRelease.generator_version,
          release.generatorVersion,
          'generator_version',
        );
      } else {
        await transaction`
          INSERT INTO developer_docs.releases (
            version,
            source_commit,
            released_at,
            generated_at,
            generator_version
          ) VALUES (
            ${release.version},
            ${release.sourceCommit},
            ${release.releasedAt},
            ${release.generatedAt},
            ${release.generatorVersion}
          )
        `;
      }

      const storedPackageRows = packageExportRowSchema.array().parse(
        await transaction`
          SELECT
            id,
            release_version,
            package_name,
            describe_format_version,
            describe_output_json,
            describe_sha256,
            generated_at
          FROM developer_docs.package_exports
          WHERE release_version = ${release.version}
          ORDER BY package_name
        `,
      );

      if (existingRelease) {
        assertEqual(
          storedPackageRows.length,
          release.packages.length,
          'package export count',
        );
      }

      for (const packageInput of release.packages) {
        let packageExport = storedPackageRows.find(
          (row) => row.package_name === packageInput.packageName,
        );
        if (existingRelease) {
          if (!packageExport) {
            throw new Error(
              `Immutable release is missing package ${packageInput.packageName}.`,
            );
          }
          assertEqual(
            packageExport.describe_format_version,
            packageInput.describeFormatVersion,
            `${packageInput.packageName}.describe_format_version`,
          );
          assertEqual(
            packageExport.describe_sha256,
            packageInput.describeSha256,
            `${packageInput.packageName}.describe_sha256`,
          );
          assertEqual(
            packageExport.describe_output_json,
            packageInput.describeOutputJson,
            `${packageInput.packageName}.describe_output_json`,
          );
        } else {
          const insertedRows = await transaction`
            INSERT INTO developer_docs.package_exports (
              release_version,
              package_name,
              describe_format_version,
              describe_output_json,
              describe_sha256,
              generated_at
            ) VALUES (
              ${release.version},
              ${packageInput.packageName},
              ${packageInput.describeFormatVersion},
              ${packageInput.describeOutputJson},
              ${packageInput.describeSha256},
              ${release.generatedAt}
            )
            RETURNING
              id,
              release_version,
              package_name,
              describe_format_version,
              describe_output_json,
              describe_sha256,
              generated_at
          `;
          packageExport = packageExportRowSchema.parse(insertedRows[0]);
        }

        const storedPages = referencePageRowSchema.array().parse(
          await transaction`
            SELECT
              package_export_id,
              page_schema_version,
              qualified_name,
              page_kind,
              route_path,
              page_data,
              generated_at
            FROM developer_docs.reference_pages
            WHERE package_export_id = ${String(packageExport.id)}
              AND page_schema_version = ${PAGE_SCHEMA_VERSION}
            ORDER BY route_path
          `,
        );

        if (existingRelease) {
          const expectedFingerprints = packageInput.pages
            .map(pageFingerprint)
            .sort();
          const storedFingerprints = storedPages
            .map(storedPageFingerprint)
            .sort();
          assertEqual(
            canonicalJson(expectedFingerprints),
            canonicalJson(storedFingerprints),
            `${packageInput.packageName}.reference_pages`,
          );
        } else {
          const pageRows = packageInput.pages.map((page) => ({
            package_export_id: String(packageExport.id),
            page_data: page.pageData,
            page_kind: page.pageKind,
            page_schema_version: PAGE_SCHEMA_VERSION,
            qualified_name: page.qualifiedName,
            route_path: page.routePath,
          }));
          await transaction`
            INSERT INTO developer_docs.reference_pages (
              package_export_id,
              page_schema_version,
              qualified_name,
              page_kind,
              route_path,
              page_data,
              generated_at
            )
            SELECT
              page.package_export_id,
              page.page_schema_version,
              page.qualified_name,
              page.page_kind,
              page.route_path,
              page.page_data,
              ${release.generatedAt}::timestamptz
            FROM jsonb_to_recordset(${transaction.json(pageRows)}::jsonb) AS page(
              package_export_id bigint,
              page_schema_version integer,
              qualified_name text,
              page_kind text,
              route_path text,
              page_data jsonb
            )
          `;
        }
      }

      const cliRows = await transaction`
        SELECT
          release_version,
          wrapper_version,
          artifact_schema_version,
          source_sha256,
          payload_sha256,
          payload_json,
          generated_at
        FROM developer_docs.cli_artifacts
        WHERE release_version = ${release.version}
      `;
      if (existingRelease) {
        if (cliRows.length !== 1) {
          throw new Error(
            'Immutable release must contain exactly one CLI artifact.',
          );
        }
        const cliRow = cliArtifactRowSchema.parse(cliRows[0]);
        assertEqual(
          cliRow.wrapper_version,
          release.wrapperVersion,
          'cli.wrapper_version',
        );
        assertEqual(
          cliRow.artifact_schema_version,
          release.cli.artifactSchemaVersion,
          'cli.artifact_schema_version',
        );
        assertEqual(
          cliRow.source_sha256,
          release.cli.sourceSha256,
          'cli.source_sha256',
        );
        assertEqual(
          cliRow.payload_sha256,
          release.cli.payloadSha256,
          'cli.payload_sha256',
        );
        assertEqual(
          cliRow.payload_json,
          release.cli.payloadJson,
          'cli.payload_json',
        );
      } else {
        await transaction`
          INSERT INTO developer_docs.cli_artifacts (
            release_version,
            wrapper_version,
            artifact_schema_version,
            source_sha256,
            payload_sha256,
            payload_json,
            generated_at
          ) VALUES (
            ${release.version},
            ${release.wrapperVersion},
            ${release.cli.artifactSchemaVersion},
            ${release.cli.sourceSha256},
            ${release.cli.payloadSha256},
            ${release.cli.payloadJson},
            ${release.generatedAt}
          )
        `;
      }

      let channelChanged = false;
      if (channel) {
        const pointerRows = await transaction`
          SELECT release_version
          FROM developer_docs.channel_pointers
          WHERE channel = ${channel}
          FOR UPDATE
        `;
        channelChanged =
          pointerRows.length === 0 ||
          pointerRows[0]?.release_version !== release.version;
        await transaction`
          INSERT INTO developer_docs.channel_pointers (
            channel,
            release_version
          ) VALUES (
            ${channel},
            ${release.version}
          )
          ON CONFLICT (channel) DO UPDATE SET
            release_version = EXCLUDED.release_version,
            updated_at = now()
          WHERE developer_docs.channel_pointers.release_version IS DISTINCT FROM EXCLUDED.release_version
        `;
      }

      return {
        channel,
        channel_changed: channelChanged,
        cli_artifacts: 1,
        mode,
        package_exports: release.packages.length,
        reference_pages: release.packages.reduce(
          (total, packageInput) => total + packageInput.pages.length,
          0,
        ),
        version: release.version,
      };
    });
  } finally {
    await sql.end();
  }
}
