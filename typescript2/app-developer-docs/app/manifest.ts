import type { MetadataRoute } from 'next';

import { siteConfig } from '@/lib/site-config';

export const dynamic = 'force-static';

export default function manifest(): MetadataRoute.Manifest {
  return {
    background_color: '#ffffff',
    description: siteConfig.description,
    display: 'standalone',
    icons: [
      {
        sizes: 'any',
        src: '/icon.svg',
        type: 'image/svg+xml',
      },
      {
        sizes: '180x180',
        src: '/apple-icon',
        type: 'image/png',
      },
    ],
    name: siteConfig.name,
    short_name: 'BAML Developer',
    start_url: '/',
    theme_color: '#0a0a0a',
  };
}
