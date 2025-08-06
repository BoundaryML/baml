import { Sema } from 'async-sema';
import type { SitemapEntry } from '../sitemap';

// Base URL for validation
export const BASE_URL = 'http://localhost:3096';

// Maximum number of concurrent fetch operations
export const MAX_CONCURRENT_REQUESTS = 30;

/**
 * Result of validating a single sitemap entry
 */
export interface ValidationResult {
  entry: SitemapEntry;
  isValid: boolean;
  status?: number;
  error?: string;
}

/**
 * Result of validating a collection of sitemap entries
 */
export interface SitemapValidationResult {
  validEntries: SitemapEntry[];
  invalidEntries: Array<{
    entry: SitemapEntry;
    status?: number;
    error?: string;
  }>;
}

/**
 * Validate a single sitemap entry by fetching its URL with concurrency control
 */
export async function validateEntryWithSemaphore(
  entry: SitemapEntry,
  semaphore: Sema,
): Promise<ValidationResult> {
  // Skip external entries (they don't use local slugs)
  if (entry.type === 'external') {
    return { entry, isValid: true };
  }

  // Acquire semaphore permit before making request
  await semaphore.acquire();

  try {
    const url = `${BASE_URL}/${entry.href}`;
    console.info('validating', url);
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
export async function validateSitemap(
  sitemap: SitemapEntry[],
  maxConcurrency: number = MAX_CONCURRENT_REQUESTS,
  verbose = true,
): Promise<SitemapValidationResult> {
  if (verbose) {
    console.log(
      `🔍 Validating ${sitemap.length} entries (max ${maxConcurrency} concurrent)...`,
    );
  }

  // Create semaphore to control concurrency
  const semaphore = new Sema(maxConcurrency);

  // Run all validations with controlled concurrency
  const results = await Promise.all(
    sitemap.map((entry) => validateEntryWithSemaphore(entry, semaphore)),
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

  if (verbose) {
    console.log(`📊 Validation Results:`);
    console.log(`   ✅ Valid entries: ${validEntries.length}`);
    console.log(`   ❌ Invalid entries: ${invalidEntries.length}`);

    if (invalidEntries.length > 0) {
      console.log(`❌ Invalid Entries:`);
      for (const { entry, status, error } of invalidEntries) {
        const reason = error || `HTTP ${status}`;
        console.log(`   • ${entry.displayTitle} (${entry.slug}) - ${reason}`);
      }
    }
  }

  return { validEntries, invalidEntries };
}

/**
 * Calculate validation statistics from validation results
 */
export function calculateValidationStats(results: SitemapValidationResult): {
  total: number;
  valid: number;
  invalid: number;
  validationRate: number;
  internalCount: number;
  externalCount: number;
} {
  const total = results.validEntries.length + results.invalidEntries.length;
  const valid = results.validEntries.length;
  const invalid = results.invalidEntries.length;
  const validationRate = total > 0 ? (valid / total) * 100 : 0;
  const internalCount = results.validEntries.filter(
    (entry) => entry.type === 'internal',
  ).length;
  const externalCount = results.validEntries.filter(
    (entry) => entry.type === 'external',
  ).length;

  return {
    total,
    valid,
    invalid,
    validationRate,
    internalCount,
    externalCount,
  };
}

/**
 * Test that concurrency is respected by monitoring active requests
 */
export async function testConcurrencyControl(
  entries: SitemapEntry[],
  maxConcurrency: number,
): Promise<{
  maxConcurrentObserved: number;
  respectsLimit: boolean;
}> {
  const semaphore = new Sema(maxConcurrency);
  let activeRequests = 0;
  let maxConcurrentObserved = 0;

  const testEntries = entries.filter(
    (entry) => entry.type === 'internal' && entry.slug,
  );

  const promises = testEntries.map(async (entry) => {
    await semaphore.acquire();
    activeRequests++;
    maxConcurrentObserved = Math.max(maxConcurrentObserved, activeRequests);

    try {
      // Simulate async work
      await new Promise((resolve) => setTimeout(resolve, 100));
      return { entry, success: true };
    } finally {
      activeRequests--;
      semaphore.release();
    }
  });

  await Promise.all(promises);

  return {
    maxConcurrentObserved,
    respectsLimit: maxConcurrentObserved <= maxConcurrency,
  };
}

/**
 * Validate sitemap entry structure
 */
export function validateSitemapEntryStructure(entry: SitemapEntry): {
  isValid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!entry.displayTitle || typeof entry.displayTitle !== 'string') {
    errors.push('Missing or invalid title');
  }

  if (!entry.type || !['internal', 'external'].includes(entry.type)) {
    errors.push('Missing or invalid type (must be "internal" or "external")');
  }

  if (entry.type === 'internal') {
    if (!entry.slug || typeof entry.slug !== 'string') {
      errors.push('Internal entries must have a slug');
    } else if (!entry.slug.startsWith('/')) {
      errors.push('Internal entry slugs must start with "/"');
    }

    if (!entry.path || typeof entry.path !== 'string') {
      errors.push('Internal entries must have a path');
    }
  } else if (entry.type === 'external') {
    if (!entry.url || typeof entry.url !== 'string') {
      errors.push('External entries must have a url');
    }
  }

  return {
    isValid: errors.length === 0,
    errors,
  };
}
