import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import matter from 'gray-matter';
import urljoin from 'url-join';
import { parse as parseYaml } from 'yaml';
import { z } from 'zod';
import {
  type BlogSitemapEntry,
  type ExternalSitemapEntry,
  OTHER_WEBSITES,
  fetchBlogEntryList,
} from './external-sitemap';

export type FernSitemapEntry = {
  type: 'fern';
  displayTitle: string;
  displaySection: string[];
  filepath: string;
  href: string;
};

export type SitemapEntry = FernSitemapEntry | BlogSitemapEntry | ExternalSitemapEntry;

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
 * Helper function to slugify text (matches Python implementation)
 */
export function slugify(text: string): string {
  // Replace non-alphanumeric characters with spaces
  const normalized = text
    .split('')
    .map((c) => (/[a-zA-Z0-9]/.test(c) ? c : ' '))
    .join('');

  // Pattern to match: consecutive caps, title case words, single caps, numbers (including underscores in words)
  const pattern = /[A-Z]{2,}(?=[A-Z][a-z_]+|[0-9]|\s|$)|[A-Z]?[a-z_]+|[A-Z]|[0-9_]+/g;
  const words = normalized.match(pattern) || [];

  return words.join('-').toLowerCase();
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
  private docsConfig: z.infer<typeof DocsConfigSchema>;

  constructor(docsYmlPath: string) {
    this.docsRoot = dirname(docsYmlPath);

    // Read and parse docs.yml
    const docsContent = readFileSync(docsYmlPath, 'utf-8');
    const docsConfig = parseYaml(docsContent);

    // Validate with Zod schema
    this.docsConfig = DocsConfigSchema.parse(docsConfig);
  }

  /**
   * Generate the complete sitemap
   */
  async generateSitemap(generateSettings: {
    includeBlogPosts?: boolean;
  } = {}): Promise<SitemapEntry[]> {
    console.log('Generating sitemap with settings:', generateSettings);

    const sitemap: SitemapEntry[] = [];

    // Process each navigation item from docs
    for (const navItem of this.docsConfig.navigation) {
      const tabId = navItem.tab;
      const tabInfo = this.docsConfig.tabs[tabId];

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

    if (generateSettings.includeBlogPosts) {
      try {
        sitemap.push(...(await fetchBlogEntryList()));
      } catch (error) {
        console.warn('Failed to fetch blog entries:', error);
      }
    }

    sitemap.push(...OTHER_WEBSITES);

    return sitemap;
  }

  /**
   * Process navigation items recursively
   */
  private processNavigationItem(
    item: z.infer<typeof PageNodeSchema> | z.infer<typeof SectionNodeSchema>,
    tab: TabInfo,
    sections: SectionInfo[],
  ): FernSitemapEntry[] {
    const entries: FernSitemapEntry[] = [];

    if ('section' in item) {
      // Handle section item - recursively process contents
      const newSectionInfo: SectionInfo = {
        sectionDisplayName: item.section,
        sectionSlug: item.slug, // Optional - will be auto-generated if not provided
      };

      const currentSections = [...sections, newSectionInfo];

      for (const contentItem of item.contents) {
        entries.push(...this.processNavigationItem(contentItem, tab, currentSections));
      }
    }

    if ('page' in item) {
      // Handle page item
      const basePath = item.path;
      const mdxPath = join(this.docsRoot, basePath);
      const mdxFrontmatter = this.readMdxFrontmatter(mdxPath);

      const pageSlug = [
        tab.tabSlug,
        ...sections.map((s) => s.sectionSlug || slugify(s.sectionDisplayName)),
        item.slug || slugify(item.page),
      ];

      // From packages/fdr-sdk/src/navigation/versions/v1/slugjoin.ts
      const pageHref = (mdxFrontmatter.slug ?? urljoin(pageSlug))
        .replaceAll('//*', '/')
        .replace(/^\/*/, '/')
        .replace(/\/*$/, '');

      entries.push({
        type: 'fern',
        filepath: mdxPath,
        displayTitle: mdxFrontmatter.title || item.page || 'PLACEHOLDER',
        displaySection: [tab.tabDisplayName, ...sections.map((s) => s.sectionDisplayName)],
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
}
