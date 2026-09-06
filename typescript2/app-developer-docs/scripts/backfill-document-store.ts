import postgres from 'postgres';

import { projectDocumentRelease } from '@/lib/generated-content/document-projector';
import {
  publishDocumentRelease,
  requireGeneratedContentPublisherDatabaseUrl,
} from '@/lib/generated-content/publisher';
import type { CompleteReleasePublicationInput } from '@/lib/generated-content/release-generator';
import {
  channelSchema,
  cliArtifactPayloadSchema,
  cliArtifactRowSchema,
  packageExportRowSchema,
  referencePageRowSchema,
  releaseRowSchema,
} from '@/lib/generated-content/schemas';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

async function readLegacyRelease(
  databaseUrl: string,
  version: string,
): Promise<CompleteReleasePublicationInput> {
  const sql = postgres(databaseUrl, { max: 1, prepare: false });
  try {
    const releaseRows = await sql`
      SELECT version, source_commit, released_at, generated_at, generator_version, created_at
      FROM developer_docs.releases
      WHERE version = ${version}
    `;
    if (releaseRows.length !== 1) {
      throw new Error(`Expected one legacy release for ${version}.`);
    }
    const release = releaseRowSchema.parse(releaseRows[0]);
    const packageExports = packageExportRowSchema.array().parse(
      await sql`
      SELECT id, release_version, package_name, describe_format_version,
        describe_output_json, describe_sha256, generated_at
      FROM developer_docs.package_exports
      WHERE release_version = ${version}
      ORDER BY package_name
    `,
    );
    const pages = referencePageRowSchema.array().parse(
      await sql`
      SELECT pages.package_export_id, pages.page_schema_version,
        pages.qualified_name, pages.page_kind, pages.route_path,
        pages.page_data, pages.generated_at
      FROM developer_docs.reference_pages AS pages
      INNER JOIN developer_docs.package_exports AS packages
        ON packages.id = pages.package_export_id
      WHERE packages.release_version = ${version}
      ORDER BY pages.route_path
    `,
    );
    const cliRows = await sql`
      SELECT release_version, wrapper_version, artifact_schema_version,
        source_sha256, payload_sha256, payload_json, generated_at
      FROM developer_docs.cli_artifacts
      WHERE release_version = ${version}
    `;
    if (cliRows.length !== 1) {
      throw new Error(`Expected one legacy CLI artifact for ${version}.`);
    }
    const cliRow = cliArtifactRowSchema.parse(cliRows[0]);
    const payload = cliArtifactPayloadSchema.parse(
      JSON.parse(cliRow.payload_json),
    );

    return {
      cli: {
        artifactSchemaVersion: cliRow.artifact_schema_version,
        payload,
        payloadJson: cliRow.payload_json,
        payloadSha256: cliRow.payload_sha256,
        productVersion: payload.product_version,
        sourceSha256: cliRow.source_sha256,
        wrapperVersion: cliRow.wrapper_version,
      },
      generatedAt: release.generated_at.toISOString(),
      generatorVersion: release.generator_version,
      packages: packageExports.map((packageExport) => ({
        describeFormatVersion: packageExport.describe_format_version,
        describeOutputJson: packageExport.describe_output_json,
        describeSha256: packageExport.describe_sha256,
        packageName: packageExport.package_name,
        pages: pages
          .filter(
            (page) =>
              String(page.package_export_id) === String(packageExport.id),
          )
          .map((page) => ({
            pageData: page.page_data,
            pageKind: page.page_kind,
            qualifiedName: page.qualified_name,
            routePath: page.route_path,
          })),
      })),
      releasedAt: release.released_at.toISOString(),
      sourceCommit: release.source_commit,
      version: release.version,
      wrapperVersion: cliRow.wrapper_version,
    };
  } finally {
    await sql.end();
  }
}

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['version', 'channel'],
    [],
  );
  const version = requireOperatorValue(parsedArguments, 'version');
  const channelValue = parsedArguments.values.get('channel');
  const channel = channelValue ? channelSchema.parse(channelValue) : null;
  const databaseUrl = requireGeneratedContentPublisherDatabaseUrl();
  const release = await readLegacyRelease(databaseUrl, version);
  const publication = await publishDocumentRelease(
    databaseUrl,
    projectDocumentRelease(release),
    channel,
  );
  console.log(JSON.stringify(publication, null, 2));
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown backfill failure.',
  );
  process.exitCode = 1;
});
