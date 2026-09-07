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
    [],
  );
  const version = requireOperatorValue(parsedArguments, 'version');
  try {
    console.log(JSON.stringify(await verifyGeneratedRelease(version), null, 2));
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
