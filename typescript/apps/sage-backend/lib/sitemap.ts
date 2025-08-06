import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import * as cheerio from 'cheerio';
import matter from 'gray-matter';
import { parse as parseYaml } from 'yaml';
import { z } from 'zod';
import urljoin from 'url-join';

// export interface SitemapEntry {
//   displayTitle: string;
//   path?: string; // Optional for blog/external entries
//   url?: string; // Used for blog/external entries
//   type: 'internal' | 'external'; // New field for entry type
//   slug: string[];
//   href?: string;
//   description?: string;
//   sectionDisplayPath?: string;
//   layout?: string;
//   'hide-toc'?: boolean;
//   [key: string]: any; // Allow other frontmatter fields
// }

export type SitemapEntry =
  | {
      type: 'internal';
      displayTitle: string;
      displaySection: string[];
      href: string;
    }
  | {
      type: 'external';
      displayTitle: string;
      url: string;
    };

// Type definitions for OOP refactor
export interface TabInfo {
  tabId: string;
  tabDisplayName: string;
  tabSlug: string;
}

export interface SectionInfo {
  sectionDisplayName: string;
  sectionSlug?: string;
}

export interface FernDoc {
  slug: string;
  path: string;
  body: string;
  title: string;
  chunkIndex?: number;
}

// Interface for blog entry
export interface BlogEntry {
  url: string;
  title: string;
}

// Interface for other websites
export interface OtherWebsite {
  page: string;
  url: string;
}

const PageFrontmatterSchema = z.object({
  title: z.string().optional(),
  slug: z.string().optional(),
  description: z.string().optional(),
});

// Zod schema for the docs.yml navigation structure
const TabNodeSchema = z.object({
  'display-name': z.string(),
  icon: z.string(),
  slug: z.string().optional(),
  href: z.string().optional(),
});

const PageNodeSchema = z.object({
  page: z.string(),
  path: z.string(),
  icon: z.string().optional(),
  slug: z.string().optional(),
});

const SectionNodeSchema = z.object({
  section: z.string(),
  icon: z.string().optional(),
  slug: z.string().optional(),
  get contents() {
    return z.array(z.union([PageNodeSchema, SectionNodeSchema]));
  },
});

const NavigationNodeSchema = z.object({
  tab: z.string(),
  layout: z.array(z.union([PageNodeSchema, SectionNodeSchema])).optional(),
});

export const DocsConfigSchema = z.object({
  title: z.string(),
  tabs: z.record(z.string(), TabNodeSchema),
  navigation: z.array(NavigationNodeSchema),
});

// Other websites to include in sitemap
const OTHER_WEBSITES: OtherWebsite[] = [
  {
    page: 'Prompt Fiddle, the BAML playground',
    url: 'https://promptfiddle.com',
  },
];

/**
 * Function to fetch blog entries from boundaryml.com/blog
 */
