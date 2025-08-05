#!/usr/bin/env tsx

import { writeFileSync } from 'node:fs';
import { Sema } from 'async-sema';
import { generateSitemap, type SitemapEntry } from '@/lib/sitemap';

/**
 * Executable script to generate sitemap from docs.yml, validate URLs, and write to sitemap.json
 *
 * Usage:
 *   pnpm tsx generate-sitemap.ts [docsYmlPath]
 *
 * If no path is provided, uses the hardcoded default path below.
 */

// Default path to docs.yml - update this with your actual path
const DEFAULT_DOCS_PATH =
  '/Users/egor/Documents/boundary/baml-fresh/baml/fern/docs.yml';

// Base URL for validation
const BASE_URL = 'http://localhost:3096';

// Maximum number of concurrent fetch operations
const MAX_CONCURRENT_REQUESTS = 30;

/**
 * Validate a single sitemap entry by fetching its URL with concurrency control
 */
async function validateEntryWithSemaphore(
  entry: SitemapEntry,
  semaphore: Sema
): Promise<{
  entry: SitemapEntry;
  isValid: boolean;
  status?: number;
  error?: string;
}> {
  // Skip external entries (they don't use local slugs)
  if (entry.type === 'external') {
    return { entry, isValid: true };
  }

  // Skip entries without slugs
  if (!entry.slug) {
    return { entry, isValid: false, error: 'No slug defined' };
  }

  // Acquire semaphore permit before making request
  await semaphore.acquire();

  try {
    const url = `${BASE_URL}${entry.slug}`;
    const response = await fetch(url, {
      method: 'GET',
      signal: AbortSignal.timeout(5000), // 5 second timeout
    });

    return {
      entry,
      isValid: response.ok,
      status: response.status,
    };
  } catch (error) {
    return {
      entry,
      isValid: false,
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  } finally {
    // Always release the semaphore permit
    semaphore.release();
  }
}

/**
 * Validate all sitemap entries with controlled concurrency
 */
async function validateSitemap(sitemap: SitemapEntry[]): Promise<{
  validEntries: SitemapEntry[];
  invalidEntries: Array<{
    entry: SitemapEntry;
    status?: number;
    error?: string;
  }>;
}> {
  console.log(`🔍 Validating ${sitemap.length} entries (max ${MAX_CONCURRENT_REQUESTS} concurrent)...`);

  // Create semaphore to control concurrency
  const semaphore = new Sema(MAX_CONCURRENT_REQUESTS);

  // Run all validations with controlled concurrency
  const results = await Promise.all(
    sitemap.map(entry => validateEntryWithSemaphore(entry, semaphore))
  );

  const validEntries: SitemapEntry[] = [];
  const invalidEntries: Array<{
    entry: SitemapEntry;
    status?: number;
    error?: string;
  }> = [];

  for (const result of results) {
    if (result.isValid) {
      validEntries.push(result.entry);
    } else {
      invalidEntries.push({
        entry: result.entry,
        status: result.status,
        error: result.error,
      });
    }
  }

  return { validEntries, invalidEntries };
}

async function main() {
  try {
    // Get docs.yml path from command line argument or use default
    const docsYmlPath = process.argv[2] || DEFAULT_DOCS_PATH;

    console.log(`📖 Generating sitemap from: ${docsYmlPath}`);
    console.log('🔄 Processing documentation structure...');

    // Generate the sitemap
    const sitemap = await generateSitemap(docsYmlPath);

    console.log(`✅ Generated sitemap with ${sitemap.length} entries`);

    // Validate all entries
    const { validEntries, invalidEntries } = await validateSitemap(sitemap);

    // Report results
    console.log(`\n📊 Validation Results:`);
    console.log(`   ✅ Valid entries: ${validEntries.length}`);
    console.log(`   ❌ Invalid entries: ${invalidEntries.length}`);

    if (invalidEntries.length > 0) {
      console.log(`\n❌ Invalid Entries:`);
      for (const { entry, status, error } of invalidEntries) {
        const reason = error || `HTTP ${status}`;
        console.log(`   • ${entry.title} (${entry.slug}) - ${reason}`);
      }
    }

    // Write validated sitemap to sitemap.json
    const outputPath = './sitemap.json';
    writeFileSync(outputPath, JSON.stringify(validEntries, null, 2));

    console.log(`\n📝 Valid sitemap written to: ${outputPath}`);
    
    // Summary
    const internalCount = validEntries.filter(entry => entry.type === 'internal').length;
    const externalCount = validEntries.filter(entry => entry.type === 'external').length;
    
    console.log(`📊 Final Summary:`);
    console.log(`   • Internal docs: ${internalCount}`);
    console.log(`   • External links: ${externalCount}`);
    console.log(`   • Total valid entries: ${validEntries.length}`);

    // Exit with error if there were invalid entries
    if (invalidEntries.length > 0) {
      console.log(`\n⚠️  ${invalidEntries.length} entries failed validation`);
      process.exit(1);
    }

  } catch (error) {
    console.error('❌ Error generating sitemap:', error);
    process.exit(1);
  }
}

// Only run if this script is executed directly
if (require.main === module) {
  main();
}
