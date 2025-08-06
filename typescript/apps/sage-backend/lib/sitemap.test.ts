import { describe, it, beforeAll, afterAll } from 'vitest';
import { writeFileSync, unlinkSync, existsSync } from 'node:fs';
import { generateSitemap, type SitemapEntry } from './sitemap';
import {
  validateSitemap,
  calculateValidationStats,
  type SitemapValidationResult,
  BASE_URL,
} from './test-utils/sitemap-validation';
import { Sema } from 'async-sema';

const TEST_DOCS_PATH = '/Users/sam/baml2/fern/docs.yml';

const MAX_CONCURRENT_REQUESTS = 30;
const FETCH_TIMEOUT_MS = 5000;

describe('Sitemap Generation and Validation', () => {
  const semaphore = new Sema(MAX_CONCURRENT_REQUESTS);

  let sitemap: SitemapEntry[];
  let validationResults: SitemapValidationResult;

  // beforeAll(async () => {
  //   // Skip tests if docs file doesn't exist
  //   if (!existsSync(TEST_DOCS_PATH)) {
  //     console.log(`⚠️ Docs file not found at ${TEST_DOCS_PATH}, skipping tests`);
  //     return;
  //   }

  //   console.log(`📖 Generating sitemap from: ${TEST_DOCS_PATH}`);
  //   console.log('🔄 Processing documentation structure...');

  //   // Generate the sitemap
  //   sitemap = await generateSitemap(TEST_DOCS_PATH);
  //   console.log(`✅ Generated sitemap with ${sitemap.length} entries`);

  //   // Validate all entries
  //   validationResults = await validateSitemap(sitemap);

  //   // Write validated sitemap to test output file
  //   writeFileSync(
  //     TEST_OUTPUT_PATH,
  //     JSON.stringify(validationResults.validEntries, null, 2),
  //   );
  //   console.log(`\n📝 Valid sitemap written to: ${TEST_OUTPUT_PATH}`);

  //   // Calculate and display stats
  //   const stats = calculateValidationStats(validationResults);
  //   console.log(`📊 Final Summary:`);
  //   console.log(`   • Internal docs: ${stats.internalCount}`);
  //   console.log(`   • External links: ${stats.externalCount}`);
  //   console.log(`   • Total valid entries: ${stats.valid}`);
  //   console.log(`   • Validation rate: ${stats.validationRate.toFixed(1)}%`);
  // }, 60000); // 60 second timeout for setup

  it('should build sitemap and validate all links', async () => {
    const entries = await generateSitemap(TEST_DOCS_PATH);

    const fetches = [];

    for (const entry of entries) {
      if (entry.type === 'external') {
        continue;
      }

      fetches.push(
        (async () => {
          const url = `${BASE_URL}/${entry.href}`;
          try {
            await semaphore.acquire();
            const response = await fetch(url, {
              method: 'GET',
              signal: AbortSignal.timeout(FETCH_TIMEOUT_MS), // 5 second timeout
            });
            if (!response.ok) {
              throw new Error(`Failed to fetch ${url}: ${response.statusText}`);
            }
            console.log(
              `✅ ${entry.href}  (${entry.displaySection.join(' > ')} > ${entry.displayTitle})`,
            );
            return { entry, success: true };
          } catch (error) {
            console.error(
              `❌ Failed to fetch ${url}: ${error} (${entry.displaySection.join(' > ')} > ${entry.displayTitle})`,
            );
            return { entry, success: false };
          } finally {
            semaphore.release();
          }
        })(),
      );
    }

    const results = await Promise.allSettled(fetches);
    const validResults = results.filter(
      (result) => result.status === 'fulfilled',
    );
    console.info('Final results:');
    console.info(`✅ Valid results: ${validResults.length}`);
    console.info(`❌ Invalid results: ${results.length - validResults.length}`);
  }, 30000);
});
