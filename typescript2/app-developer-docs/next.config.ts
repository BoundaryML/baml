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
    // Keep production on Next's default directory so static exports still
    // land in `out/`; only the long-running dev server needs isolation.
    distDir: phase === PHASE_DEVELOPMENT_SERVER ? '.next-dev' : '.next',
    images: {
      unoptimized: true,
    },
    output: 'export',
    outputFileTracingRoot: path.join(applicationDirectory, '..'),
    poweredByHeader: false,
    reactStrictMode: true,
    trailingSlash: false,
    transpilePackages: ['@b/pkg-grammar'],
  });
}
