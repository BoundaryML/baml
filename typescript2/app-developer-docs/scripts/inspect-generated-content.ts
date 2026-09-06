import {
  closeDocumentStore,
  listAllStoredRoutes,
  listDocumentReleaseSummaries,
} from '@/lib/generated-content/document-store';
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
  try {
    const version = requireOperatorValue(parsedArguments, 'version');
    const release = (await listDocumentReleaseSummaries()).find(
      (candidate) => candidate.release.version === version,
    );
    if (!release) {
      throw new Error(`Generated-content release ${version} does not exist.`);
    }
    const routes = (await listAllStoredRoutes()).filter(
      (route) => route.version === version,
    );
    console.log(
      JSON.stringify(
        {
          aliases: release.aliases,
          ...release.release,
          published_at: release.release.published_at.toISOString(),
          released_at: release.release.released_at.toISOString(),
          routes_by_kind: Object.fromEntries(
            Object.entries(
              Object.groupBy(routes, (route) => route.route_metadata.kind),
            ).map(([kind, entries]) => [kind, entries?.length ?? 0]),
          ),
        },
        null,
        2,
      ),
    );
  } finally {
    await closeDocumentStore();
  }
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error ? cause.message : 'Unknown inspection failure.',
  );
  process.exitCode = 1;
});
