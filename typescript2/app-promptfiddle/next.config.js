import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const projectDir = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

// Worker entry files from @codingame/monaco-vscode-api use ES module syntax
// (import/export). Webpack copies them as raw assets via the `new URL()`
// pattern. Next.js's TerserPlugin parses all .js assets as scripts and chokes
// on import/export. Patch the TerserPlugin's optimize method to mark worker
// assets as already-minimized so they get skipped.
try {
  const terserPath = require.resolve(
    'next/dist/build/webpack/plugins/terser-webpack-plugin/src/index.js',
  );
  const { TerserPlugin } = require(terserPath);
  const origOptimize = TerserPlugin.prototype.optimize;
  TerserPlugin.prototype.optimize = async function (compiler, compilation, assets, ...rest) {
    for (const [name, info] of compilation.assetsInfo) {
      if (/worker/i.test(name) && /\.js$/i.test(name)) {
        compilation.assetsInfo.set(name, { ...info, minimized: true });
      }
    }
    return origOptimize.call(this, compiler, compilation, assets, ...rest);
  };
} catch (e) {
  console.warn('[next.config] Could not patch TerserPlugin:', e.message);
}

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  experimental: {
    typedRoutes: true
  },
  transpilePackages: ['pkg-playground', 'pkg-proto'],
  webpack: (config, { isServer }) => {
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
