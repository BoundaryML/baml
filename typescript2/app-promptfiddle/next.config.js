import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectDir = path.dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,

  // Transpile the playground-common package
  transpilePackages: ['@baml/playground-common'],

  // Webpack configuration for aliasing and WASM support
  webpack: (config) => {
    config.resolve = config.resolve || {};
    config.resolve.alias = {
      ...config.resolve.alias,
      // Point to source for hot reload support
      '@baml/playground-common': path.resolve(projectDir, '../pkg-playground/src'),
    };

    // Enable WASM support
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
    };

    return config;
  },
};

export default nextConfig;
