import { appendFile } from 'node:fs/promises';
import { closeDocumentStore } from '@/lib/generated-content/document-store';
import { verifyGeneratedRelease } from '@/lib/generated-content/verify';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['version'],
    ['github-output'],
  );
  const version = requireOperatorValue(parsedArguments, 'version');
  try {
    const summary = await verifyGeneratedRelease(version);
    console.log(JSON.stringify(summary, null, 2));
    if (parsedArguments.flags.has('github-output')) {
      const outputPath = process.env.GITHUB_OUTPUT;
      if (!outputPath) throw new Error('GITHUB_OUTPUT is required.');
      if (!summary.csv_record_path) {
        throw new Error(
          'The HTTP contract requires a published CSV record page.',
        );
      }
      await appendFile(
        outputPath,
        `version=${summary.version}\ncsv-record-path=${summary.csv_record_path}\n`,
      );
    }
  } finally {
    await closeDocumentStore();
  }
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown verification failure.',
  );
  process.exitCode = 1;
});
