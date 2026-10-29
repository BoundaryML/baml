import type { MetadataRoute } from 'next';
import { SITE_URL } from '@/lib/site';

export default function robots(): MetadataRoute.Robots {
  return {
    rules: { allow: '/', userAgent: '*' },
    sitemap: `${SITE_URL}/sitemap.xml`,
  };
}
