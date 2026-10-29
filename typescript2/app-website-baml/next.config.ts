import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  images: {
    // 90 for the hand-painted backgrounds: the default 75 softens their brush strokes.
    qualities: [75, 90],
  },
  // PostHog's ingestion, proxied through this site (instrumentation-client.ts), as on app-website.
  async rewrites() {
    return [
      {
        destination: 'https://us-assets.i.posthog.com/static/:path*',
        source: '/relay-JkOu/static/:path*',
      },
      {
        destination: 'https://us.i.posthog.com/:path*',
        source: '/relay-JkOu/:path*',
      },
    ];
  },
  // PostHog's API requests end in a trailing slash.
  skipTrailingSlashRedirect: true,
};

export default nextConfig;
