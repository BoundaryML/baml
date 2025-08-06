import { describe, it } from 'vitest';
import { fetchBlogEntryList, fetchBlogContent } from './external-sitemap';

describe('external-sitemap', () => {
  it('should call fetchBlogEntryList', async () => {
    await fetchBlogEntryList();
  });

  it('should call fetchBlogContent', async () => {
    await fetchBlogContent('https://boundaryml.com/blog/example');
  });
});