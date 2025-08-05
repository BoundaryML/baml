#!/usr/bin/env tsx

import { readFileSync } from 'fs';

interface SitemapEntry {
  title: string;
  path: string;
  slug: string;
  section?: string;
  [key: string]: any;
}

function getStatusEmoji(statusCode: number): string {
  if (statusCode === 200) return '✅';
  if (statusCode === 404) return '❌';
  if (statusCode >= 300 && statusCode < 400) return '🔄';
  if (statusCode >= 400 && statusCode < 500) return '⚠️';
  if (statusCode >= 500) return '💥';
  return '🔌';
}

async function testSlug(
  baseUrl: string,
  slug: string,
): Promise<{ slug: string; status: number; emoji: string }> {
  const url = `${baseUrl}${slug}`;

  try {
    const response = await fetch(url, {
      method: 'HEAD', // Use HEAD to avoid downloading content
      signal: AbortSignal.timeout(5000), // 5 second timeout
    });

    const status = response.status;
    const emoji = getStatusEmoji(status);

    return { slug, status, emoji };
  } catch (error) {
    return { slug, status: 0, emoji: '🔌' };
  }
}

async function testAllSlugs(baseUrl: string, sitemapPath: string) {
  console.log(`Testing slugs against ${baseUrl}...\n`);

  // Read sitemap
  const sitemapContent = readFileSync(sitemapPath, 'utf-8');
  const sitemap: SitemapEntry[] = JSON.parse(sitemapContent);

  // Extract unique slugs
  const slugs = [
    ...new Set(sitemap.map((entry) => entry.slug).filter(Boolean)),
  ];

  console.log(`Found ${slugs.length} unique slugs to test\n`);

  let successCount = 0;
  let failureCount = 0;

  // Test each slug
  for (let i = 0; i < slugs.length; i++) {
    const slug = slugs[i];
    process.stdout.write(`[${i + 1}/${slugs.length}] Testing ${slug}... `);

    const result = await testSlug(baseUrl, slug);

    if (result.status === 200) {
      successCount++;
    } else {
      failureCount++;
    }

    const statusText =
      result.status === 0 ? 'Connection failed' : result.status.toString();
    console.log(`${result.emoji} ${statusText}`);

    // Add a small delay to avoid overwhelming the server
    await new Promise((resolve) => setTimeout(resolve, 50));
  }

  console.log(`\n📊 Summary:`);
  console.log(`✅ Success: ${successCount}`);
  console.log(`❌ Failures: ${failureCount}`);
  console.log(
    `📈 Success rate: ${((successCount / slugs.length) * 100).toFixed(1)}%`,
  );
}

// Handle Ctrl+C gracefully
process.on('SIGINT', () => {
  console.log('\n\n🛑 Testing interrupted by user');
  process.exit(0);
});

// CLI usage
if (require.main === module) {
  const baseUrl = process.argv[2] || 'https://docs.boundaryml.com';
  const sitemapPath = process.argv[3] || 'sitemap.json';

  testAllSlugs(baseUrl, sitemapPath).catch((error) => {
    console.error('❌ Error testing slugs:', error);
    process.exit(1);
  });
}

export { testAllSlugs };
