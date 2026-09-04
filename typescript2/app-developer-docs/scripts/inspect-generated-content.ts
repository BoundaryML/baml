import { createGeneratedContentReader } from '@/lib/generated-content/database';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['version'],
    [],
  );
  const version = requireOperatorValue(parsedArguments, 'version');
  const reader = createGeneratedContentReader();
  try {
    const [releases, channels, packageExports, referencePages, cliArtifact] =
      await Promise.all([
        reader.listReleases(),
        reader.listChannels(),
        reader.listPackageExports(version),
        reader.listReferencePages(version),
        reader.getCliArtifact(version),
      ]);
    const release = releases.find((candidate) => candidate.version === version);
    if (!release) {
      throw new Error(`Generated-content release ${version} does not exist.`);
    }

    console.log(
      JSON.stringify(
        {
          channels: channels
            .filter((channel) => channel.release_version === version)
            .map((channel) => channel.channel),
          cli: cliArtifact
            ? {
                artifact_schema_version:
                  cliArtifact.row.artifact_schema_version,
                payload_sha256: cliArtifact.row.payload_sha256,
                source_sha256: cliArtifact.row.source_sha256,
                wrapper_version: cliArtifact.row.wrapper_version,
              }
            : null,
          generated_at: release.generated_at.toISOString(),
          generator_version: release.generator_version,
          package_exports: packageExports.map((item) => ({
            describe_format_version: item.describe_format_version,
            describe_sha256: item.describe_sha256,
            package_name: item.package_name,
          })),
          reference_page_count: referencePages.length,
          released_at: release.released_at.toISOString(),
          source_commit: release.source_commit,
          version: release.version,
        },
        null,
        2,
      ),
    );
  } finally {
    await reader.close();
  }
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown inspection failure.',
  );
  process.exitCode = 1;
});
