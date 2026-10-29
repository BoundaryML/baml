import type { MetadataRoute } from 'next';
import { SITE_URL } from '@/lib/site';

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { changeFrequency: 'weekly', priority: 1, url: SITE_URL },
    { changeFrequency: 'weekly', priority: 0.8, url: `${SITE_URL}/quickstart` },
    { changeFrequency: 'monthly', priority: 0.6, url: `${SITE_URL}/pricing` },
  ];
}