export async function fetchBlogEntryList(): Promise<BlogEntry[]> {
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

/**
 * Function to extract frontmatter from MDX files
 */
export function extractMdxMetadata(filePath: string): Record<string, any> {
  try {
    const content = readFileSync(filePath, 'utf-8');
    const { data } = matter(content);
    return data;
  } catch (error) {
    console.warn(`Failed to read MDX file: ${filePath}`, error);
    return {};
  }
}

/**
 * Helper function to slugify text (matches Python implementation, preserves underscores)
 */
export function slugify(text: string): string {
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

/**
 * Helper function to build url2 from slug2 components
 */
export function buildUrl2FromSlug2(slug2: string[]): string {
  return urljoin(...slug2.map((part) => part.trim()))
    .replaceAll('//*', '/')
    .replace(/^\//, '')
    .replace(/\/$/, '');
}

/**
 * Function to generate slug and slug2 from tab/section/title
 */
export function generateSlug(
  tabSlug: string,
  sectionPath: string | undefined,
  title: string,
): { slug: string; slug2: string[] } {
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

  return {
    slug: `/${parts.join('/')}`,
    slug2: parts,
  };
}

/**
 * SitemapGenerator class that encapsulates sitemap generation logic and state
 */
export class SitemapGenerator {
  private docsRoot: string;
  private config: z.infer<typeof DocsConfigSchema>;

  constructor(docsYmlPath: string) {
    this.docsRoot = dirname(docsYmlPath);

    // Read and parse docs.yml
    const docsContent = readFileSync(docsYmlPath, 'utf-8');
    const docsConfig = parseYaml(docsContent);

    // Validate with Zod schema
    this.config = DocsConfigSchema.parse(docsConfig);
  }

  /**
   * Generate the complete sitemap
   */
  async generateSitemap(): Promise<SitemapEntry[]> {
    const sitemap: SitemapEntry[] = [];

    // Process each navigation item from docs
    for (const navItem of this.config.navigation) {
      const tabId = navItem.tab;
      const tabInfo = this.config.tabs[tabId];

      // Create TabInfo object
      const tab: TabInfo = {
        tabId,
        tabDisplayName: tabInfo['display-name'],
        tabSlug: tabInfo.slug || tabId,
      };

      // Skip tabs without layout (like external links)
      if (!navItem.layout) {
        continue;
      }

      for (const layoutItem of navItem.layout) {
        const entries = this.processNavigationItem(layoutItem, tab, []);
        sitemap.push(...entries);
      }
    }

    // // Fetch and add blog entries
    // try {
    //   const blogEntries = await fetchBlogEntryList();
    //   for (const blogEntry of blogEntries) {
    //     sitemap.push({
    //       title: blogEntry.title,
    //       url: blogEntry.url,
    //       type: 'external',
    //     });
    //   }
    // } catch (error) {
    //   console.warn('Failed to fetch blog entries:', error);
    // }

    // // Add other websites
    // for (const website of OTHER_WEBSITES) {
    //   sitemap.push({
    //     title: website.page,
    //     url: website.url,
    //     type: 'external',
    //   });
    // }

    return sitemap;
  }

  /**
   * Process navigation items recursively
   */
  private processNavigationItem(
    item: z.infer<typeof PageNodeSchema> | z.infer<typeof SectionNodeSchema>,
    tab: TabInfo,
    sections: SectionInfo[],
  ): SitemapEntry[] {
    const entries: SitemapEntry[] = [];

    if ('section' in item) {
      // Handle section item - recursively process contents
      const newSectionInfo: SectionInfo = {
        sectionDisplayName: item.section,
        sectionSlug: item.slug, // Optional - will be auto-generated if not provided
      };

      const currentSections = [...sections, newSectionInfo];

      for (const contentItem of item.contents) {
        entries.push(
          ...this.processNavigationItem(contentItem, tab, currentSections),
        );
      }
    }

    if ('page' in item) {
      // Handle page item
      const mdxPath = join(this.docsRoot, item.path);
      const mdxFrontmatter = this.readMdxFrontmatter(mdxPath);

      // Build display path
      const fullSectionPath = this.buildDisplayPath(tab, sections);

      const pageSlug = this.buildSlugComponents(tab, sections, item.slug);

      // From packages/fdr-sdk/src/navigation/versions/v1/slugjoin.ts
      const pageHref = urljoin(pageSlug)
        .replaceAll('//*', '/')
        .replace(/^\//, '')
        .replace(/\/$/, '');

      entries.push({
        type: 'internal',
        displayTitle: mdxFrontmatter.title || item.page || 'PLACEHOLDER',
        displaySection: [
          tab.tabDisplayName,
          ...sections.map((s) => s.sectionDisplayName),
        ],
        href: pageHref,
      });
    }

    return entries;
  }

  /**
   * Extract frontmatter from MDX files
   */
  private readMdxFrontmatter(filePath: string) {
    try {
      const content = readFileSync(filePath, 'utf-8');
      const { data } = matter(content);
      return PageFrontmatterSchema.parse(data);
    } catch (error) {
      console.warn(`Failed to read MDX file: ${filePath}`, error);
      return {};
    }
  }

  /**
   * Build display path from tab and sections
   */
  private buildDisplayPath(
    tab: TabInfo,
    sections: SectionInfo[],
  ): string | undefined {
    if (sections.length === 0) {
      return undefined;
    }

    const parts = [tab.tabDisplayName];
    for (const section of sections) {
      parts.push(section.sectionDisplayName);
    }

    return parts.join(' > ');
  }

  /**
   * Build slug components from tab and sections
   */
  private buildSlugComponents(
    tab: TabInfo,
    sections: SectionInfo[],
    pageSlug?: string,
  ): string[] {
    const components = [tab.tabSlug];

    for (const section of sections) {
      // Use provided slug or generate from display name
      const sectionSlug =
        section.sectionSlug || slugify(section.sectionDisplayName);
      if (sectionSlug) {
        components.push(sectionSlug);
      }
    }

    if (pageSlug) {
      components.push(pageSlug);
    }

    return components;
  }

  /**
   * Resolve slug with priority: configured > frontmatter > generated
   */
  private resolveSlug(
    configuredSlug: string | undefined,
    metadata: any,
    fallbackName: string,
  ): string {
    const candidateSlug = configuredSlug || metadata.slug;
    if (candidateSlug) {
      return candidateSlug;
    }
    return slugify(fallbackName);
  }
}

/**
 * Main function to generate sitemap from docs.yml (for backward compatibility)
 */
export async function generateSitemap(
  docsYmlPath: string,
): Promise<SitemapEntry[]> {
  const generator = new SitemapGenerator(docsYmlPath);
  return generator.generateSitemap();
}

/**
 * Load and parse the sitemap.json file
 */
export function loadSitemap(): SitemapEntry[] {
  try {
    const sitemap: SitemapEntry[] = JSON.parse(
      readFileSync('./sitemap.json', 'utf8'),
    );
    return sitemap;
  } catch (error) {
    console.error('Error loading sitemap.json:', error);
    throw new Error('Failed to load sitemap.json');
  }
}

/**
 * Helper function to read internal doc content from the fern directory
 */
export function readInternalDocContent(docPath: string): string {
  try {
    // Assume docs are in the fern directory relative to the sage directory
    const fullPath = join('../fern', docPath);
    const content = readFileSync(fullPath, 'utf8');

    // Remove frontmatter if present
    const frontmatterRegex = /^---\s*\n[\s\S]*?\n---\s*\n/;
    const cleanContent = content.replace(frontmatterRegex, '').trim();

    return cleanContent;
  } catch (error) {
    console.error(`Error reading internal doc ${docPath}:`, error);
    return `Document: ${docPath}`;
  }
}

/**
 * Helper function to extract clean text content from HTML
 */
export function extractTextFromHtml(html: string): string {
  const $ = cheerio.load(html);

  // Remove unwanted elements
  $(
    'script, style, nav, header, footer, .navigation, .sidebar, .ads, .cookie-banner, .header, .footer',
  ).remove();

  // Try to find main content area
  let content = '';
  const contentSelectors = [
    'main article',
    'main',
    'article',
    '.post-content',
    '.entry-content',
    '.blog-content',
    '.content',
    '[role="main"]',
    '.post-body',
    '.article-content',
  ];

  for (const selector of contentSelectors) {
    const element = $(selector);
    if (element.length) {
      const text = element.text().trim();
      if (text.length > content.length) {
        content = text;
      }
    }
  }

  // If no main content found, try body with unwanted elements removed
  if (!content) {
    $(
      'header, footer, nav, aside, .header, .footer, .nav, .sidebar, .menu, .navigation',
    ).remove();
    content = $('body').text().trim();
  }

  // Clean up whitespace and normalize
  content = content
    .replace(/\s+/g, ' ')
    .replace(/\n\s*\n/g, '\n')
    .trim();

  return content;
}

/**
 * Helper function to fetch and clean blog content from external URLs
 */
export async function fetchBlogContent(url: string): Promise<string> {
  try {
    console.log(`Fetching blog content from: ${url}`);
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const html = await response.text();
    const content = extractTextFromHtml(html);

    if (!content || content.length < 100) {
      throw new Error('Could not extract meaningful content from blog post');
    }

    console.log(
      `✓ Successfully extracted ${content.length} characters from ${url}`,
    );
    return content;
  } catch (error) {
    console.error(`✗ Error fetching blog content from ${url}:`, error);
    // Return a minimal fallback content
    return `Blog post: ${url}\nTitle: ${url.split('/').pop()?.replace(/-/g, ' ') || 'Blog Post'}`;
  }
}

/**
 * Get all internal documents from the sitemap
 */
export function getInternalDocs(sitemap?: SitemapEntry[]): SitemapEntry[] {
  const sitemapEntries = sitemap || loadSitemap();
  return sitemapEntries.filter((entry) => entry.type === 'internal');
}

/**
 * Get all external blog posts from the sitemap
 */
export function getExternalBlogs(sitemap?: SitemapEntry[]): SitemapEntry[] {
  const sitemapEntries = sitemap || loadSitemap();
  return sitemapEntries.filter((entry) => entry.type === 'external');
}

/**
 * Process internal documents into FernDoc format
 */
export function processInternalDocs(internalDocs: SitemapEntry[]): FernDoc[] {
  const fernDocs: FernDoc[] = [];

  for (const entry of internalDocs) {
    if (!entry.path || !entry.slug) {
      console.warn(
        `Skipping internal doc without path or slug: ${entry.displayTitle}`,
      );
      continue;
    }

    try {
      const content = readInternalDocContent(entry.path);
      fernDocs.push({
        slug: entry.slug,
        path: entry.path,
        body: content,
        title: entry.displayTitle,
      });
      console.log(`✓ Processed internal doc: ${entry.displayTitle}`);
    } catch (error) {
      console.error(
        `✗ Failed to process internal doc ${entry.displayTitle}:`,
        error,
      );
    }
  }

  return fernDocs;
}

/**
 * Process external blog posts into FernDoc format
 */
export async function processExternalBlogs(
  externalBlogs: SitemapEntry[],
): Promise<FernDoc[]> {
  const fernDocs: FernDoc[] = [];

  for (const entry of externalBlogs) {
    if (!entry.url) {
      console.warn(
        `Skipping external entry without URL: ${entry.displayTitle}`,
      );
      continue;
    }

    try {
      const content = await fetchBlogContent(entry.url);
      // Use the full URL as the slug for external content
      const slug = entry.url;

      fernDocs.push({
        slug: slug,
        path: entry.url,
        body: content,
        title: entry.displayTitle,
      });
      console.log(`✓ Processed external blog: ${entry.displayTitle}`);
    } catch (error) {
      console.error(
        `✗ Failed to process external blog ${entry.displayTitle}:`,
        error,
      );
    }
  }

  return fernDocs;
}
