import { populateQdrant, searchQdrant } from '@/lib/qdrant-api';
import pRetry from 'p-retry';

async function waitForQdrantReady(queries: string[]) {
  for (const query of queries) {
    await pRetry(
      async () => {
        const results = await searchQdrant(query);
        if (results.length === 0) {
          throw new Error('No results found yet');
        }
        console.log('Got query results', {
          query,
          results: results.map(({ title, url }) => `${title} (${url})`),
        });
      },
      {
        retries: 30,
        minTimeout: 2000,
        factor: 1,
        onFailedAttempt: (error) => {
          console.log(`No results yet for "${query}" (attempt ${error.attemptNumber}/30)`);
        },
      },
    );
  }
}

async function main() {
  console.log('Starting Qdrant update...');
  await populateQdrant(process.argv[2]);
  console.log('Qdrant update completed successfully!');

  console.log('Testing a few Qdrant queries with retry logic...');
  await waitForQdrantReady(['dynamic types', 'alias', 'attributes']);
}

if (require.main === module) {
  main().catch((error) => {
    console.error('Error updating Qdrant:', error);
    process.exit(1);
  });
}
