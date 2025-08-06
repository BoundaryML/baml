import { describe, expect, it } from 'vitest';
import { fetchBlogContent, fetchBlogEntryList } from './external-sitemap';

describe('external-sitemap', () => {
  it('should call fetchBlogEntryList', async () => {
    const list = await fetchBlogEntryList();
    expect(list.length).toBeGreaterThan(10);
  });

  it('should call fetchBlogContent', async () => {
    const content = await fetchBlogContent(
      'https://www.boundaryml.com/blog/schema-aligned-parsing',
    );
    expect(content).toContain('The most common way to extract structured data');
  });
});
