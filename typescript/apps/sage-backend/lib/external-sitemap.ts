import * as cheerio from 'cheerio';

export interface BlogSitemapEntry {
  type: 'blog';
  title: string;
  url: string;
}

export type ExternalSitemapEntry = {
  type: 'other';
  title: string;
  url: string;
};

// Other websites to include in sitemap
export const OTHER_WEBSITES: ExternalSitemapEntry[] = [
  {
    type: 'other',
    title: 'Prompt Fiddle, the BAML playground',
    url: 'https://promptfiddle.com',
  },
];

/**
 * Function to fetch blog entries from boundaryml.com/blog
 */
export async function fetchBlogEntryList(): Promise<BlogSitemapEntry[]> {
  try {
    const response = await fetch('https://boundaryml.com/blog');
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const html = await response.text();
    const $ = cheerio.load(html);
    const blogEntries: BlogSitemapEntry[] = [];

    // Find blog post links and extract titles from h3 elements within them
    $('a[href^="/blog/"]').each((_, element) => {
      const $link = $(element);
      const url = $link.attr('href');

      if (!url || !url.startsWith('/blog/')) return;

      // Find the title within this link's h3 element
      const $title = $link.find('h3');
      const title = $title.text().trim();

      if (!title) return;

      const fullUrl = `https://boundaryml.com${url}`;

      blogEntries.push({
        type: 'blog',
        title,
        url: fullUrl,
      });
    });

    return blogEntries;
  } catch (error) {
    console.error(`Error fetching blog links: ${error}`);
    throw error;
  }
}

function extractTextFromHtml(html: string): string {
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
    $('header, footer, nav, aside, .header, .footer, .nav, .sidebar, .menu, .navigation').remove();
    content = $('body').text().trim();
  }

  // Clean up whitespace and normalize
  content = content
    .replace(/\s+/g, ' ')
    .replace(/\n\s*\n/g, '\n')
    .trim();

  return content;
}

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

    console.log(`✓ Successfully extracted ${content.length} characters from ${url}`);
    return content;
  } catch (error) {
    console.error(`✗ Error fetching blog content from ${url}:`, error);
    // Return a minimal fallback content
    return `Blog post: ${url}\nTitle: ${url.split('/').pop()?.replace(/-/g, ' ') || 'Blog Post'}`;
  }
}
