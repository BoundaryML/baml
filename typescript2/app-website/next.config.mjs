/** biome-ignore-all assist/source/useSortedKeys: config reads top-down, not alphabetically */

import path, { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { withBaml } from '@boundaryml/baml-nextjs-plugin';

/**
 * Define __dirname for ES modules
 */
const __dirname = dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  eslint: { ignoreDuringBuilds: true },
  // Trace root stays at __dirname (app-website/) — the only value that builds
  // on Vercel. NFT drops some lazily-required Next runtime files from this
  // monorepo, which 500s force-dynamic routes; rather than chase those files,
  // pages that would be force-dynamic (e.g. /changelog) are kept STATIC and
  // fetch their data client-side through an edge rewrite (see rewrites()).
  reactStrictMode: true,
  typescript: { ignoreBuildErrors: true },
  // The CLI deploy uploads from the monorepo root (baml/), one level above
  // the pnpm workspace. Without an explicit tracing root, next's file
  // tracing and Vercel's function packaging disagree about the base path
  // and the function bundles lose next's runtime files (every lambda then
  // dies with MODULE_NOT_FOUND on next/dist internals).
  outputFileTracingRoot: fileURLToPath(new URL('../../', import.meta.url)),
  // Vercel's function tracing misses next's compiled source-map module
  // (loaded at runtime when enablePrerenderSourceMaps is on), which crashed
  // every serverless function with MODULE_NOT_FOUND on next/dist/compiled/
  // source-map. Force-include it in every function bundle.
  outputFileTracingIncludes: {
    // /api/og reads brand fonts + the lamb mark from components/og via fs.
    '/api/og': ['./components/og/**'],
    // /api/markdown-page serves the checked-in alternate representations.
    '/api/markdown-page': ['./content/*.md'],
    // next's error-reporting path requires these compiled modules
    // dynamically; nft cannot see dynamic requires, so include them.
    '/**': [
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/source-map/**',
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/stacktrace-parser/**',
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/babel/code-frame.js',
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/babel/package.json',
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/ws/**',
      '../node_modules/.pnpm/next@*/node_modules/next/dist/compiled/babel-code-frame/**',
    ],
  },
  poweredByHeader: false,
  async redirects() {
    return [
      {
        source: '/what-is-baml',
        destination: '/',
        permanent: false,
      },
      {
        source: '/explore',
        destination: '/',
        permanent: false,
      },
      {
        source: '/built-with-baml',
        destination: '/',
        permanent: true,
      },
      {
        source: '/hi',
        destination: '/',
        permanent: false,
      },
      {
        source: '/learn6',
        destination: '/',
        permanent: true,
      },
      {
        source: '/playground',
        destination: 'https://promptfiddle.com/',
        permanent: false,
      },
      {
        source: '/chat',
        destination: 'https://dashboard.boundaryml.com/chat',
        permanent: true,
      },
      {
        source: '/discord',
        destination: 'https://discord.gg/yzaTpQ3tdT',
        permanent: true,
      },
      // The CLI's telemetry notice links here; the canonical disclosure lives
      // in the repo. Temporary so a real /telemetry page can replace it later.
      {
        source: '/telemetry',
        destination:
          'https://github.com/BoundaryML/baml/blob/canary/TELEMETRY.md',
        permanent: false,
      },
      {
        source: '/jobs',
        destination: 'https://github.com/BoundaryML/baml/tree/canary/jobs',
        permanent: false,
      },
      // Retired pages fold into their live successors.
      {
        source: '/thesis',
        destination: '/explore',
        permanent: true,
      },
      {
        source: '/baml',
        destination: '/',
        permanent: true,
      },
      {
        source: '/baml-intro',
        destination: '/',
        permanent: true,
      },
      {
        source: '/solutions',
        destination: '/',
        permanent: true,
      },
    ];
  },
  async rewrites() {
    return [
      {
        source: '/relay-JkOu/static/:path*',
        destination: 'https://us-assets.i.posthog.com/static/:path*',
      },
      {
        source: '/relay-JkOu/:path*',
        destination: 'https://us.i.posthog.com/:path*',
      },
      {
        source: '/relay-JkOu/flags',
        destination: 'https://us.i.posthog.com/flags',
      },
    ];
  },
  // This is required to support PostHog trailing slash API requests
  skipTrailingSlashRedirect: true,
  images: {
    remotePatterns: [
      { hostname: 'images.unsplash.com' },
      { hostname: 'gravatar.com' },
      { hostname: 'avatars.githubusercontent.com' },
      { hostname: 'cloudflare-ipfs.com' },
      { hostname: 'lh3.googleusercontent.com' },
      { hostname: 'media.licdn.com' },
      { hostname: 'img.clerk.com' },
      { hostname: 'image.tmdb.org' },
      { hostname: 'picsum.photos' },
      { hostname: 'randomuser.me' },
      { hostname: 'cdn.brandfetch.io' },
      { hostname: 'img.youtube.com' },
      {
        protocol: 'https',
        hostname: 'mintlify.s3-us-west-1.amazonaws.com',
      },
      {
        protocol: 'https',
        hostname: 'my.spline.design',
      },
      {
        protocol: 'https',
        hostname: 'img.shields.io',
      },
    ],
  },
  experimental: {
    optimizeCss: false,
    mdxRs: false,
    // Forward browser logs to the terminal for easier debugging
    browserDebugInfoInTerminal: true,

    // cacheLife: true,
    // cacheComponents: true,
    // Activate new client-side router improvements
    // clientSegmentCache: true, // will be renamed to cacheComponents in Next.js 16

    // Explore route composition and segment overrides via DevTools
    devtoolSegmentExplorer: true,
    // Enable new caching and pre-rendering behavior

    // Disabled: with this on, the deployed Next server runtime requires
    // next/dist/compiled/source-map dynamically, which Vercel's function
    // tracing does not bundle; every serverless function then crashes with
    // MODULE_NOT_FOUND (incl. /api/og). Re-enable only with a verified fix.
    enablePrerenderSourceMaps: false,
    // Enable support for `global-not-found`, which allows you to more easily define a global 404 page.
    globalNotFound: true,
    scrollRestoration: true,
    // turbopackPersistentCaching: true,
    // useCache: true,
  },
  transpilePackages: [
    'unist-util-visit',
    'mdast',
    '@b/pkg-playground',
    '@b/pkg-proto',
  ],
  serverExternalPackages: ['shiki', '@boundaryml/baml'],

  webpack: (config, { dev, webpack, nextRuntime }) => {
    config.module.rules.push({
      test: /\.node$/,
      use: [
        {
          loader: 'nextjs-node-loader',
          options: {
            outputPath: config.output.path,
          },
        },
      ],
    });

    // Updated JSONL loader configuration
    config.module.rules.push({
      test: /\.jsonl$/,
      use: [
        {
          loader: path.resolve(__dirname, './jsonl-loader.js'),
        },
      ],
    });

    // Disable CSS minification to avoid cssnano issues
    if (!dev) {
      config.optimization.minimizer = config.optimization.minimizer.filter(
        (minimizer) => {
          const ctorName = minimizer.constructor?.name ?? '';
          const snippet = String(minimizer);
          return (
            !ctorName.includes('CssMinimizer') &&
            !snippet.includes('css-minimizer-plugin')
          );
        },
      );
    }

    // These experiments are for the WASM playground in the client/server
    // bundles. Do NOT apply them to the Edge runtime — overriding Next's stock
    // Edge experiments (notably `layers`) produces a middleware bundle that
    // fails to initialize on Vercel Edge (MIDDLEWARE_INVOCATION_FAILED).
    if (nextRuntime !== 'edge') {
      config.experiments = {
        ...config.experiments,
        asyncWebAssembly: true,
        syncWebAssembly: true,
        topLevelAwait: true,
        layers: true,
      };
    }

    // `typescript` is bundled for the in-editor autocomplete (@valtown/codemirror-ts).
    // Its lib does an optional `require('source-map-support')`, which isn't a
    // dependency and uses Node APIs — stop webpack from trying to resolve it.
    config.plugins.push(
      new webpack.IgnorePlugin({ resourceRegExp: /^source-map-support$/ }),
    );

    return config;
  },
};

const configWithPlugins = withBaml()(nextConfig);

// NOTE: This does not work, i'm not sure why:
// Error 2025-08-13T17:56:39.953052Z ERROR posthog_cli: msg="Oops! While creating release\n\nCaused by:\n    Failed to create release: "
// Error running PostHog sourcemap plugin: Command failed with code 1
// https://vercel.com/baml/site/KoXbpd8GYmmEoyfRKPzCoPP4zTfP
// PostHog build wrapper is parked; re-import withPostHogConfig from
// '@posthog/nextjs-config' when re-enabling.
// const configWithPosthog = withPostHogConfig(configWithPlugins, {
//   envId: process.env.POSTHOG_ENV_ID, // Environment ID
//   personalApiKey: process.env.POSTHOG_PERSONAL_API_KEY, // Personal API Key
// });

export default configWithPlugins;
