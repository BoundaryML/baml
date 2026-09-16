import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createMDX } from 'fumadocs-mdx/next';
import type { NextConfig } from 'next';
import { PHASE_DEVELOPMENT_SERVER } from 'next/constants.js';

const applicationDirectory = path.dirname(fileURLToPath(import.meta.url));
const withMDX = createMDX();

export default function createNextConfig(phase: string): NextConfig {
  return withMDX({
    // A production build must not replace chunks used by a running dev server.
    distDir: phase === PHASE_DEVELOPMENT_SERVER ? '.next-dev' : '.next',
    experimental: {
      // Production served new book markup with a cached pre-book stylesheet.
      // Recompute build outputs until persistent-cache invalidation is verified.
      turbopackFileSystemCacheForBuild: false,
    },
    images: {
      unoptimized: true,
    },
    outputFileTracingRoot: path.join(applicationDirectory, '..'),
    poweredByHeader: false,
    reactStrictMode: true,
    async redirects() {
      return [
        {
          destination: 'https://boundaryml.com/blog?tags=release',
          permanent: true,
          source: '/changelog',
        },
      ];
    },
    trailingSlash: false,
    transpilePackages: ['@b/pkg-grammar'],
  });
}
