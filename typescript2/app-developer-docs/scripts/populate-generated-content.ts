import { projectDocumentRelease } from '@/lib/generated-content/document-projector';
import {
  publishDocumentRelease,
  requireGeneratedContentPublisherDatabaseUrl,
} from '@/lib/generated-content/publisher';
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

  const bundle = projectDocumentRelease(release);
  const generationSummary = {
    alias_change: channel
      ? { channel, release_version: release.version }
      : null,
    cli_command_count: countCliCommands(release.cli.payload.root),
    content_schema_version: bundle.contentSchemaVersion,
    generated_at: release.generatedAt,
    generator_version: release.generatorVersion,
    manifest_hash: bundle.manifestHash,
    mode: isDryRun ? 'dry-run' : 'publication',
    released_at: release.releasedAt,
    route_count: bundle.routes.length,
    source_commit: release.sourceCommit,
    target,
    unique_snapshot_count: bundle.snapshots.size,
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

  const publication = await publishDocumentRelease(
    requireGeneratedContentPublisherDatabaseUrl(),
    bundle,
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
