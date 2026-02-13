import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectDir = path.dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  experimental: {
    typedRoutes: true
  },
  transpilePackages: ['pkg-playground', 'pkg-proto'],
  webpack: (config) => {
    config.resolve = config.resolve || {};
    config.resolve.alias = {
      ...config.resolve.alias,
      'pkg-playground': path.resolve(projectDir, '../pkg-playground/src'),
      'pkg-proto': path.resolve(projectDir, '../pkg-proto/src')
    };

    // Enable WASM support for bridge_wasm
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
    };

    return config;
  }
};

export default nextConfig;
