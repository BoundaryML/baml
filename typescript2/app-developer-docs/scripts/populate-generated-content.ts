import { requireGeneratedContentDatabaseUrl } from '@/lib/generated-content/database';
import { publishCompleteRelease } from '@/lib/generated-content/publisher';
import { generateCompleteRelease } from '@/lib/generated-content/release-generator';
import { channelSchema } from '@/lib/generated-content/schemas';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

interface CliCommandTree {
  subcommands: CliCommandTree[];
}

function countCliCommands(command: CliCommandTree): number {
  return (
    1 +
    command.subcommands.reduce(
      (total, child) => total + countCliCommands(child),
      0,
    )
  );
}

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    [
      'baml-bin',
      'source-commit',
      'released-at',
      'channel',
      'organization',
      'database',
      'branch',
    ],
    ['dry-run', 'production'],
  );
  const target = {
    branch: requireOperatorValue(parsedArguments, 'branch'),
    database: requireOperatorValue(parsedArguments, 'database'),
    organization: requireOperatorValue(parsedArguments, 'organization'),
  };
  const channelValue = parsedArguments.values.get('channel');
  const channel = channelValue ? channelSchema.parse(channelValue) : null;

  const isDryRun = parsedArguments.flags.has('dry-run');
  if (isDryRun && parsedArguments.flags.has('production')) {
    throw new Error('A dry run must not claim production publication context.');
  }

  const release = await generateCompleteRelease({
    bamlBinary: requireOperatorValue(parsedArguments, 'baml-bin'),
    releasedAt: requireOperatorValue(parsedArguments, 'released-at'),
    sourceCommit: requireOperatorValue(parsedArguments, 'source-commit'),
  });

  const generationSummary = {
    channel_pointer_change: channel
      ? { channel, release_version: release.version }
      : null,
    cli: {
      artifact_schema_version: release.cli.artifactSchemaVersion,
      command_count: countCliCommands(release.cli.payload.root),
      payload_sha256: release.cli.payloadSha256,
      source_sha256: release.cli.sourceSha256,
      wrapper_version: release.wrapperVersion,
    },
    generated_at: release.generatedAt,
    generator_version: release.generatorVersion,
    mode: isDryRun ? 'dry-run' : 'publication',
    packages: release.packages.map((packageInput) => ({
      describe_format_version: packageInput.describeFormatVersion,
      describe_sha256: packageInput.describeSha256,
      package_name: packageInput.packageName,
      projected_page_count: packageInput.pages.length,
    })),
    released_at: release.releasedAt,
    source_commit: release.sourceCommit,
    target,
    total_projected_pages: release.packages.reduce(
      (total, packageInput) => total + packageInput.pages.length,
      0,
    ),
    version: release.version,
    writes_performed: !isDryRun,
  };

  if (isDryRun) {
    console.log(JSON.stringify(generationSummary, null, 2));
    return;
  }

  const databaseContext = process.env.DEVELOPER_DOCS_DATABASE_CONTEXT;
  if (!databaseContext) {
    throw new Error(
      'DEVELOPER_DOCS_DATABASE_CONTEXT is required for database publication.',
    );
  }
  if (
    databaseContext === 'production' &&
    !parsedArguments.flags.has('production')
  ) {
    throw new Error(
      'Production publication requires the explicit --production flag.',
    );
  }
  if (
    parsedArguments.flags.has('production') &&
    databaseContext !== 'production'
  ) {
    throw new Error(
      '--production requires DEVELOPER_DOCS_DATABASE_CONTEXT=production.',
    );
  }

  const publication = await publishCompleteRelease(
    requireGeneratedContentDatabaseUrl(),
    release,
    channel,
  );
  console.log(JSON.stringify({ ...generationSummary, publication }, null, 2));
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown population failure.',
  );
  process.exitCode = 1;
});
