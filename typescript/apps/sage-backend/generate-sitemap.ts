#!/usr/bin/env tsx

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import * as cheerio from 'cheerio';
import matter from 'gray-matter';
import { parse as parseYaml } from 'yaml';
import { z } from 'zod';

// Zod schema for the docs.yml navigation structure
const TabSchema = z.object({
  'display-name': z.string().optional(),
  icon: z.string().optional(),
  slug: z.string().optional(),
  href: z.string().optional(),
});

const PageSchema = z.object({
  page: z.string(),
  path: z.string(),
  icon: z.string().optional(),
  slug: z.string().optional(),
});

const SectionSchema: z.ZodType<any> = z.lazy(() =>
  z.object({
    section: z.string(),
    icon: z.string().optional(),
    slug: z.string().optional(),
    contents: z.array(z.union([PageSchema, SectionSchema])),
  }),
);

const NavigationItemSchema = z.object({
  tab: z.string(),
  layout: z.array(z.union([PageSchema, SectionSchema])).optional(),
});

const DocsConfigSchema = z.object({
  title: z.string(),
  tabs: z.record(z.string(), TabSchema),
  navigation: z.array(NavigationItemSchema),
});

// Interface for sitemap entry
interface SitemapEntry {
  title: string;
  path?: string; // Optional for blog/external entries
  url?: string; // Used for blog/external entries
  type: 'internal' | 'external'; // New field for entry type
  // MDX frontmatter metadata
  slug?: string;
  description?: string;
  layout?: string;
  'hide-toc'?: boolean;
  [key: string]: any; // Allow other frontmatter fields
}

// Interface for blog entry
interface BlogEntry {
  url: string;
  title: string;
}

// Interface for other websites
interface OtherWebsite {
  page: string;
  url: string;
}

// Other websites to include in sitemap
const OTHER_WEBSITES: OtherWebsite[] = [
  {
    page: 'Prompt Fiddle, the BAML playground',
    url: 'https://promptfiddle.com',
  },
];

// Function to fetch blog entries from boundaryml.com/blog
async function fetchBlogEntryList(): Promise<BlogEntry[]> {
  try {
    const response = await fetch('https://boundaryml.com/blog');
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const html = await response.text();
    const $ = cheerio.load(html);
    const blogEntries: BlogEntry[] = [];

    // Find blog post cards by looking for the title h3 elements
    $('h3.tracking-tight.text-xl.font-normal').each((_, element) => {
      const $title = $(element);
      const title = $title.text().trim();

      if (!title) return;

      // Find the link - look for the closest ancestor with a href or a "Read more" link
      let $linkContainer = $title.closest('a[href*="/blog/"]');
      if (!$linkContainer.length) {
        // If title isn't in a link, look for a "Read more" link in the same card
        $linkContainer = $title
          .closest('div')
          .find('a[href*="/blog/"]')
          .first();
      }

      let url = $linkContainer.attr('href');
      if (!url) return;

      // Normalize the url
      if (url.startsWith('https://boundaryml.com')) {
        url = url.replace('https://boundaryml.com', '');
      }
      if (!url.startsWith('/blog/')) {
        return;
      }
      url = `https://boundaryml.com${url}`;

      blogEntries.push({
        url,
        title,
      });
    });

    return blogEntries;
  } catch (error) {
    console.error(`Error fetching blog links: ${error}`);
    throw error;
  }
}

// Function to extract frontmatter from MDX files
function extractMdxMetadata(filePath: string): Record<string, any> {
  try {
    const content = readFileSync(filePath, 'utf-8');
    const { data } = matter(content);
    return data;
  } catch (error) {
    console.warn(`Failed to read MDX file: ${filePath}`, error);
    return {};
  }
}

// Helper function to slugify text (matches Python implementation, preserves underscores)
function slugify(text: string): string {
  // Replace non-alphanumeric characters with spaces (preserving underscores as alnum)
  const normalized = text
    .split('')
    .map((c) => (/[a-zA-Z0-9_]/.test(c) ? c : ' '))
    .join('');

  // Pattern to match: consecutive caps, title case words, single caps, numbers (including underscores in words)
  const pattern =
    /[A-Z]{2,}(?=[A-Z][a-z_]+|[0-9]|\s|$)|[A-Z]?[a-z_]+|[A-Z]|[0-9_]+/g;
  const words = normalized.match(pattern) || [];

  return words.join('-').toLowerCase();
}

// Function to generate slug from tab/section/title
function generateSlug(
  tabSlug: string,
  sectionPath: string | undefined,
  title: string,
): string {
  const parts = [tabSlug];

  if (sectionPath) {
    // Extract only the section parts (remove tab display name)
    const sectionOnly = sectionPath.split(' > ').slice(1); // Remove first part which is tab display name

    // Convert each section to slug format
    for (const section of sectionOnly) {
      const sectionSlug = slugify(section);

      if (sectionSlug) {
        parts.push(sectionSlug);
      }
    }
  }

  // Convert title to slug format using slugify helper
  const titleSlug = slugify(title);

  if (titleSlug) {
    parts.push(titleSlug);
  }

  return `/${parts.join('/')}`;
}

