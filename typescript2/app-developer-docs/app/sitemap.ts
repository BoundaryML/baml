import type { MetadataRoute } from 'next';

import { documentationPages } from '@/lib/navigation';
import { siteConfig } from '@/lib/site-config';

export const dynamic = 'force-static';

export default function sitemap(): MetadataRoute.Sitemap {
  const paths = documentationPages.map((page) =>
    page.href === '/' ? '' : page.href,
  );

  return paths.map((path, index) => ({
    changeFrequency: 'weekly',
    priority: index === 0 ? 1 : 0.8,
    url: `${siteConfig.url}${path}`,
  }));
}
