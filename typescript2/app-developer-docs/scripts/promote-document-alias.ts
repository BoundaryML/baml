import {
  promoteDocumentAlias,
  requireGeneratedContentPublisherDatabaseUrl,
} from '@/lib/generated-content/publisher';
import { channelSchema } from '@/lib/generated-content/schemas';
import { canonicalVersionToRouteVersion } from '@/lib/generated-content/versions';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['version', 'channel', 'organization', 'database', 'branch'],
    ['production'],
  );
  const version = requireOperatorValue(parsedArguments, 'version');
  canonicalVersionToRouteVersion(version);
  const channel = channelSchema.parse(
    requireOperatorValue(parsedArguments, 'channel'),
  );
  const target = {
    branch: requireOperatorValue(parsedArguments, 'branch'),
    database: requireOperatorValue(parsedArguments, 'database'),
    organization: requireOperatorValue(parsedArguments, 'organization'),
  };

  const databaseContext = process.env.DEVELOPER_DOCS_DATABASE_CONTEXT;
  if (!databaseContext) {
    throw new Error(
      'DEVELOPER_DOCS_DATABASE_CONTEXT is required for alias promotion.',
    );
  }
  if (
    databaseContext === 'production' &&
    !parsedArguments.flags.has('production')
  ) {
    throw new Error(
      'Production alias promotion requires the explicit --production flag.',
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

  const promotion = await promoteDocumentAlias(
    requireGeneratedContentPublisherDatabaseUrl(),
    version,
    channel,
  );
  console.log(JSON.stringify({ ...promotion, target }, null, 2));
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown alias promotion failure.',
  );
  process.exitCode = 1;
});
