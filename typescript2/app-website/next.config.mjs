/** biome-ignore-all assist/source/useSortedKeys: <explanation> */

import path, { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { withBaml } from '@boundaryml/baml-nextjs-plugin';
import { withPostHogConfig } from '@posthog/nextjs-config';

/**
 * Define __dirname for ES modules
 */
const __dirname = dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  eslint: { ignoreDuringBuilds: true },
  reactStrictMode: true,
  typescript: { ignoreBuildErrors: true },
  poweredByHeader: false,
  async redirects() {
    return [
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
      {
        source: '/jobs',
        destination: 'https://github.com/BoundaryML/baml/tree/canary/jobs',
        permanent: false,
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

    enablePrerenderSourceMaps: true,
    // Enable support for `global-not-found`, which allows you to more easily define a global 404 page.
    globalNotFound: true,
    scrollRestoration: true,
    // turbopackPersistentCaching: true,
    // useCache: true,
  },
  transpilePackages: ['unist-util-visit', 'mdast'],
  serverExternalPackages: ['shiki', '@boundaryml/baml'],

  webpack: (config, { dev, isServer, webpack, nextRuntime }) => {
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
      config.optimization.minimizer = config.optimization.minimizer.filter((minimizer) => {
        const ctorName = minimizer.constructor?.name ?? '';
        const snippet = String(minimizer);
        return (
          !ctorName.includes('CssMinimizer') &&
          !snippet.includes('css-minimizer-plugin')
        );
      });
    }

    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
      syncWebAssembly: true,
      topLevelAwait: true,
      layers: true,
    };

    return config;
  },
};

const configWithPlugins = withBaml()(nextConfig);

// NOTE: This does not work, i'm not sure why:
// Error 2025-08-13T17:56:39.953052Z ERROR posthog_cli: msg="Oops! While creating release\n\nCaused by:\n    Failed to create release: "
// Error running PostHog sourcemap plugin: Command failed with code 1
// https://vercel.com/baml/site/KoXbpd8GYmmEoyfRKPzCoPP4zTfP
// const configWithPosthog = withPostHogConfig(configWithPlugins, {
//   envId: process.env.POSTHOG_ENV_ID, // Environment ID
//   personalApiKey: process.env.POSTHOG_PERSONAL_API_KEY, // Personal API Key
// });

export default configWithPlugins;