// Recursive function to process navigation items
function processNavigationItem(
  item: z.infer<typeof PageSchema> | z.infer<typeof SectionSchema>,
  tabDisplayName: string,
  tabSlug: string,
  sectionDisplayPath: string | undefined,
  sectionSlugPath: string | undefined,
  docsRoot: string,
): SitemapEntry[] {
  const entries: SitemapEntry[] = [];

  if ('page' in item) {
    // Handle page item
    const mdxPath = join(docsRoot, item.path);
    const metadata = extractMdxMetadata(mdxPath);

    const fullSectionPath = sectionDisplayPath
      ? `${tabDisplayName} > ${sectionDisplayPath}`
      : undefined;
    const slugSectionPath = sectionSlugPath
      ? `${tabSlug} > ${sectionSlugPath}`
      : undefined;

    // Determine slug: docs.yml slug > frontmatter slug > generated slug
    let finalSlug = item.slug || metadata.slug;
    if (!finalSlug) {
      finalSlug = generateSlug(tabSlug, slugSectionPath, item.page);
    } else if (!finalSlug.startsWith('/')) {
      // If slug is relative, prefix it with tab and section
      finalSlug = generateSlug(tabSlug, slugSectionPath, finalSlug);
    }

    entries.push({
      title: item.page,
      path: item.path,
      section: fullSectionPath,
      ...metadata, // Spread all frontmatter metadata
      slug: finalSlug,
      type: 'internal',
    });
  } else if ('section' in item) {
    // Handle section item - recursively process contents
    const sectionDisplayName = sectionDisplayPath
      ? `${sectionDisplayPath} > ${item.section}`
      : item.section;
    const sectionSlugName = sectionSlugPath
      ? `${sectionSlugPath} > ${item.slug || slugify(item.section)}`
      : item.slug || slugify(item.section);

    for (const contentItem of item.contents) {
      entries.push(
        ...processNavigationItem(
          contentItem,
          tabDisplayName,
          tabSlug,
          sectionDisplayName,
          sectionSlugName,
          docsRoot,
        ),
      );
    }
  }

  return entries;
}

// Main function to generate sitemap
async function generateSitemap(docsYmlPath: string): Promise<SitemapEntry[]> {
  const docsRoot = dirname(docsYmlPath);

  // Read and parse docs.yml
  const docsContent = readFileSync(docsYmlPath, 'utf-8');
  const docsConfig = parseYaml(docsContent);

  // Validate with Zod schema
  const validatedConfig = DocsConfigSchema.parse(docsConfig);

  const sitemap: SitemapEntry[] = [];

  // Process each navigation item from docs
  for (const navItem of validatedConfig.navigation) {
    const tabInfo = validatedConfig.tabs[navItem.tab];
    const tabDisplayName = tabInfo?.['display-name'] || navItem.tab;
    const tabSlug = tabInfo?.slug || navItem.tab;

    // Skip tabs without layout (like external links)
    if (!navItem.layout) {
      continue;
    }

    for (const layoutItem of navItem.layout) {
      const entries = processNavigationItem(
        layoutItem,
        tabDisplayName,
        tabSlug,
        undefined,
        undefined,
        docsRoot,
      );
      // Add type: 'internal' to all doc entries
      for (const entry of entries) {
        sitemap.push({ ...entry, type: 'internal' });
      }
    }
  }

  // Fetch and add blog entries
  try {
    const blogEntries = await fetchBlogEntryList();
    for (const blogEntry of blogEntries) {
      sitemap.push({
        title: blogEntry.title,
        url: blogEntry.url,
        type: 'external',
      });
    }
  } catch (error) {
    console.warn('Failed to fetch blog entries:', error);
  }

  // Add other websites
  for (const website of OTHER_WEBSITES) {
    sitemap.push({
      title: website.page,
      url: website.url,
      type: 'external',
    });
  }

  return sitemap;
}

// CLI usage
if (require.main === module) {
  const docsYmlPath =
    process.argv[2] ||
    '/Users/egor/Documents/boundary/baml-fresh/baml/fern/docs.yml';

  async function main() {
    try {
      const sitemap = await generateSitemap(docsYmlPath);
      console.log(JSON.stringify(sitemap, null, 2));
    } catch (error) {
      console.error('Error generating sitemap:', error);
      process.exit(1);
    }
  }

  main();
}

export {
  generateSitemap,
  DocsConfigSchema,
  type SitemapEntry,
  type BlogEntry,
  type OtherWebsite,
};
