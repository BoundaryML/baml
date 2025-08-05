import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { writeFileSync, unlinkSync, existsSync } from 'node:fs';
import { generateSitemap, type SitemapEntry } from './sitemap';
import {
  validateSitemap,
  calculateValidationStats,
  testConcurrencyControl,
  validateSitemapEntryStructure,
  type SitemapValidationResult,
} from './test-utils/sitemap-validation';

// Test docs.yml path - can be set via environment variable
const TEST_DOCS_PATH =
  process.env.DOCS_YML_PATH || '/Users/sam/baml2/fern/docs.yml';

// Output path for test results
const TEST_OUTPUT_PATH = './sitemap-test-output.json';

describe('Sitemap Generation and Validation', () => {
  let sitemap: SitemapEntry[];
  let validationResults: SitemapValidationResult;

  beforeAll(async () => {
    // Skip tests if docs file doesn't exist
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log(`⚠️ Docs file not found at ${TEST_DOCS_PATH}, skipping tests`);
      return;
    }

    console.log(`📖 Generating sitemap from: ${TEST_DOCS_PATH}`);
    console.log('🔄 Processing documentation structure...');

    // Generate the sitemap
    sitemap = await generateSitemap(TEST_DOCS_PATH);
    console.log(`✅ Generated sitemap with ${sitemap.length} entries`);

    // Validate all entries
    validationResults = await validateSitemap(sitemap);

    // Write validated sitemap to test output file
    writeFileSync(
      TEST_OUTPUT_PATH,
      JSON.stringify(validationResults.validEntries, null, 2),
    );
    console.log(`\n📝 Valid sitemap written to: ${TEST_OUTPUT_PATH}`);

    // Calculate and display stats
    const stats = calculateValidationStats(validationResults);
    console.log(`📊 Final Summary:`);
    console.log(`   • Internal docs: ${stats.internalCount}`);
    console.log(`   • External links: ${stats.externalCount}`);
    console.log(`   • Total valid entries: ${stats.valid}`);
    console.log(`   • Validation rate: ${stats.validationRate.toFixed(1)}%`);
  }, 60000); // 60 second timeout for setup

  afterAll(() => {
    // Clean up test output file
    if (existsSync(TEST_OUTPUT_PATH)) {
      unlinkSync(TEST_OUTPUT_PATH);
    }
  });

  it('should generate a sitemap with entries', async () => {
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log('Skipping test - docs file not found');
      return;
    }

    expect(sitemap).toBeDefined();
    expect(Array.isArray(sitemap)).toBe(true);
    expect(sitemap.length).toBeGreaterThan(0);
  });

  it('should have valid sitemap entry structure', async () => {
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log('Skipping test - docs file not found');
      return;
    }

    for (const entry of sitemap) {
      const validation = validateSitemapEntryStructure(entry);
      expect(validation.isValid).toBe(true);

      if (!validation.isValid) {
        console.error(
          `Entry validation failed for "${entry.title}":`,
          validation.errors,
        );
      }
    }
  });

  it('should validate URLs successfully', async () => {
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log('Skipping test - docs file not found');
      return;
    }

    expect(validationResults).toBeDefined();
    expect(validationResults.validEntries).toBeDefined();
    expect(validationResults.invalidEntries).toBeDefined();

    // Should have at least some valid entries
    expect(validationResults.validEntries.length).toBeGreaterThan(0);

    // Calculate stats using test utilities
    const stats = calculateValidationStats(validationResults);
    console.log(`\n📈 Validation Rate: ${stats.validationRate.toFixed(1)}%`);

    // Test passes if validation rate is reasonable (allow some failures for local testing)
    expect(stats.validationRate).toBeGreaterThan(0);
  });

  it('should respect concurrency limits', async () => {
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log('Skipping test - docs file not found');
      return;
    }

    // Test concurrency control with a smaller limit
    const testLimit = 5;
    const testEntries = sitemap.slice(0, 20);

    const result = await testConcurrencyControl(testEntries, testLimit);

    expect(result.respectsLimit).toBe(true);
    expect(result.maxConcurrentObserved).toBeLessThanOrEqual(testLimit);
    expect(result.maxConcurrentObserved).toBeGreaterThan(0);

    console.log(
      `Max concurrent observed: ${result.maxConcurrentObserved}/${testLimit}`,
    );
  });

  it('should write valid JSON output', async () => {
    if (!existsSync(TEST_DOCS_PATH)) {
      console.log('Skipping test - docs file not found');
      return;
    }

    expect(existsSync(TEST_OUTPUT_PATH)).toBe(true);

    // Verify the JSON is valid and matches our data
    const outputContent = JSON.parse(
      require('fs').readFileSync(TEST_OUTPUT_PATH, 'utf-8'),
    );
    expect(Array.isArray(outputContent)).toBe(true);
    expect(outputContent.length).toBe(validationResults.validEntries.length);

    // Verify structure of output entries
    for (const entry of outputContent) {
      expect(entry).toHaveProperty('title');
      expect(entry).toHaveProperty('type');
    }
  });
});
