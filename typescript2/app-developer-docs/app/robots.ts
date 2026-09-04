import type { MetadataRoute } from 'next';

import { shouldIndexDeployment } from '@/lib/deployment';
import { siteConfig } from '@/lib/site-config';

export const dynamic = 'force-static';

export default function robots(): MetadataRoute.Robots {
  if (!shouldIndexDeployment()) {
    return {
      rules: {
        disallow: '/',
        userAgent: '*',
      },
    };
  }

  return {
    host: siteConfig.url,
    rules: {
      allow: '/',
      userAgent: '*',
    },
    sitemap: `${siteConfig.url}/sitemap.xml`,
  };
}
