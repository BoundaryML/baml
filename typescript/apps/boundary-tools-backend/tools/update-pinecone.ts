import { populatePinecone, searchPinecone } from '@/lib/pinecone-api';
import pRetry from 'p-retry';

async function waitForPineconeReady(queries: string[]) {
  for (const query of queries) {
    await pRetry(
      async () => {
        const results = await searchPinecone(query);
        if (results.length === 0) {
          throw new Error('No results found yet');
        }
        console.log('Got query results', {
          query,
          results: results.map(({ title, url }) => `${title} (${url})`),
        });
      },
      {
        retries: 30, // 30 retries × 2s = 60s max
        minTimeout: 2000, // 2 second intervals
        factor: 1, // No exponential backoff, fixed 2s intervals
        onFailedAttempt: (error) => {
          console.log(`No results yet for "${query}" (attempt ${error.attemptNumber}/30)`);
        },
      },
    );
  }
}

async function main() {
  console.log('Starting Pinecone update...');
  await populatePinecone(process.argv[2]);
  console.log('Pinecone update completed successfully!');

  console.log('Testing a few pinecone queries with retry logic...');
  await waitForPineconeReady(['dynamic types', 'alias', 'attributes']);
}

if (require.main === module) {
  main().catch((error) => {
    console.error('Error updating Pinecone:', error);
    process.exit(1);
  });
}
