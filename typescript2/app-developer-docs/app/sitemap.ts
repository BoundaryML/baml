import type { MetadataRoute } from 'next';

import { listGeneratedSitemapRoutes } from '@/lib/generated-content/discovery';
import { documentationPages } from '@/lib/navigation';
import { siteConfig } from '@/lib/site-config';

export const dynamic = 'force-static';

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const paths = documentationPages.map((page) =>
    page.href === '/' ? '' : page.href,
  );
  const generatedRoutes = await listGeneratedSitemapRoutes();

  return [
    ...paths.map((path, index) => ({
      changeFrequency: 'weekly' as const,
      priority: index === 0 ? 1 : 0.8,
      url: `${siteConfig.url}${path}`,
    })),
    ...generatedRoutes.map((route) => ({
      changeFrequency: 'never' as const,
      lastModified: route.lastModified,
      priority: 0.6,
      url: `${siteConfig.url}${route.path}`,
    })),
  ];
}
