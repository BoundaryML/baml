import { readFile } from 'node:fs/promises';

import postgres from 'postgres';

import { GENERATED_CONTENT_MIGRATION_DATABASE_ENVIRONMENT_VARIABLE } from '@/lib/generated-content/constants';
import { sha256 } from '@/lib/generated-content/json';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

const migrationUrl = new URL(
  '../migrations/0002-live-document-store.sql',
  import.meta.url,
);

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['organization', 'database', 'branch'],
    ['apply', 'production'],
  );
  const target = {
    branch: requireOperatorValue(parsedArguments, 'branch'),
    database: requireOperatorValue(parsedArguments, 'database'),
    organization: requireOperatorValue(parsedArguments, 'organization'),
  };
  const migration = await readFile(migrationUrl, 'utf8');
  const migrationHash = sha256(migration);

  if (!parsedArguments.flags.has('apply')) {
    console.log(
      JSON.stringify(
        {
          migration: '0002-live-document-store.sql',
          migration_sha256: migrationHash,
          mode: 'review-only',
          target,
        },
        null,
        2,
      ),
    );
    return;
  }

  const databaseContext = process.env.DEVELOPER_DOCS_DATABASE_CONTEXT;
  if (!databaseContext) {
    throw new Error(
      'DEVELOPER_DOCS_DATABASE_CONTEXT is required when applying a migration.',
    );
  }
  if (
    databaseContext === 'production' &&
    !parsedArguments.flags.has('production')
  ) {
    throw new Error(
      'Production migration requires the explicit --production flag.',
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

  const databaseUrl =
    process.env[GENERATED_CONTENT_MIGRATION_DATABASE_ENVIRONMENT_VARIABLE];
  if (!databaseUrl) {
    throw new Error(
      `${GENERATED_CONTENT_MIGRATION_DATABASE_ENVIRONMENT_VARIABLE} is required when applying a migration.`,
    );
  }
  const sql = postgres(databaseUrl, {
    max: 1,
    prepare: false,
  });
  try {
    await sql.begin(async (transaction) => {
      await transaction.unsafe(migration);
    });
  } finally {
    await sql.end();
  }

  console.log(
    JSON.stringify(
      {
        migration: '0002-live-document-store.sql',
        migration_sha256: migrationHash,
        mode: 'applied',
        target,
      },
      null,
      2,
    ),
  );
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown migration failure.',
  );
  process.exitCode = 1;
});
